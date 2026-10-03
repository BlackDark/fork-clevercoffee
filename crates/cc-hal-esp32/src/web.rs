//! The HTTP server: the 20 `/api/*` handlers, `/`, `/ui`, and the SSE stream.
//!
//! Owner: **R3-14** (task E).
//!
//! # What this replaces
//!
//! `src/network/WebServerManager.cpp`, 1208 lines, registering 24 routes
//! (`WebServerManager.cpp:327-980`): 20 `/api/*`, the `/` redirect, `/ui`, six
//! `serveStatic` mounts from `LittleFS`, and one `AsyncEventSource` on
//! `/events`.
//!
//! # ⚠ ADR-0002 is load-bearing, and both halves of it are here
//!
//! [`docs/adr/0002`](../../../docs/adr/0002-wifi-logging-ota-memory-architecture.md)
//! records a reproducible `abort()` from failed `operator new`: a user opened the
//! web UI, the browser fired 6–10 parallel API requests within two seconds of
//! boot, and `/api/parameters?filter=all` — 19 KB of JSON — was serialised into
//! a `JsonDocument`, copied into a `String`, and copied *again* by
//! `request->send()`. Three ~19 KB allocations. The two decisions that fixed it
//! are both in this file:
//!
//! * **Decision 2, `AsyncJsonResponse`:** serialise into one buffer and write it
//!   in chunks, never build a `String` intermediate for a large response. That
//!   is [`EspHttpConnection::write`] — `httpd_resp_send_chunk` — which streams.
//!   See [`respond`], which every JSON handler goes through, and
//!   [`respond_large`], which is the one that matters.
//! * **Decision 5, the heap-aware `WiFi` shed:** the telnet log stream stops
//!   writing below 30 KB of free heap. See [`crate::telnet`]. Note that the ADR
//!   says a **soft** shed — the connection stays open and resumes — and that an
//!   active `client_.stop()` was explicitly rejected because it surfaces as
//!   "Connection reset by peer" in the operator's terminal, which looks like a
//!   network problem rather than the memory problem it is.
//!
//! # SSE: the handler must return, because httpd is one task
//!
//! **02 §4 was half right, and the earlier version of this file was wrong about
//! the other half.** 02 §4 recorded SSE on ESP-IDF as "the biggest web-tier
//! unknown" and proposed an `esp-idf-sys` FFI shim. The shim is needed — but not
//! for the reason given. Chunked transfer-coding was never the problem:
//! `EspHttpConnection::write` *is* `httpd_resp_send_chunk` (`esp-idf-svc` 0.53.0
//! `src/http/server.rs:1121-1135`). The problem is **whose task the write
//! happens on**.
//!
//! ESP-IDF's httpd is a single task serving every route. `httpd_server_init`
//! creates one thread (`components/esp_http_server/src/httpd_main.c:533`) and
//! `httpd_thread` (`:329-350`) is `while (1) { httpd_server(hd); }` over one
//! `select()`. `esp-idf-svc` adds no per-connection task and no async layer for
//! HTTP. So a `/events` handler that loops while it streams is a web server that
//! serves nobody — and this file used to do exactly that.
//!
//! Measured on hardware, one browser tab with the stream open:
//!
//! | | 60 sequential `GET /api/parameters` |
//! |---|---|
//! | no stream connected | 60/60 `200`, 22–133 ms each |
//! | one `/events` client | **55/60 timed out** at the client's 5 s limit; the 5 that got through took 6–7 s |
//!
//! The C++ has no such failure because `ESPAsyncWebServer`'s `AsyncEventSource`
//! **returns from its handler** and is pushed to later from the main loop
//! (`WebServerManager.cpp:308-319`; senders at `:1128-1152`, driven by
//! `LoopManager::updateWebsite`).
//!
//! So the shape here is the C++'s shape: the handler sets the response headers,
//! detaches the request with `httpd_req_async_handler_begin`, registers it in a
//! bounded list and **returns**; a dedicated broadcaster task does every write.
//! The seam is [`crate::web_async`] — the second `unsafe` in the workspace, and
//! it documents each of its three calls.
//!
//! A browser's `EventSource` accepts `Transfer-Encoding: chunked`; the WHATWG
//! concern in
//! [espressif/esp-idf#14121](https://github.com/espressif/esp-idf/issues/14121)
//! is about intermediaries that buffer, not about the browser.
//!
//! # What the handlers can see
//!
//! Every handler reads one [`Telemetry`] snapshot, republished by the control
//! task into an `Arc`. The handlers therefore cannot actuate anything: a
//! `POST /api/steam` sets a *request* that the control task picks up, exactly as
//! 04 §3.2 requires ("A `POST /api/...` becomes a `Command`, never a direct call
//! into control state").
//!
//! The subsystems R3 has not ported yet — the scale, OTA, the SPA bundle —
//! answer with the C++'s **shape** and an explicit `unavailable` reason, rather
//! than a plausible-looking zero. A handler that returns `0 g` for a scale the
//! firmware cannot read is a lie that costs a support call.

// The `unsafe` in this module is the lock-free `Cell` below: one writer,
// sequence-checked readers. The reasoning is on the type and at every access; the
// workspace lint is `unsafe_code = "deny"`, so it is allowed here for the same
// reason `crate::web_async` is.
#![allow(
    unsafe_code,
    reason = "`Snapshot`: one writer, and every read and write happens inside \
              `interrupt::free`, which is mutual exclusion on this single-core \
              target. The justification is on the impl itself and is checkable \
              there rather than resting on a promise about retry loops."
)]

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write as _;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use cc_config::schema::{ParamValue, SCHEMA};
use cc_config::Config;
use cc_domain::http_auth::{self, MAX_CREDENTIAL_BYTES, WWW_AUTHENTICATE};
use esp_idf_hal::interrupt;
use esp_idf_svc::http::server::{Configuration, EspHttpConnection, EspHttpServer, Request};
use esp_idf_svc::http::Method;
use esp_idf_svc::sys::EspError;
use log::{info, warn};

use crate::heap::{free_heap, min_free_heap, HEAP_SHED_BYTES};
use crate::task::{COMMAND_ACK_POLL_MS, COMMAND_ACK_TIMEOUT_MS};
use crate::time::now_ms;

/// The C++'s HTTP port. `WebServerManager` is constructed with 80.
pub const HTTP_PORT: u16 = 80;

/// The port the C++'s telnet log stream uses. `Logger::Config::port`.
pub const TELNET_PORT: u16 = 23;

/// The largest JSON response this server will build, in bytes.
///
/// `/api/parameters?filter=all` is the biggest: 96 registered parameters, each
/// an object with a name, a type, a value, a minimum and a maximum. The C++
/// measured ~19 KB (`ADR-0002` §2). 32 KB is comfortably above that and
/// comfortably below the point at which one response could be the OOM.
pub const MAX_JSON_BYTES: usize = 32 * 1024;

/// The heap floor below which the large-response path refuses to build.
///
/// **The same number as the telnet shed's**, and deliberately: ADR-0002 set one
/// threshold for "the machine is tight", and two constants for the same judgement
/// is how they drift. Below it, `/api/parameters` and `/api/config` answer `503`
/// rather than risking the `abort()` the ADR exists to prevent — a refused
/// request is a visible error, an OOM is a reboot in the middle of a shot.
pub const HEAP_FLOOR_BYTES: u32 = HEAP_SHED_BYTES;

/// The interval between temperature events on `/events`, in milliseconds.
///
/// The C++'s `tempEventInterval_`, and it matters for the same reason the
/// publish budget does: `/events` pushes a frame per client per interval, and
/// with a browser open on a slow link the frames queue.
pub const SSE_EVENT_INTERVAL_MS: u32 = 1_000;

/// The idle keep-alive frame, in milliseconds.
///
/// A browser's `EventSource` reconnects on its own but a NAT timeout will drop a
/// silent connection first, so an idle `/events` gets a comment frame. 15 s is
/// under the common 30 s NAT floor and well above the 1 s event cadence.
pub const SSE_KEEPALIVE_MS: u32 = 15_000;

/// How often the broadcaster task looks for a due frame.
///
/// 50 ms, against a 1 s event interval and a 15 s keepalive, so a due frame is
/// at most 50 ms late. The cost is one task wakeup every 50 ms whether or not
/// anyone is streaming, which is why it is not 10 ms: there is nothing to gain
/// from resolving a due frame faster than a browser paints.
pub const SSE_POLL_MS: u32 = 50;

/// The broadcaster task's stack.
///
/// 4096 B. It formats the same JSON the httpd task used to format on its own
/// 8192 B stack ([`configuration`]), so this is not smaller in the way that
/// matters — but it is a *different* task, so the httpd task's stack no longer
/// has to accommodate streaming at all, and 4096 is what the formatting needs
/// with nothing else on the stack.
const SSE_BROADCASTER_STACK_BYTES: usize = 4096;

/// How many pushed frames may wait for the broadcaster.
///
/// Four. The producers are the control task at [`SSE_EVENT_INTERVAL_MS`] and the
/// weight event, so the mailbox is emptied roughly 20× faster than it fills; it
/// exists to carry a push across the one broadcaster pass, not to buffer. Four
/// is enough that a broadcaster pass delayed by a slow client does not start
/// dropping, and small enough that a wedged broadcaster cannot grow the heap
/// without bound on a machine with 320 KB of it.
const SSE_MAILBOX_DEPTH: usize = 4;

/// The server's URI-handler budget.
///
/// `HttpServerConfiguration::max_uri_handlers` defaults to 32 (`server.rs:132`)
/// and the C++ registers 24. Raised here to 40 because the Rust table now
/// carries every route the C++ has plus `POST /api/config/upload` and the
/// `OPTIONS` preflight, and because `ESP_ERR_HTTPD_HANDLERS_FULL` fires **at
/// boot** — which is the worst possible moment to discover a route was added.
/// The headroom is against ESP-IDF's own static allocation of
/// `max_uri_handlers * sizeof(httpd_uri_t)` (`httpd_main.c:533`), so it costs
/// `8 * 8` bytes, not per-request memory.
/// `max_open_sockets` is raised from 4 to 5 for the same reason: a browser
/// opens the SPA, an SSE stream and several API requests at once, and the C++'s
/// 4 was on `ESPAsyncWebServer`, which has a different accounting.
pub const MAX_URI_HANDLERS: usize = 40;
/// See [`MAX_URI_HANDLERS`].
pub const MAX_OPEN_SOCKETS: usize = 5;

/// The telemetry every handler reads.
///
/// A flat struct of `Copy` values, published into an `Arc` by the control task.
/// It is deliberately **not** `cc_machine`'s state or `cc_config`'s `Config`:
/// those are owned by tasks this one must not reach into, and a network task
/// holding a `Config` is how the C++'s `LoopManager` ↔ `MQTTManager` ↔
/// `WebServerManager` coupling happened (04 §3.2).
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "`Telemetry` is a *report of facts about the machine*, one field per \
              key the C++'s `/api/status` publishes -- and a dozen of those are \
              booleans because a dozen of the C++'s are. Turning them into \
              enums would make the payload builder unreadable and would not \
              make the data any more correct. See `the_cpp_keys_are_the_schema`."
)]
pub struct Telemetry {
    /// `MachineState` as its integer discriminant, as the C++ publishes it
    /// (`WebServerManager.cpp:349`).
    pub machine_state: i32,
    /// The boiler temperature in °C.
    pub temperature_c: f64,
    /// The brew setpoint in °C.
    pub setpoint_c: f64,
    /// The PID output in per cent (`pidOutput / 10`, `:352`).
    pub heater_power_pct: f64,
    /// Whether the PID is running.
    pub pid_enabled: bool,
    /// `MachineStateContext::steamON_` (`MachineStateContext.h:785`).
    ///
    /// Published because `/api/steam`'s toggle needs it: the C++'s handler reads
    /// `isSteamModeActive()` to compute `!current`, and this is the only copy of
    /// that fact the httpd task can reach.
    pub steam_mode: bool,
    /// `systemContext_->backflushMode()` — whether backflush *mode* is armed.
    ///
    /// Published for `/api/backflush`'s toggle, on the same reasoning as
    /// [`Self::steam_mode`].
    pub backflush_mode: bool,
    /// Whether a brew is running.
    ///
    /// **Not** the C++'s `steamMode` and not what this used to be published as.
    /// See [`status_json`]: `steam_mode` is that flag, this is a derived
    /// `is_brew_state()`, and the C++ publishes neither this one nor any field
    /// of this shape.
    pub brewing: bool,
    /// Whether the machine is in standby.
    pub standby: bool,
    /// Milliseconds of standby remaining.
    pub standby_remaining_ms: u32,
    /// Milliseconds since boot.
    pub uptime_ms: u32,
    /// Shots since the last backflush.
    pub shots_since_backflush: u32,
    /// `maintenance.backflush_reminder.threshold`.
    pub backflush_threshold: u32,
    /// Whether the reminder is due.
    pub backflush_due: bool,
    /// The weight in grams, if a scale is fitted and has produced a reading.
    pub weight_g: Option<f64>,
    /// The brew weight in grams.
    pub brew_weight_g: Option<f64>,
    /// The water tank float, if fitted.
    pub water_tank_full: Option<bool>,
    /// The pressure in bar, if fitted.
    pub pressure_bar: Option<f64>,
    /// The Wi-Fi signal, 0–4.
    pub signal: u8,
    /// Whether the radio is associated.
    pub wifi_associated: bool,
    /// The IPv4 address as text, or `None`.
    ///
    /// A `heapless::String<15>` -- the longest an IPv4 address in dotted-quad
    /// form can be (`255.255.255.255`) -- and **not** a `String`.
    ///
    /// This is not a style preference, and it is the field that decides whether
    /// [`Snapshot`] is safe. It used to be a `String`, which meant the snapshot
    /// payload owned heap memory; combined with the non-atomic seqlock access
    /// that made a reader's `clone()` a use-after-free (see [`Snapshot`]'s docs).
    /// Fixed-size means the reader's copy is a memcpy of at most 16 bytes with
    /// no allocation, no `free`, and no allocator traffic on the httpd task.
    pub ip: Option<heapless::String<15>>,
    /// Whether MQTT is configured at all.
    pub mqtt_configured: bool,
    /// Whether MQTT has a session.
    pub mqtt_connected: bool,
    /// Whether the machine has given up on Wi-Fi.
    pub wifi_offline: bool,
}

/// A request the handlers can ask the control task to carry out.
///
/// The whole of the network→control surface for now. `Copy`, bounded, and
/// `hal::task::queue::Queue`-compatible by construction (04 §3.2: *"No
/// `String`, no `Vec`, no `Box` in a cross-task message"*).
///
/// **`POST /api/parameters` is the one command endpoint that is not in here**, and
/// the reason is that rule: a parameter write is a *list* of pairs, and a `Copy`
/// enum cannot carry a list. It travels in
/// [`crate::task::ParameterHandoff`] instead, which is drained at the same point
/// in the tick. The write itself is not a second path —
/// [`cc_config::assign::apply`] is the only writer of a parameter, and R3-13's
/// inbound MQTT calls it directly because MQTT already runs on the control task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// `POST /api/setpoint?value=<celsius>`.
    SetSetpoint(i32),
    /// `POST /api/steam?on=0|1` — the **explicit** form.
    SetSteam(bool),
    /// `POST /api/steam` with no field — the C++'s toggle.
    ///
    /// The C++'s `/api/steam` reads no parameter at all; it computes
    /// `!isSteamModeActive()` from the **live** `MachineStateContext`
    /// (`WebServerManager.cpp:444-445`). So the decision needs the machine
    /// state, which only the control task has, and the web layer cannot make it
    /// without racing a snapshot that may be a tick stale. This variant carries
    /// no value and the control task resolves it against the machine it owns —
    /// which is the faithful translation, and the reason the toggle is a
    /// separate variant rather than a flag on [`Self::SetSteam`].
    ToggleSteam,
    /// `POST /api/pid?on=0|1` — the explicit form.
    SetPid(bool),
    /// `POST /api/pid` with no field — the C++'s toggle.
    ///
    /// `!Config::getInstance().pidEnabled.get()` (`WebServerManager.cpp:466`),
    /// which the control task both holds and writes. See [`Self::ToggleSteam`]
    /// for why the resolution happens there.
    TogglePid,
    /// `POST /api/backflush?on=0|1` — the explicit form.
    SetBackflush(bool),
    /// `POST /api/backflush` with no field — the C++'s toggle.
    ///
    /// `!systemContext_->backflushMode()` (`WebServerManager.cpp:490`). See
    /// [`Self::ToggleSteam`].
    ToggleBackflush,
    /// `POST /api/backflush?value=start` — begin a backflush cycle.
    ///
    /// **Not a C++ route.** The C++'s `/api/backflush` only toggles backflush
    /// *mode* (`WebServerManager.cpp:490`); starting a cycle is a switch press
    /// or an MQTT command. This verb exists because the previous
    /// `register_command` accepted `start` and something may already send it,
    /// and removing a reachable command would be a regression. The bare POST
    /// does **not** mean this — it means toggle, as in the C++.
    StartBackflush,
    /// `POST /api/sleep`.
    Sleep,
    /// `POST /api/wake`.
    Wake,
    /// `POST /api/scale/tare`.
    Tare,
    /// `POST /api/scale/calibration`.
    Calibrate,
    /// `POST /api/maintenance/reset-backflush-counter`.
    ResetBackflushCounter,
    /// `POST /api/wifi-reset` — forget the stored credentials and reboot.
    WifiReset,
    /// `POST /api/factory-reset` — erase the configuration and reboot.
    FactoryReset,
    /// `POST /api/restart` — reboot, keeping the configuration.
    Restart,
}

/// A single-writer value that many tasks may read, **with no lock and no
/// `unsafe` in the access path**.
///
/// # Why this exists
///
/// `Shared::telemetry` was a `std::sync::Mutex` — a `pthread` mutex, so a
/// `FreeRTOS` one — taken **twice per 10 ms control tick** and read by the httpd
/// and display tasks. On this build every cross-task *blocking* primitive
/// asserts the kernel (`09-cpp-findings.md` §28), so the design was
/// contradicting its own stated rule.
///
/// # 🔴 Why this is NOT a seqlock
///
/// The obvious lock-free answer is a seqlock: stamp a sequence odd, write,
/// stamp it even, and have readers retry. That is what this used to be, and it
/// was **unsound**, not merely racy.
///
/// A seqlock gives you *atomicity of observation*, not of access. The reader
/// still dereferences a non-atomic location while the writer may be
/// overwriting it; the sequence check afterwards only decides whether to
/// *keep* the value it already read. Under Rust's memory model that
/// concurrent non-atomic read/write is undefined behaviour whether or not the
/// result is discarded — and with a heap-owning payload it is not academic:
///
/// * [`Telemetry::ip`] was a `String`, reassigned every 10 ms tick by
///   `network::publish_radio` (`network.rs:449`). Each reassignment
///   **allocates and then drops the previous `String`**, i.e. `free()`s the
///   buffer the httpd task may be `memcpy`-ing from inside its `clone()`. A
///   torn read yields a `{ptr, len, cap}` triple from two different
///   generations — a live use-after-free at 100 Hz, reachable from any HTTP
///   request, on the task that also serves the UI.
///
/// Making the payload `Copy` would have downgraded that to "garbage numbers",
/// which is better and still UB.
///
/// # What replaces it
///
/// **Mutual exclusion by interrupt masking**, which is what `FreeRTOS` itself
/// provides for exactly this shape of problem. Every read and every write goes
/// through [`esp_idf_hal::interrupt::free`], which is
/// `portENTER_CRITICAL`/`portEXIT_CRITICAL`:
///
/// * It is not a *blocking* primitive. Nothing is enqueued on a semaphore's
///   event list, so the `xTaskRemoveFromEventList` assert is unreachable.
/// * The section is one struct copy — a few dozen bytes — so it is over in
///   about a microsecond, against a 10 ms control period.
/// * A control loop therefore never *waits* on a reader. It waits at most for
///   the interrupt latency of a memcpy.
///
/// The result is that the data race is gone **by construction** rather than by
/// detection, and the only `unsafe` left in the whole type is the one
/// `Sync`/`Send` impl below — whose justification is now checkable at the call
/// site instead of resting on a promise about retry loops.
///
/// Note the contrast with [`crate::task`]'s `std::sync::Mutex` over the
/// history ring and the parameter blob: those are the *blocking* primitives,
/// they are off the control loop's critical path, and they are untouched.
pub struct Snapshot<T> {
    // `core::cell::Cell`, deliberately: it is `!Sync`, which is what makes the
    // hand-written `Sync` below an honest statement of the invariant rather
    // than a consequence of the field type.
    value: core::cell::Cell<T>,
}

// SAFETY: the *only* ways to reach `value` are `set` and `get`, and both run
// their whole read-modify-write inside `interrupt::free`, i.e. inside
// `portENTER_CRITICAL`/`portEXIT_CRITICAL`. On this single-core target that is
// mutual exclusion between the control task, the httpd task and the display
// task, and it also excludes the ISR, so no access can overlap any other.
//
// The two rules that make that argument hold, both enforced by construction
// rather than by discipline:
//
// 1. `value` is private to this module and the only `unsafe` code in this crate
//    cannot name it — `Cell::as_ptr` would hand out a raw pointer, and nothing
//    calls it. Grep for `as_ptr` to confirm; if that grep ever finds a hit,
//    this `Sync` is void.
//
// 2. `set` and `get` do not call out to anything that can block. `get` clones
//    `T`; with `T = Telemetry` that is a fixed-size copy, because `ip` is a
//    `heapless::String<15>` and not a `String`. `T: Send` bounds the transfer.
//
// `T: Send` is the right bound rather than `T: Sync`: the value is moved between
// tasks, and a reader only ever *copies* it inside the critical section.
unsafe impl<T: Send> Sync for Snapshot<T> {}
// SAFETY: `Snapshot<T>` is just a `core::cell::Cell<T>` with no thread-affine
// state, and its `Sync` impl (above) already establishes that concurrent access
// is mutually excluded. Moving one between tasks moves a value, nothing else.
unsafe impl<T: Send> Send for Snapshot<T> {}

impl<T: Clone + Default> Snapshot<T> {
    /// An empty snapshot, readable as `T::default()` until it is written.
    #[must_use]
    pub fn new() -> Self {
        Self {
            value: core::cell::Cell::new(T::default()),
        }
    }

    /// Replace the value. **One writer** — the control task.
    pub fn set(&self, value: T) {
        // `Cell::set` takes `&self`, so "one writer" is a discipline rather
        // than a type-level fact. It is honoured: `Shared::publish` and
        // `network::publish_radio` are both called from the control task, and
        // nothing else calls `set`. The mutual exclusion that makes a second
        // writer *safe* rather than merely racy is the critical section below.
        interrupt::free(|| self.value.set(value));
    }

    /// The current value.
    ///
    /// This is infallible where the seqlock's `get` returned `Option<T>`: there
    /// is no "write in flight" state to fail to observe, because a reader
    // either gets the critical section before the writer or after it.
    /// Every caller therefore loses its `unwrap_or_default()`.
    #[must_use]
    pub fn get(&self) -> T {
        interrupt::free(|| self.value.take())
    }
}

impl<T: Clone + Default> Default for Snapshot<T> {
    /// An empty snapshot -- the same value [`Snapshot::new`] produces, and what
    /// a `#[derive(Default)]` on a containing struct would reach for.
    fn default() -> Self {
        Self::new()
    }
}

/// The shared state the HTTP handlers close over.
pub struct Shared {
    /// The latest telemetry, republished by the control task.
    pub telemetry: Snapshot<Telemetry>,
    /// The temperature history, appended by the control task.
    ///
    /// The C++'s `static TemperatureHistory tempHistory` is a file-static in
    /// the web server, written from `sendTempEvent` (`WebServerManager.cpp:1134`).
    /// Here the producer is the control task, so the ring is shared state and
    /// lives here — next to the telemetry, which the same task publishes for the
    /// same reason.
    ///
    /// **Boxed, and that is not a style choice.** The ring is 600 points, 7.2 KB;
    /// a `Shared` built *by value* — which two on-target tests do — put that on
    /// an 8 KB task stack and the device rebooted mid-suite, which the runner
    /// reports as LOST rather than as a failure. It is the same lesson as the
    /// 1 KB `Display` framebuffer and the 7.2 KB `history_json` return value:
    /// anything this size is heap or it is a crash.
    pub history: Mutex<alloc::boxed::Box<cc_domain::history::History>>,
    /// How many control commands the control task has applied.
    ///
    /// The ack every command caller waits on. A command is a **request**: the
    /// handler hands it to the queue and returns, so anything it then says about
    /// the machine describes the machine as it was. The control task increments
    /// this once per command it drains, so `applied() == before` means "mine is
    /// still queued".
    applied: AtomicU32,
    /// Set when a handler has asked for a reboot; the firmware acts on it.
    pub reboot_requested: AtomicBool,
    /// The number of `/api/parameters?filter=all` responses served, for the
    /// ADR-0002 regression check.
    pub large_responses: AtomicU32,
    /// The number refused because the heap was below the floor.
    pub large_refused: AtomicU32,
}

impl Shared {
    /// A `Shared` with default telemetry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            telemetry: Snapshot::new(),
            history: Mutex::new(alloc::boxed::Box::new(cc_domain::history::History::new())),
            applied: AtomicU32::new(0),
            reboot_requested: AtomicBool::new(false),
            large_responses: AtomicU32::new(0),
            large_refused: AtomicU32::new(0),
        }
    }

    /// Append one sample to the history ring, ignoring a poisoned lock.
    ///
    /// Called by the control task at the C++'s `sendTempEvent` cadence. A
    /// poisoned lock means some other thread panicked while holding it, and the
    /// only honest response to "the chart is a diagnostic" is to lose a point
    /// rather than refuse to regulate a boiler.
    pub fn push_history(&self, current_temp: f32, target_temp: f32, heater_power: f32) {
        if let Ok(mut ring) = self.history.lock() {
            ring.push(current_temp, target_temp, heater_power);
        }
    }

    /// A snapshot of the history ring, oldest first.
    ///
    /// A copy rather than a lock held across the serialisation: the copy is a
    /// 7 KB `memcpy` on the httpd task, and holding the lock while writing
    /// 12 KB to a socket would stall the control task's next sample. The C++
    /// has the same shape of problem and solves it by never sharing the ring
    /// Replace the telemetry snapshot.
    ///
    /// **A whole-slot replace, not a merge.** There are two publishers — the
    /// control task, through [`Telemetry`]'s machine fields, and the radio,
    /// through the four fields [`network::publish_radio`] owns — so whoever calls
    /// this must leave the other's fields alone. In practice that means the
    /// control task publishes first and the radio second in the same tick; see
    /// `publish_radio` for why the order matters.
    pub fn publish(&self, telemetry: Telemetry) {
        self.telemetry.set(telemetry);
    }

    /// A copy of the current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Telemetry {
        // Falls back to the default rather than blocking. A reader that could not
        // get a consistent snapshot in four attempts gets zeroes for one poll,
        // which `/api/status` already reports as "no reading yet" — and blocking
        // here would put a `pthread` mutex on the httpd task's hottest path,
        // which is the hazard `Cell` exists to remove.
        self.telemetry.get()
    }

    /// How many large responses have been served.
    ///
    /// The number ADR-0002's consequences section asks for: a regression that
    /// brought the double-copy back would show up as this falling while the
    /// heap minimum rises.
    #[must_use]
    pub fn large_responses(&self) -> u32 {
        self.large_responses.load(Ordering::Relaxed)
    }

    /// How many large responses were refused for want of heap.
    #[must_use]
    pub fn large_refused(&self) -> u32 {
        self.large_refused.load(Ordering::Relaxed)
    }

    /// Note that the control task has applied one more command.
    ///
    /// Called by the control task, once per command drained.
    pub fn note_applied(&self) {
        self.applied.fetch_add(1, Ordering::Release);
    }

    /// How many commands the control task has applied.
    #[must_use]
    pub fn applied(&self) -> u32 {
        self.applied.load(Ordering::Acquire)
    }

    /// Block until [`Self::applied`] passes `before`, or
    /// [`COMMAND_ACK_TIMEOUT_MS`] elapses. Returns whether it advanced.
    ///
    /// The C++ has no equivalent: its web handler *is* the control task, so a
    /// `POST /api/pid` mutates the machine before it answers. Since the split
    /// that put the panel and the control loop in different tasks, a handler can
    /// only ask — so it asks, and waits a bounded time. Bounded because a
    /// stalled control task must fail visibly rather than hang a browser.
    pub fn wait_applied(&self, before: u32) -> bool {
        let deadline = now_ms().wrapping_add(COMMAND_ACK_TIMEOUT_MS);
        while self.applied() == before {
            if now_ms().wrapping_sub(deadline) < COMMAND_ACK_POLL_MS {
                return false;
            }
            crate::task::delay_ms(COMMAND_ACK_POLL_MS);
        }
        true
    }

    /// Ask for a reboot at the next opportunity.
    ///
    /// `POST /api/restart`, `/api/factory-reset` and `/api/wifi-reset` all land
    /// here, and none of them performs the reboot itself: a handler must not be
    /// able to reset the machine from inside an HTTP request, and
    /// [`Shared::take_reboot_request`] is read by the main task between ticks.
    pub fn set_reboot_requested(&self) {
        self.reboot_requested.store(true, Ordering::SeqCst);
    }

    /// Whether a reboot has been asked for, and clear the request.
    pub fn take_reboot_request(&self) -> bool {
        self.reboot_requested.swap(false, Ordering::SeqCst)
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

/// How the SSE stream frames are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SseMode {
    /// `httpd_resp_send_chunk`, via the detached request in
    /// [`crate::web_async::AsyncReq`].
    ///
    /// The default, and the only mode that can work: a chunked write needs a
    /// `httpd_req_t*`, and holding one across the handler's return is only
    /// sound once `httpd_req_async_handler_begin` has copied it out. It is also
    /// the mode that respects the socket's `send_wait_timeout`.
    #[default]
    Chunked,
}

/// The most `/events` clients that will be served at once.
///
/// **The C++ has no such limit** — `AsyncEventSource` grows a linked list and
/// the browser tabs all get a stream. This firmware does, and the reason is
/// [`MAX_OPEN_SOCKETS`]: ESP-IDF's httpd has a fixed session pool, an SSE client
/// occupies one session for as long as it is connected, and a browser opens
/// several connections of its own for the SPA. Two leaves three of five for
/// `/api/*`, which is the number the ADR-0002 reproducer (6–10 parallel
/// requests) needs to be served rather than queued behind a stream.
///
/// The cost of the cap is stated rather than hidden: a third tab gets a `503`
/// and an error in the browser console instead of a working stream. The
/// alternative — letting the session pool fill — is the failure this whole
/// change exists to remove, in the other direction.
pub const SSE_MAX_CLIENTS: usize = 2;

/// One connected `/events` client.
///
/// Owns the request handed over by `httpd_req_async_handler_begin`, plus the
/// two deadlines that decide what it gets next. Everything is per-client because
/// a browser that connects late must not inherit another's keepalive schedule —
/// and, more importantly, because a client whose write fails must be removable
/// without disturbing the others.
struct SseClient {
    req: crate::web_async::AsyncReq,
    /// Whether the C++'s `hello` is still owed to this client.
    ///
    /// A browser's `EventSource` dispatches nothing until it sees a frame, so
    /// without it the connection looks dead to the UI until the first event
    /// arrives. `WebServerManager.cpp:308-317` sends it from `onConnect`.
    await_hello: bool,
    /// When this client last got a frame of any kind. The keepalive deadline is
    /// measured from here so a busy stream does not also emit keepalives.
    last_write_ms: u32,
}

/// The `/events` stream's broadcast state.
///
/// The C++'s `AsyncEventSource` keeps a client list and every producer pushes to
/// all of them (`WebServerManager.cpp:1163`). This is now a real list, bounded by
/// [`SSE_MAX_CLIENTS`], and it lives in an `Arc` shared between the httpd task
/// (which adds and removes clients) and the broadcaster task (which writes).
pub struct Sse {
    mode: SseMode,
    clients: Mutex<Vec<SseClient>>,
    mailbox: Mutex<VecDeque<String>>,
    connected: AtomicU32,
    rejected: AtomicU32,
    pushed: AtomicU32,
    sent: AtomicU32,
    dropped: AtomicU32,
    dropped_frames: AtomicU32,
}

impl Sse {
    /// An empty stream in the given framing mode.
    #[must_use]
    pub fn new(mode: SseMode) -> Self {
        Self {
            mode,
            clients: Mutex::new(Vec::new()),
            mailbox: Mutex::new(VecDeque::new()),
            connected: AtomicU32::new(0),
            rejected: AtomicU32::new(0),
            pushed: AtomicU32::new(0),
            sent: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
            dropped_frames: AtomicU32::new(0),
        }
    }

    /// The framing mode.
    #[must_use]
    pub const fn mode(&self) -> SseMode {
        self.mode
    }

    /// How many clients have connected since boot.
    ///
    /// Counts *connections*, not concurrent clients, so it keeps rising after
    /// a tab is closed. [`Sse::connected_now`] is the concurrent figure.
    #[must_use]
    pub fn clients(&self) -> u32 {
        self.connected.load(Ordering::SeqCst)
    }

    /// How many clients are connected right now.
    #[must_use]
    pub fn connected_now(&self) -> usize {
        self.clients.lock().map_or(0, |c| c.len())
    }

    /// How many connections were refused for want of a free client slot.
    #[must_use]
    pub fn rejected(&self) -> u32 {
        self.rejected.load(Ordering::SeqCst)
    }

    /// How many event frames have been written.
    #[must_use]
    pub fn sent(&self) -> u32 {
        self.sent.load(Ordering::SeqCst)
    }

    /// How many frames were dropped because the client had gone away.
    ///
    /// A `write` to a closed socket is not an error the C++ surfaces either;
    /// counting it is what makes "the stream delivered events for ten minutes
    /// with zero drops" a measurable claim rather than an assertion.
    #[must_use]
    pub fn dropped(&self) -> u32 {
        self.dropped.load(Ordering::SeqCst)
    }

    /// How many pushes have been offered to the stream.
    #[must_use]
    pub fn pushed(&self) -> u32 {
        self.pushed.load(Ordering::SeqCst)
    }

    /// How many pushed frames were discarded because the mailbox was full.
    ///
    /// Separate from [`Sse::dropped`] because they are different failures: a
    /// `dropped` frame reached a client that had gone, a `dropped_frames` frame
    /// never left the machine at all.
    #[must_use]
    pub fn dropped_frames(&self) -> u32 {
        self.dropped_frames.load(Ordering::SeqCst)
    }

    /// One SSE frame: `event: <name>\ndata: <payload>\n\n`.
    #[must_use]
    pub fn frame(event: &str, payload: &str) -> String {
        // The blank line is required by the WHATWG spec and is the single most
        // common way a hand-rolled SSE endpoint delivers nothing at all: the
        // browser buffers the frame and never dispatches it.
        format!("event: {event}\ndata: {payload}\n\n")
    }

    /// An SSE comment frame, which carries no event and keeps the socket warm.
    #[must_use]
    pub fn keepalive() -> String {
        format!(": {}\n\n", now_ms())
    }

    /// Add a client, or hand the request back because the stream is full.
    ///
    /// `Result<(), AsyncReq>` rather than `bool` because the request **must** be
    /// completed on every path: `httpd_req_async_handler_begin` has already set
    /// `sd->for_async_req` (`httpd_txrx.c:700`), and a request left incomplete
    /// keeps the socket out of the httpd task's `select()` for ever
    /// (`esp_http_server.h:864-866`). Returning it lets the caller release it;
    /// returning `false` and dropping it would leak the socket — which is
    /// exactly how a second refused client would cost the whole API.
    pub(crate) fn attach(
        &self,
        req: crate::web_async::AsyncReq,
    ) -> Result<(), crate::web_async::AsyncReq> {
        let now = now_ms();
        // A poisoned lock means some other client write panicked. Refusing is
        // the only safe answer: the list's length is the bound that keeps the
        // httpd task's socket budget intact.
        let Ok(mut clients) = self.clients.lock() else {
            return Err(req);
        };
        if clients.len() >= SSE_MAX_CLIENTS {
            return Err(req);
        }
        clients.push(SseClient {
            req,
            await_hello: true,
            last_write_ms: now,
        });
        Ok(())
    }
}

/// Write a JSON response, streamed.
///
/// **This is ADR-0002 decision 2.** `httpd_resp_send_chunk` is called once per
/// chunk, so the payload is serialised into one `Vec<u8>` and handed to the
/// socket in pieces; there is no intermediate `String`, and no second copy
/// inside the web server. The C++'s `request->send(200, "application/json",
/// response)` with a `String` response was the other half of the OOM.
fn respond(conn: &mut EspHttpConnection<'_>, status: u16, json: &str) -> Result<(), EspError> {
    conn.initiate_response(status, Some("OK"), &[("Content-Type", "application/json")])?;
    // 512-byte chunks: small enough that the send path's buffer is not the
    // largest allocation on the heap, large enough that a 19 KB response is 38
    // calls rather than 1900.
    for chunk in json.as_bytes().chunks(512) {
        let _ = conn.write(chunk);
    }
    // A zero-length chunk terminates the chunked body; without it the browser
    // waits for more data that never comes.
    let _ = conn.write(&[]);
    Ok(())
}

/// The `401` a request that failed [`Auth`] gets.
///
/// `WWW-Authenticate: Basic realm="CleverCoffee"` is the header that makes a
/// browser show its credential prompt, and it is the C++'s realm verbatim
/// (`WebServerManager.cpp:286`, via `AsyncAuthenticationMiddleware`).
///
/// The body is the C++'s shape rather than [`error_body`]'s, because a browser
/// shows the challenge and *not* the body: this text is what an operator
/// reading a `curl` transcript sees, and "authentication required" is a better
/// answer to that than `{"error":"Unauthorized"}`.
fn unauthorized(conn: &mut EspHttpConnection<'_>) -> Result<(), EspError> {
    conn.initiate_response(
        401,
        Some("Unauthorized"),
        &[
            ("Content-Type", "application/json"),
            // The header that makes a browser raise its credential prompt, and
            // the C++'s realm verbatim (`WebServerManager.cpp:286`). A different
            // realm is a different protection space, and a browser holding
            // credentials for one will not send them for another.
            ("WWW-Authenticate", WWW_AUTHENTICATE),
        ],
    )?;
    // The same chunked idiom as [`respond`], which is what `fn_handler`'s
    // wrapper expects of every response on this server.
    let _ = conn.write_all(b"{\"error\":\"authentication required\"}");
    let _ = conn.write(&[]);
    Ok(())
}

/// The HTTP Basic credential check for every route on this server.
///
/// # Why this holds the `Arc<Config>` and not a copy of the credential
///
/// `Web::start` is handed the boot-time `Arc<Config>` and the server outlives
/// the call, so the password is read through [`Secret::expose`] per request and
/// **no second copy of it exists in the process**. Cloning it into an owned
/// field would have been one line and one more plaintext credential lying in
/// the heap.
///
/// # What `enforced` means, and why it is decided once
///
/// The C++ installs the middleware in `setupMiddleware`
/// (`WebServerManager.cpp:272-296`), which runs from `WebServerManager::initialize`
/// — once, at boot. So `system.auth.enabled` protects nothing until the next
/// reboot in the C++ too, and the same is true here. `needs_reboot` reports it
/// in the `POST /api/parameters` answer so the operator is told rather than
/// guessing, which the C++ cannot do.
///
/// The C++'s other half is reproduced exactly: **empty credentials mean no
/// authentication at all**, not a locked door. `WebServerManager.cpp:290-294`
/// logs "Web authentication enabled but credentials not set" and serves the API
/// open. Failing closed instead would mean that enabling auth and then not
/// finishing locks an operator out of a machine whose only other console is a
/// UART. The warning is reproduced too, and is deliberately loud.
#[derive(Clone)]
pub struct Auth {
    config: Arc<Config>,
    enforced: bool,
}

impl core::fmt::Debug for Auth {
    /// Never the configuration.
    ///
    /// A derived `Debug` would print every parameter including the four
    /// credentials; `Secret` redacts its own field but the *rest* of the config
    /// is not what a log line about authentication wants.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Auth")
            .field("enforced", &self.enforced)
            .finish_non_exhaustive()
    }
}

impl Auth {
    /// Decide, once, whether this server challenges — as the C++'s
    /// `setupMiddleware` does.
    #[must_use]
    pub fn from_config(config: &Arc<Config>) -> Self {
        let wanted = config.system.auth.enabled;
        let has_credentials = !config.system.auth.username.is_empty()
            && !config.system.auth.password.expose().is_empty();
        if wanted && !has_credentials {
            // The C++'s exact condition, and its exact consequence.
            warn!(
                "http: web authentication is enabled but no credentials are set; \
                 serving the API unauthenticated, exactly as the C++ does"
            );
        }
        Self {
            config: Arc::clone(config),
            enforced: wanted && has_credentials,
        }
    }

    /// Whether requests are challenged at all.
    #[must_use]
    pub fn is_enforced(&self) -> bool {
        self.enforced
    }

    /// Whether this request may proceed.
    ///
    /// `header` is the request's `Authorization` header, or `None`. **It is
    /// never logged and never formatted**, on any path: a rejected request is
    /// counted and answered, nothing more.
    #[must_use]
    pub fn admits(&self, header: Option<&str>) -> bool {
        if !self.enforced {
            return true;
        }
        let mut scratch = [0u8; MAX_CREDENTIAL_BYTES];
        http_auth::authorized(
            header,
            &self.config.system.auth.username,
            self.config.system.auth.password.expose(),
            &mut scratch,
        )
    }
}

/// Register `handler` on `uri`, behind [`Auth`].
///
/// **Every data route on this server goes through here, and that is the point.**
/// The alternative — a credential check at the top of each of two dozen
/// closures — is a control whose strength is the number of places nobody
/// forgets, and a forgotten one is an unauthenticated `/api/restart`. Funnelling
/// registration through one function makes "this route is not protected" a thing
/// a reader can see at the call site rather than deduce.
///
/// The two deliberate exceptions are both visible where they are made:
/// [`register_preflight`] for `OPTIONS`, and `crate::web_async::register_raw_sse`
/// for `/events`, which is an `extern "C"` handler ESP-IDF dispatches itself.
fn register<F>(
    server: &mut EspHttpServer<'static>,
    auth: &Arc<Auth>,
    uri: &str,
    method: Method,
    handler: F,
) -> Result<(), EspError>
where
    F: for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> Result<(), EspError> + Send + 'static,
{
    let auth = Arc::clone(auth);
    server
        .fn_handler::<EspError, _>(uri, method, move |mut req| {
            // `req.header` borrows, so the verdict is taken before
            // `req.connection` asks for `&mut`. Nothing derived from the
            // header outlives this statement.
            let admitted = auth.admits(req.header("Authorization"));
            if !admitted {
                return unauthorized(req.connection());
            }
            handler(req)
        })
        .map(|_| ())
}

/// Register the CORS preflight answer, and **not** behind [`Auth`].
///
/// The C++ adds `AsyncCorsMiddleware` with `setOrigin("*")`,
/// `setMethods("GET,POST,PUT,DELETE,OPTIONS")` and
/// `setHeaders("Content-Type,Authorization,X-Requested-With")`
/// (`WebServerManager.cpp:272-277`). A preflight is the one request a browser
/// sends **without** credentials — that is what makes it a preflight — so
/// challenging one makes every cross-origin request fail, permanently and
/// confusingly. It is registered through `fn_handler` and not [`register`]
/// deliberately, and this is the one place in the file where that is true.
///
/// `/api*` rather than one URI per route: ESP-IDF matches URI and method
/// independently (`httpd_uri.c:97-122` — a URI match with the wrong method sets
/// 405 and the search *continues*), so a single wildcard answers every API
/// preflight while leaving every `GET`/`POST` to its own exact handler. This is
/// also what `routes()` advertises, which is the point: the entry used to be
/// `("/api/status", Method::Options)` with no handler behind it at all.
///
/// **The C++'s per-response `Access-Control-Allow-Origin: *` is not ported**, and
/// the omission is deliberate: the SPA is served same-origin from `/ui` and
/// needs nothing, and a wildcard origin on an endpoint that can reboot a boiler
/// or change its safety cut-offs is a widening with no consumer. Recorded in
/// `docs/rust-migration/intentional-diffs.md`.
fn register_preflight(server: &mut EspHttpServer<'static>) -> Result<(), EspError> {
    server
        .fn_handler::<EspError, _>("/api*", Method::Options, |mut req| {
            let conn = req.connection();
            conn.initiate_response(
                204,
                Some("No Content"),
                &[
                    ("Access-Control-Allow-Origin", "*"),
                    (
                        "Access-Control-Allow-Methods",
                        "GET,POST,PUT,DELETE,OPTIONS",
                    ),
                    (
                        "Access-Control-Allow-Headers",
                        "Content-Type,Authorization,X-Requested-With",
                    ),
                    ("Content-Length", "0"),
                ],
            )?;
            let _ = conn.write(&[]);
            Ok(())
        })
        .map(|_| ())
}

/// Write a large JSON response, or refuse.
///
/// The [`MAX_JSON_BYTES`] / [`HEAP_FLOOR_BYTES`] guard, which is ADR-0002's
/// actual lesson: a large response is affordable *until* it is not, and the
/// moment it is not is exactly when a second browser tab or a 5 s MQTT pass is
/// also running.
fn respond_large(
    conn: &mut EspHttpConnection<'_>,
    shared: &Shared,
    json: &str,
) -> Result<(), EspError> {
    if json.len() > MAX_JSON_BYTES {
        shared.large_refused.fetch_add(1, Ordering::Relaxed);
        return respond(
            conn,
            503,
            "{\"error\":\"response too large\",\"limit\":32768}",
        );
    }
    if free_heap() < HEAP_FLOOR_BYTES {
        shared.large_refused.fetch_add(1, Ordering::Relaxed);
        warn!(
            "http: refusing a {} B response with {} B of heap free (floor {HEAP_FLOOR_BYTES} B)",
            json.len(),
            free_heap()
        );
        return respond(
            conn,
            503,
            "{\"error\":\"insufficient memory\",\"retry\":true}",
        );
    }
    shared.large_responses.fetch_add(1, Ordering::Relaxed);
    respond(conn, 200, json)
}

/// [`respond_large`], plus the `Content-Disposition` that makes the browser save
/// the file instead of rendering it.
///
/// `WebServerManager.cpp:717`. `respond_large` is reused rather than
/// reimplemented so the ADR-0002 heap floor and the 32 KB ceiling apply to this
/// route too — the download serialises the same document `/api/config` does, so
/// it is the same size and has the same failure modes.
fn respond_download(
    conn: &mut EspHttpConnection<'_>,
    shared: &Shared,
    json: &str,
) -> Result<(), EspError> {
    if json.len() > MAX_JSON_BYTES {
        shared.large_refused.fetch_add(1, Ordering::Relaxed);
        return respond(
            conn,
            503,
            "{\"error\":\"response too large\",\"limit\":32768}",
        );
    }
    if free_heap() < HEAP_FLOOR_BYTES {
        shared.large_refused.fetch_add(1, Ordering::Relaxed);
        warn!(
            "http: refusing a {} B config download with {} B of heap free (floor {HEAP_FLOOR_BYTES} B)",
            json.len(),
            free_heap()
        );
        return respond(
            conn,
            503,
            "{\"error\":\"insufficient memory\",\"retry\":true}",
        );
    }
    shared.large_responses.fetch_add(1, Ordering::Relaxed);
    conn.initiate_response(
        200,
        Some("OK"),
        &[
            ("Content-Type", "application/json"),
            (
                "Content-Disposition",
                "attachment; filename=\"config.json\"",
            ),
        ],
    )?;
    for chunk in json.as_bytes().chunks(UI_CHUNK_BYTES) {
        let _ = conn.write(chunk);
    }
    let _ = conn.write(&[]);
    Ok(())
}

/// `GET /api/ota/status` — the C++'s `handleStatus` (`ota.cpp:726-756`), with
/// every field present and the update permanently idle.
///
/// The UI parses this with `OtaStatusSchema` (`schemas.ts:59-72`), which
/// **requires** `status`, `progress` and `updateInProgress`; omitting them makes
/// `pollOtaStatus` return `null` and the OTA page cannot render at all
/// (`OTAUpdateSection.tsx:56-62`). So the shape is the C++'s, and the honest
/// content is: nothing is updating, nothing ever will from this build, and here
/// is the task that owns it.
///
/// `updating`, `updateInProgress`, `type`, `uploadedSize`, `totalSize` and
/// `filesystemPartition` are the C++'s remaining keys (`ota.cpp:735-742`) and
/// are reported at their idle values so a client reading the C++'s full shape
/// gets zeros rather than `undefined`.
///
/// `error` is deliberately **absent**: the C++ only sets it when an update has
/// failed (`ota.cpp:744-751`), and "OTA was never built" is not an update error.
/// `message` carries that instead.
#[must_use]
pub fn ota_status_json() -> String {
    // `Status::Idle` as the enum's integer discriminant, the way the C++
    // serialises it (`doc["status"] = state.getUpdateStatus()`, an enum written
    // through ArduinoJson's integer encoding, `ota.cpp:738`).
    //
    // The UI reads `status` as a *string* — `z.enum([...])` in
    // `OtaStatusSchema` — so the integer would fail validation and the page
    // would go blank. The string is what the UI actually parses, so that is what
    // this sends; the divergence from the C++'s wire type is recorded in
    // intentional-diffs.
    String::from(
        "{\"success\":true,\"updating\":false,\"updateInProgress\":false,\"progress\":0,\
\"status\":\"idle\",\"type\":\"none\",\"uploadedSize\":0,\"totalSize\":0,\
\"filesystemPartition\":\"spiffs\",\
\"message\":\"OTA is not available in this build\",\"reason\":\"R3-15\"}",
    )
}

/// The C++'s error body, verbatim. `ApiResponses::errorResponse`.
#[must_use]
pub fn error_body(message: &str) -> String {
    format!("{{\"error\":\"{message}\"}}")
}

/// `POST /api/config/upload` — the C++'s answer, verbatim.
///
/// `sendConfigUploadResponse` (`WebServerManager.cpp:50-62`) writes
/// `{"success":…,"message":…,"restart":…}`.
///
/// `restart` is `success`, i.e. **the device does not reboot itself**. The C++
/// sets the flag and returns; the operator's browser sees it and calls
/// `POST /api/restart` two seconds later
/// (`ui/packages/frontend/src/pages/SystemPage.tsx:220,251`). Rebooting from
/// inside the handler would tear the socket down under the response the client
/// is still reading.
///
/// The C++ adds `Connection: close` (`:60`). It is **not** reproduced, and the
/// reason is that the hazard it guards against is already handled one layer
/// down: ESP-IDF's `httpd_req_delete` "finish[es] off reading any
/// pending/leftover data", draining and discarding whatever body a handler left
/// unread (`httpd_parse.c:841-855`). So the oversized upload — the one path that
/// deliberately stops reading — cannot have its tail parsed as the next request
/// on the socket. Setting the header through `httpd_resp_set_hdr` would only
/// append a second `Connection` header to a response ESP-IDF manages itself.
#[must_use]
pub fn upload_response(success: bool, message: &str) -> String {
    format!("{{\"success\":{success},\"message\":\"{message}\",\"restart\":{success}}}")
}

/// `/api/status` — the C++'s `WebServerManager.cpp:327-372`.
///
/// Field for field. The C++ adds `weight`/`brewWeight` only when the scale is
/// enabled; here they are `null` when there is no reading, which is a smaller
/// change than gating them and a smaller one than reporting a fabricated 0 g.
///
/// # `steamMode` is the latched flag, and `brewing` is a different fact
///
/// The C++ writes `doc["steamMode"] = systemContext_->steamMode()`
/// (`WebServerManager.cpp:359`), which is `SystemContext::steamMode()` →
/// `MachineStateContext::isSteamModeActive()` → `steamON_`
/// (`MachineStateContext.h:395`, `:785`). That flag is **latched**: it is set
/// `true` in `SteamRunningState::onEntryImpl` (`SteamStates.cpp:16`), cleared in
/// `SteamRunningState::onExitImpl` (`:21`) and cleared again in
/// `StandbyState::onEntryImpl` (`SystemStates.cpp:17`). It means "steam mode is
/// engaged", and it survives for as long as the machine is in the steam state
/// and not a moment longer.
///
/// [`Telemetry::steam_mode`] is that flag: `Machine::steam_mode` is set at
/// exactly those three points in `cc-machine/src/states.rs:218`, `:263` and
/// `:365`. So this route emits it under the C++'s name, which is the parity
/// answer — the UI's `/api/steam` toggle answers `steamMode` from the same fact
/// (`machine-toggle-result.ts:9`, `useMachineToggles.ts:41`), and a status poll
/// that disagreed with the toggle that set it would be a worse bug than the one
/// this replaces.
///
/// `brewing` is **kept, and is not the same value**. It is
/// `state.is_brew_state() && state != BrewFinished` (`main.rs:2983`) — derived
/// from the current state rather than latched, and the C++ publishes no such
/// field at all. It was previously emitted *under the name* `steamMode`, which
/// made a brew-state flag answer to a steam-mode name; nothing consumed it
/// (`rg` finds no reader of `/api/status` in the UI at all), so both are emitted
/// now and the addition is recorded in `docs/rust-migration/intentional-diffs.md`.
#[must_use]
pub fn status_json(t: &Telemetry) -> String {
    // `write!` into the `String` rather than `push_str(&format!(..))`: this runs
    // on the httpd task for every poll from an open browser tab, and the
    // `format!` form allocates a temporary `String` per field. ADR-0002 is
    // about not building a large response out of copies, and the small ones
    // should not be copying either.
    let mut out = String::with_capacity(512);
    let _ = write!(
        out,
        "{{\"temperature\":{:.2},\"setpoint\":{:.2},\"heaterPower\":{:.2},\
         \"machineState\":{},\"isStandby\":{},\"standbyTime\":{},\
         \"pidEnabled\":{},\"steamMode\":{},\"brewing\":{},\"uptime\":{},\
         \"shotsSinceBackflush\":{},\"backflushReminderThreshold\":{},\
         \"backflushReminderDue\":{}",
        t.temperature_c,
        t.setpoint_c,
        t.heater_power_pct,
        t.machine_state,
        t.standby,
        t.standby_remaining_ms,
        t.pid_enabled,
        t.steam_mode,
        t.brewing,
        t.uptime_ms,
        t.shots_since_backflush,
        t.backflush_threshold,
        t.backflush_due,
    );
    let _ = write!(out, "{}", optional_number("weight", t.weight_g));
    let _ = write!(out, "{}", optional_number("brewWeight", t.brew_weight_g));
    let _ = write!(out, "{}", optional_number("pressure", t.pressure_bar));
    match t.water_tank_full {
        Some(full) => {
            let _ = write!(out, ",\"waterTankFull\":{full}");
        }
        None => {
            let _ = write!(out, ",\"waterTankFull\":null");
        }
    }
    let _ = write!(
        out,
        ",\"wifiSignal\":{},\"wifiAssociated\":{},\"wifiOffline\":{},\
         \"mqttConfigured\":{},\"mqttConnected\":{}",
        t.signal, t.wifi_associated, t.wifi_offline, t.mqtt_configured, t.mqtt_connected,
    );
    match &t.ip {
        Some(ip) => {
            let _ = write!(out, ",\"ip\":\"{ip}\"");
        }
        None => {
            let _ = write!(out, ",\"ip\":null");
        }
    }
    let _ = write!(out, ",\"heapFree\":{}}}", free_heap());
    out
}

fn optional_number(name: &str, value: Option<f64>) -> String {
    match value {
        Some(v) => format!(",\"{name}\":{v:.2}"),
        None => format!(",\"{name}\":null"),
    }
}

/// `/api/temperatures` — `WebServerManager.cpp:620-630` / `getTempString`,
/// `:1165-1191`.
///
/// Three keys, the C++ names, and the same payload as the `new_temps` SSE
/// event, because the UI reads one shape from two places.
#[must_use]
pub fn temperatures_json(t: &Telemetry) -> String {
    format!(
        "{{\"currentTemp\":{:.2},\"targetTemp\":{:.2},\"heaterPower\":{:.2}}}",
        t.temperature_c, t.setpoint_c, t.heater_power_pct
    )
}

/// `GET /api/history` — `WebServerManager.cpp:631-645` and
/// `TemperatureHistory::generateJson` (`:134-153`).
///
/// Three parallel arrays, oldest point first, each value rounded to two
/// decimals by `round2` (`helperUtils.h:32`). The C++ builds this with
/// `ArduinoJson` into an `AsyncJsonResponse`; here it is one `String` handed to
/// [`respond_large`], which streams it in 512-byte chunks and refuses it below
/// the ADR-0002 heap floor — the same two protections, arrived at from a
/// different library.
///
/// **The spacing is part of the contract.** The ring keeps every third sample,
/// and the UI reconstructs the x axis as `now - 3 * i` seconds
/// (`CleverCoffeeContext.tsx:240-243`). A ring that kept every sample would
/// draw 30 minutes of data across a 10-minute axis.
///
/// An empty ring answers three empty arrays, not an error: a machine that has
/// been up for four seconds has no history, and the chart's own loading state is
/// a better answer than a failure.
#[must_use]
pub fn history_json(shared: &Shared) -> String {
    // **Copied out under the lock, formatted after it.**
    //
    // Holding the guard across the formatting is the mistake this replaces: the
    // control task appends a sample on the same mutex, so ~1800 `write!` calls
    // and a heap growing to ~12 KB stall the control loop for the duration — a
    // missed 10 ms period and a late heartbeat, caused by a chart. The copy goes
    // to the **heap**, because 600 points is 7.2 KB and the httpd task has 8 KB
    // of stack — the same lesson as the 1 KB display framebuffer, and the same
    // fix.
    //
    // The ring's own `len()` is read under the lock too. Inferring the length
    // from the copied points would truncate at the first zero — and a boiler
    // genuinely at 0 °C is a value this machine reports.
    let (points, len) = match shared.history.lock() {
        Ok(guard) => {
            let len = guard.len();
            let mut copy = alloc::boxed::Box::new([cc_domain::history::Point::default(); 600]);
            for (i, point) in copy.iter_mut().enumerate().take(len) {
                if let Some(p) = guard.get(i) {
                    *point = p;
                }
            }
            (copy, len)
        }
        Err(_) => {
            return String::from("{\"currentTemps\":[],\"targetTemps\":[],\"heaterPowers\":[]}");
        }
    };
    let _ = points;

    let mut out = String::with_capacity(64 + len * 18);
    out.push_str("{\"currentTemps\":[");
    for i in 0..len {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{:.2}", points[i].current_temp);
    }
    out.push_str("],\"targetTemps\":[");
    for i in 0..len {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{:.2}", points[i].target_temp);
    }
    out.push_str("],\"heaterPowers\":[");
    for i in 0..len {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{:.2}", points[i].heater_power);
    }
    out.push_str("]}");
    out
}

/// The `weight` SSE event's payload. `getWeightJsonString`, `:1193-1210`.
#[must_use]
pub fn weight_json(t: &Telemetry) -> String {
    match (t.weight_g, t.brew_weight_g) {
        (Some(w), Some(b)) => format!("{{\"weight\":{w:.0},\"brewWeight\":{b:.0}}}"),
        _ => error_body("Scale data unavailable"),
    }
}

/// `GET /api/health` — `WebServerManager.cpp:411`.
///
/// An empty 200 in the C++. This adds a body, because an empty 200 cannot
/// distinguish "the machine is alive" from "the handler is registered but the
/// control task has not published yet", and that distinction is the whole
/// question a health check is asked.
#[must_use]
pub fn health_json(t: &Telemetry) -> String {
    format!(
        "{{\"status\":\"ok\",\"uptime\":{},\"machineState\":{}}}",
        t.uptime_ms, t.machine_state
    )
}

/// `GET /api/nvs-debug` — `WebServerManager.cpp:648-678`.
///
/// The C++'s `metadata` block, with the C++'s key names. **No parameter
/// values**: the C++ reported counts and heap figures only, and this keeps that.
/// The configuration's four credentials are in the same blob, and this endpoint
/// is reachable without authentication (`system.auth` is not enforced by the
/// C++'s REST API at all), so a payload that listed parameters would be a
/// credential disclosure with no local symptom.
#[must_use]
pub fn nvs_debug_json(describe: &str, parameter_count: usize) -> String {
    let (version, bytes) = parse_describe(describe);
    format!(
        "{{\"message\":\"NVS debugging - parameter details available\",\
         \"parameters_count\":{parameter_count},\
         \"parameters\":[],\
         \"metadata\":{{\"total_parameters\":{parameter_count},\
         \"nvs_namespace\":\"cc\",\"blob_schema_version\":{version},\
         \"blob_bytes\":{bytes},\"free_heap\":{},\"min_free_heap\":{}}}}}",
        free_heap(),
        min_free_heap()
    )
}

/// Pull `(version, bytes)` out of a `BlobConfigStore::describe` string.
///
/// The description is `"cc/cc.config: schema v1, 2071 B JSON"`. Parsing it back
/// is a little absurd, which is the argument for the store exposing the two
/// numbers directly — and it does, in `raw()`. What is awkward is that this
/// function wants them *and* the descriptive line, and building the line from
/// the numbers would be worse. So this parses, and the format is pinned by
/// `cc_config::blob_store`'s own tests.
fn parse_describe(describe: &str) -> (u32, u32) {
    let version = describe
        .split("schema v")
        .nth(1)
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(0);
    let bytes = describe
        .split(" B JSON")
        .next()
        .and_then(|rest| rest.rsplit(' ').next())
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(0);
    (version, bytes)
}

/// The endpoints R3 has not ported, and what they say.
///
/// Not a stub list for its own sake: each of these is a real route the C++
/// registers, a real tab in the React UI, and a real support question. A route
/// that 404s is indistinguishable from a firmware that lost the feature, whereas
/// a route that answers "not in this build, here is what it would do" is a
/// diagnosable answer.
#[must_use]
pub fn unavailable_json(feature: &str, task: &str) -> String {
    format!("{{\"error\":\"{feature} is not available in this build\",\"reason\":\"{task}\"}}")
}

// The built React SPA, embedded by `build.rs`. The table is `&'static` and the
// bytes live in flash, so serving the UI costs no RAM beyond the httpd send
// buffer — which is the reason the bundle is embedded rather than mounted: see
// `build.rs` for the size arithmetic that decided it.
mod bundle {
    include!(concat!(env!("OUT_DIR"), "/ui_bundle.rs"));
}

use bundle::{UiAsset, UI_ASSETS, UI_FILE_COUNT, UI_INDEX, UI_TOTAL_BYTES};

/// The chunk size the embedded-file writer streams at.
///
/// The same 512 B [`respond`] uses, and for the same reason: the httpd task's
/// stack is 8 KB and the send path buffers, so a large chunk is a large
/// allocation on a machine whose heap is the scarce resource. At 512 B the
/// 183 KB bundle is 357 `httpd_resp_send_chunk` calls, which is nothing.
const UI_CHUNK_BYTES: usize = 512;

/// What `GET /ui...` resolved to.
///
/// The three cases are distinct because conflating them is what makes a broken
/// SPA hard to diagnose: serving `index.html` for a missing `.js` produces a
/// `200`, a correct-looking HTML body, and a browser console full of MIME
/// errors — which looks like a broken app rather than a missing file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiTarget {
    /// Serve this embedded file.
    File(&'static UiAsset),
    /// No such file, but the path has no extension: it is a client-side route,
    /// so serve the SPA shell and let the router sort it out.
    SpaRoute,
    /// No such file, and it names an extension: the caller sends a 404.
    Missing,
}

/// Resolve a request URI to an embedded asset.
///
/// `/ui` and `/ui/` are the shell. Everything else is looked up under `/ui`, and
/// an unknown extensionless path falls back to the shell — that is what makes a
/// deep link like `/ui/config/behavior` survive a page reload.
///
/// The lookup is a linear scan over a `&'static` table comparing borrowed
/// strings, so a request allocates nothing. Path traversal is unrepresentable
/// rather than filtered: there is no filesystem, and a path is either a key in
/// the table or it is nothing.
///
/// The query string is stripped before the extension test. The C++ tests
/// `request->url()`, which is the URL *including* the query
/// (`WebServerManager.cpp:977`, `:1017`), so a C++ SPA route with `?x=1.5` is
/// treated as an asset request and 404s. That is a latent C++ bug, not parity
/// worth reproducing.
#[must_use]
pub fn resolve_ui(uri: &str) -> UiTarget {
    let path = uri.split_once('?').map_or(uri, |(path, _)| path);
    let Some(rest) = path.strip_prefix("/ui") else {
        return UiTarget::Missing;
    };
    // `/ui*` is the registered template, so `/uixyz` reaches this handler too
    // and must not be treated as a child of `/ui`.
    if !rest.is_empty() && !rest.starts_with('/') {
        return UiTarget::Missing;
    }
    let rest = rest.trim_start_matches('/');
    if rest.is_empty() {
        return UiTarget::File(UI_INDEX);
    }
    if let Some(asset) = UI_ASSETS
        .iter()
        .find(|asset| asset.path.strip_prefix('/') == Some(rest))
    {
        return UiTarget::File(asset);
    }
    if rest.contains('.') {
        UiTarget::Missing
    } else {
        UiTarget::SpaRoute
    }
}

/// The `Content-Type` for an embedded asset, from its path.
///
/// **This function is the difference between a working UI and a blank page.** A
/// browser refuses to execute a script whose `Content-Type` is not a JavaScript
/// media type, and it refuses a stylesheet that is not CSS — with a `200` in
/// the network tab either way. `application/javascript` is used rather than the
/// newer `text/javascript` because that is what the C++'s `getContentType`
/// returns (`WebServerManager.cpp`, the `.js` arm) and because it is in every
/// browser's accept list.
#[must_use]
pub fn mime_for(path: &str) -> &'static str {
    match path.rsplit_once('.').map_or("", |(_, extension)| extension) {
        "html" | "htm" => "text/html",
        "js" | "mjs" => "application/javascript",
        "css" => "text/css",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain",
        // Never `text/plain` for an unknown type: that is the MIME error that
        // produces a blank page, so an unrecognised asset must not claim to be
        // text. It is also what makes an unrecognised type a download rather
        // than something the browser tries to execute.
        _ => "application/octet-stream",
    }
}

/// Write one embedded file, with its MIME type and encoding.
///
/// `esp-idf-svc` 0.53.0 cannot send a `Content-Length`: the `httpd_resp_set_len`
/// call is commented out in `server.rs:1056-1058`, so every response it writes
/// is chunked. The terminating zero-length chunk below is therefore not
/// optional — without it the browser waits for a body that never ends. This is
/// the same reason [`respond`] ends with `conn.write(&[])`.
///
/// Compressed assets are sent with `Content-Encoding: gzip` unconditionally
/// rather than negotiated. Only the gzip form is embedded, so there is no
/// identity variant to fall back to, and every browser advertises gzip. The
/// ceiling: a client that cannot decode gzip cannot load the UI, and `curl`
/// needs `--compressed`.
fn write_ui_file(
    conn: &mut EspHttpConnection<'_>,
    asset: &'static UiAsset,
) -> Result<(), EspError> {
    let mime = mime_for(asset.path);
    // Cache parity with the C++ (`WebServerManager.cpp:907-921`): the shell must
    // not be cached or a rebuilt UI is unreachable until the entry expires,
    // while Vite's content-hashed asset names make the rest safe to pin.
    let cache = if asset.path == UI_INDEX.path {
        "no-cache, no-store, must-revalidate"
    } else {
        "max-age=604800"
    };

    if asset.gzip {
        conn.initiate_response(
            200,
            Some("OK"),
            &[
                ("Content-Type", mime),
                ("Content-Encoding", "gzip"),
                ("Cache-Control", cache),
            ],
        )?;
    } else {
        conn.initiate_response(
            200,
            Some("OK"),
            &[("Content-Type", mime), ("Cache-Control", cache)],
        )?;
    }

    for chunk in asset.bytes.chunks(UI_CHUNK_BYTES) {
        let _ = conn.write(chunk);
    }
    let _ = conn.write(&[]);
    Ok(())
}

/// `GET /ui` and everything under it.
///
/// Registered once, on the template `/ui*`. ESP-IDF's
/// `httpd_uri_match_wildcard` (`httpd_uri.c:24-70`) treats a trailing `*` as
/// "prefix", and a template *without* `*` or `?` still requires an exact length
/// match — so enabling [`Configuration::uri_match_wildcard`] for this one route
/// leaves all 23 exact `/api/*` handlers exactly as strict as they were.
fn serve_ui(mut req: Request<&mut EspHttpConnection<'_>>) -> Result<(), EspError> {
    let target = resolve_ui(req.uri());
    let conn = req.connection();
    match target {
        UiTarget::File(asset) => write_ui_file(conn, asset),
        UiTarget::SpaRoute => write_ui_file(conn, UI_INDEX),
        UiTarget::Missing => {
            // The C++'s "File not found" (`WebServerManager.cpp:994`), and
            // deliberately not the shell: a missing asset must be visible as a
            // 404 rather than smuggled in as HTML the browser then fails to
            // parse as JavaScript.
            conn.initiate_response(404, Some("Not Found"), &[("Content-Type", "text/plain")])?;
            conn.write_all(b"File not found")
        }
    }
}

/// The routes this server registers, for the boot log and a route test.
#[must_use]
pub fn routes() -> Vec<(&'static str, Method)> {
    vec![
        ("/api/status", Method::Get),
        ("/api/health", Method::Get),
        ("/api/temperatures", Method::Get),
        ("/api/history", Method::Get),
        ("/api/nvs-debug", Method::Get),
        ("/api/parameter-help", Method::Get),
        ("/api/config", Method::Get),
        ("/api/config/download", Method::Get),
        ("/api/config/upload", Method::Post),
        ("/api/parameters", Method::Get),
        ("/api/parameters", Method::Post),
        // A real preflight handler, on a wildcard. It used to be advertised as
        // `("/api/status", Method::Options)` with nothing registered behind it,
        // so a preflight 404'd; see `register_preflight`.
        ("/api*", Method::Options),
        ("/api/setpoint", Method::Post),
        ("/api/steam", Method::Post),
        ("/api/pid", Method::Post),
        ("/api/backflush", Method::Post),
        ("/api/sleep", Method::Post),
        ("/api/wake", Method::Post),
        ("/api/scale/tare", Method::Post),
        ("/api/scale/calibration", Method::Post),
        ("/api/maintenance/reset-backflush-counter", Method::Post),
        ("/api/wifi-reset", Method::Post),
        ("/api/factory-reset", Method::Post),
        ("/api/restart", Method::Post),
        // R3-15 defers OTA. `/api/ota/status` answers a real status document
        // saying so; the three mutating routes answer `unavailable_json`. A
        // 404 here would be indistinguishable from a lost feature.
        ("/api/ota/status", Method::Get),
        ("/api/ota/firmware", Method::Post),
        ("/api/ota/filesystem", Method::Post),
        ("/api/ota/url", Method::Post),
        ("/events", Method::Get),
        ("/", Method::Get),
        ("/ui*", Method::Get),
    ]
}

/// What `/events` needs, behind the `user_ctx` pointer ESP-IDF hands back.
///
/// `register_raw_sse` can only pass **one** pointer, and the route now needs two
/// things: the stream itself and the credential check. They travel together in
/// one `Arc` rather than in a second global, so the check cannot be wired for
/// one route and forgotten for the other.
pub(crate) struct SseRoute {
    /// The broadcast state the handler attaches the detached request to.
    pub sse: Arc<Sse>,
    /// The same [`Auth`] every `fn_handler` route goes through.
    pub auth: Arc<Auth>,
}

/// A running HTTP server.
///
/// The `EspHttpServer` is moved into its own thread by the caller, because
/// `esp-idf-svc` registers every handler on an internal httpd task
/// (`server.rs:499-500`, "Registered Httpd server handler ... for URI") and the
/// handlers are `Send + 'static` closures over an `Arc<Shared>`. So there is
/// nothing to do here but hold the handle, and the value of this type is the
/// route table above plus the ownership statement.
pub struct Web {
    server: EspHttpServer<'static>,
    sse: Arc<Sse>,
    shared: Arc<Shared>,
}

impl Web {
    /// Start the server and register every route.
    ///
    /// `commands` is the network→control channel. A handler never calls into the
    /// state machine: it formats a [`Command`], and the control task drains the
    /// queue at the top of its tick (04 §3.2). `send` is non-blocking and drops
    /// on a full queue, which is the C++'s behaviour by way of the web server
    /// simply never being able to reach the main loop.
    ///
    /// # Errors
    ///
    /// `EspError` from `EspHttpServer::new` (`ESP_ERR_HTTPD_ALLOC_MEM` or
    /// `ESP_ERR_HTTPD_TASK` — both are "this machine has no heap for a web
    /// server", which is the running-out-of-room case and is not worth
    /// distinguishing at boot) or from any `httpd_register_uri_handler`, whose
    /// `ESP_ERR_HTTPD_HANDLERS_FULL` is the one worth naming: it fires at boot
    /// if the route table outgrows [`MAX_URI_HANDLERS`], and
    /// `the_route_table_fits_the_servers_handler_budget` is the test that
    /// catches it before a flash.
    ///
    /// `parameters` is the mailbox `POST /api/parameters` hands its accepted
    /// pairs to. It is a parameter and not a field of [`Shared`] because it is
    /// not telemetry: it is a request, it is drained by the control task rather
    /// than read by a handler, and [`Shared`] is documented as "the telemetry
    /// every handler reads". See [`crate::task::ParameterHandoff`] for why the
    /// pairs do not travel as a [`Command`].
    #[allow(
        clippy::too_many_lines,
        reason = "this IS a route table. 25 registrations with their handlers, \
                  one after another, is the clearest possible form of it; \
                  splitting it into `register_reads`/`register_commands` would \
                  hide the one property that matters, which is that the whole \
                  table is visible at once against `routes()`."
    )]
    pub fn start(
        shared: Arc<Shared>,
        sse: Arc<Sse>,
        config: &Arc<Config>,
        nvs_description: &str,
        send: &Arc<dyn Fn(Command) + Send + Sync + 'static>,
        parameters: &Arc<crate::task::ParameterHandoff>,
    ) -> Result<Self, EspError> {
        // Every handler is `Send + 'static` (`server.rs:530-538`), so each one
        // captures its own `Arc::clone`. Taking these three by reference and
        // cloning into a local owned `Arc` once is what makes that possible
        // without an `unsafe fn` (`handler_nonstatic`, `server.rs:568`, which
        // this workspace denies).
        let send = Arc::clone(send);
        let parameters = Arc::clone(parameters);
        // Decided once, here, exactly as the C++'s `setupMiddleware` decides it
        // once in `WebServerManager::initialize`. See [`Auth`].
        let auth = Arc::new(Auth::from_config(config));
        if auth.is_enforced() {
            info!("http: web authentication enabled (HTTP Basic, realm CleverCoffee)");
        }
        // `EspHttpServer::new` returns `EspIOError` (`server.rs:345`) while every
        // `httpd_*` call returns `EspError`, so the one conversion is here
        // rather than repeated in every handler.
        let mut server = EspHttpServer::new(&configuration()).map_err(|e| e.0)?;

        // --- reads -------------------------------------------------------
        {
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/status",
                Method::Get,
                move |mut req| {
                    let body = status_json(&shared.snapshot());
                    respond(req.connection(), 200, &body)
                },
            )?;
        }
        {
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/health",
                Method::Get,
                move |mut req| {
                    let body = health_json(&shared.snapshot());
                    respond(req.connection(), 200, &body)
                },
            )?;
        }
        {
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/temperatures",
                Method::Get,
                move |mut req| {
                    let body = temperatures_json(&shared.snapshot());
                    respond(req.connection(), 200, &body)
                },
            )?;
        }
        {
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/history",
                Method::Get,
                move |mut req| {
                    // The C++'s `AsyncJsonResponse` (`:634-640`) with the same
                    // refusal below the heap floor, which `respond_large` applies to
                    // every response over `MAX_JSON_BYTES` — and 600 points is about
                    // 12 KB of JSON, so this route is the second-largest the server
                    // serves after `/api/parameters?filter=all`.
                    let body = history_json(&shared);
                    respond_large(req.connection(), &shared, &body)
                },
            )?;
        }
        {
            let described = String::from(nvs_description);
            register(
                &mut server,
                &auth,
                "/api/nvs-debug",
                Method::Get,
                move |mut req| {
                    // The store itself stays with the control task, which is the
                    // only writer; a handler gets the description string it needs and
                    // nothing that could write. `ConfigStore::describe` takes
                    // `&self` and this is its whole result — namespace, key, schema
                    // version and byte count — so nothing is lost and a `Send +
                    // 'static` handler needs no shared NVS handle at all.
                    let body = nvs_debug_json(&described, cc_config::SCHEMA.len());
                    respond(req.connection(), 200, &body)
                },
            )?;
        }
        {
            register(
                &mut server,
                &auth,
                "/api/parameter-help",
                Method::Get,
                |mut req| {
                    respond(
                        req.connection(),
                        200,
                        &unavailable_json("Per-parameter help", "R3-16 (the schema UI)"),
                    )
                },
            )?;
        }
        {
            let config = Arc::clone(config);
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/config",
                Method::Get,
                move |mut req| {
                    // ADR-0002 decision 2: serialise ONCE. The C++ built a
                    // `JsonDocument`, copied it to a `String`, and the web server
                    // copied it again — three ~19 KB allocations, which is the abort
                    // the ADR exists to prevent. `json_export` returns a single
                    // `String` and `respond_large` streams it out in chunks.
                    let Ok(json) = cc_config::json_export(&config) else {
                        return respond(
                            req.connection(),
                            500,
                            &error_body("Failed to generate config"),
                        );
                    };
                    let body = json;
                    respond_large(req.connection(), &shared, &body)
                },
            )?;
        }
        {
            let config = Arc::clone(config);
            let shared = Arc::clone(&shared);
            // The **published** body, not the boot snapshot.
            //
            // `config` is the `Config` as it was when the server was built, so
            // rendering from it answers a question about the past: a
            // `POST /api/parameters` write lands in the control task's own
            // `Config` and in NVS, and this GET would still report the old
            // number. The human saw exactly that -- "the UI says success, the PID
            // is still disabled" -- and the UI was faithfully reporting a stale
            // answer.
            //
            // The control task publishes the body it would have served once a
            // heartbeat (`main.rs`), so the read reflects the running machine.
            // Before the first publish it falls back to the boot snapshot, which
            // is the right answer for the first second after boot.
            let parameters = Arc::clone(&parameters);
            register(
                &mut server,
                &auth,
                "/api/parameters",
                Method::Get,
                move |mut req| {
                    // `live()` hands back an `Arc` clone (a refcount bump),
                    // not a copy of ~8.8 KB. The fallback is built ONLY when
                    // nothing has been published yet, so the httpd task's own
                    // stack never holds two copies of the body -- and the guard
                    // is dropped before `respond_large` reads it.
                    let published = parameters.live();
                    let generated = published.is_none().then(|| parameters_json(&config));
                    let body: &str = match (&published, &generated) {
                        (Some(shared_body), _) => shared_body.as_str(),
                        (None, Some(body)) => body.as_str(),
                        // Unreachable: `generated` is `Some` exactly when
                        // `published` is `None`. Written out anyway so that if
                        // that ever stops being true it is a compile error rather
                        // than a 200-bytes-of-nothing response.
                        (None, None) => unreachable!("generated is Some when published is None"),
                    };
                    respond_large(req.connection(), &shared, body)
                },
            )?;
        }
        {
            // `GET /api/config/download` — the C++'s
            // `AsyncURIMatcher::exact("/api/config/download")`
            // (`WebServerManager.cpp:706-723`). The body is the same
            // `exportToJsonObject` document `/api/config` serves; what makes it a
            // download is the `Content-Disposition` header, which is the whole
            // point of the separate route.
            let config = Arc::clone(config);
            let shared = Arc::clone(&shared);
            register(
                &mut server,
                &auth,
                "/api/config/download",
                Method::Get,
                move |mut req| {
                    let Ok(json) = cc_config::json_export(&config) else {
                        // The C++'s `response->overflowed()` arm (`:711-715`).
                        return respond(
                            req.connection(),
                            500,
                            &error_body("Failed to generate config"),
                        );
                    };
                    // `Content-Disposition: attachment; filename="config.json"`
                    // — `WebServerManager.cpp:717`, verbatim. Without it a
                    // browser renders the JSON instead of saving it, which is
                    // the whole reason this route exists separately.
                    respond_download(req.connection(), &shared, &json)
                },
            )?;
        }
        {
            // `POST /api/parameters` — the C++'s writer
            // (`WebServerManager.cpp:821-878`), which is the same route as the
            // `GET` above because the C++ registers it `HTTP_ANY` and branches
            // on the method (`:815`, `:879`). Two registrations rather than one
            // `HTTP_ANY` because `esp-idf-svc`'s `Method` has no "any"
            // (embedded-svc 0.29 `http.rs:17-52`). A third method still gets
            // the right status: ESP-IDF's own "method not registered for this
            // URI" handler answers **405**, which is what the C++ sends (`:880`),
            // though its body is ESP-IDF's "Specified method is invalid for this
            // resource" rather than the C++'s `{"error":"Method not allowed"}`.
            let handoff = Arc::clone(&parameters);
            register(
                &mut server,
                &auth,
                "/api/parameters",
                Method::Post,
                move |mut req| {
                    let body = drain_body_bounded(req.connection(), MAX_PARAMETER_BODY_BYTES);
                    // `request->params()` (`:823`) is the query string *and* the body,
                    // in that order, because `AsyncWebServerRequest` appends the query
                    // args before the POST fields. So `?pid.enabled=1` and
                    // `pid.enabled=1` are the same request, and a request may carry
                    // both.
                    let mut pairs = cc_config::form::parse_form(query_of(req.uri()));
                    pairs.extend(cc_config::form::parse_form(&body));
                    if pairs.len() > MAX_PARAMETER_PAIRS {
                        return respond(
                            req.connection(),
                            400,
                            &error_body("too many parameters in one request"),
                        );
                    }

                    let verdict = classify_parameters(&pairs);
                    if let ParameterPost::Rejected { reasons, .. } = &verdict {
                        // The C++ logs one `WARNING` per failure (`:853`, `:857`) and
                        // then answers a single 400 that names none of them. Naming
                        // them here is the whole diagnostic value of a rejected
                        // parameter: without it, `400` on a 20-field form is a
                        // guessing game.
                        for reason in reasons {
                            warn!("http: /api/parameters rejected {reason}");
                        }
                    }
                    let accepted = verdict.clone().into_pairs();
                    if !accepted.is_empty() && !handoff.stage_and_wait(accepted) {
                        // Either the mailbox was full, or the control task had not
                        // applied the request within the ack timeout. The response
                        // is about to say the parameters were saved, so this is the
                        // one case where this handler's `200` would be a lie: it is
                        // a 503 and the UI keeps the value it typed.
                        warn!("http: /api/parameters was not applied by the control task in time");
                        return respond(
                            req.connection(),
                            503,
                            &error_body("the control task did not apply the parameters, retry"),
                        );
                    }
                    let (status, payload) = verdict.response();
                    // A `200 {"success":true}` on a write that cannot affect the
                    // running machine is the lie the human reported. Naming the
                    // keys that need a reboot turns it into an answer.
                    let reboot = verdict.reboot_required();
                    if !reboot.is_empty() {
                        let keys = reboot
                            .iter()
                            .map(|k| format!("\"{k}\""))
                            .collect::<Vec<_>>()
                            .join(",");
                        let body = format!(
                            "{{\"success\":true,\"message\":\"Parameters updated and saved\",\
\"requiresReboot\":true,\"requiresRebootKeys\":[{keys}],\"reason\":\"read once at startup\"}}"
                        );
                        return respond(req.connection(), status, &body);
                    }
                    respond(req.connection(), status, payload)
                },
            )?;
        }
        {
            // `POST /api/config/upload` — the C++'s
            // `AsyncURIMatcher::exact("/api/config/upload")` with an
            // `AsyncCallbackJsonWebHandler` (`WebServerManager.cpp:725-762`).
            //
            // **The body is `application/json`, not multipart.** The C++ says so
            // in a comment on the line above its own registration (`:725` — "Config
            // upload: application/json body (AsyncCallbackJsonWebHandler buffers
            // full body before parse)"), and `ui/packages/frontend/src/pages/SystemPage.tsx:180-183`
            // posts the selected file's text with `Content-Type:
            // application/json` and no boundary. A multipart reader here would
            // answer the live "Upload configuration" button with 400.
            //
            // The four outcomes, and which is which:
            //
            // * **over the transport cap** → `413`, and nothing is parsed. See
            //   [`drain_body_checked`] for why this cannot truncate.
            // * **not a top-level object** → `400`, the C++'s message verbatim
            //   (`:732`).
            // * **rejected by the reader** → `400`, and the offending keys are
            //   named in the log rather than the body, because the body is what
            //   the UI shows and "invalid values" is what the C++ shows
            //   (`:750`).
            // * **applied** → `200` and `restart: true`, which tells the browser
            //   to call `POST /api/restart` (the C++ does not reboot itself
            //   either — `:760` sets a flag and returns).
            let handoff = Arc::clone(&parameters);
            register(
                &mut server,
                &auth,
                "/api/config/upload",
                Method::Post,
                move |mut req| {
                    let Some(body) = drain_body_checked(req.connection(), MAX_CONFIG_UPLOAD_BYTES)
                    else {
                        // The C++'s `setMaxContentLength(16384)` (`:762`) makes
                        // `ESPAsyncWebServer` answer 413 for an over-long body.
                        // Nothing is parsed, so nothing can be half-applied; and
                        // the tail left unread is purged by ESP-IDF itself
                        // (`httpd_req_delete`, `httpd_parse.c:841-855`).
                        return respond(
                            req.connection(),
                            413,
                            &upload_response(
                                false,
                                "Configuration is too large; the limit is 16384 bytes",
                            ),
                        );
                    };

                    // `cc_config::json::document_pairs` is the whole validation:
                    // size, syntax, top-level object, flat dotted keys, one known
                    // key, every value in range — and it is **total**, so a
                    // document with nine good keys and one impossible one
                    // produces no pairs at all rather than nine.
                    let pairs = match cc_config::document_pairs(&body) {
                        Ok(pairs) => pairs,
                        Err(rejection) => {
                            // `WebServerManager.cpp:737-752` names two of these
                            // exactly; the rest share the C++'s third message,
                            // which is what its own `importFromJson` answers
                            // `false` for (`Config.cpp:348-374`).
                            if let cc_config::ImportError::InvalidValues { rejected } = &rejection {
                                for value in rejected {
                                    warn!(
                                        "http: /api/config/upload rejected {}: {}",
                                        value.key,
                                        cc_config::json::describe_reason(value.reason)
                                    );
                                }
                            }
                            let message = match &rejection {
                                cc_config::ImportError::NotAnObject => {
                                    "JSON body must be a top-level object"
                                }
                                cc_config::ImportError::FlatDottedKeys { .. } => {
                                    "Flat dotted-key JSON is not supported. Use nested objects (see Download Config)."
                                }
                                _ => "Configuration validation failed. Use a file from Download Config.",
                            };
                            return respond(
                                req.connection(),
                                400,
                                &upload_response(false, message),
                            );
                        }
                    };

                    // Belt and braces, and the reason this route can promise it
                    // never half-applies.
                    //
                    // `document_pairs` renders each value into the string form
                    // `cc_config::assign::parse` accepts, so every pair is
                    // already good by the only rule that matters — and that is a
                    // *property*, pinned by `cc-config`'s
                    // `an_every_pair_from_an_upload_is_accepted_by_the_one_writer`,
                    // not a promise. If a future parameter kind ever breaks the
                    // rendering, `classify_parameters` turns a silent partial
                    // apply into a visible `400` here, and refuses the **whole**
                    // document rather than staging the part that happened to
                    // parse — which is the property `/api/parameters` cannot
                    // offer, because that route deliberately applies what it can.
                    let verdict = classify_parameters(&pairs);
                    if let ParameterPost::Rejected { reasons, .. } = &verdict {
                        for reason in reasons {
                            warn!("http: /api/config/upload rejected {reason}");
                        }
                        return respond(
                            req.connection(),
                            400,
                            &upload_response(
                                false,
                                "Configuration validation failed. Use a file from Download Config.",
                            ),
                        );
                    }

                    // `document_pairs` answers `NoKnownParameters` when the
                    // document names nothing this firmware knows, so `pairs` is
                    // non-empty on every path that reaches here — there is no
                    // "nothing to do" case to skip the control task for, as
                    // there is on `/api/parameters`.
                    if !handoff.stage_and_wait(pairs) {
                        // The same 503 `POST /api/parameters` answers
                        // (`:1873-1882`): the response is about to say the
                        // configuration was saved, so it must not say it unless
                        // the control task applied it.
                        warn!(
                            "http: /api/config/upload was not applied by the control task in time"
                        );
                        return respond(
                            req.connection(),
                            503,
                            &upload_response(
                                false,
                                "the control task did not apply the configuration, retry",
                            ),
                        );
                    }
                    info!("http: /api/config/upload validated and applied");
                    respond(
                        req.connection(),
                        200,
                        &upload_response(true, "Configuration validated and applied successfully."),
                    )
                },
            )?;
        }

        // --- CORS preflight ------------------------------------------------
        register_preflight(&mut server)?;

        // --- commands ----------------------------------------------------
        register_command(
            &mut server,
            &auth,
            "/api/setpoint",
            Arc::clone(&send),
            parse_setpoint,
        )?;
        // The C++'s three toggle routes (`WebServerManager.cpp:437-509`). All
        // three read no field in the C++; all three are what the UI's buttons
        // call with a bare POST.
        register_toggle(
            &mut server,
            &auth,
            &shared,
            Arc::clone(&send),
            &Toggle {
                uri: "/api/steam",
                key: "steamMode",
                toggled: Command::ToggleSteam,
                explicit: Command::SetSteam,
                current: |t| t.steam_mode,
            },
        )?;
        register_toggle(
            &mut server,
            &auth,
            &shared,
            Arc::clone(&send),
            &Toggle {
                uri: "/api/pid",
                key: "pidEnabled",
                toggled: Command::TogglePid,
                explicit: Command::SetPid,
                current: |t| t.pid_enabled,
            },
        )?;
        register_toggle(
            &mut server,
            &auth,
            &shared,
            Arc::clone(&send),
            &Toggle {
                uri: "/api/backflush",
                key: "backflushOn",
                toggled: Command::ToggleBackflush,
                explicit: Command::SetBackflush,
                current: |t| t.backflush_mode,
            },
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/sleep",
            Arc::clone(&send),
            Command::Sleep,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/wake",
            Arc::clone(&send),
            Command::Wake,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/scale/tare",
            Arc::clone(&send),
            Command::Tare,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/scale/calibration",
            Arc::clone(&send),
            Command::Calibrate,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/maintenance/reset-backflush-counter",
            Arc::clone(&send),
            Command::ResetBackflushCounter,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/wifi-reset",
            Arc::clone(&send),
            Command::WifiReset,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/factory-reset",
            Arc::clone(&send),
            Command::FactoryReset,
        )?;
        register_flag(
            &mut server,
            &auth,
            "/api/restart",
            Arc::clone(&send),
            Command::Restart,
        )?;

        // --- OTA (R3-15, deferred) -----------------------------------------
        //
        // `src/ota.cpp:847-866` registers four routes. OTA is explicitly
        // deferred ("we can implement OTA later"), so none of them updates
        // anything — but all four are registered, because the UI has a tab that
        // calls them and a **404 is indistinguishable from a lost feature**.
        //
        // `/api/ota/status` answers the C++'s real status shape; the three
        // mutating routes answer `unavailable_json("OTA", "R3-15")`, which says
        // plainly that this build has no OTA and names the task that owns it.
        // Nothing here is a stub pretending to work: no route claims success it
        // did not achieve.
        {
            register(
                &mut server,
                &auth,
                "/api/ota/status",
                Method::Get,
                |mut req| respond(req.connection(), 200, &ota_status_json()),
            )?;
        }
        for uri in ["/api/ota/firmware", "/api/ota/filesystem", "/api/ota/url"] {
            // `sendUploadResult(request, "No firmware file provided")` is the
            // C++'s *missing-file* arm; the honest answer for a build with no
            // OTA at all is the unavailability one, and `400` is the status the
            // UI's error path already handles (`OTAUpdateSection.tsx:191-207`
            // shows `result.message` for any non-success).
            register(&mut server, &auth, uri, Method::Post, |mut req| {
                respond(req.connection(), 501, &unavailable_json("OTA", "R3-15"))
            })?;
        }

        // --- static ------------------------------------------------------
        {
            register(&mut server, &auth, "/", Method::Get, |mut req| {
                // WebServerManager.cpp:974: `request->redirect("/ui/")`.
                let conn = req.connection();
                conn.initiate_response(302, Some("Found"), &[("Location", "/ui/")])?;
                conn.write_all(b"")
            })?;
        }
        {
            // One handler for the shell, the assets and the client-side routes.
            // The `*` is what makes `/ui/brew` reach it at all; see [`serve_ui`].
            register(&mut server, &auth, "/ui*", Method::Get, serve_ui)?;
        }

        // --- SSE ---------------------------------------------------------
        // Registered as a **raw** handler, not through `fn_handler`. That is the
        // whole of the double-response fix: `fn_handler` wraps every closure in
        // `to_native_handler`, which calls `complete()` after the handler
        // returns, and `complete()` writes a complete response
        // (`httpd_resp_send(.., 0)` with `Content-Length: 0`) that ends the
        // response before the broadcaster has sent a frame — then the detached
        // request's chunked response follows as a second response on the same
        // socket. See `crate::web_async::register_raw_sse` for the full
        // argument and the wire capture.
        //
        // The handler still returns immediately, which is the other half of the
        // requirement: ESP-IDF's httpd is one task, so a handler that does not
        // return is a server that does not serve. `spawn_broadcaster` below
        // owns the writing.
        crate::web_async::register_raw_sse(
            esp_idf_svc::handle::RawHandle::handle(&server),
            Arc::new(SseRoute {
                sse: Arc::clone(&sse),
                auth: Arc::clone(&auth),
            }),
        )?;

        // The broadcaster is the only writer of `/events`, and it is not the
        // httpd task. Started after the routes so a client cannot connect to a
        // stream nobody is servicing.
        spawn_broadcaster(Arc::clone(&sse))?;

        info!(
            "http: listening on port {HTTP_PORT}, {} routes, {} B handler budget",
            routes().len(),
            MAX_URI_HANDLERS
        );
        // The bundle is embedded rather than mounted, so this line is the only
        // proof at runtime that the UI is in the image at all -- and the two
        // numbers are the ones to check against a Vite build.
        info!("http: web UI embedded in flash: {UI_FILE_COUNT} files, {UI_TOTAL_BYTES} B");
        Ok(Self {
            server,
            sse,
            shared,
        })
    }

    /// The SSE counters, for the boot log and `/api/status`.
    #[must_use]
    pub fn sse(&self) -> &Sse {
        &self.sse
    }

    /// The shared state, for the control task.
    #[must_use]
    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// The `EspHttpServer` handle.
    ///
    /// Dropping it stops the server (`server.rs:732-736`), which is why the
    /// firmware must not let it fall out of scope: the httpd task would be torn
    /// down while a browser is mid-request.
    #[must_use]
    pub fn server(&self) -> &EspHttpServer<'static> {
        &self.server
    }

    /// Push one `new_temps` event to every connected client.
    ///
    /// The C++'s `sendTempEvent` (`WebServerManager.cpp:1128-1143`) sends a
    /// `ping` and then a `new_temps` carrying exactly the `/api/temperatures`
    /// body. Both are sent: the `ping` is a liveness marker the UI uses to show
    /// that the stream is alive independently of the data, and dropping it would
    /// make a stream that is connected but idle indistinguishable from one that
    /// has died.
    pub fn send_temp_event(&self) {
        let snapshot = self.shared.snapshot();
        let payload = temperatures_json(&snapshot);
        self.sse.broadcast(Sse::frame("new_temps", &payload));
    }

    /// Push one `weight` event. `WebServerManager.cpp:1145-1152`.
    pub fn send_weight_event(&self) {
        let snapshot = self.shared.snapshot();
        let payload = weight_json(&snapshot);
        self.sse.broadcast(Sse::frame("weight", &payload));
    }
}

/// The `/api/parameters` body: every registered parameter, C++ shape.
///
/// `Config::getAllParameters(array, "all")` (`WebServerManager.cpp:818`), which
/// is a loop of `param->toJson(paramObj)` (`:401-404`).
///
/// # The C++'s ten fields, and the six this firmware emits
///
/// `BaseParamDef::toJsonBase` (`Config.h:99-109`) writes `name`, `label`,
/// `section`, `order`, `helpText` and `type`; `ParamDef::toJson`
/// (`:215-238`) adds `value`, `default`, and — for the arithmetic kinds only —
/// `min` and `max`. That is ten.
///
/// This emits `name`, `type`, `value`, `default`, `min`, `max`: six. **The four
/// missing are `label`, `section`, `order` and `helpText`, and they are missing
/// because there is no data for them, not because they were overlooked.**
/// `cc_config::schema::ParamSpec` carries `key`, `kind`, `default`, `min` and
/// `max`; the C++'s `displayName_`, `section_`, `order_` and `helpText_` are
/// per-parameter literals in `Config.h` that the Rust schema never recorded, and
/// inventing them would put text in front of an operator's UI that the C++ does
/// not have. That is a `cc-config` data gap, recorded here rather than papered
/// over. The UI's editor works without them — it labels by `name`.
///
/// # The value is the stored one, and what "live" means today
///
/// `value` is the C++'s `currentValue_` (`Config.h:226-238`) — what the machine
/// is configured with, not the compiled-in default. The compiled-in default is
/// reported separately as `default`, which is the C++'s `defaultValue_`. A
/// firmware that reported the default in both places would show a
/// saved-and-reloaded operator's settings as if they had been lost.
///
/// **It is the `Config` as of boot, and that is currently the same thing.** The
/// only runtime writer of the store is the Wi-Fi provisioning path
/// (`network::apply_staged`), and it is followed by a reboot, so between boots
/// nothing changes the configuration the web layer holds. The day a
/// `POST /api/config` or a `POST /api/parameters` writer exists — neither is
/// registered yet — this has to read the control task's copy rather than the one
/// captured at boot, or `value` will silently go stale. The `Arc<Config>` is
/// what makes that a one-line change; until then it would be a lie to call this
/// live.
#[must_use]
pub fn parameters_json(config: &Config) -> String {
    // Sized from the C++'s measured ~19 KB (`ADR-0002` §2) plus the `value`
    // field this adds, so the buffer is not reallocated mid-build: a
    // reallocation here is a second copy of a 20 KB buffer, which is precisely
    // the shape ADR-0002 is about.
    let mut out = String::with_capacity(SCHEMA.len() * 160);
    let values = cc_config::values_for(config);
    let _ = write!(out, "[");
    for (index, spec) in SCHEMA.iter().enumerate() {
        if index > 0 {
            let _ = write!(out, ",");
        }
        let _ = write!(
            out,
            "{{\"name\":\"{}\",\"type\":{},\"value\":{},",
            spec.key,
            spec.kind.cpp_param_type(),
            // A `None` here is a `SCHEMA`/`live_value` disagreement, which
            // `cc_config::json`'s own test rules out. `null` is the honest
            // rendering if it ever happens: the key is present, the value is
            // not, and the React editor shows an empty field rather than the
            // default silently presented as the current setting.
            values[index].map_or_else(|| String::from("null"), live_value_json),
        );
        let _ = write!(
            out,
            "\"default\":{},\"min\":{},\"max\":{}}}",
            param_value_json(spec.default),
            opt_number(spec.min),
            opt_number(spec.max),
        );
    }
    let _ = write!(out, "]");
    out
}

/// A live value as JSON.
///
/// The same type distinctions as [`param_value_json`], and for the same reason:
/// the C++'s `toJson` distinguishes `bool` from `int` from `double` from
/// `const char*`, and a `"94.5"` where the editor expects a number makes it
/// refuse the input.
fn live_value_json(value: cc_config::LiveValue<'_>) -> String {
    match value {
        cc_config::LiveValue::Bool(b) => String::from(if b { "true" } else { "false" }),
        cc_config::LiveValue::Int(i) => i.to_string(),
        cc_config::LiveValue::Float(f) => f.to_string(),
        cc_config::LiveValue::Text(t) => format!("\"{t}\""),
        // An enum is an integer on the wire, the same as `ParamValue::Enum`,
        // and `Config.h:105` writes the discriminant as an `int`.
        cc_config::LiveValue::Enum(i) => i.to_string(),
    }
}

fn opt_number(value: Option<f64>) -> String {
    value.map_or_else(|| String::from("null"), |v| format!("{v}"))
}

/// A schema default as JSON.
///
/// The C++'s `toJson` (`:851-866`) distinguishes `bool` from `int` from
/// `double` from `const char*` when it serialises, and so does this — which is
/// why a `Text` default is quoted and a `Bool` is not. Getting that wrong makes
/// the React editor's number input receive `"94.5"` and refuse it.
fn param_value_json(value: ParamValue<'_>) -> String {
    match value {
        ParamValue::Bool(b) => String::from(if b { "true" } else { "false" }),
        ParamValue::Int(i) => i.to_string(),
        ParamValue::Float(f) => f.to_string(),
        ParamValue::Text(t) => alloc::format!("\"{t}\""),
        ParamValue::Enum(i) => i.to_string(),
    }
}

/// The broadcaster task: everything `/events` writes, on a task that is not
/// the httpd task.
///
/// This function is the fix. The previous `/events` handler looped **inside**
/// the handler, and ESP-IDF's httpd is one task for the whole server
/// (`httpd_main.c:533` creates the single `httpd` thread; `httpd_thread` at
/// `:329` is `while (1) { httpd_server(hd); }` over one `select()`), so a
/// looping handler is a looping server. Measured on hardware before this
/// change: with one browser tab holding a stream open, 55 of 60 sequential
/// `GET /api/parameters` requests timed out at the client's 5 s limit, and the
/// five that did get through took 6–7 s. The C++ does not have this failure
/// because `ESPAsyncWebServer`'s `AsyncEventSource` **returns from its handler**
/// and pushes events later from the main loop task
/// (`WebServerManager.cpp:308-319` registers the source; the senders at
/// `:1128-1152` run from `LoopManager::updateWebsite`).
///
/// The shape here is deliberately the same: the handler registers a detached
/// request and returns; this task does the writing. Nothing in this loop runs on
/// the httpd task, so `/api/*` latency is independent of how many streams are
/// open.
///
/// The 50 ms sleep is the loop's only cost when idle. It is not a busy-wait on
/// the httpd task, and it is shorter than it needs to be for correctness — it
/// only bounds how late a due frame is noticed.
/// # Errors
///
/// Never, in practice, and that is deliberate: a thread that cannot be created
/// is not a condition the firmware can serve around, but it is also not a reason
/// to refuse to boot. The web API works without `/events`, so a failure here is
/// logged and the machine comes up. The `Result` is kept because the caller is
/// already in a `Result`-returning function and inventing a second error type
/// for "the SSE stream will not run" would be worse than an always-`Ok` one.
pub fn spawn_broadcaster(sse: Arc<Sse>) -> Result<(), EspError> {
    let spawned = std::thread::Builder::new()
        .name("sse-broadcast".into())
        .stack_size(SSE_BROADCASTER_STACK_BYTES)
        .spawn(move || broadcaster(&sse));
    match spawned {
        Ok(_) => {
            info!("sse: broadcaster task started");
            Ok(())
        }
        Err(e) => {
            warn!("sse: no broadcaster task ({e}) — /events will not stream");
            Ok(())
        }
    }
}

/// Transport frames to every client. It originates only the keepalive.
///
/// It reads no machine state, and deliberately so: the C++'s `AsyncEventSource`
/// reads none either. The producer is the control task (`broadcast_temps`,
/// `WebServerManager.cpp:1128-1143` driven from `LoopManager::updateWebsite`),
/// and giving the broadcaster a `Shared` would invite the next reader to start
/// generating telemetry here — the coupling 04 §3.2 exists to prevent.
fn broadcaster(sse: &Sse) {
    loop {
        let now = now_ms();

        // Take the pushed frames out first, so the lock is not held across the
        // client writes below. Frames go to every client before the cadence
        // frames do, so a pushed `weight` is not stuck behind a `new_temps`.
        let pushed: VecDeque<String> = if let Ok(mut m) = sse.mailbox.lock() {
            std::mem::take(&mut *m)
        } else {
            VecDeque::new()
        };

        let Ok(mut locked) = sse.clients.lock() else {
            esp_idf_hal::delay::FreeRtos::delay_ms(SSE_POLL_MS);
            continue;
        };
        let mut clients = std::mem::take(&mut *locked);
        drop(locked);

        let mut survivors = Vec::with_capacity(clients.len());
        for mut client in clients.drain(..) {
            let mut ok = true;

            if client.await_hello {
                let hello = Sse::frame("hello", "{\"connected\":true}");
                ok = client.req.write(&hello).is_ok();
                if ok {
                    sse.sent.fetch_add(1, Ordering::SeqCst);
                    client.await_hello = false;
                }
            }

            if ok {
                for frame in &pushed {
                    if client.req.write(frame).is_err() {
                        ok = false;
                        sse.dropped.fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                    sse.sent.fetch_add(1, Ordering::SeqCst);
                    client.last_write_ms = now;
                }
            }

            // The keepalive is the broadcaster's own, and the only thing it
            // originates: the C++'s `AsyncEventSource` has no timer of its own
            // and its `new_temps` cadence is the main loop's
            // (`WebServerManager.cpp:1128-1143`), which is `broadcast_temps`
            // here. So the two producers are the control task (events) and this
            // task (liveness), and neither duplicates the other. It is measured
            // from the last write of any kind, so a stream that is receiving
            // events does not also emit keepalives.
            if ok && now.wrapping_sub(client.last_write_ms) >= SSE_KEEPALIVE_MS {
                let keepalive = Sse::keepalive();
                if client.req.write(&keepalive).is_err() {
                    ok = false;
                } else {
                    sse.sent.fetch_add(1, Ordering::SeqCst);
                    client.last_write_ms = now;
                }
            }

            if ok {
                survivors.push(client);
            } else {
                // A closed client is the normal end of an SSE stream, not an
                // error. Counting it is what makes "zero drops over ten
                // minutes" a measurement.
                sse.dropped.fetch_add(1, Ordering::Relaxed);
                client.req.complete();
            }
        }

        // Put the survivors back, and keep any client that connected while this
        // task had the list detached. The handler appends, so the fresh one
        // goes after the survivors; ordering does not matter because every
        // client gets the same frames.
        if let Ok(mut slot) = sse.clients.lock() {
            survivors.append(&mut *slot);
            *slot = survivors;
        } else {
            // The lock is poisoned, so nobody can be using the list. Completing
            // every request is the only way to hand the sessions back; leaking
            // them would eventually stop httpd accepting connections at all
            // (`esp_http_server.h:864-866`).
            for client in survivors {
                client.req.complete();
            }
        }

        esp_idf_hal::delay::FreeRtos::delay_ms(SSE_POLL_MS);
    }
}

/// Register a `POST` handler that takes no parameters and emits a command.
///
/// The C++'s eight no-argument POSTs (`/api/wake`, `/api/sleep`, `/api/health`
/// aside). The `202 Accepted` is deliberate and different from the C++'s `200`:
/// the command has been *queued*, not performed. A `200` on
/// `POST /api/factory-reset` would be a lie — the machine is about to reboot.
fn register_flag(
    server: &mut EspHttpServer<'static>,
    auth: &Arc<Auth>,
    uri: &'static str,
    send: Arc<dyn Fn(Command) + Send + Sync + 'static>,
    command: Command,
) -> Result<(), EspError> {
    register(server, auth, uri, Method::Post, move |mut req| {
        let _ = drain_body(req.connection());
        send(command);
        respond(req.connection(), 202, "{\"accepted\":true}")
    })
}

/// Register a `POST` handler for a route that is a **toggle** in the C++.
///
/// The C++'s `/api/pid`, `/api/steam` and `/api/backflush` read no field. They
/// compute `!current` from live machine state and answer with the new value
/// (`WebServerManager.cpp:444-475`, `:490-491`). A bare `POST` is what the React
/// UI sends (`useMachineToggles.ts:22,31,40` — no body, no query), so a bare
/// `POST` has to work, and it has to *toggle*.
///
/// The explicit forms still work, because the human uses them and the
/// integration checklist spells them `?on=0` / `?on=1`: `value`/`on` in the query
/// or the body wins, and is passed through as [`Command::SetPid`] and its
/// siblings. Only the **absent** field selects the toggle.
///
/// `on_toggle` is the command to emit when no field was given. It carries no
/// value, so the control task resolves `!current` against the machine it owns —
/// the same machine the C++'s handler reads. Doing it here instead would mean
/// toggling a possibly-stale snapshot, and two rapid clicks could both compute
/// the same target.
///
/// # What it answers
///
/// The C++'s `ApiResponses::boolResponse(key, value)` — `{"success": true,
/// "<key>": <bool>}` (`ApiResponses.cpp:10-19`) — where `key` is the route's own
/// name: `pidEnabled`, `steamMode`, `backflushOn`. The C++ answers `200` in all
/// three cases, and this keeps that.
///
/// A route that answers `202 {"accepted":true}` instead would break the UI: it
/// reads `response.ok` (which `202` satisfies) but the human's own tooling and
/// the C++ contract both expect the resulting value in the body.
/// One C++ toggle route: its URI, the key its response reports, and the three
/// functions that turn a request into a command.
///
/// A struct rather than eight positional arguments because the three function
/// pointers are meaningless without the URI and the key: pairing
/// `Command::TogglePid` with `/api/steam`'s `current` is a mistake a reader
/// cannot see and a compiler cannot catch, and naming them together makes the
/// pairing the thing being written down.
#[derive(Clone, Copy)]
struct Toggle {
    /// The route, `/api/…`.
    uri: &'static str,
    /// The C++'s `ApiResponses::boolResponse` key (`ApiResponses.h:12`).
    key: &'static str,
    /// The command a bare `POST` emits. Carries no value: see
    /// [`Command::ToggleSteam`].
    toggled: Command,
    /// Wraps an explicit `value`/`on` field into the absolute command.
    explicit: fn(bool) -> Command,
    /// Reads the current value out of the telemetry snapshot, for the response.
    current: fn(&Telemetry) -> bool,
}

/// Register one [`Toggle`] route.
fn register_toggle(
    server: &mut EspHttpServer<'static>,
    auth: &Arc<Auth>,
    shared: &Arc<Shared>,
    send: Arc<dyn Fn(Command) + Send + Sync + 'static>,
    toggle: &Toggle,
) -> Result<(), EspError> {
    let Toggle {
        uri,
        key,
        toggled: on_toggle,
        explicit,
        current,
    } = *toggle;
    let shared = Arc::clone(shared);
    register(server, auth, uri, Method::Post, move |mut req| {
        let mut fields = cc_config::form::parse_form(query_of(req.uri()));
        let body = drain_body(req.connection());
        fields.extend(cc_config::form::parse_form(&body));
        // `first_of` scans in *name* order, so `value` anywhere beats `on`
        // anywhere — the C++'s `hasParam("value", …).orElse(hasParam("on",
        // …))` order (`WebServerManager.cpp:392`).
        let (chosen, value) = match first_of(&fields, &["value", "on"]) {
            // `start` is the one non-boolean field, and only on
            // `/api/backflush`; see [`Command::StartBackflush`].
            Some(field) if uri == "/api/backflush" && field == "start" => {
                (Command::StartBackflush, None)
            }
            Some(field) => (explicit(parse_flag(&field)), None),
            // No field at all: the C++'s toggle.
            None => (on_toggle, Some(!current(&shared.snapshot()))),
        };
        // **The value is read after the command has been applied.**
        //
        // It used to be computed *before* sending: `!current(&snapshot)` for a
        // bare toggle, from the last telemetry the control task published. So
        // the answer described the machine as it was, and a client that
        // trusted it wrote the old value into its own state. The report was
        // "I press the toggle, the device switches, and the switch stays
        // active until I refresh" — the device was right and the answer was
        // a lie.
        //
        // The wait is bounded (400 ms) and the fallback is the requested
        // value rather than a panic: a stalled control task must not hang a
        // browser, and `refetchParameters` is the client's other route to
        // truth.
        let before = shared.applied();
        send(chosen);
        let settled = shared.wait_applied(before);
        let value = if settled {
            current(&shared.snapshot())
        } else {
            warn!(
                "http: {uri} answered from the requested value — the control \
                     task has not applied the command after {COMMAND_ACK_TIMEOUT_MS} ms"
            );
            value.unwrap_or_else(|| explicit_value(&chosen))
        };
        let body = format!("{{\"success\":true,\"{key}\":{value}}}");
        respond(req.connection(), 200, &body)
    })
}

/// The value an explicit `SetPid`/`SetSteam`/`SetBackflush` carries.
fn explicit_value(command: &Command) -> bool {
    match command {
        Command::SetPid(on) | Command::SetSteam(on) | Command::SetBackflush(on) => *on,
        _ => true,
    }
}

/// A boolean field, C++-style: what `AsyncWebServerRequest::getParam(...)->value()`
/// means when it is compared against `1`.
///
/// The C++ compares the **string** `"1"` (`if (value == "1")` throughout
/// `WebServerManager.cpp`), so `"true"` and `"on"` are not C++ spellings. They
/// are accepted anyway because this firmware already documented them
/// (`register_command`'s comment above) and scripts use them; accepting a
/// superset cannot break a C++-shaped caller.
fn parse_flag(value: &str) -> bool {
    matches!(value, "1" | "true" | "on" | "yes")
}

/// The `POST /api/setpoint` field, parsed through the **schema's** bound.
///
/// # Why this is not `0.0..=150.0`
///
/// That was the C++ handler's own filter (`WebServerManager.cpp:393-395`) and
/// this port reproduced it verbatim — which is how `brew.setpoint = 150` could
/// be written and persisted. The C++ does not have the resulting bug only
/// because the *next* line, `Config::brewSetpoint.set(newSetpoint)`
/// (`WebServerManager.cpp:400`), range-checks 20..=110 (`Config.h:795-802`,
/// `defaults.h:81-82`) and returns false. This port has no such second line on
/// this route: it persisted through `persist_setpoint`
/// (`cc-firmware/src/main.rs`), which wrote whatever it was handed. So the
/// bound that actually protects the value is the schema's, and the only way to
/// be sure of using it is to ask for it — [`cc_config::assign::parse`] is that
/// ask, and it cannot drift from `ParamSpec` the way a repeated literal does.
///
/// A 150 °C setpoint is not a cosmetic defect: `safety.emergency_temp` defaults
/// to 150 and S1's test is *strictly greater*
/// (`cc_safety::SafetyState::is_over_threshold`), so it drives the boiler to
/// the emergency threshold and holds it there with a debounce that can never
/// trip. `cc_safety::validate_config` now refuses that pair on every write
/// path; this is the half that makes the HTTP contract honest about it.
///
/// # Reject, do not clamp
///
/// The two candidates were a `400` and a silent clamp to 110. Clamping was
/// rejected because the caller would be told `202 {"accepted":true}` for a
/// request it did not make: a UI slider stuck at 110 looks like a stuck UI,
/// not a refused write, and the operator has no way to learn the machine is not
/// holding what they asked for. `400` is also what the C++ already answers for
/// a value it will not take (`:404-406`), what `POST /api/parameters` answers
/// for the same key through the same [`cc_config::assign::parse`]
/// (`WebServerManager.cpp:843-859` → this port's `400`), and what
/// `register_command` already sends when this returns `None`.
///
/// # What this costs, deliberately
///
/// The C++ accepts `0.0..=150.0` here and applies `0..20` to the **running**
/// machine — `setProcessSetpoint` (`:398`) runs before the range-checked
/// `set` — while persisting nothing. So `?value=5` is a `202` on the C++ and a
/// `400` here, and `?value=150` is a `200` on the C++ that leaves a 150 °C
/// process setpoint in RAM. Narrowing the accepted range is a divergence from
/// the C++ and is recorded in
/// [`intentional-diffs.md`](../../docs/rust-migration/intentional-diffs.md).
/// What is *not* given up: a fractional setpoint still truncates rather than
/// being refused, because `setProcessSetpoint` takes a `double` in the C++ and
/// a truncated integer is not a safety question.
///
/// # Why an `i32`
///
/// `Command` is `Copy` and holds no `String` (04 §3.2), and a whole-degree
/// setpoint is what the schema's range is quoted in. 20..=110 fits with room
/// to spare.
fn parse_setpoint(value: &str) -> Option<Command> {
    match cc_config::assign::parse("brew.setpoint", value) {
        Ok(cc_config::json::LiveValue::Float(celsius)) =>
        {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "cc_config::assign::parse has already range-checked this \
                          against the schema's 20.0..=110.0, which is three \
                          orders of magnitude inside an i32; the C++ truncates \
                          the same way into its process setpoint"
            )]
            Some(Command::SetSetpoint(celsius as i32))
        }
        // Every other outcome — a non-number, `NaN`, `inf`, or a value outside
        // the schema's range — is a `None`, and `register_command` answers
        // `400`. `Ok` on a non-`Float` is unreachable (the key is registered as
        // a float) and is refused rather than unwrapped, so a future schema
        // retyping cannot turn into a panic on the httpd task.
        _ => None,
    }
}

/// Register a `POST` handler that parses one field into a command.
fn register_command(
    server: &mut EspHttpServer<'static>,
    auth: &Arc<Auth>,
    uri: &'static str,
    send: Arc<dyn Fn(Command) + Send + Sync + 'static>,
    parse: fn(&str) -> Option<Command>,
) -> Result<(), EspError> {
    register(server, auth, uri, Method::Post, move |mut req| {
        // The C++ reads `hasParam("value", true)` — the `true` is "from the
        // body" (`WebServerManager.cpp:392`) — and 0 is a valid setpoint
        // (`:393`), so the field's presence is what matters, not its
        // truthiness.
        //
        // The query string is read too, and the **body wins** where both
        // carry the field. The C++'s `POST /api/pid` reads no field at all —
        // it is a toggle — so there is no C++ answer for `?on=0` to match,
        // and every script, the UI's own button and the integration
        // checklist spell it `?on=0` or `?on=1`. Accepting both is what
        // `curl -X POST '.../api/pid?on=0'` needs; the body is checked first
        // because a form post that also carries a stale query string should
        // do what the form says.
        let mut fields = cc_config::form::parse_form(query_of(req.uri()));
        let body = drain_body(req.connection());
        fields.extend(cc_config::form::parse_form(&body));
        let value = first_of(&fields, &["value", "on"]);
        let Some(value) = value else {
            return respond(req.connection(), 400, &error_body("missing `value`"));
        };
        match parse(&value) {
            Some(command) => {
                send(command);
                respond(req.connection(), 202, "{\"accepted\":true}")
            }
            None => respond(req.connection(), 400, &error_body("value out of range")),
        }
    })
}

/// The first value any of `names` has, in **name** order rather than field order.
///
/// `hasParam(name, true).orElse(hasParam(other, true))` — the C++'s
/// `hasParam("value", …)` then `hasParam("on", …)` (`:392`), so `value` anywhere
/// in the request beats `on` anywhere in it. Scanning the fields once and taking
/// whichever key matched first would flip that for a request carrying both.
fn first_of(fields: &[cc_config::form::Field], names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    })
}

/// The query string of `uri`, or `""`.
///
/// `EspHttpConnection::uri()` (`esp-idf-svc` `src/http/server.rs:949-955`)
/// returns `httpd_req_t::uri`, and that field is the **whole** request target
/// including the `?…` — `esp_http_server` reads the query back out of it with
/// `r->uri + res->field_data[UF_QUERY].off` (`httpd_parse.c:992`) rather than
/// from a member of its own. So the query string needs no `esp-idf-sys` call to
/// reach, which is what `cc_config::form`'s module documentation used to say the
/// opposite of; the split is at the first `?` and everything after it.
fn query_of(uri: &str) -> &str {
    uri.split_once('?').map_or("", |(_, query)| query)
}

/// What one `POST /api/parameters` resolved to, before anything is written.
///
/// The C++'s `hasErrors` / `hasUpdates` pair (`WebServerManager.cpp:826-827`),
/// kept as a value so the handler's decision — which of three response bodies to
/// send, and whether to emit a command at all — is a function that can be tested
/// without a socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParameterPost {
    /// At least one parameter was rejected. The C++ answers `400` and does not
    /// report which, but the pairs that *were* accepted are still written:
    /// `apply` walks the request in order and only collects the failures.
    Rejected {
        /// The pairs that passed. Empty when all failed.
        accepted: Vec<(String, String)>,
        /// One line per rejection, for the log.
        reasons: Vec<String>,
    },
    /// Everything was written.
    Updated {
        /// Every pair in the request, validated.
        accepted: Vec<(String, String)>,
    },
    /// Nothing in the request named a parameter with a value, so nothing was
    /// written. The C++'s `"No parameters updated"` (`WebServerManager.cpp:877`).
    Nothing,
}

impl ParameterPost {
    /// The status code and the body, verbatim from the C++.
    ///
    /// * rejected → `400 {"error":"Some parameter updates failed"}` (`:868`)
    /// * updated → `200 {"success":true,"message":"Parameters updated and
    ///   saved"}` (`:874-875`)
    /// * nothing → `200 {"success":true,"message":"No parameters updated"}`
    ///   (`:877`)
    #[must_use]
    pub fn response(&self) -> (u16, &'static str) {
        match self {
            Self::Rejected { .. } => (400, "{\"error\":\"Some parameter updates failed\"}"),
            Self::Updated { .. } => (
                200,
                "{\"success\":true,\"message\":\"Parameters updated and saved\"}",
            ),
            Self::Nothing => (
                200,
                "{\"success\":true,\"message\":\"No parameters updated\"}",
            ),
        }
    }

    /// The accepted pairs that will not affect the running machine until a
    /// reboot.
    ///
    /// Most parameters are read from `Config` on every tick, so a write takes
    /// effect immediately — the control task pushes the ones the reducer caches
    /// (`pid.enabled`, `brew.setpoint`) into `cc_machine::Machine` explicitly.
    /// A few are read **once**, at bring-up: which switches exist
    /// (`hardware.switches.*.enabled` → `SwitchBank::new`), whether a scale is
    /// fitted, whether the tank float is fitted. Those cannot change under a
    /// running machine, and the only honest thing is to say so.
    ///
    /// The C++ does not say it, because in the C++ a switch's enable flag is read
    /// by `SystemInitializer` at boot too — the same limitation, reported as
    /// silence. This firmware names the keys, which is the difference between
    /// "my setting vanished" and a diagnosis.
    #[must_use]
    pub fn reboot_required(&self) -> Vec<&str> {
        let pairs = match self {
            Self::Rejected { accepted, .. } | Self::Updated { accepted } => accepted,
            Self::Nothing => return Vec::new(),
        };
        pairs
            .iter()
            .map(|(key, _)| key.as_str())
            .filter(|key| needs_reboot(key))
            .collect()
    }

    /// The pairs to hand to the control task, if any.
    ///
    /// Non-empty for both outcomes that wrote something: a `400` that rejected
    /// one of six parameters still applies the other five, and dropping them
    /// would make the response and the machine disagree.
    #[must_use]
    pub fn into_pairs(self) -> Vec<(String, String)> {
        match self {
            Self::Rejected { accepted, .. } | Self::Updated { accepted } => accepted,
            Self::Nothing => Vec::new(),
        }
    }
}

/// Whether a parameter is read once at bring-up, so a write needs a reboot.
///
/// The rule is "does `SwitchBank::new` / the sensor bring-up read it", not a
/// guess: `hardware.switches.*.enabled` decides whether `poll` emits an edge at
/// all (`switches.rs:220-241` prints "the reducer will IGNORE this switch"), and
/// the switch bank is constructed once in `bring_up` and never rebuilt. The two
/// sensor flags are the same shape — `hardware.sensors.watertank.enabled` becomes
/// `SwitchBank::tank_fitted` (`switches.rs:196`) and
/// `hardware.sensors.scale.enabled` decides whether a sampler exists at all
/// (`main.rs:1362`).
///
/// `system.auth.*` is the same kind of thing for a different reason, and it is
/// the one the C++ cannot report: the C++ installs its authentication
/// middleware in `setupMiddleware`, which runs once from
/// `WebServerManager::initialize` (`WebServerManager.cpp:272-296`), so
/// enabling `system.auth.enabled` protects **nothing** until the machine
/// restarts — an operator who sets it, sees `200`, and is still serving an open
/// API. `Auth::from_config` reproduces the C++'s boot-time decision, so this
/// list has to say so, and `requiresRebootKeys` in the `POST /api/parameters`
/// answer is where it says it.
///
/// Everything else is read from `Config` per tick or per event, so a write is
/// live. That includes `pid.enabled`, which the control task pushes into the
/// machine explicitly — see the `POST /api/parameters` drain in `main.rs`.
fn needs_reboot(key: &str) -> bool {
    key.starts_with("hardware.switches.")
        || key.starts_with("hardware.sensors.watertank.enabled")
        || key == "hardware.sensors.scale.enabled"
        || key.starts_with("system.auth.")
        // **The probe type decides which driver is constructed**, so it is read
        // once at boot like every other `hardware.*` setting. It was missing
        // from this list, which is how a saved `TSIC_306` on a `DS18B20` board
        // looked applied while the machine carried on reading the other bus —
        // the operator changes it, the API says `success`, and nothing happens
        // until a reboot. The C++ has the same property (it builds
        // `TempSensorDallas` or `TempSensorTSIC` in `SystemInitializer`) and its
        // UI has to say so by hand; here the answer is the list.
        || key == "hardware.sensors.temperature.type"
}

/// The most pairs one `POST /api/parameters` may carry.
///
/// The C++ has no bound: it iterates `request->params()` and a client can send
/// ten thousand. Here the pairs are staged in a heap `Vec` on a 320 KB machine
/// and each one is a `String` pair, so an unbounded request is a
/// denial-of-service with one `curl`. [`crate::task::STAGED_PARAMETER_DEPTH`]
/// requests times this is 256 pairs — eight times the 98 the firmware registers,
/// so no legitimate request is refused, and the body is bounded at 4 KB by
/// [`drain_body`] long before the count is reached.
pub const MAX_PARAMETER_PAIRS: usize = 64;

/// The most bytes one `POST /api/parameters` body may be.
///
/// 1024, and the reason it is not the 256 every other body gets is that this
/// body is a *list*: `hardware.sensors.watertank.keep_heater_on_empty=1` is 51
/// characters, so 256 fits five of them. A settings form that cannot be submitted
/// is the reason an operator ends up using `curl` sixteen times, so this is
/// generous — twenty parameters, or the whole of a small machine's switches — and
/// still three orders of magnitude below the heap it could otherwise take. The
/// query string has its own, smaller, bound that this firmware does not set:
/// `CONFIG_HTTPD_MAX_URI_LEN`, 512 bytes by default
/// (`esp_http_server.h:377`).
pub const MAX_PARAMETER_BODY_BYTES: usize = 1024;

/// The most bytes one `POST /api/config/upload` body may be.
///
/// 16 KB — `MAX_CONFIG_UPLOAD_SIZE` at `WebServerManager.cpp:48`, which the
/// C++ passes to `jsonHandler.setMaxContentLength` (`:762`). That is the
/// **transport** limit; the *policy* limit is `cc_config::MAX_CONFIG_BYTES`
/// (8 KB), which is what a document over it is refused with. Two limits on
/// purpose: the C++'s is the largest request it will accept off the wire, and
/// the Rust one is the largest configuration the store can hold, so a body
/// between the two gets the clearer of the two answers rather than being cut
/// off.
pub const MAX_CONFIG_UPLOAD_BYTES: usize = 16 * 1024;

/// Decide what a `POST /api/parameters` means, without writing anything.
///
/// The C++'s loop over `request->params()` (`WebServerManager.cpp:829-865`),
/// with the same two rules: a field with no name or no value is **skipped**
/// rather than rejected (`:830`), and a rejected value does not undo the
/// accepted ones.
///
/// The pairs are validated here because the handler has to answer `200` or `400`
/// **synchronously**, and the `Config` is the control task's. The control task
/// then applies the pairs with `cc_config::assign::apply`, which validates them
/// again — through the same [`cc_config::assign::parse`], so the two verdicts
/// cannot disagree, and the second is an idempotent re-application rather than a
/// second rule.
#[must_use]
pub fn classify_parameters(pairs: &[cc_config::form::Field]) -> ParameterPost {
    let mut accepted = Vec::new();
    let mut reasons = Vec::new();
    for (key, raw) in pairs {
        // `:830` — `p->name().length() > 0 && p->value().length() > 0`. A
        // valueless field is "not mentioned", which is also why a text parameter
        // cannot be set to the empty string over HTTP.
        if key.is_empty() || raw.is_empty() {
            continue;
        }
        match cc_config::assign::parse(key, raw) {
            Ok(_) => accepted.push((key.clone(), raw.clone())),
            Err(err) => reasons.push(format!("{key}: {err}")),
        }
    }
    if !reasons.is_empty() {
        ParameterPost::Rejected { accepted, reasons }
    } else if accepted.is_empty() {
        ParameterPost::Nothing
    } else {
        ParameterPost::Updated { accepted }
    }
}

/// Read a request body into a `String`, bounded.
///
/// [`crate::telnet::LINE_BUFFER_BYTES`] (256), the same bound the log stream
/// uses. A `POST /api/setpoint` body is `value=94.5` — nine characters — and a
/// body that does not fit is a client bug or an attack, not a large legitimate
/// request. The C++ has no bound at all (`AsyncWebServerRequest` will buffer
/// whatever it is sent), which on a 320 KB heap is a denial of service with three
/// words.
fn drain_body(conn: &mut EspHttpConnection<'_>) -> String {
    drain_body_bounded(conn, crate::telnet::LINE_BUFFER_BYTES)
}

/// Read a request body into a `String`, bounded by `limit`.
///
/// A second bound rather than one, because the two routes that read a body need
/// different ones: a `POST /api/pid` body is six characters and a
/// `POST /api/parameters` body is a *list* of them, twenty of which is a
/// plausible settings form. One bound for both would be either too small for the
/// second or too generous for the first, and the first is the one a stranger can
/// reach.
fn drain_body_bounded(conn: &mut EspHttpConnection<'_>, limit: usize) -> String {
    drain_body_checked(conn, limit).unwrap_or_default()
}

/// Read a request body, **reporting** whether it did not fit.
///
/// [`drain_body_bounded`] truncates silently, which is the right answer for a
/// form body — a truncated `a=1&b=2` is a request that sets fewer parameters,
/// and `POST /api/parameters` re-validates every pair it acts on, so nothing
/// wrong reaches the machine.
///
/// It is the **wrong** answer for `POST /api/config/upload`, and the difference
/// is the whole reason this exists. An upload is a *document*: it is applied
/// key by key, and a document cut short mid-way is a document whose keys are
/// individually valid and collectively a different machine. Truncating at the
/// cap and handing the remainder to the reader would apply **half a
/// configuration** — a new PID gain with the old emergency cutoff — to a machine
/// that may be mid-shot. So this returns `None` the moment the body exceeds
/// `limit`, and the caller refuses without parsing.
fn drain_body_checked(conn: &mut EspHttpConnection<'_>, limit: usize) -> Option<String> {
    let mut body = String::new();
    let mut buf = [0u8; 128];
    loop {
        match conn.read(&mut buf) {
            Ok(0) | Err(_) => return Some(body),
            Ok(n) => {
                if body.len() + n > limit {
                    return None;
                }
                body.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        }
    }
}

/// The first form field named `name` in `body`.
///
/// See [`cc_config::form`] for the encoding, and [`query_of`] for why the query
/// string is read too: an earlier revision of this file claimed
/// `EspHttpConnection::uri()` returned the path only, which is wrong.
#[cfg(any(test, feature = "device-tests"))]
fn first_field(body: &str, name: &str) -> Option<String> {
    cc_config::form::field(body, name)
}

impl Sse {
    /// Note that a push was attempted, whether or not a client received it.
    ///
    /// Counted so that "the control task is producing events" and "a client is
    /// receiving them" are two different numbers.
    pub fn note_push_attempt(&self) {
        self.pushed.fetch_add(1, Ordering::Relaxed);
    }

    /// Push a frame to every connected client.
    ///
    /// The C++'s `AsyncEventSource::send` (`WebServerManager.cpp:1163`): every
    /// producer pushes, and every client receives. It cannot write from the
    /// caller's task — the sockets belong to the broadcaster — so it hands the
    /// frame over through a bounded mailbox, and the broadcaster drains it on
    /// its next pass (at most [`SSE_POLL_MS`] later).
    ///
    /// A full mailbox drops the **newest** frame and counts it in
    /// [`Sse::dropped_frames`], rather than blocking the caller. The caller is
    /// the control task, and blocking it to serve a browser tab is the same
    /// mistake as blocking the httpd task, one layer over.
    pub fn broadcast(&self, frame: String) {
        let Ok(mut mailbox) = self.mailbox.lock() else {
            self.dropped_frames.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if mailbox.len() == SSE_MAILBOX_DEPTH && mailbox.pop_front().is_some() {
            self.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
        mailbox.push_back(frame);
    }
}

/// Build the [`Configuration`] for this firmware.
///
/// `stack_size` is raised from the default 6144 (`server.rs:141`) to 8192
/// because the SSE handler formats JSON on the httpd task's stack, and
/// `serde_json`'s formatter wants more than 6 KB of headroom on a task that also
/// has to be able to unwind. `keep_alive` is on with a 5 s idle timeout
/// (`KeepAlive::new()`, `server.rs:88-95`) so a browser's pooled connection
/// survives between API calls.
#[must_use]
pub fn configuration() -> Configuration {
    Configuration {
        http_port: HTTP_PORT,
        max_uri_handlers: MAX_URI_HANDLERS,
        max_open_sockets: MAX_OPEN_SOCKETS,
        stack_size: 8192,
        keep_alive: Some(esp_idf_svc::http::server::KeepAlive::new()),
        // Required by the single `/ui*` handler: without it ESP-IDF compares
        // URIs exactly and no path under `/ui` is ever routed, so the SPA would
        // 404 on its own assets. It is a server-wide switch, so it was checked
        // rather than assumed -- see [`serve_ui`] for why the other 23
        // registrations stay exact.
        uri_match_wildcard: true,
        ..Default::default()
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

    #[cfg_attr(test, test)]
    pub fn the_status_payload_has_the_csqs_keys() {
        // WebServerManager.cpp:356-372. The names are a wire format the React
        // UI reads, so they are pinned here rather than left to review.
        let t = Telemetry {
            temperature_c: 93.5,
            setpoint_c: 94.0,
            heater_power_pct: 42.0,
            machine_state: 20,
            uptime_ms: 1_234,
            ..Telemetry::default()
        };
        let json = status_json(&t);
        for key in [
            "\"temperature\"",
            "\"setpoint\"",
            "\"heaterPower\"",
            "\"machineState\"",
            "\"isStandby\"",
            "\"standbyTime\"",
            "\"pidEnabled\"",
            "\"steamMode\"",
            "\"brewing\"",
            "\"uptime\"",
            "\"shotsSinceBackflush\"",
            "\"backflushReminderThreshold\"",
            "\"backflushReminderDue\"",
        ] {
            assert!(json.contains(key), "{key} missing from {json}");
        }
        assert!(json.contains("\"temperature\":93.50"), "{json}");
        assert!(json.contains("\"machineState\":20"), "{json}");
        assert!(json.contains("\"uptime\":1234"), "{json}");
    }

    #[cfg_attr(test, test)]
    pub fn an_absent_reading_is_null_and_never_a_fabricated_zero() {
        // A scale that is not fitted, or has not produced a reading, must not
        // report 0 g: 0 g is a real weight and the UI would show a full cup.
        let json = status_json(&Telemetry::default());
        assert!(json.contains("\"weight\":null"), "{json}");
        assert!(json.contains("\"brewWeight\":null"), "{json}");
        assert!(json.contains("\"pressure\":null"), "{json}");
        assert!(json.contains("\"waterTankFull\":null"), "{json}");
    }

    #[cfg_attr(test, test)]
    pub fn a_present_reading_is_formatted_to_two_decimals() {
        let t = Telemetry {
            weight_g: Some(12.5),
            brew_weight_g: Some(0.0),
            ..Telemetry::default()
        };
        let json = status_json(&t);
        assert!(json.contains("\"weight\":12.50"), "{json}");
        // A real zero is 0.00, not null: the two are distinguishable.
        assert!(json.contains("\"brewWeight\":0.00"), "{json}");
    }

    #[cfg_attr(test, test)]
    pub fn the_status_payload_is_valid_json() {
        let t = Telemetry {
            ip: heapless::String::<15>::try_from("192.168.1.42").ok(),
            water_tank_full: Some(true),
            ..Telemetry::default()
        };
        let json = status_json(&t);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok(), "{json} is not valid JSON");
    }

    #[cfg_attr(test, test)]
    pub fn the_temperatures_payload_is_the_csqs_three_keys() {
        // getTempString, WebServerManager.cpp:1180-1182. The same shape is the
        // `new_temps` SSE event, so the UI has one parser.
        let t = Telemetry {
            temperature_c: 91.25,
            setpoint_c: 94.0,
            heater_power_pct: 17.5,
            ..Telemetry::default()
        };
        let json = temperatures_json(&t);
        assert_eq!(
            json,
            "{\"currentTemp\":91.25,\"targetTemp\":94.00,\"heaterPower\":17.50}"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_health_payload_distinguishes_alive_from_published() {
        let json = health_json(&Telemetry {
            uptime_ms: 42,
            machine_state: 20,
            ..Telemetry::default()
        });
        assert!(json.contains("\"status\":\"ok\""), "{json}");
        assert!(json.contains("\"uptime\":42"), "{json}");
    }

    #[cfg_attr(test, test)]
    pub fn nvs_debug_reports_the_blob_and_the_heap_and_no_parameters() {
        // The C++ reported counts and heap figures only (WebServerManager.cpp:659-666)
        // and the credentials are in the same blob, so a payload listing
        // parameters would be a disclosure with no local symptom.
        let json = nvs_debug_json("cc/cc.config: schema v1, 2071 B JSON", 96);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["parameters_count"], 96);
        assert_eq!(parsed["metadata"]["nvs_namespace"], "cc");
        assert_eq!(parsed["metadata"]["blob_schema_version"], 1);
        assert_eq!(parsed["metadata"]["blob_bytes"], 2071);
        assert!(parsed["metadata"]["free_heap"].as_u64().is_some());
        assert!(parsed["metadata"]["min_free_heap"].as_u64().is_some());
        assert_eq!(parsed["parameters"].as_array().map(Vec::len), Some(0));
    }

    #[cfg_attr(test, test)]
    pub fn an_empty_blob_describes_as_zeroes_rather_than_panicking() {
        let json = nvs_debug_json("cc/cc.config: empty", 0);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["metadata"]["blob_schema_version"], 0);
        assert_eq!(parsed["metadata"]["blob_bytes"], 0);
    }

    #[cfg_attr(test, test)]
    pub fn every_csqs_api_route_is_registered() {
        // 20 /api/* from WebServerManager.cpp:327-812, plus `/api/config/download`
        // (`:706`), the four OTA routes (`ota.cpp:847-866`), and `/`, `/ui` and
        // `/events`.
        //
        // This list is the parity contract with `ui/packages/frontend/src/lib/
        // routes.ts`, the frontend's own `API_ROUTES` table. A route the UI can
        // call and this server does not register is a 404 in the browser and a
        // blank page in the UI, so the omission is the bug this test exists to
        // prevent.
        let routes = routes();
        for expected in [
            "/api/status",
            "/api/health",
            "/api/temperatures",
            "/api/history",
            "/api/nvs-debug",
            "/api/parameter-help",
            "/api/config",
            "/api/config/download",
            "/api/parameters",
            "/api/setpoint",
            "/api/steam",
            "/api/pid",
            "/api/backflush",
            "/api/sleep",
            "/api/wake",
            "/api/scale/tare",
            "/api/scale/calibration",
            "/api/maintenance/reset-backflush-counter",
            "/api/wifi-reset",
            "/api/factory-reset",
            "/api/restart",
            "/api/ota/status",
            "/api/ota/firmware",
            "/api/ota/filesystem",
            "/api/ota/url",
        ] {
            assert!(
                routes.iter().any(|(path, _)| *path == expected),
                "{expected} is missing"
            );
        }
    }

    /// The frontend's `API_ROUTES` table, verbatim from
    /// `ui/packages/frontend/src/lib/routes.ts`.
    ///
    /// Transcribed here rather than parsed at build time so that adding a route
    /// to one side and forgetting the other is a **test failure** and not a 404
    /// somebody finds by clicking. `getApiRoute` prefixes `/api`, so each entry
    /// is compared with that prefix applied.
    #[cfg_attr(test, test)]
    pub fn every_route_the_frontend_calls_is_registered() {
        const FRONTEND: &[&str] = &[
            "/api/steam",
            "/api/pid",
            "/api/backflush",
            "/api/setpoint",
            "/api/wake",
            "/api/sleep",
            "/api/config",
            "/api/config/download",
            "/api/parameters",
            "/api/parameter-help",
            "/api/status",
            "/api/health",
            "/api/temperatures",
            "/api/history",
            "/api/scale/tare",
            "/api/scale/calibration",
            "/api/ota/status",
            "/api/ota/firmware",
            "/api/ota/filesystem",
            "/api/ota/url",
            "/api/restart",
            "/api/factory-reset",
            "/api/wifi-reset",
            "/api/nvs-debug",
            "/api/maintenance/reset-backflush-counter",
        ];
        let routes = routes();
        for path in FRONTEND {
            assert!(
                routes.iter().any(|(registered, _)| registered == path),
                "the UI calls {path} but no handler is registered for it"
            );
        }
    }

    // ==================================================== the setpoint route

    #[cfg_attr(test, test)]
    pub fn the_setpoint_route_takes_what_the_schema_will_store() {
        // The bound is `cc_config::assign::parse`'s, not a repeated literal, so
        // this cannot drift from `brew.setpoint`'s `ParamSpec` range. The pairs
        // are the schema's two edges (`Config.h:795-802`, `defaults.h:81-82`)
        // and the value the integration checklist posts, spelled both ways.
        for (field, expected) in [
            ("20", 20),
            ("20.0", 20),
            ("95", 95),
            ("95.0", 95),
            ("110", 110),
            ("110.0", 110),
        ] {
            assert_eq!(
                parse_setpoint(field),
                Some(Command::SetSetpoint(expected)),
                "{field} is inside the schema's range and must be accepted"
            );
        }
    }

    #[cfg_attr(test, test)]
    pub fn the_setpoint_route_refuses_a_value_that_would_defeat_the_interlock() {
        // The defect this route carried: `?value=150` passed the C++'s
        // permissive `0.0..=150.0` filter and was persisted, which parks the
        // boiler on `safety.emergency_temp` (default 150) where S1's *strictly
        // greater* test can never trip. A `None` here is a `400` from
        // `register_command`, not a silently clamped write. The rest are the
        // values that stop being numbers at all — `NaN` and `inf` included,
        // which every comparison lets through.
        for field in [
            "150", "150.0", "200", "-1", "0", "110.5", "1e400", "NaN", "hot",
        ] {
            assert_eq!(
                parse_setpoint(field),
                None,
                "{field} must be refused: outside the schema's 20.0..=110.0, or not \
                 a finite number"
            );
        }
    }

    // ==================================================== the toggle routes

    #[cfg_attr(test, test)]
    pub fn a_flag_reads_the_csqs_spellings_and_treats_anything_else_as_off() {
        // The C++ compares the string "1" (`if (value == "1")`), so "1" is the
        // spelling that must work. The extras are this firmware's documented
        // superset.
        assert!(parse_flag("1"));
        assert!(parse_flag("true"));
        assert!(parse_flag("on"));
        assert!(!parse_flag("0"));
        assert!(!parse_flag("false"));
        assert!(!parse_flag("off"));
        assert!(!parse_flag("banana"));
    }

    #[cfg_attr(test, test)]
    pub fn an_explicit_toggle_command_carries_the_value_it_was_given() {
        // The explicit forms must survive the move from `register_command` to
        // `register_toggle`: `?on=0` and body `value=1` are how the human and
        // the integration checklist spell them.
        assert!(!explicit_value(&Command::SetPid(false)));
        assert!(explicit_value(&Command::SetPid(true)));
        assert!(!explicit_value(&Command::SetSteam(false)));
        assert!(!explicit_value(&Command::SetBackflush(false)));
    }

    #[cfg_attr(test, test)]
    pub fn a_toggle_route_inverts_the_published_value() {
        // The bare-POST path: no field, so the target is `!current`. These are
        // the three `current` functions the route table actually registers, so a
        // change that points a route at the wrong telemetry field fails here.
        fn pid(t: &Telemetry) -> bool {
            t.pid_enabled
        }
        fn steam(t: &Telemetry) -> bool {
            t.steam_mode
        }
        fn backflush(t: &Telemetry) -> bool {
            t.backflush_mode
        }

        let off = Telemetry::default();
        let on = Telemetry {
            pid_enabled: true,
            steam_mode: true,
            backflush_mode: true,
            ..Telemetry::default()
        };
        // Each field is read from its own field, so turning one on must not make
        // another's toggle think it is already on.
        assert!(!pid(&off) && pid(&on));
        assert!(!steam(&off) && steam(&on));
        assert!(!backflush(&off) && backflush(&on));

        let only_steam = Telemetry {
            steam_mode: true,
            ..Telemetry::default()
        };
        assert!(!pid(&only_steam), "the PID toggle must not read steam_mode");
        assert!(
            !backflush(&only_steam),
            "the backflush toggle must not read steam_mode"
        );
    }

    // ==================================================== OTA (R3-15, deferred)

    #[cfg_attr(test, test)]
    pub fn the_ota_status_document_satisfies_the_uis_schema() {
        // `OtaStatusSchema` (`ui/.../lib/schemas.ts:59-72`) **requires** status,
        // progress and updateInProgress. If any is missing, `pollOtaStatus`
        // returns null and the OTA page cannot render at all — so this is the
        // test that keeps the page working on a build with no OTA.
        let json = ota_status_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["status"], "idle");
        assert_eq!(parsed["progress"], 0);
        assert_eq!(parsed["updateInProgress"], false);
        assert_eq!(parsed["updating"], false);
        // "not available" must be visible, not merely implied by an idle status.
        assert_eq!(parsed["reason"], "R3-15");
        let message = parsed["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("not available"),
            "the message must say OTA is absent: {message}"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_ota_status_document_is_not_an_update_error() {
        // The C++ only emits `error` when an update actually failed
        // (`ota.cpp:744-751`). "OTA was never built" is not a failed update, and
        // reporting it as one would make the UI show a failure toast on a
        // machine that has simply never had OTA.
        let json = ota_status_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(parsed.get("error").is_none(), "{json}");
    }

    #[cfg_attr(test, test)]
    pub fn an_unavailable_ota_route_says_which_build_and_which_task() {
        let json = unavailable_json("OTA", "R3-15");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(parsed["error"]
            .as_str()
            .unwrap_or_default()
            .contains("not available"));
        assert_eq!(parsed["reason"], "R3-15");
    }

    // ============================================ POST /api/parameters honesty

    #[cfg_attr(test, test)]
    pub fn a_write_that_needs_a_reboot_is_named_rather_than_claimed_applied() {
        // The lie the human reported: "success" for a write that cannot change
        // the running machine. `hardware.switches.brew.enabled` is read once by
        // `SwitchBank::new`, so it is in this list.
        let verdict =
            classify_parameters(&[("hardware.switches.brew.enabled".into(), "true".into())]);
        assert_eq!(
            verdict.reboot_required(),
            vec!["hardware.switches.brew.enabled"]
        );
        // And it is still a 200 with the C++'s message — the write *was*
        // accepted and persisted; only the runtime effect is deferred.
        assert_eq!(verdict.response().0, 200);
    }

    #[cfg_attr(test, test)]
    pub fn an_ordinary_parameter_is_not_reported_as_needing_a_reboot() {
        // `pid.enabled` is pushed into the running machine by the control task,
        // so it must NOT appear in this list — otherwise every ordinary write
        // would be reported as deferred and the warning would be worthless.
        let verdict = classify_parameters(&[("pid.enabled".into(), "1".into())]);
        assert!(
            verdict.reboot_required().is_empty(),
            "{:?}",
            verdict.reboot_required()
        );
        assert!(!needs_reboot("pid.enabled"));
        assert!(!needs_reboot("brew.setpoint"));
        assert!(!needs_reboot("pid.regular.kp"));
    }

    #[cfg_attr(test, test)]
    pub fn a_rejected_write_still_names_the_reboot_keys_it_did_accept() {
        // A 400 that applied five of six parameters is still a write, and the
        // reboot keys among the five are still deferred.
        let verdict = classify_parameters(&[
            ("pid.enabled".into(), "1".into()),
            ("hardware.switches.steam.enabled".into(), "true".into()),
            ("pid.regular.kp".into(), "not-a-number".into()),
        ]);
        assert_eq!(verdict.response().0, 400);
        assert_eq!(
            verdict.reboot_required(),
            vec!["hardware.switches.steam.enabled"]
        );
    }

    #[cfg_attr(test, test)]
    pub fn a_write_that_changed_nothing_needs_no_reboot() {
        // `ParameterPost::Nothing` is the C++'s "No parameters updated"
        // (`WebServerManager.cpp:877`), which is reached when the request names
        // no parameter *with a value* — the `:830` skip, not a rejection. An
        // unparseable value is `Rejected`, not `Nothing`, so it is the empty
        // field that gets here.
        let verdict = classify_parameters(&[("pid.regular.kp".into(), String::new())]);
        assert!(matches!(verdict, ParameterPost::Nothing));
        assert!(verdict.reboot_required().is_empty());

        // And the distinction is real: a bad *value* is a 400, not a no-op.
        let rejected = classify_parameters(&[("pid.regular.kp".into(), "banana".into())]);
        assert!(matches!(rejected, ParameterPost::Rejected { .. }));
        assert_eq!(rejected.response().0, 400);
    }

    #[cfg_attr(test, test)]
    pub fn the_route_table_fits_the_servers_handler_budget() {
        // If this ever grows past MAX_URI_HANDLERS the server will fail to start
        // with ESP_ERR_HTTPD_HANDLERS_FULL, at boot, which is a bad place to find
        // out.
        assert!(routes().len() <= MAX_URI_HANDLERS);
    }

    // ============================================== /api/status's steamMode

    #[cfg_attr(test, test)]
    pub fn steam_mode_is_the_latched_steam_flag_and_not_the_brew_state() {
        // The defect this pins. The C++'s `steamMode` is
        // `MachineStateContext::steamON_` -- set in
        // `SteamRunningState::onEntryImpl` (`SteamStates.cpp:16`), cleared in its
        // `onExitImpl` (`:21`) and in `StandbyState::onEntryImpl`
        // (`SystemStates.cpp:17`). This port emitted `"brewing"`
        // -- `state.is_brew_state()`, a *derived* value with no C++ counterpart --
        // under the C++'s name, so a poll answered a steam-mode question with a
        // brew-state answer.
        let steaming = status_json(&Telemetry {
            steam_mode: true,
            brewing: false,
            ..Telemetry::default()
        });
        assert!(steaming.contains("\"steamMode\":true"), "{steaming}");
        assert!(steaming.contains("\"brewing\":false"), "{steaming}");

        let brewing_not_steaming = status_json(&Telemetry {
            steam_mode: false,
            brewing: true,
            ..Telemetry::default()
        });
        assert!(
            brewing_not_steaming.contains("\"steamMode\":false"),
            "{brewing_not_steaming}"
        );
        assert!(
            brewing_not_steaming.contains("\"brewing\":true"),
            "{brewing_not_steaming}"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_status_steam_mode_agrees_with_the_steam_toggle_response() {
        // `POST /api/steam` answers `{"success":true,"steamMode":<bool>}` from
        // `Telemetry::steam_mode` (`WebServerManager.cpp:444-475`), and the UI
        // reads that field (`machine-toggle-result.ts:9`). If `/api/status`
        // reported a different fact under the same name, a poll would contradict
        // the toggle that set it -- which is the class of bug this file already
        // records once, for the toggle reading its value before the command was
        // applied.
        let t = Telemetry {
            steam_mode: true,
            brewing: true,
            ..Telemetry::default()
        };
        let json = status_json(&t);
        let from_toggle = format!("{{\"success\":true,\"steamMode\":{}}}", t.steam_mode);
        assert!(json.contains(&from_toggle), "{json} vs {from_toggle}");
    }

    // ============================================== HTTP Basic authentication

    /// A `Config` with the given `system.auth.*`, for the auth cases.
    fn auth_config(enabled: bool, username: &str, password: &str) -> Arc<Config> {
        let mut config = Config::default();
        config.system.auth.enabled = enabled;
        config.system.auth.username = username.to_string();
        config.system.auth.password = cc_domain::secret::Secret::new(password.to_string());
        Arc::new(config)
    }

    /// base64, so a test states a credential rather than a blob.
    fn basic(user: &str, pass: &str) -> String {
        format!(
            "Basic {}",
            cc_domain::http_auth::tests_support::encode(format!("{user}:{pass}").as_bytes())
        )
    }

    #[cfg_attr(test, test)]
    pub fn auth_is_inert_when_it_is_switched_off() {
        // The default. `system.auth.enabled` is `false`
        // (`cc-config/src/schema.rs:690-694`), so an operator who has never
        // touched it sees exactly the firmware they had before this existed.
        let auth = Auth::from_config(&auth_config(false, "admin", "admin"));
        assert!(!auth.is_enforced());
        assert!(auth.admits(None), "no header at all must pass");
        assert!(auth.admits(Some("Basic Zm9vOmJhcg==")), "a wrong one too");
    }

    #[cfg_attr(test, test)]
    pub fn auth_admits_the_right_credentials_and_refuses_everything_else() {
        let auth = Auth::from_config(&auth_config(true, "barista", "espresso"));
        assert!(auth.is_enforced());
        assert!(auth.admits(Some(&basic("barista", "espresso"))));
        assert!(!auth.admits(None), "no header");
        assert!(
            !auth.admits(Some(&basic("barista", "wrong"))),
            "wrong password"
        );
        assert!(
            !auth.admits(Some(&basic("wrong", "espresso"))),
            "wrong username"
        );
        assert!(!auth.admits(Some("Basic not-base64!")), "malformed");
        assert!(!auth.admits(Some("Bearer whatever")), "wrong scheme");
    }

    #[cfg_attr(test, test)]
    pub fn auth_with_empty_credentials_serves_the_api_open_exactly_as_the_cpp_does() {
        // `WebServerManager.cpp:283-294`: the middleware is installed only when
        // the username AND the password are non-empty, and the else arm is a
        // `LOG(WARNING)` -- not a locked door. Reproduced deliberately; the
        // alternative locks an operator out of a machine whose only other
        // console is a UART. See `docs/rust-migration/intentional-diffs.md`.
        for (username, password) in [("", "espresso"), ("barista", ""), ("", "")] {
            let auth = Auth::from_config(&auth_config(true, username, password));
            assert!(
                !auth.is_enforced(),
                "{username:?}/{password:?} must not challenge"
            );
            assert!(auth.admits(None));
        }
    }

    #[cfg_attr(test, test)]
    pub fn the_challenge_is_the_cpp_realm() {
        // `authMiddleware_->setRealm("CleverCoffee")` (`WebServerManager.cpp:286`).
        // A different realm is a different challenge, and a browser that has
        // cached credentials for one will not send them for the other.
        assert_eq!(WWW_AUTHENTICATE, "Basic realm=\"CleverCoffee\"");
    }

    #[cfg_attr(test, test)]
    pub fn enabling_auth_is_reported_as_needing_a_reboot() {
        // The C++ installs its middleware once, in `setupMiddleware`, from
        // `WebServerManager::initialize`. Enabling `system.auth.enabled` therefore
        // protects nothing until the next boot -- and an operator who sets it,
        // sees `200`, and is still serving an open API is exactly the lie this
        // repository calls out. `requiresRebootKeys` is where it is said.
        let verdict = classify_parameters(&[("system.auth.enabled".into(), "1".into())]);
        assert_eq!(
            verdict.reboot_required(),
            vec!["system.auth.enabled"],
            "enabling authentication must be reported as needing a reboot"
        );
        // ... and a reboot is not claimed for a parameter that takes effect live.
        let live = classify_parameters(&[("brew.setpoint".into(), "95".into())]);
        assert!(live.reboot_required().is_empty());
    }

    #[cfg_attr(test, test)]
    pub fn the_auth_debug_never_prints_the_configuration() {
        // A derived `Debug` on a struct holding the whole `Config` would print
        // every parameter including the four credentials.
        let auth = Auth::from_config(&auth_config(true, "barista", "espresso"));
        let rendered = format!("{auth:?}");
        assert!(!rendered.contains("espresso"), "{rendered}");
        assert!(!rendered.contains("barista"), "{rendered}");
    }

    // ================================================ POST /api/config/upload

    #[cfg_attr(test, test)]
    pub fn the_config_upload_route_is_registered() {
        // `ui/packages/frontend/src/pages/SystemPage.tsx:182` posts here. It was
        // a live button with no route behind it, so every operator who clicked
        // "Upload configuration" got a 404.
        assert!(
            routes().contains(&("/api/config/upload", Method::Post)),
            "the upload route must be in the table"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_upload_response_is_the_cpp_shape() {
        // `sendConfigUploadResponse` (`WebServerManager.cpp:50-62`): three keys,
        // and `restart` mirrors `success` because the C++ does not reboot
        // itself -- it sets a flag and the browser calls `/api/restart`.
        assert_eq!(
            upload_response(true, "Configuration validated and applied successfully."),
            "{\"success\":true,\"message\":\"Configuration validated and applied successfully.\",\"restart\":true}"
        );
        assert_eq!(
            upload_response(false, "JSON body must be a top-level object"),
            "{\"success\":false,\"message\":\"JSON body must be a top-level object\",\"restart\":false}"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_upload_body_is_bounded_and_the_cap_is_the_cpps() {
        // `MAX_CONFIG_UPLOAD_SIZE = 16384` (`WebServerManager.cpp:48`), passed to
        // `setMaxContentLength` at `:762`.
        assert_eq!(MAX_CONFIG_UPLOAD_BYTES, 16 * 1024);
        // The policy limit underneath it is `cc_config`'s own 8 KB; the two are
        // different questions ("how much will I read off the wire" and "how big a
        // configuration may be") and a body between them gets the clearer answer.
        const {
            assert!(MAX_CONFIG_UPLOAD_BYTES > cc_config::MAX_CONFIG_BYTES);
        }
    }

    // ============================================================== CORS

    #[cfg_attr(test, test)]
    pub fn the_advertised_options_handler_is_a_wildcard_over_the_api() {
        // It used to be advertised as `("/api/status", Method::Options)` with no
        // `fn_handler` behind it anywhere, so a preflight 404'd while the boot
        // log claimed the route existed. See `register_preflight`.
        let options: Vec<&str> = routes()
            .iter()
            .filter(|(_, method)| *method == Method::Options)
            .map(|(uri, _)| *uri)
            .collect();
        assert_eq!(options, vec!["/api*"], "one real preflight route");
    }

    // ==================================================== POST /api/parameters

    #[cfg_attr(test, test)]
    pub fn the_parameter_route_is_registered_for_both_methods() {
        // The C++ registers it once as HTTP_ANY and branches on the method
        // (`WebServerManager.cpp:813-815`); two registrations is the closest
        // `esp-idf-svc` can get, because its `Method` has no "any"
        // (embedded-svc 0.29 `http.rs:17-52`).
        let routes = routes();
        assert!(routes.contains(&("/api/parameters", Method::Get)));
        assert!(routes.contains(&("/api/parameters", Method::Post)));
    }

    #[cfg_attr(test, test)]
    pub fn a_query_string_is_reachable_from_the_uri() {
        // `httpd_req_t::uri` is the whole request target, query included —
        // `esp_http_server` reads the query back out of it at
        // `r->uri + res->field_data[UF_QUERY].off` (`httpd_parse.c:992`), and
        // there is no `httpd_req_t::query` member (`esp_http_server.h:373-400`).
        // An earlier revision of `cc_config::form` claimed otherwise and said the
        // query string was unreachable without an `esp-idf-sys` call; this is the
        // test that says so.
        assert_eq!(query_of("/api/pid?on=0"), "on=0");
        assert_eq!(query_of("/api/pid"), "");
        assert_eq!(query_of("/api/parameters?a=1&b=2"), "a=1&b=2");
        // Only the first `?` splits, so a `?` inside the query stays put.
        assert_eq!(query_of("/x?a=1?b=2"), "a=1?b=2");
    }

    #[cfg_attr(test, test)]
    pub fn a_parameter_post_of_the_four_kinds_is_accepted() {
        // One of each kind, and the C++'s "all four spellings" for a bool.
        let verdict = classify_parameters(&[
            ("pid.enabled".into(), "1".into()),
            ("mqtt.port".into(), "1884".into()),
            ("pid.regular.kp".into(), "3.5".into()),
            ("system.hostname".into(), "kettle".into()),
        ]);
        assert_eq!(verdict.clone().into_pairs().len(), 4);
        assert!(
            matches!(verdict, ParameterPost::Updated { .. }),
            "{verdict:?}"
        );
        assert_eq!(verdict.response().0, 200);
    }

    #[cfg_attr(test, test)]
    pub fn an_unknown_key_is_a_400_and_names_nothing_in_the_body() {
        // The C++ answers `{"error":"Some parameter updates failed"}` for an
        // unknown key (`:856-859`, `:868`) and names nothing — the reasons go to
        // the log, which is what the handler does here too.
        let verdict = classify_parameters(&[("no.such.parameter".into(), "1".into())]);
        assert!(matches!(verdict, ParameterPost::Rejected { .. }));
        assert_eq!(
            verdict.response(),
            (400, "{\"error\":\"Some parameter updates failed\"}")
        );
        assert!(verdict.into_pairs().is_empty(), "nothing was written");
    }

    #[cfg_attr(test, test)]
    pub fn a_value_out_of_range_is_the_same_400_as_an_unknown_key() {
        // `:867-868` — one `hasErrors` flag covers both, so both are one status
        // and one body.
        for raw in ["-1e6", "1e6", "abc", "NaN"] {
            let verdict = classify_parameters(&[("pid.regular.kp".into(), raw.into())]);
            assert_eq!(
                verdict.response().0,
                400,
                "pid.regular.kp={raw:?} should be a 400"
            );
        }
    }

    #[cfg_attr(test, test)]
    pub fn one_rejected_parameter_does_not_lose_the_accepted_ones() {
        // `WebServerManager.cpp:829-865`: each pair is applied as the loop reaches
        // it, and `hasErrors` is only a flag. So the good pairs still travel to
        // the control task and the answer is still a 400.
        let verdict = classify_parameters(&[
            ("mqtt.port".into(), "1884".into()),
            ("pid.regular.kp".into(), "nope".into()),
            ("pid.enabled".into(), "true".into()),
        ]);
        let ParameterPost::Rejected { accepted, reasons } = &verdict else {
            panic!("expected a rejection, got {verdict:?}");
        };
        assert_eq!(accepted.len(), 2);
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].starts_with("pid.regular.kp:"), "{reasons:?}");
        assert_eq!(verdict.response().0, 400);
    }

    #[cfg_attr(test, test)]
    pub fn a_request_that_names_no_parameter_is_the_third_response() {
        // The C++'s third body, for a request whose fields are all valueless
        // (`:830` skips them) or which names nothing at all (`:876-878`).
        for pairs in [
            vec![],
            vec![("pid.enabled".into(), String::new())],
            vec![(String::new(), "1".into())],
        ] {
            let verdict = classify_parameters(&pairs);
            assert_eq!(verdict, ParameterPost::Nothing, "{pairs:?}");
            assert!(verdict.clone().into_pairs().is_empty());
            assert_eq!(
                verdict.response(),
                (
                    200,
                    "{\"success\":true,\"message\":\"No parameters updated\"}"
                )
            );
        }
    }

    #[cfg_attr(test, test)]
    pub fn the_updated_response_is_the_cpp_body_verbatim() {
        // `WebServerManager.cpp:874-875`.
        assert_eq!(
            ParameterPost::Updated {
                accepted: Vec::new()
            }
            .response(),
            (
                200,
                "{\"success\":true,\"message\":\"Parameters updated and saved\"}"
            )
        );
    }

    #[cfg_attr(test, test)]
    pub fn a_query_string_and_a_body_carry_the_same_parameter() {
        // `request->params()` (`:823`) is the query string and the body together,
        // and the handler merges them in that order — which is what makes
        // `curl -X POST '.../api/parameters?pid.enabled=1'` work.
        let mut fields = cc_config::form::parse_form(query_of("/api/parameters?pid.enabled=1"));
        fields.extend(cc_config::form::parse_form("pid.regular.kp=2.5"));
        assert_eq!(fields.len(), 2);
        assert!(matches!(
            classify_parameters(&fields),
            ParameterPost::Updated { .. }
        ));
    }

    #[cfg_attr(test, test)]
    pub fn a_command_field_is_read_from_the_query_string_as_well_as_the_body() {
        // `POST /api/pid?on=0` answered `400 {"error":"missing value"}` before,
        // because only the body was read. The C++'s `POST /api/pid` reads no
        // field at all (`:462-479`, a toggle), so there is no C++ answer for
        // `?on=0` to match; every script and the integration checklist spell it
        // that way, so both are accepted and the body wins.
        let from_query = cc_config::form::parse_form(query_of("/api/pid?on=0"));
        assert_eq!(first_of(&from_query, &["value", "on"]), Some("0".into()));
        let from_body = cc_config::form::parse_form("on=1");
        assert_eq!(first_of(&from_body, &["value", "on"]), Some("1".into()));
        // `value` beats `on` wherever each is — the C++'s
        // `hasParam("value", …)` then `hasParam("on", …)`.
        let both = cc_config::form::parse_form("on=1&value=7");
        assert_eq!(first_of(&both, &["value", "on"]), Some("7".into()));
        // Neither is a 400 rather than a default.
        assert_eq!(
            first_of(&cc_config::form::parse_form(""), &["value", "on"]),
            None
        );
        // And the single-field helper the C++'s `hasParam` is.
        assert_eq!(first_field("value=3&value=4", "value"), Some("3".into()));
        assert_eq!(first_field("a=1", "value"), None);
    }

    #[cfg_attr(test, test)]
    pub fn the_redirect_and_the_ui_are_routes() {
        let routes = routes();
        assert!(
            routes.contains(&("/", Method::Get)),
            "the / -> /ui/ redirect"
        );
        assert!(
            routes.contains(&("/ui*", Method::Get)),
            "the UI is one wildcard route: the shell, its assets and its \
             client-side routes all reach the same handler"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_sse_stream_is_a_route() {
        assert!(routes().contains(&("/events", Method::Get)));
    }

    // ---- SSE framing ---------------------------------------------------

    #[cfg_attr(test, test)]
    pub fn an_sse_frame_ends_with_a_blank_line() {
        // The WHATWG parser dispatches on the blank line. Without it the
        // browser buffers the frame and delivers nothing, forever, with no
        // error — the single most common way a hand-rolled SSE endpoint is
        // "broken".
        let frame = Sse::frame("new_temps", "{\"currentTemp\":90.0}");
        assert!(frame.starts_with("event: new_temps\n"), "{frame}");
        assert!(frame.contains("data: {"), "{frame}");
        assert!(frame.ends_with("\n\n"), "{frame}");
    }

    #[cfg_attr(test, test)]
    pub fn a_keepalive_is_a_comment_and_carries_no_event() {
        let frame = Sse::keepalive();
        assert!(frame.starts_with(':'), "{frame}");
        assert!(frame.ends_with("\n\n"), "{frame}");
        assert!(!frame.contains("event:"), "{frame}");
    }

    #[cfg_attr(test, test)]
    pub fn the_default_sse_mode_is_the_chunked_one() {
        // Chunked framing is the only mode left: it is the one that keeps
        // `send_wait_timeout` and the only one that can be written from a
        // detached request.
        assert_eq!(SseMode::default(), SseMode::Chunked);
        assert_eq!(Sse::new(SseMode::default()).mode(), SseMode::Chunked);
    }

    #[cfg_attr(test, test)]
    pub fn the_client_cap_leaves_sockets_for_the_api() {
        // The bound that keeps one browser tab from starving `/api/*`. ESP-IDF's
        // httpd has a fixed session pool and an SSE client holds its session for
        // as long as it is connected, so the cap is a socket budget, not a
        // politeness limit. ADR-0002's reproducer fires 6-10 parallel API
        // requests; this asserts at least three sockets stay available.
        const { assert!(SSE_MAX_CLIENTS < MAX_OPEN_SOCKETS) };
        const { assert!(MAX_OPEN_SOCKETS - SSE_MAX_CLIENTS >= 3) };
    }

    #[cfg_attr(test, test)]
    pub fn a_fresh_stream_has_no_clients_and_no_counters() {
        let sse = Sse::new(SseMode::Chunked);
        assert_eq!(sse.connected_now(), 0);
        assert_eq!(sse.clients(), 0);
        assert_eq!(sse.rejected(), 0);
        assert_eq!(sse.sent(), 0);
        assert_eq!(sse.dropped(), 0);
        assert_eq!(sse.pushed(), 0);
        assert_eq!(sse.dropped_frames(), 0);
    }

    /// How many frames the mailbox is holding.
    ///
    /// A `usize` rather than the `Result`: the on-target build has no `std`, so
    /// `MutexGuard` is not `PartialEq` there and comparing the `Result` directly
    /// would compile for the host and fail for the device. That asymmetry is
    /// exactly what `just lint-esp32` exists to catch, and this is the shape it
    /// caught.
    fn mailbox_len(sse: &Sse) -> usize {
        sse.mailbox.lock().map_or(0, |m| m.len())
    }

    #[cfg_attr(test, test)]
    pub fn a_push_counts_and_a_full_mailbox_counts_the_drop() {
        // `Sse::broadcast` is the C++'s `AsyncEventSource::send` path, and it
        // must not block its caller: the caller is the control task. A mailbox
        // that filled is counted, not waited on.
        let sse = Sse::new(SseMode::Chunked);
        for i in 0..SSE_MAILBOX_DEPTH {
            sse.broadcast(Sse::frame("weight", &format!("{{\"n\":{i}}}")));
        }
        sse.note_push_attempt();
        assert_eq!(sse.pushed(), 1);
        assert_eq!(sse.dropped_frames(), 0);
        assert_eq!(mailbox_len(&sse), SSE_MAILBOX_DEPTH);

        // One more than the mailbox holds: the oldest is discarded and counted.
        sse.broadcast(Sse::frame("weight", "{\"n\":99}"));
        assert_eq!(sse.dropped_frames(), 1);
        assert_eq!(mailbox_len(&sse), SSE_MAILBOX_DEPTH);
    }

    #[cfg_attr(test, test)]
    pub fn the_broadcaster_cadence_fits_under_the_poll_interval() {
        // The broadcaster can only be as responsive as its poll, and the poll
        // must not be the thing that makes the loop a busy-wait.
        const { assert!(SSE_POLL_MS < SSE_EVENT_INTERVAL_MS) };
        const { assert!(SSE_POLL_MS <= 100) };
    }

    #[cfg_attr(test, test)]
    pub fn the_keepalive_interval_is_under_the_nat_floor() {
        // 15 s against the common 30 s NAT UDP timeout; a silent stream is
        // dropped by the router long before the browser notices.
        const { assert!(SSE_KEEPALIVE_MS <= 15_000) };
        const { assert!(SSE_KEEPALIVE_MS > SSE_EVENT_INTERVAL_MS) };
    }

    // ---- the heap guard -------------------------------------------------

    #[cfg_attr(test, test)]
    pub fn the_large_response_floor_is_the_adrs_floor() {
        // ADR-0002 decision 5, `Logger.cpp:13`. Two subscribers, one number.
        assert_eq!(HEAP_FLOOR_BYTES, 30_000);
    }

    #[cfg_attr(test, test)]
    pub fn the_large_response_limit_is_above_the_csqs_19kb() {
        // ADR-0002 measured `/api/parameters?filter=all` at 19 KB. A limit
        // below that would 503 the endpoint the C++ serves.
        const { assert!(MAX_JSON_BYTES > 19 * 1024) };
        const {
            assert!(MAX_JSON_BYTES < 64 * 1024, "and not so large it is the OOM");
        };
    }

    #[cfg_attr(test, test)]
    pub fn the_shared_telemetry_round_trips() {
        let shared = Shared::new();
        let t = Telemetry {
            temperature_c: 88.0,
            machine_state: 33,
            ..Telemetry::default()
        };
        shared.publish(t.clone());
        assert_eq!(shared.snapshot(), t);
    }

    #[cfg_attr(test, test)]
    pub fn the_parameters_body_carries_a_value_for_every_parameter() {
        // `Config.h:226-238`: `toJson` writes `value` alongside `default`, and
        // it is the *current* value. A parameter with no `value` is the shape
        // the React editor cannot render, and it was the shape this firmware
        // shipped: the response had five fields and the C++ has ten.
        let body = parameters_json(&Config::default());
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        let entries = parsed.as_array().expect("an array");
        assert_eq!(entries.len(), SCHEMA.len());
        for entry in entries {
            let object = entry.as_object().expect("an object");
            for key in ["name", "type", "value", "default", "min", "max"] {
                assert!(
                    object.contains_key(key),
                    "{} has no {key}: {}",
                    object.get("name").and_then(|n| n.as_str()).unwrap_or("?"),
                    body
                );
            }
        }
    }

    #[cfg_attr(test, test)]
    pub fn a_parameter_value_is_typed_like_its_default() {
        // The C++'s `toJson` distinguishes `bool` from `int` from `double` from
        // `const char*`, and so does this. A quoted number makes the React
        // editor's number input refuse the value, so the two fields of a pair
        // must agree on shape — not merely both be present.
        let body = parameters_json(&Config::default());
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        for entry in parsed.as_array().expect("an array") {
            let object = entry.as_object().expect("an object");
            let name = object["name"].as_str().unwrap_or("?");
            let value = &object["value"];
            let default = &object["default"];
            assert_eq!(
                value.is_boolean(),
                default.is_boolean(),
                "{name}: value {value} and default {default} disagree on bool-ness"
            );
            assert_eq!(
                value.is_number(),
                default.is_number(),
                "{name}: value {value} and default {default} disagree on number-ness"
            );
            assert_eq!(
                value.is_string(),
                default.is_string(),
                "{name}: value {value} and default {default} disagree on string-ness"
            );
        }
    }

    #[cfg_attr(test, test)]
    pub fn a_set_parameter_reports_the_stored_value_not_the_default() {
        // The regression `value` exists to prevent: a firmware that reported
        // the compiled-in default would show a saved-and-reloaded operator's
        // settings as if they had been lost.
        let mut config = Config::default();
        config.brew.setpoint = 91.5;
        let body = parameters_json(&config);
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        let setpoint = parsed
            .as_array()
            .expect("an array")
            .iter()
            .map(|e| e.as_object().expect("an object"))
            .find(|o| o["name"] == "brew.setpoint")
            .expect("brew.setpoint is registered");
        assert_eq!(setpoint["value"], 91.5);
        // …and `default` still reports what a factory reset would give.
        //
        // 95.0, not 94.5: `constants/Temperature.h:18` `DEFAULT_BREW_SETPOINT_C =
        // 95.0f` and `Config::default()` sets 95.0. This assertion was 94.5 and
        // failed on its first run on hardware. The device-test harness caught it,
        // which is the harness doing its job.
        assert_eq!(setpoint["default"], 95.0);
    }

    #[cfg_attr(test, test)]
    pub fn the_radio_fields_survive_a_machine_publish() {
        // The two-publisher contract, and the reason the control task publishes
        // the radio *after* the machine telemetry. `publish` replaces the whole
        // slot, so a machine publish that ran second would erase the radio's
        // four fields and `/api/status` would go back to `wifiAssociated: false`
        // — which is the bug this ordering exists to prevent.
        let shared = Shared::new();
        // The radio publishes first — a read-modify-write, exactly as
        // `network::publish_radio` does it.
        let mut radio = shared.snapshot();
        radio.wifi_associated = true;
        radio.signal = 4;
        radio.ip = heapless::String::<15>::try_from("10.0.0.7").ok();
        shared.publish(radio);
        // …and the control task's publish is the one that must not run second.
        let before = shared.snapshot();
        shared.publish(telemetry_with_radio_untouched(&before));
        let after = shared.snapshot();
        assert!(
            after.wifi_associated,
            "the machine publish erased the radio's association flag"
        );
        assert_eq!(after.signal, 4);
        assert_eq!(after.ip, before.ip);
    }

    /// What a control task's `publish` looks like when it is careful: it carries
    /// the machine fields and *reuses* the radio's, because the two are separate
    /// publishers and a whole-slot replace is the only thing `Shared` offers.
    fn telemetry_with_radio_untouched(previous: &Telemetry) -> Telemetry {
        Telemetry {
            machine_state: 20,
            temperature_c: 93.5,
            signal: previous.signal,
            wifi_associated: previous.wifi_associated,
            wifi_offline: previous.wifi_offline,
            ip: previous.ip.clone(),
            ..Telemetry::default()
        }
    }

    #[cfg_attr(test, test)]
    pub fn a_default_snapshot_reports_no_radio_rather_than_a_fabricated_one() {
        // What `/api/status` said before the radio published: no association, no
        // signal, no address. These are the *absence* readings, and they must
        // read as absence — a fabricated `wifiSignal: 3` or a plausible-looking
        // address would be a lie an operator cannot act on.
        let t = Telemetry::default();
        assert!(!t.wifi_associated);
        assert_eq!(t.signal, 0);
        assert!(t.ip.is_none());
        let json = status_json(&t);
        assert!(json.contains("\"wifiAssociated\":false"), "{json}");
        assert!(json.contains("\"wifiSignal\":0"), "{json}");
        assert!(json.contains("\"ip\":null"), "{json}");
    }

    #[cfg_attr(test, test)]
    pub fn the_status_body_reports_an_associated_radio() {
        // The positive case, and the shape the C++'s reader expects: the keys
        // are always present, and they carry the radio's numbers when there are
        // any. `/api/status` had no `wifi*` key at all in the C++
        // (`WebServerManager.cpp:352-363`), so these are this firmware's
        // additions, kept stable because the UI reads them.
        let t = Telemetry {
            wifi_associated: true,
            signal: 4,
            ip: heapless::String::<15>::try_from("10.0.0.7").ok(),
            ..Telemetry::default()
        };
        let json = status_json(&t);
        assert!(json.contains("\"wifiAssociated\":true"), "{json}");
        assert!(json.contains("\"wifiSignal\":4"), "{json}");
        assert!(json.contains("\"ip\":\"10.0.0.7\""), "{json}");
    }

    #[cfg_attr(test, test)]
    pub fn a_reboot_request_is_one_shot() {
        let shared = Shared::new();
        shared.reboot_requested.store(true, Ordering::SeqCst);
        assert!(shared.take_reboot_request());
        assert!(
            !shared.take_reboot_request(),
            "a second take must not reboot again"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_ui_shell_and_its_assets_are_embedded_and_gzipped() {
        // The size assertion is the one that matters: the bundle is embedded rather
        // than mounted, and it only fits because it is compressed. In a `const`
        // block so a build that outgrows the app partition fails HERE rather
        // than as an OTA that will not fit on a machine that is already wired
        // up.
        const _: () = assert!(
            UI_TOTAL_BYTES < 400 * 1024,
            "the embedded web UI no longer fits the app partition"
        );
        const _: () = assert!(UI_FILE_COUNT >= 3, "index.html, one JS and one CSS bundle");

        // A JS file served as the wrong type is a blank page behind a 200, so
        // the embedded assets are checked for the pairing that causes it.
        for asset in UI_ASSETS {
            assert!(
                mime_for(asset.path) != "text/plain",
                "{} would be served as text/plain",
                asset.path
            );
            assert!(!asset.bytes.is_empty(), "{} is empty", asset.path);
        }
        let shell = resolve_ui("/ui");
        assert_eq!(shell, UiTarget::File(UI_INDEX));
        assert_eq!(UI_INDEX.path, "/index.html");
        assert!(UI_INDEX.gzip, "the shell must be gzip-encoded");
    }

    #[cfg_attr(test, test)]
    pub fn a_javascript_bundle_is_served_as_javascript() {
        assert_eq!(
            mime_for("/assets/index-DTmvHJP_.js"),
            "application/javascript"
        );
        assert_eq!(mime_for("/index.html"), "text/html");
        assert_eq!(mime_for("/assets/index-B4vm-kEh.css"), "text/css");
        assert_eq!(mime_for("/logo.png"), "image/png");
        // An unrecognised extension must never be claimed as text.
        assert_eq!(mime_for("/thing"), "application/octet-stream");
    }

    #[cfg_attr(test, test)]
    pub fn a_client_side_route_serves_the_shell_but_a_missing_asset_does_not() {
        // A reload on a deep link must boot the app, not 404.
        assert_eq!(resolve_ui("/ui/config/behavior"), UiTarget::SpaRoute);
        assert_eq!(resolve_ui("/ui/system"), UiTarget::SpaRoute);
        // A missing script must be a 404. Serving the shell here is the failure
        // that produces "200, HTML where JS was expected, blank screen".
        assert_eq!(resolve_ui("/ui/assets/missing.js"), UiTarget::Missing);
        // A query string is not part of the extension test.
        assert_eq!(resolve_ui("/ui/system?x=1.5"), UiTarget::SpaRoute);
        // `/ui*` also matches `/uixyz`, which is not a child of `/ui`.
        assert_eq!(resolve_ui("/uixyz"), UiTarget::Missing);
        // And the real asset is found.
        let js = UI_ASSETS
            .iter()
            .find(|a| mime_for(a.path) == "application/javascript")
            .map_or_else(|| panic!("no embedded JS"), |a| a.path);
        assert_eq!(
            resolve_ui(&alloc::format!("/ui{js}")),
            UiTarget::File(
                UI_ASSETS
                    .iter()
                    .find(|a| a.path == js)
                    .unwrap_or(&UI_ASSETS[0])
            )
        );
    }

    #[cfg_attr(test, test)]
    pub fn wildcard_matching_leaves_the_api_routes_exact() {
        // `/ui*` only works because the server matches wildcards, and that
        // switch is global. `httpd_uri_match_wildcard` requires an exact length
        // match for a template with no `*`/`?` (httpd_uri.c:57-60), so this
        // asserts the invariant the whole `/api/*` surface depends on.
        for (uri, _) in routes() {
            if uri != "/ui*" {
                assert!(!uri.ends_with('*'), "{uri} would match by prefix");
            }
        }
        assert!(configuration().uri_match_wildcard);
    }

    #[cfg_attr(test, test)]
    pub fn an_unavailable_endpoint_names_the_task_that_owns_it() {
        let json = unavailable_json("OTA", "R3-15");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["reason"], "R3-15");
        assert!(parsed["error"].as_str().is_some_and(|e| e.contains("OTA")));
    }
}
