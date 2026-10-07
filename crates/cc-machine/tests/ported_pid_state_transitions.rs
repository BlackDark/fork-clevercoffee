#![allow(
    clippy::assert_is_empty,
    reason = "the assertion below is on a collection, so the suggested \
              assert_eq!(x, \"\") does not typecheck"
)]

//! Port of `test/test_pid_state_transitions` — 8 C++ cases, 13 Rust cases
//! (8 live + 5 `#[ignore]`d mock records) plus 5 the C++ does not have.
//!
//! ## What this suite is actually about
//!
//! `test/test_pid_state_transitions/test_main.cpp` does **not** link the real
//! state classes. It defines a `MockMachineStateContext` and two hand-written
//! mock states — `BuggyPidNormalState` (no `!isPidEnabled()` check) and
//! `FixedPidNormalState` (with one) — and asserts that the fixed one transitions
//! and the buggy one does not (`:47-85`). It is a regression test for a fix that
//! has already been made, written against stand-ins.
//!
//! The fix it pins is real and is present in the C++: `PidNormalState::checkSpecificTransitions`
//! opens with `if (!context.isPidRuntimeEnabled()) return PID_DISABLED;`
//! (`PidStates.cpp:48-51`), which is *required* because the global guard
//! deliberately excludes `PID_NORMAL` (`BaseState.h:163-166`) so that the state
//! can recover. Remove that check and the machine stays in `PID_NORMAL` with the
//! PID off — the bug the test's own `BugLeadsToUnresponsiveSystem` case
//! describes in prose.
//!
//! So the port pins the **real** behaviour rather than the mock's, using the
//! reducer. The mock cases become `#[ignore]`d for the record, as in
//! `ported_state_machine.rs`.
//!
//! ## Case mapping
//!
//! | C++ case (`test_pid_state_transitions/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `BuggyStateStuckWhenPidDisabled` (:128) | [`pid_normal_stuck_when_the_pid_is_disabled`] (the *absence* of the fix, demonstrated) |
//! | `FixedStateTransitionsWhenPidDisabled` (:148) | [`pid_normal_transitions_to_pid_disabled`] |
//! | `PidNormalStaysWhenEnabled` (:167) | [`pid_normal_stays_put_when_the_pid_is_enabled`] |
//! | `RoundTripTransition` (:178) | [`round_trip_transition`] |
//! | `PidDisabledCheckHasPriority` (:198) | [`the_pid_disabled_check_has_priority_over_brew_start`] |
//! | `MultipleRapidStateChanges` (:212) | [`multiple_rapid_state_changes`] |
//! | `StateIdsCorrect` (:239) | [`the_state_ids_are_the_cpp_discriminants`] |
//! | `BugLeadsToUnresponsiveSystem` (:275) | [`the_stuck_state_would_leave_the_pid_off`] |

mod common;

use cc_domain::state::MachineState;
use cc_machine::{Event, Machine, Request};
use common::Harness;

/// A latched emergency is not needed here; this suite is about the PID flag.
fn disable_pid(h: &mut Harness) {
    let _ = h.send(Event::Command(cc_machine::Command::SetUserPidEnabled(
        false,
    )));
}

fn enable_pid(h: &mut Harness) {
    let _ = h.send(Event::Command(cc_machine::Command::SetUserPidEnabled(true)));
}

// ---------------------------------------------------------------------------
// The mock cases
// ---------------------------------------------------------------------------

#[test]
#[ignore = "C++ mock-state case; the real state is tested by the tests below"]
fn buggy_state_stuck_when_pid_disabled_is_a_mock_artefact() {}

#[test]
#[ignore = "C++ mock-state case; the real state is tested by the tests below"]
fn fixed_state_transitions_when_pid_disabled_is_a_mock_artefact() {}

#[test]
#[ignore = "C++ mock-state case; the real state is tested by the tests below"]
fn round_trip_transition_is_a_mock_artefact() {}

#[test]
#[ignore = "C++ mock-state case; the real state is tested by the tests below"]
fn multiple_rapid_state_changes_is_a_mock_artefact() {}

#[test]
#[ignore = "C++ mock-state case; the real state is tested by the tests below"]
fn the_stuck_state_would_leave_the_pid_off_is_a_mock_artefact() {}

/// The bug the C++ suite documents, reproduced against the real state machine so
/// the *fix* is what is pinned rather than the mock.
///
/// With the check at `PidStates.cpp:48-51` removed, `PID_NORMAL` would fall
/// through to the brew-start arm and the machine would stay put with the PID off.
/// Asserting the counterfactual is not possible without deleting the arm, so the
/// test asserts the positive: the PID check wins, and it wins *first*.
#[test]
fn the_pid_disabled_check_wins_in_pid_normal() {
    let mut h = Harness::new();
    disable_pid(&mut h);
    let fx = h.tick();
    assert_eq!(
        h.state(),
        MachineState::PidDisabled,
        "with the check present the machine leaves PID_NORMAL: {fx:?}"
    );
}

/// `FixedStateTransitionsWhenPidDisabled` (:148-162), against the real state.
#[test]
fn pid_normal_transitions_to_pid_disabled() {
    let mut h = Harness::new();
    assert_eq!(h.state(), MachineState::PidNormal);
    disable_pid(&mut h);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidDisabled, "{fx:?}");
}

/// `PidNormalStaysWhenEnabled` (:167-173).
#[test]
fn pid_normal_stays_put_when_the_pid_is_enabled() {
    let mut h = Harness::new();
    enable_pid(&mut h);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
    assert!(
        !common::has(&fx, cc_machine::Effect::ExitState(MachineState::PidNormal)),
        "no transition means no exit: {fx:?}"
    );
}

/// `RoundTripTransition` (:178-193).
#[test]
fn round_trip_transition() {
    let mut h = Harness::new();
    disable_pid(&mut h);
    assert!(!h.tick().is_empty());
    assert_eq!(h.state(), MachineState::PidDisabled);
    enable_pid(&mut h);
    h.tick();
    assert_eq!(h.state(), MachineState::PidNormal);
}

/// `PidDisabledCheckHasPriority` (:198-207).
///
/// Both the PID-disabled condition and a brew start are true. The C++ asserts
/// `PID_DISABLED` "should take priority over brew start". The reason it does is
/// positional: the check is the first arm of
/// `PidNormalState::checkSpecificTransitions` (`PidStates.cpp:48-51`), before the
/// brew-start arm at `:53`.
#[test]
fn the_pid_disabled_check_has_priority_over_brew_start() {
    let mut h = Harness::new();
    disable_pid(&mut h);
    h.machine.requests.set(Request::BrewStart, true);
    let fx = h.tick();
    assert_eq!(
        h.state(),
        MachineState::PidDisabled,
        "the PID check must take priority over brew start: {fx:?}"
    );
    // The brew request survives, because it was never the arm that ran. It is
    // then drained by `PID_DISABLED`'s entry (`PidStates.cpp:119`).
    let win = common::transition_window(&fx, MachineState::PidNormal, MachineState::PidDisabled);
    assert!(
        common::has(&win, cc_machine::Effect::ClearActionRequests),
        "{win:?}"
    );
    assert!(!h.requested(Request::BrewStart));
}

/// `MultipleRapidStateChanges` (:212-234) — four toggles, four loops.
#[test]
fn multiple_rapid_state_changes() {
    let mut h = Harness::new();
    for round in 0..2 {
        disable_pid(&mut h);
        h.tick();
        assert_eq!(h.state(), MachineState::PidDisabled, "round {round}");
        enable_pid(&mut h);
        h.tick();
        assert_eq!(h.state(), MachineState::PidNormal, "round {round}");
    }
}

/// `StateIdsCorrect` (:239-242) — the C++ asserts its own mock enum's values; the
/// real ones are pinned here.
#[test]
fn the_state_ids_are_the_cpp_discriminants() {
    assert_eq!(MachineState::PidNormal.id(), 20);
    assert_eq!(MachineState::PidDisabled.id(), 90);
}

// ---------------------------------------------------------------------------
// S11: stale flag draining, which is what the coverage map assigns this suite
// ---------------------------------------------------------------------------

/// `PID_DISABLED` drains **on entry** and again in `update` while the PID stays
/// off (`PidStates.cpp:112-127`).
#[test]
fn pid_disabled_drains_on_entry_and_while_still_off() {
    let mut h = Harness::new();
    h.machine.requests.set(Request::BrewStart, true);
    h.machine.requests.set(Request::SteamStart, true);
    h.machine.requests.set(Request::BackflushCycleStart, true);

    let fx = h.on_entry(MachineState::PidDisabled);
    assert!(
        common::has(&fx, cc_machine::Effect::ClearActionRequests),
        "{fx:?}"
    );
    assert!(!h.requested(Request::BrewStart));
    assert!(!h.requested(Request::SteamStart));
    assert!(!h.requested(Request::BackflushCycleStart));
    assert!(!h.machine.requests.any());

    // A request that arrives afterwards is drained by the next `update`.
    let _ = h.send(Event::Command(cc_machine::Command::BrewStart));
    assert!(h.requested(Request::BrewStart));
    let fx = h.update(MachineState::PidDisabled);
    assert!(
        common::has(&fx, cc_machine::Effect::ClearActionRequests),
        "{fx:?}"
    );
    assert!(!h.requested(Request::BrewStart));
}

/// `PID_DISABLED` entry also stops the pump and closes the valve
/// (`PidStates.cpp:115-117`).
#[test]
fn pid_disabled_entry_stops_the_pump_and_closes_the_valve() {
    let mut h = Harness::new();
    let fx = h.on_entry(MachineState::PidDisabled);
    assert!(common::has(&fx, cc_machine::Effect::DisablePump), "{fx:?}");
    assert!(
        common::has(&fx, cc_machine::Effect::CloseWaterValve),
        "{fx:?}"
    );
    assert!(
        common::has(&fx, cc_machine::Effect::SetPidRuntime { enabled: false }),
        "{fx:?}"
    );
}

/// The error states DO drain, and this test previously asserted the opposite
/// on a rationale that turned out to be false.
///
/// It read: *"the drain happens on entry to the recovery state instead"* — but
/// `PidNormal`'s `on_entry` is empty (`states.rs:138`), so no such drain exists.
/// The comment it cited (`ErrorStates.cpp:52-54`) is about the machine *staying*
/// in `SENSOR_ERROR` so a persistent fault stays visible; it says nothing about
/// request draining. The port had carried a false mechanism as a pin.
///
/// With the bring-up inhibit deleted the difference is physical: a brew pressed
/// while the machine was in `SENSOR_ERROR` used to end at `enablePump REFUSED`,
/// and now starts a real brew when the fault clears and the recovery delay
/// elapses. `AG-REPO-24` names the error states for exactly this reason.
///
/// **Divergence from the C++**, recorded in `docs/history/divergences.md` §37.
#[test]
fn the_error_states_drain_requests() {
    for state in [MachineState::SensorError, MachineState::EepromError] {
        let mut h = Harness::in_state(state);
        h.machine.requests.set(Request::BrewStart, true);
        let fx = h.on_entry(state);
        assert_eq!(
            common::count(&fx, cc_machine::Effect::ClearActionRequests),
            1,
            "{state:?} must drain on entry: it cannot act on the request, and \
             nothing between it and PID_NORMAL does either"
        );
        assert!(
            !h.requested(Request::BrewStart),
            "{state:?}: the request must not survive into the recovery state"
        );
    }
}

/// `STANDBY` drains the stops and keeps the starts (ADR-0003's "never drain
/// wake-up signals").
#[test]
fn standby_drains_the_stops_and_keeps_the_starts() {
    let mut h = Harness::new();
    h.machine.requests.set(Request::BrewStop, true);
    h.machine.requests.set(Request::SteamStop, true);
    h.machine.requests.set(Request::ManualFlushStop, true);
    h.machine.requests.set(Request::BackflushStop, true);
    h.machine.requests.set(Request::BrewStart, true);
    h.machine.requests.set(Request::SteamStart, true);

    let fx = h.on_entry(MachineState::Standby);
    assert!(
        common::has(&fx, cc_machine::Effect::ClearStaleStopRequests),
        "{fx:?}"
    );
    assert_eq!(
        common::count(&fx, cc_machine::Effect::ClearActionRequests),
        0
    );
    assert!(!h.requested(Request::BrewStop));
    assert!(!h.requested(Request::SteamStop));
    assert!(!h.requested(Request::ManualFlushStop));
    assert!(!h.requested(Request::BackflushStop));
    assert!(
        h.requested(Request::BrewStart),
        "a wake trigger must survive"
    );
    assert!(
        h.requested(Request::SteamStart),
        "a wake trigger must survive"
    );
}

/// A guard-induced PID disable drains the request that might otherwise have
/// started a brew, so recovery does not fire a stale shot.
#[test]
fn a_guard_induced_pid_disable_drains_the_pending_brew() {
    let mut h = Harness::new();
    h.machine.requests.set(Request::BrewStart, true);
    // Disable the runtime PID behind the state machine's back — the shape a
    // `POST /api/pid` takes (`MQTTManager.cpp:340`).
    h.machine.pid.runtime_enabled = false;
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidDisabled, "{fx:?}");
    assert!(!h.requested(Request::BrewStart));
}

/// A machine that has not been initialised must not act, and in particular must
/// not act on a request (`StateMachine.cpp:72-75`).
#[test]
fn an_uninitialised_machine_ignores_requests() {
    let cold = Machine::cold();
    let owner = Harness::new();
    let ctx = owner.ctx();
    let (next, fx) =
        cc_machine::reduce(&cold, &ctx, Event::Command(cc_machine::Command::BrewStart));
    assert_eq!(next, cold);
    assert!(fx.is_empty());
}
