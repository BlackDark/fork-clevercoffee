//! Finding #15: a brew pressed during the emergency latch must not fire on
//! recovery.
//!
//! **Measured on a bench ESP32, 2026-10-07.** With a bench build
//! (`just bench-flash`, emergency threshold at 30 °C) the machine was tripped by
//! warming a DS18B20 by hand. A brew press during the latch was correctly
//! refused — `enablePump REFUSED — latched 1`, and the pump LED stayed dark. The
//! probe then cooled below the threshold, the latch cleared, **and the machine
//! started that same brew with nobody pressing anything.** On a real machine the
//! water is at brewing temperature and the reservoir is not empty.
//!
//! The fix is compliance with this repository's own rules rather than an
//! invention: `AG-REPO-24` ("states that cannot act on action requests must
//! drain them") and ADR-0003 rules 2 and 3 (drain on entry, and during update
//! guarded by the same condition as the exit). It is a divergence from the C++,
//! recorded in `docs/history/divergences.md` §36.
//!
//! The contrast is the point, and it is asserted at the bottom of this file: the
//! same press in `WATER_TANK_EMPTY` was already drained, which is what the
//! operator saw on the LEDs during R4-04 case 13.3.

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

/// The contrast with 13.3, which is what makes this a *fix* rather than a
/// different policy: `WATER_TANK_EMPTY` already drained the same press, and
/// that is what the operator watched happen at the LEDs.
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
