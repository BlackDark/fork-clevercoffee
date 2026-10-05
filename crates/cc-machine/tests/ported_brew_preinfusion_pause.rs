//! Port of `test/test_brew_preinfusion_pause` — 2 C++ cases, 2 Rust cases.
//!
//! `test_brew_preinfusion_pause` is the S5 suite in the coverage map: the pause
//! keeps the water valve **open** to hold puck pressure, and the pump off. Two
//! cases, both on `BrewPreinfusionPauseState`:
//!
//! | C++ case (`test_brew_preinfusion_pause/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `PauseEntryStopsPumpAndOpensValve` (:63) | [`pause_entry_stops_pump_and_opens_valve`] |
//! | `PauseUpdateKeepsPumpOffAndValveOpen` (:72) | [`pause_update_keeps_pump_off_and_valve_open`] |
//!
//! Both C++ cases assert `EXPECT_EQ(enablePumpCalls, 0)`, which is a *global*
//! count across the whole entry or update — so it says "the pump is never turned
//! on", which is stronger than "the pump ends up off" and is the assertion worth
//! keeping. The Rust asserts the same thing as "no `EnablePump` effect anywhere
//! in the sequence", and additionally checks that a whole tick in the pause state
//! produces the same verdict, which the C++ case does not.

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

    // And through a whole control-loop iteration, which is what actually
    // happens: `update` runs, then the S5 valve check runs, and the pause *is*
    // on the whitelist so the valve survives.
    //
    // The configuration has to be automatic-with-pre-infusion: with the shipped
    // default (`brew.mode = MANUAL_BREW`) the pause state's own
    // "manual mode - skipping pause" arm (`BrewStates.cpp:198-202`) fires
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
// What the C++ suite should also have checked
// ---------------------------------------------------------------------------

/// The pause's `onExit` stops the pump but **not** the valve
/// (`BrewStates.cpp:164-169`).
///
/// This is the S5 dependency: on the *abort* path the valve is closed by
/// `valveSafetyShutdownCheck`, not by the exit. `BrewStates.cpp:165-166` says so,
/// and the comment is only true because the exit is deliberately partial.
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

/// The pause timer: the state's own elapsed time, added to the configured
/// pre-infusion time (`BrewStates.cpp:177-182`).
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

/// The pause ends after `brew.pre_infusion.pause` seconds
/// (`BrewStates.cpp:205-213`) — strictly `>=`, because
/// `hasStateTimeoutElapsed` is `>=` (`MachineStateContext.cpp:454`).
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

/// A brew stop during the pause goes to the PID state, not to `BREW_RUNNING`
/// (`BaseState::checkBrewStopRequest`, `BaseState.h:103-111`).
#[test]
fn a_brew_stop_during_the_pause_goes_to_the_pid_state() {
    let mut h = Harness::in_state(MachineState::BrewPreinfusionPause);
    h.config.pid.enabled = true;
    h.machine.requests.set(cc_machine::Request::BrewStop, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
}
