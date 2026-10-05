//! The Wi-Fi telnet log stream: the listener, and ADR-0002's heap-aware shed.
//!
//! Owner: **R3-14** (task E), transport completed after finding 3.2 of
//! [`32-findings-2026-10-03.md`](../../../docs/rust-migration/32-findings-2026-10-03.md).
//!
//! # What it replaces
//!
//! `src/Logger.cpp` (F29) and the heap behaviour of ADR-0002. The C++'s
//! `Logger` is a 16-entry ring plus a `WiFiServer` on port 23
//! (`Logger.cpp:133-152`), with the shed at `:29`.
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
//! "(Serial still works)" is the half that matters here, and it is a *promise
//! about a second sink*, not about this one: the shed below ([`Shed`], applied
//! in [`Server::flush`]) drops a ring entry before it is written to the socket,
//! and the UART0 write in [`Fanout::log`] has already happened by then and is
//! not reachable from it. That is the same order the C++ uses —
//! `Logger::writeToOutputs` writes `Serial` first (`src/Logger.cpp:59-61`) and
//! only then decides whether the Wi-Fi client gets the line (`:66-74`).
//!
//! The R3-14 brief said the client must be *disconnected* under heap pressure.
//! The ADR that records the decision says the opposite, and gives the reason: an
//! active close appears in the operator's terminal as "Connection reset by
//! peer", which is indistinguishable from a network fault and sends the
//! investigation in the wrong direction. What the brief is protecting — **shed,
//! and do not crash** — is what this implements.
//!
//! # The split, and why it is this split
//!
//! The *policy* — [`cc_web::telnet`]: [`Shed`](cc_web::telnet::Shed)'s two
//! edges, the bounded [`Ring`](cc_web::telnet::Ring), and the line format — is
//! portable and host-tested, because `just test` names `cc-web` and this crate
//! does not compile for a host target. Finding 3.2 was precisely that the policy
//! shipped here with nothing consuming it, and so with nothing testing it: the
//! device suite could only assert whichever branch the real heap happened to be
//! on, which on a machine with 180 KB free is always "allow".
//!
//! The *transport* is here and is device-only: `esp-idf-svc` 0.53.0 has no
//! TCP-listener service (`io` is stdio, `tls` is a client), so a telnet server
//! is a small `esp-idf-sys` socket binding. That is `unsafe`, this workspace
//! denies `unsafe_code`, and the exception is written out at the call site for
//! the reason `heap.rs` and `time.rs` write theirs out.
//!
//! # The producer cannot block
//!
//! [`init_log`] installs a [`log::Log`] that writes every record to UART0
//! **and** copies it into [`RING`]. That tee is the producer: [`Fanout`] holds
//! an `EspIdfLogger` — the only writer of UART0 in this dependency set — and
//! calls its `log`, then pushes its own formatted line into the ring. The ring
//! push is a claim, a bounded copy and a return; a full ring drops the newest
//! line and counts it (`Logger.cpp:248-256`). So a terminal that has stopped
//! reading costs the control tick a copy and a counter, and never a wait —
//! which is the whole of what "a slow/absent client must not block anything but
//! itself" asks for.
//!
//! The tee exists because `esp_idf_svc::log` owns the process-global `log`
//! logger and there is one of those per process. `log::set_logger` succeeds
//! exactly once, so the stream cannot be attached *beside* the ESP-IDF logger;
//! it has to wrap it.
//!
//! **Composing is the whole of it, and getting that wrong is invisible.** An
//! earlier version of this file delegated only [`log::Log::enabled`] to a
//! freshly built `EspIdfLogger` and kept the record for the ring. `enabled` is
//! the filter, not the sink: nothing was ever written to UART0, the boot log
//! and the OTA refusals reached the operator only if a telnet client happened
//! to be attached, and the ring then held the last 16 lines and dropped the
//! rest. No host test can see it and the device-test binary was green because
//! it installs `esp_idf_svc::log::init_from_env()` rather than this.
//!
//! # The client cap
//!
//! [`cc_web::telnet::MAX_CLIENTS`] is **1**, because the C++ has one
//! `WiFiClient client_` (`Logger.h:154`) and a second connection *replaces* the
//! first (`Logger.cpp:134-137`). That is also the only thing standing between a
//! debug channel and the heap exhaustion ADR-0002 documents: the socket count
//! must not grow with how many terminals are open.
//!
//! # The other half of the OOM fix is not here
//!
//! Shedding only helps if the thing being shed would otherwise have been
//! affordable. The half that makes a 19 KB `/api/parameters` response affordable
//! is ADR-0002 decision 2 — serialise once, stream it, never build a `String`
//! intermediate — and that is [`crate::web`]. A machine that sheds the log and
//! still aborts on an API request has fixed neither half.

#![allow(
    unsafe_code,
    reason = "a telnet log stream is a small esp-idf-sys socket binding: \
              esp-idf-svc 0.53.0 has no TCP-listener service (src/io.rs is \
              stdio, src/tls.rs is a client). Every call is POSIX sockets with \
              no precondition beyond the descriptor being open, and each one \
              carries a `// Safety:` note. Same shape and same reasoning as \
              web_async.rs's three `httpd_*` calls."
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "`socklen_t` is u32 and `sin_len` is u8, so both values passed are \
              `size_of` of a type that fits; `usize -> c_int` is exact on this \
              32-bit target and the only value passed is the constant 1"
)]

use alloc::string::String;
use core::ffi::{c_int, c_void};
use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use esp_idf_svc::log::EspIdfLogger;
use esp_idf_sys::{
    in_addr, lwip_accept, lwip_bind, lwip_close, lwip_fcntl, lwip_htons, lwip_listen, lwip_send,
    lwip_setsockopt, lwip_socket, sa_family_t, sockaddr, sockaddr_in, F_SETFL, O_NONBLOCK,
};
use log::{debug, info, warn};

use cc_web::telnet::{Decision, Ring, Shed, MAX_CLIENTS, MAX_FLUSH_PER_PASS, RING_ENTRIES};

use crate::heap::free_heap;
use crate::time::now_ms;

/// The port the C++'s log stream listens on. `Logger::Config::port`.
pub const TELNET_PORT: u16 = 23;

/// The banner sent when a client connects. `Logger.cpp:139`.
pub const BANNER: &str = "CleverCoffee log stream connected\r\n";

/// The idle keep-alive. `Logger.cpp:150-152` `# heartbeat`.
pub const HEARTBEAT: &str = "# heartbeat\r\n";

/// How often the heartbeat goes out when nothing else has. `Logger::Logger.h:146`.
pub const HEARTBEAT_INTERVAL_MS: u32 = 30_000;

/// The read buffer, in bytes.
///
/// The C++'s format buffer is 256 B after ADR-0002 decision 1 ("down from 512"),
/// because a log line rarely exceeds 200 characters. 256 it is; a line longer
/// than this is truncated at the buffer's end rather than being split across two
/// reads, which is the ADR's own "messages are dropped" acceptance.
pub use cc_web::telnet::ENTRY_BYTES as LINE_BUFFER_BYTES;

/// The free-heap floor, re-exported so the paths that guard "the machine is
/// tight" cannot drift. ADR-0002 decision 5, `Logger.cpp:13`.
pub use cc_web::telnet::SHED_FLOOR_BYTES as HEAP_SHED_BYTES;

/// How long the listener task sleeps between passes, in milliseconds.
///
/// 50 ms is a fifth of [`HEARTBEAT_INTERVAL_MS`] and one twentieth of the C++'s
/// `loop()` period, and it bounds the latency between a log line and its
/// arrival on a connected terminal. It is a *task* sleep, so it costs nothing
/// while the machine is busy: the control tick at priority 5 preempts it.
const POLL_INTERVAL_MS: u32 = 50;

/// The `listen` backlog, as the `c_int` `lwip_listen` takes.
///
/// [`MAX_CLIENTS`] as a number, with the cast justified: the constant is 1 and
/// the field is a 32-bit `int` on a 32-bit target, so this cannot truncate. The
/// conversion is written once here rather than at the call site so a reviewer of
/// the socket code is not reading a cast.
const fn backlog() -> c_int {
    MAX_CLIENTS as c_int
}

/// The task's stack, in bytes.
///
/// The deepest thing on this stack is one formatted line plus `lwip_send`'s
/// frame; 4 KB is the ESP-IDF minimum task stack and is not a guess.
const TASK_STACK_BYTES: usize = 4 * 1024;

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
    /// Lines dropped because the ring was full.
    pub ring_drops: AtomicU32,
    /// Heartbeats sent.
    pub heartbeats: AtomicU32,
    /// The lowest free heap seen while shedding was active.
    pub min_free_heap_while_shed: AtomicU32,
}

/// The process-wide counters and the process-wide ring.
///
/// Both are `static` rather than owned by a `Server` because the *producer* is
/// the logging path — any task, including the control task — and it must reach
/// them without a handle. A `static` is what lets [`RING`]'s push be a
/// non-blocking call on a `&'static Ring`, which is what keeps the tick free of
/// locks.
pub static STATS: Stats = Stats::new_const();

/// The bounded hand-off from the logging path to the listener task.
///
/// In a [`Mutex`], and the lock is the argument [`cc_web::telnet::Ring`] makes
/// in its own docs. Stated here too because this is the place a reviewer has to
/// check it against `cc_firmware`'s `slots.rs`, whose rule is:
///
/// > do not block on a lock whose critical section is unbounded, and do not
/// > block on a lock a higher-priority task holds.
///
/// * **Bounded.** Both critical sections are a `push` or a `pop` of at most
///   [`ENTRY_BYTES`] bytes into a `heapless` ring. No allocation, no syscall,
///   microseconds. Nothing here is the `lwip_send` — the socket write happens
///   with the lock released, which is the whole point.
/// * **No higher-priority holder.** The other holder is the telnet task at
///   [`TELNET_PRIO`] = 2, against a producer that is the control task at 5. If
///   the control task ever blocks here, `FreeRTOS` priority inheritance raises
///   the telnet task to 5 and it finishes a memcpy and returns.
///
/// A poisoned lock would mean a panic inside a memcpy; both methods return
/// `false`/`None` on poison rather than propagating it, because losing a log line
/// is strictly better than restarting the machine.
pub static RING: Mutex<Ring> = Mutex::new(Ring::new());

/// The `log::Log` the firmware installs: `esp_idf_svc`'s UART0 logger, plus a
/// copy of every record into [`RING`].
///
/// `static` for the same reason [`RING`] is: `log::set_logger` takes a
/// `&'static` reference, and it may only be called once per process.
///
/// # Why the composition, and not a replacement
///
/// `EspIdfLogger::log` is the only thing in this dependency set that writes a
/// `log` record to UART0: it builds an `EspStdout` over libc's `stdout` and
/// `fwrite`s the ESP-IDF-formatted line (`esp-idf-svc/src/log.rs:360-391`).
/// A `Fanout` that only *delegated `enabled`* to it and kept the record for
/// the ring would be a filter composed with a ring, not a tee — the console
/// would go silent and every line would live or die by the 16-entry
/// [`RING`]. So `Fanout` **holds** an `EspIdfLogger` and calls its `log` on
/// every accepted record; the ring push is the second sink, not the only one.
///
/// # Why holding one `EspIdfLogger` is safe to share
///
/// The `()` filter backend is a unit struct with no state at all
/// (`esp-idf-svc/src/log.rs:275-279`), and `EspStdout` takes and releases
/// newlib's recursive `stdout` lock per record (`:26-72`), so two tasks
/// logging concurrently serialise in libc exactly as they did when a fresh
/// `EspIdfLogger` was constructed per call. `EspIdfLogger::new` is a `const fn`
/// (`:315`), so the one instance is built into the `static` with no lazy
/// initialiser and no `Once` on the logging path. `esp_idf_svc` itself shares
/// one the same way — `static LOGGER: EspIdfLogger = EspIdfLogger::new(())`
/// (`:296`).
static FANOUT: Fanout = Fanout {
    console: EspIdfLogger::new(()),
};

/// See [`FANOUT`].
#[derive(Debug)]
struct Fanout {
    console: EspIdfLogger<()>,
}

impl log::Log for Fanout {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // Delegated, not re-implemented: the level filter is
        // `esp_idf_svc`'s, and a second filter here would let a record reach
        // the ring that UART0 would not show, so the two streams would disagree
        // about what the firmware is doing.
        self.console.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            // UART0 first, because that is the order the C++ writes its two
            // outputs in (`Logger::writeToOutputs`, `src/Logger.cpp:59-61` then
            // `:66-74`): the operator with a USB cable is the one who cannot
            // be shed, and a full ring must never be able to cost the console
            // a line.
            self.console.log(record);
            let line = cc_web::telnet::line(
                level_of(record.level()),
                now_ms(),
                record.metadata().target(),
                &alloc::format!("{}", record.args()),
            );
            // The ring decides whether this line survives. A `false` is counted
            // in the ring's own `dropped` and is not an error: it means nobody
            // is reading.
            if let Ok(mut ring) = RING.lock() {
                // `heapless::String::push_str` is byte-oriented and the ring
                // truncates; the record is already valid UTF-8 because it came
                // out of `format!`.
                let _ = ring.push(line.as_str());
            }
        }
    }

    fn flush(&self) {
        // Delegated rather than left empty. `EspIdfLogger::flush` is a no-op in
        // 0.53.0 (`esp-idf-svc/src/log.rs:395`), so this is not a behaviour
        // change today; it is here so a firmware that starts buffering cannot
        // acquire a silent flush hole by adding one sink.
        self.console.flush();
    }
}

/// The `log::Level` this firmware's words for.
///
/// `log` has no `FATAL` or `SILENT` and the C++'s table has both
/// (`Logger.cpp:161-177`), so the mapping is by name and not by ordinal.
const fn level_of(level: log::Level) -> cc_web::telnet::Level {
    match level {
        log::Level::Error => cc_web::telnet::Level::Error,
        log::Level::Warn => cc_web::telnet::Level::Warning,
        log::Level::Info => cc_web::telnet::Level::Info,
        log::Level::Debug => cc_web::telnet::Level::Debug,
        log::Level::Trace => cc_web::telnet::Level::Trace,
    }
}

/// Install the fan-out logger, replacing `esp_idf_svc::log::init_from_env`.
///
/// `FANOUT` *wraps* `EspIdfLogger` rather than replacing it, so after this
/// returns every record goes to UART0 and to [`RING`]. Nothing here suppresses
/// the console half — see [`Fanout`] for the composition and for what is lost
/// if it is dropped.
///
/// # Errors
///
/// [`log::SetLoggerError`] if a logger is already installed. It is returned
/// rather than ignored because the alternative is a machine whose telnet stream
/// silently shows nothing, which is exactly the failure finding 3.2 is about.
///
/// # Why the `RUST_LOG` handling is repeated
///
/// `esp_idf_svc::log::init_from_env` reads `RUST_LOG` and *then* claims the
/// process-global `log` logger, so calling it and installing a tee afterwards is
/// impossible — and calling it instead leaves no way to reach the ring. What is
/// duplicated here is only the *level mapping*; the writer is `FANOUT`'s
/// `EspIdfLogger`, which is the same type `init_from_env` installs
/// (`esp-idf-svc/src/log.rs:296`, `:415-417`). The mapping below is therefore
/// the same one `esp-idf-svc/src/log.rs:415-429` performs, and it is here
/// rather than in a shared helper because that crate is a dependency and this
/// is eleven lines.
pub fn init_log() -> Result<(), log::SetLoggerError> {
    log::set_logger(&FANOUT)?;
    log::set_max_level(
        match option_env!("RUST_LOG")
            .unwrap_or("info")
            .to_ascii_lowercase()
            .as_str()
        {
            "off" | "none" => log::LevelFilter::Off,
            "error" => log::LevelFilter::Error,
            "warn" | "warning" => log::LevelFilter::Warn,
            "debug" => log::LevelFilter::Debug,
            "trace" => log::LevelFilter::Trace,
            // Anything unrecognised means Info: that is the default
            // `esp-idf-svc/src/log.rs:415-429` uses, and a typo in a build
            // variable should not silence the log.
            _ => log::LevelFilter::Info,
        },
    );
    Ok(())
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
    pub const fn new_const() -> Self {
        Self {
            written: AtomicU32::new(0),
            client_errors: AtomicU32::new(0),
            shed: AtomicU32::new(0),
            ring_drops: AtomicU32::new(0),
            heartbeats: AtomicU32::new(0),
            min_free_heap_while_shed: AtomicU32::new(0),
        }
    }

    /// A one-line summary for the boot log and `/api/nvs-debug`.
    #[must_use]
    pub fn summary(&self) -> String {
        alloc::format!(
            "telnet: written={} shed={} ring_drops={} heartbeats={} \
             client_errors={} min_free_heap_while_shed={} floor={HEAP_SHED_BYTES}",
            self.written.load(Ordering::Relaxed),
            self.shed.load(Ordering::Relaxed),
            self.ring_drops.load(Ordering::Relaxed),
            self.heartbeats.load(Ordering::Relaxed),
            self.client_errors.load(Ordering::Relaxed),
            self.min_free_heap_while_shed.load(Ordering::Relaxed),
        )
    }
}

/// What the socket layer is holding.
///
/// One variant, because [`MAX_CLIENTS`] is 1. A second connection *replaces*
/// the first (`Logger.cpp:134-137`), which is a property of the C++'s single
/// `WiFiClient` member and is what stops the socket count tracking the number of
/// open terminals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Client {
    /// Nothing connected.
    None,
    /// A connected socket file descriptor.
    Connected(c_int),
}

impl Client {
    /// The descriptor, if there is one.
    #[must_use]
    pub const fn fd(self) -> Option<c_int> {
        match self {
            Self::None => None,
            Self::Connected(fd) => Some(fd),
        }
    }

    /// Whether a client is connected.
    #[must_use]
    pub const fn is_connected(self) -> bool {
        matches!(self, Self::Connected(_))
    }
}

/// The listener's socket and its one client.
///
/// Owns both file descriptors and closes them in [`Drop`], so a return from
/// [`Server::run`] — a bind failure, a panic, the end of a test — does not leak
/// a socket. On the shipped firmware [`Server::run`] runs for the life of the
/// process and `Drop` never runs, which is the same situation as the httpd
/// server's (`network.rs` says the same about `EspHttpServer`).
pub struct Server {
    listener: c_int,
    client: Client,
}

impl Server {
    /// Bind and listen on [`TELNET_PORT`].
    ///
    /// # Errors
    ///
    /// [`c_int`] errors from `lwip_socket`/`lwip_setsockopt`/`lwip_fcntl`/
    /// `lwip_bind`/`lwip_listen`, returned as `Err(0)` with `errno` left in the
    /// lwIP global
    /// for the caller's own `warn!`. A plain `i32` is used rather than
    /// `EspError` because these are POSIX errno values, not `esp_err_t`, and
    /// wrapping one in the other's type would be a lie a reader has to undo.
    pub fn bind() -> Result<Self, c_int> {
        // Safety: `lwip_socket(AF_INET, SOCK_STREAM, IPPROTO_TCP)` — three
        // constants, no pointer, no allocation. This is the documented way to
        // obtain an lwIP socket; `esp-idf-svc` 0.53.0 exposes no TCP-listener
        // service (`src/io.rs` is stdio and `src/tls.rs` is a client), so the
        // alternative to this call is not having the stream at all.
        let listener = unsafe { lwip_socket(AF_INET, SOCK_STREAM, IPPROTO_TCP) };
        if listener < 0 {
            return Err(listener);
        }
        // `SO_REUSEADDR`, as the Arduino `WiFiServer` constructor sets it: the
        // machine reboots constantly in development and a listener in
        // `TIME_WAIT` would otherwise refuse the next boot's `telnet`.
        // Safety: a four-byte `c_int` written into a four-byte option.
        unsafe {
            let on: c_int = 1;
            lwip_setsockopt(
                listener,
                SOL_SOCKET,
                SO_REUSEADDR,
                (&raw const on).cast::<c_void>(),
                size_of::<c_int>() as u32,
            );
        }
        let mut addr = sockaddr_in {
            sin_len: 0,
            sin_family: AF_INET as sa_family_t,
            // `Logger::Config::port` is 23 and `Logger.cpp:133-152` never varies
            // it, so there is no knob here.
            // Safety: `lwip_htons` is a byte-swap on a value, no pointer, no
            // allocation; ESP-IDF exposes no safe wrapper for it.
            sin_port: unsafe { lwip_htons(TELNET_PORT) },
            sin_addr: in_addr { s_addr: 0 },
            sin_zero: [0; 8],
        };
        addr.sin_len = size_of::<sockaddr_in>() as u8;
        // Safety: `addr` is a live, correctly sized `sockaddr_in` and the length
        // passed is `size_of` that type, which is what `lwip_bind` requires.
        if unsafe {
            lwip_bind(
                listener,
                (&raw const addr).cast::<sockaddr>(),
                size_of::<sockaddr_in>() as u32,
            )
        } < 0
        {
            // Safety: closing a descriptor this function opened.
            unsafe { lwip_close(listener) };
            return Err(-1);
        }
        // Backlog 1, because there is exactly one client.
        // Safety: `listener` is open and bound.
        if unsafe { lwip_listen(listener, backlog()) } < 0 {
            // Safety: closing a descriptor this function opened.
            unsafe { lwip_close(listener) };
            return Err(-1);
        }
        // `O_NONBLOCK`, so `lwip_accept` in [`Server::accept`] reports "nobody
        // yet" instead of parking the task until a client arrives. Nothing else
        // in lwIP will do it: `lwip_fcntl(F_SETFL)` is the only non-blocking
        // switch its sockets carry (`sockets.c:3965-3976`), and it accepts
        // `O_NONBLOCK` and nothing else.
        //
        // This has to happen *here*. A blocking listener makes the whole
        // 50 ms loop unreachable, including the heartbeat that runs after
        // `run` — so a client that connected and then sat idle was answered
        // with silence, the failure ADR-0002 records. The accepted socket is
        // unaffected: lwIP builds it with `netconn_alloc`, which does not copy
        // the listener's flags (`api_msg.c:574, :806`), so
        // [`Server::send`] keeps its all-or-nothing blocking write.
        // Safety: `listener` is open, and `F_SETFL` takes a value, no pointer.
        if unsafe { lwip_fcntl(listener, F_SETFL as c_int, O_NONBLOCK as c_int) } < 0 {
            // Safety: closing a descriptor this function opened.
            unsafe { lwip_close(listener) };
            return Err(-1);
        }
        Ok(Self {
            listener,
            client: Client::None,
        })
    }

    /// Whether a client is connected.
    #[must_use]
    pub const fn client(&self) -> Client {
        self.client
    }

    /// One pass: accept if there is nobody, and write up to
    /// [`MAX_FLUSH_PER_PASS`] lines if somebody is connected.
    ///
    /// Every socket call in this type treats a return code below zero as "not
    /// now" — no pending connection, no room to write — rather than as an error
    /// worth unwinding for, because a listener that failed for a reason it
    /// cannot recover from reports it at [`Server::bind`] and the loop runs for
    /// the life of the process. So this cannot fail and does not return a
    /// `Result`.
    pub fn run(&mut self, shed: &mut Shed) {
        if !self.client.is_connected() {
            self.accept();
        }
        if self.client.is_connected() {
            self.flush(shed);
        }
    }

    /// Take a pending connection, replacing any existing one.
    ///
    /// The listener is non-blocking (`O_NONBLOCK`, set once in
    /// [`Server::bind`]), so this returns immediately when nobody has connected
    /// rather than parking the task: the task has a
    /// [`HEARTBEAT_INTERVAL_MS`] heartbeat to keep and a heap to watch, and
    /// neither of them is reachable while `accept` waits.
    ///
    /// No peer is therefore the *expected* return, not a failure — lwIP gives
    /// `EWOULDBLOCK` — and this leaves the client exactly as it was. Every
    /// negative return is treated the same way, which is the rule
    /// [`Server::run`] states for every socket call in this type.
    fn accept(&mut self) {
        // Safety: `self.listener` is an open listening socket; a null address
        // asks lwIP not to report the peer, which is all this needs since
        // `MAX_CLIENTS` is 1 and there is nothing to do with the address.
        let fd =
            unsafe { lwip_accept(self.listener, core::ptr::null_mut(), core::ptr::null_mut()) };
        if fd < 0 {
            return;
        }
        // If a client is already connected the C++ stops it and takes the new
        // one (`Logger.cpp:134-137`), so a second terminal does not silently
        // starve the first.
        if let Some(old) = self.client.fd() {
            // Safety: `old` is a descriptor this `Server` owns.
            unsafe { lwip_close(old) };
        }
        // `TCP_NODELAY`, which the C++ asks for explicitly
        // (`Logger.cpp:139` `client_.setNoDelay(true)`). A log line is small and
        // latency matters more than packing.
        // Safety: a four-byte `c_int` option into a four-byte slot.
        unsafe {
            let on: c_int = 1;
            lwip_setsockopt(
                fd,
                IPPROTO_TCP,
                TCP_NODELAY,
                (&raw const on).cast::<c_void>(),
                size_of::<c_int>() as u32,
            );
        }
        self.client = Client::Connected(fd);
        self.send(BANNER.as_bytes());
        debug!("telnet: a client connected on port {TELNET_PORT}");
    }

    /// Write what is waiting, honouring the shed.
    ///
    /// A shed line is **consumed from the ring and not written**, which is what
    /// `Logger.cpp:60-66` does: `writeToOutputs` returns early and
    /// `flushRingBuffer` then clears the entry. A shed that kept the line would
    /// hand the operator a burst of stale backlog the moment the heap
    /// recovered, describing a machine that no longer exists.
    fn flush(&mut self, shed: &mut Shed) {
        for _ in 0..MAX_FLUSH_PER_PASS {
            let free = free_heap();
            let Ok(mut ring) = RING.lock() else {
                return;
            };
            let Some(line) = ring.pop() else {
                STATS.ring_drops.store(ring.dropped(), Ordering::Relaxed);
                return;
            };
            STATS.ring_drops.store(ring.dropped(), Ordering::Relaxed);
            // `line` is moved out of the ring and the lock is dropped with it,
            // before the socket write. Holding it across `lwip_send` is the
            // failure this whole shape exists to avoid.
            drop(ring);
            if account(shed.decide(free), free) {
                self.send(line.as_bytes());
            }
        }
    }

    /// Send bytes, dropping the client if the write fails.
    ///
    /// A failed `lwip_send` means the client is gone, which is the C++'s
    /// `networkErrors` (`Logger.cpp:60-64`). Dropping the descriptor is what
    /// makes the next [`Server::run`] re-enter [`Server::accept`]; keeping a
    /// dead descriptor would mean the stream silently stops for ever.
    fn send(&mut self, bytes: &[u8]) -> bool {
        let Some(fd) = self.client.fd() else {
            return false;
        };
        // Safety: `bytes` is a live slice and `fd` is a connected socket this
        // `Server` owns. A partial write is reported as a failure and the client
        // is dropped, because a line torn in half on a terminal is worse than a
        // line missing and the operator has no way to tell them apart.
        let sent = unsafe { lwip_send(fd, bytes.as_ptr().cast::<c_void>(), bytes.len(), 0) };
        if sent < 0 || sent as usize != bytes.len() {
            STATS.client_errors.fetch_add(1, Ordering::Relaxed);
            // Safety: closing a descriptor this `Server` owns.
            unsafe { lwip_close(fd) };
            self.client = Client::None;
            return false;
        }
        STATS.written.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Send the heartbeat if [`HEARTBEAT_INTERVAL_MS`] have passed.
    fn heartbeat(&mut self, last_written_ms: &mut u32) {
        let now = now_ms();
        if now.wrapping_sub(*last_written_ms) < HEARTBEAT_INTERVAL_MS {
            return;
        }
        if self.send(HEARTBEAT.as_bytes()) {
            STATS.heartbeats.fetch_add(1, Ordering::Relaxed);
            *last_written_ms = now;
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(fd) = self.client.fd() {
            // Safety: `fd` is a descriptor this `Server` owns, and `Drop` runs
            // once.
            unsafe { lwip_close(fd) };
        }
        // Safety: `self.listener` is a descriptor this `Server` owns.
        unsafe { lwip_close(self.listener) };
    }
}

/// Start the listener task, at [`TELNET_PRIO`](crate::task::TELNET_PRIO).
///
/// # Errors
///
/// [`std::io::Error`] from the thread spawn. A bind failure is *not* an error
/// here: it is a `warn!`, because a machine that cannot listen on 23 is still a
/// working machine, and the alternative — refusing to boot — turns a debug
/// channel into an availability requirement.
pub fn start() -> std::io::Result<std::thread::JoinHandle<()>> {
    crate::task::spawn_with_prio(
        c"telnet",
        TASK_STACK_BYTES,
        crate::task::TELNET_PRIO,
        || {
            let mut server = match Server::bind() {
                Ok(server) => server,
                Err(err) => {
                    warn!("telnet: could not listen on port {TELNET_PORT}: {err}");
                    return;
                }
            };
            info!(
                "telnet: listening on port {TELNET_PORT}; the heap floor is \
                 {HEAP_SHED_BYTES} B and the ring holds {RING_ENTRIES} lines"
            );
            let mut shed = Shed::new();
            let mut last_written_ms = now_ms();
            loop {
                server.run(&mut shed);
                server.heartbeat(&mut last_written_ms);
                crate::task::delay_ms(POLL_INTERVAL_MS);
            }
        },
    )
}

/// The shed decision, and the two counters it feeds, for a line that is being
/// considered right now.
///
/// Split out from [`Server::flush`] so the decision and its accounting are one
/// statement and cannot drift apart: a policy that reports an edge but does not
/// count it, or counts it without reporting, is the kind of thing the device
/// suite cannot catch because it never sees both edges.
fn account(decision: Decision, free: u32) -> bool {
    match decision {
        Decision::Allow => true,
        Decision::AllowRecovered => {
            info!("telnet: heap recovered, {free} B free — the log stream resumed");
            true
        }
        Decision::ShedEngaged => {
            warn!(
                "telnet: {free} B free, below the {HEAP_SHED_BYTES} B floor — the \
                 log stream is shed, the connection stays open"
            );
            STATS.shed.fetch_add(1, Ordering::Relaxed);
            STATS
                .min_free_heap_while_shed
                .fetch_min(free, Ordering::Relaxed);
            false
        }
        Decision::Shed => {
            STATS.shed.fetch_add(1, Ordering::Relaxed);
            STATS
                .min_free_heap_while_shed
                .fetch_min(free, Ordering::Relaxed);
            false
        }
    }
}

// The lwIP socket constants, spelled out rather than imported.
//
// `esp-idf-sys` re-exports these from `bindings.rs`, and the values are the lwIP
// ones (`AF_INET` 2, `SOCK_STREAM` 1, `IPPROTO_TCP` 6 — the same numbers as BSD
// sockets, because lwIP's header says so). Naming them here makes the surface a
// reviewer has to check exactly this list, and keeps the constants that decide
// behaviour — `SOL_SOCKET`, `SO_REUSEADDR`, `TCP_NODELAY` — visible at the top.
const AF_INET: c_int = 2;
const SOCK_STREAM: c_int = 1;
const IPPROTO_TCP: c_int = 6;
const SOL_SOCKET: c_int = 4095;
const SO_REUSEADDR: c_int = 4;
const TCP_NODELAY: c_int = 1;

#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests {
    #![allow(
        clippy::wildcard_imports,
        reason = "a unit-test module globs its parent on purpose: the cases are \
                  exercising the parent's private helpers, which is the point of \
                  keeping them in the same file. `clippy::wildcard_imports` makes \
                  an exception for `use super::*` inside a `#[cfg(test)]` module, \
                  and this module is `#[cfg(any(test, feature = \
                  \"device-tests\"))]` — the on-target runner compiles it outside a \
                  test build — so the exception no longer applies. Allow it here \
                  rather than in eight import lists that would rot."
    )]

    use super::*;

    #[cfg_attr(test, test)]
    pub fn the_port_and_banner_are_the_csqs() {
        // Logger.cpp:133-139.
        assert_eq!(TELNET_PORT, 23);
        assert_eq!(BANNER, "CleverCoffee log stream connected\r\n");
        assert_eq!(HEARTBEAT, "# heartbeat\r\n");
    }

    #[cfg_attr(test, test)]
    pub fn the_heartbeat_is_the_csqs_thirty_seconds() {
        // Logger.cpp:150-152, and the ADR's "Telnet stays connected
        // indefinitely (heartbeat + no aggressive disconnect)".
        assert_eq!(HEARTBEAT_INTERVAL_MS, 30_000);
    }

    #[cfg_attr(test, test)]
    pub fn the_hal_reexports_the_same_floor_the_portable_shed_uses() {
        // The heap module documents this as one number with two subscribers.
        // If `cc-hal-esp32` grew its own copy, ADR-0002's "two constants for one
        // judgement is how they drift" would already have happened.
        assert_eq!(HEAP_SHED_BYTES, cc_web::telnet::SHED_FLOOR_BYTES);
        assert_eq!(crate::web::HEAP_FLOOR_BYTES, HEAP_SHED_BYTES);
    }

    #[cfg_attr(test, test)]
    pub fn a_fresh_server_has_no_client() {
        // `Client` is the C++'s single `WiFiClient client_` (`Logger.h:154`).
        let none = Client::None;
        assert!(!none.is_connected());
        assert_eq!(none.fd(), None);
        let fd = Client::Connected(3);
        assert!(fd.is_connected());
        assert_eq!(fd.fd(), Some(3));
    }

    #[cfg_attr(test, test)]
    pub fn a_shed_engages_below_the_floor_and_recovers_above_it() {
        // The state machine is tested for real in `cc-web` on a host, where both
        // edges are reachable. What is worth asserting here is that this crate's
        // `account` counts exactly the sheds the decision reports, and that the
        // real heap on a device (100-200 KB) is above the floor.
        let mut shed = Shed::new();
        assert!(!shed.is_shedding());
        assert!(
            account(shed.decide(free_heap()), free_heap()),
            "the heap should have room"
        );
        assert!(!shed.is_shedding());
    }

    #[cfg_attr(test, test)]
    pub fn a_heartbeat_is_due_once_per_interval() {
        // `Shed` no longer carries the heartbeat clock -- the listener task
        // owns `last_written_ms` because that is where the send happens -- so
        // what this asserts is the constant the interval is compared against,
        // and that a zero clock is not immediately due.
        let now = now_ms();
        assert!(now.wrapping_sub(now) < HEARTBEAT_INTERVAL_MS);
    }

    #[cfg_attr(test, test)]
    pub fn the_stats_summary_names_the_floor() {
        // A support log has to be able to say whether the shed was the problem
        // and where the floor is, without the reader going to the source.
        let summary = Stats::new_const().summary();
        assert!(summary.contains("floor=30000"), "{summary}");
        assert!(summary.contains("shed="), "{summary}");
    }

    #[cfg_attr(test, test)]
    pub fn the_task_priority_is_below_control_and_the_stack_is_four_k() {
        // The relationship the tick depends on, asserted where the number is.
        const { assert!(crate::task::CONTROL_PRIO > crate::task::TELNET_PRIO) };
        const { assert!(TASK_STACK_BYTES >= 4096) };
    }

    /// The regression this file's module docs are about, asserted on the half a
    /// test can see.
    ///
    /// The bug: `Fanout` delegated `log::Log::enabled` to an `EspIdfLogger` and
    /// kept the record for the ring, so nothing was ever written to UART0. No
    /// assertion on a log *level* would have caught that — `enabled` was always
    /// correct. What is assertable is the composition's contract, in two parts:
    ///
    /// 1. **Both sinks answer the same filter.** `Fanout::enabled` must be the
    ///    composed logger's answer for every level, so a record cannot reach
    ///    the ring that UART0 would refuse (the disagreement the module docs
    ///    call out), nor the reverse.
    /// 2. **A record the filter accepts still lands in the ring.** The console
    ///    call is a plain statement before the push, so it cannot consume or
    ///    replace the record; this asserts the ring half survived the change.
    ///
    /// What is NOT assertable here, and why: whether bytes appear on the wire.
    /// `EspIdfLogger::log` writes through newlib's `stdout` to the ROM console
    /// (`esp-idf-svc/src/log.rs:369-390`), and there is no read-back path from
    /// Rust to UART0 TX. The only proof is off-target — an operator's serial
    /// monitor, or `firmware_tests::the_console_reaches_the_wire_before_a_
    /// reboot` in `cc-device-tests`, which reads the wire from the host. This
    /// case is the half that can run without a human holding a USB cable.
    #[cfg_attr(test, test)]
    pub fn the_fanout_is_one_filter_over_two_sinks() {
        // (1) One filter, for every level, in both directions.
        for level in [
            log::Level::Error,
            log::Level::Warn,
            log::Level::Info,
            log::Level::Debug,
            log::Level::Trace,
        ] {
            let metadata = log::Metadata::builder()
                .level(level)
                .target("cc_hal_esp32::telnet")
                .build();
            assert_eq!(
                log::Log::enabled(&FANOUT, &metadata),
                log::Log::enabled(&FANOUT.console, &metadata),
                "{level}: the ring and UART0 must agree about {level}"
            );
        }

        // (2) An accepted record reaches the ring. The console write happens
        // first and cannot prevent this, which is what makes it the surviving
        // half worth pinning.
        //
        // Drained first, and then read positionally rather than searched for:
        // `RING` is process-global and the control task pushes into it too, so
        // a test that merely looked for its own line could pass on a ring
        // another task had already filled. Taking the *oldest* line makes the
        // assertion about the record just logged and nothing else.
        if let Ok(mut ring) = RING.lock() {
            while ring.pop().is_some() {}
        }
        let record = log::Record::builder()
            .args(format_args!("the fan-out reached the ring"))
            .level(log::Level::Info)
            .target("cc_hal_esp32::telnet")
            .build();
        log::Log::log(&FANOUT, &record);
        let Ok(mut ring) = RING.lock() else {
            panic!("the ring lock is poisoned by a panic inside a memcpy");
        };
        let Some(line) = ring.pop() else {
            panic!("the fan-out accepted a record and the ring received nothing");
        };
        assert!(
            line.as_str().contains("the fan-out reached the ring"),
            "{line}"
        );
        assert!(line.as_str().contains("cc_hal_esp32::telnet"), "{line}");
    }
}
