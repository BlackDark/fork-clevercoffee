//! The repair loop behind finding #12.
//!
//! **Why this is a test and not a comment.** Before this, a stored configuration
//! that failed `cc_safety::validate_config` was discarded **whole** at boot: all
//! 98 parameters reverted to their compiled-in defaults except the Wi-Fi
//! credential. On a bench that cost `hardware.sensors.temperature.type` — the
//! machine reverted to TSIC-306 while a DS18B20 was fitted, and sat in
//! `SENSOR_ERROR` with `currentTemp: NaN`. One wrong number cost every other
//! number.
//!
//! These tests pin the replacement: **revert the implicated keys, keep
//! everything else**, write the repair back, and never leave the machine running
//! something the validator refuses.
//!
//! # The case that makes one pass insufficient
//!
//! `steam.setpoint` may legally be 140 and `safety.emergency_hysteresis` 15.
//! 140 + 15 is **above** the emergency threshold's 150 default, so reverting the
//! implicated key alone leaves the configuration unsafe. The repair therefore
//! re-validates and escalates — see [`repair_unsafe`].

use alloc::string::String;
use cc_config::config::{repair_unsafe, Config};
extern crate alloc;

/// A **synthetic** validator, standing in for `cc_safety::validate_config`.
///
/// It is synthetic on purpose. `cc-config` and `cc-safety` are peers, so the
/// library takes the verdict as a parameter and cannot name the real type; a test
/// here that wanted the real validator would have to rebuild the 12-field
/// `Config -> SafetyConfig` mapping that `cc_firmware::control::safety_config`
/// owns, and a drifted copy of that mapping is exactly the sort of thing this
/// repository has been bitten by. So this file tests the **loop**: which keys it
/// reverts, in what order, when it escalates, when it stops, and what it leaves
/// alone. `implicated_keys()` is pinned per variant in `cc-safety`'s own tests,
/// where the types are native. The two halves together are the production path.
fn validate(config: &Config) -> Option<&'static [&'static str]> {
    // The same shape as the real rule: the threshold must clear the steam
    // setpoint plus the hysteresis, with the hysteresis able to exceed the
    // threshold's default — which is why one pass is not enough.
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

/// The oracle's counter-example: both values individually legal, jointly unsafe,
/// and reverting the threshold alone does **not** fix it.
fn setpoint_and_hysteresis_collision() -> Config {
    let mut c = Config::default();
    c.steam.setpoint = 140.0;
    c.safety.emergency_hysteresis = 15.0;
    c.safety.emergency_temp = 145.0;
    c
}

/// Settings that must survive any repair, because they are the ones an operator
/// actually notices losing.
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
    // **The direct regression test for #12.** It asserts the *preserved* keys,
    // not merely that the offending one reverted: a test that only checked the
    // reverted key would have passed against the old discard-everything
    // behaviour if it looked at the wrong pair.
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
    // The variant whose wrong answer energises hardware at reset, so it is the
    // one worth testing hardest.
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
    // The bench case that cost the session. The repair must never touch it.
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
