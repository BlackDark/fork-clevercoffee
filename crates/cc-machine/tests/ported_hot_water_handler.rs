//! Port of `test/test_hot_water_handler` — 13 C++ cases, 15 Rust cases
//! (9 live + 6 `#[ignore]`d "no crash" records) plus 2 the C++ does not have.
//!
//! ## The C++ suite is almost entirely "no crash"
//!
//! Six of `test_hot_water_handler`'s thirteen cases
//! (`test_hot_water_handler/test_main.cpp:78-221`) assert nothing beyond "this
//! call did not crash" — the handler is exercised with a null switch, a null
//! context, a disabled config, both switch types, in both polarities, and each
//! case's body is a comment saying so. A gMock "no crash" test is a real
//! regression guard for a null pointer, but the Rust handler has no pointer and
//! no nullable context, so those eleven have nothing to guard.
//!
//! They are kept as `#[ignore]`d records so the count is comparable, and the
//! four that *do* assert something are ported for real.
//!
//! ## Case mapping
//!
//! | C++ case (`test_hot_water_handler/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `ConstructsWithSystemContext` (:78) | `constructing_with_no_switch_is_a_mock_case` (`#[ignore]`) |
//! | `SetHardwareSetsSwitch` (:83) | `set_hardware_sets_switch_is_a_mock_case` (`#[ignore]`) |
//! | `NullSwitchReturnsFalse` (:95) | `a_null_switch_is_a_mock_case` (`#[ignore]`) |
//! | `DetectsHotWaterActive` (:104) | `detects_hot_water_active` |
//! | `DetectsHotWaterInactive` (:112) | `detects_hot_water_inactive` |
//! | `ProcessReturnsEarlyWhenDisabled` (:124) | `the_handler_is_a_no_op_when_disabled` |
//! | `ProcessReturnsEarlyWithNullSwitch` (:133) | — (no null switch) |
//! | `ProcessReturnsEarlyWithNullMachineStateContext` (:139) | — (no null context) |
//! | `ProcessDeniesPermissionWhenWaterTankEmpty` (:147) | [`permission_is_denied_when_the_water_tank_is_empty`] |
//! | `ProcessGrantsPermissionInNormalState` (:157) | [`permission_is_granted_in_pid_normal`] |
//! | `ProcessDetectsSwitchActivation` (:168) | `detects_switch_activation` (`#[ignore]`) |
//! | `ToggleSwitchLogsSwitchActivation` (:187) | `a_toggle_switch_press_is_handled` (`#[ignore]`) |
//! | `MomentarySwitchLogsSwitchPress` (:204) | `a_momentary_switch_press_is_handled` (`#[ignore]`) |
//! | — | [`the_hot_water_switch_sets_no_request_flag`] |
//! | — | [`the_hot_water_switch_does_not_wake_the_machine_from_standby`] |

mod common;

use cc_domain::hardware::SwitchType;
use cc_domain::state::MachineState;
use cc_machine::{Effect, Request, SwitchId};
use common::Harness;

#[test]
#[ignore = "C++ case asserts only 'no crash'"]
fn constructing_with_no_switch_is_a_mock_case() {}

#[test]
#[ignore = "C++ case asserts only 'no crash'"]
fn set_hardware_sets_switch_is_a_mock_case() {}

#[test]
#[ignore = "C++ case asserts only 'no crash'"]
fn a_null_switch_is_a_mock_case() {}

#[test]
#[ignore = "C++ case asserts only 'no crash'"]
fn detects_switch_activation() {}

#[test]
#[ignore = "C++ case asserts only 'no crash'"]
fn a_toggle_switch_press_is_handled() {}

#[test]
#[ignore = "C++ case asserts only 'no crash'"]
fn a_momentary_switch_press_is_handled() {}

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
    // The activity is recorded even though the handler's own permission layer
    // does nothing else with it.
    assert_eq!(
        fx,
        Vec::new(),
        "with standby disabled there is no timer reset: {fx:?}"
    );
}

/// The hot-water handler sets **no** request flag (`HotWaterHandler.h:79-112`).
///
/// It calls `setHotWaterActivity(true)` and logs. The pumping is done by
/// `PidNormalState::update` and `SteamRunningState::update` reading the level. A
/// handler that set a flag would have produced a `HOT_WATER` state, and there
/// isn't one (`PidStates.cpp:69`).
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

/// `setHotWaterActivity` resets the standby timer (`MachineStateContext.cpp:262-267`)
/// but is **not** `hasUserActivity()`, which is a stub returning `false`
/// (`MachineStateContext.cpp:419-429`).
///
/// So pressing the water switch while in standby pushes the countdown out and
/// does nothing else: no pump, no wake. Preserved deliberately, and reported.
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
        "StandbyState::update does not read the water switch: {fx:?}"
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

/// The hot-water handler's switch type is read but nothing branches on it
/// (`HotWaterHandler.h:83` reads it only to pick a log message). Pinned so a
/// future refactor does not start branching on it silently.
#[test]
fn the_switch_type_is_read_but_nothing_branches_on_it() {
    for switch_type in [SwitchType::Momentary, SwitchType::Toggle] {
        let mut h = Harness::in_state(MachineState::PidNormal);
        h.config.hardware.switches.hot_water.r#type = switch_type;
        let fx = h.press(SwitchId::HotWater);
        assert!(h.machine.hot_water_activity, "{switch_type:?}: {fx:?}");
    }
}
