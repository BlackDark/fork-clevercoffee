//! The eighteen states: what each one does on entry, on exit, on update, and
//! when it wants to transition.
//!
//! Four functions, one per hook, each a `match` over [`MachineState`] with **no
//! wildcard arm**. That is the mechanism: adding a 19th state fails to compile
//! here, in [`crate::guards`], and in `cc_safety::water_flow_allowed` until
//! someone has decided what its entry, exit, update and transitions are. The C++
//! gets the same protection from the `BaseState` template plus a factory
//! `switch`; here it is four `match`es and the compiler.
//!
//! # The ADR-0003 contract, in three lines per state
//!
//! | | ADR-0003 | Where |
//! | --- | --- | --- |
//! | energise | "Enable hardware in `onEntryImpl()`" | [`on_entry`] |
//! | reinforce | "Reinforce hardware state in `update()`" | [`update`] |
//! | release | "Disable hardware in `onExitImpl()`" | [`on_exit`] |
//!
//! # The two states that break the pattern, and why
//!
//! Both are **preserved deliberately** and both are reported:
//!
//! * **`BACKFLUSH_FILLING` does not re-assert in `update`.**
//!   `BackflushFillingState::update` (`BackflushStates.cpp:71-76`) only logs,
//!   so it never calls `enablePump()`/`openWaterValve()`. It *is* on S5's
//!   water-flow whitelist, so `valveSafetyShutdownCheck` does not close its
//!   valve, and the pump is only blocked by the tank interlock — which means
//!   the pump survives an arbitrary number of loops in exactly the one
//!   backflush state where nothing re-asserts it. `test_backflush_states` does
//!   not check `update`, which is why nobody noticed.
//! * **`SENSOR_ERROR` and `EEPROM_ERROR` have no `onExit` hardware release.**
//!   They never energise anything, so there is nothing to release — and they
//!   do drain nothing, because `PidDisabledState`'s drain (`PidStates.cpp:119`)
//!   is what protects the recovery, and the recovery states are
//!   `PID_NORMAL`/`PID_DISABLED`, which drain on *their* entry.

use cc_domain::state::MachineState;

use crate::context::Context;
use crate::effect::{Effect, Effects};
use crate::machine::Machine;
use crate::timing;

/// `MachineStateContext::getPidState()` (`MachineStateContext.cpp:313-315`).
///
/// ```cpp
/// return isPidConfigEnabled() ? PID_NORMAL : PID_DISABLED;
/// ```
///
/// Note this reads the **config**, not the runtime flag. It is the C++'s
/// "where do I go when I am done" answer, and it is used by every state that
/// ends an operation: brew stop, steam stop, sensor-error recovery, tank refill,
/// backflush-disabled, standby exit. The two differ exactly after an emergency
/// stop, which is why `EmergencyStopState::onExitImpl` restores the runtime flag
/// from config before anyone asks.
#[must_use]
pub fn pid_state(ctx: &Context<'_>) -> MachineState {
    if ctx.pid_config_enabled() {
        MachineState::PidNormal
    } else {
        MachineState::PidDisabled
    }
}

/// `BaseState::checkBrewStopRequest` (`BaseState.h:103-111`).
///
/// ```cpp
/// std::optional<MachineStateId> checkBrewStopRequest(MachineStateContext& context) {
///     if (context.isBrewStopRequested()) {
///         context.setBrewStopRequested(false);
///         const MachineStateId pidState = context.getPidState();
///         context.logStateTransition(getStateId(), pidState, "Brew stop requested");
///         return pidState;
///     }
///     return std::nullopt;
/// }
/// ```
///
/// Returns `None` when no stop is pending; otherwise clears the flag and
/// reports the destination. The flag is cleared **here**, not on the transition,
/// so a second brew-stop request arriving in the same tick is dropped rather
/// than queued.
fn check_brew_stop_request(machine: &mut Machine, ctx: &Context<'_>) -> Option<MachineState> {
    if !machine.requests.brew_stop {
        return None;
    }
    machine.requests.brew_stop = false;
    Some(pid_state(ctx))
}

/// `checkBackflushModeDisabled` (`BackflushStates.cpp:18-28`).
///
/// ```cpp
/// if (context.isBackflushModeActive()) return std::nullopt;
/// context.setBackflushEnterRequested(false);
/// context.setBackflushCycleStartRequested(false);
/// context.setBackflushStopRequested(false);
/// return context.getPidState();
/// ```
///
/// Every backflush state calls this **first**, so turning backflush mode off
/// overrides whatever the switch was doing — including a stop request, which is
/// why `test_backflush_states`' `ModeDisabledMidFillTransitionsToPid` does not
/// need to clear the stop flag first.
fn check_backflush_mode_disabled(machine: &mut Machine, ctx: &Context<'_>) -> Option<MachineState> {
    if machine.is_backflush_mode_active() {
        return None;
    }
    machine.requests.backflush_enter = false;
    machine.requests.backflush_cycle_start = false;
    machine.requests.backflush_stop = false;
    Some(pid_state(ctx))
}

/// What a state does when it is entered: `onEntryImpl` plus the base class's
/// entry bookkeeping.
///
/// The caller has already set [`Machine::entry_at`]
/// (`StateMachine.cpp:141-148` swaps, stamps the time, then enters), so
/// [`state_elapsed_ms`](Machine::state_elapsed_ms) is 0 here.
#[allow(clippy::match_same_arms)]
// Justification: the eighteen arms are kept separate on purpose. Several
// states genuinely do the same thing, but each cites a different C++
// `onEntryImpl` / `onExitImpl` / `update`, and a future change to one state
// must not silently change another. Merging them would make the C++
// provenance unreviewable and the ADR-0003 table untestable per row.
#[must_use]
pub fn on_entry(state: MachineState, machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    let mut fx = Effects::new();
    match state {
        // `InitState::onEntryImpl` (`InitState.cpp:12-14`) logs only.
        MachineState::Init => {}

        // `PidNormalState::onEntryImpl` (`PidStates.cpp:16-19`): "Don't reset
        // standby timer on entry - only reset on user activity". Deliberately
        // does not re-arm the countdown, so time spent in a brew counts toward
        // standby.
        MachineState::PidNormal => {}

        // `PidDisabledState::onEntryImpl` (`PidStates.cpp:112-120`).
        MachineState::PidDisabled => {
            machine.pid.runtime_enabled = false;
            fx.push(Effect::SetPidRuntime { enabled: false });
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
            // S11: drain every flag, so a request that arrived before the
            // machine went PID-disabled cannot fire the moment it comes back.
            machine.requests.clear_all();
            fx.push(Effect::ClearActionRequests);
        }

        // `BrewPreinfusionState::onEntryImpl` (`BrewStates.cpp:67-79`).
        MachineState::BrewPreinfusion => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
            machine.brew.elapsed_ms = 0.0;
            // `initTotalTargetBrewTime` (`BrewStates.cpp:53-62`): the target is
            // set from the very start so the display shows "0s/30s" rather than
            // "0s" and then jumping.
            machine.brew.target_ms = ctx.total_target_brew_ms();
        }

        // `BrewPreinfusionPauseState::onEntryImpl` (`BrewStates.cpp:152-162`).
        MachineState::BrewPreinfusionPause => {
            // Pump off, **valve deliberately open** to hold puck pressure.
            fx.push(Effect::DisablePump);
            fx.push(Effect::OpenWaterValve);
            machine.brew.elapsed_ms = preinfusion_ms(ctx);
        }

        // `BrewRunningState::onEntryImpl` (`BrewStates.cpp:218-235`).
        MachineState::BrewRunning => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
            // "In automatic mode, include preinfusion + pause time; in manual
            // mode, start from 0" — which is why a manual shot's timer starts at
            // zero and an automatic one does not.
            machine.brew.elapsed_ms = if ctx.brew_is_automatic() {
                ctx.preinfusion_base_ms()
            } else {
                0.0
            };
            machine.brew.target_ms = ctx.total_target_brew_ms();
        }

        // `BrewFinishedState::onEntryImpl` (`BrewStates.cpp:306-312`).
        MachineState::BrewFinished => {
            // The C++ makes one call here —
            // `maintenanceCoordinator().recordBrewIfQualified(processCurrentBrewTime(), getCurrentBrewWeight(), hardwareSensorsScaleEnabled.get())`
            // — and that call applies the rule, increments the counter and
            // writes NVS. The rule and the increment happen here, at the same
            // point and with the same three arguments; only the write is the
            // shell's, because only the shell has the store.
            //
            // Doing the decision here rather than in the applier is the fix for
            // the counter never moving: it is evaluated where the value it
            // changes lives, so there is nothing to wire up and nothing to
            // forget. The effect below carries the answer.
            let scale_enabled = ctx.config.hardware.sensors.scale.enabled;
            let (elapsed_ms, weight) = (machine.brew.elapsed_ms, machine.brew_weight());
            let counted = crate::maintenance::record_brew_if_qualified(
                &mut machine.shots_since_backflush,
                elapsed_ms,
                weight,
                scale_enabled,
            );
            fx.push(Effect::RecordBrew { counted });
        }

        // `ManualFlushRunningState::onEntryImpl` (`SystemStates.cpp:58-63`).
        MachineState::ManualFlushRunning => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
        }

        // `SteamRunningState::onEntryImpl` (`SteamStates.cpp:14-17`).
        MachineState::SteamRunning => {
            machine.steam_mode = true;
            machine.steam_first_on = true;
            fx.push(Effect::SetSteamMode { enabled: true });
        }

        // `BackflushState::onEntryImpl` (`BackflushStates.cpp:33-36`).
        MachineState::BackflushIdle => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        // `BackflushFillingState::onEntryImpl` (`BackflushStates.cpp:61-65`).
        MachineState::BackflushFilling => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
        }

        // `BackflushFlushingState::onEntryImpl` (`BackflushStates.cpp:97-101`).
        MachineState::BackflushFlushing => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        // `BackflushFinishedState::onEntryImpl` (`BackflushStates.cpp:142-146`).
        MachineState::BackflushFinished => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
            machine.shots_since_backflush = 0;
            fx.push(Effect::ResetShotsSinceBackflush);
        }

        // `WaterTankEmptyState::onEntryImpl` (`ErrorStates.cpp:58-60`): a log
        // line. No hardware, deliberately — see the note in this module's docs
        // about `keep_heater_on_empty`.
        MachineState::WaterTankEmpty => {}

        // `EmergencyStopState::onEntryImpl` (`EmergencyStopState.cpp:15-17`).
        MachineState::EmergencyStop => {
            fx.push(Effect::EmergencyShutdown);
            machine.pid.runtime_enabled = false;
            fx.push(Effect::SetPidRuntime { enabled: false });
        }

        // `StandbyState::onEntryImpl` (`SystemStates.cpp:14-25`).
        MachineState::Standby => {
            machine.steam_mode = false;
            machine.steam_first_on = false;
            fx.push(Effect::SetSteamMode { enabled: false });
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
            machine.pid.runtime_enabled = false;
            fx.push(Effect::SetPidRuntime { enabled: false });
            // "Drain stale stop flags (preserve start flags as wake triggers)" —
            // ADR-0003's "never drain wake-up signals".
            machine.requests.clear_stale_stops();
            fx.push(Effect::ClearStaleStopRequests);
        }

        // `SensorErrorState::onEntryImpl` (`ErrorStates.cpp:13-17`).
        MachineState::SensorError => {
            // `errorStartTime_ = millis()`. The C++ gets a fresh value for free
            // because the transition constructs a new object
            // (`StateFactory.cpp:24`); here entry is the only writer.
            machine.error_since = Some(machine.now);
        }

        // `EepromErrorState::onEntryImpl` (`ErrorStates.cpp:94-100`).
        MachineState::EepromError => {
            machine.pid.runtime_enabled = false;
            fx.push(Effect::SetPidRuntime { enabled: false });
            machine.error_since = Some(machine.now);
        }
    }
    fx
}

/// What a state does when it is left: `onExitImpl` plus the base class's exit
/// bookkeeping.
///
/// ADR-0003: "**Every state that activates pump or valve must deactivate them in
/// `onExitImpl`** — the next state's `onEntry` may not run if an error
/// interrupts the transition."
#[allow(clippy::match_same_arms)]
// Justification: the eighteen arms are kept separate on purpose. Several
// states genuinely do the same thing, but each cites a different C++
// `onEntryImpl` / `onExitImpl` / `update`, and a future change to one state
// must not silently change another. Merging them would make the C++
// provenance unreviewable and the ADR-0003 table untestable per row.
#[must_use]
pub fn on_exit(state: MachineState, machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    let mut fx = Effects::new();
    match state {
        // No `onExitImpl` (`InitState.h:16-21`).
        MachineState::Init => {}

        // `PidNormalState::onExitImpl` (`PidStates.cpp:21-25`).
        //
        // Pump only, no valve: `PID_NORMAL` never opens the water valve itself,
        // but `PidNormalState::update` runs the pump for hot-water dispensing
        // (`PidStates.cpp:33-43`), so leaving this state must stop the pump.
        MachineState::PidNormal => {
            fx.push(Effect::DisablePump);
        }

        // No `onExitImpl` (`PidStates.h`).
        MachineState::PidDisabled => {}

        // `BrewPreinfusionState::onExitImpl` (`BrewStates.cpp:81-86`).
        //
        // "Pump off only — valve stays open to hold puck pressure through
        // pause/running. Valve closure happens in BREW_RUNNING::onExitImpl or
        // via valveSafetyShutdownCheck on abort."
        //
        // On the *abort* path (BREW_PREINFUSION → PID_NORMAL) neither applies:
        // `PID_NORMAL`'s exit is not run, so the valve stays open until
        // `valveSafetyShutdownCheck` closes it, which is a different function
        // later in the same tick. `test_state_flow_integration`'s
        // `BrewAbortDuringPreinfusion` pins exactly this.
        MachineState::BrewPreinfusion => {
            fx.push(Effect::DisablePump);
        }

        // `BrewPreinfusionPauseState::onExitImpl` (`BrewStates.cpp:164-169`).
        MachineState::BrewPreinfusionPause => {
            fx.push(Effect::DisablePump);
        }

        // `BrewRunningState::onExitImpl` (`BrewStates.cpp:237-242`).
        MachineState::BrewRunning => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        // `BrewFinishedState::onExitImpl` (`BrewStates.cpp:314-319`).
        MachineState::BrewFinished => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        // `ManualFlushRunningState::onExitImpl` (`SystemStates.cpp:65-70`).
        MachineState::ManualFlushRunning => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        // `SteamRunningState::onExitImpl` (`SteamStates.cpp:19-25`).
        MachineState::SteamRunning => {
            machine.steam_mode = false;
            machine.steam_first_on = false;
            fx.push(Effect::SetSteamMode { enabled: false });
            // "Safety: Disable water injection pump when exiting steam mode" —
            // STEAM_RUNNING runs the pump for water injection
            // (`SteamStates.cpp:36-46`).
            fx.push(Effect::DisablePump);
        }

        // No `onExitImpl` — `BackflushState` has none (`BackflushStates.h:9-14`).
        MachineState::BackflushIdle => {}

        // `BackflushFillingState::onExitImpl` (`BackflushStates.cpp:67-69`).
        MachineState::BackflushFilling => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        // `BackflushFlushingState::onExitImpl` (`BackflushStates.cpp:102-104`).
        MachineState::BackflushFlushing => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        // No `onExitImpl` (`BackflushStates.h:30-35`).
        MachineState::BackflushFinished => {}

        // No `onExitImpl` (`ErrorStates.h:22-28`).
        MachineState::WaterTankEmpty => {}

        // `EmergencyStopState::onExitImpl` (`EmergencyStopState.cpp:24-31`).
        MachineState::EmergencyStop => {
            // "Emergency entry forced runtime PID off (performEmergencyShutdown).
            // Restore it from config so recovery resumes heating instead of
            // leaving the machine stuck in PID_DISABLED ('disabled manually')."
            let enabled = ctx.pid_config_enabled();
            machine.pid.runtime_enabled = enabled;
            fx.push(Effect::SetPidRuntime { enabled });
        }

        // `StandbyState::onExitImpl` (`SystemStates.cpp:27-34`).
        MachineState::Standby => {
            // `exitStandbyMode()`'s only real effect: display power save off.
            fx.push(Effect::WakeDisplay);
            let enabled = ctx.pid_config_enabled();
            machine.pid.runtime_enabled = enabled;
            fx.push(Effect::SetPidRuntime { enabled });
        }

        // `SensorErrorState::onExitImpl` (`ErrorStates.cpp:19-22`).
        MachineState::SensorError => {
            machine.error_since = None;
        }

        // `EepromErrorState::onExitImpl` (`ErrorStates.cpp:102-105`).
        MachineState::EepromError => {
            machine.error_since = None;
        }
    }
    fx
}

/// What a state does on every loop: `update()`.
///
/// ADR-0003: "Reinforce hardware state in `update()` (defense against safety
/// checks toggling it off)". This is the second half of the C++'s defence in
/// depth: `valveSafetyShutdownCheck` runs after this
/// (`LoopManager.cpp:616-619`) and closes the valve unless the state is
/// whitelist-gated, so a state that asserts "open" and a safety check that
/// asserts "closed" in the same tick is *resolved by the whitelist*, not by
/// which call came last.
///
/// The two states that energise hardware and do **not** re-assert it are
/// `BACKFLUSH_FILLING` and `STEAM_RUNNING`. `STEAM_RUNNING` is fine — the steam
/// valve is not the machine's to assert. `BACKFLUSH_FILLING` is not: its
/// `update` (`BackflushStates.cpp:71-76`) only logs. Preserved; see the module
/// docs.
#[allow(clippy::match_same_arms)]
// Justification: the eighteen arms are kept separate on purpose. Several
// states genuinely do the same thing, but each cites a different C++
// `onEntryImpl` / `onExitImpl` / `update`, and a future change to one state
// must not silently change another. Merging them would make the C++
// provenance unreviewable and the ADR-0003 table untestable per row.
#[must_use]
pub fn update(state: MachineState, machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    let mut fx = Effects::new();
    match state {
        // `InitState::update` (`InitState.cpp:16-22`): a debug log.
        MachineState::Init => {}

        // `PidNormalState::update` (`PidStates.cpp:27-44`).
        //
        // Hot-water dispensing has no state of its own: the water switch drives
        // the pump directly while the machine is in PID_NORMAL. The comment in
        // the C++ says so explicitly.
        MachineState::PidNormal => {
            if machine.switches.hot_water {
                fx.push(Effect::EnablePump);
            } else {
                fx.push(Effect::DisablePump);
            }
        }

        // `PidDisabledState::update` (`PidStates.cpp:122-133`).
        //
        // "Only drain stale action flags if PID remains disabled. If PID was
        // just re-enabled, skip draining so PID_NORMAL can process pending
        // requests." The conditional matters: an unconditional drain would eat
        // the very request that is about to bring the PID back.
        MachineState::PidDisabled => {
            if !machine.is_pid_runtime_enabled() {
                machine.requests.clear_all();
                fx.push(Effect::ClearActionRequests);
            }
        }

        // `BrewPreinfusionState::update` (`BrewStates.cpp:88-104`).
        MachineState::BrewPreinfusion => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
            machine.brew.elapsed_ms = f64::from(machine.state_elapsed_ms());
        }

        // `BrewPreinfusionPauseState::update` (`BrewStates.cpp:171-189`).
        MachineState::BrewPreinfusionPause => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::OpenWaterValve);
            machine.brew.elapsed_ms = preinfusion_ms(ctx) + f64::from(machine.state_elapsed_ms());
        }

        // `BrewRunningState::update` (`BrewStates.cpp:244-266`).
        MachineState::BrewRunning => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
            let base = if ctx.brew_is_automatic() {
                ctx.preinfusion_base_ms()
            } else {
                0.0
            };
            machine.brew.elapsed_ms = base + f64::from(machine.state_elapsed_ms());
        }

        // `BrewFinishedState::update` (`BrewStates.cpp:321-326`): a log.
        MachineState::BrewFinished => {}

        // `ManualFlushRunningState::update` (`SystemStates.cpp:72-81`).
        MachineState::ManualFlushRunning => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
        }

        // `SteamRunningState::update` (`SteamStates.cpp:27-47`).
        //
        // The same water switch, repurposed: in STEAM_RUNNING it injects water
        // into the boiler. `test_steam_water_injection` is the oracle.
        MachineState::SteamRunning => {
            if machine.switches.hot_water {
                fx.push(Effect::EnablePump);
            } else {
                fx.push(Effect::DisablePump);
            }
        }

        // `BackflushState::update` (`BackflushStates.cpp:38-40`): a log.
        MachineState::BackflushIdle => {}

        // `BackflushFillingState::update` (`BackflushStates.cpp:71-76`): a log.
        //
        // **Does not re-assert the pump or the valve** — see the module docs.
        MachineState::BackflushFilling => {}

        // `BackflushFlushingState::update` (`BackflushStates.cpp:106-111`): a
        // log.
        MachineState::BackflushFlushing => {}

        // `BackflushFinishedState::update` (`BackflushStates.cpp:148-150`): a
        // log.
        MachineState::BackflushFinished => {}

        // `WaterTankEmptyState::update` (`ErrorStates.cpp:62-67`): a log.
        //
        // No heater release, deliberately: with
        // `hardware.sensors.watertank.keep_heater_on_empty` the heater is
        // *supposed* to keep running, which is why this state's standby timeout
        // has to keep working (`ErrorStates.cpp:83-85`).
        MachineState::WaterTankEmpty => {}

        // `EmergencyStopState::update` (`EmergencyStopState.cpp:33-39`).
        //
        // Re-runs the full emergency shutdown **every loop**, not just on
        // entry. This is S2's enforcement: the latch lives in the actuator
        // facade, and something could re-energise a relay between loops.
        MachineState::EmergencyStop => {
            fx.push(Effect::EmergencyShutdown);
            machine.pid.runtime_enabled = false;
            fx.push(Effect::SetPidRuntime { enabled: false });
        }

        // `StandbyState::update` (`SystemStates.cpp:36-41`): a log. The heater
        // is off because the PID gate (`should_pid_be_enabled`) excludes
        // STANDBY, not because anything switches it off here.
        MachineState::Standby => {}

        // `SensorErrorState::update` (`ErrorStates.cpp:24-30`): a log.
        MachineState::SensorError => {}

        // `EepromErrorState::update` (`ErrorStates.cpp:107-109`): a log.
        MachineState::EepromError => {}
    }

    let _ = ctx;
    fx
}

/// What a state wants to transition to: `checkSpecificTransitions`.
///
/// Reached only when no global guard fired — see
/// [`crate::guards::global_guard`]. In the C++ the same ordering lives in
/// `BaseState::checkTransitions` (`BaseState.h:137-174`), which returns the
/// guard's answer before delegating here.
///
/// The return type is `Option<MachineState>`: `None` means "stay put", which is
/// the C++'s `std::nullopt`. Note that a `Some` equal to the current state is
/// still a self-transition, and the caller discards it
/// (`StateMachine.cpp:107-111`).
#[allow(clippy::too_many_lines)]
// Justification: this is a literal transcription of eighteen
// `checkSpecificTransitions` methods, and the eighteen arms are the
// specification. Splitting them across private helpers would hide the order in
// which each state tests its conditions — and that order decides which flag
// wins, which is exactly what the C++ suites check.
#[must_use]
pub fn check_specific(
    state: MachineState,
    machine: &mut Machine,
    ctx: &Context<'_>,
) -> Option<MachineState> {
    match state {
        // `InitState::checkSpecificTransitions` (`InitState.cpp:24-32`).
        MachineState::Init => {
            // Note `checkPidConfig` returns `isPidRuntimeEnabled()`
            // (`InitState.cpp:45-47`) — the *runtime* flag, not the config
            // preference, despite the name. This is what makes INIT the
            // recovery state for an emergency stop
            // (`EmergencyStopState.cpp:44`).
            Some(if machine.is_pid_runtime_enabled() {
                MachineState::PidNormal
            } else {
                MachineState::PidDisabled
            })
        }

        // `PidNormalState::checkSpecificTransitions` (`PidStates.cpp:46-97`).
        MachineState::PidNormal => {
            // "CRITICAL: Check if PID was disabled while in this state". The
            // global guard excludes PID_NORMAL, so this is the *only* place the
            // machine notices — which is what
            // `test_pid_state_transitions` is about.
            if !machine.is_pid_runtime_enabled() {
                return Some(MachineState::PidDisabled);
            }
            if machine.requests.brew_start {
                machine.requests.brew_start = false;
                // In manual mode, go directly to BREW_RUNNING (skip
                // preinfusion).
                return Some(if ctx.brew_is_automatic() {
                    MachineState::BrewPreinfusion
                } else {
                    MachineState::BrewRunning
                });
            }
            // "Hot water is handled directly in PID_NORMAL via pump control (no
            // separate state needed)" — so there is no hot-water transition
            // here, and the water switch's effect is in `update`.
            if machine.requests.steam_start {
                machine.requests.steam_start = false;
                return Some(MachineState::SteamRunning);
            }
            if machine.requests.manual_flush_start {
                machine.requests.manual_flush_start = false;
                return Some(MachineState::ManualFlushRunning);
            }
            if machine.requests.backflush_enter {
                machine.requests.backflush_enter = false;
                return Some(MachineState::BackflushIdle);
            }
            if machine.requests.standby {
                machine.requests.standby = false;
                return Some(MachineState::Standby);
            }
            initialize_standby_if_needed(machine, ctx);
            if machine.standby.should_enter(ctx.config.standby.enabled) {
                return Some(MachineState::Standby);
            }
            None
        }

        // `PidDisabledState::checkSpecificTransitions` (`PidStates.cpp:135-147`),
        // **plus one deliberate divergence**.
        MachineState::PidDisabled => {
            if machine.is_pid_runtime_enabled() {
                return Some(MachineState::PidNormal);
            }
            // ⚠ NOT in the C++ — `isStandbyRequested()` is checked by exactly two
            // states there (`PidNormalState`, `PidStates.cpp:85`, and
            // `EepromErrorState`, `ErrorStates.cpp:78`). `PidDisabledState`
            // checks only the standby **timer**, so in the C++ a request to sleep
            // is silently ignored whenever the PID happens to be off.
            //
            // Measured on hardware: `POST /api/sleep` returned `202
            // {"accepted":true}`, the command reached the control task
            // (`control: command Sleep`), and the machine sat in `PID_DISABLED`
            // indefinitely. The UI offers a sleep button unconditionally, so a
            // machine configured with the PID off — the out-of-the-box state,
            // since `pid.enabled` defaults to `false` — cannot be put to sleep
            // by its own web interface at all.
            //
            // That is a defect, not a design: a state whose only additional exit
            // is a *timeout* is a state that cannot be left on request, and the
            // requested exit already exists in the sibling state one line away.
            // Approved by the human 2026-09-30. See `intentional-diffs.md` §13 and
            // `09-cpp-findings.md` §25.
            if machine.requests.standby {
                machine.requests.standby = false;
                return Some(MachineState::Standby);
            }
            // "Check standby timeout (mirrors original kPidDisabled behavior)".
            initialize_standby_if_needed(machine, ctx);
            if machine.standby.should_enter(ctx.config.standby.enabled) {
                return Some(MachineState::Standby);
            }
            None
        }

        // `BrewPreinfusionState::checkSpecificTransitions`
        // (`BrewStates.cpp:106-150`).
        MachineState::BrewPreinfusion => {
            if let Some(target) = check_brew_stop_request(machine, ctx) {
                return Some(target);
            }
            // In manual mode, skip preinfusion entirely.
            if !ctx.brew_is_automatic() {
                return Some(MachineState::BrewRunning);
            }
            if !ctx.config.brew.pre_infusion.enabled {
                return Some(MachineState::BrewRunning);
            }
            let pre_ms = secs_to_u32(ctx.config.brew.pre_infusion.time);
            if machine.state_timeout_elapsed(pre_ms) {
                return Some(if ctx.config.brew.pre_infusion.pause > 0.0 {
                    MachineState::BrewPreinfusionPause
                } else {
                    MachineState::BrewRunning
                });
            }
            None
        }

        // `BrewPreinfusionPauseState::checkSpecificTransitions`
        // (`BrewStates.cpp:191-216`).
        MachineState::BrewPreinfusionPause => {
            if let Some(target) = check_brew_stop_request(machine, ctx) {
                return Some(target);
            }
            // "Safety check: In manual mode, skip pause (shouldn't reach here,
            // but just in case)".
            if !ctx.brew_is_automatic() {
                return Some(MachineState::BrewRunning);
            }
            let pause_ms = secs_to_u32(ctx.config.brew.pre_infusion.pause);
            if machine.state_timeout_elapsed(pause_ms) {
                return Some(MachineState::BrewRunning);
            }
            None
        }

        // `BrewRunningState::checkSpecificTransitions` (`BrewStates.cpp:268-304`).
        MachineState::BrewRunning => {
            // Note: brew stop goes to BREW_FINISHED here, not to the PID state.
            // The other brew states use `checkBrewStopRequest` (→ PID state);
            // this one is the exception, so a stop in the main phase still
            // records the shot and shows the result for three seconds.
            if machine.requests.brew_stop {
                machine.requests.brew_stop = false;
                return Some(MachineState::BrewFinished);
            }
            // "Check automatic brew stop conditions (only in automatic mode)".
            if ctx.brew_is_automatic() {
                let target = machine.brew.target_ms;
                if ctx.config.brew.by_time.enabled
                    && target > 0.0
                    && machine.brew.elapsed_ms >= target
                {
                    return Some(MachineState::BrewFinished);
                }
                let target_weight = ctx.config.brew.by_weight.target_weight;
                // `BrewStates.cpp:296`:
                // `currentWeight >= static_cast<float>(targetWeight)`.
                #[allow(clippy::cast_possible_truncation)]
                // Justification: the C++ casts the configured `double` target to
                // `float` and compares it against a `float` reading, so the
                // truncation is the reference behaviour, not an accident.
                let target_weight_f32 = target_weight as f32;
                if ctx.config.brew.by_weight.enabled
                    && target_weight > 0.0
                    && machine.brew_weight() >= target_weight_f32
                {
                    return Some(MachineState::BrewFinished);
                }
            }
            None
        }

        // `BrewFinishedState::checkSpecificTransitions` (`BrewStates.cpp:328-345`).
        MachineState::BrewFinished => {
            if machine.requests.brew_start {
                machine.requests.brew_start = false;
                return Some(MachineState::BrewPreinfusion);
            }
            // "Use a hardcoded 3 second timeout for the finished state (display
            // time)". Not configurable in the C++; [`timing`] names the constant
            // so that stays true of the Rust too.
            if machine.state_timeout_elapsed(timing::BREW_FINISHED_DISPLAY_TIMEOUT_MS) {
                return Some(pid_state(ctx));
            }
            None
        }

        // `SteamRunningState::checkSpecificTransitions` (`SteamStates.cpp:49-60`).
        MachineState::SteamRunning => {
            if machine.requests.steam_stop {
                machine.requests.steam_stop = false;
                return Some(pid_state(ctx));
            }
            // "No direct handler checks - handlers set flags, states only check
            // flags. This fixes timing issues with direct hardware checks."
            None
        }

        // `ManualFlushRunningState::checkSpecificTransitions`
        // (`SystemStates.cpp:83-94`).
        MachineState::ManualFlushRunning => {
            if machine.requests.manual_flush_stop {
                machine.requests.manual_flush_stop = false;
                // "Return to backflush idle since manual flush is only available
                // from backflush mode" — the comment is a claim about how the
                // switch works, and the branch is the enforcement of it.
                return Some(if machine.is_backflush_mode_active() {
                    MachineState::BackflushIdle
                } else {
                    pid_state(ctx)
                });
            }
            None
        }

        // `BackflushState::checkSpecificTransitions` (`BackflushStates.cpp:42-59`).
        MachineState::BackflushIdle => {
            if let Some(target) = check_backflush_mode_disabled(machine, ctx) {
                return Some(target);
            }
            if machine.requests.manual_flush_start {
                machine.requests.manual_flush_start = false;
                return Some(MachineState::ManualFlushRunning);
            }
            if machine.requests.backflush_cycle_start {
                machine.requests.backflush_cycle_start = false;
                return Some(MachineState::BackflushFilling);
            }
            None
        }

        // `BackflushFillingState::checkSpecificTransitions`
        // (`BackflushStates.cpp:78-95`).
        MachineState::BackflushFilling => {
            if let Some(target) = check_backflush_mode_disabled(machine, ctx) {
                return Some(target);
            }
            if machine.requests.backflush_stop {
                machine.requests.backflush_stop = false;
                return Some(MachineState::BackflushIdle);
            }
            if machine.state_timeout_elapsed(ctx.backflush_fill_ms()) {
                return Some(MachineState::BackflushFlushing);
            }
            None
        }

        // `BackflushFlushingState::checkSpecificTransitions`
        // (`BackflushStates.cpp:113-140`).
        MachineState::BackflushFlushing => {
            if let Some(target) = check_backflush_mode_disabled(machine, ctx) {
                return Some(target);
            }
            if machine.requests.backflush_stop {
                machine.requests.backflush_stop = false;
                return Some(MachineState::BackflushIdle);
            }
            if machine.state_timeout_elapsed(ctx.backflush_flush_ms()) {
                let configured = ctx.backflush_cycles();
                if crate::backflush::resolve_cycle_advance(machine.backflush.cycle, configured)
                    == crate::backflush::CycleAdvanceEffect::StartNextCycle
                {
                    machine.backflush.cycle += 1;
                    return Some(MachineState::BackflushFilling);
                }
                machine.backflush.cycle = 1;
                return Some(MachineState::BackflushFinished);
            }
            None
        }

        // `BackflushFinishedState::checkSpecificTransitions`
        // (`BackflushStates.cpp:152-175`).
        MachineState::BackflushFinished => {
            if let Some(target) = check_backflush_mode_disabled(machine, ctx) {
                return Some(target);
            }
            if machine.requests.backflush_stop {
                machine.requests.backflush_stop = false;
                return Some(MachineState::BackflushIdle);
            }
            if machine.requests.backflush_cycle_start {
                machine.requests.backflush_cycle_start = false;
                return Some(MachineState::BackflushFilling);
            }
            if machine.state_timeout_elapsed(timing::BACKFLUSH_FINISHED_DISPLAY_TIMEOUT_MS) {
                return Some(MachineState::BackflushIdle);
            }
            None
        }

        // `WaterTankEmptyState::checkSpecificTransitions`
        // (`ErrorStates.cpp:69-92`).
        //
        // # The emergency check the C++ has here is dead
        //
        // `ErrorStates.cpp:70-74` starts with `if (context.isEmergencyStop())
        // return EMERGENCY_STOP;`, but `BaseState::checkTransitions` tests
        // emergency **first** and with no exclusion (`BaseState.h:139-142`), so
        // an emergency can never reach this function. Same for
        // `EepromErrorState` (`ErrorStates.cpp:112-115`). Preserved by
        // *omitting* it: the reachable behaviour is identical, and an
        // unreachable arm is one more thing to keep correct.
        MachineState::WaterTankEmpty => {
            if machine.is_water_tank_full() {
                return Some(pid_state(ctx));
            }
            if machine.requests.standby {
                machine.requests.standby = false;
                return Some(MachineState::Standby);
            }
            // "Safety: with keep_heater_on_empty the heater may still be running
            // here. The standby timeout must keep working so the heater never
            // runs unattended indefinitely while the tank stays empty."
            initialize_standby_if_needed(machine, ctx);
            if machine.standby.should_enter(ctx.config.standby.enabled) {
                return Some(MachineState::Standby);
            }
            None
        }

        // `SensorErrorState::checkSpecificTransitions` (`ErrorStates.cpp:32-56`).
        MachineState::SensorError => {
            let error_duration = error_duration_ms(machine);
            if !machine.has_sensor_error() && !machine.sensors.has_temperature_error {
                // "Error resolved - wait for recovery delay before resuming".
                // Strictly `>`, not `>=` (`ErrorStates.cpp:43`).
                if error_duration > timing::ERROR_RECOVERY_DELAY_MS {
                    return Some(pid_state(ctx));
                }
            } else {
                // "Error still present - keep resetting the clock so recovery
                // delay is measured from when the error actually clears, not
                // from entry."
                machine.error_since = Some(machine.now);
            }
            // "Stay in SENSOR_ERROR until error resolves — never transition to
            // PID_DISABLED. A persistent sensor error requires the user to
            // investigate; silently disabling the PID hides the problem."
            None
        }

        // `EepromErrorState::checkSpecificTransitions` (`ErrorStates.cpp:111-125`).
        MachineState::EepromError => {
            let error_duration = error_duration_ms(machine);
            // "EEPROM recovery timeout - attempting recovery".
            //
            // Note this goes to `PID_DISABLED` **unconditionally** — not to
            // `getPidState()` as every other recovery path does
            // (`ErrorStates.cpp:121`). A machine whose storage is unreadable
            // comes back with temperature control off, and only the user's
            // power-switch press can bring it back. Preserved.
            if error_duration > timing::EEPROM_RECOVERY_TIMEOUT_MS {
                return Some(MachineState::PidDisabled);
            }
            None
        }

        // `EmergencyStopState::checkSpecificTransitions`
        // (`EmergencyStopState.cpp:41-47`).
        MachineState::EmergencyStop => {
            if is_emergency_cleared(machine) {
                return Some(MachineState::Init);
            }
            None
        }

        // `StandbyState::checkSpecificTransitions` (`SystemStates.cpp:43-56`).
        MachineState::Standby => {
            if machine.requests.normal_operation {
                machine.requests.normal_operation = false;
                return Some(wake_from_standby(machine, ctx));
            }
            // "Exit standby on any user activity: brew start, steam start, or
            // hot water activity".
            //
            // `hasUserActivity()` and `shouldExitStandby()` are **stubs that
            // return false** (`MachineStateContext.cpp:419-429`, "TODO: Implement
            // proper user activity detection"), so in the shipped firmware only
            // the two request flags can wake the machine. The hot-water switch
            // does *not* wake it, despite the comment, because
            // `setHotWaterActivity` only resets the standby timer and sets no
            // flag. Preserved; reported.
            if machine.requests.brew_start || machine.requests.steam_start {
                return Some(wake_from_standby(machine, ctx));
            }
            None
        }
    }
}

/// The exit of `STANDBY`, which is the same for both wake reasons
/// (`SystemStates.cpp:44-55`).
fn wake_from_standby(machine: &mut Machine, ctx: &Context<'_>) -> MachineState {
    let _ = machine;
    pid_state(ctx)
}

/// `EmergencyStopState::isEmergencyCleared` (`EmergencyStopState.cpp:54-75`),
/// reduced to what is reachable.
///
/// The C++ has two branches: with a `ProcessController` wired in it calls
/// `processController->isEmergencyCleared(temp)`, which is
/// `EmergencyStopManager::isEmergencyCleared` (`EmergencyStopManager.cpp:71-90`):
/// invalid reading → false; `> 100.0 °C` → false; otherwise true. Without one it
/// falls back to the same `EMERGENCY_SAFE_TEMP_C` comparison
/// (`EmergencyStopState.cpp:66-70`).
///
/// **They are not the same function.** The manager validates the reading first;
/// the fallback does not. So with no `ProcessController` a reading of
/// `-40 °C` — a shorted probe — clears emergency stop, and with one it does not.
/// In the shipped firmware a `ProcessController` always exists, so the manager's
/// behaviour is the live one, and that is what is ported. The Rust has one
/// `cc_safety::can_clear` and no nullable controller, so the divergence has no
/// path to be reached; [`cc_safety::can_clear`] is
/// `EmergencyStopManager::isEmergencyCleared` exactly.
///
/// # One C++ difference that is preserved
///
/// The comparison is `temperature > 100.0`, so **exactly 100.0 °C clears**.
/// `cc_safety::can_clear` uses `<=`, which agrees.
#[must_use]
pub fn is_emergency_cleared(machine: &Machine) -> bool {
    cc_safety::can_clear(machine.temperature())
}

/// `std::chrono::duration_cast<milliseconds>(millis() - errorStartTime_).count()`
/// — `ErrorStates.cpp:25` and `ErrorStates.cpp:117`.
///
/// Returns 0 before entry has stamped `error_since`, which is unreachable
/// because `on_entry` is what sets it and it is cleared only on exit.
fn error_duration_ms(machine: &Machine) -> u32 {
    match machine.error_since {
        Some(since) => machine.now.since(since).raw(),
        None => 0,
    }
}

/// `context.initializeStandbyTimerIfNeeded()` (`MachineStateContext.cpp:207-210`)
/// → `StandbyCoordinator::initializeIfNeeded()` (`StandbyCoordinator.h:140-148`).
///
/// "If timer hasn't been started yet, initialize it" — and only when standby is
/// enabled. A pure mutation of [`Machine::standby`], so it produces no effect:
/// the C++'s `reset()` here is a timer re-arm, not user activity, and emitting
/// `ResetStandbyTimer` would tell the applier something a user did.
fn initialize_standby_if_needed(machine: &mut Machine, ctx: &Context<'_>) {
    if !ctx.config.standby.enabled {
        return;
    }
    if machine.standby.started_at.is_none() {
        machine.standby.reset(machine.now, ctx.standby_timeout_ms());
    }
}

/// `context.getPreInfusionTime() * 1000.0` (`BrewStates.cpp:29,159,178`).
///
/// Note this is **not** gated on `brew.pre_infusion.enabled` — the pause and
/// running states add the raw configured time to their elapsed timer whether or
/// not pre-infusion is on. It is harmless today (`BrewPreinfusion` is skipped
/// entirely when disabled, so the pause state is unreachable), and it is
/// reproduced rather than "fixed" because the arithmetic is visible on the brew
/// timer.
fn preinfusion_ms(ctx: &Context<'_>) -> f64 {
    ctx.config.brew.pre_infusion.time * 1000.0
}

/// `static_cast<unsigned long>(seconds * 1000.0)`, the C++'s truncating cast.
#[allow(clippy::cast_possible_truncation)]
// Justification: reproduces `static_cast<unsigned long>(seconds * 1000.0)`
// (`BrewStates.cpp:130,206`), truncation included.
#[allow(clippy::cast_sign_loss)]
// Justification: `secs_to_u32` checks `seconds > 0.0` before calling this, so the
// value cast is known to be non-negative.
fn saturating_millis(seconds: f64) -> u32 {
    (seconds * 1000.0) as u32
}

fn secs_to_u32(seconds: f64) -> u32 {
    if seconds <= 0.0 {
        return 0;
    }
    saturating_millis(seconds)
}
