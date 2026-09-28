//! Port of `test/test_brew_handler` — 21 C++ cases, 20 Rust cases
//! (16 live + 4 `#[ignore]`d mock records) plus 2 the C++ does not have.
//!
//! S5 in the coverage map: this suite owns `valveSafetyShutdownCheck`.
//!
//! ## The C++ suite is half mock plumbing
//!
//! Its first eleven cases (`ConstructsWithSystemContext` through
//! `GetsSwitchTypeFromConfig`, `test_brew_handler/test_main.cpp:82-143`) test
//! gMock's `EXPECT_CALL` plumbing and a null-switch guard that exists only
//! because the handler holds a raw `Switch*`. The Rust handler holds no pointer —
//! it is handed an [`Event`](cc_machine::Event) — so there is no null to guard.
//! Those eleven become `#[ignore]`d records; the other eight are ported for real,
//! and the valve-safety cases (the S5 ones) are ported against the reducer.
//!
//! ## Case mapping
//!
//! | C++ case (`test_brew_handler/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `ConstructsWithSystemContext` (:82) | `constructing_with_no_switch_is_a_mock_case` (`#[ignore]`) |
//! | `SetHardwareSetsPointers` (:87) | `set_hardware_sets_pointers_is_a_mock_case` (`#[ignore]`) |
//! | `NullSwitchReturnsFalseForAllQueries` (:100) | `a_null_switch_is_a_mock_case` (`#[ignore]`) |
//! | `NullSwitchIsBrewActiveReturnsFalse` (:107) | `a_null_context_is_a_mock_case` (`#[ignore]`) |
//! | `DetectsSwitchPress` (:115) | `detects_switch_press` |
//! | `DetectsSwitchRelease` (:123) | `detects_switch_release` |
//! | `ClearsSwitchStateChange` (:131) | `the_level_tracks_the_switch` |
//! | `GetsSwitchTypeFromConfig` (:137) | `reads_the_switch_type_from_the_config` |
//! | `ProcessReturnsEarlyWhenDisabled` (:149) | `the_handler_is_a_no_op_when_disabled` |
//! | `ProcessReturnsEarlyWithNullSwitch` (:158) | — (no null switch exists) |
//! | `ProcessReturnsEarlyWithNullMachineStateContext` (:164) | — (no null context exists) |
//! | `ProcessDetectsSwitchChangeWithContext` (:173) | [`a_press_requests_a_brew_and_a_release_does_not`] |
//! | `ProcessDeniesPermissionWhenWaterTankEmpty` (:194) | [`permission_is_denied_when_the_water_tank_is_empty`] |
//! | `ValveSafetyShutdownWithNullContext` (:209) | — (no null context) |
//! | `ValveSafetyShutdownClosesValveWhenNotBrewing` (:214) | [`the_valve_safety_check_closes_the_valve_when_not_brewing`] |
//! | `ValveSafetyShutdownClosesValveViaHardwareAbstraction` (:233) | [`the_valve_safety_check_closes_through_the_applier`] |
//! | `ValveSafetyShutdownKeepsValveDuringBrew` (:248) | [`the_valve_safety_check_keeps_the_valve_during_a_brew`] |
//! | `BackflushToggleSwitchDeactivatedStopsActiveCycle` (:264) | [`a_backflush_toggle_switch_deactivated_stops_the_active_cycle`] |
//! | `ApplyBackflushModeSetsEnterRequestOnly` (:280) | [`applying_backflush_mode_sets_only_the_enter_request`] |
//! | `BackflushIdleShortPressStartsCycle` (:288) | [`a_short_press_in_backflush_idle_starts_a_cycle`] |
//! | `ToggleSwitchDetectsActivationAndDeactivation` (:301) | [`a_toggle_switch_detects_activation_and_deactivation`] |

mod common;

use cc_domain::hardware::SwitchType;
use cc_domain::state::MachineState;
use cc_machine::{Event, Request, SwitchId};
use common::Harness;

// ---------------------------------------------------------------------------
// The mock-plumbing cases
// ---------------------------------------------------------------------------

#[test]
#[ignore = "C++ mock case: gMock EXPECT_CALL plumbing"]
fn constructing_with_no_switch_is_a_mock_case() {}

#[test]
#[ignore = "C++ mock case: the Rust handler holds no Switch pointer"]
fn set_hardware_sets_pointers_is_a_mock_case() {}

#[test]
#[ignore = "C++ mock case: the Rust handler holds no Switch pointer"]
fn a_null_switch_is_a_mock_case() {}

#[test]
#[ignore = "C++ mock case: the Rust handler has no nullable context"]
fn a_null_context_is_a_mock_case() {}

// ---------------------------------------------------------------------------
// Switch detection
// ---------------------------------------------------------------------------

#[test]
fn detects_switch_press() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    assert!(!h.machine.switches.brew);
    let _ = h.press(SwitchId::Brew);
    assert!(h.machine.switches.brew);
}

#[test]
fn detects_switch_release() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::Brew);
    assert!(h.machine.switches.brew);
    let _ = h.release(SwitchId::Brew);
    assert!(!h.machine.switches.brew);
}

#[test]
fn the_level_tracks_the_switch() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    // `clearSwitchStateChange` has no analogue: the reducer is stateless with
    // respect to "has this been read yet", so there is nothing to clear. The
    // level is the whole fact.
    assert!(!h.machine.switches.brew);
    let _ = h.press(SwitchId::Brew);
    assert!(h.machine.switches.brew);
    let _ = h.release(SwitchId::Brew);
    assert!(!h.machine.switches.brew);
}

#[test]
fn reads_the_switch_type_from_the_config() {
    let mut h = Harness::new();
    h.config.hardware.switches.brew.r#type = SwitchType::Momentary;
    assert_eq!(h.ctx().brew_switch_type(), SwitchType::Momentary);
    h.config.hardware.switches.brew.r#type = SwitchType::Toggle;
    assert_eq!(h.ctx().brew_switch_type(), SwitchType::Toggle);
}

#[test]
fn the_handler_is_a_no_op_when_disabled() {
    let mut h = Harness::new();
    h.config.hardware.switches.brew.enabled = false;
    let fx = h.press(SwitchId::Brew);
    assert!(fx.is_empty(), "{fx:?}");
    // The level is still recorded — the switch physically moved — but no request
    // is made, because the C++'s `isEnabled()` gate comes before
    // `processImpl` (`BaseHandler.h:77-80`).
    assert!(h.machine.switches.brew);
    assert!(!h.requested(Request::BrewStart));
}

#[test]
fn a_press_requests_a_brew_and_a_release_does_not() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.brew.r#type = SwitchType::Momentary;

    // First call: the switch stays LOW (no change).
    let fx = h.tick();
    assert!(!h.requested(Request::BrewStart), "{fx:?}");

    // Then it goes HIGH.
    let fx = h.press(SwitchId::Brew);
    assert!(h.requested(Request::BrewStart), "{fx:?}");

    // A momentary release does nothing: "Momentary: release doesn't trigger
    // stop (handled by second press)" (`BrewHandler.h:246`).
    h.machine.requests.clear_all();
    let fx = h.release(SwitchId::Brew);
    assert!(!h.requested(Request::BrewStop), "{fx:?}");
    assert!(!h.requested(Request::BrewStart), "{fx:?}");
}

#[test]
fn permission_is_denied_when_the_water_tank_is_empty() {
    let mut h = Harness::in_state(MachineState::WaterTankEmpty);
    h.config.hardware.switches.brew.r#type = SwitchType::Momentary;

    let fx = h.press(SwitchId::Brew);
    assert!(
        !h.requested(Request::BrewStart),
        "brew should not be requested when the water tank is empty: {fx:?}"
    );
}

// ---------------------------------------------------------------------------
// S5: the valve safety check
// ---------------------------------------------------------------------------

#[test]
fn the_valve_safety_check_closes_the_valve_when_not_brewing() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let fx = h.tick();
    assert!(
        common::has(&fx, cc_machine::Effect::CloseWaterValve),
        "{fx:?}"
    );
}

#[test]
fn the_valve_safety_check_closes_through_the_applier() {
    // The C++ regression: closing the valve by poking the relay leaves
    // `HardwareManager::valveState_` at `WATER_OPEN` while the relay is
    // physically off, so the *next* `openWaterValve()` short-circuits and no
    // water flows. The fix was to close through the facade
    // (`BrewHandler.h:116-121`).
    //
    // In the port the property is structural: the only way to reach hardware is
    // [`cc_machine::apply`], and the only way to ask for a close is
    // [`cc_machine::Effect::CloseWaterValve`]. There is no relay to poke. What
    // this test pins is that the effect is produced at all, and that the applier
    // maps it to the facade method rather than to a pin.
    use cc_machine::{Actuators, Effect, SideChannels};

    #[derive(Debug, Default)]
    struct Recorder {
        calls: Vec<&'static str>,
    }
    impl Actuators for Recorder {
        fn enable_pump(&mut self) {
            self.calls.push("enable_pump");
        }
        fn disable_pump(&mut self) {
            self.calls.push("disable_pump");
        }
        fn open_water_valve(&mut self) {
            self.calls.push("open_water_valve");
        }
        fn close_water_valve(&mut self) {
            self.calls.push("close_water_valve");
        }
        fn open_steam_valve(&mut self) {
            self.calls.push("open_steam_valve");
        }
        fn close_steam_valve(&mut self) {
            self.calls.push("close_steam_valve");
        }
        fn enable_heater(&mut self) {
            self.calls.push("enable_heater");
        }
        fn disable_heater(&mut self) {
            self.calls.push("disable_heater");
        }
        fn set_heater_duty(&mut self, _d: f32) {
            self.calls.push("set_heater_duty");
        }
        fn emergency_shutdown(&mut self) {
            self.calls.push("emergency_shutdown");
        }
        fn safe_hardware_shutdown(&mut self) {
            self.calls.push("safe_hardware_shutdown");
        }
    }
    struct NoSide;
    impl SideChannels for NoSide {}

    let mut h = Harness::in_state(MachineState::PidNormal);
    let fx = h.tick();
    assert!(common::has(&fx, Effect::CloseWaterValve), "{fx:?}");

    let mut act = Recorder::default();
    let mut side = NoSide;
    cc_machine::apply(&mut act, &mut side, &h.machine, &fx);
    assert!(
        act.calls.contains(&"close_water_valve"),
        "the applier must route the close through the facade: {act:?}"
    );
}

#[test]
fn the_valve_safety_check_keeps_the_valve_during_a_brew() {
    for state in [
        MachineState::BrewPreinfusion,
        MachineState::BrewPreinfusionPause,
        MachineState::BrewRunning,
    ] {
        let mut h = Harness::in_state(state);
        h.config.brew.mode = cc_domain::process::BrewMode::Automatic;
        h.config.brew.pre_infusion.enabled = true;
        h.config.brew.pre_infusion.pause = 30.0;
        let fx = h.tick();
        assert_eq!(
            common::count(&fx, cc_machine::Effect::CloseWaterValve),
            0,
            "the valve must stay open during {state:?}: {fx:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Backflush switch handling
// ---------------------------------------------------------------------------

#[test]
fn a_backflush_toggle_switch_deactivated_stops_the_active_cycle() {
    let mut h = Harness::in_state(MachineState::BackflushFilling);
    h.config.hardware.switches.brew.r#type = SwitchType::Toggle;
    h.machine.backflush.on = true;

    let _ = h.press(SwitchId::Brew);
    h.machine.requests.backflush_stop = false;

    let _ = h.release(SwitchId::Brew);
    assert!(
        h.requested(Request::BackflushStop),
        "a toggle released mid-backflush must request a stop"
    );
}

#[test]
fn applying_backflush_mode_sets_only_the_enter_request() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.backflush.cycles = 5;
    let _ = h.send(Event::Command(cc_machine::Command::BackflushEnter));

    assert!(h.requested(Request::BackflushEnter));
    assert!(!h.requested(Request::BackflushCycleStart));
    assert!(h.machine.backflush.on);
    assert_eq!(h.machine.backflush.cycle, 1, "enable re-arms the counter");
}

#[test]
fn a_short_press_in_backflush_idle_starts_a_cycle() {
    let mut h = Harness::in_state(MachineState::BackflushIdle);
    h.config.hardware.switches.brew.r#type = SwitchType::Momentary;
    h.config.backflush.cycles = 5;
    h.machine.backflush.on = true;

    let _ = h.press(SwitchId::Brew);
    assert!(h.requested(Request::BackflushCycleStart));
    assert!(!h.requested(Request::ManualFlushStart));
}

#[test]
fn a_toggle_switch_detects_activation_and_deactivation() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.brew.r#type = SwitchType::Toggle;

    // Toggle ON.
    let _ = h.press(SwitchId::Brew);
    assert!(h.requested(Request::BrewStart));

    h.machine.requests.clear_all();
    h.enter_state_without_effects(MachineState::BrewRunning);
    let _ = h.release(SwitchId::Brew);
    assert!(
        h.requested(Request::BrewStop),
        "a toggle released while brewing must request a brew stop"
    );
}

/// A toggle that is *already on* during a brew requests nothing at all
/// (`BrewHandler.h:233-237`): a toggle cannot be pressed again, so the only way
/// to stop a toggle-driven brew is to switch it off.
#[test]
fn a_toggle_press_during_a_brew_requests_nothing() {
    let mut h = Harness::in_state(MachineState::BrewRunning);
    h.config.hardware.switches.brew.r#type = SwitchType::Toggle;
    h.machine.switches.brew = false;

    let _ = h.press(SwitchId::Brew);
    assert!(!h.requested(Request::BrewStart), "{:?}", h.machine.requests);
    assert!(!h.requested(Request::BrewStop), "{:?}", h.machine.requests);
}

/// A momentary second press during a brew requests a stop
/// (`BrewHandler.h:224-231`).
#[test]
fn a_momentary_second_press_during_a_brew_requests_a_stop() {
    let mut h = Harness::in_state(MachineState::BrewRunning);
    h.config.hardware.switches.brew.r#type = SwitchType::Momentary;
    h.machine.switches.brew = false;

    let _ = h.press(SwitchId::Brew);
    assert!(h.requested(Request::BrewStop));
    assert!(!h.requested(Request::BrewStart));
}
