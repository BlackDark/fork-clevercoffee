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

/// `/api/status` — the C++'s `WebServerManager.cpp:327-372`.
///
/// Field for field. The C++ adds `weight`/`brewWeight` only when the scale is
/// enabled; here they are `null` when there is no reading, which is a smaller
/// change than gating them and a smaller one than reporting a fabricated 0 g.
///
/// `heap_free` is `esp_get_free_heap_size()`, read by the caller: it is an FFI
/// call and this function is a pure builder. The bytes are the same ones the
/// handler emitted before the reading was passed in.
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
pub fn status_json(t: &Telemetry, heap_free: u32) -> String {
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
    let _ = write!(out, ",\"heapFree\":{heap_free}}}");
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

// ============================================================== /api/nvs-debug

/// `GET /api/nvs-debug` — `WebServerManager.cpp:648-678`.
///
/// The C++'s `metadata` block, with the C++'s key names. **No parameter
/// values**: the C++ reported counts and heap figures only, and this keeps that.
/// The configuration's four credentials are in the same blob, and this endpoint
/// is reachable without authentication (`system.auth` is not enforced by the
/// C++'s REST API at all), so a payload that listed parameters would be a
/// credential disclosure with no local symptom.
///
/// `free_heap` and `min_free_heap` are the two `esp_get_*_free_heap_size()`
/// readings, passed in by the caller for the same reason
/// [`status_json`] takes its own.
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
/// down: ESP-IDF's `httpd_req_delete` "finish\[es] off reading any
/// pending/leftover data", draining and discarding whatever body a handler left
/// unread (`httpd_parse.c:841-855`). So the oversized upload — the one path that
/// deliberately stops reading — cannot have its tail parsed as the next request
/// on the socket. Setting the header through `httpd_resp_set_hdr` would only
/// append a second `Connection` header to a response ESP-IDF manages itself.
#[must_use]
pub fn upload_response(success: bool, message: &str) -> String {
    format!("{{\"success\":{success},\"message\":\"{message}\",\"restart\":{success}}}")
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

// ======================================================== /api/parameters body

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
/// network tab either way. `application/javascript` is used rather than the
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

#[cfg(test)]
mod tests;
