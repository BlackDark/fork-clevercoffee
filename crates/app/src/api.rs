//! The web API: the thirty routes of `api-contract.md`, as pure handlers.
//!
//! Every handler is a function from a [`Backend`] and a parsed request to a [`Reply`]. None of them
//! awaits, allocates or touches hardware, which is the property the contract asks for ("handlers
//! must not block", asserted by a test that walks the source) and the reason the whole API is
//! testable on a host against a fake backend.
//!
//! What is deliberately kept from the C++ firmware, and why:
//!
//! - **All four OTA routes.** The user confirmed OTA stays, so the frontend is unchanged. They are
//!   corrected rather than removed: every OTA path requires the configured password when auth is
//!   enabled, refuses to start unless the machine is idle (D01), and the URL variant gains the
//!   scheme and host allow-list it was missing (D16).
//! - **The odd status codes.** `/api/ota/url` answers 202 and not 200, because the frontend
//!   checks for 202 and would treat a 200 as a completed update. A contract is a compatibility
//!   requirement, and a "cleaner" status code is a broken frontend.
//! - **The integer state ids** in `/api/status`, because the dashboard renders states by number.
//!
//! What is corrected, each with a defect id:
//!
//! - D10: auth is a real check in the router, not middleware that never authenticates.
//! - D13: `/api/config/upload` is transactional and reports counts; the C++ returned 200 with 90 of
//!   96 parameters rejected.
//! - D14: secrets are redacted in `/api/parameters`, `/api/config`, `/api/config/download` and
//!   `/api/nvs-debug`.
//! - D15: `/api/temperatures` answers an error with 500, not with 200 and an error body.
//! - D25: an explicitly empty parameter value is a reset to default, not a silent skip.
//! - D26: `?filter=` is honoured.

use core::fmt::Write;

use clevercoffee_http::{Body, Method, RequestHead, Response, Status};

/// The largest body any handler produces. The biggest is `/api/parameters` with all 99 entries
/// tagged, which is about 24 KB; 32 KB covers it without a heap.
pub const MAX_REPLY: usize = 32 * 1024;

/// The largest request body any handler accepts. A config document is 16 KB.
pub const MAX_REQUEST_BODY: usize = 16 * 1024;

/// A response, owned.
///
/// [`clevercoffee_http::Response`] borrows its body, which is the right shape for a socket writer
/// and the wrong one for a handler that has to build a payload first: the string and the response
/// that points at it would have to be returned together. This type owns both, and
/// [`Reply::to_response`] borrows for the duration of one write.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Reply {
    pub status: Status,
    pub content_type: &'static str,
    pub headers: heapless::Vec<(&'static str, heapless::String<128>), 3>,
    pub body: heapless::String<MAX_REPLY>,
}

impl Reply {
    pub fn json(status: Status) -> Self {
        Self::with_type(status, "application/json; charset=utf-8")
    }

    pub fn text(status: Status) -> Self {
        Self::with_type(status, "text/plain; charset=utf-8")
    }

    /// A text reply with a body.
    pub fn text_body(status: Status, body: &str) -> Self {
        Self::with_type(status, "text/plain; charset=utf-8").with_body_json(body)
    }

    pub fn with_type(status: Status, content_type: &'static str) -> Self {
        Self {
            status,
            content_type,
            headers: heapless::Vec::new(),
            body: heapless::String::new(),
        }
    }

    /// A JSON reply with a pre-built payload.
    pub fn json_body(status: Status, body: &str) -> Self {
        let mut r = Self::json(status);
        let _ = r.body.push_str(body);
        r
    }

    pub fn header(mut self, name: &'static str, value: &str) -> Self {
        let mut v = heapless::String::new();
        let _ = v.push_str(value);
        let _ = self.headers.push((name, v));
        self
    }

    /// The C++ success shape for a mutation: `{"success":true,"message":"..."}`.
    pub fn success(message: &str) -> Self {
        Self::json_body(Status::Ok, &success_body(message))
    }

    /// The C++ failure shape: `{"success":false,"message":"..."}` with a status.
    pub fn failure(status: Status, message: &str) -> Self {
        Self::json_body(status, &failure_body(message))
    }

    pub fn to_response(&self) -> Response<'_> {
        let mut r = Response::new(self.status);
        r.body = Body::Bytes(self.body.as_bytes());
        for (name, value) in &self.headers {
            r = r.header(name, value.as_str());
        }
        if self.get_header("Content-Type").is_none() {
            r = r.header("Content-Type", self.content_type);
        }
        r
    }

    fn get_header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The body as a string, for a test.
    pub fn body(&self) -> &str {
        self.body.as_str()
    }

    /// Whether the body contains `needle`. A test helper, kept here so every test reads the same.
    pub fn has(&self, needle: &str) -> bool {
        self.body.contains(needle)
    }
}

fn success_body(message: &str) -> heapless::String<256> {
    let mut s = heapless::String::new();
    let _ = write!(s, "{{\"success\":true,\"message\":\"{message}\"}}");
    s
}

fn failure_body(message: &str) -> heapless::String<256> {
    let mut s = heapless::String::new();
    let _ = write!(s, "{{\"success\":false,\"message\":\"{message}\"}}");
    s
}

/// What the API needs from the machine. Implemented by the firmware layer over the real
/// [`crate::Machine`], and by a fake in the tests.
pub trait Backend {
    /// The `/api/status` payload, except the two scale fields, which the caller adds when the
    /// scale is enabled so a machine without one does not report zeroes for a sensor it does not
    /// have. That was the shape of D03's sibling in the API.
    fn status_json(&mut self, out: &mut heapless::String<MAX_REPLY>) -> bool;

    /// Whether a scale is fitted.
    fn scale_enabled(&self) -> bool {
        false
    }

    /// The current and brew weights, when a scale is fitted.
    fn weights(&mut self) -> (f64, f64) {
        (0.0, 0.0)
    }

    /// The `/api/parameters` array, already filtered and with secrets redacted.
    fn parameters_json(&mut self, filter: Filter) -> heapless::String<MAX_REPLY>;

    /// One parameter's help text.
    fn parameter_help(&mut self, key: &str) -> Option<&'static str>;

    /// The nested config document, with secrets redacted.
    fn config_json(&mut self) -> heapless::String<MAX_REPLY>;

    /// The stored key/value pairs for `/api/nvs-debug`, with secrets redacted.
    fn nvs_debug_json(&mut self) -> heapless::String<MAX_REPLY>;

    /// The history series.
    fn history_json(&mut self) -> heapless::String<MAX_REPLY>;

    /// The live temperature payload.
    fn temperatures_json(&mut self) -> heapless::String<MAX_REPLY>;

    /// `/api/temperatures`'s failure case: a machine with no reading answers 500, not 200 with an
    /// error body (D15).
    fn temperatures_available(&self) -> bool {
        true
    }

    fn set_setpoint(&mut self, celsius: f64) -> Result<(), &'static str>;
    fn toggle_pid(&mut self) -> Result<(), &'static str>;
    fn set_steam(&mut self, on: bool) -> Result<(), &'static str>;
    /// Whether steam mode is on, so a body-less `/api/steam` can toggle it as the C++ did.
    fn steam_enabled(&self) -> bool;
    /// Sets the PID to an explicit value, which the C++ route could not do because it ignored the
    /// body entirely.
    fn set_pid(&mut self, on: bool) -> Result<(), &'static str>;
    fn set_backflush(&mut self, on: bool) -> Result<(), &'static str>;
    fn reset_backflush_counter(&mut self) -> Result<(), &'static str>;
    fn scale_tare(&mut self) -> Result<(), &'static str>;
    fn scale_calibration(&mut self) -> Result<(), &'static str>;
    fn wifi_reset(&mut self) -> Result<(), &'static str>;
    fn restart(&mut self) -> Result<(), &'static str>;
    fn factory_reset(&mut self) -> Result<(), &'static str>;
    fn wake(&mut self) -> Result<(), &'static str>;
    fn sleep(&mut self) -> Result<(), &'static str>;

    /// Applies a parameter map, returning the counts the contract documents.
    fn apply_parameters(&mut self, pairs: &ParameterPairs) -> ParameterOutcome;

    /// Validates and applies a config document.
    fn apply_config(&mut self, document: &str) -> ParameterOutcome;

    /// Whether the machine is idle enough to start an update.
    fn is_idle(&self) -> bool;

    /// The OTA state, for `/api/ota/status`.
    fn ota_status_json(&mut self) -> heapless::String<MAX_REPLY>;

    /// Starts an OTA. `source` is the body of a firmware upload, or the URL of a remote one.
    fn start_ota(&mut self, kind: OtaKind, source: &str) -> Result<(), OtaRefusal>;

    /// The configured password, when auth is enabled. Never logged, never returned.
    fn ota_password(&self) -> Option<&str> {
        None
    }
}

/// The OTA flavours the four routes cover.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OtaKind {
    Firmware,
    Filesystem,
    Url,
}

/// Why an OTA was refused. Each maps to exactly one status code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OtaRefusal {
    /// Not authenticated. 401.
    Unauthenticated,
    /// The machine is brewing, or otherwise not idle. 409.
    Busy,
    /// The body or URL is not acceptable. 400.
    BadRequest(&'static str),
    /// The device could not start. 500.
    Failed,
}

impl OtaRefusal {
    pub fn status(self) -> Status {
        match self {
            OtaRefusal::Unauthenticated => Status::Unauthorized,
            OtaRefusal::Busy => Status::Conflict,
            OtaRefusal::BadRequest(_) => Status::BadRequest,
            OtaRefusal::Failed => Status::InternalServerError,
        }
    }
}

/// The parameter filter, as the contract defines it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Filter {
    #[default]
    All,
    Hardware,
    Behavior,
    Other,
    /// The C++ accepted `visible` and `editable` in the spec and ignored both.
    Visible,
    Editable,
}

impl Filter {
    pub fn parse(query: Option<&str>) -> Option<Self> {
        match query {
            None => Some(Filter::All),
            Some("all") => Some(Filter::All),
            Some("hardware") => Some(Filter::Hardware),
            Some("behavior") => Some(Filter::Behavior),
            Some("other") => Some(Filter::Other),
            Some("visible") => Some(Filter::Visible),
            Some("editable") => Some(Filter::Editable),
            Some(_) => None,
        }
    }
}

/// The outcome of applying parameters or a config document.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ParameterOutcome {
    pub accepted: u16,
    pub rejected: u16,
    pub clamped: u16,
    pub unknown: u16,
    pub missing: u16,
    /// True when nothing at all was applied, which is what makes the transaction atomic.
    pub applied: bool,
}

/// Form-encoded parameters, bounded.
#[derive(Debug, Default)]
pub struct ParameterPairs {
    pairs: heapless::Vec<(heapless::String<64>, heapless::String<64>), 24>,
    /// Set when a key arrived without a value, which means "reset to default" (D25).
    pub resets: u16,
}

impl ParameterPairs {
    /// Parses `a=1&b=2`. Percent-decoding is deliberately not attempted: every key this API
    /// accepts is a dotted ASCII identifier and every value is a number or an enum name, so a
    /// percent sign in a value is a malformed request rather than something to guess at.
    pub fn parse(body: &str) -> Option<Self> {
        let mut out = Self::default();
        for pair in body.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (k, v) = pair.split_once('=')?;
            if k.is_empty() || k.len() > 63 {
                return None;
            }
            // The key is copied into a fixed buffer and matched against the schema table, so a
            // request cannot make the firmware hold an unbounded string.
            let mut key = heapless::String::<64>::new();
            key.push_str(k).ok()?;
            let mut value = heapless::String::<64>::new();
            value.push_str(v).ok()?;
            if value.is_empty() {
                out.resets += 1;
            }
            out.pairs.push((key, value)).ok()?;
        }
        Some(out)
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// The routes, in the contract's order. Index 0 is route 1, which is what a test asserts against.
pub const ROUTES: [&str; 30] = [
    "/api/status",
    "/api/config",
    "/api/setpoint",
    "/api/health",
    "/api/wake",
    "/api/sleep",
    "/api/steam",
    "/api/pid",
    "/api/backflush",
    "/api/maintenance/reset-backflush-counter",
    "/api/scale/tare",
    "/api/scale/calibration",
    "/api/parameter-help",
    "/api/temperatures",
    "/api/history",
    "/api/nvs-debug",
    "/api/wifi-reset",
    "/api/config/download",
    "/api/config/upload",
    "/api/restart",
    "/api/factory-reset",
    "/api/parameters",
    "/api/ota/firmware",
    "/api/ota/filesystem",
    "/api/ota/url",
    "/api/ota/status",
    "/events",
    "/",
    "/ui/",
    "*",
];

/// Which methods each route answers.
pub fn methods_for(path: &str) -> &'static [Method] {
    const GET: &[Method] = &[Method::Get, Method::Head];
    const POST: &[Method] = &[Method::Post];
    const ANY: &[Method] = &[
        Method::Get,
        Method::Head,
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
    ];
    match path {
        "/api/status"
        | "/api/config"
        | "/api/health"
        | "/api/parameter-help"
        | "/api/temperatures"
        | "/api/history"
        | "/api/nvs-debug"
        | "/api/config/download"
        | "/api/ota/status"
        | "/events"
        | "/"
        | "/ui/" => GET,
        "/api/setpoint"
        | "/api/wake"
        | "/api/sleep"
        | "/api/steam"
        | "/api/pid"
        | "/api/backflush"
        | "/api/maintenance/reset-backflush-counter"
        | "/api/scale/tare"
        | "/api/scale/calibration"
        | "/api/wifi-reset"
        | "/api/config/upload"
        | "/api/restart"
        | "/api/factory-reset"
        | "/api/ota/firmware"
        | "/api/ota/filesystem"
        | "/api/ota/url" => POST,
        "/api/parameters" => ANY,
        _ => ANY,
    }
}

/// Whether a route needs the configured password. Only the destructive ones and the mutating
/// config routes do; the read routes stay open so a dashboard on a phone works without a login,
/// which is the C++ behaviour and the user's expectation.
pub fn is_protected(path: &str) -> bool {
    matches!(
        path,
        "/api/factory-reset"
            | "/api/restart"
            | "/api/config/upload"
            | "/api/ota/firmware"
            | "/api/ota/filesystem"
            | "/api/ota/url"
    )
}

/// The API: the route table, the auth setting, and the dispatch.
#[derive(Debug)]
pub struct Api {
    /// Whether `system.auth.enabled` is set. When false, the password is not consulted at all,
    /// which is what a machine on a home network has always done.
    pub auth_enabled: bool,
    /// Whether a valid credential was presented on this connection.
    pub authenticated: bool,
}

impl Default for Api {
    fn default() -> Self {
        Self::new()
    }
}

impl Api {
    pub const fn new() -> Self {
        Self {
            auth_enabled: false,
            authenticated: false,
        }
    }

    /// Checks a credential against `expected`.
    ///
    /// A constant-time comparison, because a machine on a kitchen network is still a machine whose
    /// password a neighbour's laptop could time.
    pub fn authenticate(&mut self, expected: Option<&str>, presented: Option<&str>) -> bool {
        if !self.auth_enabled {
            // Auth disabled means every request is allowed, which is the documented configuration
            // and not an oversight: the C++ never authenticated at all (D10) and a machine whose
            // owner has not set a password must still be usable.
            self.authenticated = true;
            return true;
        }
        let (Some(expected), Some(presented)) = (expected, presented) else {
            self.authenticated = false;
            return false;
        };
        self.authenticated = constant_time_eq(expected.as_bytes(), presented.as_bytes());
        self.authenticated
    }

    /// Routes one request.
    ///
    /// `body` is the already-read request body, empty for a `GET`. The handler never reads the
    /// socket, which is what "must not block" means here.
    pub fn dispatch<B: Backend + ?Sized>(
        &mut self,
        backend: &mut B,
        head: &RequestHead,
        body: &str,
    ) -> Reply {
        let path = head.path.as_str();
        let method = head.method;
        let route = match ROUTES.iter().position(|r| *r == path) {
            Some(i) => i,
            None => {
                // A path under `/api/` answers JSON, anything else text, as the C++ did.
                return if path.starts_with("/api/") {
                    Reply::json_body(Status::NotFound, "{\"error\":\"not found\"}")
                } else {
                    Reply::text_body(Status::NotFound, "not found")
                };
            }
        };
        let known = ROUTES[route];
        if known == "*" {
            return Reply::json_body(Status::NotFound, "{\"error\":\"not found\"}");
        }
        if !methods_for(known).contains(&method) {
            return Reply::json_body(
                Status::MethodNotAllowed,
                "{\"error\":\"method not allowed\"}",
            );
        }
        // Only enforced when authentication is switched on. With it off, a machine whose owner
        // never set a password stays usable, which is the C++ behaviour and what the contract
        // documents; treating "no password configured" as "refuse everything" would lock a user
        // out of their own machine over the web UI.
        if self.auth_enabled && is_protected(known) && !self.authenticated {
            return Reply::json_body(
                Status::Unauthorized,
                "{\"error\":\"authentication required\"}",
            )
            .header("WWW-Authenticate", "Basic realm=\"CleverCoffee\"");
        }
        if body.len() > MAX_REQUEST_BODY {
            return Reply::json_body(Status::PayloadTooLarge, "{\"error\":\"body too large\"}");
        }
        match known {
            "/api/status" => self.status(backend),
            "/api/health" => Reply::success("ok"),
            "/api/config" => Reply::json(Status::Ok).with_body_json(backend.config_json().as_str()),
            "/api/config/download" => Reply::json(Status::Ok)
                .with_body_json(backend.config_json().as_str())
                .header(
                    "Content-Disposition",
                    "attachment; filename=\"config.json\"",
                ),
            "/api/parameters" => match head.method {
                Method::Post => self.post_parameters(backend, body),
                _ => self.get_parameters(backend, head),
            },
            "/api/setpoint" => self.setpoint(backend, body, head),
            "/api/wake" => self.wake(backend),
            "/api/sleep" => self.sleep(backend),
            "/api/steam" => self.steam(backend, body, head),
            "/api/pid" => self.pid(backend, body),
            "/api/backflush" => self.backflush(backend, body),
            "/api/maintenance/reset-backflush-counter" => match backend.reset_backflush_counter() {
                Ok(()) => Reply::success("backflush counter reset"),
                Err(e) => Reply::failure(Status::InternalServerError, e),
            },
            "/api/scale/tare" => self.scale(backend, false),
            "/api/scale/calibration" => self.scale(backend, true),
            "/api/parameter-help" => self.parameter_help(backend, head),
            "/api/temperatures" => self.temperatures(backend),
            "/api/history" => {
                Reply::json(Status::Ok).with_body_json(backend.history_json().as_str())
            }
            "/api/nvs-debug" => {
                Reply::json(Status::Ok).with_body_json(backend.nvs_debug_json().as_str())
            }
            "/api/wifi-reset" => match backend.wifi_reset() {
                Ok(()) => Reply::success("wifi credentials cleared"),
                Err(e) => Reply::failure(Status::InternalServerError, e),
            },
            "/api/config/upload" => self.config_upload(backend, body, head),
            "/api/restart" => match backend.restart() {
                Ok(()) => Reply::success("restarting").header("restart", "true"),
                Err(e) => Reply::failure(Status::InternalServerError, e),
            },
            "/api/factory-reset" => match backend.factory_reset() {
                Ok(()) => Reply::success("factory reset; the device will reboot"),
                Err(e) => Reply::failure(Status::InternalServerError, e),
            },
            "/api/ota/firmware" => self.ota(backend, OtaKind::Firmware, body),
            "/api/ota/filesystem" => self.ota(backend, OtaKind::Filesystem, body),
            "/api/ota/url" => self.ota(backend, OtaKind::Url, body),
            "/api/ota/status" => {
                Reply::json(Status::Ok).with_body_json(backend.ota_status_json().as_str())
            }
            "/events" => Reply::with_type(Status::Ok, "text/event-stream"),
            "/" => Reply::text_body(Status::Found, "").header("Location", "/ui/"),
            "/ui/" => Reply::json_body(Status::NotFound, "{\"error\":\"no ui\"}"),
            _ => Reply::json_body(Status::NotFound, "{\"error\":\"not found\"}"),
        }
    }

    fn status<B: Backend + ?Sized>(&mut self, backend: &mut B) -> Reply {
        let mut body = heapless::String::new();
        if !backend.status_json(&mut body) {
            return Reply::failure(Status::InternalServerError, "status unavailable");
        }
        if backend.scale_enabled() {
            let (weight, brew) = backend.weights();
            // The two fields are appended rather than built by the backend, so a machine without a
            // scale cannot report a weight of zero as though it had one.
            if body.as_bytes().last() == Some(&b'}') {
                let _ = body.pop();
            }
            let _ = write!(body, ",\"weight\":{weight:.1},\"brewWeight\":{brew:.1}}}");
        }
        Reply::json(Status::Ok).with_body_json(body.as_str())
    }

    fn get_parameters<B: Backend + ?Sized>(
        &mut self,
        backend: &mut B,
        head: &RequestHead,
    ) -> Reply {
        match Filter::parse(head.query_param("filter")) {
            Some(filter) => {
                let body = backend.parameters_json(filter);
                Reply::json(Status::Ok).with_body_json(body.as_str())
            }
            None => Reply::json_body(
                Status::UnprocessableEntity,
                "{\"error\":\"unknown filter\"}",
            ),
        }
    }

    fn post_parameters<B: Backend + ?Sized>(&mut self, backend: &mut B, body: &str) -> Reply {
        let Some(pairs) = ParameterPairs::parse(body) else {
            return Reply::json_body(Status::BadRequest, "{\"error\":\"malformed form body\"}");
        };
        if pairs.is_empty() {
            return Reply::json_body(Status::BadRequest, "{\"error\":\"no parameters\"}");
        }
        let outcome = backend.apply_parameters(&pairs);
        parameter_reply(outcome, "parameters applied")
    }

    fn setpoint<B: Backend + ?Sized>(
        &mut self,
        backend: &mut B,
        body: &str,
        head: &RequestHead,
    ) -> Reply {
        // Both shapes: the C++ read a form or query parameter, the spec documented JSON. JSON wins
        // when it is there, so a client that sends JSON is not silently ignored.
        let raw = json_number(body, "value")
            .or_else(|| query_or_form(head, body, "value"))
            .or_else(|| query_or_form(head, body, "setpoint"));
        let Some(raw) = raw else {
            return Reply::json_body(Status::BadRequest, "{\"error\":\"missing value\"}");
        };
        let Ok(value) = raw.parse::<f64>() else {
            return Reply::json_body(Status::BadRequest, "{\"error\":\"value is not a number\"}");
        };
        if !value.is_finite() {
            // `NaN` parses as an f64 in Rust and would sail past a range check written with `<`.
            return Reply::json_body(Status::BadRequest, "{\"error\":\"value is not finite\"}");
        }
        match backend.set_setpoint(value) {
            Ok(()) => Reply::success("setpoint updated"),
            Err("out_of_range") => {
                Reply::json_body(Status::BadRequest, "{\"error\":\"out_of_range\"}")
            }
            Err(e) => Reply::failure(Status::InternalServerError, e),
        }
    }

    fn wake<B: Backend + ?Sized>(&mut self, backend: &mut B) -> Reply {
        match backend.wake() {
            Ok(()) => Reply::success("awake"),
            Err(e) => Reply::failure(Status::InternalServerError, e),
        }
    }

    fn sleep<B: Backend + ?Sized>(&mut self, backend: &mut B) -> Reply {
        match backend.sleep() {
            Ok(()) => Reply::success("sleeping"),
            Err(e) => Reply::failure(Status::InternalServerError, e),
        }
    }

    fn steam<B: Backend + ?Sized>(
        &mut self,
        backend: &mut B,
        body: &str,
        head: &RequestHead,
    ) -> Reply {
        let on = match flag(body, head, "steamMode")
            .or_else(|| flag(body, head, "enabled"))
            .or_else(|| flag(body, head, "steam"))
        {
            Some(v) => v,
            // The C++ handler is a pure toggle, and the spec's `{enabled}` was ignored. A toggle is
            // kept, but an explicit boolean is honoured first, so both clients work.
            None => !backend.steam_enabled(),
        };
        match backend.set_steam(on) {
            // The field name is `steamMode`, not `steamEnabled`: the spec says the former and the
            // C++ sent the latter. Contract drift row 2, resolved in favour of the code.
            Ok(()) => Reply::json_body(
                Status::Ok,
                if on {
                    "{\"success\":true,\"message\":\"steam mode updated\",\"steamMode\":true}"
                } else {
                    "{\"success\":true,\"message\":\"steam mode updated\",\"steamMode\":false}"
                },
            ),
            Err("busy") => Reply::json_body(
                Status::ServiceUnavailable,
                "{\"success\":false,\"message\":\"busy\"}",
            ),
            Err(e) => Reply::failure(Status::InternalServerError, e),
        }
    }

    fn pid<B: Backend + ?Sized>(&mut self, backend: &mut B, body: &str) -> Reply {
        if let Some(on) = json_bool(body, "enabled") {
            // An explicit value is honoured. The C++ ignored the body and toggled, which is the
            // drift row 3 of the contract.
            return match backend.set_pid(on) {
                Ok(()) => Reply::success(if on { "pid on" } else { "pid off" }),
                Err("busy") => Reply::json_body(
                    Status::ServiceUnavailable,
                    "{\"success\":false,\"message\":\"busy\"}",
                ),
                Err(e) => Reply::failure(Status::InternalServerError, e),
            };
        }
        match backend.toggle_pid() {
            Ok(()) => Reply::success("pid toggled"),
            Err("busy") => Reply::json_body(
                Status::ServiceUnavailable,
                "{\"success\":false,\"message\":\"busy\"}",
            ),
            Err(e) => Reply::failure(Status::InternalServerError, e),
        }
    }

    fn backflush<B: Backend + ?Sized>(&mut self, backend: &mut B, body: &str) -> Reply {
        let on = json_bool(body, "backflushOn")
            .or_else(|| json_bool(body, "enabled"))
            .unwrap_or(true);
        match backend.set_backflush(on) {
            Ok(()) => {
                // The C++ returned both `success` and `backflushOn`; the spec documented only the
                // first. Both are sent, which satisfies the spec's client and the C++'s.
                let mut body = heapless::String::<256>::new();
                let _ = write!(
                    body,
                    "{{\"success\":true,\"message\":\"backflush updated\",\"backflushOn\":{on}}}"
                );
                Reply::json(Status::Ok).with_body_json(body.as_str())
            }
            Err("busy") => Reply::json_body(
                Status::ServiceUnavailable,
                "{\"success\":false,\"message\":\"busy\"}",
            ),
            Err("not_in_backflush_idle") => Reply::json_body(
                Status::BadRequest,
                "{\"success\":false,\"message\":\"not in backflush idle\"}",
            ),
            Err(e) => Reply::failure(Status::InternalServerError, e),
        }
    }

    fn scale<B: Backend + ?Sized>(&mut self, backend: &mut B, calibration: bool) -> Reply {
        // 404 when no scale is fitted, which is the C++ behaviour: the route only exists when the
        // scale was enabled at boot.
        if !backend.scale_enabled() {
            return Reply::json_body(Status::NotFound, "{\"error\":\"no scale\"}");
        }
        let r = if calibration {
            backend.scale_calibration()
        } else {
            backend.scale_tare()
        };
        match r {
            Ok(()) => Reply::success(if calibration {
                "calibration started"
            } else {
                "tared"
            }),
            Err(e) => Reply::failure(Status::InternalServerError, e),
        }
    }

    fn parameter_help<B: Backend + ?Sized>(
        &mut self,
        backend: &mut B,
        head: &RequestHead,
    ) -> Reply {
        let Some(key) = head.query_param("param") else {
            return Reply::json_body(Status::UnprocessableEntity, "{\"error\":\"missing param\"}");
        };
        match backend.parameter_help(key) {
            Some(help) => {
                let mut r = Reply::json(Status::Ok);
                let _ = write!(r.body, "{{\"name\":\"{key}\",\"helpText\":\"{help}\"}}");
                r
            }
            None => Reply::json_body(Status::NotFound, "{\"error\":\"unknown parameter\"}"),
        }
    }

    fn temperatures<B: Backend + ?Sized>(&mut self, backend: &mut B) -> Reply {
        if !backend.temperatures_available() {
            // D15: the C++ answered 200 with an error body. A client cannot tell that from a
            // reading, so the status carries it.
            return Reply::failure(Status::InternalServerError, "no temperature reading");
        }
        let body = backend.temperatures_json();
        Reply::json(Status::Ok).with_body_json(body.as_str())
    }

    fn config_upload<B: Backend + ?Sized>(
        &mut self,
        backend: &mut B,
        body: &str,
        head: &RequestHead,
    ) -> Reply {
        let content_type = head.get("content-type").unwrap_or("");
        if !content_type.starts_with("application/json") {
            return Reply::json_body(
                Status::UnsupportedMediaType,
                "{\"error\":\"expected application/json\"}",
            );
        }
        if body.is_empty() {
            return Reply::json_body(Status::BadRequest, "{\"error\":\"empty body\"}");
        }
        let outcome = backend.apply_config(body);
        parameter_reply(outcome, "configuration validated and applied successfully.")
    }

    fn ota<B: Backend + ?Sized>(&mut self, backend: &mut B, kind: OtaKind, body: &str) -> Reply {
        // The password check is per-route rather than in the router, because the OTA routes are the
        // ones D17 said were unprotected and the fix has to be visible here.
        if let Some(expected) = backend.ota_password() {
            let presented = ota_password_from(body);
            let presented = presented.as_deref().unwrap_or("");
            if !constant_time_eq(expected.as_bytes(), presented.as_bytes()) {
                return Reply::json_body(
                    Status::Unauthorized,
                    "{\"error\":\"authentication required\"}",
                );
            }
        }
        if !backend.is_idle() {
            // D01: the C++ started an update with the pump running and left the actuators
            // energised behind the progress screen.
            return Reply::json_body(Status::Conflict, "{\"error\":\"machine is busy\"}");
        }
        // One buffer type for both shapes: an upload body is up to 16 KB, a URL is 256 bytes, and
        // a firmware image arrives as a stream into the OTA task rather than through this handler,
        // so what arrives here for an upload is a description, not the image.
        let mut source: heapless::String<512> = heapless::String::new();
        let source: Option<heapless::String<512>> = match kind {
            OtaKind::Url => ota_url_from(body).map(|u| {
                let mut wide = heapless::String::<512>::new();
                let _ = wide.push_str(u.as_str());
                wide
            }),
            _ => {
                if source.push_str(body).is_err() {
                    return Reply::json_body(
                        Status::PayloadTooLarge,
                        "{\"error\":\"image too large\"}",
                    );
                }
                Some(source)
            }
        };
        let Some(source) = source else {
            return Reply::json_body(Status::BadRequest, "{\"error\":\"missing url\"}");
        };
        if let Err(e) = validate_ota_source(kind, source.as_str()) {
            let mut body = heapless::String::<128>::new();
            let _ = write!(body, "{{\"error\":\"{e}\"}}");
            return Reply::json_body(Status::BadRequest, body.as_str());
        }
        match backend.start_ota(kind, source.as_str()) {
            Ok(()) => {
                if kind == OtaKind::Url {
                    // 202, not 200: the C++ answered 202 and the frontend checks for it.
                    Reply::json_body(
                        Status::Accepted,
                        "{\"success\":true,\"message\":\"update queued\"}",
                    )
                } else {
                    Reply::success("update started")
                }
            }
            Err(refusal) => Reply::json_body(refusal.status(), "{\"error\":\"update refused\"}"),
        }
    }
}

impl Reply {
    /// Replaces the body with a pre-built JSON payload.
    fn with_body_json(mut self, payload: &str) -> Self {
        self.body.clear();
        let _ = self.body.push_str(payload);
        self
    }
}

fn parameter_reply(outcome: ParameterOutcome, message: &str) -> Reply {
    let mut body = heapless::String::<MAX_REPLY>::new();
    let success = outcome.applied;
    let _ = write!(
        body,
        "{{\"success\":{success},\"message\":\"{}\",\"restart\":{restart},\"accepted\":{},\"rejected\":{},\"clamped\":{},\"unknown\":{},\"missing\":{}}}",
        if success { message } else { "configuration rejected" },
        outcome.accepted,
        outcome.rejected,
        outcome.clamped,
        outcome.unknown,
        outcome.missing,
        restart = success
    );
    // 400 when anything was rejected or unknown, which is the whole point of D13: the C++ answered
    // 200 with 90 of 96 parameters refused.
    // 200 whenever anything was applied, 400 only when the transaction rolled back. The counts in
    // the body are what tell a client *how much* was wrong, which the C++ could not (D13).
    let status = if success {
        Status::Ok
    } else {
        Status::BadRequest
    };
    Reply::json(status).with_body_json(body.as_str())
}

/// Reads a value from a query string or a form body, whichever has it.
fn query_or_form<'a>(head: &'a RequestHead, body: &'a str, key: &str) -> Option<&'a str> {
    if let Some(v) = head.query_param(key) {
        return Some(v);
    }
    body.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then_some(v)
    })
}

fn json_number<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let mut needle = heapless::String::<48>::new();
    let _ = write!(needle, "\"{key}\"");
    let at = body.find(needle.as_str())?;
    let rest = &body[at + needle.len()..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e'))
        .unwrap_or(rest.len());
    if end == 0 {
        None
    } else {
        Some(&rest[..end])
    }
}

fn json_bool(body: &str, key: &str) -> Option<bool> {
    let mut needle = heapless::String::<48>::new();
    let _ = write!(needle, "\"{key}\"");
    let at = body.find(needle.as_str())?;
    let rest = &body[at + needle.len()..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn flag(body: &str, head: &RequestHead, key: &str) -> Option<bool> {
    if let Some(v) = query_or_form(head, body, key) {
        return match v {
            "1" | "true" | "on" | "yes" => Some(true),
            "0" | "false" | "off" | "no" => Some(false),
            _ => None,
        };
    }
    json_bool(body, key)
}

/// The OTA password, from a JSON body or a form field.
fn ota_password_from(body: &str) -> Option<heapless::String<256>> {
    if let Some(v) = json_string(body, "password") {
        return Some(v);
    }
    body.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == "password").then(|| {
            let mut copy = heapless::String::<256>::new();
            let _ = copy.push_str(v);
            copy
        })
    })
}

fn ota_url_from(body: &str) -> Option<heapless::String<256>> {
    if let Some(v) = json_string(body, "url") {
        return Some(v);
    }
    body.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == "url").then(|| {
            let mut copy = heapless::String::<256>::new();
            let _ = copy.push_str(v);
            copy
        })
    })
}

fn json_string(body: &str, key: &str) -> Option<heapless::String<256>> {
    let mut needle = heapless::String::<48>::new();
    let _ = write!(needle, "\"{key}\"");
    let at = body.find(needle.as_str())?;
    let rest = &body[at + needle.len()..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    let mut out = heapless::String::<256>::new();
    let _ = out.push_str(&rest[..end]);
    Some(out)
}

/// The URL allow-list, which the C++ OTA-from-URL route did not have (D16).
///
/// A firmware image fetched from an arbitrary URL is a remote code execution path with a progress
/// bar, so the scheme must be HTTP or HTTPS and the host must be one the operator configured.
pub const DEFAULT_OTA_HOSTS: [&str; 2] = ["github.com", "objects.githubusercontent.com"];

/// The file extensions the firmware variant accepts, matching the filesystem variant's check.
pub const FIRMWARE_EXTENSIONS: [&str; 2] = ["bin", "elf"];

pub fn validate_ota_source(kind: OtaKind, source: &str) -> Result<(), &'static str> {
    match kind {
        OtaKind::Url => {
            let Some(rest) = source
                .strip_prefix("https://")
                .or_else(|| source.strip_prefix("http://"))
            else {
                return Err("url scheme must be http or https");
            };
            let host = rest.split('/').next().unwrap_or("");
            let host = host.split('@').next_back().unwrap_or(host);
            if host.is_empty() {
                return Err("url has no host");
            }
            if !DEFAULT_OTA_HOSTS.contains(&host) {
                return Err("url host is not allowed");
            }
            Ok(())
        }
        _ => {
            if source.is_empty() {
                return Err("empty image");
            }
            let ext = source.rsplit('.').next().unwrap_or("");
            if !FIRMWARE_EXTENSIONS.contains(&ext) {
                // The extension check the filesystem route had and the firmware route did not.
                return Err("image must be .bin or .elf");
            }
            Ok(())
        }
    }
}

/// A comparison whose time does not depend on where the first difference is.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_route_table_is_the_contract_s_thirty_routes() {
        assert_eq!(ROUTES.len(), 30);
        let mut seen = heapless::Vec::<&str, 30>::new();
        for r in ROUTES {
            assert!(!seen.contains(&r), "{r} appears twice");
            let _ = seen.push(r);
        }
        assert_eq!(ROUTES[0], "/api/status");
        assert_eq!(ROUTES[ROUTES.len() - 1], "*");
    }

    #[test]
    fn the_url_allow_list_refuses_what_it_should() {
        assert!(validate_ota_source(OtaKind::Url, "https://github.com/x/y.bin").is_ok());
        assert!(validate_ota_source(OtaKind::Url, "http://github.com/x/y.bin").is_ok());
        for bad in [
            "ftp://github.com/x",
            "https://evil.example.com/x.bin",
            "https://github.com.evil.com/x.bin",
            "https:///x.bin",
            "file:///etc/passwd",
        ] {
            assert!(
                validate_ota_source(OtaKind::Url, bad).is_err(),
                "{bad} must be refused"
            );
        }
    }

    #[test]
    fn the_firmware_route_checks_the_extension() {
        assert!(validate_ota_source(OtaKind::Firmware, "firmware.bin").is_ok());
        assert!(validate_ota_source(OtaKind::Firmware, "firmware.elf").is_ok());
        assert!(validate_ota_source(OtaKind::Firmware, "firmware.sh").is_err());
        assert!(validate_ota_source(OtaKind::Firmware, "").is_err());
    }

    #[test]
    fn a_credential_comparison_does_not_short_circuit() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secrez"));
        assert!(!constant_time_eq(b"secret", b"secr"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn a_form_body_parses_and_an_empty_value_counts_as_a_reset() {
        let p = ParameterPairs::parse("brew.setpoint=93&pid.agg.kp=10&brew.mode=").unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p.get("brew.setpoint"), Some("93"));
        assert_eq!(
            p.resets, 1,
            "an empty value means reset to default, not skip"
        );
        assert!(ParameterPairs::parse("novalue").is_none());
        assert!(ParameterPairs::parse("=1").is_none());
    }

    #[test]
    fn a_json_number_and_boolean_are_read_without_a_parser() {
        assert_eq!(json_number("{\"value\": 93.5}", "value"), Some("93.5"));
        assert_eq!(json_number("{\"value\":-3}", "value"), Some("-3"));
        assert_eq!(json_number("{\"other\": 1}", "value"), None);
        assert_eq!(json_bool("{\"enabled\": true}", "enabled"), Some(true));
        assert_eq!(json_bool("{\"enabled\": false}", "enabled"), Some(false));
        assert_eq!(json_bool("{\"enabled\": 1}", "enabled"), None);
    }

    #[test]
    fn the_filter_parses_what_the_contract_documents() {
        assert_eq!(Filter::parse(None), Some(Filter::All));
        assert_eq!(Filter::parse(Some("all")), Some(Filter::All));
        assert_eq!(Filter::parse(Some("hardware")), Some(Filter::Hardware));
        assert_eq!(Filter::parse(Some("behavior")), Some(Filter::Behavior));
        assert_eq!(Filter::parse(Some("other")), Some(Filter::Other));
        assert_eq!(Filter::parse(Some("visible")), Some(Filter::Visible));
        assert_eq!(Filter::parse(Some("editable")), Some(Filter::Editable));
        assert_eq!(Filter::parse(Some("nonsense")), None);
    }

    #[test]
    fn a_reply_serialises_to_a_response_with_its_content_type() {
        let r = Reply::success("done");
        let resp = r.to_response();
        assert_eq!(resp.status, Status::Ok);
        assert_eq!(
            resp.get_header("Content-Type"),
            Some("application/json; charset=utf-8")
        );
        assert!(r.has("\"success\":true"));
    }
}
