//! The brew switch, and the valve fail-safe it shares a tick with.
//!
//! A momentary switch requests a start on the press edge and a stop on a second press; a toggle
//! requests a start on the rising edge and a stop on the falling edge. A switch that is disabled
//! records its level and requests nothing. An empty tank denies the brew, which is the one
//! permission check the switch does honour.
//!
//! The rest of the file is the S5 valve fail-safe: the water valve is closed whenever the machine
//! is not brewing, and closing it goes through the actuator port rather than a relay. That second
//! part is not a style preference. Closing the valve behind the facade's back leaves its internal
//! bookkeeping believing the valve is open, so the next request to open it is ignored and no
//! water flows at all.

mod common;

use cc_domain::hardware::SwitchType;
use cc_domain::state::MachineState;
use cc_machine::{Event, Request, SwitchId};
use common::Harness;

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
    // The level is the whole fact: there is no "has this press been read yet" flag to clear,
    // because the handler acts on the edge as it arrives.
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
    // The level is still recorded -- the switch physically moved -- but no request is made:
    // the enabled check comes before anything the switch has to say.
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

    // A momentary release requests nothing: for a momentary switch the stop is the second
    // press, so that a bouncing or partial release cannot end a shot.
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
    // The reason the close is an effect rather than a GPIO write: writing the relay directly
    // leaves the actuator facade believing the valve is open while the relay is off, so the next
    // request to open it is ignored and no water flows.
    //
    // The property is structural here -- the only way to reach hardware is [`cc_machine::apply`],
    // and the only way to ask for a close is [`cc_machine::Effect::CloseWaterValve`] -- so what
    // this test pins is that the effect is produced at all, and that the applier routes it to the
    // facade method.
    use cc_machine::{Actuators, Effect, MachineChannels};

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
    impl MachineChannels for NoSide {
        fn on_record_brew(&mut self, _counted: bool, _shots_since_backflush: i32) {}
        fn on_reset_shots_since_backflush(&mut self, _shots_since_backflush: i32) {}
        fn on_request_reboot(&mut self) {}
    }

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

/// A toggle that is *already on* during a brew requests nothing at all: a toggle cannot be
/// pressed twice, so the only way to stop a toggle-driven brew is to switch it off.
#[test]
fn a_toggle_press_during_a_brew_requests_nothing() {
    let mut h = Harness::in_state(MachineState::BrewRunning);
    h.config.hardware.switches.brew.r#type = SwitchType::Toggle;
    h.machine.switches.brew = false;

    let _ = h.press(SwitchId::Brew);
    assert!(!h.requested(Request::BrewStart), "{:?}", h.machine.requests);
    assert!(!h.requested(Request::BrewStop), "{:?}", h.machine.requests);
}

/// A momentary second press during a brew requests a stop.
#[test]
fn a_momentary_second_press_during_a_brew_requests_a_stop() {
    let mut h = Harness::in_state(MachineState::BrewRunning);
    h.config.hardware.switches.brew.r#type = SwitchType::Momentary;
    h.machine.switches.brew = false;

    let _ = h.press(SwitchId::Brew);
    assert!(h.requested(Request::BrewStop));
    assert!(!h.requested(Request::BrewStart));
}
