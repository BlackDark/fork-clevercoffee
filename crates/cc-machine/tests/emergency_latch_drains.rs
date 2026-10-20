//! A brew pressed during the emergency latch must not fire when the latch clears.
//!
//! **Measured on a bench ESP32, 2026-10-07.** With a bench build (`just bench-flash`, emergency
//! threshold at 30 °C) the machine was tripped by warming a DS18B20 by hand. A brew press during
//! the latch was correctly refused -- `enablePump REFUSED - latched 1`, and the pump LED stayed
//! dark. The probe then cooled below the threshold, the latch cleared, **and the machine started
//! that same brew with nobody pressing anything.** On a real machine the water is at brewing
//! temperature and the reservoir is not empty.
//!
//! The state drains its action requests on entry and in its update: `AG-REPO-24` ("states that
//! cannot act on action requests must drain them") plus ADR-0003 rules 2 and 3.
//!
//! The drain is not a new policy, and the last test is the contrast that shows it: an empty tank
//! already drained the same press before the latch did.

mod common;

use cc_domain::state::MachineState;
use cc_machine::event::SwitchId;

/// The scenario, as a helper: trip, press, recover.
fn latched_machine_with_a_brew_pressed_during_the_latch() -> common::Harness {
    let mut h = common::Harness::in_state(MachineState::PidNormal);

    // The trip: three readings above the threshold latch the safety state and
    // the machine enters EMERGENCY_STOP. The safety reducer runs on the Tick
    // event, so driving the temperature through the context is not enough —
    // this is the state machine's own path.
    h.machine.safety.latched = true;
    let _ = h.elapse(10);
    assert_eq!(
        h.state(),
        MachineState::EmergencyStop,
        "the latch must put the machine in EMERGENCY_STOP"
    );

    // The press: the operator taps brew while the machine is over-temperature.
    h.press(SwitchId::Brew);

    h
}

#[test]
fn a_brew_pressed_during_the_latch_is_refused_and_then_dropped() {
    let mut h = latched_machine_with_a_brew_pressed_during_the_latch();

    // While latched: nothing may be energised, and the request must be gone.
    let fx = h.elapse(10);
    assert!(
        !common::has(&fx, cc_machine::Effect::EnablePump),
        "a latched machine may not energise the pump: {fx:?}"
    );
    assert!(
        !common::has(&fx, cc_machine::Effect::OpenWaterValve),
        "a latched machine may not open the water valve: {fx:?}"
    );
    assert!(
        common::has(&fx, cc_machine::Effect::ClearActionRequests),
        "the latch must drain action requests on every tick, not only on entry: \
         a request arrives *after* entry runs. Effects {fx:?}"
    );
    assert!(
        !h.machine.requests.brew_start,
        "the brew request must not survive a tick of EMERGENCY_STOP — this is \
         the exact defect #15 is"
    );
}

#[test]
fn the_recovery_tick_starts_no_brew() {
    let mut h = latched_machine_with_a_brew_pressed_during_the_latch();

    // Several ticks of a latched machine, so nothing is left pending.
    for _ in 0..5 {
        h.elapse(10);
    }

    // The recovery: the temperature falls, S1 clears the latch, and the machine
    // leaves EMERGENCY_STOP for the PID state.
    h.machine.safety.clear();
    let fx = h.elapse(10);
    assert_ne!(
        h.state(),
        MachineState::EmergencyStop,
        "clearing the latch must leave EMERGENCY_STOP"
    );
    assert!(
        !common::has(&fx, cc_machine::Effect::EnablePump),
        "the machine must not begin the brew that was pressed during the \
         latch: {fx:?}"
    );
    assert!(
        !h.machine.requests.brew_start,
        "no brew may be left pending for PID_NORMAL to pick up"
    );
    assert!(
        !common::has(
            &fx,
            cc_machine::Effect::ExitState(MachineState::EmergencyStop)
        ) || h.state() != MachineState::BrewRunning,
        "recovery must not land in a brew state"
    );
}

/// The contrast with an empty tank: `WATER_TANK_EMPTY` already drained the same press, so the
/// latch draining it too is consistency rather than a second policy.
#[test]
fn an_empty_tank_drains_the_same_press_and_the_latch_now_does_too() {
    let mut tank = common::Harness::in_state(MachineState::PidNormal);
    tank.press(SwitchId::Brew);

    let mut latch = latched_machine_with_a_brew_pressed_during_the_latch();
    let _ = tank.elapse(10);
    let _ = latch.elapse(10);

    assert!(
        !latch.machine.requests.brew_start,
        "the latch must behave like the states that already drain"
    );
}

/// **The tick the latch clears on.** The drain in `update` cannot be guarded on the latch: the
/// exit test is `cc_safety::can_clear`, a *temperature* test, and `cc_safety` clears the latch
/// earlier in the same control period. On the recovery tick the latch is therefore already false
/// while the machine is still in `EMERGENCY_STOP`, so a latch-guarded drain would skip exactly
/// the tick that matters.
///
/// This test pins that: the request is set **after** the latch has been cleared and before the
/// tick runs. Removing the unguarded drain makes it fail.
#[test]
fn a_request_set_on_the_recovery_tick_is_still_drained() {
    let mut h = latched_machine_with_a_brew_pressed_during_the_latch();
    for _ in 0..3 {
        h.elapse(10);
    }

    // The latch clears -- which is what `cc_safety` does earlier in the period,
    // before `update` runs.
    h.machine.safety.clear();
    assert!(!h.machine.is_emergency_stop(), "the latch is clear");

    // The request arrives in the window between that and the update.
    h.press(SwitchId::Brew);

    // The tick that also leaves the state.
    h.elapse(10);

    assert!(
        !h.machine.requests.brew_start,
        "the update must drain unguarded: by the time it runs the latch is \
         already false, so a latch-guarded drain skips the one tick that matters"
    );
    assert_ne!(h.state(), MachineState::BrewRunning);
}

/// The drain covers every action request, not just brew. `clear_all` spares
/// exactly one flag, and that carve-out is ADR-0003's "never drain wake-up
/// signals" rule in its actual scope.
#[test]
fn every_action_request_is_drained_except_the_standby_wake_signal() {
    for switch in [
        SwitchId::Brew,
        SwitchId::Steam,
        SwitchId::HotWater,
        SwitchId::Power,
    ] {
        let mut h = latched_machine_with_a_brew_pressed_during_the_latch();
        h.press(switch);
        h.elapse(10);
        assert!(
            !h.machine.requests.any(),
            "{switch:?} must be drained by the latch"
        );
    }

    // `standby` is the one flag `clear_all` preserves, deliberately: it asks the
    // machine to go somewhere rather than start something. ADR-0003's carve-out
    // is scoped to `STANDBY`, but the survival of this flag is a fact about
    // `clear_all` and is pinned here rather than left to be rediscovered.
    let mut h = latched_machine_with_a_brew_pressed_during_the_latch();
    h.machine.requests.standby = true;
    h.elapse(10);
    assert!(
        h.machine.requests.standby,
        "`clear_all` spares the standby request; if that changes, ADR-0003 and \\
         this test both have to be revisited"
    );
}

/// Where recovery lands. A bare `assert_ne!(state, EmergencyStop)` would also pass on a
/// regression to `Standby`, or on a state that never leaves at all.
#[test]
fn recovery_lands_in_init_and_not_in_a_brew_state() {
    let mut h = latched_machine_with_a_brew_pressed_during_the_latch();
    for _ in 0..3 {
        h.elapse(10);
    }
    h.machine.safety.clear();
    h.elapse(10);

    assert_eq!(
        h.state(),
        MachineState::Init,
        "EMERGENCY_STOP returns to INIT, which re-reads the runtime PID flag, and \
         not straight to PID_NORMAL"
    );
    assert!(!h.machine.requests.brew_start);
}

/// The error states drain as well: a request that survives one starts a real brew when the
/// machine recovers (`AG-REPO-24`).
#[test]
fn a_brew_pressed_during_a_sensor_error_is_not_acted_on_after_it_recovers() {
    use cc_machine::{timing, Sensors};

    fn probe_faulted() -> Sensors {
        Sensors {
            has_temperature_error: true,
            ..Sensors::healthy()
        }
    }

    let mut h = common::Harness::in_state(MachineState::PidNormal);
    h.press(SwitchId::Brew);
    // The probe faults in the same period the request is outstanding.
    let _ = h.send(cc_machine::Event::SensorUpdated(probe_faulted()));
    h.elapse(10);
    assert_eq!(h.state(), MachineState::SensorError);

    // The fault clears and the recovery delay elapses.
    let _ = h.send(cc_machine::Event::SensorUpdated(Sensors::healthy()));
    h.elapse(timing::ERROR_RECOVERY_DELAY_MS + 20);

    assert!(
        !h.machine.requests.brew_start,
        "SENSOR_ERROR must drain like the emergency latch does, or the request \
         fires when the machine recovers"
    );
}
