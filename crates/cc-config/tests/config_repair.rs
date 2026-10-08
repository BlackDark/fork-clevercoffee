//! Finding #12 repair loop.
//!
//! Old boot dropped all 98 parameters except the Wi-Fi credential. On a DS18B20
//! bench that reset `hardware.sensors.temperature.type` to TSIC-306
//! (`SENSOR_ERROR`, `NaN`). These tests revert implicated keys only.
//!
//! One pass is not enough: `steam.setpoint` 140 plus `safety.emergency_hysteresis`
//! 15 exceeds the 150 default of `safety.emergency_temp`. See [`repair_unsafe`].

use alloc::string::String;
use cc_config::config::{repair_unsafe, Config};
extern crate alloc;

/// Stand-in for `cc_safety::validate_config`.
///
/// `cc-config` cannot name that type. Real keys are pinned in
/// `div38_every_violation_names_the_keys_it_implicates`. This file pins the loop.
fn validate(config: &Config) -> Option<&'static [&'static str]> {
    // Threshold must clear steam setpoint + hysteresis. Hysteresis can exceed
    // the threshold default, so one pass is not enough.
    if config.safety.emergency_temp <= config.steam.setpoint + config.safety.emergency_hysteresis {
        return Some(&["safety.emergency_temp"]);
    }
    if config.hardware.relays.heater.trigger_type
        == cc_domain::hardware::RelayTriggerType::LowTrigger
    {
        return Some(&["hardware.relays.heater.trigger_type"]);
    }
    if config.brew.by_weight.enabled
        && !config.hardware.sensors.scale.enabled
        && !config.brew.by_time.enabled
    {
        return Some(&["brew.by_weight.enabled"]);
    }
    None
}

/// A configuration that is unsafe *only* because of the emergency threshold.
fn threshold_collision() -> Config {
    let mut c = Config::default();
    c.safety.emergency_temp = 100.0;
    c
}

/// Both values legal alone. Together unsafe. Reverting the threshold does not fix it.
fn setpoint_and_hysteresis_collision() -> Config {
    let mut c = Config::default();
    c.steam.setpoint = 140.0;
    c.safety.emergency_hysteresis = 15.0;
    c.safety.emergency_temp = 145.0;
    c
}

/// Settings a wipe would lose and a repair must keep.
fn mark_operational_settings(c: &mut Config) {
    c.system.hostname = "silvia".into();
    c.hardware.sensors.temperature.r#type =
        cc_domain::hardware::TemperatureSensorType::DallasDs18b20;
    c.mqtt.broker = "10.0.1.1".into();
    c.pid.regular.kp = 62.0;
}

#[test]
fn the_repair_resolves_a_threshold_collision() {
    let mut c = threshold_collision();
    let repair = repair_unsafe(&mut c, validate);
    assert!(
        repair.resolved,
        "a single implicated key should be enough here"
    );
    assert_eq!(repair.reverted, vec!["safety.emergency_temp".to_owned()]);
    assert!(
        validate(&c).is_none(),
        "the repaired config must pass validation"
    );
}

#[test]
fn the_repair_keeps_every_setting_it_did_not_implicate() {
    // Preserved keys, not only the reverted one. Checking only the reverted
    // key would pass against the old discard-everything path.
    let mut c = threshold_collision();
    mark_operational_settings(&mut c);

    repair_unsafe(&mut c, validate);

    assert_eq!(c.system.hostname, "silvia", "hostname must survive");
    assert_eq!(
        c.hardware.sensors.temperature.r#type,
        cc_domain::hardware::TemperatureSensorType::DallasDs18b20,
        "the probe type must survive — this is the exact loss that put the bench \\
         into SENSOR_ERROR with NaN"
    );
    assert_eq!(c.mqtt.broker, "10.0.1.1", "the MQTT broker must survive");
    assert!(
        (c.pid.regular.kp - 62.0).abs() < 1e-9,
        "PID gains must survive"
    );
}

#[test]
fn one_pass_is_not_enough_and_the_repair_escalates() {
    let mut c = setpoint_and_hysteresis_collision();
    let repair = repair_unsafe(&mut c, validate);

    assert!(
        repair.resolved,
        "steam 140 + hysteresis 15 exceeds the threshold default 150, so \
         reverting only safety.emergency_temp leaves it unsafe"
    );
    assert!(
        repair.reverted.len() > 1,
        "the repair must escalate past the first implicated key, got {:?}",
        repair.reverted
    );
    assert!(validate(&c).is_none());
}

#[test]
fn a_low_trigger_relay_reverts_to_high_trigger() {
    // Wrong polarity energises the heater at reset.
    let mut c = Config::default();
    c.hardware.relays.heater.trigger_type = cc_domain::hardware::RelayTriggerType::LowTrigger;

    let repair = repair_unsafe(&mut c, validate);
    assert!(repair.resolved);
    assert_eq!(
        c.hardware.relays.heater.trigger_type,
        cc_domain::hardware::RelayTriggerType::HighTrigger,
        "reverting must land on HighTrigger specifically — it is the safe side, \\
         and the default"
    );
    assert!(validate(&c).is_none());
}

#[test]
fn an_already_valid_configuration_is_left_alone() {
    let mut c = Config::default();
    mark_operational_settings(&mut c);
    let repair = repair_unsafe(&mut c, validate);

    assert!(repair.resolved);
    assert!(
        repair.reverted.is_empty(),
        "nothing to fix, so nothing may be reverted: {:?}",
        repair.reverted
    );
    assert_eq!(c.system.hostname, "silvia");
}

#[test]
fn the_wifi_credential_survives_a_repair() {
    // Credential must survive. A wipe leaves the machine unreachable.
    let mut c = threshold_collision();
    c.system.wifi.ssid = "Cappuxinno".into();
    c.system.wifi.password = cc_domain::secret::Secret::new(String::from("hunter2"));

    repair_unsafe(&mut c, validate);

    assert_eq!(c.system.wifi.ssid, "Cappuxinno");
    assert_eq!(
        c.system.wifi.password.expose(),
        "hunter2",
        "the credential is the one thing the old discard preserved by hand; the \
         repair must preserve it structurally, not by a special case"
    );
}
