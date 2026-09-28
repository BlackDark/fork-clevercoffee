//! Port of `test/test_backflush_states` — 4 C++ cases, 4 Rust cases.
//!
//! This suite is S5 in the coverage map: `BACKFLUSH_FILLING` and
//! `BACKFLUSH_FLUSHING` are on `cc_safety::water_flow_allowed`, so the S5 valve
//! check must not close their valves.
//!
//! ## Case mapping
//!
//! | C++ case (`test_backflush_states/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `DisableClearsCycleStartRequestFlag` (:81) | [`disable_clears_the_cycle_start_request_flag`] |
//! | `ModeDisabledMidFillTransitionsToPid` (:91) | [`mode_disabled_mid_fill_transitions_to_pid`] |
//! | `LastCycleTransitionsToFinished` (:106) | [`last_cycle_transitions_to_finished`] |
//! | `FinishedOnEntryResetsMaintenanceCounter` (:119) | [`finished_on_entry_resets_the_maintenance_counter`] |
//!
//! Plus the S5 case the suite is *assigned* to check and does not, and the
//! `BACKFLUSH_FILLING` re-assert gap reported in `parity_findings.rs`.

mod common;

use cc_domain::state::MachineState;
use cc_machine::{backflush, Command, Effect, Event, Request};
use common::Harness;

/// A machine in backflush mode with the given cycle count.
fn backflushing(state: MachineState, cycle: i32) -> Harness {
    let mut h = Harness::in_state(state);
    h.config.backflush.cycles = 5;
    h.config.backflush.fill_time = 5.0;
    h.config.backflush.flush_time = 10.0;
    h.machine.backflush.on = true;
    h.machine.backflush.cycle = cycle;
    h
}

#[test]
fn disable_clears_the_cycle_start_request_flag() {
    let mut h = Harness::new();
    h.config.backflush.cycles = 5;
    h.machine.backflush.on = false;
    h.machine.backflush.cycle = 1;

    let _ = h.send(Event::Command(Command::BackflushEnter));
    assert!(h.machine.backflush.on);
    assert!(
        h.requested(Request::BackflushEnter),
        "enable requests the entry"
    );

    // A cycle start arrives, then the mode is turned off.
    h.machine.requests.set(Request::BackflushCycleStart, true);
    let _ = h.send(Event::Command(Command::BackflushEnter)); // idempotent: already on
    assert!(h.requested(Request::BackflushCycleStart));

    // `applyBackflushMode(false)` is the disable; the command layer's `BackflushEnter`
    // is the *enable*, so the disable is driven the same way the C++ drives it —
    // through the mode-change resolution.
    let outcome = backflush::apply_backflush_mode(
        h.machine.backflush.on,
        h.machine.backflush.cycle,
        false,
        h.config.backflush.cycles,
        &h.machine.requests,
    );
    h.machine.backflush.on = outcome.on;
    h.machine.requests.backflush_enter = outcome.enter_requested;
    h.machine.requests.backflush_cycle_start = outcome.cycle_start_requested;
    h.machine.requests.backflush_stop = outcome.stop_requested;

    assert!(!h.requested(Request::BackflushCycleStart));
    assert!(!h.requested(Request::BackflushEnter));
}

#[test]
fn mode_disabled_mid_fill_transitions_to_pid() {
    let mut h = backflushing(MachineState::BackflushFilling, 1);

    // Turn the mode off while the fill is running.
    h.machine.backflush.on = false;

    let next = h.check_specific(MachineState::BackflushFilling);
    assert_eq!(
        next,
        Some(h.pid_state()),
        "the mode-disabled check comes first"
    );
    // And it also clears the three backflush request flags
    // (`BackflushStates.cpp:22-24`).
    assert!(!h.requested(Request::BackflushEnter));
    assert!(!h.requested(Request::BackflushCycleStart));
    assert!(!h.requested(Request::BackflushStop));
}

#[test]
fn last_cycle_transitions_to_finished() {
    let mut h = backflushing(MachineState::BackflushFlushing, 5);
    h.advance_clock(10_000);

    let next = h.check_specific(MachineState::BackflushFlushing);
    assert_eq!(next, Some(MachineState::BackflushFinished));
    // The counter is re-armed to 1 for the next run
    // (`BackflushStates.cpp:134`).
    assert_eq!(h.machine.backflush.cycle, 1);
}

#[test]
fn finished_on_entry_resets_the_maintenance_counter() {
    let mut h = backflushing(MachineState::BackflushFinished, 1);
    // Count a shot: 5000 ms is the minimum brew time
    // (`defaults.h:46`, `BACKFLUSH_REMINDER_MIN_BREW_TIME_MS`).
    h.machine.brew.elapsed_ms = 5_000.0;
    h.machine.shots_since_backflush = 7;
    assert!(h.machine.shots_since_backflush > 0);

    let fx = h.on_entry(MachineState::BackflushFinished);
    assert_eq!(h.machine.shots_since_backflush, 0);
    assert!(common::has(&fx, Effect::ResetShotsSinceBackflush), "{fx:?}");
}

// ---------------------------------------------------------------------------
// The S5 case this suite is assigned to check and does not
// ---------------------------------------------------------------------------

/// `BACKFLUSH_FILLING` and `BACKFLUSH_FLUSHING` are on the S5 whitelist, so a
/// full tick in either must not close the water valve.
///
/// This is the case the coverage map assigns `test_backflush_states` for, and the
/// C++ suite does not contain it: it only calls `checkTransitions` and
/// `onEntry`, never a loop.
#[test]
fn the_s5_valve_check_keeps_the_valve_open_during_a_backflush() {
    for state in [
        MachineState::BackflushFilling,
        MachineState::BackflushFlushing,
    ] {
        let mut h = backflushing(state, 1);
        h.config.standby.enabled = false;
        let fx = h.tick();
        assert_eq!(
            common::count(&fx, Effect::CloseWaterValve),
            0,
            "S5 must not close the valve in {state:?}: {fx:?}"
        );
    }
}

/// And the converse: `BACKFLUSH_IDLE` and `BACKFLUSH_FINISHED` are *not* on the
/// whitelist, so the same check closes the valve there.
#[test]
fn the_s5_valve_check_closes_the_valve_outside_the_backflush() {
    for state in [MachineState::BackflushIdle, MachineState::BackflushFinished] {
        let mut h = backflushing(state, 1);
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::CloseWaterValve),
            "S5 must close the valve in {state:?}: {fx:?}"
        );
    }
}

/// The backflush cycle, end to end: fill → flush ×5 → finished → idle.
#[test]
fn the_full_backflush_cycle_runs_the_configured_number_of_cycles() {
    let mut h = backflushing(MachineState::BackflushIdle, 1);
    h.config.pid.enabled = true;

    // Start the first cycle.
    h.machine.requests.set(Request::BackflushCycleStart, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::BackflushFilling, "{fx:?}");

    for cycle in 1..=5 {
        // Fill: 5 s.
        let _ = h.elapse(5_000);
        assert_eq!(h.state(), MachineState::BackflushFlushing, "cycle {cycle}");
        // Flush: 10 s.
        let _ = h.elapse(10_000);
        if cycle < 5 {
            assert_eq!(h.state(), MachineState::BackflushFilling, "cycle {cycle}");
            assert_eq!(h.machine.backflush.cycle, cycle + 1);
        } else {
            assert_eq!(h.state(), MachineState::BackflushFinished, "cycle {cycle}");
        }
    }

    // Finished shows for 3 s and then returns to idle.
    let _ = h.elapse(3_000);
    assert_eq!(h.state(), MachineState::BackflushIdle);
}

/// A backflush stop from any backflush state returns to `BACKFLUSH_IDLE`
/// (`BackflushStates.cpp:83-87`, `:118-122`, `:157-161`).
#[test]
fn a_backflush_stop_returns_to_idle_from_every_backflush_state() {
    for state in [
        MachineState::BackflushFilling,
        MachineState::BackflushFlushing,
        MachineState::BackflushFinished,
    ] {
        let mut h = backflushing(state, 1);
        h.machine.requests.set(Request::BackflushStop, true);
        let next = h.check_specific(state);
        assert_eq!(next, Some(MachineState::BackflushIdle), "from {state:?}");
        assert!(
            !h.requested(Request::BackflushStop),
            "the flag must be drained"
        );
    }
}

/// A short press in `BACKFLUSH_IDLE` starts a cycle; a long press starts a manual
/// flush instead (`BrewHandler.h:198-203`).
#[test]
fn a_short_press_in_idle_starts_a_cycle_and_a_long_press_starts_a_flush() {
    let mut h = backflushing(MachineState::BackflushIdle, 1);
    h.config.hardware.switches.brew.r#type = cc_domain::hardware::SwitchType::Momentary;

    let _ = h.send(Event::ButtonPressed {
        switch: cc_machine::SwitchId::Brew,
        long_press: false,
    });
    assert!(
        h.requested(Request::BackflushCycleStart),
        "a short press starts a cycle"
    );
    assert!(!h.requested(Request::ManualFlushStart));

    // Reset, then a long press.
    let mut h2 = backflushing(MachineState::BackflushIdle, 1);
    h2.config.hardware.switches.brew.r#type = cc_domain::hardware::SwitchType::Momentary;
    let _ = h2.send(Event::ButtonPressed {
        switch: cc_machine::SwitchId::Brew,
        long_press: true,
    });
    assert!(
        h2.requested(Request::ManualFlushStart),
        "a long press starts a manual flush"
    );
    assert!(!h2.requested(Request::BackflushCycleStart));
}

/// Manual flush is reachable from `BACKFLUSH_IDLE` and returns there on stop
/// (`BackflushStates.cpp:47-51` and `SystemStates.cpp:87-90`).
#[test]
fn manual_flush_runs_from_idle_and_returns_there() {
    let mut h = backflushing(MachineState::BackflushIdle, 1);
    h.machine.requests.set(Request::ManualFlushStart, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::ManualFlushRunning, "{fx:?}");

    h.machine.requests.set(Request::ManualFlushStop, true);
    let fx = h.tick();
    assert_eq!(
        h.state(),
        MachineState::BackflushIdle,
        "manual flush stop returns to BACKFLUSH_IDLE while the mode is on: {fx:?}"
    );
}
