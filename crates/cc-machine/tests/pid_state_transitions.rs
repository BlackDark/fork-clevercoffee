#![allow(
    clippy::assert_is_empty,
    reason = "the assertion below is on a collection, so the suggested \
              assert_eq!(x, \"\") does not typecheck"
)]

//! The PID flag, and what happens to action requests while it is off.
//!
//! `PID_NORMAL` is the machine's home state, and the global guard deliberately excludes it so
//! that the machine can recover. That makes the state's own rules the only place a disabled PID
//! is noticed while the machine is in it. The PID check is the first arm of those rules, so it
//! wins over a brew-start request in the same tick:
//! [`the_pid_disabled_check_wins_in_pid_normal`] and
//! [`the_pid_disabled_check_has_priority_over_brew_start`] pin both halves.
//!
//! The rest of the file is S11, the drain of stale action requests: `PID_DISABLED` drains on
//! entry and again while the PID stays off, the two error states drain on entry, and `STANDBY`
//! drains the stops while keeping the starts as wake triggers.

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

/// The check that keeps the machine recoverable, asserted from the positive side.
///
/// Without the PID-disabled arm, the machine would fall through to the brew-start arm and stay in
/// `PID_NORMAL` with the PID off and nothing left to bring it back. The counterfactual cannot be
/// demonstrated without deleting the arm, so this pins what the arm does: it fires, and it fires
/// before anything else.
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

/// A disabled PID moves the machine out of `PID_NORMAL`.
#[test]
fn pid_normal_transitions_to_pid_disabled() {
    let mut h = Harness::new();
    assert_eq!(h.state(), MachineState::PidNormal);
    disable_pid(&mut h);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidDisabled, "{fx:?}");
}

/// With the PID enabled and nothing requested, the machine stays where it is.
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

/// The machine can go to `PID_DISABLED` and come back.
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

/// The PID-disabled check takes priority over a brew start.
///
/// Both conditions are true in the same tick, and the order of the arms decides: the PID check
/// comes first. The brew request survives that tick -- it was never the arm that ran -- and the
/// entry it causes is what drains it, so it cannot fire a shot while the PID is off.
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
    // The brew request survives, because it was never the arm that ran; the entry it caused is
    // what drains it.
    let win = common::transition_window(&fx, MachineState::PidNormal, MachineState::PidDisabled);
    assert!(
        common::has(&win, cc_machine::Effect::ClearActionRequests),
        "{win:?}"
    );
    assert!(!h.requested(Request::BrewStart));
}

/// Four toggles in four ticks, with nothing left behind between them.
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

/// The state ids are pinned: they are part of what is written to logs and to the API, so a
/// renumber would be a breaking change rather than a refactor.
#[test]
fn the_state_ids_are_pinned() {
    assert_eq!(MachineState::PidNormal.id(), 20);
    assert_eq!(MachineState::PidDisabled.id(), 90);
}

// ---------------------------------------------------------------------------
// S11: stale flag draining, which is what the coverage map assigns this suite
// ---------------------------------------------------------------------------

/// `PID_DISABLED` drains **on entry** and again in `update` while the PID stays off, so a
/// request that arrives during the off period cannot fire when the PID comes back.
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

/// Entry also stops the pump, closes the valve and reports the runtime PID off.
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

/// The two error states drain on entry, which is a deliberate divergence.
///
/// They cannot act on a request, and nothing between them and `PID_NORMAL` can either: that
/// state's entry emits no effects. Without the drain, a brew pressed while the machine is in an
/// error state starts a real shot once the fault clears and the recovery delay elapses.
/// `AG-REPO-24` names the error states for exactly this reason.
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

/// `STANDBY` drains the stops and keeps the starts. A start flag is an intentional wake trigger:
/// it has to outlive the state the machine is asleep in, or the press that woke it is lost.
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
    // Disable the runtime PID behind the state machine's back, which is the shape a parameter
    // write takes: the control task pushes the new value in, and the reducer only reads it.
    h.machine.pid.runtime_enabled = false;
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidDisabled, "{fx:?}");
    assert!(!h.requested(Request::BrewStart));
}

/// An uninitialised machine ignores a request rather than acting on it: boot has not chosen a
/// state to act in.
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
