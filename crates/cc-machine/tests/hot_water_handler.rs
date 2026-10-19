//! The hot-water switch: a pump, not a state.
//!
//! The switch sets no request flag and moves the machine to no state. It marks
//! [`Machine::hot_water_activity`](cc_machine::Machine) -- which re-arms the standby countdown --
//! and the pump is then driven by the update of the state the machine is already in, `PID_NORMAL`
//! or `STEAM_RUNNING`. [`the_hot_water_switch_sets_no_request_flag`] pins that, because a request
//! flag would have implied a `HOT_WATER` state that does not exist.
//!
//! Two consequences look surprising and are deliberate: pressing the switch in standby pushes the
//! countdown out without waking the machine, and an empty tank refuses the pump through the
//! handler's own permission check as well as through the safety verdict.

mod common;

use cc_domain::hardware::SwitchType;
use cc_domain::state::MachineState;
use cc_machine::{Effect, Request, SwitchId};
use common::Harness;

#[test]
fn detects_hot_water_active() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::HotWater);
    assert!(h.machine.switches.hot_water);
}

#[test]
fn detects_hot_water_inactive() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::HotWater);
    let _ = h.release(SwitchId::HotWater);
    assert!(!h.machine.switches.hot_water);
}

#[test]
fn the_handler_is_a_no_op_when_disabled() {
    let mut h = Harness::new();
    h.config.hardware.switches.hot_water.enabled = false;
    let fx = h.press(SwitchId::HotWater);
    assert!(fx.is_empty(), "{fx:?}");
    assert!(
        !h.machine.hot_water_activity,
        "the disabled handler does not even note the activity"
    );
}

#[test]
fn permission_is_denied_when_the_water_tank_is_empty() {
    let mut h = Harness::in_state(MachineState::WaterTankEmpty);
    let fx = h.press(SwitchId::HotWater);
    assert!(!h.machine.hot_water_activity, "{fx:?}");
    assert_eq!(fx.len(), 0, "the handler is refused entirely: {fx:?}");
}

#[test]
fn permission_is_granted_in_pid_normal() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let fx = h.press(SwitchId::HotWater);
    assert!(h.machine.hot_water_activity, "{fx:?}");
    // The activity is recorded even with standby disabled, where nothing consumes it.
    assert_eq!(
        fx,
        Vec::new(),
        "with standby disabled there is no timer reset: {fx:?}"
    );
}

/// The hot-water switch sets **no** request flag.
///
/// The pump is driven from the switch level by the update of the state the machine is already
/// in, so a request flag -- and with it a `HOT_WATER` state -- would be inventing a phase this
/// machine does not have.
#[test]
fn the_hot_water_switch_sets_no_request_flag() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::HotWater);
    assert!(!h.machine.requests.any(), "{:?}", h.machine.requests);
    // And the state is unchanged until the tick, and the tick does not change it
    // either: hot water is a pump, not a state.
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
    assert!(common::has(&fx, Effect::EnablePump), "{fx:?}");
}

/// The water switch re-arms the standby countdown without waking the machine.
///
/// It is activity for the timer and nothing else: `STANDBY`'s update does not read the switch, so
/// the pump stays off until something else wakes the machine. Deliberate -- a held water switch is
/// not a request to brew.
#[test]
fn the_hot_water_switch_does_not_wake_the_machine_from_standby() {
    let mut h = Harness::in_state(MachineState::Standby);
    h.config.pid.enabled = true;
    h.config.standby.enabled = true;
    h.config.standby.time = 35.0;
    h.machine.pid.runtime_enabled = false;
    h.machine.standby.started_at = Some(cc_domain::units::Millis::new(0));
    h.machine.standby.remaining_ms = 1000;

    let fx = h.press(SwitchId::HotWater);
    assert!(h.machine.hot_water_activity, "{fx:?}");
    assert!(
        common::has(&fx, Effect::ResetStandbyTimer),
        "the timer is re-armed: {fx:?}"
    );
    assert_eq!(
        h.machine.standby.remaining_ms,
        35 * 60 * 1000,
        "reset to the configured timeout, not to 1000 ms"
    );

    let fx = h.tick();
    assert_eq!(
        h.state(),
        MachineState::Standby,
        "and the machine does not wake: {fx:?}"
    );
    assert_eq!(
        common::count(&fx, Effect::EnablePump),
        0,
        "standby's update does not read the water switch: {fx:?}"
    );
}

/// The permission layer is a *second* interlock behind `cc-safety`'s S4: even if
/// the handler ran in `WATER_TANK_EMPTY`, the verdict refuses the pump.
#[test]
fn an_empty_tank_refuses_the_pump_even_without_the_handler_layer() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.machine.sensors.water_tank_full = false;
    let fx = h.press(SwitchId::HotWater);
    let fx2 = h.tick();

    // The handler is *not* refused in PID_NORMAL — the water switch is a
    // PID_NORMAL feature, not a WATER_TANK_EMPTY one — so the pump effect is
    // produced. What refuses it is the safety verdict, which the applier obeys.
    let verdict = cc_safety::reduce(
        &cc_safety::SafetyState::CLEAR,
        &cc_safety::Telemetry::new(
            cc_domain::units::Celsius::new(25.0),
            false,
            MachineState::PidNormal,
        ),
        &cc_safety::SafetyConfig::default(),
        cc_domain::units::Millis::new(0),
    );
    assert!(!verdict.verdict.may_pump);
    assert!(!h.machine.requests.any(), "{fx:?}");
    assert_eq!(
        h.state(),
        MachineState::WaterTankEmpty,
        "the guard fires: {fx2:?}"
    );
    assert!(!h.requested(Request::BrewStart));
}

/// The switch type is read but nothing branches on it: momentary and toggle behave identically,
/// because the pump follows the level rather than an edge. Pinned so a refactor cannot start
/// branching on it without a test noticing.
#[test]
fn the_switch_type_is_read_but_nothing_branches_on_it() {
    for switch_type in [SwitchType::Momentary, SwitchType::Toggle] {
        let mut h = Harness::in_state(MachineState::PidNormal);
        h.config.hardware.switches.hot_water.r#type = switch_type;
        let fx = h.press(SwitchId::HotWater);
        assert!(h.machine.hot_water_activity, "{switch_type:?}: {fx:?}");
    }
}
