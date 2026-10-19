//! Every `*_json` renderer the REST surface emits, plus [`mime_for`].
//!
//! These are the functions that were reachable only by flashing a board: pure
//! `String` builders over a [`Telemetry`] and a [`cc_config::Config`], held in a
//! file that named `esp_idf_svc`. Two of them read a heap gauge and now take it
//! as a parameter — see the crate root for why, and for the note that the bytes
//! they emit are unchanged.

use alloc::format;
use alloc::string::{String, ToString};
use core::fmt::Write as _;

use cc_config::schema::{ParamValue, SCHEMA};
use cc_config::Config;

use crate::telemetry::Telemetry;

// ============================================================== /api/status

/// `/api/status` — one field per key a client reads.
///
/// `weight` and `brewWeight` are `null` when there is no reading, rather than being
/// omitted when the scale is disabled: a `null` is a smaller change for a client
/// than a missing key, and a smaller lie than a fabricated `0`.
///
/// `heap_free` is `esp_get_free_heap_size()`, read by the caller: it is an FFI call
/// and this function is a pure builder.
///
/// # `steamMode` is the latched flag, and `brewing` is a different fact
///
/// `steamMode` means "steam mode is engaged", and [`Telemetry::steam_mode`] is
/// **latched**: `Machine::steam_mode` is set on entering `SteamRunning` and cleared
/// on leaving it and on entering `Standby` (`cc-machine/src/states.rs`). This route
/// emits it under that name so it agrees with the `/api/steam` toggle, which answers
/// from the same fact — a status poll that disagreed with the toggle that set it
/// would be a worse bug than the one this replaces.
///
/// `brewing` is **not the same value**. It is derived on every render
/// (`state.is_brew_state() && state != BrewFinished`), and the two were once emitted
/// under one name, which made a brew-state flag answer to a steam-mode name. Both
/// keys are emitted now, and the addition is recorded as §24 of
/// `docs/history/divergences.md`.
#[must_use]
pub fn status_json(t: &Telemetry, heap_free: u32) -> String {
    // `write!` into the `String` rather than `push_str(&format!(..))`: this runs on
    // the httpd task for every poll from an open browser tab, and the `format!` form
    // allocates a temporary `String` per field.
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
    let _ = write!(out, ",\"heapFree\":{heap_free}}}");
    out
}

fn optional_number(name: &str, value: Option<f64>) -> String {
    match value {
        Some(v) => format!(",\"{name}\":{v:.2}"),
        None => format!(",\"{name}\":null"),
    }
}

/// `/api/temperatures` — three keys, the same payload as the `new_temps` SSE
/// event, because the UI reads one shape from two places.
#[must_use]
pub fn temperatures_json(t: &Telemetry) -> String {
    format!(
        "{{\"currentTemp\":{:.2},\"targetTemp\":{:.2},\"heaterPower\":{:.2}}}",
        t.temperature_c, t.setpoint_c, t.heater_power_pct
    )
}

/// The `weight` SSE event's payload.
#[must_use]
pub fn weight_json(t: &Telemetry) -> String {
    match (t.weight_g, t.brew_weight_g) {
        (Some(w), Some(b)) => format!("{{\"weight\":{w:.0},\"brewWeight\":{b:.0}}}"),
        _ => error_body("Scale data unavailable"),
    }
}

/// `GET /api/health` — a body, where an empty `200` would not do.
///
/// An empty `200` cannot distinguish "the machine is alive" from "the handler is
/// registered but the control task has not published yet", and that distinction is
/// the whole question a health check is asked.
#[must_use]
pub fn health_json(t: &Telemetry) -> String {
    format!(
        "{{\"status\":\"ok\",\"uptime\":{},\"machineState\":{}}}",
        t.uptime_ms, t.machine_state
    )
}

// ============================================================== /api/nvs-debug

/// `GET /api/nvs-debug` — counts and heap figures, and **no parameter values**.
///
/// The configuration's four credentials live in the same blob, and this endpoint is
/// reachable without authentication, so a payload that listed parameters would be a
/// credential disclosure with no local symptom.
///
/// `free_heap` and `min_free_heap` are the two `esp_get_*_free_heap_size()`
/// readings, passed in by the caller for the same reason [`status_json`] takes its
/// own.
#[must_use]
pub fn nvs_debug_json(
    describe: &str,
    parameter_count: usize,
    free_heap: u32,
    min_free: u32,
) -> String {
    let (version, bytes) = parse_describe(describe);
    format!(
        "{{\"message\":\"NVS debugging - parameter details available\",\
         \"parameters_count\":{parameter_count},\
         \"parameters\":[],\
         \"metadata\":{{\"total_parameters\":{parameter_count},\
         \"nvs_namespace\":\"cc\",\"blob_schema_version\":{version},\
         \"blob_bytes\":{bytes},\"free_heap\":{free_heap},\
         \"min_free_heap\":{min_free}}}}}",
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

// ============================================== bodies with no telemetry at all

/// The JSON error body: `{"error":"…"}`.
///
/// `message` is interpolated without escaping, so it must not carry a `"` or a
/// backslash.
#[must_use]
pub fn error_body(message: &str) -> String {
    format!("{{\"error\":\"{message}\"}}")
}

/// `POST /api/config/upload` — `{"success":…,"message":…,"restart":…}`.
///
/// `restart` is `success`, i.e. **the device does not reboot itself**. The flag
/// tells the operator's browser to call `POST /api/restart` two seconds later
/// (`ui/packages/frontend/src/pages/SystemPage.tsx`). Rebooting from inside the
/// handler would tear the socket down under the response the client is still
/// reading.
///
/// No `Connection: close` is set. The hazard that header guards against is already
/// handled one layer down: ESP-IDF's `httpd_req_delete` "finish\[es] off reading
/// any pending/leftover data", draining and discarding whatever body a handler left
/// unread. So the oversized upload — the one path that deliberately stops reading —
/// cannot have its tail parsed as the next request on the socket. Setting the header
/// through `httpd_resp_set_hdr` would only append a second `Connection` header to a
/// response ESP-IDF manages itself.
#[must_use]
pub fn upload_response(success: bool, message: &str) -> String {
    format!("{{\"success\":{success},\"message\":\"{message}\",\"restart\":{success}}}")
}

// ============================================= endpoints not in this build

/// The answer a route that this build does not implement gives.
///
/// Not a stub for its own sake: each of these is a real route, a real tab in the
/// React UI, and a real support question. A route that `404`s is indistinguishable
/// from a firmware that lost the feature, whereas a route that answers "not in this
/// build, here is what it would do" is a diagnosable answer.
#[must_use]
pub fn unavailable_json(feature: &str, task: &str) -> String {
    format!("{{\"error\":\"{feature} is not available in this build\",\"reason\":\"{task}\"}}")
}

// ======================================================== /api/parameters body

/// The `/api/parameters` body: an array of every registered parameter.
///
/// # Six fields per parameter, and why not ten
///
/// This emits `name`, `type`, `value`, `default`, `min`, `max`. Four fields a
/// configuration editor would like are missing: `label`, `section`, `order` and
/// `helpText`.
///
/// `label`, `section` and `order` are missing because there is no data for them.
/// They are per-parameter presentation literals that the schema never recorded,
/// and inventing them would put text in front of an operator that nothing in this
/// repository asserts. That is a `cc-config` data gap, recorded here rather than
/// papered over. The UI's editor works without them — it labels by `name`.
///
/// `helpText` is a fourth case and it is **not** a data gap:
/// [`cc_config::schema::ParamSpec::help`] carries every string, and
/// `GET /api/parameter-help` serves them — see [`crate::help`]. This body omits the
/// field for **size**: the strings are ~9 KB, and this body is already a `String`
/// of measured tens of KB, built in one allocation on a part with 320 KB of RAM.
/// Adding 50% to a body whose consumer renders without it is the wrong trade; a
/// client that wants the text asks `/api/parameter-help` for it, which is what the
/// UI does.
///
/// # The value is the stored one, re-rendered by the control task
///
/// `value` is what the machine is configured with; `default` is the compiled-in
/// default, reported separately. A firmware that reported the default in both
/// places would show a saved-and-reloaded operator's settings as if they had been
/// lost.
///
/// Both callers render the same document: the control task calls this once a second
/// and again immediately after a parameter write, publishing the result for
/// `GET /api/parameters`, and the GET handler calls it only as a fallback before the
/// first publish. So `value` is live to within one `PARAMETERS_PUBLISH_MS`, and a
/// save is reflected before its `200` is written.
#[must_use]
pub fn parameters_json(config: &Config) -> String {
    // Sized so the buffer is not reallocated mid-build: a reallocation here is a
    // second copy of a 15 KB buffer, on 320 KB of RAM.
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
/// a `"94.5"` where the editor expects a number makes it refuse the input.
fn live_value_json(value: cc_config::LiveValue<'_>) -> String {
    match value {
        cc_config::LiveValue::Bool(b) => String::from(if b { "true" } else { "false" }),
        cc_config::LiveValue::Int(i) => i.to_string(),
        cc_config::LiveValue::Float(f) => f.to_string(),
        cc_config::LiveValue::Text(t) => format!("\"{t}\""),
        // An enum is its integer discriminant on the wire, the same as
        // `ParamValue::Enum`.
        cc_config::LiveValue::Enum(i) => i.to_string(),
    }
}

fn opt_number(value: Option<f64>) -> String {
    value.map_or_else(|| String::from("null"), |v| format!("{v}"))
}

/// A schema default as JSON.
///
/// A `Text` default is quoted and a `Bool` is not: the editor reads these into
/// typed inputs, and getting the distinction wrong makes a number input receive
/// `"94.5"` and refuse it.
fn param_value_json(value: ParamValue<'_>) -> String {
    match value {
        ParamValue::Bool(b) => String::from(if b { "true" } else { "false" }),
        ParamValue::Int(i) => i.to_string(),
        ParamValue::Float(f) => f.to_string(),
        ParamValue::Text(t) => format!("\"{t}\""),
        ParamValue::Enum(i) => i.to_string(),
    }
}

// ============================================================== /ui assets

/// The `Content-Type` for an embedded asset, from its path.
///
/// **This function is the difference between a working UI and a blank page.** A
/// browser refuses to execute a script whose `Content-Type` is not a JavaScript
/// media type, and it refuses a stylesheet that is not CSS — with a `200` in the
/// network tab either way. `application/javascript` is used rather than the newer
/// `text/javascript` because it is in every browser's accept list.
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

#[cfg(test)]
mod tests;
