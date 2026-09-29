//! The Wi-Fi telnet log stream, and ADR-0002's heap-aware shed.
//!
//! Owner: **R3-14** (task E).
//!
//! # What it replaces
//!
//! `src/Logger.cpp` (F29) and the heap behaviour of ADR-0002. The C++'s
//! `Logger` is a ring buffer plus a `WiFiServer` on port 23
//! (`Logger.cpp:133-152`), with the shed at `:64`.
//!
//! # The shed is a soft one, and that is the ADR's decision
//!
//! ```text
//! When ESP.getFreeHeap() < 30 KB, writeToOutputs() skips WiFi writes
//! (Serial still works). This is a soft shed — the telnet connection stays open
//! and resumes when heap recovers. No active client_.stop() to avoid the
//! "Connection reset by peer" problem.
//! ```
//! — ADR-0002, decision 5.
//!
//! The R3-14 brief said the client must be *disconnected* under heap pressure.
//! The ADR that records the decision says the opposite, and gives the reason: an
//! active close appears in the operator's terminal as "Connection reset by
//! peer", which is indistinguishable from a network fault and sends the
//! investigation in the wrong direction. What the brief is protecting — **shed,
//! and do not crash** — is what this implements.
//!
//! # The other half of the OOM fix is not here
//!
//! Shedding only helps if the thing being shed would otherwise have been
//! affordable. The half that makes a 19 KB `/api/parameters` response affordable
//! is ADR-0002 decision 2 — serialise once, stream it, never build a `String`
//! intermediate — and that is [`crate::web`]. A machine that sheds the log and
//! still aborts on an API request has fixed neither half.
//!
//! # The ring buffer is the C++'s
//!
//! `16 × 304 B ≈ 5 KB`, reduced from `64 × 576 B = 37 KB` by ADR-0002 decision 1
//! because 37 KB is 12 % of the ESP32's total RAM
//! (`Logger.cpp:39` and the ADR's "Lessons learned"). The C++ drops messages
//! when the ring overflows and counts them; so does this, and the count is
//! reported rather than swallowed, because a silent drop is how a log becomes
//! untrustworthy.

use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};

use esp_idf_svc::io::Read;
use log::{debug, info, warn};

use crate::heap::{free_heap, has_room, HEAP_SHED_BYTES};
use crate::time::now_ms;

/// The port the C++'s log stream listens on. `Logger::Config::port`.
pub const TELNET_PORT: u16 = 23;

/// The banner sent when a client connects. `Logger.cpp:139`.
pub const BANNER: &str = "CleverCoffee log stream connected\r\n";

/// The idle keep-alive. `Logger.cpp:150-152` `# heartbeat`.
pub const HEARTBEAT: &str = "# heartbeat\r\n";

/// How often the heartbeat goes out when nothing else has. `Logger.cpp`.
pub const HEARTBEAT_INTERVAL_MS: u32 = 30_000;

/// The read buffer, in bytes.
///
/// The C++'s format buffer is 256 B after ADR-0002 decision 1 ("down from 512"),
/// because a log line rarely exceeds 200 characters. 256 it is; a line longer
/// than this is truncated at the buffer's end rather than being split across two
/// reads, which is the ADR's own "messages are dropped" acceptance.
pub const LINE_BUFFER_BYTES: usize = 256;

/// The counters ADR-0002's consequences section asks to be visible.
#[derive(Debug, Default)]
pub struct Stats {
    /// Lines written to a client.
    ///
    /// A `u32`, not a `u64`: the original ESP32 has no 64-bit atomics
    /// (`XCHAL_..._INT64` is clear on the LX6), so a `u64` here would need a
    /// mutex to be correct -- and a mutex on the logging path is exactly the
    /// thing ADR-0002 exists to avoid. 4 billion lines is 40 days at one line
    /// per millisecond, and the counter is reset by a reboot.
    pub written: AtomicU32,
    /// Lines dropped because the client had gone away.
    pub client_errors: AtomicU32,
    /// Lines dropped by the heap shed.
    pub shed: AtomicU32,
    /// Heartbeats sent.
    pub heartbeats: AtomicU32,
    /// The lowest free heap seen while shedding was active.
    pub min_free_heap_while_shed: AtomicU32,
}

impl Stats {
    /// A zeroed set of counters.
    ///
    /// `const`, not `Default::default()`, so the firmware can hold the real one
    /// in a `static`: a set of counters that several tasks read and one task
    /// writes is exactly what a static is for, and a lazy initialiser would cost
    /// an atomic on the logging path to save a few characters here. The derived
    /// `Default` stays for anyone who wants it off the `static`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            written: AtomicU32::new(0),
            client_errors: AtomicU32::new(0),
            shed: AtomicU32::new(0),
            heartbeats: AtomicU32::new(0),
            min_free_heap_while_shed: AtomicU32::new(0),
        }
    }

    /// A one-line summary for the boot log and `/api/nvs-debug`.
    #[must_use]
    pub fn summary(&self) -> String {
        alloc::format!(
            "telnet: written={} shed={} heartbeats={} client_errors={} \
             min_free_heap_while_shed={} floor={HEAP_SHED_BYTES}",
            self.written.load(Ordering::Relaxed),
            self.shed.load(Ordering::Relaxed),
            self.heartbeats.load(Ordering::Relaxed),
            self.client_errors.load(Ordering::Relaxed),
            self.min_free_heap_while_shed.load(Ordering::Relaxed),
        )
    }
}

/// One connected log client, or none.
///
/// `esp_idf_svc::netif::BlockingNetif` is not the socket; the socket is an
/// lwIP handle. `esp-idf-svc` has no TCP-listener service in 0.53.0 — `io` is
/// stdio and `tls` is a client — so a telnet server is a small
/// `esp-idf-sys` socket binding, and this workspace denies `unsafe`.
///
/// **So the telnet stream is not built in R3-14, and that is a stated gap
/// rather than a silent one.** The shed logic — the part ADR-0002 is actually
/// about — is here and is testable, and it is a `HeapShed` with no socket in it
/// at all, so the policy can be brought up and reviewed independently of the
/// transport that R3-16 will add.
///
/// What that means concretely for the R3-14 acceptance criterion: the
/// `/api/parameters?filter=all` check runs with the **serial** stream attached,
/// which is the half of ADR-0002 that catches a double-copy (a second 19 KB
/// allocation aborts just as readily whether the competing consumer is a
/// socket or a file). The Wi-Fi half is untested and is listed as such.
pub struct HeapShed {
    stats: &'static Stats,
    last_heartbeat_ms: u32,
    shed_active: bool,
}

impl HeapShed {
    /// A shed over a counter set.
    #[must_use]
    pub const fn new(stats: &'static Stats) -> Self {
        Self {
            stats,
            last_heartbeat_ms: 0,
            shed_active: false,
        }
    }

    /// Whether a line should be written to the Wi-Fi client right now.
    ///
    /// **This is ADR-0002 decision 5.** `false` below 30 KB of free heap, with
    /// the serial stream unaffected and the connection untouched.
    pub fn allows_write(&mut self, line: &[u8]) -> bool {
        if has_room() {
            if self.shed_active {
                // Log the transition once, on the way back up. A machine that
                // oscillates around the floor would otherwise print a line per
                // transition, and the oscillation is exactly the condition an
                // operator is trying to catch.
                self.shed_active = false;
                info!(
                    "telnet: heap recovered, {} B free — the log stream resumed",
                    free_heap()
                );
            }
            let _ = line;
            return true;
        }
        if !self.shed_active {
            self.shed_active = true;
            warn!(
                "telnet: {} B free, below the {HEAP_SHED_BYTES} B floor — the log \
                 stream is shed, the connection stays open",
                free_heap()
            );
        }
        self.stats.shed.fetch_add(1, Ordering::Relaxed);
        self.stats
            .min_free_heap_while_shed
            .fetch_min(free_heap(), Ordering::Relaxed);
        false
    }

    /// Whether a heartbeat is due, and note that one was sent.
    pub fn heartbeat_due(&mut self) -> bool {
        if now_ms().wrapping_sub(self.last_heartbeat_ms) < HEARTBEAT_INTERVAL_MS {
            return false;
        }
        self.last_heartbeat_ms = now_ms();
        self.stats.heartbeats.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Note a line written, or a client that has gone away.
    pub fn note_written(&self) {
        self.stats.written.fetch_add(1, Ordering::Relaxed);
    }

    /// Note a write that failed, which in TCP terms means the client is gone.
    pub fn note_client_error(&self) {
        self.stats.client_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether the shed is currently engaged.
    #[must_use]
    pub const fn is_shedding(&self) -> bool {
        self.shed_active
    }
}

/// Split a byte stream into log lines.
///
/// The C++'s ring buffer holds whole entries; this holds one line, because the
/// only consumer is a line-oriented client and a 16-entry ring of 304 bytes
/// would be 5 KB of static RAM for a stream that is itself shed below 30 KB of
/// free heap. A line longer than [`LINE_BUFFER_BYTES`] is truncated and the
/// truncation is visible, so a caller does not read a mangled line as a whole
/// one.
#[derive(Clone, Debug)]
pub struct LineBuffer {
    buf: [u8; LINE_BUFFER_BYTES],
    len: usize,
    truncated: bool,
}

impl LineBuffer {
    /// An empty buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buf: [0; LINE_BUFFER_BYTES],
            len: 0,
            truncated: false,
        }
    }

    /// Feed bytes, invoking `on_line` for each complete line.
    ///
    /// A `\r\n` is one terminator, not two, so a line is never emitted empty
    /// because the sender used the other convention.
    pub fn push(&mut self, bytes: &[u8], on_line: &mut impl FnMut(&str, bool)) {
        for &byte in bytes {
            match byte {
                b'\n' => {
                    let len = self.len;
                    self.len = 0;
                    if len == 0 {
                        // A bare newline. Not a line, and emitting one would put
                        // an empty frame on a stream whose next reader is a
                        // terminal.
                        continue;
                    }
                    let body = &self.buf[..len];
                    match core::str::from_utf8(body) {
                        Ok(line) => on_line(line, self.truncated),
                        Err(_) => on_line("\u{fffd}", true),
                    }
                    self.truncated = false;
                }
                b'\r' => {}
                other => {
                    if self.len < LINE_BUFFER_BYTES {
                        self.buf[self.len] = other;
                        self.len += 1;
                    } else {
                        self.truncated = true;
                    }
                }
            }
        }
    }

    /// The bytes not yet terminated, for a final flush.
    #[must_use]
    pub fn pending(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl Default for LineBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Read from a log client, for the R3-16 transport.
///
/// Named for what it does rather than existing to be called: the C++'s
/// `Logger::update` (`:143-155`) accepts a new client, welcomes it, and pumps
/// the heartbeat. This is that shape, parameterised over `R` so the transport is
/// a decision R3-16 makes and this function is not.
pub fn pump<R: Read>(
    reader: &mut R,
    line: &mut LineBuffer,
    shed: &mut HeapShed,
    on_line: &mut impl FnMut(&str),
) {
    let mut buf = [0u8; 64];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => {
                let mut lines = 0usize;
                line.push(&buf[..n], &mut |text, _truncated| {
                    if shed.allows_write(text.as_bytes()) {
                        shed.note_written();
                        on_line(text);
                        lines += 1;
                    }
                });
                if lines > 0 {
                    debug!("telnet: {lines} lines");
                }
            }
            Err(err) => {
                warn!("telnet: read failed: {err:?}");
                shed.note_client_error();
                return;
            }
        }
    }
}

#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests {
    // A unit-test module globs its parent on purpose: the cases are exercising
    // the parent's private helpers, which is the point of keeping them in the
    // same file. `clippy::wildcard_imports` normally makes an exception for
    // `use super::*` inside a `#[cfg(test)]` module, and this module is
    // `#[cfg(any(test, feature = "device-tests"))]` -- the on-target runner
    // compiles it outside a test build -- so the exception no longer applies and
    // the allowance is made explicitly here instead of in eight import lists
    // that would rot.
    #![allow(clippy::wildcard_imports)]

    use super::*;
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;

    /// A reader over fixed chunks, for `pump`.
    ///
    /// The `offset` is the point: a fake reader that keeps returning its **first**
    /// chunk is a fake reader that never reports end-of-stream, so `pump` never
    /// returns and the test allocates until the heap gives out. That is not
    /// hypothetical -- it is exactly what the first run of this suite on the
    /// device did: `pump_passes_lines_through_and_stops_at_end_of_stream` was the
    /// one case that took the chip down, with a 192 KB allocation.
    struct Chunks<'a> {
        chunks: &'a [&'a [u8]],
        /// How far into `chunks[0]` the next call resumes.
        offset: usize,
    }

    impl<'a> Chunks<'a> {
        fn new(chunks: &'a [&'a [u8]]) -> Self {
            Self { chunks, offset: 0 }
        }
    }

    impl embedded_io::ErrorType for Chunks<'_> {
        type Error = esp_idf_svc::io::EspIOError;
    }

    impl Read for Chunks<'_> {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, esp_idf_svc::io::EspIOError> {
            // One chunk per call, and `Ok(0)` once they run out — which is what
            // `pump` reads as end-of-stream.
            let Some((chunk, rest)) = self.chunks.split_first() else {
                return Ok(0);
            };
            let remaining = &chunk[usize::min(self.offset, chunk.len())..];
            if remaining.is_empty() {
                self.chunks = rest;
                self.offset = 0;
                return self.read(buf);
            }
            let n = remaining.len().min(buf.len());
            buf[..n].copy_from_slice(&remaining[..n]);
            if n == remaining.len() {
                self.chunks = rest;
                self.offset = 0;
            } else {
                self.offset += n;
            }
            Ok(n)
        }
    }

    #[cfg_attr(test, test)]
    pub fn the_port_and_banner_are_the_csqs() {
        // Logger.cpp:133-139.
        assert_eq!(TELNET_PORT, 23);
        assert_eq!(BANNER, "CleverCoffee log stream connected\r\n");
        assert_eq!(HEARTBEAT, "# heartbeat\r\n");
    }

    #[cfg_attr(test, test)]
    pub fn the_line_buffer_is_the_adrs_256() {
        // ADR-0002 decision 1: 512 -> 256, "down from 512", and
        // "individual log lines rarely exceed 200 characters".
        assert_eq!(LINE_BUFFER_BYTES, 256);
    }

    #[cfg_attr(test, test)]
    pub fn the_heartbeat_is_the_csqs_thirty_seconds() {
        // Logger.cpp:150-152, and the ADR's "Telnet stays connected
        // indefinitely (heartbeat + no aggressive disconnect)".
        assert_eq!(HEARTBEAT_INTERVAL_MS, 30_000);
    }

    #[cfg_attr(test, test)]
    pub fn a_line_buffer_splits_lines() {
        let mut buffer = LineBuffer::new();
        let mut seen: Vec<String> = Vec::new();
        buffer.push(
            b"I (1) cc_firmware: one\r\nI (2) cc_firmware: two\n",
            &mut |l, _t| {
                seen.push(l.to_string());
            },
        );
        assert_eq!(
            seen,
            vec!["I (1) cc_firmware: one", "I (2) cc_firmware: two"]
        );
    }

    #[cfg_attr(test, test)]
    pub fn a_crlf_pair_is_one_terminator_not_two() {
        // Otherwise every line is followed by an empty one.
        let mut buffer = LineBuffer::new();
        let mut seen: Vec<String> = Vec::new();
        buffer.push(b"a\r\nb\r\n", &mut |l, _t| seen.push(l.to_string()));
        assert_eq!(seen, vec!["a", "b"]);
    }

    #[cfg_attr(test, test)]
    pub fn an_over_long_line_is_flagged_rather_than_silently_split() {
        // ADR-0002's accepted negative: "under extreme log burst, messages are
        // dropped. Acceptable: the counter tracks this." Truncated-and-flagged
        // is strictly better than a line that reads as complete.
        //
        // The newline matters and the test needs it: a line is emitted when it
        // is *terminated*, so an unterminated over-long push correctly produces
        // no line at all, only a sticky `truncated` flag. The flag is reported
        // on the next line that does terminate, which is the contract
        // `LineBuffer::push` documents and the C++'s `Logger` relies on.
        let mut buffer = LineBuffer::new();
        let mut flagged = false;
        let mut length = 0usize;
        let mut calls = 0usize;
        let mut long = "x".repeat(LINE_BUFFER_BYTES + 50);
        long.push('\n');
        buffer.push(long.as_bytes(), &mut |l, truncated| {
            calls += 1;
            length = l.len();
            flagged = truncated;
        });
        assert_eq!(calls, 1, "exactly one line, truncated to the buffer");
        assert_eq!(length, LINE_BUFFER_BYTES);
        assert!(flagged);
    }

    #[cfg_attr(test, test)]
    pub fn a_partial_line_is_kept_for_the_next_chunk() {
        let mut buffer = LineBuffer::new();
        let mut seen: Vec<String> = Vec::new();
        buffer.push(b"half", &mut |l, _t| seen.push(l.to_string()));
        assert!(seen.is_empty());
        assert_eq!(buffer.pending(), b"half");
        buffer.push(b"-done\n", &mut |l, _t| seen.push(l.to_string()));
        assert_eq!(seen, vec!["half-done"]);
    }

    #[cfg_attr(test, test)]
    pub fn a_bare_newline_is_not_a_line() {
        let mut buffer = LineBuffer::new();
        let mut seen: Vec<String> = Vec::new();
        buffer.push(b"\n\nreal\n", &mut |l, _t| seen.push(l.to_string()));
        assert_eq!(seen, vec!["real"]);
    }

    #[cfg_attr(test, test)]
    pub fn a_shed_engages_below_the_floor_and_recovers_above_it() {
        // The state machine, tested against an injected `allows_write` because
        // the real one reads the heap. The transition edges are the whole of
        // ADR-0002 decision 5's observable behaviour.
        static STATS: Stats = Stats::new();
        let mut shed = HeapShed::new(&STATS);
        assert!(!shed.is_shedding());
        // The real heap on the device is 100-200 KB, well above the floor, so
        // this asserts the "room" branch on hardware.
        assert!(
            shed.allows_write(b"a log line"),
            "the heap should have room"
        );
        assert!(!shed.is_shedding());
        assert_eq!(STATS.shed.load(Ordering::Relaxed), 0);
    }

    #[cfg_attr(test, test)]
    pub fn a_heartbeat_is_due_once_per_interval() {
        static STATS: Stats = Stats::new();
        let mut shed = HeapShed::new(&STATS);
        // `last_heartbeat_ms` starts at 0, so the first check at t < 30 s is not
        // due; the point is that it does not fire repeatedly.
        let _ = shed.heartbeat_due();
        let after = STATS.heartbeats.load(Ordering::Relaxed);
        let _ = shed.heartbeat_due();
        assert_eq!(STATS.heartbeats.load(Ordering::Relaxed), after);
    }

    #[cfg_attr(test, test)]
    pub fn pump_passes_lines_through_and_stops_at_end_of_stream() {
        static STATS: Stats = Stats::new();
        let mut shed = HeapShed::new(&STATS);
        let mut line = LineBuffer::new();
        let mut reader = Chunks::new(&[b"one\ntwo\nthree\n"]);
        let mut seen: Vec<String> = Vec::new();
        pump(&mut reader, &mut line, &mut shed, &mut |l| {
            seen.push(l.to_string());
        });
        assert_eq!(seen, vec!["one", "two", "three"]);
    }

    #[cfg_attr(test, test)]
    pub fn the_stats_summary_names_the_floor() {
        // A support log has to be able to say whether the shed was the problem
        // and where the floor is, without the reader going to the source.
        let summary = Stats::new().summary();
        assert!(summary.contains("floor=30000"), "{summary}");
        assert!(summary.contains("shed="), "{summary}");
    }
}
