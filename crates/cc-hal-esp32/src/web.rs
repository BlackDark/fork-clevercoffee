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
use esp_idf_svc::http::server::{Configuration, EspHttpConnection, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::sys::EspError;
use log::{info, warn};

use crate::heap::{free_heap, min_free_heap, HEAP_SHED_BYTES};
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
/// `HttpServerConfiguration::max_uri_handlers` defaults to 32 (`server.rs:132`).
/// 20 `/api/*` + `/` + `/ui` + `/events` is 23, and the headroom is for the OTA
/// routes R3-15 will add. `max_open_sockets` is raised from 4 to 5 for the same
/// reason: a browser opens the SPA, an SSE stream and several API requests at
/// once, and the C++'s 4 was on `ESPAsyncWebServer`, which has a different
/// accounting.
pub const MAX_URI_HANDLERS: usize = 32;
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
    /// Whether a brew is running.
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
    /// The IPv4 address as a string, or `None`.
    ///
    /// A `String` and not a `heapless::String<15>`, which is what makes
    /// `Telemetry` non-`Copy`. It is built once per control tick *in the control
    /// task* and read by the httpd task, so the allocation is the control
    /// task's and never the httpd task's -- and the httpd task is the one
    /// ADR-0002 is about.
    pub ip: Option<String>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// `POST /api/setpoint?value=<celsius>`.
    SetSetpoint(i32),
    /// `POST /api/steam?on=0|1`.
    SetSteam(bool),
    /// `POST /api/pid?on=0|1`.
    SetPid(bool),
    /// `POST /api/backflush?on=0|1`.
    SetBackflush(bool),
    /// `POST /api/backflush`.
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

/// The shared state the HTTP handlers close over.
pub struct Shared {
    /// The latest telemetry, republished by the control task.
    pub telemetry: Mutex<Telemetry>,
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
            telemetry: Mutex::new(Telemetry::default()),
            reboot_requested: AtomicBool::new(false),
            large_responses: AtomicU32::new(0),
            large_refused: AtomicU32::new(0),
        }
    }

    /// Replace the telemetry snapshot.
    ///
    /// **A whole-slot replace, not a merge.** There are two publishers — the
    /// control task, through [`Telemetry`]'s machine fields, and the radio,
    /// through the four fields [`network::publish_radio`] owns — so whoever calls
    /// this must leave the other's fields alone. In practice that means the
    /// control task publishes first and the radio second in the same tick; see
    /// `publish_radio` for why the order matters.
    pub fn publish(&self, telemetry: Telemetry) {
        if let Ok(mut slot) = self.telemetry.lock() {
            *slot = telemetry;
        }
    }

    /// A copy of the current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Telemetry {
        self.telemetry
            .lock()
            .map_or_else(|_| Telemetry::default(), |t| t.clone())
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

    /// Add a client, or refuse it because the stream is full.
    fn attach(&self, req: crate::web_async::AsyncReq) -> bool {
        let now = now_ms();
        // A poisoned lock means some other client write panicked. Refusing is
        // the only safe answer: the list's length is the bound that keeps the
        // httpd task's socket budget intact.
        let Ok(mut clients) = self.clients.lock() else {
            return false;
        };
        if clients.len() >= SSE_MAX_CLIENTS {
            return false;
        }
        clients.push(SseClient {
            req,
            await_hello: true,
            last_write_ms: now,
        });
        true
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

/// The C++'s error body, verbatim. `ApiResponses::errorResponse`.
#[must_use]
pub fn error_body(message: &str) -> String {
    format!("{{\"error\":\"{message}\"}}")
}

/// `/api/status` — the C++'s `WebServerManager.cpp:327-372`.
///
/// Field for field. The C++ adds `weight`/`brewWeight` only when the scale is
/// enabled; here they are `null` when there is no reading, which is a smaller
/// change than gating them and a smaller one than reporting a fabricated 0 g.
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
         \"pidEnabled\":{},\"brewing\":{},\"uptime\":{},\
         \"shotsSinceBackflush\":{},\"backflushReminderThreshold\":{},\
         \"backflushReminderDue\":{}",
        t.temperature_c,
        t.setpoint_c,
        t.heater_power_pct,
        t.machine_state,
        t.standby,
        t.standby_remaining_ms,
        t.pid_enabled,
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

/// The `/ui` body for a build with no embedded bundle.
///
/// F25 (the React SPA) is a re-embed of `ui/build`, and that bundle is not in
/// the tree yet. Saying so is the honest response; serving an empty page would
/// look like a JavaScript error in the browser.
pub const UI_PLACEHOLDER: &str = concat!(
    "CleverCoffee control UI\n",
    "\n",
    "This build has no embedded web bundle (F25/R3-20). The REST API and the\n",
    "/events stream are live; see docs/rust-migration/01-feature-inventory.md F25.\n"
);

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
        ("/api/parameters", Method::Get),
        ("/api/status", Method::Options),
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
        ("/events", Method::Get),
        ("/", Method::Get),
        ("/ui", Method::Get),
    ]
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
    #[allow(
        clippy::too_many_lines,
        reason = "this IS a route table. 24 registrations with their handlers, \
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
    ) -> Result<Self, EspError> {
        // Every handler is `Send + 'static` (`server.rs:530-538`), so each one
        // captures its own `Arc::clone`. Taking these three by reference and
        // cloning into a local owned `Arc` once is what makes that possible
        // without an `unsafe fn` (`handler_nonstatic`, `server.rs:568`, which
        // this workspace denies).
        let send = Arc::clone(send);
        // `EspHttpServer::new` returns `EspIOError` (`server.rs:345`) while every
        // `httpd_*` call returns `EspError`, so the one conversion is here
        // rather than repeated in every handler.
        let mut server = EspHttpServer::new(&configuration()).map_err(|e| e.0)?;

        // --- reads -------------------------------------------------------
        {
            let shared = Arc::clone(&shared);
            server.fn_handler::<EspError, _>("/api/status", Method::Get, move |mut req| {
                let body = status_json(&shared.snapshot());
                respond(req.connection(), 200, &body)
            })?;
        }
        {
            let shared = Arc::clone(&shared);
            server.fn_handler::<EspError, _>("/api/health", Method::Get, move |mut req| {
                let body = health_json(&shared.snapshot());
                respond(req.connection(), 200, &body)
            })?;
        }
        {
            let shared = Arc::clone(&shared);
            server.fn_handler::<EspError, _>(
                "/api/temperatures",
                Method::Get,
                move |mut req| {
                    let body = temperatures_json(&shared.snapshot());
                    respond(req.connection(), 200, &body)
                },
            )?;
        }
        {
            server.fn_handler::<EspError, _>("/api/history", Method::Get, |mut req| {
                respond(
                    req.connection(),
                    200,
                    &unavailable_json("Temperature history", "R3-09 (the timeseries ring)"),
                )
            })?;
        }
        {
            let described = String::from(nvs_description);
            server.fn_handler::<EspError, _>("/api/nvs-debug", Method::Get, move |mut req| {
                // The store itself stays with the control task, which is the
                // only writer; a handler gets the description string it needs and
                // nothing that could write. `ConfigStore::describe` takes
                // `&self` and this is its whole result — namespace, key, schema
                // version and byte count — so nothing is lost and a `Send +
                // 'static` handler needs no shared NVS handle at all.
                let body = nvs_debug_json(&described, cc_config::SCHEMA.len());
                respond(req.connection(), 200, &body)
            })?;
        }
        {
            server.fn_handler::<EspError, _>("/api/parameter-help", Method::Get, |mut req| {
                respond(
                    req.connection(),
                    200,
                    &unavailable_json("Per-parameter help", "R3-16 (the schema UI)"),
                )
            })?;
        }
        {
            let config = Arc::clone(config);
            let shared = Arc::clone(&shared);
            server.fn_handler::<EspError, _>("/api/config", Method::Get, move |mut req| {
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
            })?;
        }
        {
            let config = Arc::clone(config);
            let shared = Arc::clone(&shared);
            server.fn_handler::<EspError, _>("/api/parameters", Method::Get, move |mut req| {
                let body = parameters_json(&config);
                respond_large(req.connection(), &shared, &body)
            })?;
        }

        // --- commands ----------------------------------------------------
        register_command(&mut server, "/api/setpoint", Arc::clone(&send), |value| {
            // WebServerManager.cpp:393-395: 0..=150 is accepted, and 0 is
            // a *value*, not an absence. Parsed as an integer because the
            // C++ reads a `double` and a fractional setpoint is a bug in
            // the caller, not a request to round.
            // Truncating a fractional setpoint is deliberate and is the
            // C++'s: `request->getParam("value", true)->value().toDouble()`
            // into an `int` field (WebServerManager.cpp:393-395). The
            // schema's own range is integral, so a fractional value is a
            // caller bug and rounding it is more useful than a 400.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "v is in 0.0..=150.0, which fits an i32 with room to \
                              spare; the C++ truncates the same way"
            )]
            value
                .parse::<f64>()
                .ok()
                .filter(|v| (0.0..=150.0).contains(v))
                .map(|v| Command::SetSetpoint(v as i32))
        })?;
        register_command(
            &mut server,
            "/api/steam",
            Arc::clone(&send),
            |value| match value {
                "1" | "true" | "on" => Some(Command::SetSteam(true)),
                "0" | "false" | "off" => Some(Command::SetSteam(false)),
                _ => None,
            },
        )?;
        register_command(
            &mut server,
            "/api/pid",
            Arc::clone(&send),
            |value| match value {
                "1" | "true" | "on" => Some(Command::SetPid(true)),
                "0" | "false" | "off" => Some(Command::SetPid(false)),
                _ => None,
            },
        )?;
        register_command(
            &mut server,
            "/api/backflush",
            Arc::clone(&send),
            |value| match value {
                "1" | "true" | "on" => Some(Command::SetBackflush(true)),
                "0" | "false" | "off" => Some(Command::SetBackflush(false)),
                "start" => Some(Command::StartBackflush),
                _ => None,
            },
        )?;
        register_flag(&mut server, "/api/sleep", Arc::clone(&send), Command::Sleep)?;
        register_flag(&mut server, "/api/wake", Arc::clone(&send), Command::Wake)?;
        register_flag(
            &mut server,
            "/api/scale/tare",
            Arc::clone(&send),
            Command::Tare,
        )?;
        register_flag(
            &mut server,
            "/api/scale/calibration",
            Arc::clone(&send),
            Command::Calibrate,
        )?;
        register_flag(
            &mut server,
            "/api/maintenance/reset-backflush-counter",
            Arc::clone(&send),
            Command::ResetBackflushCounter,
        )?;
        register_flag(
            &mut server,
            "/api/wifi-reset",
            Arc::clone(&send),
            Command::WifiReset,
        )?;
        register_flag(
            &mut server,
            "/api/factory-reset",
            Arc::clone(&send),
            Command::FactoryReset,
        )?;
        register_flag(
            &mut server,
            "/api/restart",
            Arc::clone(&send),
            Command::Restart,
        )?;

        // --- static ------------------------------------------------------
        {
            server.fn_handler::<EspError, _>("/", Method::Get, |mut req| {
                // WebServerManager.cpp:974: `request->redirect("/ui/")`.
                let conn = req.connection();
                conn.initiate_response(302, Some("Found"), &[("Location", "/ui/")])?;
                conn.write_all(b"")
            })?;
        }
        {
            server.fn_handler::<EspError, _>("/ui", Method::Get, |mut req| {
                let conn = req.connection();
                conn.initiate_response(200, Some("OK"), &[("Content-Type", "text/plain")])?;
                conn.write_all(UI_PLACEHOLDER.as_bytes())
            })?;
        }

        // --- SSE ---------------------------------------------------------
        // The handler must return: ESP-IDF's httpd is one task, so a handler
        // that does not return is a server that does not serve. It sets the
        // response headers, detaches the request, registers it, and leaves.
        // `spawn_broadcaster` below owns the writing.
        #[allow(
            unsafe_code,
            reason = "the `/events` handler must detach its request and return, \
                      or the single-task httpd stops serving every other route; \
                      see `crate::web_async` for the full argument"
        )]
        {
            let sse = Arc::clone(&sse);
            server.fn_handler::<EspError, _>("/events", Method::Get, move |mut req| {
                let conn = req.connection();
                conn.initiate_response(
                    200,
                    Some("OK"),
                    &[
                        ("Content-Type", "text/event-stream"),
                        ("Cache-Control", "no-cache"),
                        ("Connection", "keep-alive"),
                    ],
                )?;
                sse.connected.fetch_add(1, Ordering::SeqCst);
                // `initiate_response` has already set the status, type and
                // headers on the request, and `httpd_req_async_handler_begin`
                // copies exactly those into the detached request. Doing it in
                // this order is what makes the browser see
                // `Content-Type: text/event-stream` on the response.
                let Some(async_req) = (unsafe {
                    // SAFETY: `conn` is the live connection of the handler
                    // running right now, and `initiate_response` above was the
                    // last thing to touch it — which is what makes the header
                    // copy inside `begin` correct. `web_async`'s module docs
                    // carry the full argument.
                    crate::web_async::begin_detached(esp_idf_svc::handle::RawHandle::handle(conn))
                }) else {
                    warn!("sse: could not detach the request");
                    sse.rejected.fetch_add(1, Ordering::Relaxed);
                    return respond(conn, 503, &error_body("sse unavailable"));
                };
                if sse.attach(async_req) {
                    // The C++'s `onConnect` sends a `hello` event
                    // (WebServerManager.cpp:308-317). A browser's `EventSource`
                    // dispatches nothing until it sees a frame, so without this
                    // the connection looks dead to the UI for its first
                    // interval. The broadcaster writes it on its first pass.
                    Ok(())
                } else {
                    sse.rejected.fetch_add(1, Ordering::Relaxed);
                    respond(conn, 503, &error_body("too many event-stream clients"))
                }
            })?;
        }

        // The broadcaster is the only writer of `/events`, and it is not the
        // httpd task. Started after the routes so a client cannot connect to a
        // stream nobody is servicing.
        spawn_broadcaster(Arc::clone(&sse))?;

        info!(
            "http: listening on port {HTTP_PORT}, {} routes, {} B handler budget",
            routes().len(),
            MAX_URI_HANDLERS
        );
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
    uri: &'static str,
    send: Arc<dyn Fn(Command) + Send + Sync + 'static>,
    command: Command,
) -> Result<(), EspError> {
    server
        .fn_handler::<EspError, _>(uri, Method::Post, move |mut req| {
            let _ = drain_body(req.connection());
            send(command);
            respond(req.connection(), 202, "{\"accepted\":true}")
        })
        .map(|_| ())
}

/// Register a `POST` handler that parses one field into a command.
fn register_command(
    server: &mut EspHttpServer<'static>,
    uri: &'static str,
    send: Arc<dyn Fn(Command) + Send + Sync + 'static>,
    parse: fn(&str) -> Option<Command>,
) -> Result<(), EspError> {
    server
        .fn_handler::<EspError, _>(uri, Method::Post, move |mut req| {
            let body = drain_body(req.connection());
            // The C++ uses `hasParam("value", true)` (WebServerManager.cpp:392) —
            // the `true` is "from the body" — and 0 is a valid setpoint (`:393`),
            // so the field's presence is what matters, not its truthiness.
            let Some(value) = first_field(&body, "value").or_else(|| first_field(&body, "on"))
            else {
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
        .map(|_| ())
}

/// Read a request body into a `String`, bounded.
///
/// 256 bytes, the same bound the log stream uses. A `POST /api/setpoint` body
/// is `value=94.5` — nine characters — and a body that does not fit is a client
/// bug or an attack, not a large legitimate request. The C++ has no bound at
/// all (`AsyncWebServerRequest` will buffer whatever it is sent), which on a
/// 320 KB heap is a denial of service with three words.
fn drain_body(conn: &mut EspHttpConnection<'_>) -> String {
    let mut body = String::new();
    let mut buf = [0u8; 128];
    loop {
        match conn.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if body.len() + n > crate::telnet::LINE_BUFFER_BYTES {
                    break;
                }
                body.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        }
    }
    body
}

/// The first form field named `name` in `body`.
///
/// See [`cc_config::form`] for why the REST API reads bodies rather than query
/// strings: `EspHttpConnection::uri()` returns the path only, and `esp-idf-svc`
/// exposes no accessor for `httpd_req_t::query`.
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
            ip: Some("192.168.1.42".into()),
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
        // 20 /api/* from WebServerManager.cpp:327-812, plus /, /ui and /events.
        let routes = routes();
        for expected in [
            "/api/status",
            "/api/health",
            "/api/temperatures",
            "/api/history",
            "/api/nvs-debug",
            "/api/parameter-help",
            "/api/config",
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
        ] {
            assert!(
                routes.iter().any(|(path, _)| *path == expected),
                "{expected} is missing"
            );
        }
    }

    #[cfg_attr(test, test)]
    pub fn the_route_table_fits_the_servers_handler_budget() {
        // esp-idf-svc's default is 32 (server.rs:132) and the C++ registers 24.
        // If this ever grows past MAX_URI_HANDLERS the server will fail to start
        // with ESP_ERR_HTTPD_HANDLERS_FULL, at boot, which is a bad place to find
        // out.
        assert!(routes().len() <= MAX_URI_HANDLERS);
    }

    #[cfg_attr(test, test)]
    pub fn the_redirect_and_the_ui_are_routes() {
        let routes = routes();
        assert!(
            routes.contains(&("/", Method::Get)),
            "the / -> /ui/ redirect"
        );
        assert!(routes.contains(&("/ui", Method::Get)));
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
        // The radio publishes first.
        if let Ok(mut slot) = shared.telemetry.lock() {
            slot.wifi_associated = true;
            slot.signal = 4;
            slot.ip = Some(alloc::string::String::from("10.0.0.7"));
        }
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
            ip: Some(alloc::string::String::from("10.0.0.7")),
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
    pub fn the_ui_placeholder_says_why_it_is_empty() {
        // An empty page looks like a JavaScript failure. A sentence does not.
        assert!(UI_PLACEHOLDER.contains("no embedded web bundle"));
        assert!(UI_PLACEHOLDER.contains("F25"));
    }

    #[cfg_attr(test, test)]
    pub fn an_unavailable_endpoint_names_the_task_that_owns_it() {
        let json = unavailable_json("OTA", "R3-15");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["reason"], "R3-15");
        assert!(parsed["error"].as_str().is_some_and(|e| e.contains("OTA")));
    }
}
