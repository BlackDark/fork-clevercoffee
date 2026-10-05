//! The Wi-Fi log stream's portable half: the heap shed, the bounded ring that
//! carries lines from the logger to the socket, and the line format.
//!
//! # Why this is here and not in `cc-hal-esp32`
//!
//! Finding **3.2 of
//! [`32-findings-2026-10-03.md`](../../../docs/history/review-2026-10-03.md)
//! recorded that the telnet log server had a `HeapShed` and nothing consuming
//! it: `cc-hal-esp32/src/telnet.rs` said so in its own doc comment. The shed
//! policy was therefore never exercised by `just test`, and the acceptance
//! criterion for R3-14/R3-11 could not be met at all — there was no client to
//! disconnect and no line to shed.
//!
//! Three of the four pieces of a log stream need no chip: whether a given free
//! heap permits a write ([`Shed`]), what a client that stops reading costs the
//! machine ([`Ring`]), and what a line looks like on the wire ([`line()`]). They
//! live here for the same reason [`crate::payload`] lives here — `just test`
//! names this crate, so a test here runs on every commit and
//! `cc-hal-esp32` does not compile for a host target at all.
//!
//! The fourth — the socket — is `cc_hal_esp32::telnet`, and it is device-only.
//! That split is the point, not a compromise: the policy is reviewable without
//! reading a `lwip_bind`.
//!
//! # What is the C++'s, precisely
//!
//! | here | C++ |
//! | --- | --- |
//! | [`SHED_FLOOR_BYTES`] = 30 000 | `Logger.cpp:13` `MIN_HEAP_FOR_WIFI_LOG`, checked at `:29` |
//! | [`RING_ENTRIES`] = 16 | `Logger.h:161` `LOG_RING_SIZE` |
//! | [`ENTRY_BYTES`] = 256 | `Logger.h:159` `LOG_BUFFER_SIZE` after ADR-0002 decision 1 |
//! | [`MAX_CLIENTS`] = 1 | one `WiFiClient client_` member, `Logger.h:154` |
//! | drop-newest on a full ring | `Logger.cpp:248-256` — count and return |
//! | `...` on a truncated line | `Logger.cpp:221-228` |
//!
//! **The entry is 256 B here and 304 B there** (`Logger.h:160`
//! `LOG_ENTRY_SIZE = LOG_BUFFER_SIZE + 48`), because the C++ sizes the entry
//! for its own `[HH:MM:SS] [LEVEL] ` prefix while this formats the prefix
//! straight into the same 256 B. The ring is 4 096 B rather than 4 864 B for
//! the same reason. No knob: [`ENTRY_BYTES`] is the existing
//! `LINE_BUFFER_BYTES`, moved here with the rest of the policy.

use core::fmt::Write;

use heapless::String;

/// The free-heap floor below which the Wi-Fi log stream stops writing.
///
/// ADR-0002 decision 5 and `Logger.cpp:13`. `cc_hal_esp32::heap` re-exports
/// this as `HEAP_SHED_BYTES` and `cc_hal_esp32::web` re-exports *that* as
/// `HEAP_FLOOR_BYTES`, because three paths guard "the machine is tight" and
/// three constants for one judgement is how they drift.
pub const SHED_FLOOR_BYTES: u32 = 30_000;

/// How many lines the ring holds before it starts dropping the newest.
///
/// `Logger.h:161`. Reused, not chosen: the C++'s ring is 16 entries and the
/// ring size is a static-RAM number someone measured, not a tuning parameter.
pub const RING_ENTRIES: usize = 16;

/// The bytes one line may occupy in the ring.
///
/// ADR-0002 decision 1 ("down from 512") and `Logger.h:159` — "individual log
/// lines rarely exceed 200 characters". A longer line is truncated with a
/// visible `...`, never silently split across two entries.
pub const ENTRY_BYTES: usize = 256;

/// How many log clients the machine serves at once.
///
/// **One, and this is the heap bound that matters.** The C++ has exactly one
/// `WiFiClient client_` (`Logger.h:154`) and a second connection *replaces* the
/// first (`Logger.cpp:134-137`: `client_.stop()`, then `server_.available()`),
/// so the socket count does not grow with how many terminals are open. An
/// unbounded listener is how a debug channel becomes the OOM.
pub const MAX_CLIENTS: usize = 1;

/// How many lines one pass over the ring may flush.
///
/// `Logger.h:145` `MAX_WIFI_FLUSH_PER_UPDATE`. Bounded so that a backlog of
/// backlogged lines cannot keep the telnet task inside one loop iteration
/// instead of going back to `accept()`.
pub const MAX_FLUSH_PER_PASS: usize = 8;

/// The log levels `log` can carry, and the words the C++ printed for them.
///
/// `Logger.cpp:161-177` `getLevelString`. `FATAL` and `SILENT` are in the C++'s
/// table and unreachable here: the `log` crate has no such levels, and adding
/// them to a `match` on a value that cannot occur would be a claim about the
/// world rather than about the code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    /// `TRACE`.
    Trace,
    /// `DEBUG`.
    Debug,
    /// `INFO`.
    Info,
    /// `WARNING`.
    Warning,
    /// `ERROR`.
    Error,
}

impl Level {
    /// The word on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
        }
    }
}

/// One pass of the shed, including which edge it crossed.
///
/// The edge is part of the answer and not a side effect because it is what the
/// operator reads: a machine that oscillates around the floor prints one line
/// per transition or thousands, and the oscillation is exactly the condition
/// being investigated (`ADR-0002` decision 5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decision {
    /// Above the floor, and it was above it last time. Write it, say nothing.
    Allow,
    /// Above the floor, having been below it. Write it, and report the recovery.
    AllowRecovered,
    /// Below the floor for the first time. Shed it, and report the shed.
    ShedEngaged,
    /// Below the floor, and already reported. Shed it, say nothing.
    Shed,
}

/// ADR-0002 decision 5 as a pure state machine over the free-heap reading.
///
/// The heap read itself is the caller's (`esp_get_free_heap_size`), which is why
/// this takes the number rather than asking for it: a host test can then drive
/// **both** edges, which `cc-hal-esp32`'s version could not — a device test can
/// only assert the branch the real heap happened to be on.
#[derive(Clone, Debug)]
pub struct Shed {
    engaged: bool,
}

impl Shed {
    /// A shed that has not yet engaged.
    #[must_use]
    pub const fn new() -> Self {
        Self { engaged: false }
    }

    /// Decide whether one line goes to the Wi-Fi client, given `free` bytes.
    ///
    /// **This is ADR-0002 decision 5 and nothing more.** Below
    /// [`SHED_FLOOR_BYTES`] the line is not written; the connection stays open,
    /// the serial stream is untouched, and the stream resumes when the heap
    /// recovers. There is deliberately no "disconnect the client" arm: an
    /// active close appears in the operator's terminal as "Connection reset by
    /// peer", which is indistinguishable from a network fault and sends the
    /// investigation the wrong way.
    pub fn decide(&mut self, free: u32) -> Decision {
        if free >= SHED_FLOOR_BYTES {
            return if core::mem::replace(&mut self.engaged, false) {
                Decision::AllowRecovered
            } else {
                Decision::Allow
            };
        }
        if core::mem::replace(&mut self.engaged, true) {
            Decision::Shed
        } else {
            Decision::ShedEngaged
        }
    }

    /// Whether the last [`Shed::decide`] was below the floor.
    #[must_use]
    pub const fn is_shedding(&self) -> bool {
        self.engaged
    }
}

impl Default for Shed {
    fn default() -> Self {
        Self::new()
    }
}

/// The bounded hand-off between the logging path and the telnet task.
///
/// # Why a ring and not a channel
///
/// The producer is whichever task happened to log — usually the control task at
/// priority 5 — and the consumer is the telnet task at priority
/// [`TELNET_PRIO`](../../cc_hal_esp32/task/index.html). **The producer must
/// never wait for the consumer**, and a client that has stopped reading must
/// not be able to make the control tick wait either. A full ring therefore
/// *drops the newest line and counts it* (`Logger.cpp:248-256` does exactly
/// this), and [`Ring::push`] returns rather than blocking.
///
/// The alternative — holding a lock across the socket write — is the failure
/// ADR-0002 exists to prevent, and it would put a network write on the control
/// tick's critical path.
///
/// # Why this is `&mut self` and not `&self`
///
/// Because it lives in a `no_std`, `forbid(unsafe_code)` crate and is reached
/// from several tasks, the only safe way to get "another task may append" is a
/// lock the caller owns. [`cc_hal_esp32::telnet`](../../cc_hal_esp32/telnet/index.html)
/// holds the real one in a `Mutex` and states the argument at the call site; it
/// is the same rule `cc_firmware`'s `slots.rs` states for the three locks the
/// control task takes:
///
/// > do not block on a lock whose critical section is unbounded, and do not
/// > block on a lock a higher-priority task holds.
///
/// Neither half is violated here. The critical section is a bounded
/// [`ENTRY_BYTES`]-byte copy with no allocation and no syscall, so it is
/// microseconds; and the only other holder is the telnet task at priority 2, so
/// `FreeRTOS` priority inheritance boosts it to the control task's 5 for the
/// duration of a memcpy and it then releases.
///
/// An atomics-per-byte ring would also be lock-free and would also be correct.
/// It was not chosen: 256 relaxed `store`s per line on the control tick's
/// logging path is a cost with no measured benefit over a memcpy, and the
/// memcpy is the one a reader can check by reading it.
#[derive(Debug)]
pub struct Ring {
    lines: heapless::Deque<heapless::String<ENTRY_BYTES>, RING_ENTRIES>,
    dropped: u32,
    pushed: u32,
}

impl Default for Ring {
    fn default() -> Self {
        Self::new()
    }
}

impl Ring {
    /// An empty ring.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lines: heapless::Deque::new(),
            dropped: 0,
            pushed: 0,
        }
    }

    /// Offer one line. Returns `false` if it was dropped.
    ///
    /// Never blocks and never allocates: the line is truncated to
    /// [`ENTRY_BYTES`] and copied into the ring, and a full ring is a `false`
    /// and a counter. This is the property the control tick depends on.
    pub fn push(&mut self, line: &str) -> bool {
        // Truncate rather than reject: `Logger.cpp:221-228` truncates with a
        // visible marker, and rejecting would make an over-long line invisible
        // instead of visibly wrong.
        let n = core::cmp::min(line.len(), ENTRY_BYTES);
        let mut entry = heapless::String::new();
        match entry.push_str(&line[..n]) {
            Ok(()) => {}
            Err(_) => return false,
        }
        if self.lines.push_back(entry).is_err() {
            self.dropped += 1;
            return false;
        }
        self.pushed += 1;
        true
    }

    /// Take the oldest line, or `None` if the ring is empty.
    pub fn pop(&mut self) -> Option<heapless::String<ENTRY_BYTES>> {
        self.lines.pop_front()
    }

    /// Lines dropped because the ring was full.
    #[must_use]
    pub const fn dropped(&self) -> u32 {
        self.dropped
    }

    /// Lines successfully queued.
    #[must_use]
    pub const fn pushed(&self) -> u32 {
        self.pushed
    }

    /// Whether a line is waiting to be taken.
    #[must_use]
    pub fn has_line(&self) -> bool {
        !self.lines.is_empty()
    }
}

/// Format one line for the wire.
///
/// `[<uptime_ms>] [<LEVEL>] <target>: <body>\r\n` — the C++'s
/// `[HH:MM:SS] [LEVEL] <message>\r\n` (`Logger.cpp:212`) with two changes, both
/// forced by facts about this port rather than chosen:
///
/// * **`uptime_ms` instead of a wall clock.** `Logger::formatTimestamp`
///   (`Logger.cpp:180-196`) uses `time(nullptr)`, and nothing in this port
///   calls `settimeofday` — there is no SNTP client and no NVS-restore of the
///   clock, so a `HH:MM:SS` field would print a 1970 date or an error string.
///   A boot-relative stamp is the same field with an honest reference.
/// * **`<target>` is kept.** The C++ passed `file`, `function` and `line` into
///   `formatLogMessage` and printed none of them; `log` has no file or line, so
///   without the target a line from the control task and a line from the HTTP
///   task are indistinguishable in the one place an operator is reading them.
///
/// The line is truncated to [`ENTRY_BYTES`] with a trailing `...`, which is
/// `Logger.cpp:221-228`.
#[must_use]
pub fn line(level: Level, uptime_ms: u32, target: &str, body: &str) -> String<ENTRY_BYTES> {
    let mut out = String::new();
    // The prefix can fail only if the capacity is smaller than
    // "[4294967295] [WARNING] x: ", which `ENTRY_BYTES` is not, and the `?`
    // says so once here rather than at three call sites.
    let _ = write!(out, "[{uptime_ms}] [{}] {target}: ", level.as_str());

    // Bytes left for the body once the terminator is accounted for. A body
    // that does not fit gives the last three of those bytes to the marker, so
    // the result is always exactly `ENTRY_BYTES` when it is truncated and
    // shorter when it is not.
    let room = ENTRY_BYTES
        .saturating_sub(out.len())
        .saturating_sub(CRLF.len());
    if body.len() > room {
        let keep = room.saturating_sub(ELLIPSIS.len());
        let _ = out.push_str(&body[..keep]);
        out.push_str(ELLIPSIS).ok();
    } else {
        out.push_str(body).ok();
    }
    out.push_str(CRLF).ok();
    out
}

/// The truncation marker. `Logger.cpp:224`.
const ELLIPSIS: &str = "...";
/// The line terminator the C++ writes. `Logger.cpp:212`.
const CRLF: &str = "\r\n";

#[cfg(test)]
mod tests;
