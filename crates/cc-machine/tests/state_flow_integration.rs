//! End-to-end flows: a whole brew, a manual flush, an emergency, and a standby round trip,
//! checked at every step against the effects the hardware would see.
//!
//! # Driving the reducer rather than the states
//!
//! Each step is one `Event::Tick`, which is what the machine's control loop sends, so a test sees
//! the whole tick: the guard, the exit half, the entry half and the update, in that order. That is
//! stronger than checking one hook at a time, and it is why [`brew_abort_during_preinfusion`] can
//! assert that the valve is closed by the end of the tick rather than only that the exit hook left
//! it alone. Where a test is about a single hook, its comment says so.
//!
//! The steps walk one brew from `PID_NORMAL` through pre-infusion, pause, running and finished,
//! then the same machine through an emergency stop and its recovery.

mod common;

use cc_domain::process::BrewMode;
use cc_domain::state::MachineState;
use cc_domain::units::Millis;
use cc_machine::{Event, Machine, Request, Sensors, SwitchId};
use cc_safety::SafetyState;
use common::{automatic_brew_with_preinfusion, context_for, Harness};

/// A latched emergency, as `cc_safety::reduce` would report it after S1 trips.
fn emergency_latched() -> Event {
    Event::Safety(cc_safety::Outcome {
        state: SafetyState {
            last_sample_seq: None,
            latched: true,
            high_reading_count: cc_safety::DEBOUNCE_COUNT,
        },
        verdict: cc_safety::Verdict {
            may_heat: false,
            may_pump: false,
            may_open_water: false,
            may_open_steam: false,
            latched: true,
            reason: Some(cc_safety::Reason::Overtemp {
                consecutive: cc_safety::DEBOUNCE_COUNT,
                threshold: cc_domain::units::Celsius::new(150.0),
            }),
        },
    })
}

/// An emergency that has been cleared: valid reading at or below 100 °C
/// (`cc_safety::can_clear`).
fn emergency_cleared() -> Event {
    Event::Safety(cc_safety::Outcome {
        state: SafetyState::CLEAR,
        verdict: cc_safety::Verdict {
            may_heat: true,
            may_pump: true,
            may_open_water: true,
            may_open_steam: true,
            latched: false,
            reason: None,
        },
    })
}

// ---------------------------------------------------------------------------
// Flow 1: complete brew cycle
// ---------------------------------------------------------------------------

/// `ComplBrewCycle_FullFlow` (:98-178).
#[test]
fn completes_brew_cycle_full_flow() {
    let mut h = Harness {
        config: automatic_brew_with_preinfusion(),
        machine: Machine::cold(),
        now: 0,
    };
    h.machine = cc_machine::boot(Millis::new(0), &context_for(&h.config)).0;
    h.machine.state = MachineState::PidNormal;
    h.machine.pid.runtime_enabled = true;

    // --- Step 1: PID_NORMAL + brew start → BREW_PREINFUSION ----------------
    h.machine.requests.set(Request::BrewStart, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::BrewPreinfusion, "{fx:?}");

    // --- Step 2: entry enables the pump and opens the valve -----------------
    let entry =
        common::transition_window(&fx, MachineState::PidNormal, MachineState::BrewPreinfusion);
    assert!(
        common::has(&entry, cc_machine::Effect::EnablePump),
        "{entry:?}"
    );
    assert!(
        common::has(&entry, cc_machine::Effect::OpenWaterValve),
        "{entry:?}"
    );
    // And the target: 5000 ms of pre-infusion plus pause, plus the 25 s brew time, so the
    // display shows "0s/30s" from the first frame.
    common::assert_ms(h.machine.brew.target_ms, 30_000.0);
    common::assert_ms(h.machine.brew.elapsed_ms, 0.0);

    // --- Step 3: past the 3 s pre-infusion → BREW_PREINFUSION_PAUSE ---------
    let fx = h.elapse(3_100);
    assert_eq!(h.state(), MachineState::BrewPreinfusionPause, "{fx:?}");
    let win = common::transition_window(
        &fx,
        MachineState::BrewPreinfusion,
        MachineState::BrewPreinfusionPause,
    );
    // Step 4: leaving the pause disables the pump and leaves the valve open.
    assert!(
        common::has(&win, cc_machine::Effect::DisablePump),
        "{win:?}"
    );
    assert!(
        common::count(&win, cc_machine::Effect::CloseWaterValve) == 0,
        "preinfusion exit must not close the valve: {win:?}"
    );
    // Step 5: entry stops the pump and keeps the valve open.
    assert!(
        common::has(&win, cc_machine::Effect::DisablePump),
        "{win:?}"
    );
    assert!(
        common::has(&win, cc_machine::Effect::OpenWaterValve),
        "{win:?}"
    );
    assert_eq!(
        common::count(&win, cc_machine::Effect::EnablePump),
        0,
        "{win:?}"
    );

    // --- Step 4: past the 2 s pause → BREW_RUNNING -------------------------
    let fx = h.elapse(2_100);
    assert_eq!(h.state(), MachineState::BrewRunning, "{fx:?}");
    let win = common::transition_window(
        &fx,
        MachineState::BrewPreinfusionPause,
        MachineState::BrewRunning,
    );
    assert!(
        common::has(&win, cc_machine::Effect::DisablePump),
        "{win:?}"
    );
    assert_eq!(common::count(&win, cc_machine::Effect::CloseWaterValve), 0);
    assert!(common::has(&win, cc_machine::Effect::EnablePump), "{win:?}");
    assert!(
        common::has(&win, cc_machine::Effect::OpenWaterValve),
        "{win:?}"
    );

    // The timer is rebuilt from the base on entry, so entering BREW_RUNNING after 2.1 s of pause
    // shows 5.0 s -- the 3 s pre-infusion plus the 2 s pause -- and not 7.1 s. The running state's
    // timer is "base plus time in this state", not "total time since the brew started".
    common::assert_ms(h.machine.brew.elapsed_ms, 5_000.0);
    // One second later it is 6.0 s.
    let _ = h.elapse(1_000);
    common::assert_ms(h.machine.brew.elapsed_ms, 6_000.0);

    // --- Step 5: brew stop → BREW_FINISHED ----------------------------------
    h.machine.requests.set(Request::BrewStop, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::BrewFinished, "{fx:?}");
    let win = common::transition_window(&fx, MachineState::BrewRunning, MachineState::BrewFinished);
    // Step 10: leaving the running state disables the pump and closes the valve.
    assert!(
        common::has(&win, cc_machine::Effect::DisablePump),
        "{win:?}"
    );
    assert!(
        common::has(&win, cc_machine::Effect::CloseWaterValve),
        "{win:?}"
    );
    // Step 11: entering BREW_FINISHED changes neither the pump nor the valve.
    let entry_half = common::entry_half(&win, MachineState::BrewFinished);
    assert_eq!(
        common::count(&entry_half, cc_machine::Effect::EnablePump),
        0
    );
    // The shot is offered to the maintenance counter here, as an effect that carries the
    // **decision** rather than the three facts it came from: the qualification rule is applied
    // where the counter lives, so what the shell has to persist is the count. A brew by time is
    // 30 s, so it counts.
    assert!(
        common::has(&win, cc_machine::Effect::RecordBrew { counted: true }),
        "{win:?}"
    );
    assert_eq!(
        h.machine.shots_since_backflush, 1,
        "the reducer owns the counter; it must have moved before the effect \
         was applied"
    );

    // --- Step 6: past the 3 s finished timeout → PID_NORMAL ----------------
    let fx = h.elapse(3_100);
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
}

// ---------------------------------------------------------------------------
// Flow 2: brew abort
// ---------------------------------------------------------------------------

/// The pause's exit leaves the valve open, and the S5 check closes it later in the same tick.
///
/// Both halves are visible here because the test drives whole ticks: the exit half of the window
/// carries no close, and the tick as a whole does.
#[test]
fn brew_abort_during_preinfusion_pause() {
    let mut h = Harness {
        config: automatic_brew_with_preinfusion(),
        machine: Machine::cold(),
        now: 0,
    };
    h.machine = cc_machine::boot_in(
        MachineState::BrewPreinfusionPause,
        true,
        Millis::new(0),
        &context_for(&h.config),
    )
    .0;
    h.machine.requests.set(Request::BrewStop, true);

    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");

    // The exit half does not close the valve...
    let win = common::transition_window(
        &fx,
        MachineState::BrewPreinfusionPause,
        MachineState::PidNormal,
    );
    let exit_half = common::exit_half(&win, MachineState::PidNormal);
    assert!(
        common::count(&exit_half, cc_machine::Effect::CloseWaterValve) == 0,
        "the pause's exit must not close the valve: {exit_half:?}"
    );
    assert!(
        common::has(&exit_half, cc_machine::Effect::DisablePump),
        "{exit_half:?}"
    );

    // ...but the S5 check, which runs after the state machine in the same tick,
    // does. `PID_NORMAL` is not on the whitelist.
    assert!(
        common::has(&fx, cc_machine::Effect::CloseWaterValve),
        "the S5 valve check must close it later in the same tick: {fx:?}"
    );
    let check = common::index_of(&fx, cc_machine::Effect::CloseWaterValve).unwrap();
    assert!(
        check
            > common::index_of(&fx, cc_machine::Effect::EnterState(MachineState::PidNormal))
                .unwrap(),
        "the S5 check runs after the transition, not before: {fx:?}"
    );
}

/// `BrewAbortDuringPreinfusion` (:209-228) — the same abort one state earlier.
#[test]
fn brew_abort_during_preinfusion() {
    let mut h = Harness {
        config: automatic_brew_with_preinfusion(),
        machine: Machine::cold(),
        now: 0,
    };
    h.machine = cc_machine::boot_in(
        MachineState::BrewPreinfusion,
        true,
        Millis::new(0),
        &context_for(&h.config),
    )
    .0;
    h.machine.requests.set(Request::BrewStop, true);

    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
    let win =
        common::transition_window(&fx, MachineState::BrewPreinfusion, MachineState::PidNormal);
    let exit_half = common::exit_half(&win, MachineState::PidNormal);
    assert!(
        common::has(&exit_half, cc_machine::Effect::DisablePump),
        "{exit_half:?}"
    );
    assert_eq!(
        common::count(&exit_half, cc_machine::Effect::CloseWaterValve),
        0,
        "preinfusion exit does not close the valve either: {exit_half:?}"
    );
    // And the S5 check closes it, because PID_NORMAL is not a water-flow state.
    assert!(
        common::has(&fx, cc_machine::Effect::CloseWaterValve),
        "{fx:?}"
    );
}

// ---------------------------------------------------------------------------
// Flow 3: PID toggle cycle
// ---------------------------------------------------------------------------

/// `PidToggleCycle_BrewWorksAfterReEnable` (:235-273).
#[test]
fn pid_toggle_cycle_brew_works_after_re_enable() {
    let mut h = Harness {
        config: automatic_brew_with_preinfusion(),
        machine: Machine::cold(),
        now: 0,
    };
    h.machine = cc_machine::boot_in(
        MachineState::PidNormal,
        true,
        Millis::new(0),
        &context_for(&h.config),
    )
    .0;

    // --- Step 1: user disables PID → PID_DISABLED --------------------------
    let _ = h.send(Event::Command(cc_machine::Command::SetUserPidEnabled(
        false,
    )));
    assert!(!h.machine.pid.runtime_enabled);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidDisabled, "{fx:?}");
    let win = common::transition_window(&fx, MachineState::PidNormal, MachineState::PidDisabled);
    // Step 2: entering PID_DISABLED disables the pump and closes the valve.
    assert!(
        common::has(&win, cc_machine::Effect::DisablePump),
        "{win:?}"
    );
    assert!(
        common::has(&win, cc_machine::Effect::CloseWaterValve),
        "{win:?}"
    );
    assert!(
        common::has(&win, cc_machine::Effect::ClearActionRequests),
        "{win:?}"
    );

    // --- Step 2: re-enable PID → PID_NORMAL -------------------------------
    let _ = h.send(Event::Command(cc_machine::Command::SetUserPidEnabled(true)));
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");

    // --- Step 3: brew start still works, and the pump still turns on -------
    h.machine.requests.set(Request::BrewStart, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::BrewPreinfusion, "{fx:?}");
    let win =
        common::transition_window(&fx, MachineState::PidNormal, MachineState::BrewPreinfusion);
    assert!(
        common::has(&win, cc_machine::Effect::EnablePump),
        "Regression: pump must enable after a PID toggle cycle: {win:?}"
    );
    assert!(
        common::has(&win, cc_machine::Effect::OpenWaterValve),
        "Regression: valve must open after a PID toggle cycle: {win:?}"
    );
}

// ---------------------------------------------------------------------------
// Flow 4: emergency
// ---------------------------------------------------------------------------

/// `EmergencyDuringBrew_HardwareCleanedUp` (:280-301).
#[test]
fn emergency_during_brew_hardware_cleaned_up() {
    let mut h = Harness {
        config: automatic_brew_with_preinfusion(),
        machine: Machine::cold(),
        now: 0,
    };
    h.machine = cc_machine::boot_in(
        MachineState::BrewRunning,
        true,
        Millis::new(0),
        &context_for(&h.config),
    )
    .0;

    let fx = h.send_all([
        emergency_latched(),
        Event::Tick {
            now: Millis::new(0),
        },
    ]);
    assert_eq!(h.state(), MachineState::EmergencyStop, "{fx:?}");

    let win =
        common::transition_window(&fx, MachineState::BrewRunning, MachineState::EmergencyStop);
    assert!(
        common::has(&win, cc_machine::Effect::DisablePump),
        "{win:?}"
    );
    assert!(
        common::has(&win, cc_machine::Effect::CloseWaterValve),
        "{win:?}"
    );
    // And the new state's entry latches the actuator facade.
    assert!(
        common::has(&win, cc_machine::Effect::EmergencyShutdown),
        "{win:?}"
    );
}

/// `EmergencyDuringPreinfusion_Detected` (:303-313).
#[test]
fn emergency_during_preinfusion_detected() {
    let mut h = Harness::new();
    h.enter_state_without_effects(MachineState::BrewPreinfusion);
    let fx = h.send(emergency_latched());
    assert_eq!(
        h.next_state_this_tick(),
        Some(MachineState::EmergencyStop),
        "{fx:?}"
    );
}

/// Recovery after a transient overheat restores the runtime PID from the configuration.
///
/// Without that, the machine comes back with the PID off and never heats again -- it is stuck in
/// `PID_DISABLED` unless someone re-enables it by hand.
#[test]
fn emergency_recovery_restores_pid_from_config() {
    let mut h = Harness::new();
    h.config.pid.enabled = true;
    h.machine.state = MachineState::EmergencyStop;
    h.machine.safety.latched = true;

    // Entry forces the runtime PID off.
    let _ = h.on_entry(MachineState::EmergencyStop);
    assert!(
        !h.machine.pid.runtime_enabled,
        "emergency entry must force the runtime PID off"
    );

    // Temperature back in the safe range → the recovery verdict.
    let _ = h.send_all([
        Event::SensorUpdated(Sensors::healthy()),
        emergency_cleared(),
    ]);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::Init, "{fx:?}");

    // The exit half restores the runtime PID from config.
    let win = common::transition_window(&fx, MachineState::EmergencyStop, MachineState::Init);
    assert!(
        common::has(&win, cc_machine::Effect::SetPidRuntime { enabled: true }),
        "{win:?}"
    );
    assert!(h.machine.pid.runtime_enabled);

    // And INIT then routes to PID_NORMAL, not PID_DISABLED.
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
}

/// `EmergencyRecovery_RespectsConfigPidDisabled` (:347-356).
#[test]
fn emergency_recovery_respects_config_pid_disabled() {
    let mut h = Harness::new();
    h.config.pid.enabled = false;
    h.machine.state = MachineState::EmergencyStop;
    h.machine.safety.latched = true;

    let _ = h.on_entry(MachineState::EmergencyStop);
    let _ = h.send(emergency_cleared());
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::Init, "{fx:?}");
    assert!(
        !h.machine.pid.runtime_enabled,
        "config remains the source of truth: recovery must not re-enable a PID \
         the user turned off"
    );

    // And INIT routes to PID_DISABLED, because INIT reads the *runtime* flag, which recovery
    // leaves off.
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidDisabled, "{fx:?}");
}

// ---------------------------------------------------------------------------
// Flow 5: manual flush
// ---------------------------------------------------------------------------

/// `ManualFlushFlow_PumpAndValveControlled` (:363-391).
#[test]
fn manual_flush_flow_pump_and_valve_controlled() {
    let mut h = Harness::new();

    h.machine.requests.set(Request::ManualFlushStart, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::ManualFlushRunning, "{fx:?}");
    let win = common::transition_window(
        &fx,
        MachineState::PidNormal,
        MachineState::ManualFlushRunning,
    );
    assert!(common::has(&win, cc_machine::Effect::EnablePump), "{win:?}");
    assert!(
        common::has(&win, cc_machine::Effect::OpenWaterValve),
        "{win:?}"
    );

    h.machine.requests.set(Request::ManualFlushStop, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
    let win = common::transition_window(
        &fx,
        MachineState::ManualFlushRunning,
        MachineState::PidNormal,
    );
    assert!(
        common::has(&win, cc_machine::Effect::DisablePump),
        "{win:?}"
    );
    assert!(
        common::has(&win, cc_machine::Effect::CloseWaterValve),
        "{win:?}"
    );
}

/// `ManualFlush_UpdateKeepsPumpAndValveActive` (:393-404).
#[test]
fn manual_flush_update_keeps_pump_and_valve_active() {
    let mut h = Harness::in_state(MachineState::ManualFlushRunning);
    let fx = h.update(MachineState::ManualFlushRunning);
    assert!(common::has(&fx, cc_machine::Effect::EnablePump), "{fx:?}");
    assert!(
        common::has(&fx, cc_machine::Effect::OpenWaterValve),
        "{fx:?}"
    );

    // And through a whole tick, not just the `update` hook.
    let fx = h.tick();
    assert!(common::has(&fx, cc_machine::Effect::EnablePump), "{fx:?}");
    assert!(
        common::has(&fx, cc_machine::Effect::OpenWaterValve),
        "{fx:?}"
    );
    // The S5 whitelist includes MANUAL_FLUSH_RUNNING, so the valve survives.
    assert_eq!(
        common::count(&fx, cc_machine::Effect::CloseWaterValve),
        0,
        "the valve must stay open during a manual flush: {fx:?}"
    );
}

// ---------------------------------------------------------------------------
// Flow 6: standby and wake
// ---------------------------------------------------------------------------

/// `StandbyAndWake_PidDisabledAndRestored` (:411-438).
#[test]
fn standby_and_wake_pid_disabled_and_restored() {
    let mut h = Harness::new();
    h.config.pid.enabled = true;

    h.machine.requests.set(Request::Standby, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::Standby, "{fx:?}");
    let win = common::transition_window(&fx, MachineState::PidNormal, MachineState::Standby);
    assert!(
        common::has(&win, cc_machine::Effect::DisablePump),
        "{win:?}"
    );
    assert!(
        common::has(&win, cc_machine::Effect::CloseWaterValve),
        "{win:?}"
    );
    assert!(
        common::has(&win, cc_machine::Effect::SetPidRuntime { enabled: false }),
        "{win:?}"
    );
    assert!(!h.machine.pid.runtime_enabled);

    h.machine.requests.set(Request::NormalOperation, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
    assert!(h.machine.pid.runtime_enabled);
    assert!(common::has(&fx, cc_machine::Effect::WakeDisplay), "{fx:?}");
    assert!(
        common::has(&fx, cc_machine::Effect::ResetMqttReconnectCount),
        "{fx:?}"
    );
}

/// `StandbyWakeOnBrewRequest` (:440-451).
#[test]
fn standby_wake_on_brew_request() {
    let mut h = Harness::in_state(MachineState::Standby);
    h.machine.pid.runtime_enabled = false;
    h.machine.requests.set(Request::BrewStart, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
}

// ---------------------------------------------------------------------------
// Additional edge cases
// ---------------------------------------------------------------------------

/// `BrewRunningOnExit_AlwaysClosesValve` (:457-471).
#[test]
fn brew_running_on_exit_always_closes_valve() {
    let mut h = Harness::in_state(MachineState::BrewRunning);
    let fx = h.on_exit(MachineState::BrewRunning);
    assert!(
        common::has(&fx, cc_machine::Effect::DisablePump),
        "Safety: pump must be disabled on brew exit: {fx:?}"
    );
    assert!(
        common::has(&fx, cc_machine::Effect::CloseWaterValve),
        "Safety: valve must be closed on brew exit: {fx:?}"
    );
}

/// `BrewFinishedOnExit_AlsoClosesValve` (:473-485).
#[test]
fn brew_finished_on_exit_also_closes_valve() {
    let mut h = Harness::in_state(MachineState::BrewFinished);
    let fx = h.on_exit(MachineState::BrewFinished);
    assert!(common::has(&fx, cc_machine::Effect::DisablePump), "{fx:?}");
    assert!(
        common::has(&fx, cc_machine::Effect::CloseWaterValve),
        "{fx:?}"
    );
}

/// `PidNormalToBrewManualMode_SkipsPreinfusion` (:487-497).
#[test]
fn pid_normal_to_brew_manual_mode_skips_preinfusion() {
    let mut h = Harness::new();
    h.config.brew.mode = BrewMode::Manual;
    h.machine.requests.set(Request::BrewStart, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::BrewRunning, "{fx:?}");
    // And the brew timer starts at zero: a manual shot has no pre-infusion phase to count.
    common::assert_ms(h.machine.brew.elapsed_ms, 0.0);
}

/// `PidDisabledDrainsActionRequests` (:499-513).
#[test]
fn pid_disabled_drains_action_requests() {
    let mut h = Harness::new();
    let _ = h.send(Event::Command(cc_machine::Command::SetUserPidEnabled(
        false,
    )));
    let _ = h.on_entry(MachineState::PidDisabled);
    assert!(!h.requested(Request::BrewStart));

    // Requests arrive while the machine is already PID-disabled.
    let _ = h.send_all([
        Event::Command(cc_machine::Command::BrewStart),
        Event::Command(cc_machine::Command::ManualFlushStart),
    ]);
    assert!(h.requested(Request::BrewStart));

    // The update drain clears them, because the PID is still off.
    let _ = h.update(MachineState::PidDisabled);
    assert!(
        !h.requested(Request::BrewStart),
        "brew start must be drained"
    );
    assert!(
        !h.requested(Request::ManualFlushStart),
        "manual flush start must be drained"
    );
}

/// `PID_DISABLED`'s update skips the drain once the PID is back on, so the request that brought
/// the PID back is not eaten.
#[test]
fn pid_disabled_stops_draining_once_pid_is_re_enabled() {
    let mut h = Harness::in_state(MachineState::PidDisabled);
    h.machine.pid.runtime_enabled = true;
    h.machine.requests.set(Request::BrewStart, true);

    let fx = h.update(MachineState::PidDisabled);
    assert_eq!(
        common::count(&fx, cc_machine::Effect::ClearActionRequests),
        0
    );
    assert!(h.requested(Request::BrewStart));
}

/// `BrewPreinfusionToRunning_WhenPreinfusionDisabled` (:515-527).
#[test]
fn brew_preinfusion_to_running_when_preinfusion_disabled() {
    let mut h = Harness::new();
    h.config.brew.mode = BrewMode::Automatic;
    h.config.brew.pre_infusion.enabled = false;
    h.enter_state_without_effects(MachineState::BrewPreinfusion);

    let fx = h.tick();
    assert_eq!(h.state(), MachineState::BrewRunning, "{fx:?}");
}

/// `BrewPreinfusionToRunning_WhenPauseIsZero` (:529-546).
#[test]
fn brew_preinfusion_to_running_when_pause_is_zero() {
    let mut h = Harness::new();
    h.config.brew.mode = BrewMode::Automatic;
    h.config.brew.pre_infusion.enabled = true;
    h.config.brew.pre_infusion.time = 2.0;
    h.config.brew.pre_infusion.pause = 0.0;
    h.enter_state_without_effects(MachineState::BrewPreinfusion);

    let fx = h.elapse(2_100);
    assert_eq!(
        h.state(),
        MachineState::BrewRunning,
        "a zero pause skips BREW_PREINFUSION_PAUSE entirely: {fx:?}"
    );
}

/// `EmergencyDuringManualFlush_DetectedByBaseState` (:548-566).
#[test]
fn emergency_during_manual_flush_detected_by_base_state() {
    let mut h = Harness::in_state(MachineState::ManualFlushRunning);
    let _ = h.send(emergency_latched());
    assert_eq!(
        h.next_state_this_tick(),
        Some(MachineState::EmergencyStop),
        "the emergency guard wins over everything, including manual flush"
    );

    let fx = h.on_exit(MachineState::ManualFlushRunning);
    assert!(common::has(&fx, cc_machine::Effect::DisablePump), "{fx:?}");
    assert!(
        common::has(&fx, cc_machine::Effect::CloseWaterValve),
        "{fx:?}"
    );
}

/// `PidDisabledDuringBrew_ForcesTransition` (:568-587).
#[test]
fn pid_disabled_during_brew_forces_transition() {
    let mut h = Harness {
        config: automatic_brew_with_preinfusion(),
        machine: Machine::cold(),
        now: 0,
    };
    h.machine = cc_machine::boot_in(
        MachineState::BrewRunning,
        true,
        Millis::new(0),
        &context_for(&h.config),
    )
    .0;

    let _ = h.send(Event::Command(cc_machine::Command::SetUserPidEnabled(
        false,
    )));
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidDisabled, "{fx:?}");

    let win = common::transition_window(&fx, MachineState::BrewRunning, MachineState::PidDisabled);
    assert!(
        common::has(&win, cc_machine::Effect::DisablePump),
        "{win:?}"
    );
    assert!(
        common::has(&win, cc_machine::Effect::CloseWaterValve),
        "{win:?}"
    );
}

/// The switch-driven path: a brew switch press sets the flag, and the tick that follows consumes
/// it.
#[test]
fn a_brew_switch_press_starts_a_brew_in_the_same_loop() {
    let mut h = Harness {
        config: automatic_brew_with_preinfusion(),
        machine: Machine::cold(),
        now: 0,
    };
    h.machine = cc_machine::boot_in(
        MachineState::PidNormal,
        true,
        Millis::new(0),
        &context_for(&h.config),
    )
    .0;
    h.config.hardware.switches.brew.r#type = cc_domain::hardware::SwitchType::Momentary;

    let press = h.press(SwitchId::Brew);
    assert!(h.requested(Request::BrewStart), "{press:?}");
    assert_eq!(
        h.state(),
        MachineState::PidNormal,
        "the press alone does not transition; the tick does"
    );
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::BrewPreinfusion, "{fx:?}");
}
