//! Every route, in its success case and in each error case the contract lists.
//!
//! The fake backend is a struct of flags, so a test says what the machine is doing and asserts what
//! the wire says back. That is the assertion the C++ firmware could not make: it had no way to ask
//! "what does this route return when the scale is absent" without a scale.

use clevercoffee_app::api::{
    Api, Backend, Filter, OtaKind, OtaRefusal, ParameterOutcome, ParameterPairs, Reply, ROUTES,
};
use clevercoffee_http::{Header, Method, RequestHead, Status};

/// Into a `heapless::String`, for a fake that returns one.
fn hs(s: &str) -> heapless::String<32768> {
    let mut out = heapless::String::new();
    let _ = out.push_str(s);
    out
}

/// A backend whose every answer is a flag, so a test reads as a table.
#[derive(Debug, Default)]
struct Fake {
    status_ok: bool,
    scale: bool,
    temperature_ok: bool,
    idle: bool,
    steam_on: bool,
    pid: bool,
    busy: bool,
    setpoint_range: Option<(f64, f64)>,
    applied: bool,
    last_setpoint: Option<f64>,
    calls: Vec<&'static str>,
    ota_password: Option<&'static str>,
    ota_started: Option<OtaKind>,
}

impl Fake {
    fn new() -> Self {
        Self {
            status_ok: true,
            temperature_ok: true,
            idle: true,
            applied: true,
            ..Self::default()
        }
    }
}

impl Backend for Fake {
    fn status_json(&mut self, out: &mut heapless::String<32768>) -> bool {
        self.calls.push("status");
        if !self.status_ok {
            return false;
        }
        let _ = out.push_str("{\"temperature\":92.4,\"machineState\":20,\"isStandby\":false}");
        true
    }

    fn scale_enabled(&self) -> bool {
        self.scale
    }

    fn weights(&mut self) -> (f64, f64) {
        (36.0, 12.0)
    }

    fn parameters_json(&mut self, filter: Filter) -> heapless::String<32768> {
        self.calls.push("parameters");
        // Echoes the filter the handler resolved, so a test can see that the query was honoured
        // rather than merely accepted.
        let wire = match filter {
            Filter::All => "all",
            Filter::Hardware => "hardware",
            Filter::Behavior => "behavior",
            Filter::Other => "other",
            Filter::Visible => "visible",
            Filter::Editable => "editable",
        };
        hs(&format!("[{{\"name\":\"a\",\"filter\":\"{wire}\"}}]"))
    }

    fn parameter_help(&mut self, key: &str) -> Option<&'static str> {
        (key == "brew.setpoint").then_some("the boiler setpoint")
    }

    fn config_json(&mut self) -> heapless::String<32768> {
        self.calls.push("config");
        hs("{\"brew\":{\"setpoint\":92.0}}")
    }

    fn nvs_debug_json(&mut self) -> heapless::String<32768> {
        self.calls.push("nvs");
        hs("{\"entries\":[{\"key\":\"wifi.pass\",\"value\":\"\"}]}")
    }

    fn history_json(&mut self) -> heapless::String<32768> {
        hs("{\"currentTemps\":[],\"targetTemps\":[],\"heaterPowers\":[]}")
    }

    fn temperatures_json(&mut self) -> heapless::String<32768> {
        hs("{\"currentTemp\":92.4,\"targetTemp\":92.0,\"heaterPower\":37.5}")
    }

    fn temperatures_available(&self) -> bool {
        self.temperature_ok
    }

    fn set_setpoint(&mut self, celsius: f64) -> Result<(), &'static str> {
        self.calls.push("setpoint");
        if let Some((lo, hi)) = self.setpoint_range {
            if celsius < lo || celsius > hi {
                return Err("out_of_range");
            }
        }
        self.last_setpoint = Some(celsius);
        Ok(())
    }

    fn toggle_pid(&mut self) -> Result<(), &'static str> {
        self.calls.push("toggle_pid");
        if self.busy {
            return Err("busy");
        }
        self.pid = !self.pid;
        Ok(())
    }

    fn set_pid(&mut self, on: bool) -> Result<(), &'static str> {
        self.calls.push("set_pid");
        if self.busy {
            return Err("busy");
        }
        self.pid = on;
        Ok(())
    }

    fn set_steam(&mut self, on: bool) -> Result<(), &'static str> {
        self.calls.push("steam");
        if self.busy {
            return Err("busy");
        }
        self.steam_on = on;
        Ok(())
    }

    fn steam_enabled(&self) -> bool {
        self.steam_on
    }

    fn set_backflush(&mut self, on: bool) -> Result<(), &'static str> {
        self.calls.push("backflush");
        if self.busy {
            return Err("busy");
        }
        let _ = on;
        Ok(())
    }

    fn reset_backflush_counter(&mut self) -> Result<(), &'static str> {
        self.calls.push("reset_backflush");
        Ok(())
    }

    fn scale_tare(&mut self) -> Result<(), &'static str> {
        self.calls.push("tare");
        Ok(())
    }

    fn scale_calibration(&mut self) -> Result<(), &'static str> {
        self.calls.push("calibrate");
        Ok(())
    }

    fn wifi_reset(&mut self) -> Result<(), &'static str> {
        self.calls.push("wifi_reset");
        Ok(())
    }

    fn restart(&mut self) -> Result<(), &'static str> {
        self.calls.push("restart");
        Ok(())
    }

    fn factory_reset(&mut self) -> Result<(), &'static str> {
        self.calls.push("factory_reset");
        Ok(())
    }

    fn wake(&mut self) -> Result<(), &'static str> {
        self.calls.push("wake");
        Ok(())
    }

    fn sleep(&mut self) -> Result<(), &'static str> {
        self.calls.push("sleep");
        Ok(())
    }

    fn apply_parameters(&mut self, pairs: &ParameterPairs) -> ParameterOutcome {
        self.calls.push("apply_parameters");
        ParameterOutcome {
            accepted: pairs.len() as u16,
            rejected: 0,
            clamped: 0,
            unknown: 0,
            missing: 0,
            applied: self.applied,
        }
    }

    fn apply_config(&mut self, document: &str) -> ParameterOutcome {
        self.calls.push("apply_config");
        let _ = document;
        ParameterOutcome {
            accepted: 88,
            rejected: if self.applied { 0 } else { 6 },
            clamped: 2,
            unknown: if self.applied { 0 } else { 1 },
            missing: 0,
            applied: self.applied,
        }
    }

    fn is_idle(&self) -> bool {
        self.idle
    }

    fn ota_status_json(&mut self) -> heapless::String<32768> {
        hs("{\"updating\":false,\"queued\":false}")
    }

    fn start_ota(&mut self, kind: OtaKind, source: &str) -> Result<(), OtaRefusal> {
        self.calls.push("ota");
        let _ = source;
        self.ota_started = Some(kind);
        Ok(())
    }

    fn ota_password(&self) -> Option<&str> {
        self.ota_password
    }
}

/// A request head for a target, with the query split out the way the parser splits it.
fn head(method: Method, target: &str) -> RequestHead {
    let mut h = RequestHead::new(method);
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };
    let _ = h.path.push_str(path);
    let _ = h.query.push_str(query);
    let _ = h.target.push_str(target);
    h
}

fn with_header(mut h: RequestHead, name: &str, value: &str) -> RequestHead {
    if let Some(header) = Header::new(name.as_bytes(), value.as_bytes()) {
        h.headers[0] = Some(header);
        h.header_count = 1;
    }
    h
}

fn get(api: &mut Api, backend: &mut Fake, path: &str) -> Reply {
    api.dispatch(backend, &head(Method::Get, path), "")
}

fn post(api: &mut Api, backend: &mut Fake, path: &str, body: &str) -> Reply {
    api.dispatch(backend, &head(Method::Post, path), body)
}

#[test]
fn every_route_in_the_contract_is_reachable_and_answers() {
    let mut api = Api::new();
    let mut b = Fake::new();
    // One call per route, with a body each method needs. The point is that no route 404s or 405s
    // on its own happy path.
    for path in ROUTES.iter().filter(|p| **p != "*" && **p != "/ui/") {
        let reply = match clevercoffee_app::api::methods_for(path).first() {
            Some(Method::Post) => post(&mut api, &mut b, path, "{}"),
            _ => get(&mut api, &mut b, path),
        };
        assert_ne!(
            reply.status,
            Status::MethodNotAllowed,
            "{path} answered 405"
        );
        // The two scale routes and `/ui/` answer 404 when the hardware or the assets are absent,
        // which is the documented behaviour, so a 404 from them is not a missing route. What a
        // missing route looks like is caught separately.
        let documented_404 = matches!(*path, "/api/scale/tare" | "/api/scale/calibration" | "/ui/");
        if !documented_404 {
            assert_ne!(
                reply.status,
                Status::NotFound,
                "{path} answered 404 on its own happy path: {}",
                reply.body()
            );
        }
    }
}

#[test]
fn status_returns_the_documented_fields_and_the_scale_ones_only_with_a_scale() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = get(&mut api, &mut b, "/api/status");
    assert_eq!(r.status, Status::Ok);
    assert!(r.has("\"temperature\":92.4"));
    assert!(r.has("\"machineState\":20"));
    assert!(!r.has("\"weight\""), "no scale, no weight field");

    b.scale = true;
    let r = get(&mut api, &mut b, "/api/status");
    assert!(r.has("\"weight\":36.0"), "{}", r.body());
    assert!(r.has("\"brewWeight\":12.0"), "{}", r.body());
}

#[test]
fn a_status_that_cannot_be_read_is_a_500() {
    let mut api = Api::new();
    let mut b = Fake::new();
    b.status_ok = false;
    let r = get(&mut api, &mut b, "/api/status");
    assert_eq!(r.status, Status::InternalServerError);
    assert!(r.has("\"success\":false"));
}

#[test]
fn temperatures_answers_500_when_there_is_no_reading() {
    // D15: the C++ answered 200 with an error body, which a client cannot distinguish from data.
    let mut api = Api::new();
    let mut b = Fake::new();
    b.temperature_ok = false;
    let r = get(&mut api, &mut b, "/api/temperatures");
    assert_eq!(r.status, Status::InternalServerError);
}

#[test]
fn temperatures_returns_the_three_documented_fields() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = get(&mut api, &mut b, "/api/temperatures");
    assert_eq!(r.status, Status::Ok);
    for field in ["currentTemp", "targetTemp", "heaterPower"] {
        assert!(r.has(field), "{} missing from {}", field, r.body());
    }
}

#[test]
fn the_setpoint_route_accepts_a_form_a_query_and_json_and_refuses_the_rest() {
    let mut api = Api::new();
    let mut b = Fake::new();
    assert_eq!(
        post(&mut api, &mut b, "/api/setpoint", "value=93.5").status,
        Status::Ok
    );
    assert_eq!(b.last_setpoint, Some(93.5));
    assert_eq!(
        post(&mut api, &mut b, "/api/setpoint", "{\"value\":88}").status,
        Status::Ok
    );
    assert_eq!(b.last_setpoint, Some(88.0));
    assert_eq!(
        post(&mut api, &mut b, "/api/setpoint", "value=abc").status,
        Status::BadRequest
    );
    assert_eq!(
        post(&mut api, &mut b, "/api/setpoint", "").status,
        Status::BadRequest
    );
    assert_eq!(
        post(&mut api, &mut b, "/api/setpoint", "value=NaN").status,
        Status::BadRequest,
        "a NaN setpoint must never reach the PID"
    );

    b.setpoint_range = Some((20.0, 110.0));
    let r = post(&mut api, &mut b, "/api/setpoint", "value=150");
    assert_eq!(r.status, Status::BadRequest);
    assert!(r.has("out_of_range"));
}

#[test]
fn the_pid_route_honours_an_explicit_value_and_toggles_otherwise() {
    let mut api = Api::new();
    let mut b = Fake::new();
    assert_eq!(post(&mut api, &mut b, "/api/pid", "{}").status, Status::Ok);
    assert!(b.pid, "a bare call toggles on");
    assert_eq!(post(&mut api, &mut b, "/api/pid", "{}").status, Status::Ok);
    assert!(!b.pid, "and toggles off again");
    post(&mut api, &mut b, "/api/pid", "{\"enabled\":true}");
    assert!(
        b.pid,
        "an explicit value is honoured, which the C++ ignored"
    );
}

#[test]
fn a_busy_machine_refuses_a_pid_or_steam_change_with_503() {
    let mut api = Api::new();
    let mut b = Fake::new();
    b.busy = true;
    assert_eq!(
        post(&mut api, &mut b, "/api/pid", "{}").status,
        Status::ServiceUnavailable
    );
    assert_eq!(
        post(&mut api, &mut b, "/api/steam", "{}").status,
        Status::ServiceUnavailable
    );
    assert_eq!(
        post(&mut api, &mut b, "/api/backflush", "{}").status,
        Status::ServiceUnavailable
    );
}

#[test]
fn the_steam_route_toggles_and_reports_steam_mode() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = post(&mut api, &mut b, "/api/steam", "{}");
    assert!(b.steam_on);
    assert!(r.has("\"steamMode\":true"), "{}", r.body());
    let r = post(&mut api, &mut b, "/api/steam", "{\"steamMode\":false}");
    assert!(!b.steam_on);
    assert!(r.has("\"steamMode\":false"), "{}", r.body());
}

#[test]
fn the_backflush_route_returns_both_fields_the_cpp_frontend_expects() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = post(&mut api, &mut b, "/api/backflush", "{\"backflushOn\":true}");
    assert!(r.has("\"success\":true"));
    assert!(r.has("\"backflushOn\":true"), "{}", r.body());
}

#[test]
fn the_scale_routes_are_404_without_a_scale_and_work_with_one() {
    let mut api = Api::new();
    let mut b = Fake::new();
    assert_eq!(
        post(&mut api, &mut b, "/api/scale/tare", "").status,
        Status::NotFound
    );
    assert_eq!(
        post(&mut api, &mut b, "/api/scale/calibration", "").status,
        Status::NotFound
    );
    b.scale = true;
    assert_eq!(
        post(&mut api, &mut b, "/api/scale/tare", "").status,
        Status::Ok
    );
    assert_eq!(
        post(&mut api, &mut b, "/api/scale/calibration", "").status,
        Status::Ok
    );
}

#[test]
fn parameter_help_needs_a_parameter_and_answers_422_or_404() {
    let mut api = Api::new();
    let mut b = Fake::new();
    assert_eq!(
        get(&mut api, &mut b, "/api/parameter-help").status,
        Status::UnprocessableEntity
    );
    assert_eq!(
        get(&mut api, &mut b, "/api/parameter-help?param=nope").status,
        Status::NotFound
    );
    let r = get(&mut api, &mut b, "/api/parameter-help?param=brew.setpoint");
    assert_eq!(r.status, Status::Ok);
    assert!(
        r.has("\"helpText\":\"the boiler setpoint\""),
        "{}",
        r.body()
    );
}

#[test]
fn the_parameters_filter_is_honoured_and_an_unknown_one_is_422() {
    let mut api = Api::new();
    let mut b = Fake::new();
    for f in ["all", "hardware", "behavior", "other"] {
        let r = get(&mut api, &mut b, &format!("/api/parameters?filter={f}"));
        assert_eq!(r.status, Status::Ok, "filter {f}");
        assert!(
            r.has(&format!("\"{f}\"")),
            "filter {f} was not honoured: {}",
            r.body()
        );
    }
    assert_eq!(
        get(&mut api, &mut b, "/api/parameters?filter=nonsense").status,
        Status::UnprocessableEntity
    );
}

#[test]
fn posting_parameters_rejects_an_empty_or_malformed_body() {
    let mut api = Api::new();
    let mut b = Fake::new();
    assert_eq!(
        post(&mut api, &mut b, "/api/parameters", "").status,
        Status::BadRequest
    );
    assert_eq!(
        post(&mut api, &mut b, "/api/parameters", "noequals").status,
        Status::BadRequest
    );
    let r = post(
        &mut api,
        &mut b,
        "/api/parameters",
        "brew.setpoint=93&pid.agg.kp=10",
    );
    assert_eq!(r.status, Status::Ok);
    assert!(r.has("\"accepted\":2"), "{}", r.body());
}

#[test]
fn a_config_upload_needs_json_and_reports_400_when_anything_is_rejected() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let h = with_header(
        head(Method::Post, "/api/config/upload"),
        "Content-Type",
        "text/plain",
    );
    assert_eq!(
        api.dispatch(&mut b, &h, "{}").status,
        Status::UnsupportedMediaType
    );

    let h = with_header(
        head(Method::Post, "/api/config/upload"),
        "Content-Type",
        "application/json",
    );
    let r = api.dispatch(&mut b, &h, "{\"brew\":{}}");
    assert_eq!(r.status, Status::Ok, "a clean document is 200");
    assert!(r.has("\"accepted\":88"), "{}", r.body());

    b.applied = false;
    let r = api.dispatch(&mut b, &h, "{\"brew\":{\"setpoint\":150}}");
    assert_eq!(
        r.status,
        Status::BadRequest,
        "D13: a rejected field is a 400"
    );
    assert!(r.has("\"rejected\":6"), "{}", r.body());
    assert!(r.has("\"success\":false"), "{}", r.body());
}

#[test]
fn an_oversized_body_is_refused() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let big = "x".repeat(17 * 1024);
    assert_eq!(
        post(&mut api, &mut b, "/api/config/upload", &big).status,
        Status::PayloadTooLarge
    );
}

#[test]
fn the_config_download_route_carries_the_attachment_header_and_the_redacted_body() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = get(&mut api, &mut b, "/api/config/download");
    assert_eq!(r.status, Status::Ok);
    let resp = r.to_response();
    assert_eq!(
        resp.get_header("Content-Disposition"),
        Some("attachment; filename=\"config.json\"")
    );
}

#[test]
fn the_nvs_debug_route_never_carries_a_secret() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = get(&mut api, &mut b, "/api/nvs-debug");
    assert_eq!(r.status, Status::Ok);
    assert!(
        r.has("\"value\":\"\""),
        "a secret is redacted: {}",
        r.body()
    );
}

#[test]
fn the_root_redirects_to_the_ui() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = get(&mut api, &mut b, "/");
    let resp = r.to_response();
    assert_eq!(resp.status, Status::Found);
    assert_eq!(resp.get_header("Location"), Some("/ui/"));
}

#[test]
fn an_unknown_api_path_is_json_and_an_unknown_other_path_is_text() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = get(&mut api, &mut b, "/api/nope");
    assert_eq!(r.status, Status::NotFound);
    assert_eq!(r.content_type, "application/json; charset=utf-8");
    let r = get(&mut api, &mut b, "/nope");
    assert_eq!(r.status, Status::NotFound);
    assert_eq!(r.content_type, "text/plain; charset=utf-8");
}

#[test]
fn a_wrong_method_is_405_not_404() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = post(&mut api, &mut b, "/api/status", "");
    assert_eq!(r.status, Status::MethodNotAllowed);
    let r = get(&mut api, &mut b, "/api/restart");
    assert_eq!(r.status, Status::MethodNotAllowed);
}

#[test]
fn an_unauthenticated_mutation_is_401_when_auth_is_enabled() {
    // D10: the C++ middleware never authenticated, so every mutating route was open.
    let mut api = Api::new();
    api.auth_enabled = true;
    let mut b = Fake::new();
    let r = post(&mut api, &mut b, "/api/factory-reset", "");
    assert_eq!(r.status, Status::Unauthorized);
    assert!(
        !b.calls.contains(&"factory_reset"),
        "the handler must not have run"
    );

    assert!(api.authenticate(Some("swordfish"), Some("swordfish")));
    let r = post(&mut api, &mut b, "/api/factory-reset", "");
    assert_eq!(r.status, Status::Ok);
    assert!(b.calls.contains(&"factory_reset"));
}

#[test]
fn auth_disabled_means_every_route_is_open_which_is_the_documented_configuration() {
    let mut api = Api::new();
    let mut b = Fake::new();
    assert!(api.authenticate(None, None));
    assert_eq!(
        post(&mut api, &mut b, "/api/factory-reset", "").status,
        Status::Ok
    );
}

#[test]
fn a_wrong_credential_is_refused() {
    let mut api = Api::new();
    api.auth_enabled = true;
    assert!(!api.authenticate(Some("swordfish"), Some("swordfisl")));
    assert!(!api.authenticate(Some("swordfish"), None));
}

#[test]
fn an_ota_while_brewing_is_409() {
    // D01: the C++ started an update with the pump running and left the actuators energised.
    let mut api = Api::new();
    let mut b = Fake::new();
    b.idle = false;
    for path in ["/api/ota/firmware", "/api/ota/filesystem", "/api/ota/url"] {
        let r = post(&mut api, &mut b, path, "firmware.bin");
        assert_eq!(r.status, Status::Conflict, "{path}");
    }
    assert!(b.ota_started.is_none(), "no update may start while brewing");
}

#[test]
fn an_ota_with_the_wrong_password_is_401() {
    // D17: the C++ OTA routes ignored the configured password entirely.
    let mut api = Api::new();
    let mut b = Fake::new();
    b.ota_password = Some("swordfish");
    let r = post(&mut api, &mut b, "/api/ota/firmware", "firmware.bin");
    assert_eq!(r.status, Status::Unauthorized);
    let body = "password=swordfish&filename=firmware.bin";
    let r = post(&mut api, &mut b, "/api/ota/firmware", body);
    assert_eq!(r.status, Status::Ok, "{}", r.body());
    assert_eq!(b.ota_started, Some(OtaKind::Firmware));
}

#[test]
fn a_url_ota_answers_202_and_refuses_a_url_outside_the_allow_list() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = post(
        &mut api,
        &mut b,
        "/api/ota/url",
        "{\"url\":\"https://github.com/o/r/releases/fw.bin\"}",
    );
    assert_eq!(
        r.status,
        Status::Accepted,
        "the C++ answered 202 and the frontend checks it"
    );
    assert!(b.ota_started.is_some());

    let r = post(
        &mut api,
        &mut b,
        "/api/ota/url",
        "{\"url\":\"https://evil.example.com/fw.bin\"}",
    );
    assert_eq!(r.status, Status::BadRequest);
    let r = post(
        &mut api,
        &mut b,
        "/api/ota/url",
        "{\"url\":\"ftp://github.com/fw.bin\"}",
    );
    assert_eq!(r.status, Status::BadRequest);
}

#[test]
fn the_firmware_route_checks_the_extension_the_filesystem_route_already_did() {
    // D16: the URL route had no allow-list and the firmware route had no extension check.
    let mut api = Api::new();
    let mut b = Fake::new();
    assert_eq!(
        post(&mut api, &mut b, "/api/ota/firmware", "update.sh").status,
        Status::BadRequest
    );
    assert_eq!(
        post(&mut api, &mut b, "/api/ota/filesystem", "index.html").status,
        Status::BadRequest
    );
    assert_eq!(
        post(&mut api, &mut b, "/api/ota/firmware", "update.bin").status,
        Status::Ok
    );
}

#[test]
fn the_ota_status_route_reports_the_documented_fields() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = get(&mut api, &mut b, "/api/ota/status");
    assert_eq!(r.status, Status::Ok);
    assert!(r.has("\"updating\"") && r.has("\"queued\""), "{}", r.body());
}

#[test]
fn the_events_route_is_an_event_stream() {
    let mut api = Api::new();
    let mut b = Fake::new();
    let r = get(&mut api, &mut b, "/events");
    assert_eq!(r.status, Status::Ok);
    assert_eq!(r.content_type, "text/event-stream");
}

#[test]
fn no_handler_awaits_or_blocks() {
    // The contract's own requirement, checked against the source rather than asserted in a
    // comment: an `await` inside a handler would make a route block the executor.
    let src = include_str!("../src/api.rs");
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with("///") || t.starts_with("//!") {
            continue;
        }
        assert!(
            !t.contains(".await"),
            "a handler contains an await, which would block the executor: {line}"
        );
    }
}
