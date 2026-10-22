//! An empty tank blocks water but must neither prevent standby nor wake it.
//!
//! Both `WATER_TANK_EMPTY` and `STANDBY` are excluded from the tank guard.

mod common;

use cc_domain::state::MachineState;
use cc_domain::units::Millis;
use cc_machine::{Command, Effect, Event, Request, Sensors};
use common::Harness;

/// A machine in `WATER_TANK_EMPTY` with the tank reported empty.
fn tank_empty() -> Harness {
    let mut h = Harness::in_state(MachineState::WaterTankEmpty);
    h.machine.sensors = Sensors {
        water_tank_full: false,
        ..Sensors::healthy()
    };
    h.config.hardware.sensors.watertank.enabled = true;
    h
}

#[test]
fn standby_request_transitions_to_standby() {
    let mut h = tank_empty();
    h.machine.requests.set(Request::Standby, true);

    let fx = h.tick();
    assert_eq!(
        h.state(),
        MachineState::Standby,
        "the standby request must be honoured while the tank is empty: {fx:?}"
    );
    assert!(
        !h.requested(Request::Standby),
        "the request flag must be drained"
    );
}

#[test]
fn standby_timeout_transitions_to_standby() {
    let mut h = tank_empty();
    h.config.standby.enabled = true;
    h.config.standby.time = 35.0;

    // Arm the countdown with a user action, then run it out.
    let _ = h.send(Event::Command(Command::SteamStart));
    assert!(h.machine.standby.started_at.is_some());
    assert_eq!(h.machine.standby.remaining_ms, 35 * 60 * 1000);

    // The standby countdown is only recomputed once a second, so step past the timeout in chunks.
    let mut fx = Vec::new();
    for _ in 0..(35 * 60 + 2) {
        fx = h.elapse(1_000);
        if h.state() == MachineState::Standby {
            break;
        }
    }
    assert_eq!(
        h.state(),
        MachineState::Standby,
        "the standby timeout must fire while the tank is empty: {fx:?}"
    );
}

#[test]
fn stays_put_without_a_standby_signal_or_a_refill() {
    let mut h = tank_empty();
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::WaterTankEmpty, "{fx:?}");
    assert_eq!(
        common::count(&fx, Effect::ExitState(MachineState::WaterTankEmpty)),
        0,
        "no signal means no transition — a self-transition would be noise: {fx:?}"
    );
}

#[test]
fn refill_returns_to_the_pid_state() {
    let mut h = tank_empty();
    h.machine.sensors.water_tank_full = true;

    let fx = h.tick();
    assert_eq!(h.state(), h.pid_state(), "{fx:?}");

    // And leaving `WATER_TANK_EMPTY` reaches a PID state with the PID enabled,
    // so the machine resumes heating.
    let win = common::transition_window(&fx, MachineState::WaterTankEmpty, h.pid_state());
    assert!(
        h.machine.pid.runtime_enabled,
        "a refill must restore the runtime PID: {win:?}"
    );
}

#[test]
fn standby_is_not_forced_out_by_an_empty_tank() {
    let mut h = Harness::in_state(MachineState::Standby);
    h.machine.sensors = Sensors {
        water_tank_full: false,
        ..Sensors::healthy()
    };
    h.machine.pid.runtime_enabled = false;

    let fx = h.tick();
    assert_eq!(
        h.state(),
        MachineState::Standby,
        "standby (heater off) must persist while the tank is empty: {fx:?}"
    );
    assert_eq!(
        common::count(&fx, Effect::ExitState(MachineState::Standby)),
        0
    );
}

// ---------------------------------------------------------------------------
// What the suite also checks
// ---------------------------------------------------------------------------

/// The tank guard fires for every state that is neither `WATER_TANK_EMPTY` nor `STANDBY`,
/// including the states that are mid-brew, mid-steam and mid-backflush.
///
/// This is the case that matters most for safety: an empty tank during a brew stops the water,
/// and reaching `WATER_TANK_EMPTY` is what starts the recovery.
#[test]
fn the_tank_guard_fires_in_every_other_state() {
    for state in cc_domain::state::ALL {
        let mut h = Harness::in_state(state);
        h.machine.sensors.water_tank_full = false;
        let next = h.next_state_this_tick();
        if state == MachineState::WaterTankEmpty || state == MachineState::Standby {
            assert_ne!(
                next,
                Some(MachineState::WaterTankEmpty),
                "{state:?} is excluded from the tank guard"
            );
        } else {
            assert_eq!(
                next,
                Some(MachineState::WaterTankEmpty),
                "{state:?} must fall into WATER_TANK_EMPTY"
            );
        }
    }
}

/// The emergency guard beats the tank guard.
#[test]
fn emergency_beats_the_tank_guard() {
    let mut h = Harness::in_state(MachineState::BrewRunning);
    h.machine.sensors.water_tank_full = false;
    h.machine.safety.latched = true;
    assert_eq!(
        h.next_state_this_tick(),
        Some(MachineState::EmergencyStop),
        "emergency is checked first and has no exclusion"
    );
}

/// A tank-empty machine cannot re-arm the pump, and the second layer is that the brew and hot-water
/// switches are refused in this state rather than relying on the safety verdict alone.
#[test]
fn the_water_switches_are_refused_while_the_tank_is_empty() {
    let mut h = tank_empty();
    for switch in [cc_machine::SwitchId::Brew, cc_machine::SwitchId::HotWater] {
        let fx = h.send(Event::ButtonPressed {
            switch,
            long_press: false,
        });
        assert!(
            !h.requested(Request::BrewStart) && !h.machine.hot_water_activity,
            "{switch:?} must be refused in WATER_TANK_EMPTY: {fx:?}"
        );
    }
}

/// S4 in `cc-safety`: the verdict refuses the pump while the tank is empty, and
/// only the pump.
#[test]
fn the_safety_verdict_refuses_the_pump_and_nothing_else() {
    let verdict = cc_safety::reduce(
        &cc_safety::SafetyState::CLEAR,
        &cc_safety::Telemetry::new(
            cc_domain::units::Celsius::new(25.0),
            false,
            MachineState::PidNormal,
        ),
        &cc_safety::SafetyConfig::default(),
        Millis::new(0),
    );
    assert!(!verdict.verdict.may_pump, "S4 blocks the pump");
    assert!(
        verdict.verdict.may_heat,
        "an empty tank does not block the heater"
    );
    assert_eq!(
        verdict.verdict.reason,
        Some(cc_safety::Reason::WaterTankEmpty)
    );
}

/// `keep_heater_on_empty` is what makes the standby timeout matter here: with it set the heater
/// keeps running while the tank is empty, so `STANDBY` is the only thing that will turn it off.
#[test]
fn keep_heater_on_empty_only_affects_the_heater() {
    let mut h = tank_empty();
    h.config.hardware.sensors.watertank.keep_heater_on_empty = true;
    h.config.standby.enabled = true;
    h.config.standby.time = 1.0;
    h.machine.pid.runtime_enabled = true;

    assert!(cc_machine::should_pid_be_enabled(
        MachineState::WaterTankEmpty,
        true,
        false
    ));

    // And the standby timeout still fires, so the heater is not left running
    // unattended forever.
    let _ = h.send(Event::Command(Command::SteamStart));
    for _ in 0..63 {
        let _ = h.elapse(1_000);
        if h.state() == MachineState::Standby {
            break;
        }
    }
    assert_eq!(h.state(), MachineState::Standby);
}
