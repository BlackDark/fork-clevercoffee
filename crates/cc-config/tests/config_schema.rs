//! Schema, round-trip, redaction, and store tests for `cc-config` — task R2-06.
//!
//! The C++ oracle for this area is `test/test_config` (7 cases) and
//! `test/test_config_json` (4 cases); the assertions below carry over the
//! `test_config_json` cases that have a Rust equivalent (`usesFlatDotKeys`,
//! nested get/set, sibling preservation, missing-path handling) and add the ones
//! the C++ cannot express because it has no typed configuration value.

mod support;

use cc_config::config::SafetyView;
use cc_config::json::{ImportError, RejectReason};
use cc_config::schema::{self, ParamValue};
use cc_config::{json_export, json_import, Config, ConfigStore, Secret, StoreError};
use cc_domain::hardware::{RelayTriggerType, TemperatureSensorType};
use cc_domain::units::Celsius;

use support::MemStore;

// ===================================================================== schema —

#[test]
fn the_schema_registers_ninety_eight_parameters() {
    assert_eq!(schema::SCHEMA.len(), 98);
    // 96 registered by `Config::getAllConfigParams()`, plus the two the C++
    // defines but forgets.
    let cxx_registered = schema::SCHEMA.len() - 2;
    assert_eq!(cxx_registered, 96);
}

/// The regression test for finding 1 of 01 §10. Against the C++ this
/// configuration change does not survive a reboot, and there is nothing to
/// test.
#[test]
fn the_emergency_temperature_is_persisted_and_comes_back() {
    let mut config = Config::default();
    config.safety.emergency_temp = 130.0;
    config.safety.emergency_hysteresis = 12.0;

    let text = json_export(&config).expect("export");
    let parsed = json_import(&text).expect("re-import");

    assert!(
        (parsed.safety.emergency_temp - 130.0).abs() < 1e-9,
        "safety.emergency_temp must survive a store round trip; got {}",
        parsed.safety.emergency_temp
    );
    assert!(
        (parsed.safety.emergency_hysteresis - 12.0).abs() < 1e-9,
        "safety.emergency_hysteresis must survive a store round trip; got {}",
        parsed.safety.emergency_hysteresis
    );
}

#[test]
fn the_emergency_parameters_appear_in_the_exported_json() {
    let text = json_export(&Config::default()).expect("export");
    assert!(
        text.contains("\"emergency_temp\""),
        "safety.emergency_temp missing from {text}"
    );
    assert!(
        text.contains("\"emergency_hysteresis\""),
        "safety.emergency_hysteresis missing from {text}"
    );
}

#[test]
fn every_schema_default_matches_the_config_default() {
    // The schema table and the struct are written independently; this is the
    // test that keeps them from drifting.
    let exported = json_export(&Config::default()).expect("export");
    let actual: serde_json::Value = serde_json::from_str(&exported).expect("valid json");
    let expected = schema::default_tree();
    assert_eq!(
        actual, expected,
        "Config::default() and schema::SCHEMA disagree"
    );
}

#[test]
fn the_default_config_has_exactly_the_schema_keys() {
    let exported = json_export(&Config::default()).expect("export");
    let value: serde_json::Value = serde_json::from_str(&exported).expect("valid json");
    let mut leaves = 0usize;
    count_leaves(&value, &mut leaves);
    assert_eq!(leaves, schema::SCHEMA.len(), "no extra or missing leaves");
}

fn count_leaves(value: &serde_json::Value, count: &mut usize) {
    match value {
        serde_json::Value::Object(map) => {
            for v in map.values() {
                count_leaves(v, count);
            }
        }
        _ => *count += 1,
    }
}

#[test]
fn defaults_match_the_cpp_defaults_h() {
    let c = Config::default();
    assert!(!c.pid.enabled, "defaults.h / Config.h:733");
    assert!(
        (c.pid.ema_factor - 0.6).abs() < 1e-12,
        "EMA_FACTOR, defaults.h:32"
    );
    assert!(
        (c.pid.regular.kp - 62.0).abs() < 1e-12,
        "AGGKP, defaults.h:24"
    );
    assert!(
        (c.pid.regular.tn - 52.0).abs() < 1e-12,
        "AGGTN, defaults.h:25"
    );
    assert!(
        (c.pid.regular.tv - 11.5).abs() < 1e-12,
        "AGGTV, defaults.h:26"
    );
    assert!(
        (c.pid.regular.i_max - 55.0).abs() < 1e-12,
        "AGGIMAX, defaults.h:27"
    );
    assert!(
        (c.pid.steam.kp - 150.0).abs() < 1e-12,
        "STEAMKP, defaults.h:28"
    );
    assert!(
        (c.brew.setpoint - 95.0).abs() < 1e-12,
        "SETPOINT, defaults.h:18"
    );
    assert!(
        (c.brew.temp_offset - 0.0).abs() < 1e-12,
        "TEMPOFFSET, defaults.h:19"
    );
    assert!(
        (c.steam.setpoint - 120.0).abs() < 1e-12,
        "STEAMSETPOINT, defaults.h:20"
    );
    assert!(
        (c.safety.emergency_temp - 150.0).abs() < 1e-12,
        "Config.h:815"
    );
    assert!(
        (c.safety.emergency_hysteresis - 5.0).abs() < 1e-12,
        "Config.h:824"
    );
    assert_eq!(
        c.hardware.relays.heater.trigger_type,
        RelayTriggerType::HighTrigger
    );
    assert!(c.hardware.oled.enabled);
    assert!(
        !c.hardware.sensors.watertank.enabled,
        "the tank sensor is off by default"
    );
    assert!(c.display.heating_logo);
    assert!(c.maintenance.backflush_reminder.enabled);
    assert_eq!(c.mqtt.port, 1883);
    assert_eq!(c.system.hostname, "silvia");
    assert!(c.system.wifi.ssid.is_empty());
}

#[test]
fn derived_pid_tunings_match_the_cpp_calculation() {
    // ProcessController::calculatePIDParameters(), src/control/ProcessController.cpp:382-390
    let c = Config::default();
    let (kp, ki, kd) = c.pid_tunings();
    assert!((kp - 62.0).abs() < 1e-12);
    assert!((ki - 62.0 / 52.0).abs() < 1e-12, "Ki = Kp / Tn");
    assert!((kd - 11.5 * 62.0).abs() < 1e-12, "Kd = Tv * Kp");
}

#[test]
fn a_zero_tn_gives_a_zero_ki_rather_than_a_division_by_zero() {
    // ProcessController.cpp:383-387 explicitly guards this.
    let mut c = Config::default();
    c.pid.regular.tn = 0.0;
    let (_, ki, _) = c.pid_tunings();
    assert!((ki - 0.0).abs() < 1e-12);
    assert!(ki.is_finite());
}

#[test]
fn the_effective_brew_setpoint_includes_the_offset() {
    let mut c = Config::default();
    assert!((c.effective_brew_setpoint() - 95.0).abs() < 1e-12);
    c.brew.temp_offset = 2.5;
    assert!(
        (c.effective_brew_setpoint() - 97.5).abs() < 1e-12,
        "the offset compensates a probe that reads low"
    );
}

#[test]
fn the_heater_window_is_the_cpp_constant() {
    // ProcessState.h:183 windowSize_ = 1000
    assert_eq!(Config::HEATER_WINDOW_MS, 1000);
}

// ================================================================= round trip —

#[test]
fn default_to_json_to_config_is_lossless() {
    let original = Config::default();
    let text = json_export(&original).expect("export");
    let parsed = json_import(&text).expect("re-import");
    assert_eq!(original, parsed);
}

#[test]
fn a_fully_customised_configuration_survives_a_round_trip() {
    let mut original = Config::default();
    original.pid.enabled = true;
    original.pid.regular.kp = 41.5;
    original.brew.setpoint = 88.0;
    original.brew.mode = cc_domain::process::BrewMode::Automatic;
    original.steam.setpoint = 135.0;
    original.safety.emergency_temp = 170.0;
    original.display.template = cc_domain::system::DisplayTemplate::Modern;
    original.display.language = cc_domain::system::Language::German;
    original.system.log_level = cc_domain::system::LogLevel::Debug;
    original.hardware.oled.address = cc_domain::hardware::OledAddress::Addr3d;
    original.hardware.sensors.watertank.mode = cc_domain::hardware::SwitchMode::NormallyOpen;
    original.hardware.relays.pump.trigger_type = RelayTriggerType::LowTrigger;
    original.hardware.sensors.scale.r#type = cc_domain::hardware::ScaleType::Bluetooth;
    original.backflush.cycles = 7;
    original.system.wifi.ssid = String::from("test-ssid");
    original.system.wifi.password = Secret::new(String::from("test-pass"));
    original.mqtt.password = Secret::new(String::from("test-mqtt"));

    let text = json_export(&original).expect("export");
    let parsed = json_import(&text).expect("re-import");
    assert_eq!(original, parsed);
}

#[test]
fn enums_serialise_as_integers_like_the_cpp() {
    // Config.h:423 writes `static_cast<int>(currentValue_)`.
    let text = json_export(&Config::default()).expect("export");
    assert!(
        text.contains("\"mode\":0"),
        "brew.mode must be an integer: {text}"
    );
    assert!(
        text.contains("\"log_level\":2"),
        "system.log_level must be 2 (INFO)"
    );
    assert!(
        text.contains("\"trigger_type\":1"),
        "hardware.relays.heater.trigger_type must be 1 (HIGH_TRIGGER)"
    );
}

#[test]
fn the_json_is_nested_not_flat() {
    // ConfigJson::setNested, src/ConfigJson.cpp:83-106
    let text = json_export(&Config::default()).expect("export");
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    let kp = value
        .get("pid")
        .and_then(|p| p.get("regular"))
        .and_then(|r| r.get("kp"))
        .and_then(serde_json::Value::as_f64);
    assert!(
        (kp.unwrap_or_default() - 62.0).abs() < 1e-12,
        "pid.regular.kp must nest"
    );
}

#[test]
fn a_partial_document_leaves_every_other_parameter_at_its_default() {
    // Config.cpp:328-331 skips absent parameters, exactly as this does.
    let parsed = json_import(r#"{"brew":{"setpoint":88.0}}"#).expect("import");
    assert!((parsed.brew.setpoint - 88.0).abs() < 1e-12);
    assert!(
        (parsed.steam.setpoint - 120.0).abs() < 1e-12,
        "untouched key keeps its default"
    );
    assert!((parsed.pid.regular.kp - 62.0).abs() < 1e-12);
}

#[test]
fn sibling_keys_under_a_shared_parent_are_preserved() {
    // test_config_json / PreservesSiblingKeysUnderSharedParents
    let text = r#"{
        "brew": {
            "setpoint": 95.0,
            "mode": 1,
            "by_time": { "enabled": true, "target_time": 28.0 },
            "pre_infusion": { "pause": 5.0 }
        },
        "pid": { "enabled": true, "regular": { "kp": 50.0 } },
        "system": { "wifi": { "ssid": "test-ssid" } }
    }"#;
    let parsed = json_import(text).expect("import");
    assert!((parsed.brew.setpoint - 95.0).abs() < 1e-12);
    assert_eq!(parsed.brew.mode, cc_domain::process::BrewMode::Automatic);
    assert!(parsed.brew.by_time.enabled);
    assert!((parsed.brew.by_time.target_time - 28.0).abs() < 1e-12);
    assert!((parsed.brew.pre_infusion.pause - 5.0).abs() < 1e-12);
    assert!(parsed.pid.enabled);
    assert!((parsed.pid.regular.kp - 50.0).abs() < 1e-12);
    assert_eq!(parsed.system.wifi.ssid, "test-ssid");
}

// ============================================================ import rejection

#[test]
fn an_empty_body_is_rejected() {
    // Config.cpp:348
    assert_eq!(json_import(""), Err(ImportError::Empty));
}

#[test]
fn a_non_object_root_is_rejected() {
    // Config.cpp:364
    assert_eq!(json_import("[1,2,3]"), Err(ImportError::NotAnObject));
    assert_eq!(json_import("42"), Err(ImportError::NotAnObject));
    assert_eq!(json_import("\"hello\""), Err(ImportError::NotAnObject));
    assert_eq!(json_import("null"), Err(ImportError::NotAnObject));
}

#[test]
fn unparseable_json_is_rejected() {
    // Config.cpp:353
    assert!(matches!(json_import("{"), Err(ImportError::Syntax { .. })));
    assert!(matches!(
        json_import("not json"),
        Err(ImportError::Syntax { .. })
    ));
}

#[test]
fn flat_dotted_keys_are_rejected_and_named() {
    // test_config_json / DetectsFlatTopLevelKeys, and Config.cpp:370.
    match json_import(r#"{"brew.setpoint": 95.0}"#) {
        Err(ImportError::FlatDottedKeys { key }) => assert_eq!(key, "brew.setpoint"),
        other => panic!("expected a flat-key rejection, got {other:?}"),
    }
}

#[test]
fn a_flat_document_with_nested_siblings_is_still_rejected() {
    // test_config_json / DetectsFlatTopLevelKeys: the check is per *top-level*
    // key, exactly as `usesFlatDotKeys` is. A dotted key buried inside a
    // sub-object is not a flat key and is simply an unknown key.
    match json_import(r#"{"brew.setpoint": 95.0, "pid": {"enabled": true}}"#) {
        Err(ImportError::FlatDottedKeys { key }) => assert_eq!(key, "brew.setpoint"),
        other => panic!("expected a flat-key rejection, got {other:?}"),
    }
    // A dotted key one level down is not a flat key.
    let ok = json_import(r#"{"pid": {"regular.kp": 50.0}}"#);
    assert!(
        ok.is_err(),
        "a nested dotted key names nothing, so nothing imports"
    );
}

#[test]
fn an_object_with_no_known_key_is_rejected() {
    // Config.cpp:344: `return updatedCount > 0;`
    assert_eq!(
        json_import(r#"{"nonsense": {"a": 1}}"#),
        Err(ImportError::NoKnownParameters)
    );
    assert_eq!(json_import("{}"), Err(ImportError::NoKnownParameters));
}

#[test]
fn an_oversized_document_is_rejected() {
    // Config.cpp:359 (`doc.overflowed()`), here an explicit policy.
    let filler = "x".repeat(cc_config::json::MAX_TEXT_LEN + 64);
    let text = format!(r#"{{"system":{{"hostname":"{filler}"}}}}"#);
    assert!(
        matches!(
            json_import(&text),
            Err(ImportError::TooLarge { .. } | ImportError::InvalidValues { .. })
        ),
        "an oversized text parameter must be refused"
    );
}

#[test]
fn a_value_over_its_range_rejects_the_whole_import() {
    // Deliberately stricter than the C++, which logs a warning and continues
    // (Config.cpp:331-342) and then reports success.
    match json_import(r#"{"brew":{"setpoint":500.0}}"#) {
        Err(ImportError::InvalidValues { rejected }) => {
            assert_eq!(rejected.len(), 1);
            assert_eq!(rejected[0].key, "brew.setpoint");
            assert_eq!(
                rejected[0].reason,
                RejectReason::OutOfRange {
                    min: 20.0,
                    max: 110.0
                }
            );
        }
        other => panic!("expected an invalid-value rejection, got {other:?}"),
    }
}

#[test]
fn several_bad_values_are_all_reported_at_once() {
    match json_import(r#"{"brew":{"setpoint":500.0},"pid":{"regular":{"kp":-1.0}}}"#) {
        Err(ImportError::InvalidValues { rejected }) => {
            assert_eq!(
                rejected.len(),
                2,
                "both bad values must be named: {rejected:?}"
            );
        }
        other => panic!("expected an invalid-value rejection, got {other:?}"),
    }
}

#[test]
fn a_wrong_json_type_is_rejected() {
    match json_import(r#"{"pid":{"enabled":"yes"}}"#) {
        Err(ImportError::InvalidValues { rejected }) => {
            assert_eq!(rejected[0].reason, RejectReason::WrongType);
        }
        other => panic!("expected a type rejection, got {other:?}"),
    }
}

#[test]
fn an_impossible_enum_discriminant_is_rejected() {
    match json_import(r#"{"hardware":{"oled":{"type":9}}}"#) {
        Err(ImportError::InvalidValues { rejected }) => {
            assert_eq!(rejected[0].reason, RejectReason::UnknownEnumDiscriminant);
        }
        other => panic!("expected an enum rejection, got {other:?}"),
    }
}

#[test]
fn the_heater_relay_trigger_type_can_be_set_to_low_via_json() {
    // The *schema* accepts it — validation of the combination happens in
    // cc-safety, not here. This test exists to prove the two layers are
    // separate, so a reader does not assume cc-config silently protects the
    // heater.
    let parsed =
        json_import(r#"{"hardware":{"relays":{"heater":{"trigger_type":0}}}}"#).expect("import");
    assert_eq!(
        parsed.hardware.relays.heater.trigger_type,
        RelayTriggerType::LowTrigger
    );
    assert_eq!(
        parsed.safety_view().heater_relay_trigger,
        RelayTriggerType::LowTrigger
    );
}

#[test]
fn a_legitimate_low_trigger_pump_relay_is_accepted() {
    // Only the *heater* is unsafe; a pump or valve relay is a cold contact and
    // a floating input is harmless.
    let parsed =
        json_import(r#"{"hardware":{"relays":{"pump":{"trigger_type":0}}}}"#).expect("import");
    assert_eq!(
        parsed.hardware.relays.pump.trigger_type,
        RelayTriggerType::LowTrigger
    );
}

#[test]
fn a_leaf_where_an_object_belongs_is_treated_as_absent() {
    // ConfigJson::getNested returns null and the parameter is skipped.
    let parsed = json_import(r#"{"brew":{"setpoint":88.0,"by_time":7}}"#).expect("import");
    assert!(
        (parsed.brew.setpoint - 88.0).abs() < 1e-12,
        "the good sibling still imports"
    );
    assert!(
        !parsed.brew.by_time.enabled,
        "the malformed group must fall back to its default, not leak through"
    );
}

#[test]
fn unknown_keys_are_ignored_for_forward_compatibility() {
    // A document exported by a newer firmware must still import.
    let parsed = json_import(r#"{"pid":{"enabled":true,"future_knob":42},"totally_new":{"x":1}}"#)
        .expect("import");
    assert!(parsed.pid.enabled);
}

// ================================================================= redaction —

/// Skill rule 7. `format!("{cfg:?}")` must not contain a single character of
/// any credential.
#[test]
fn debug_of_a_config_contains_no_plaintext_credentials() {
    let mut config = Config::default();
    config.system.auth.password = Secret::new(String::from("hunter2"));
    config.mqtt.password = Secret::new(String::from("swordfish"));
    config.system.ota_password = Secret::new(String::from("opensesame"));
    config.system.wifi.password = Secret::new(String::from("letmein"));
    config.system.wifi.ssid = String::from("test-ssid");
    config.mqtt.username = String::from("test-user");

    let rendered = format!("{config:?}");
    for secret in ["hunter2", "swordfish", "opensesame", "letmein"] {
        assert!(
            !rendered.contains(secret),
            "{secret} leaked into Debug output"
        );
    }
    // Non-secrets must still be visible, or redaction has broken the logs.
    assert!(rendered.contains("test-ssid"), "the SSID is not a secret");
    assert!(
        rendered.contains("test-user"),
        "the MQTT username is not a secret"
    );
    assert!(
        rendered.contains("[redacted]"),
        "the redacted marker should be visible so a reader can tell it happened"
    );
}

#[test]
fn display_of_a_secret_contains_no_plaintext() {
    let password = Secret::new(String::from("hunter2"));
    assert!(!format!("{password}").contains("hunter2"));
    assert!(!format!("{password:?}").contains("hunter2"));
    assert_eq!(format!("{password}"), "[redacted]");
}

#[test]
fn the_json_blob_does_contain_the_credentials_because_the_machine_needs_them() {
    // Stated explicitly so the redaction above is not mistaken for encryption.
    let mut config = Config::default();
    config.system.auth.password = Secret::new(String::from("hunter2"));
    let text = json_export(&config).expect("export");
    assert!(
        text.contains("hunter2"),
        "the stored blob must carry the real password; redaction is for diagnostics only"
    );
}

#[test]
fn the_safety_view_names_exactly_the_safety_relevant_values() {
    let mut config = Config::default();
    config.safety.emergency_temp = 165.0;
    config.safety.emergency_hysteresis = 9.0;
    config.steam.setpoint = 118.0;
    config.hardware.relays.heater.trigger_type = RelayTriggerType::HighTrigger;
    config.hardware.sensors.temperature.r#type = TemperatureSensorType::DallasDs18b20;
    let view: SafetyView = config.safety_view();
    // `SafetyView` is `Celsius`-typed so the join with `cc_safety::SafetyConfig`
    // is a copy rather than a narrowing cast (see `SafetyView::emergency_temp`),
    // so the assertions are against `Celsius`, not against a float.
    assert_eq!(view.emergency_temp, Celsius::new(165.0));
    assert_eq!(view.emergency_hysteresis, Celsius::new(9.0));
    assert_eq!(view.steam_setpoint, Celsius::new(118.0));
    assert_eq!(view.heater_relay_trigger, RelayTriggerType::HighTrigger);
    assert_eq!(
        view.temperature_sensor,
        TemperatureSensorType::DallasDs18b20
    );
}

#[test]
fn the_default_temperature_sensor_is_the_cpps_tsic_306() {
    // PARITY with `Config.h:1085-1092`, which defaults this to `TSIC_306`.
    //
    // An earlier revision defaulted it to `DALLAS_DS18B20` and made
    // `cc_safety::validate_config` reject `TSIC_306`, so that a default the
    // validator refuses could not be the machine's own configuration. Both of
    // those are now reversed (R3-07): the driver exists, the validator accepts
    // it, and the C++'s value is restored.
    //
    // The probe fitted to the attached machine is a DS18B20 and always has been
    // — so a machine running these defaults reports a not-connected temperature
    // sensor and says so, rather than silently reading a bus it does not own.
    let config = Config::default();
    assert_eq!(
        config.hardware.sensors.temperature.r#type,
        TemperatureSensorType::Tsic306
    );
    // And the schema default must agree, or the export test catches it.
    let spec = schema::SCHEMA
        .iter()
        .find(|s| s.key == "hardware.sensors.temperature.type")
        .expect("the parameter is registered");
    assert_eq!(
        spec.default,
        ParamValue::Enum(TemperatureSensorType::Tsic306 as i8)
    );
    // The wire value is unchanged: both enums are positional
    // (`defaults.h:170-173`), so a C++-written NVS still parses.
    assert_eq!(TemperatureSensorType::DallasDs18b20 as i8, 1);
    assert_eq!(TemperatureSensorType::Tsic306 as i8, 0);
}

// ==================================================================== store —

#[test]
fn an_empty_store_loads_nothing_and_uses_defaults() {
    let mut store = MemStore::empty();
    assert_eq!(store.load().expect("load"), None);
    assert_eq!(Config::default(), Config::default());
}

#[test]
fn save_then_load_returns_the_same_configuration() {
    let mut store = MemStore::empty();
    let mut original = Config::default();
    original.safety.emergency_temp = 133.0;
    original.system.hostname = String::from("test-host");

    store.save(&original).expect("save");
    assert_eq!(store.saves, 1);
    assert_eq!(store.load().expect("load"), Some(original));
}

#[test]
fn erase_all_returns_the_store_to_never_written() {
    let mut store = MemStore::with(Config::default());
    store.erase_all().expect("erase");
    assert_eq!(store.load().expect("load"), None);
    assert_eq!(store.peek(), None);
}

#[test]
fn reset_to_defaults_overwrites_a_stored_configuration() {
    let mut store = MemStore::with(Config::default());
    let mut custom = Config::default();
    custom.brew.setpoint = 60.0;
    store.save(&custom).expect("save");
    store.reset_to_defaults().expect("reset");
    assert_eq!(store.load().expect("load"), Some(Config::default()));
}

#[test]
fn a_corrupt_store_is_reported_not_guessed_at() {
    let store = MemStore::failing(StoreError::Corrupt);
    let mut store = store;
    assert_eq!(store.load(), Err(StoreError::Corrupt));
}

#[test]
fn a_write_failure_leaves_the_previous_configuration_in_place() {
    let mut store = MemStore::with(Config::default());
    store.fail_with = Some(StoreError::WriteFailed);
    let mut custom = Config::default();
    custom.brew.setpoint = 60.0;
    assert_eq!(store.save(&custom), Err(StoreError::WriteFailed));
    // The point of a blob store: a failed write cannot leave a half-applied
    // configuration, because there is no "half" of one blob.
    assert!(store.peek().is_some());
}

#[test]
fn store_errors_render() {
    assert!(!StoreError::Unavailable.to_string().is_empty());
    assert!(!StoreError::Corrupt.to_string().is_empty());
    assert!(!StoreError::ReadOnly.to_string().is_empty());
    assert!(!StoreError::WriteFailed.to_string().is_empty());
}

// =============================================== the docs/example_config.json shape

/// `docs/example_config.json` must import unchanged. That file is what a user
/// downloads, and the C++ accepts it, so this port has to as well.
#[test]
fn a_document_in_the_shape_of_the_shipped_example_imports() {
    // A representative subset of docs/example_config.json, in its exact shape.
    let text = r#"{
        "backflush": { "cycles": 5, "fill_time": 5, "flush_time": 10 },
        "brew": {
            "by_time": { "enabled": false, "target_time": 25 },
            "by_weight": { "auto_tare": false, "enabled": true, "target_weight": 36 },
            "mode": 1,
            "pid_delay": 10,
            "pre_infusion": { "enabled": true, "pause": 5, "time": 2 },
            "setpoint": 95,
            "temp_offset": 0
        },
        "display": {
            "blinking": { "delta": 0.3 },
            "fullscreen_brew_timer": true,
            "fullscreen_hot_water_timer": true,
            "fullscreen_manual_flush_timer": true,
            "heating_logo": true,
            "inverted": false,
            "language": 0,
            "pid_off_logo": true,
            "post_brew_timer_duration": 3,
            "template": 0
        },
        "pid": { "enabled": true, "regular": { "kp": 62, "tn": 52, "tv": 11.5, "i_max": 55 } },
        "safety": { "emergency_temp": 150, "emergency_hysteresis": 5 },
        "steam": { "setpoint": 120 },
        "system": { "hostname": "silvia", "wifi": { "ssid": "test-ssid", "password": "test-pass" } }
    }"#;
    let parsed = json_import(text).expect("the shipped example must import");
    assert_eq!(parsed.brew.mode, cc_domain::process::BrewMode::Automatic);
    assert!(parsed.brew.pre_infusion.enabled);
    assert!(parsed.display.fullscreen_brew_timer);
    assert_eq!(parsed.system.wifi.ssid, "test-ssid");
    assert!((parsed.safety.emergency_temp - 150.0).abs() < 1e-9);
}

// =========================================================== wifi credential

#[test]
fn a_provisioned_credential_is_stored_and_cleared_as_one_thing() {
    let mut config = Config::default();
    assert!(
        !config.is_wifi_provisioned(),
        "the defaults are unprovisioned"
    );

    config.set_wifi_credential(String::from("kitchen"), String::from("hunter2"));
    assert!(config.is_wifi_provisioned());
    assert_eq!(config.system.wifi.ssid, "kitchen");
    assert_eq!(config.wifi_password(), "hunter2");

    config.clear_wifi_credential();
    assert!(!config.is_wifi_provisioned());
    assert!(config.system.wifi.ssid.is_empty());
    // A cleared credential must leave an *empty* password, not the old one.
    assert!(config.wifi_password().is_empty());
}

#[test]
fn an_open_network_is_a_credential_with_an_empty_password() {
    // `system.wifi.password` is documented as "leave empty for open networks",
    // so an empty password is a value, not the absence of one. What makes a
    // machine unprovisioned is an empty SSID, and only that.
    let mut config = Config::default();
    config.set_wifi_credential(String::from("guest"), String::new());
    assert!(config.is_wifi_provisioned());
    assert!(config.wifi_password().is_empty());
}

#[test]
fn a_stored_credential_survives_a_blob_round_trip() {
    // The provisioning path is only useful if the write is durable, and the
    // durability question is the store's, not the setter's — so the test spans
    // both. `Debug` must not leak the password anywhere along the way.
    let mut store = MemStore::empty();
    let mut config = Config::default();
    config.set_wifi_credential(String::from("kitchen"), String::from("hunter2"));
    store.save(&config).expect("save");

    let mut reloaded = store.load().expect("load").expect("a blob was written");
    assert!(reloaded.is_wifi_provisioned());
    assert_eq!(reloaded.wifi_password(), "hunter2");
    assert!(
        !format!("{reloaded:?}").contains("hunter2"),
        "the Debug of a Config must not carry the Wi-Fi password"
    );

    reloaded.clear_wifi_credential();
    store.save(&reloaded).expect("save the cleared credential");
    let third = store.load().expect("load").expect("a blob was written");
    assert!(!third.is_wifi_provisioned());
}

#[test]
fn schema_specs_accept_their_own_defaults() {
    for spec in schema::SCHEMA {
        assert!(
            spec.accepts(spec.default),
            "{} default {:?} is outside its own range",
            spec.key,
            spec.default
        );
    }
}

#[test]
fn param_value_kinds_round_trip_through_the_spec() {
    let spec = schema::find("brew.setpoint").expect("registered");
    assert_eq!(ParamValue::Float(95.0).kind(), schema::ParamKind::Float);
    assert!(spec.accepts(ParamValue::Float(95.0)));
    assert!((ParamValue::Float(95.0).as_f64().unwrap_or_default() - 95.0).abs() < 1e-12);
    assert!(ParamValue::Text("x").as_f64().is_none());
}
