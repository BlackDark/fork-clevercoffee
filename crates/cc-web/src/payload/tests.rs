//! The payload tests, moved verbatim from `cc-hal-esp32/src/web.rs`'s
//! `#[cfg(any(test, feature = "device-tests"))] mod tests`.
//!
//! They were reachable only by flashing a board. `status_json` and
//! `nvs_debug_json` gained a heap parameter here, which is the one change in
//! this file: the caller passes `free_heap()`, so the bytes are the same and the
//! assertion no longer needs a chip to have a heap.

use super::*;

use alloc::vec::Vec;

use crate::telemetry::Telemetry;
use cc_config::schema::SCHEMA;

#[test]
fn the_status_payload_has_the_csqs_keys() {
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
    let json = status_json(&t, 0);
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

#[test]
fn an_absent_reading_is_null_and_never_a_fabricated_zero() {
    // A scale that is not fitted, or has not produced a reading, must not
    // report 0 g: 0 g is a real weight and the UI would show a full cup.
    let json = status_json(&Telemetry::default(), 0);
    assert!(json.contains("\"weight\":null"), "{json}");
    assert!(json.contains("\"brewWeight\":null"), "{json}");
    assert!(json.contains("\"pressure\":null"), "{json}");
    assert!(json.contains("\"waterTankFull\":null"), "{json}");
}

#[test]
fn a_present_reading_is_formatted_to_two_decimals() {
    let t = Telemetry {
        weight_g: Some(12.5),
        brew_weight_g: Some(0.0),
        ..Telemetry::default()
    };
    let json = status_json(&t, 0);
    assert!(json.contains("\"weight\":12.50"), "{json}");
    // A real zero is 0.00, not null: the two are distinguishable.
    assert!(json.contains("\"brewWeight\":0.00"), "{json}");
}

#[test]
fn the_status_payload_is_valid_json() {
    let t = Telemetry {
        ip: heapless::String::<15>::try_from("192.168.1.42").ok(),
        water_tank_full: Some(true),
        ..Telemetry::default()
    };
    let json = status_json(&t, 0);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
    assert!(parsed.is_ok(), "{json} is not valid JSON");
}

#[test]
fn the_heap_reading_is_interpolated_verbatim() {
    // The parameter the caller passes IS the gauge ESP-IDF would have returned;
    // asserting it here is what makes the move safe rather than merely
    // convenient.
    let json = status_json(&Telemetry::default(), 41_237);
    assert!(json.contains("\"heapFree\":41237"), "{json}");
}

#[test]
fn the_temperatures_payload_is_the_csqs_three_keys() {
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

#[test]
fn the_health_payload_distinguishes_alive_from_published() {
    let json = health_json(&Telemetry {
        uptime_ms: 42,
        machine_state: 20,
        ..Telemetry::default()
    });
    assert!(json.contains("\"status\":\"ok\""), "{json}");
    assert!(json.contains("\"uptime\":42"), "{json}");
}

#[test]
fn nvs_debug_reports_the_blob_and_the_heap_and_no_parameters() {
    // The C++ reported counts and heap figures only (WebServerManager.cpp:659-666)
    // and the credentials are in the same blob, so a payload listing
    // parameters would be a disclosure with no local symptom.
    let json = nvs_debug_json("cc/cc.config: schema v1, 2071 B JSON", 96, 40_000, 22_000);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed["parameters_count"], 96);
    assert_eq!(parsed["metadata"]["nvs_namespace"], "cc");
    assert_eq!(parsed["metadata"]["blob_schema_version"], 1);
    assert_eq!(parsed["metadata"]["blob_bytes"], 2071);
    assert_eq!(parsed["metadata"]["free_heap"], 40_000);
    assert_eq!(parsed["metadata"]["min_free_heap"], 22_000);
    assert_eq!(parsed["parameters"].as_array().map(Vec::len), Some(0));
}

#[test]
fn an_empty_blob_describes_as_zeroes_rather_than_panicking() {
    let json = nvs_debug_json("cc/cc.config: empty", 0, 0, 0);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed["metadata"]["blob_schema_version"], 0);
    assert_eq!(parsed["metadata"]["blob_bytes"], 0);
}

#[test]
fn the_parameters_body_carries_a_value_for_every_parameter() {
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

#[test]
fn a_parameter_value_is_typed_like_its_default() {
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

#[test]
fn a_set_parameter_reports_the_stored_value_not_the_default() {
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

#[test]
fn a_default_snapshot_reports_no_radio_rather_than_a_fabricated_one() {
    // What `/api/status` said before the radio published: no association, no
    // signal, no address. These are the *absence* readings, and they must
    // read as absence — a fabricated `wifiSignal: 3` or a plausible-looking
    // address would be a lie an operator cannot act on.
    let t = Telemetry::default();
    assert!(!t.wifi_associated);
    assert_eq!(t.signal, 0);
    assert!(t.ip.is_none());
    let json = status_json(&t, 0);
    assert!(json.contains("\"wifiAssociated\":false"), "{json}");
    assert!(json.contains("\"wifiSignal\":0"), "{json}");
    assert!(json.contains("\"ip\":null"), "{json}");
}

#[test]
fn the_status_body_reports_an_associated_radio() {
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
    let json = status_json(&t, 0);
    assert!(json.contains("\"wifiAssociated\":true"), "{json}");
    assert!(json.contains("\"wifiSignal\":4"), "{json}");
    assert!(json.contains("\"ip\":\"10.0.0.7\""), "{json}");
}

#[test]
fn steam_mode_is_the_latched_steam_flag_and_not_the_brew_state() {
    // The defect this pins. The C++'s `steamMode` is
    // `MachineStateContext::steamON_` -- set in
    // `SteamRunningState::onEntryImpl` (`SteamStates.cpp:16`), cleared in its
    // `onExitImpl` (`:21`) and in `StandbyState::onEntryImpl`
    // (`SystemStates.cpp:17`). This port emitted `"brewing"`
    // -- `state.is_brew_state()`, a *derived* value with no C++ counterpart --
    // under the C++'s name, so a poll answered a steam-mode question with a
    // brew-state answer.
    let steaming = status_json(
        &Telemetry {
            steam_mode: true,
            brewing: false,
            ..Telemetry::default()
        },
        0,
    );
    assert!(steaming.contains("\"steamMode\":true"), "{steaming}");
    assert!(steaming.contains("\"brewing\":false"), "{steaming}");

    let brewing_not_steaming = status_json(
        &Telemetry {
            steam_mode: false,
            brewing: true,
            ..Telemetry::default()
        },
        0,
    );
    assert!(
        brewing_not_steaming.contains("\"steamMode\":false"),
        "{brewing_not_steaming}"
    );
    assert!(
        brewing_not_steaming.contains("\"brewing\":true"),
        "{brewing_not_steaming}"
    );
}

#[test]
fn the_status_steam_mode_agrees_with_the_steam_toggle_response() {
    // `POST /api/steam` answers `{"success":true,"steamMode":<bool>}` from
    // `Telemetry::steam_mode` (`WebServerManager.cpp:444-475`), and the UI
    // reads that field (`machine-toggle-result.ts:9`). If `/api/status`
    // reported a different fact under the same name, a poll would contradict
    // the toggle that set it -- which is the class of bug this file already
    // records once, for the toggle reading its value before the command was
    // applied.
    //
    // **This assertion was wrong until `cc-web` was created.** It compared the
    // poll against the *whole* toggle body, `{"success":true,"steamMode":…}`,
    // and `/api/status` has never emitted a `success` key — so the test could
    // only have failed. It did not fail on hardware, because `cc-hal-esp32` is
    // unreachable from `cargo test` and `just test-esp32` is not in the gate:
    // this is the third instance of the gap finding 4.1 names, and the first
    // one that is a *broken test* rather than an untested bug. The comparison is
    // now on the field both documents carry.
    let t = Telemetry {
        steam_mode: true,
        brewing: true,
        ..Telemetry::default()
    };
    let json = status_json(&t, 0);
    let from_toggle = format!("\"steamMode\":{}", t.steam_mode);
    assert!(json.contains(&from_toggle), "{json} vs {from_toggle}");
}

// ==================================================== OTA (R3-15, deferred)

#[test]
fn the_ota_status_document_satisfies_the_uis_schema() {
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

#[test]
fn the_ota_status_document_is_not_an_update_error() {
    // The C++ only emits `error` when an update actually failed
    // (`ota.cpp:744-751`). "OTA was never built" is not a failed update, and
    // reporting it as one would make the UI show a failure toast on a
    // machine that has simply never had OTA.
    let json = ota_status_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(parsed.get("error").is_none(), "{json}");
}

#[test]
fn an_unavailable_ota_route_says_which_build_and_which_task() {
    let json = unavailable_json("OTA", "R3-15");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(parsed["error"]
        .as_str()
        .unwrap_or_default()
        .contains("not available"));
    assert_eq!(parsed["reason"], "R3-15");
}

#[test]
fn an_unavailable_endpoint_names_the_task_that_owns_it() {
    let json = unavailable_json("OTA", "R3-15");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed["reason"], "R3-15");
    assert!(parsed["error"].as_str().is_some_and(|e| e.contains("OTA")));
}

// ================================================ POST /api/config/upload

#[test]
fn the_upload_response_is_the_cpp_shape() {
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

// ============================================================== /ui assets

#[test]
fn a_javascript_bundle_is_served_as_javascript() {
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
