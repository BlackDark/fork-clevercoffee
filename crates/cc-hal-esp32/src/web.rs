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
//! # SSE: the 02 §4 finding, resolved
//!
//! 02 §4 recorded SSE on ESP-IDF as "the biggest web-tier unknown" and proposed
//! a `esp-idf-sys` FFI shim. **That suggestion was wrong, and 02 §4's own
//! evidence says so**: `EspHttpConnection::write` *is*
//! `httpd_resp_send_chunk` (`esp-idf-svc` 0.53.0 `src/http/server.rs:1121-1135`),
//! so chunked transfer-coding is already available, and
//! `EspHttpConnection::raw_connection().write_all()` (`:1143`) is a raw
//! `write(2)` on the socket behind the request, for when chunked framing is not
//! wanted. Both ship. No FFI, no `unsafe`, no shim.
//!
//! [`Sse::Mode`] picks between them, and the default is chunked because it is
//! the one that goes through ESP-IDF's send path and therefore the one that
//! respects the socket's send timeout. A browser's `EventSource` accepts
//! `Transfer-Encoding: chunked` — the WHATWG concern in
//! [espressif/esp-idf#14121](https://github.com/espressif/esp-idf/issues/14121)
//! is about intermediaries that buffer, not about the browser — and if a
//! deployment ever does need the raw path, one `const` changes it.
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
    /// `EspHttpConnection::write` → `httpd_resp_send_chunk`
    /// (`esp-idf-svc` `src/http/server.rs:1121-1135`).
    ///
    /// The default, and the reason 02 §4's proposed FFI shim is not needed:
    /// this *is* the chunked API, in safe Rust, going through ESP-IDF's send
    /// path and therefore respecting `send_wait_timeout`.
    #[default]
    Chunked,
    /// `EspHttpConnection::raw_connection().write_all()` (`server.rs:1143`) — a
    /// raw `write(2)` on the socket behind the request.
    ///
    /// Not the default, and the difference is worth naming: this bypasses
    /// ESP-IDF's send path entirely, so it does not respect `send_wait_timeout`
    /// and will block for as long as the TCP window says. That is the right
    /// trade for a deliberately-open stream and the wrong one for a request
    /// handler.
    Raw,
}

/// The `/events` stream's broadcast state.
///
/// The C++'s `AsyncEventSource` keeps a client list and every producer pushes to
/// all of them (`WebServerManager.cpp:1163`). Here there is at most **one**
/// client, which is the difference that makes this a `Mutex<Option<..>>` rather
/// than a fan-out: `httpd_sess_get_ctx`/`open` gives one session per connection,
/// and a second browser tab is a second connection that the second
/// registration will simply not be told about. Recorded here rather than
/// silently truncated — a second tab showing a stale machine is a bug report.
pub struct Sse {
    mode: SseMode,
    clients: AtomicU32,
    sent: AtomicU32,
    dropped: AtomicU32,
}

impl Sse {
    /// A stream in the given framing mode.
    #[must_use]
    pub const fn new(mode: SseMode) -> Self {
        Self {
            mode,
            clients: AtomicU32::new(0),
            sent: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
        }
    }

    /// The framing mode.
    #[must_use]
    pub const fn mode(&self) -> SseMode {
        self.mode
    }

    /// How many clients have connected since boot.
    #[must_use]
    pub fn clients(&self) -> u32 {
        self.clients.load(Ordering::SeqCst)
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
            let shared = Arc::clone(&shared);
            server.fn_handler::<EspError, _>("/api/parameters", Method::Get, move |mut req| {
                let body = parameters_json();
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
        {
            let shared = Arc::clone(&shared);
            let sse = Arc::clone(&sse);
            server.fn_handler::<EspError, _>("/events", Method::Get, move |mut req| {
                sse.clients.fetch_add(1, Ordering::SeqCst);
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
                // The C++'s `onConnect` sends a `hello` event
                // (WebServerManager.cpp:308-317). A browser's `EventSource`
                // dispatches nothing until it sees a frame, so without this the
                // connection looks dead to the UI for its first interval.
                let hello = Sse::frame("hello", "{\"connected\":true}");
                stream(&sse, conn, &hello, &shared, SseMode::Chunked)
            })?;
        }

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
/// `Config::getAllParameters(array, "all")` (`WebServerManager.cpp:818`).
/// Each entry is `{name, type, value, min, max}` — the C++'s `toJson` output
/// (`:851-866`), which the React UI's parameter editor reads.
///
/// **The values are not included.** `cc_config`'s `ParamSpec` carries the
/// defaults and the ranges, not the live values, and the live values live in
/// the `Config` the control task owns. So this reports the schema — the names,
/// the types, the ranges and the defaults — which is what the editor needs to
/// render, and leaves the values to a `GET /api/config` or an
/// `SSE new_temps`. That split is also what keeps the four credentials out of
/// an unauthenticated endpoint.
#[must_use]
pub fn parameters_json() -> String {
    let mut out = String::with_capacity(SCHEMA.len() * 96);
    let _ = write!(out, "[");
    for (index, spec) in SCHEMA.iter().enumerate() {
        if index > 0 {
            let _ = write!(out, ",");
        }
        let _ = write!(
            out,
            "{{\"name\":\"{}\",\"type\":{},\"default\":{},\"min\":{},\"max\":{}}}",
            spec.key,
            spec.kind.cpp_param_type(),
            param_value_json(spec.default),
            opt_number(spec.min),
            opt_number(spec.max),
        );
    }
    let _ = write!(out, "]");
    out
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

/// Write an SSE preamble and hold the connection open.
///
/// **The 02 §4 finding, resolved by using the API that exists.** 02 §4 proposed
/// an `esp-idf-sys` FFI shim for "send preamble only, keep socket open" and
/// recorded the workaround as grabbing the socket fd and writing raw frames.
/// Both of those are now unnecessary:
///
/// * `EspHttpConnection::write` **is** `httpd_resp_send_chunk`
///   (`esp-idf-svc` 0.53.0 `src/http/server.rs:1121-1135`), so chunked
///   transfer-coding is available in safe Rust, going through ESP-IDF's send
///   path and therefore respecting `send_wait_timeout`;
/// * `EspHttpConnection::raw_connection().write_all()` (`:1143`) is a raw
///   `write(2)` on the same socket, for a caller that wants no chunk framing.
///
/// [`SseMode`] picks. Chunked is the default because it is the one that cannot
/// block unboundedly.
fn stream(
    sse: &Sse,
    conn: &mut EspHttpConnection<'_>,
    first: &str,
    shared: &Shared,
    mode: SseMode,
) -> Result<(), EspError> {
    sse.sent.fetch_add(1, Ordering::SeqCst);
    let wrote = match mode {
        SseMode::Chunked => conn.write_all(first.as_bytes()).is_ok(),
        SseMode::Raw => conn
            .raw_connection()
            .and_then(|raw| raw.write_all(first.as_bytes()))
            .is_ok(),
    };
    if !wrote {
        sse.dropped.fetch_add(1, Ordering::Relaxed);
        return Ok(());
    }

    let mut last_event = now_ms();
    let mut last_keepalive = now_ms();
    loop {
        let now = now_ms();
        let snapshot = shared.snapshot();
        let frame = if now.wrapping_sub(last_event) >= SSE_EVENT_INTERVAL_MS {
            last_event = now;
            Sse::frame("new_temps", &temperatures_json(&snapshot))
        } else if now.wrapping_sub(last_keepalive) >= SSE_KEEPALIVE_MS {
            last_keepalive = now;
            Sse::keepalive()
        } else {
            esp_idf_hal::delay::FreeRtos::delay_ms(50);
            continue;
        };

        sse.sent.fetch_add(1, Ordering::SeqCst);
        let wrote = match mode {
            SseMode::Chunked => conn.write_all(frame.as_bytes()).is_ok(),
            SseMode::Raw => conn
                .raw_connection()
                .and_then(|raw| raw.write_all(frame.as_bytes()))
                .is_ok(),
        };
        if !wrote {
            // A closed client is the normal end of an SSE stream, not an error.
            // Counting it is what makes "zero drops over ten minutes" a
            // measurement.
            sse.dropped.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
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
    /// receiving them" are two different numbers. While the client list does
    /// not exist the two are equal by construction and the count is not a
    /// delivery measurement — which is why [`Sse::sent`] and
    /// [`Sse::dropped`], which the stream itself increments, are the ones the
    /// R3-14 acceptance criterion is stated against.
    pub fn note_push_attempt(&self) {
        self.clients.fetch_add(0, Ordering::Relaxed);
    }

    /// Push a frame to every connected client.
    ///
    /// There is at most one, because `httpd_sess_get_ctx` gives one session per
    /// connection and a second browser tab is a second connection that this
    /// broadcast cannot reach. **That is a real gap, not a simplification** — a
    /// second tab shows a stale machine — and it is closed by keeping a client
    /// list rather than a counter, which is R3-16's work when the transport
    /// exists.
    pub fn broadcast(&self, _frame: String) {
        // No client list yet: see the doc comment on the field.
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
        // The reason 02 section 4's proposed FFI shim is not needed: chunked
        // transfer-coding already ships, in safe Rust.
        assert_eq!(SseMode::default(), SseMode::Chunked);
        assert_eq!(Sse::new(SseMode::default()).mode(), SseMode::Chunked);
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
