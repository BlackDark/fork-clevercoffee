//! Pre-infusion pause holds the valve open without ever pulsing the pump.
//!
//! Assert the whole effect sequence, not merely the final output state.

mod common;

use cc_domain::state::MachineState;
use cc_machine::Effect;
use common::Harness;

#[test]
fn pause_entry_stops_pump_and_opens_valve() {
    let mut h = Harness::in_state(MachineState::BrewPreinfusionPause);
    let fx = h.on_entry(MachineState::BrewPreinfusionPause);

    assert!(
        common::count(&fx, Effect::DisablePump) >= 1,
        "entry must stop the pump: {fx:?}"
    );
    assert!(
        common::count(&fx, Effect::OpenWaterValve) >= 1,
        "entry must hold the water valve open: {fx:?}"
    );
    assert_eq!(
        common::count(&fx, Effect::EnablePump),
        0,
        "the pump must never be turned on during the pause: {fx:?}"
    );
}

#[test]
fn pause_update_keeps_pump_off_and_valve_open() {
    let mut h = Harness::in_state(MachineState::BrewPreinfusionPause);
    let fx = h.update(MachineState::BrewPreinfusionPause);

    assert!(
        common::count(&fx, Effect::DisablePump) >= 1,
        "update must keep the pump off: {fx:?}"
    );
    assert!(
        common::count(&fx, Effect::OpenWaterValve) >= 1,
        "update must re-assert the open valve: {fx:?}"
    );
    assert_eq!(common::count(&fx, Effect::EnablePump), 0, "{fx:?}");

    // And through a whole control-loop iteration, which is the real shape: the update runs, then
    // the valve check runs, and the pause is on the whitelist so the valve survives.
    //
    // The configuration has to be automatic-with-pre-infusion: with the shipped default
    // (`brew.mode` manual) the pause state's own "manual mode, skip the pause" arm fires
    // immediately, which is correct but is not what this case is about.
    h.config.brew.mode = cc_domain::process::BrewMode::Automatic;
    h.config.brew.pre_infusion.enabled = true;
    h.config.brew.pre_infusion.time = 3.0;
    h.config.brew.pre_infusion.pause = 2.0;
    let fx = h.tick();
    assert_eq!(
        common::count(&fx, Effect::EnablePump),
        0,
        "a whole tick in the pause must never enable the pump: {fx:?}"
    );
    assert_eq!(
        common::count(&fx, Effect::CloseWaterValve),
        0,
        "S5 must not close the valve during a pre-infusion pause: {fx:?}"
    );
    assert!(common::has(&fx, Effect::DisablePump), "{fx:?}");
    assert!(common::has(&fx, Effect::OpenWaterValve), "{fx:?}");
}

// ---------------------------------------------------------------------------
// What the suite also checks
// ---------------------------------------------------------------------------

/// The pause's exit stops the pump but **not** the valve.
///
/// That partial exit is the S5 dependency: on the abort path the exit leaves the valve open and
/// the valve fail-safe is what closes it, later in the same tick.
#[test]
fn pause_exit_stops_the_pump_but_not_the_valve() {
    let mut h = Harness::in_state(MachineState::BrewPreinfusionPause);
    let fx = h.on_exit(MachineState::BrewPreinfusionPause);
    assert!(common::has(&fx, Effect::DisablePump), "{fx:?}");
    assert_eq!(
        common::count(&fx, Effect::CloseWaterValve),
        0,
        "the pause exit must not close the valve: {fx:?}"
    );
}

/// The pause timer: the configured pre-infusion time plus the time spent in this state.
#[test]
fn the_pause_timer_is_preinfusion_plus_time_in_this_state() {
    let mut h = Harness::in_state(MachineState::BrewPreinfusionPause);
    h.config.brew.pre_infusion.enabled = true;
    h.config.brew.pre_infusion.time = 3.0;
    h.config.brew.pre_infusion.pause = 2.0;

    let _ = h.on_entry(MachineState::BrewPreinfusionPause);
    common::assert_ms(h.machine.brew.elapsed_ms, 3_000.0);

    h.advance_clock(1_000);
    let _ = h.update(MachineState::BrewPreinfusionPause);
    common::assert_ms(h.machine.brew.elapsed_ms, 4_000.0);
}

/// The pause ends after `brew.pre_infusion.pause` seconds -- the comparison is `>=`, not `>`,
/// so a pause of exactly the configured length has ended.
#[test]
fn the_pause_ends_at_the_configured_pause_time() {
    let mut h = Harness::in_state(MachineState::BrewPreinfusionPause);
    h.config.brew.mode = cc_domain::process::BrewMode::Automatic;
    h.config.brew.pre_infusion.enabled = true;
    h.config.brew.pre_infusion.time = 3.0;
    h.config.brew.pre_infusion.pause = 2.0;

    let _ = h.elapse(1_999);
    assert_eq!(h.state(), MachineState::BrewPreinfusionPause);

    let _ = h.elapse(1);
    assert_eq!(h.state(), MachineState::BrewRunning, "2.000 s elapsed");
}

/// A brew stop during the pause goes back to the PID state rather than on to `BREW_RUNNING`.
#[test]
fn a_brew_stop_during_the_pause_goes_to_the_pid_state() {
    let mut h = Harness::in_state(MachineState::BrewPreinfusionPause);
    h.config.pid.enabled = true;
    h.machine.requests.set(cc_machine::Request::BrewStop, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
}
