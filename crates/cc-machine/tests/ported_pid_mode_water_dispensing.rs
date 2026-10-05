//! Port of `test/test_pid_mode_water_dispensing` — 6 C++ cases.
//!
//! The twin of [`ported_steam_water_injection`], on the other side of the same
//! switch: in `PID_NORMAL` the hot-water switch dispenses hot water, in
//! `STEAM_RUNNING` it injects water into the boiler. Both are handled by a
//! state's `update` and nowhere else — there is no `HOT_WATER` state
//! (`PidStates.cpp:69`, "Hot water is handled directly in `PID_NORMAL` via pump
//! control (no separate state needed)").
//!
//! ## The C++ suite tests a mock
//!
//! As with the steam suite, `test_pid_mode_water_dispensing/test_main.cpp`
//! defines a `MockPidModeContext` (`:73-131`) with its own `pid_mode_active_`
//! field and never includes `PidStates.cpp`. The port drives the real
//! `PidNormalState::update` instead.
//!
//! ## Case mapping
//!
//! | C++ case (`test_pid_mode_water_dispensing/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `WaterSwitchActivatesPumpInPidMode` (:151) | [`water_switch_activates_pump_in_pid_mode`] |
//! | `PumpStaysActiveWhileWaterSwitchHeldInPidMode` (:184) | [`pump_stays_active_while_the_switch_is_held_in_pid_mode`] |
//! | `PumpDeactivatesWhenWaterSwitchReleasedInPidMode` (:216) | [`pump_deactivates_when_the_switch_is_released_in_pid_mode`] |
//! | `SystemStaysPidModeDuringWaterDispensing` (:252) | [`the_system_stays_in_pid_mode_during_dispensing`] |
//! | `WaterDispensingOnlyWorksInPidMode` (:275) | [`water_dispensing_only_works_in_pid_mode`] |
//! | `MultipleDenseCycles` (:304) | [`multiple_dense_cycles`] |

mod common;

use cc_domain::state::MachineState;
use cc_machine::{Effect, SwitchId};
use common::Harness;

#[test]
fn water_switch_activates_pump_in_pid_mode() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::HotWater);
    assert!(h.machine.switches.hot_water);

    let fx = h.tick();
    assert!(
        common::has(&fx, Effect::EnablePump),
        "CRITICAL: the pump must activate when the water switch is pressed in PID mode: {fx:?}"
    );
}

#[test]
fn pump_stays_active_while_the_switch_is_held_in_pid_mode() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::HotWater);

    for cycle in 0..10 {
        assert!(h.machine.switches.hot_water, "cycle {cycle}");
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::EnablePump),
            "the pump must stay active while the switch is held, cycle {cycle}: {fx:?}"
        );
    }
}

#[test]
fn pump_deactivates_when_the_switch_is_released_in_pid_mode() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::HotWater);
    let _ = h.tick();

    let _ = h.release(SwitchId::HotWater);
    let fx = h.tick();
    assert!(
        common::has(&fx, Effect::DisablePump),
        "the pump must deactivate when the water switch is released: {fx:?}"
    );
}

#[test]
fn the_system_stays_in_pid_mode_during_dispensing() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::HotWater);
    let fx = h.tick();
    assert!(common::has(&fx, Effect::EnablePump), "{fx:?}");
    assert_eq!(
        h.state(),
        MachineState::PidNormal,
        "dispensing hot water must not leave PID_NORMAL: {fx:?}"
    );
}

#[test]
fn water_dispensing_only_works_in_pid_mode() {
    // The C++ mock's `setPidModeActive(false)` is the mock's own flag. The real
    // analogue is a state in which `PidNormalState::update` is not the code
    // reading the switch — `STEAM_RUNNING` reads it itself, and every other state
    // ignores it entirely.
    let mut h = Harness::in_state(MachineState::BrewFinished);
    let _ = h.press(SwitchId::HotWater);
    let fx = h.tick();
    assert_eq!(
        common::count(&fx, Effect::EnablePump),
        0,
        "outside PID_NORMAL and STEAM_RUNNING the water switch drives nothing: {fx:?}"
    );
    assert_eq!(h.state(), MachineState::BrewFinished, "{fx:?}");
}

#[test]
fn multiple_dense_cycles() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let mut on_count = 0_usize;
    let mut off_count = 0_usize;

    for cycle in 0..5 {
        let _ = h.press(SwitchId::HotWater);
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::EnablePump),
            "the pump should be active in cycle {cycle}: {fx:?}"
        );
        on_count += 1;

        let _ = h.release(SwitchId::HotWater);
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::DisablePump),
            "the pump should be inactive in cycle {cycle}: {fx:?}"
        );
        off_count += 1;
    }
    assert_eq!(on_count, 5);
    assert_eq!(off_count, 5);
}

// ---------------------------------------------------------------------------
// What the C++ suite should also have checked
// ---------------------------------------------------------------------------

/// `PID_NORMAL`'s `update` turns the pump **off** on every loop while the switch
/// is released (`PidStates.cpp:39-42`) — it is a level-driven `update`, not an
/// edge, so the pump is actively held off rather than merely left alone.
#[test]
fn pid_normal_holds_the_pump_off_while_the_switch_is_released() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    for _ in 0..5 {
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::DisablePump),
            "the water switch is released, so the pump must be held off: {fx:?}"
        );
    }
}

/// The pump is stopped when the machine leaves `PID_NORMAL`, which is the only
/// thing `PidNormalState::onExitImpl` does (`PidStates.cpp:21-25`, "Safety:
/// Disable pump when exiting PID normal state (water dispensing cleanup)").
#[test]
fn leaving_pid_normal_stops_the_dispensing_pump() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::HotWater);
    let _ = h.tick();

    h.machine.requests.set(cc_machine::Request::BrewStart, true);
    let fx = h.tick();
    let win = common::transition_window(&fx, MachineState::PidNormal, MachineState::BrewRunning);
    assert!(
        common::has(&win, Effect::DisablePump),
        "PID_NORMAL's exit must stop the dispensing pump: {win:?}"
    );
    // ...and the new state's entry turns it straight back on, which is correct:
    // the brew needs the pump.
    assert!(common::has(&win, Effect::EnablePump), "{win:?}");
}
