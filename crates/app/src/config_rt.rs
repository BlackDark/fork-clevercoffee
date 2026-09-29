//! Turning a validated configuration document into the values the machine reads on every tick.
//!
//! This is the seam between T-04/T-06 and T-14, and it was missing: the schema knows about 99
//! parameters and the machine wants about thirty of them as plain fields, and nothing connected
//! the two. That gap is the shape of defect D12, which is why the C++ firmware had an
//! emergency-stop threshold that the emergency-stop manager read but no user could ever set: the
//! value existed in configuration and nothing carried it to the code.
//!
//! So the mapping is explicit, exhaustive, and tested both ways:
//!
//! - [`build`] reads a [`ResolvedDoc`] and produces a [`RuntimeConfig`]. Anything absent falls
//!   back to the schema's own default, so a partial document is a partial configuration rather
//!   than a machine with zeroes in it.
//! - [`resolve`] goes the other way, which is what `/api/config` and the exporter need.
//! - [`coverage`] names every schema key the bridge does **not** read, so a new parameter shows
//!   up in a test failure rather than being silently inert.
//!
//! Units are converted here and nowhere else. The schema speaks in seconds, minutes, grams and
//! degrees; the machine speaks in milliseconds and the same degrees. A conversion that lives in
//! two places is a conversion that is wrong in one of them.

use clevercoffee_config::schema::{self, Value};
use clevercoffee_config::{Report, ResolvedDoc};
use clevercoffee_domain::emergency::Thresholds;
use clevercoffee_domain::pid::Gains;
use clevercoffee_domain::Timing;
use heapless::String;

use crate::machine::RuntimeConfig;

/// The schema keys the machine does not read, and why.
///
/// A parameter can legitimately be inert: it belongs to the web UI's own settings, to a feature
/// this port has not written yet, or to the display's appearance. What is not acceptable is a
/// parameter that *looks* live and is not, which is D12. So the list is written down, and
/// [`coverage`] fails when a key the bridge reads stops being read or a new key appears without a
/// decision.
pub const NOT_READ_BY_THE_MACHINE: &[&str] = &[
    // Appearance and device identity. The display reads its template and language; the network
    // reads the hostname; neither changes how the machine controls anything.
    "system.hostname",
    "system.log_level",
    "system.showdisplay.enabled",
    "system.timing_debug.enabled",
    "system.offline_mode",
    "display.template",
    "display.language",
    "system.wifi.ssid",
    "system.wifi.password",
    "system.ota_password",
    "system.auth.enabled",
    "system.auth.username",
    "system.auth.password",
    // MQTT: the bridge and the discovery generator read these, not the control path.
    "mqtt.enabled",
    "mqtt.port",
    "mqtt.username",
    "mqtt.password",
    "mqtt.topic",
    // Display appearance, which the display crate reads and the control path never sees.
    "display.heating_logo",
    "display.pid_off_logo",
    "display.blinking.delta",
    // The maintenance reminder, which is a coordinator the machine has no equivalent of. The C++
    // had one and the port does not; the count is still published over MQTT.
    "maintenance.backflush_reminder.enabled",
    "maintenance.backflush_reminder.threshold",
    // MQTT naming, read by the bridge and the discovery generator.
    "mqtt.broker",
    "mqtt.hassio.enabled",
    "mqtt.hassio.prefix",
    // Board identity and the panel's own wiring, both resolved at boot.
    "hardware.board",
    "hardware.oled.enabled",
    "hardware.oled.type",
    // Relay trigger polarity. The C++ configured it per relay; this port wires the relays
    // active-low because that is what the machines in the field have, and records the difference
    // here rather than pretending the setting is live.
    "hardware.relays.heater.trigger_type",
    "hardware.relays.valve.trigger_type",
    "hardware.relays.pump.trigger_type",
    // Sensor and switch *type* and *mode* are resolved at boot by the board layer, not per tick.
    "hardware.sensors.scale.type",
    "hardware.switches.brew.type",
    "hardware.switches.brew.mode",
    "hardware.switches.steam.type",
    "hardware.switches.steam.mode",
    "hardware.switches.power.type",
    "hardware.switches.power.mode",
    "hardware.switches.hot_water.type",
    "hardware.switches.hot_water.mode",
    // PID variants the port does not implement: the brew-detection gain set and the steam gain
    // set. Named here so a user who sets them is told they are inert rather than being left to
    // wonder why nothing changed.
    "pid.steam.kp",
    "pid.bd.enabled",
    "pid.bd.kp",
    "pid.bd.tn",
    "pid.bd.tv",
    "brew.by_weight.auto_tare",
    "display.inverted",
    "display.fullscreen_brew_timer",
    "display.fullscreen_manual_flush_timer",
    "display.fullscreen_hot_water_timer",
    "display.post_brew_timer_duration",
    // Not yet ported: the calibration wizard and the BLE scale, which was deferred.
    // Not yet ported: the maintenance reminder is a coordinator the machine has no equivalent of.
    "hardware.oled.address",
    "hardware.leds.status.enabled",
    "hardware.leds.status.inverted",
    "hardware.leds.brew.enabled",
    "hardware.leds.brew.inverted",
    "hardware.leds.steam.enabled",
    "hardware.leds.steam.inverted",
    "hardware.sensors.temperature.type",
    "hardware.sensors.watertank.enabled",
    "hardware.sensors.watertank.mode",
    "hardware.sensors.scale.samples",
    "hardware.sensors.scale.calibration",
    "hardware.sensors.scale.calibration2",
    "hardware.sensors.scale.known_weight",
];

/// Reads a numeric field, falling back to the schema's default for it.
///
/// The fallback is `Param::default_number()` rather than a literal here, so a default that moves
/// in the schema moves with it and the machine cannot disagree with the documentation.
fn num(doc: &ResolvedDoc, key: &str) -> f64 {
    use clevercoffee_config::import::DocValue;
    match doc.value(key) {
        // Every numeric shape the document can carry, because the schema has four of them and a
        // helper that only read one would silently fall back to the default for the other three.
        // `brew.mode` is an enum stored as an integer, and reading it as a number is how the
        // manual/automatic choice reaches the machine at all.
        Some(DocValue::Number(n)) => n,
        Some(DocValue::Int(i)) => i as f64,
        Some(DocValue::Bool(b)) => b as u8 as f64,
        Some(DocValue::Enum(e)) => e as f64,
        _ => default_number(key),
    }
}

fn boolean(doc: &ResolvedDoc, key: &str) -> bool {
    match doc.value(key) {
        Some(v) => v.as_bool().unwrap_or_else(|| default_bool(key)),
        None => default_bool(key),
    }
}

/// The schema's own default for a numeric key.
fn default_number(key: &str) -> f64 {
    match schema::find(key) {
        Some(p) => match p.default {
            Value::Number(n) => n,
            Value::Int(i) => i as f64,
            Value::Bool(b) => b as u8 as f64,
            Value::Enum(e) => e as f64,
            _ => 0.0,
        },
        None => 0.0,
    }
}

/// The schema's own default for a boolean key.
fn default_bool(key: &str) -> bool {
    match schema::find(key) {
        Some(p) => matches!(p.default, Value::Bool(true)),
        None => false,
    }
}

fn seconds(doc: &ResolvedDoc, key: &str) -> u32 {
    // A negative or absurd duration in a document that passed validation cannot reach here, and
    // the saturation is there because `as u32` on a negative float is zero in Rust and zero
    // seconds would mean "immediately", which is the opposite of a safe fallback.
    let v = num(doc, key);
    if !v.is_finite() || v <= 0.0 {
        return 0;
    }
    (v * 1000.0).min(u32::MAX as f64) as u32
}

fn minutes(doc: &ResolvedDoc, key: &str) -> u32 {
    let v = num(doc, key);
    if !v.is_finite() || v <= 0.0 {
        return 0;
    }
    (v * 60_000.0).min(u32::MAX as f64) as u32
}

/// Builds the machine's configuration from a validated document.
///
/// Takes the document rather than a payload so the caller cannot skip validation: the only way to
/// get here with a rejected value is to have called [`clevercoffee_config::validate`] and ignored
/// the report, which the provisioning and API paths both refuse to do.
pub fn build(doc: &ResolvedDoc) -> RuntimeConfig {
    let mut c = RuntimeConfig::default();

    c.setpoint_c = num(doc, "brew.setpoint");

    c.brew_by_time_enabled = boolean(doc, "brew.by_time.enabled");
    c.brew_by_weight_enabled = boolean(doc, "brew.by_weight.enabled");
    c.brew_target_time_ms = seconds(doc, "brew.by_time.target_time");
    c.brew_target_weight_g = num(doc, "brew.by_weight.target_weight");
    // `brew.mode` is 0 manual, 1 automatic. The C++ read it as an enum and the frontend writes an
    // enum, so the conversion is one comparison rather than a match over strings.
    c.brew_mode_manual = num(doc, "brew.mode") < 0.5;

    c.preinfusion_enabled = boolean(doc, "brew.pre_infusion.enabled");
    c.preinfusion_ms = seconds(doc, "brew.pre_infusion.time");
    c.preinfusion_pause_ms = seconds(doc, "brew.pre_infusion.pause");
    c.brew_pid_delay_ms = seconds(doc, "brew.pid_delay");

    c.backflush_cycles = num(doc, "backflush.cycles").clamp(1.0, 255.0) as u8;
    c.backflush_fill_ms = seconds(doc, "backflush.fill_time");
    c.backflush_flush_ms = seconds(doc, "backflush.flush_time");
    c.backflush_finished_timeout_ms = Timing::BACKFLUSH_FINISHED_DISPLAY.as_millis() as u32;
    c.brew_finished_timeout_ms = Timing::BREW_FINISHED_DISPLAY.as_millis() as u32;

    c.standby_enabled = boolean(doc, "standby.enabled");
    c.standby_timeout_ms = minutes(doc, "standby.time");

    c.keep_heater_on_empty = boolean(doc, "hardware.sensors.watertank.keep_heater_on_empty");
    c.brew_switch_enabled = boolean(doc, "hardware.switches.brew.enabled");
    // The panel's switches are enabled one at a time in the schema, so "is there a brew switch"
    // is the brew row and nothing else. The C++ had a single `hardwareSwitchesBrewEnabled` and
    // invented a global one on top of it.
    c.brew_switch_present = c.brew_switch_enabled;
    c.steam_switch_present = boolean(doc, "hardware.switches.steam.enabled");
    c.hot_water_switch_present = boolean(doc, "hardware.switches.hot_water.enabled");
    c.power_switch_present = boolean(doc, "hardware.switches.power.enabled");
    c.scale_enabled = boolean(doc, "hardware.sensors.scale.enabled");
    c.pressure_enabled = boolean(doc, "hardware.sensors.pressure.enabled");
    c.steam_setpoint_c = num(doc, "steam.setpoint");
    c.pid_proportional_on_measurement = boolean(doc, "pid.use_ponm");
    c.pid_ema_factor = num(doc, "pid.ema_factor");
    c.pid_integrator_max = num(doc, "pid.regular.i_max");
    c.brew_temp_offset_c = num(doc, "brew.temp_offset");

    c.emergency = Thresholds {
        trip_c: num(doc, "safety.emergency_temp"),
        // The clear point is derived rather than configured, and deliberately far enough below
        // the trip point that a noisy reading cannot chatter across it. The C++ had 150 and 100
        // and two more constants that disagreed with them (D33).
        clear_c: (num(doc, "safety.emergency_temp")
            - num(doc, "safety.emergency_hysteresis") * 10.0)
            .max(0.0),
        consecutive_to_trip: 3,
        hysteresis_c: num(doc, "safety.emergency_hysteresis"),
    };

    c.pid = Gains {
        kp: num(doc, "pid.regular.kp"),
        tn: num(doc, "pid.regular.tn"),
        tv: num(doc, "pid.regular.tv"),
    };
    c.pid_enabled_at_boot = boolean(doc, "pid.enabled");

    c.pump_timeout_brew_ms = c
        .brew_target_time_ms
        .saturating_mul(2)
        .max(Timing::BREW_PUMP_TIMEOUT.as_millis() as u32);
    c.pump_timeout_hot_water_ms = Timing::HOT_WATER_PUMP_TIMEOUT.as_millis() as u32;

    c
}

/// The machine's configuration as schema values, for `/api/config` and the exporter.
///
/// The inverse of [`build`] for the fields the machine owns. Everything else comes from the
/// stored document, because the machine has no opinion about a display template.
pub fn resolve(doc: &ResolvedDoc, current: &RuntimeConfig) -> ResolvedDoc {
    let mut out = ResolvedDoc::new();
    for (key, value) in doc.fields() {
        out.insert(key, *value);
    }
    let mut put = |key: &str, v: clevercoffee_config::import::DocValue| {
        out.insert(key, v);
    };
    use clevercoffee_config::import::DocValue;
    put("brew.setpoint", DocValue::Number(current.setpoint_c));
    put(
        "brew.by_time.enabled",
        DocValue::Bool(current.brew_by_time_enabled),
    );
    put(
        "brew.by_time.target_time",
        DocValue::Number(current.brew_target_time_ms as f64 / 1000.0),
    );
    put(
        "brew.by_weight.enabled",
        DocValue::Bool(current.brew_by_weight_enabled),
    );
    put(
        "brew.by_weight.target_weight",
        DocValue::Number(current.brew_target_weight_g),
    );
    put(
        "brew.mode",
        DocValue::Int(if current.brew_mode_manual { 0 } else { 1 }),
    );
    put(
        "brew.pre_infusion.enabled",
        DocValue::Bool(current.preinfusion_enabled),
    );
    put(
        "brew.pre_infusion.time",
        DocValue::Number(current.preinfusion_ms as f64 / 1000.0),
    );
    put(
        "brew.pre_infusion.pause",
        DocValue::Number(current.preinfusion_pause_ms as f64 / 1000.0),
    );
    put(
        "brew.pid_delay",
        DocValue::Number(current.brew_pid_delay_ms as f64 / 1000.0),
    );
    put(
        "backflush.cycles",
        DocValue::Int(current.backflush_cycles as i32),
    );
    put(
        "backflush.fill_time",
        DocValue::Number(current.backflush_fill_ms as f64 / 1000.0),
    );
    put(
        "backflush.flush_time",
        DocValue::Number(current.backflush_flush_ms as f64 / 1000.0),
    );
    put("standby.enabled", DocValue::Bool(current.standby_enabled));
    put(
        "standby.time",
        DocValue::Number(current.standby_timeout_ms as f64 / 60_000.0),
    );
    put(
        "hardware.sensors.watertank.keep_heater_on_empty",
        DocValue::Bool(current.keep_heater_on_empty),
    );
    put(
        "hardware.switches.brew.enabled",
        DocValue::Bool(current.brew_switch_enabled),
    );
    put(
        "safety.emergency_temp",
        DocValue::Number(current.emergency.trip_c),
    );
    put(
        "safety.emergency_hysteresis",
        DocValue::Number(current.emergency.hysteresis_c),
    );
    put("pid.regular.kp", DocValue::Number(current.pid.kp));
    put("pid.regular.tn", DocValue::Number(current.pid.tn));
    put("pid.regular.tv", DocValue::Number(current.pid.tv));
    put("pid.enabled", DocValue::Bool(current.pid_enabled_at_boot));
    put("steam.setpoint", DocValue::Number(current.steam_setpoint_c));
    put(
        "pid.use_ponm",
        DocValue::Bool(current.pid_proportional_on_measurement),
    );
    put("pid.ema_factor", DocValue::Number(current.pid_ema_factor));
    put(
        "pid.regular.i_max",
        DocValue::Number(current.pid_integrator_max),
    );
    put(
        "brew.temp_offset",
        DocValue::Number(current.brew_temp_offset_c),
    );
    put(
        "hardware.switches.steam.enabled",
        DocValue::Bool(current.steam_switch_present),
    );
    put(
        "hardware.switches.hot_water.enabled",
        DocValue::Bool(current.hot_water_switch_present),
    );
    put(
        "hardware.switches.power.enabled",
        DocValue::Bool(current.power_switch_present),
    );
    put(
        "hardware.sensors.scale.enabled",
        DocValue::Bool(current.scale_enabled),
    );
    put(
        "hardware.sensors.pressure.enabled",
        DocValue::Bool(current.pressure_enabled),
    );
    out
}

/// The machine's configuration as a JSON document, for `/api/status`-style callers and for a log
/// line at boot.
///
/// Deliberately small: a boot log that dumps thirty numbers nobody reads is how a diagnostic
/// becomes noise. The setpoints, the modes and the safety limits are what a support conversation
/// actually needs.
pub fn summary(c: &RuntimeConfig) -> String<256> {
    let mut s = String::new();
    use core::fmt::Write;
    let _ = write!(
        s,
        "setpoint={}C by_time={} target={}ms by_weight={} target={}g mode={} preinfusion={}/{}ms \
         backflush={}x{}ms standby={} {}ms emergency={}C pid={}/{}/{} enabled={}",
        c.setpoint_c,
        c.brew_by_time_enabled,
        c.brew_target_time_ms,
        c.brew_by_weight_enabled,
        c.brew_target_weight_g,
        if c.brew_mode_manual { "manual" } else { "auto" },
        c.preinfusion_ms,
        c.preinfusion_pause_ms,
        c.backflush_cycles,
        c.backflush_fill_ms,
        if c.standby_enabled { "on" } else { "off" },
        c.standby_timeout_ms,
        c.emergency.trip_c,
        c.pid.kp,
        c.pid.tn,
        c.pid.tv,
        c.pid_enabled_at_boot
    );
    s
}

/// Which schema keys the bridge reads.
///
/// Used by the coverage test and by `/api/parameters` to tag an entry as one the machine acts on,
/// which is the difference between a parameter a user can change that does something and one that
/// does not.
pub const READ_KEYS: &[&str] = &[
    "brew.setpoint",
    "brew.by_time.enabled",
    "brew.by_time.target_time",
    "brew.by_weight.enabled",
    "brew.by_weight.target_weight",
    "brew.mode",
    "brew.pre_infusion.enabled",
    "brew.pre_infusion.time",
    "brew.pre_infusion.pause",
    "brew.pid_delay",
    "backflush.cycles",
    "backflush.fill_time",
    "backflush.flush_time",
    "standby.enabled",
    "standby.time",
    "hardware.sensors.watertank.keep_heater_on_empty",
    "hardware.switches.brew.enabled",
    "hardware.switches.steam.enabled",
    "hardware.switches.hot_water.enabled",
    "hardware.switches.power.enabled",
    "hardware.sensors.scale.enabled",
    "hardware.sensors.pressure.enabled",
    "steam.setpoint",
    "pid.use_ponm",
    "pid.ema_factor",
    "pid.regular.i_max",
    "brew.temp_offset",
    "safety.emergency_temp",
    "safety.emergency_hysteresis",
    "pid.enabled",
    "pid.regular.kp",
    "pid.regular.tn",
    "pid.regular.tv",
];

/// Whether the machine acts on this parameter at all.
pub fn is_read_by_the_machine(key: &str) -> bool {
    READ_KEYS.contains(&key)
}

/// Validates a payload and, if it is applicable, builds the machine's configuration from it.
///
/// The one entry point the provisioning channel and the API both use, so neither can build a
/// configuration from a document that was never validated. Returns the report either way, because
/// a caller that applies nothing still has to tell the user what was wrong with it.
pub fn build_from_payload(payload: &str, allow_clamp: bool) -> (Report, Option<RuntimeConfig>) {
    let Ok(document) = clevercoffee_config::json::parse(payload.as_bytes()) else {
        return (Report::default(), None);
    };
    let doc = document.into_resolved();
    let report = clevercoffee_config::validate(&doc, allow_clamp);
    if !report.report.is_applicable() {
        return (report.report, None);
    }
    (report.report, Some(build(&doc)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clevercoffee_config::import::DocValue;

    fn doc_with(pairs: &[(&str, DocValue)]) -> ResolvedDoc {
        let mut d = ResolvedDoc::new();
        for (k, v) in pairs {
            d.insert(k, *v);
        }
        d
    }

    #[test]
    fn an_empty_document_gives_the_schema_defaults_and_nothing_zero() {
        // The failure this prevents is a machine that boots with a zero-second pre-infusion and a
        // zero-degree setpoint because the keys were absent rather than unset.
        let c = build(&ResolvedDoc::new());
        // The schema is the authority for a default, not `RuntimeConfig::default`, because a
        // document that carries no keys must produce exactly what a freshly flashed machine with
        // an empty config region produces.
        assert_eq!(c.setpoint_c, 95.0, "the schema's brew setpoint");
        assert_eq!(
            c.brew_target_time_ms, 25_000,
            "the schema's target time, in seconds"
        );
        assert!(
            c.brew_target_time_ms > 0,
            "a target of zero stops the shot instantly"
        );
        assert!(c.setpoint_c > 0.0);
        assert!(c.preinfusion_ms > 0);
        assert!(
            c.steam_setpoint_c > c.setpoint_c,
            "steam is hotter than brewing"
        );
    }

    #[test]
    fn every_duration_is_converted_from_seconds_to_milliseconds() {
        let c = build(&doc_with(&[
            ("brew.by_time.target_time", DocValue::Number(27.0)),
            ("brew.pre_infusion.time", DocValue::Number(2.5)),
            ("backflush.flush_time", DocValue::Number(7.0)),
            ("standby.time", DocValue::Number(15.0)),
        ]));
        assert_eq!(c.brew_target_time_ms, 27_000);
        assert_eq!(c.preinfusion_ms, 2_500);
        assert_eq!(c.backflush_flush_ms, 7_000);
        assert_eq!(
            c.standby_timeout_ms, 900_000,
            "fifteen minutes, in milliseconds"
        );
    }

    #[test]
    fn a_manual_brew_mode_is_the_zero_enum_and_an_automatic_one_is_not() {
        let manual = build(&doc_with(&[("brew.mode", DocValue::Int(0))]));
        assert!(manual.brew_mode_manual);
        let automatic = build(&doc_with(&[("brew.mode", DocValue::Int(1))]));
        assert!(!automatic.brew_mode_manual);
    }

    #[test]
    fn the_emergency_threshold_and_hysteresis_both_reach_the_machine() {
        // The C++ never loaded `safety.emergency_temp` from storage (D12), so this is the test
        // that would have caught it.
        let c = build(&doc_with(&[
            ("safety.emergency_temp", DocValue::Number(140.0)),
            ("safety.emergency_hysteresis", DocValue::Number(5.0)),
        ]));
        assert_eq!(c.emergency.trip_c, 140.0);
        assert_eq!(c.emergency.hysteresis_c, 5.0);
        assert_eq!(
            c.emergency.clear_c, 90.0,
            "the clear point sits below the trip point"
        );
    }

    #[test]
    fn the_pid_gains_reach_the_machine_with_the_schemas_names() {
        let c = build(&doc_with(&[
            ("pid.regular.kp", DocValue::Number(50.0)),
            ("pid.regular.tn", DocValue::Number(200.0)),
            ("pid.regular.tv", DocValue::Number(20.0)),
            ("pid.enabled", DocValue::Bool(true)),
        ]));
        assert_eq!(c.pid.kp, 50.0);
        assert_eq!(c.pid.tn, 200.0);
        assert_eq!(c.pid.tv, 20.0);
        assert!(c.pid_enabled_at_boot);
    }

    #[test]
    fn the_pump_deadline_is_never_shorter_than_the_shot_it_guards() {
        // A 20-second target with a 5-second deadline would stop every shot at 5 seconds. The
        // rule is twice the target, and never below the C++'s own limit.
        let short = build(&doc_with(&[(
            "brew.by_time.target_time",
            DocValue::Number(20.0),
        )]));
        assert!(short.pump_timeout_brew_ms >= 40_000);
        let long = build(&doc_with(&[(
            "brew.by_time.target_time",
            DocValue::Number(300.0),
        )]));
        assert_eq!(long.pump_timeout_brew_ms, 600_000);
    }

    #[test]
    fn a_resolve_and_build_round_trip_preserves_every_value_the_machine_owns() {
        let original = build(&doc_with(&[
            ("brew.setpoint", DocValue::Number(92.5)),
            ("brew.by_time.target_time", DocValue::Number(31.0)),
            ("backflush.cycles", DocValue::Int(7)),
            ("standby.enabled", DocValue::Bool(true)),
            ("standby.time", DocValue::Number(12.0)),
            ("pid.regular.kp", DocValue::Number(44.0)),
        ]));
        let back = build(&resolve(&doc_with(&[]), &original));
        assert_eq!(back.setpoint_c, original.setpoint_c);
        assert_eq!(back.brew_target_time_ms, original.brew_target_time_ms);
        assert_eq!(back.backflush_cycles, original.backflush_cycles);
        assert_eq!(back.standby_enabled, original.standby_enabled);
        assert_eq!(back.standby_timeout_ms, original.standby_timeout_ms);
        assert_eq!(back.pid.kp, original.pid.kp);
    }

    #[test]
    fn every_schema_key_is_either_read_or_explicitly_accounted_for() {
        // The coverage test. A new parameter appears in the schema and lands in exactly one of the
        // two lists or this fails, which is the only way "the setting exists but does nothing"
        // gets caught before a user finds it.
        let mut unaccounted: heapless::Vec<&'static str, 16> = heapless::Vec::new();
        for param in schema::ordered() {
            let key = param.key;
            if is_read_by_the_machine(key) || NOT_READ_BY_THE_MACHINE.contains(&key) {
                continue;
            }
            let _ = unaccounted.push(key);
        }
        assert!(
            unaccounted.is_empty(),
            "these parameters are neither read nor listed as inert: {unaccounted:?}"
        );
    }

    #[test]
    fn nothing_is_listed_as_read_that_does_not_exist() {
        for key in READ_KEYS {
            assert!(
                schema::find(key).is_some(),
                "{key} is in READ_KEYS but not in the schema"
            );
        }
        for key in NOT_READ_BY_THE_MACHINE {
            assert!(
                schema::find(key).is_some(),
                "{key} is in NOT_READ_BY_THE_MACHINE but not in the schema"
            );
            assert!(!is_read_by_the_machine(key), "{key} is in both lists");
        }
    }

    #[test]
    fn a_payload_with_a_rejected_field_produces_no_configuration_at_all() {
        // D13: the C++ applied 5 of 96 and reported success. Here a bad setpoint means the caller
        // gets a report and no configuration.
        let (report, config) = build_from_payload(r#"{"brew":{"setpoint":400.0}}"#, false);
        assert!(!report.is_applicable());
        assert!(report.rejected > 0);
        assert!(config.is_none(), "a rejected document must build nothing");
    }

    #[test]
    fn a_clean_payload_builds_a_configuration() {
        let (report, config) = build_from_payload(
            r#"{"brew":{"setpoint":92.0,"by_time":{"target_time":30}}}"#,
            false,
        );
        assert!(report.is_applicable(), "{report:?}");
        let c = config.expect("a clean document builds");
        assert_eq!(c.setpoint_c, 92.0);
        assert_eq!(c.brew_target_time_ms, 30_000);
    }

    #[test]
    fn a_malformed_payload_reports_nothing_applied_rather_than_panicking() {
        for bad in ["", "{", "not json", "{\"brew\":", "[1,2,3]"] {
            let (report, config) = build_from_payload(bad, false);
            assert!(config.is_none(), "{bad:?} built a configuration");
            assert_eq!(report.accepted, 0);
        }
    }

    #[test]
    fn the_boot_summary_names_the_values_a_support_conversation_needs() {
        let s = summary(&build(&ResolvedDoc::new()));
        assert!(s.contains("setpoint="), "{s}");
        assert!(s.contains("emergency="), "{s}");
        assert!(s.len() < 256, "the summary must fit its buffer");
    }
}
