//! The HTTP server's routing and handlers, independent of the hardware.
//!
//! [`Api`] is a picoserve service over a [`Backend`] the firmware implements. On the
//! PC the same service runs on tokio against a fake backend (see `tests/http.rs`),
//! so every endpoint is tested end to end without a device.
//!
//! Routes follow the C++ firmware's `/api/*` (the web UI is unchanged apart from
//! removing the filesystem and URL updates), plus `/events` (server-sent events)
//! and the embedded UI under `/ui/`.

#![no_std]

extern crate alloc;

use core::fmt::Write as _;

use cc_api::json::Writer;
use cc_api::{FormPairs, Live, multipart};
use cc_core::history::History;
use cc_core::{Command, Config, Reply, Status};
use picoserve::ResponseSent;
use picoserve::io::{Read, Write};
use picoserve::request::{Path, Request};
use picoserve::response::sse::{EventSource, EventStream, EventWriter};
use picoserve::response::{Content, IntoResponse, Response, ResponseWriter, StatusCode};
use picoserve::routing::PathRouterService;

/// Largest accepted config upload.
pub const MAX_CONFIG_UPLOAD: usize = 16 * 1024;
/// Server-sent events interval.
pub const EVENT_INTERVAL_MS: u64 = 1_000;

/// An embedded UI file.
#[derive(Debug, Clone, Copy)]
pub struct Asset {
    pub bytes: &'static [u8],
    pub content_type: &'static str,
    pub gzip: bool,
    /// Hashed file names can be cached forever; `index.html` must not be.
    pub immutable: bool,
}

/// Firmware update progress, as `/api/ota/status` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtaStatus {
    pub status: &'static str,
    pub progress: u8,
    pub in_progress: bool,
    pub uploaded: u32,
    pub total: u32,
    pub error: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaError {
    Busy,
    TooLarge,
    Write,
    Invalid,
}

impl OtaError {
    pub const fn message(self) -> &'static str {
        match self {
            OtaError::Busy => "Update already in progress",
            OtaError::TooLarge => "Firmware does not fit the update partition",
            OtaError::Write => "Writing the update failed",
            OtaError::Invalid => "Not a valid firmware image",
        }
    }
}

/// What the web server needs from the device.
#[allow(async_fn_in_trait)]
pub trait Backend {
    /// Runs `f` with the current configuration, status and live values.
    fn read<R>(&self, f: impl FnOnce(&Config, &Status, &Live) -> R) -> R;
    fn history<R>(&self, f: impl FnOnce(&History) -> R) -> R;
    /// Applies a command to the machine.
    fn command(&self, cmd: Command) -> Reply;
    /// Changes the configuration; the backend validates nothing more, persists if
    /// `f` returns `true`, and tells the machine.
    fn update_config(&self, f: &mut dyn FnMut(&mut Config) -> bool);
    fn restart(&self);
    fn factory_reset(&self);
    fn wifi_reset(&self);
    fn asset(&self, path: &str) -> Option<Asset>;
    async fn sleep_ms(&self, ms: u64);

    fn ota_status(&self) -> OtaStatus;
    fn ota_begin(&self) -> Result<(), OtaError>;
    async fn ota_write(&self, data: &[u8]) -> Result<(), OtaError>;
    /// Validates and activates the image; the device restarts afterwards.
    async fn ota_finish(&self) -> Result<(), OtaError>;
    fn ota_abort(&self, err: OtaError);

    /// Bench builds: stop feeding the watchdog.
    #[cfg(feature = "bench")]
    fn debug_hang(&self);
}

/// The router: everything under one service.
#[derive(Debug)]
pub struct Api<'b, B> {
    backend: &'b B,
}

impl<'b, B: Backend> Api<'b, B> {
    pub fn new(backend: &'b B) -> Self {
        Self { backend }
    }

    pub fn router(self) -> picoserve::Router<impl picoserve::routing::PathRouter> {
        picoserve::Router::from_service(self)
    }
}

// --- bodies -----------------------------------------------------------------------

/// A JSON body generated on demand. It is rendered twice (length, then content) and
/// streamed in windows, so a 20 KB parameter list needs no allocation.
struct Json<F>(F);

const WINDOW: usize = 1024;

struct Counter(usize);
impl core::fmt::Write for Counter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0 += s.len();
        Ok(())
    }
}

/// Collects bytes `[skip, skip + buf.len())` of the output.
struct Window<'a> {
    skip: usize,
    pos: usize,
    buf: &'a mut [u8],
    len: usize,
}
impl core::fmt::Write for Window<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            if self.pos >= self.skip && self.len < self.buf.len() {
                self.buf[self.len] = b;
                self.len += 1;
            }
            self.pos += 1;
        }
        Ok(())
    }
}

impl<F: Fn(&mut dyn core::fmt::Write) -> core::fmt::Result> Content for Json<F> {
    fn content_type(&self) -> &'static str {
        "application/json"
    }

    fn content_length(&self) -> usize {
        let mut c = Counter(0);
        let _ = (self.0)(&mut c);
        c.0
    }

    async fn write_content<W: Write>(self, mut writer: W) -> Result<(), W::Error> {
        let total = self.content_length();
        let mut buf = alloc::vec![0u8; WINDOW];
        let mut sent = 0;
        while sent < total {
            let mut w = Window {
                skip: sent,
                pos: 0,
                buf: &mut buf,
                len: 0,
            };
            let _ = (self.0)(&mut w);
            let n = w.len;
            if n == 0 {
                break;
            }
            writer.write_all(&buf[..n]).await?;
            sent += n;
        }
        Ok(())
    }
}

fn json<F: Fn(&mut dyn core::fmt::Write) -> core::fmt::Result>(
    status: StatusCode,
    f: F,
) -> Response<impl picoserve::response::HeadersIter + use<F>, impl picoserve::response::Body + use<F>> {
    Response::new(status, Json(f)).with_headers(CORS)
}

const CORS: [(&str, &str); 3] = [
    ("Access-Control-Allow-Origin", "*"),
    ("Access-Control-Allow-Methods", "GET,POST,PUT,DELETE,OPTIONS"),
    (
        "Access-Control-Allow-Headers",
        "Content-Type,Authorization,X-Requested-With",
    ),
];

/// `{"success":..,"message":..}` plus optional extra fields.
fn message(
    status: StatusCode,
    success: bool,
    msg: &str,
) -> Response<impl picoserve::response::HeadersIter + use<>, impl picoserve::response::Body + use<>> {
    let mut s: heapless::String<256> = heapless::String::new();
    {
        let mut w = Writer::new(&mut s);
        let _ = w.begin_object();
        let _ = w.kv_bool("success", success);
        let _ = w.kv_str(if success { "message" } else { "error" }, msg);
        if !success {
            let _ = w.kv_str("message", msg);
        }
        let _ = w.end_object();
    }
    Response::new(status, Owned(s)).with_headers(CORS)
}

/// `{"success":true,"<key>":<bool>}` (the toggle endpoints).
fn flag(
    key: &'static str,
    v: bool,
) -> Response<impl picoserve::response::HeadersIter + use<>, impl picoserve::response::Body + use<>> {
    let mut s: heapless::String<96> = heapless::String::new();
    let _ = write!(s, "{{\"success\":true,\"{key}\":{v}}}");
    Response::new(StatusCode::OK, Owned(s)).with_headers(CORS)
}

struct Owned<const N: usize>(heapless::String<N>);
impl<const N: usize> Content for Owned<N> {
    fn content_type(&self) -> &'static str {
        "application/json"
    }
    fn content_length(&self) -> usize {
        self.0.len()
    }
    async fn write_content<W: Write>(self, mut writer: W) -> Result<(), W::Error> {
        writer.write_all(self.0.as_bytes()).await
    }
}

struct Bytes {
    data: &'static [u8],
    content_type: &'static str,
}
impl Content for Bytes {
    fn content_type(&self) -> &'static str {
        self.content_type
    }
    fn content_length(&self) -> usize {
        self.data.len()
    }
    async fn write_content<W: Write>(self, mut writer: W) -> Result<(), W::Error> {
        writer.write_all(self.data).await
    }
}

// --- authentication -------------------------------------------------------------------

/// Checks HTTP basic auth against the configured credentials.
pub fn authorized(cfg: &Config, header: Option<&str>) -> bool {
    if !cfg.system_auth_enabled {
        return true;
    }
    let Some(v) = header.and_then(|h| h.strip_prefix("Basic ")) else {
        return false;
    };
    let mut buf = [0u8; 128];
    let Some(decoded) = base64_decode(v.trim(), &mut buf) else {
        return false;
    };
    let Some((user, pass)) = decoded.split_once(':') else {
        return false;
    };
    ct_eq(user.as_bytes(), cfg.system_auth_username.as_bytes())
        & ct_eq(pass.as_bytes(), cfg.system_auth_password.as_bytes())
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn base64_decode<'b>(s: &str, out: &'b mut [u8]) -> Option<&'b str> {
    let val = |c: u8| -> Option<u32> {
        Some(u32::from(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        }))
    };
    let s = s.trim_end_matches('=').as_bytes();
    let mut n = 0;
    for chunk in s.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= val(c)? << (18 - 6 * i);
        }
        let bytes = [(acc >> 16) as u8, (acc >> 8) as u8, acc as u8];
        let take = chunk.len().saturating_sub(1);
        out.get_mut(n..n + take)?.copy_from_slice(&bytes[..take]);
        n += take;
    }
    core::str::from_utf8(&out[..n]).ok()
}

// --- server-sent events -----------------------------------------------------------

struct Events<'b, B> {
    backend: &'b B,
}

impl<B: Backend> EventSource for Events<'_, B> {
    async fn write_events<W: Write>(self, mut writer: EventWriter<'_, W>) -> Result<(), W::Error> {
        writer.write_event("hello", "Connected to CleverCoffee").await?;
        loop {
            self.backend.sleep_ms(EVENT_INTERVAL_MS).await;
            let mut s: heapless::String<160> = heapless::String::new();
            let scale = self.backend.read(|cfg, st, _| {
                let _ = cc_api::write_temperatures(st, &mut Writer::new(&mut s));
                cfg.hardware_sensors_scale_enabled.then_some(*st)
            });
            writer.write_event("new_temps", s.as_str()).await?;
            if let Some(st) = scale {
                s.clear();
                let _ = cc_api::write_weight(&st, &mut Writer::new(&mut s));
                writer.write_event("weight", s.as_str()).await?;
            }
        }
    }
}

// --- routing ----------------------------------------------------------------------

impl<B: Backend> PathRouterService for Api<'_, B> {
    async fn call_path_router_service<R: Read, W: ResponseWriter<Error = R::Error>>(
        &self,
        _state: &(),
        _params: (),
        path: Path<'_>,
        mut request: Request<'_, R>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        let b = self.backend;
        let method = request.parts.method();
        let mut path_buf = [0u8; 128];
        let path = cc_api::percent_decode(path.encoded(), &mut path_buf).unwrap_or("");
        let mut query_buf = [0u8; 256];
        let query = request
            .parts
            .query()
            .map_or("", |q| cc_api::percent_decode(q.0, &mut query_buf).unwrap_or(""));
        // Keep the raw query for form decoding (percent_decode above is only for logs).
        let raw_query = request.parts.query().map_or("", |q| q.0);
        let _ = query;

        let auth = request
            .parts
            .headers()
            .get("Authorization")
            .and_then(|h| h.as_str().ok());
        if !b.read(|cfg, _, _| authorized(cfg, auth)) {
            let resp = Response::new(StatusCode::UNAUTHORIZED, "Unauthorized")
                .with_headers([("WWW-Authenticate", "Basic realm=\"CleverCoffee\"")]);
            return resp
                .write_to(request.body_connection.finalize().await?, response_writer)
                .await;
        }

        macro_rules! reply {
            ($resp:expr) => {
                return $resp
                    .write_to(request.body_connection.finalize().await?, response_writer)
                    .await
            };
        }

        if method == "OPTIONS" {
            reply!(Response::empty(StatusCode::NO_CONTENT).with_headers(CORS));
        }

        match (method, path) {
            ("GET", "/api/status") => {
                let (cfg, st, live) = b.read(|c, s, l| (c.clone(), *s, *l));
                reply!(json(StatusCode::OK, move |w| cc_api::write_status(
                    &st,
                    &cfg,
                    &live,
                    &mut Writer::new(w)
                )));
            }
            ("GET", "/api/health") => reply!(Response::empty(StatusCode::OK).with_headers(CORS)),
            ("GET", "/api/temperatures") => {
                let st = b.read(|_, s, _| *s);
                reply!(json(StatusCode::OK, move |w| cc_api::write_temperatures(
                    &st,
                    &mut Writer::new(w)
                )));
            }
            ("GET", "/api/history") => {
                // A copy, so the body is rendered without holding the device's lock.
                let h = alloc::boxed::Box::new(b.history(Clone::clone));
                reply!(json(StatusCode::OK, move |w| cc_api::write_history(
                    &h,
                    &mut Writer::new(w)
                )));
            }
            ("GET", "/api/parameters") => {
                let cfg = b.read(|c, _, _| c.clone());
                reply!(json(StatusCode::OK, move |w| cc_api::write_parameters(
                    &cfg,
                    &mut Writer::new(w)
                )));
            }
            ("GET", "/api/parameter-help") => {
                let mut v = [0u8; 96];
                let Some(key) = cc_api::form_value(raw_query, "param", &mut v) else {
                    reply!(message(StatusCode::new(422), false, "parameter is missing"));
                };
                if cc_core::config::find(key).is_none() {
                    reply!(message(StatusCode::NOT_FOUND, false, "parameter not found"));
                }
                let mut key_owned: heapless::String<96> = heapless::String::new();
                let _ = key_owned.push_str(key);
                reply!(json(StatusCode::OK, move |w| cc_api::write_parameter_help(
                    &key_owned,
                    &mut Writer::new(w)
                )
                .unwrap_or(Ok(()))));
            }
            ("GET", "/api/config" | "/api/config/download") => {
                let cfg = b.read(|c, _, _| c.clone());
                let download = path.ends_with("download");
                let body = Json(move |w: &mut dyn core::fmt::Write| {
                    let mut wr = if download {
                        Writer::pretty(w)
                    } else {
                        Writer::new(w)
                    };
                    cc_api::write_config(&cfg, &mut wr)
                });
                if download {
                    reply!(Response::ok(body).with_headers([
                        ("Content-Disposition", "attachment; filename=\"config.json\""),
                        ("Access-Control-Allow-Origin", "*"),
                    ]));
                }
                reply!(Response::ok(body).with_headers(CORS));
            }
            ("GET", "/api/ota/status") => {
                let s = b.ota_status();
                reply!(json(StatusCode::OK, move |w| {
                    let mut w = Writer::new(w);
                    w.begin_object()?;
                    w.kv_str("status", s.status)?;
                    w.kv_int("progress", i64::from(s.progress))?;
                    w.kv_bool("updateInProgress", s.in_progress)?;
                    w.kv_bool("updating", s.in_progress)?;
                    w.kv_str("type", "firmware")?;
                    w.kv_int("uploadedSize", i64::from(s.uploaded))?;
                    w.kv_int("totalSize", i64::from(s.total))?;
                    if let Some(e) = s.error {
                        w.kv_str("error", e)?;
                    }
                    w.end_object()
                }));
            }
            ("POST", "/api/setpoint") => {
                let body = read_small_body(&mut request).await;
                let mut v = [0u8; 32];
                let mut v2 = [0u8; 32];
                let value = cc_api::form_value(raw_query, "value", &mut v).or_else(|| {
                    body.as_deref()
                        .and_then(|bd| cc_api::form_value(bd, "value", &mut v2))
                });
                let Some(value) = value.and_then(|s| s.parse::<f64>().ok()) else {
                    reply!(message(StatusCode::BAD_REQUEST, false, "No value provided"));
                };
                match b.command(Command::SetBrewSetpoint(value)) {
                    Reply::Ok => reply!(message(StatusCode::OK, true, "Setpoint updated")),
                    _ => reply!(message(StatusCode::BAD_REQUEST, false, "Invalid setpoint value")),
                }
            }
            ("POST", "/api/wake") => {
                b.command(Command::Wake);
                reply!(message(StatusCode::OK, true, "Waking up"));
            }
            ("POST", "/api/sleep") => {
                b.command(Command::Sleep);
                reply!(message(StatusCode::OK, true, "Going to standby"));
            }
            ("POST", "/api/steam") => {
                let Reply::Steam(on) = b.command(Command::ToggleSteam) else {
                    unreachable!()
                };
                reply!(flag("steamMode", on));
            }
            ("POST", "/api/pid") => {
                let Reply::Pid(on) = b.command(Command::TogglePid) else {
                    unreachable!()
                };
                reply!(flag("pidEnabled", on));
            }
            ("POST", "/api/backflush") => match b.command(Command::ToggleBackflush) {
                Reply::Backflush(on) => reply!(flag("backflushOn", on)),
                Reply::Rejected(msg) => reply!(message(StatusCode::BAD_REQUEST, false, msg)),
                _ => reply!(message(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    false,
                    "Internal server error"
                )),
            },
            ("POST", "/api/maintenance/reset-backflush-counter") => {
                b.command(Command::ResetShotCounter);
                let st = b.read(|c, s, _| (s.shots_since_backflush, s.backflush_reminder_due(c)));
                let mut s: heapless::String<128> = heapless::String::new();
                let _ = write!(
                    s,
                    "{{\"success\":true,\"shotsSinceBackflush\":{},\"backflushReminderDue\":{}}}",
                    st.0, st.1
                );
                reply!(Response::new(StatusCode::OK, Owned(s)).with_headers(CORS));
            }
            ("POST", "/api/scale/tare" | "/api/scale/calibration") => {
                if !b.read(|c, _, _| c.hardware_sensors_scale_enabled) {
                    reply!(message(StatusCode::NOT_FOUND, false, "Scale is not enabled"));
                }
                match b.command(if path.ends_with("tare") {
                    Command::ToggleScaleTare
                } else {
                    Command::ToggleScaleCalibration
                }) {
                    Reply::ScaleTare(v) => reply!(flag("scaleTareOn", v)),
                    Reply::ScaleCalibration(v) => reply!(flag("scaleCalibrationOn", v)),
                    _ => reply!(message(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        false,
                        "Internal server error"
                    )),
                }
            }
            ("POST", "/api/parameters") => {
                let body = read_small_body(&mut request).await;
                let src = match body.as_deref() {
                    Some(bd) if !bd.is_empty() => bd,
                    _ => raw_query,
                };
                let mut result = Ok(false);
                let mut src_owned: alloc::string::String = alloc::string::String::from(src);
                b.update_config(&mut |cfg| {
                    result = apply_form(cfg, &src_owned);
                    matches!(result, Ok(true))
                });
                src_owned.clear();
                match result {
                    Ok(true) => {
                        b.command(Command::ConfigUpdated);
                        reply!(message(StatusCode::OK, true, "Parameters updated and saved"));
                    }
                    Ok(false) => reply!(message(StatusCode::OK, true, "No parameters updated")),
                    Err(e) => reply!(message(StatusCode::BAD_REQUEST, false, e.as_str())),
                }
            }
            ("POST", "/api/config/upload") => {
                let len = request.body_connection.content_length();
                if len > MAX_CONFIG_UPLOAD {
                    reply!(upload_reply(
                        StatusCode::new(413),
                        false,
                        "Configuration file too large"
                    ));
                }
                let mut buf = alloc::vec![0u8; len];
                let mut reader = request.body_connection.body().reader();
                let mut got = 0;
                while got < len {
                    match reader.read(&mut buf[got..]).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => got += n,
                    }
                }
                buf.truncate(got);
                let mut outcome = Err(cc_api::ImportError::NothingToImport);
                b.update_config(&mut |cfg| {
                    outcome = cc_api::import_config(&buf, cfg);
                    outcome.is_ok()
                });
                match outcome {
                    Ok(_) => {
                        b.command(Command::ConfigUpdated);
                        reply!(upload_reply(
                            StatusCode::OK,
                            true,
                            "Configuration validated and applied successfully."
                        ));
                    }
                    Err(e) => {
                        let mut s: heapless::String<160> = heapless::String::new();
                        let _ = write!(s, "{e}");
                        reply!(upload_reply_owned(StatusCode::BAD_REQUEST, &s));
                    }
                }
            }
            ("POST", "/api/restart") => {
                let resp = message(StatusCode::OK, true, "Restarting...");
                let sent = resp
                    .write_to(request.body_connection.finalize().await?, response_writer)
                    .await;
                b.restart();
                sent
            }
            ("POST", "/api/factory-reset") => {
                let resp = message(StatusCode::OK, true, "Factory reset. Restarting...");
                let sent = resp
                    .write_to(request.body_connection.finalize().await?, response_writer)
                    .await;
                b.factory_reset();
                sent
            }
            #[cfg(feature = "bench")]
            ("POST", "/api/debug/hang") => {
                let resp = message(StatusCode::OK, true, "Hanging the control loop");
                let sent = resp
                    .write_to(request.body_connection.finalize().await?, response_writer)
                    .await;
                b.debug_hang();
                sent
            }
            ("POST", "/api/wifi-reset") => {
                let resp = message(
                    StatusCode::OK,
                    true,
                    "WiFi settings are being reset. Rebooting...",
                );
                let sent = resp
                    .write_to(request.body_connection.finalize().await?, response_writer)
                    .await;
                b.wifi_reset();
                sent
            }
            ("POST", "/api/ota/firmware") => {
                let result = ota_upload(b, &mut request).await;
                let code = match result {
                    Ok(()) => StatusCode::OK,
                    Err(OtaError::Busy) => StatusCode::new(409),
                    Err(_) => StatusCode::BAD_REQUEST,
                };
                let msg = result
                    .err()
                    .map_or("Firmware update successful. Restarting...", OtaError::message);
                let resp = message(code, result.is_ok(), msg);
                let sent = resp
                    .write_to(request.body_connection.finalize().await?, response_writer)
                    .await;
                if result.is_ok() {
                    b.restart();
                }
                sent
            }
            ("POST", "/api/ota/filesystem" | "/api/ota/url") => {
                reply!(message(
                    StatusCode::new(410),
                    false,
                    "Not supported: the web UI is part of the firmware now. Upload a firmware image."
                ));
            }
            ("GET", "/events") => {
                reply!(EventStream(Events { backend: b }));
            }
            ("GET", "/") => {
                reply!(Response::new(StatusCode::FOUND, "").with_headers([("Location", "/ui/")]));
            }
            (_, p) if p.starts_with("/api/") => {
                let known = ROUTES.iter().any(|(_, r)| *r == p);
                if known {
                    reply!(message(
                        StatusCode::METHOD_NOT_ALLOWED,
                        false,
                        "Method not allowed"
                    ));
                }
                reply!(message(StatusCode::NOT_FOUND, false, "API endpoint not found"));
            }
            ("GET", p) if p == "/ui" || p.starts_with("/ui/") => {
                let rel = p.trim_start_matches("/ui").trim_start_matches('/');
                // SPA: paths without an extension are routes, served index.html.
                let file = if rel.is_empty() || !rel.rsplit('/').next().unwrap_or("").contains('.') {
                    "index.html"
                } else {
                    rel
                };
                if let Some(a) = b.asset(file) {
                    let cache = if a.immutable {
                        "public, max-age=31536000, immutable"
                    } else {
                        "no-cache"
                    };
                    let resp = Response::ok(Bytes {
                        data: a.bytes,
                        content_type: a.content_type,
                    })
                    .with_headers([
                        ("Cache-Control", cache),
                        ("Content-Encoding", if a.gzip { "gzip" } else { "identity" }),
                    ]);
                    reply!(resp);
                }
                reply!(Response::new(StatusCode::NOT_FOUND, "Not found"));
            }
            // Anything else goes to the UI: phones probing for a captive portal
            // (`/generate_204`, `/hotspot-detect.html`) then open the setup page.
            ("GET", _) => {
                reply!(Response::new(StatusCode::FOUND, "").with_headers([("Location", "/ui/")]));
            }
            _ => reply!(Response::new(StatusCode::NOT_FOUND, "Not found")),
        }
    }
}

/// Known API routes (for 405 versus 404).
pub const ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/status"),
    ("GET", "/api/health"),
    ("GET", "/api/temperatures"),
    ("GET", "/api/history"),
    ("GET", "/api/parameters"),
    ("POST", "/api/parameters"),
    ("GET", "/api/parameter-help"),
    ("GET", "/api/config"),
    ("GET", "/api/config/download"),
    ("POST", "/api/config/upload"),
    ("GET", "/api/ota/status"),
    ("POST", "/api/ota/firmware"),
    ("POST", "/api/setpoint"),
    ("POST", "/api/wake"),
    ("POST", "/api/sleep"),
    ("POST", "/api/steam"),
    ("POST", "/api/pid"),
    ("POST", "/api/backflush"),
    ("POST", "/api/maintenance/reset-backflush-counter"),
    ("POST", "/api/scale/tare"),
    ("POST", "/api/scale/calibration"),
    ("POST", "/api/restart"),
    ("POST", "/api/factory-reset"),
    ("POST", "/api/wifi-reset"),
];

fn upload_reply(
    status: StatusCode,
    ok: bool,
    msg: &str,
) -> Response<impl picoserve::response::HeadersIter + use<>, impl picoserve::response::Body + use<>> {
    let mut s: heapless::String<256> = heapless::String::new();
    let mut w = Writer::new(&mut s);
    let _ = w.begin_object();
    let _ = w.kv_bool("success", ok);
    let _ = w.kv_str("message", msg);
    let _ = w.kv_bool("restart", ok);
    let _ = w.end_object();
    Response::new(status, Owned(s)).with_headers(CORS)
}

fn upload_reply_owned(
    status: StatusCode,
    msg: &heapless::String<160>,
) -> Response<impl picoserve::response::HeadersIter + use<>, impl picoserve::response::Body + use<>> {
    upload_reply(status, false, msg)
}

/// Applies a form body to `cfg`, all-or-nothing. `Ok(true)` if anything changed.
#[allow(clippy::result_large_err)] // a stack string, not worth boxing in no_std
pub fn apply_form(cfg: &mut Config, form: &str) -> Result<bool, heapless::String<128>> {
    let mut next = cfg.clone();
    let mut changed = false;
    let mut kb = [0u8; 96];
    let mut vb = [0u8; 256];
    let mut pairs = FormPairs::new(form);
    while let Some(p) = pairs.next_pair(&mut kb, &mut vb) {
        let Ok((k, v)) = p else {
            return Err(text("Malformed form data"));
        };
        if v.is_empty() {
            continue;
        }
        match next.set_from_str(k, v) {
            Ok(c) => changed |= c,
            Err(e) => {
                let mut s = heapless::String::new();
                let _ = write!(s, "{k}: {e}");
                return Err(s);
            }
        }
    }
    *cfg = next;
    Ok(changed)
}

fn text(s: &str) -> heapless::String<128> {
    let mut o = heapless::String::new();
    let _ = o.push_str(s);
    o
}

/// Reads a small (≤ 2 KB) body as text, for form posts.
/// Buffers live on the heap: an HTTP worker's future stays small (they are static).
async fn read_small_body<R: Read>(request: &mut Request<'_, R>) -> Option<alloc::string::String> {
    let len = request.body_connection.content_length();
    if len == 0 || len > 2048 {
        return None;
    }
    let mut buf = alloc::vec![0u8; len];
    let mut reader = request.body_connection.body().reader();
    let mut got = 0;
    while got < len {
        match reader.read(&mut buf[got..len]).await {
            Ok(0) | Err(_) => break,
            Ok(n) => got += n,
        }
    }
    buf.truncate(got);
    alloc::string::String::from_utf8(buf).ok()
}

async fn ota_upload<B: Backend, R: Read>(b: &B, request: &mut Request<'_, R>) -> Result<(), OtaError> {
    let ct = request
        .parts
        .headers()
        .get("Content-Type")
        .and_then(|h| h.as_str().ok())
        .unwrap_or("");
    let boundary = multipart::boundary_from_content_type(ct).ok_or(OtaError::Invalid)?;
    let mut parser = multipart::Parser::new(boundary).map_err(|_| OtaError::Invalid)?;
    b.ota_begin()?;
    let mut reader = request.body_connection.body().reader();
    let mut buf = alloc::vec![0u8; 1024];
    let mut in_file = false;
    let mut saw_file = false;
    // Data is copied out of the parser callback and written after it returns, because
    // the backend write is async.
    let mut pending = alloc::vec![0u8; 1024 + 128];
    loop {
        let n = match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                b.ota_abort(OtaError::Write);
                return Err(OtaError::Write);
            }
        };
        let mut plen = 0usize;
        let mut overflow = false;
        let res = parser.push(&buf[..n], &mut |e| match e {
            multipart::Event::Part { name, .. } => {
                in_file = name == "firmware";
                saw_file |= in_file;
            }
            multipart::Event::Data(d) if in_file => {
                if plen + d.len() <= pending.len() {
                    pending[plen..plen + d.len()].copy_from_slice(d);
                    plen += d.len();
                } else {
                    overflow = true;
                }
            }
            multipart::Event::PartEnd => in_file = false,
            _ => {}
        });
        if res.is_err() || overflow {
            b.ota_abort(OtaError::Invalid);
            return Err(OtaError::Invalid);
        }
        if plen > 0
            && let Err(e) = b.ota_write(&pending[..plen]).await
        {
            b.ota_abort(e);
            return Err(e);
        }
        if parser.is_done() {
            break;
        }
    }
    if !saw_file || !parser.is_done() {
        b.ota_abort(OtaError::Invalid);
        return Err(OtaError::Invalid);
    }
    b.ota_finish().await
}
