//! Orchestration: the 18-state machine as a pure Elm-style reducer, per
//! [04 §3.1](../../docs/rust-migration/04-target-architecture.md#31-internal-structure-functional-core-imperative-shell).
//!
//! Owner: **R2-08**.
//!
//! # The shape
//!
//! ```text
//! reduce(machine, ctx, event) -> (machine', effects)
//! ```
//!
//! Pure. No I/O, no clock, no sleeping, no globals, no `Config::getInstance()`.
//! That is what makes the exhaustive `state x event` table a host test, and the
//! control loop host-benchmarkable (R2-09).
//!
//! # What this crate is *not*
//!
//! It is not `LoopManager::update()` ported. That god-function reaches into ten
//! subsystems in eight fixed steps (`src/core/LoopManager.cpp:90-253`) and the
//! order between them is load-bearing and untested. Here the same behaviour is
//! one function of three values, and the eight steps are:
//!
//! | C++ step | Where it is here |
//! | --- | --- |
//! | 2. sensor update | [`Event::SensorUpdated`] — a value, not a read |
//! | 3. handlers, standby | [`Event::ButtonPressed`] / [`Event::ButtonReleased`] / [`Event::Command`], plus `standby_update` in the tick |
//! | 4. state machine | [`states::update`], [`states::check_specific`] |
//! | 4-tail. hot water + brew handlers, **S5 valve check** | `handlers::pump_timeouts` then the valve check, at the end of the tick |
//! | 5. process control | `process_control` at the end of the tick |
//! | 6/7/8. network, web, display | **not here** — slow consumers, bounded queues (04 §3.2) |
//!
//! Steps 6-8 are not "deferred to a later task". They were never part of control
//! safety, and putting them behind a queue is the point of 04 §3.2.
//!
//! # One loop, one `Tick`
//!
//! The reducer processes **one** event and returns. A control-loop iteration is
//! therefore a *sequence* of `reduce` calls, ending in
//! [`Event::Tick`]:
//!
//! ```text
//! for ev in inbox.drain()        { (m, fx) = reduce(&m, &ctx, ev);   apply(fx) }
//! (m, fx) = reduce(&m, &ctx, Event::Tick { now: clock() }); apply(fx)
//! ```
//!
//! The ordering matters and is the C++'s. The switch edges are step 3 and the
//! state machine is step 4, so a brew switch press sets `brew_start` and the
//! `Tick` in the *same* loop consumes it. A press that arrives after the
//! `Tick` waits one iteration, which is the same as a press that arrives late in
//! the C++'s loop.
//!
//! # At most one transition per tick
//!
//! `StateMachine::update()` performs at most one transition per loop
//! (`StateMachine.cpp:81-86`, and 01 §5). [`reduce`] therefore computes **one**
//! target and applies it once. A machine that needs to go
//! `PID_NORMAL → BREW_PREINFUSION → BREW_RUNNING` takes two loops, and so does
//! the C++.
//!
//! # Purity, and how it is enforced
//!
//! [`reduce`] takes `&Machine` and returns `(Machine, Effects)`. There is no
//! interior mutability anywhere in [`Machine`], no `static mut`, and no
//! `unwrap`/`expect` outside tests. `tests/purity.rs` clones the input,
//! reduces, and asserts the input is byte-identical afterwards.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

use cc_domain::state::MachineState;
use cc_domain::units::Millis;

pub mod applier;
pub mod backflush;
pub mod context;
pub mod effect;
pub mod event;
pub mod guards;
pub mod handlers;
pub mod machine;
pub mod maintenance;
pub mod states;
pub mod timing;

pub use applier::{apply, apply_one, Actuators, Diagnostics, MachineChannels};
pub use backflush::{
    apply_backflush_mode, resolve_cycle_advance, resolve_mode_change, CycleAdvanceEffect,
    ModeChangeEffect, ModeChangeInput, ModeChangeOutcome,
};
pub use cc_safety::steam_flow_allowed;
pub use cc_safety::water_flow_allowed;
pub use context::Context;
pub use effect::{Effect, Effects, PumpWatchdog, MAX_EFFECTS_PER_EVENT};
pub use event::{Command, Event, Sensors, SwitchId};
pub use guards::{should_pid_be_enabled, Guard};
pub use machine::{
    Backflush, BrewProgress, Machine, Pid, Request, Requests, StandbyTimer, SwitchLevels,
};
pub use maintenance::{
    decode_shot_count, encode_shot_count, is_reminder_due, qualifies_as_counted_shot,
    record_brew_if_qualified, BACKFLUSH_REMINDER_THRESHOLD, MIN_BREW_TIME_MS, MIN_BREW_WEIGHT_G,
    SHOT_COUNT_BYTES,
};

/// The whole control decision, as a pure function.
///
/// # The contract
///
/// * `machine` is **not** mutated. The returned value is the new machine.
/// * No effect is emitted twice and none is dropped: the applier applies the
///   vector front to back and the tests assert on exact sequences.
/// * At most one transition, exactly as `StateMachine::update()`.
/// * Total: every [`Event`] is defined for every [`MachineState`]. The
///   exhaustive table in `tests/exhaustive_state_event.rs` proves it.
///
/// # What is *not* here, on purpose
///
/// * No clock. [`Event::Tick`] carries the reading.
/// * No sleeping. `PowerHandler`'s two `delay(1000)` calls around the reboot
///   belong to [`MachineChannels::on_request_reboot`], which is the only code in
///   the firmware allowed to block.
/// * No `ESP.restart()`. [`Effect::RequestReboot`] asks; the applier does.
/// * No `Vec` in the input. [`Event`] and [`Command`] are `Copy` and
///   fixed-size so they can cross a task boundary through
///   `hal::task::queue::Queue<Command, 32>` (04 §3.2).
///
/// The returned [`Effects`] is a fixed-capacity `heapless::Vec`, not an
/// `alloc::vec::Vec`: this function runs four to five times per 10 ms control
/// tick, and one heap allocation per call was 400 allocations a second in the
/// loop that also runs the heater deadman. See [`effect::Effects`].
#[must_use]
pub fn reduce(machine: &Machine, ctx: &Context<'_>, ev: Event) -> (Machine, Effects) {
    // `StateMachine::update()` returns immediately when it has not been
    // initialised (`StateMachine.cpp:72-75`). Reproduced rather than dropped:
    // an uninitialised machine must not act on a stale emergency request.
    if !machine.initialized {
        return (*machine, Effects::new());
    }

    let mut m = *machine;
    let mut fx = Effects::new();

    // ---- 1. the event itself ----------------------------------------------
    match ev {
        Event::SensorUpdated(sensors) => {
            m.sensors = sensors;
        }
        Event::ButtonPressed { switch, long_press } => {
            m.switches.set_level(switch, true);
            // `Switch::longPressDetected()` is latched on the edge. The C++ reads
            // it live (`BrewHandler.h:199`, `PowerHandler.h:144`); an edge is the
            // only place the reducer can see it, and by the time a hold is long
            // enough to matter the press event is history — which is why
            // `handlers::long_press_reboot` reads the latched flag later.
            match switch {
                SwitchId::Brew => m.switches.brew_long_press = long_press,
                SwitchId::Power => m.switches.power_long_press = long_press,
                SwitchId::Steam | SwitchId::HotWater => {}
            }
        }
        Event::ButtonReleased { switch } => {
            m.switches.set_level(switch, false);
        }
        Event::Command(_) => {}
        Event::Tick { now } => {
            m.now = now;
        }
        Event::PidOutput(output) => {
            apply_pid_output(&mut m, ctx, output, &mut fx);
        }
        Event::Safety(outcome) => {
            // The whole of `cc_safety`'s memory, in one value. S1 is a
            // three-reading debounce, so the counter is part of the machine's
            // state and the shell keeps no copy: it passes `&machine.safety` in
            // and stores the result.
            m.safety = outcome.state;
            m.verdict = outcome.verdict;
        }
    }

    // ---- 2. the handler layer (C++ step 3 / step 4-tail) -------------------
    fx.extend(&handlers::apply_input(&mut m, ctx, ev));

    // ---- 3. the control pass (C++ steps 3-5), once per loop ----------------
    if let Event::Tick { now } = ev {
        tick(&mut m, ctx, now, &mut fx);
    }

    (m, fx)
}

/// Everything that happens once per control-loop iteration.
///
/// In the C++ this is `LoopManager` steps 3 through 5, in that order:
/// `updateSwitchesAndStandby` (`LoopManager.cpp:561-569`),
/// `updateStateMachine` (`:571-621`) and `updateProcessControl` (`:288-321`).
fn tick(m: &mut Machine, ctx: &Context<'_>, now: Millis, fx: &mut Effects) {
    // ---- the switch handlers' per-loop work (LoopManager step 3) -----------
    //
    // `PowerHandler::recordSystemInitialization` is the only per-loop work any
    // handler does; the rest of `updateSwitchesAndStandby` is `powerHandler` and
    // `steamHandler`, which only react to edges and are handled by
    // `Event::ButtonPressed`/`ButtonReleased`/`Command`.
    handlers::record_power_handler_initialisation(m, ctx);

    // ---- standbyCoordinator().update() (LoopManager step 3, :568) ----------
    standby_update(m, ctx, now);

    // ---- currentState_->update(context) (StateMachine.cpp:81) -------------
    fx.extend(&states::update(m.state, m, ctx));

    // ---- currentState_->checkTransitions(context) (StateMachine.cpp:84) ---
    // At most one transition. `checkTransitions` runs the global guards first
    // and only then the state-specific rules (`BaseState.h:137-174`).
    if let Some(target) = next_state(m, ctx) {
        transition(m, ctx, target, fx);
    }

    // ---- the handler tail, AFTER the state machine (LoopManager.cpp:616-619) -
    //
    // The order of these two lines is the whole design: a state that asserts
    // "valve open" and the S5 fail-safe that asserts "valve closed" are both
    // applied in the same tick, and the *whitelist* is what decides.
    fx.extend(&handlers::pump_timeouts(m));

    // ---- valveSafetyShutdownCheck() (BrewHandler.h:105-122) ----------------
    //
    // `BrewHandler::valveSafetyShutdownCheck` runs every loop, after the state
    // machine, and closes the water valve unless the current state is on the
    // whitelist. The whitelist *is* `cc_safety::water_flow_allowed`
    // (ADR-0004: it "becomes a `match` with no wildcard arm"). Calling it
    // rather than re-deriving the list is the whole reason the list lives in
    // one place.
    if !cc_safety::water_flow_allowed(m.state) {
        fx.push(Effect::CloseWaterValve);
    }

    // ---- steamValveSafetyShutdownCheck() — **does not exist in the C++** ------
    //
    // The mirror of the check above, and the closest thing this port has to
    // `BrewHandler::valveSafetyShutdownCheck`'s steam counterpart. There is no
    // such function in the C++: `openSteamValve` checks only `emergencyMode_`
    // and nothing calls it either. See 09 §2 and
    // `intentional-diffs.md` #2.
    //
    // It is here because steam and water share **one relay**
    // (`ValveState.h:8-11`, GPIO17 in `pinmapping.h:39`), so an ungated steam
    // valve is an ungated *water* valve. The whitelist is
    // `cc_safety::steam_flow_allowed`, whose derivation is in that function's
    // documentation: `STEAM_RUNNING` and nothing else.
    if !cc_safety::steam_flow_allowed(m.state) {
        fx.push(Effect::CloseSteamValve);
    }

    // ---- PowerHandler::checkForLongPressReboot (PowerHandler.h:142-147) ----
    fx.extend(&handlers::long_press_reboot(m));

    // ---- updateProcessControl (LoopManager step 5) -------------------------
    process_control(m, ctx, fx);
}

/// The destination for this tick, or `None` to stay put.
///
/// `StateMachine::update()`'s `if (auto newStateId =
/// currentState_->checkTransitions(context_))` (`StateMachine.cpp:84-86`),
/// which is [`guards::global_guard`] then [`states::check_specific`].
///
/// # Flags are consumed even when the transition is discarded
///
/// [`states::check_specific`] clears the request flag it acts on *before*
/// returning the destination, exactly as `BaseState::checkBrewStopRequest` does
/// (`BaseState.h:104-105`). The self-transition check below therefore runs
/// **after** the flag is gone, which is the C++'s order: the flags are consumed
/// by `checkTransitions`, and only `executeTransition` discards the result.
///
/// That is load-bearing, not cosmetic. The emergency guard has **no** exclusion
/// (`BaseState.h:139-142`), so in `EMERGENCY_STOP` it returns `EMERGENCY_STOP`;
/// and the sensor-error guard likewise returns `SENSOR_ERROR` from
/// `SENSOR_ERROR`. Without the skip, every loop in those two states would
/// re-run `onExit` and `onEntry` — which for `EMERGENCY_STOP` would mean
/// `onExit` restoring the runtime PID from config and `onEntry` forcing it off
/// again, and for `PID_DISABLED` would mean draining the flags on every loop.
fn next_state(machine: &mut Machine, ctx: &Context<'_>) -> Option<MachineState> {
    let guard = guards::global_guard(machine);
    if let Some(target) = guards::guard_destination(guard) {
        return Some(target);
    }
    let current = machine.state;
    states::check_specific(current, machine, ctx)
}

/// Apply one transition: `onExit` → swap → stamp → `onEntry`.
///
/// `StateMachine::executeTransition` (`StateMachine.cpp:106-149`):
///
/// ```cpp
/// currentState_->onExit(context_);
/// currentState_ = std::move(newState);
/// auto now = steady_clock::now();
/// totalStateTransitions_++;
/// context_.updateStateEntryTime(now);
/// currentState_->onEntry(context_);
/// ```
///
/// The self-transition check is **not** repeated here: [`next_state`] has
/// already applied it, so a guard that names the current state produces no
/// effects at all.
fn transition(m: &mut Machine, ctx: &Context<'_>, target: MachineState, fx: &mut Effects) {
    // `createStateInstance(newStateId)` (`StateMachine.cpp:118`). The C++ logs
    // FATAL and restarts the device for an unknown id
    // (`StateFactory.cpp:65-69`); `MachineState` is an enum, so the id is known
    // by construction and `MachineState::from_id` is the only place a raw id
    // enters, where it returns `None` rather than rebooting.
    let old = m.state;
    if old == target {
        return;
    }

    // `StandbyState::checkSpecificTransitions` calls
    // `context.resetMqttReconnectCount()` immediately before returning the PID
    // state, on both of its two wake paths (`SystemStates.cpp:46` and `:52`).
    // The C++ puts that at the moment the exit is *decided*; here that moment is
    // the transition, so it goes at the top of the exit half.
    if old == MachineState::Standby {
        fx.push(Effect::ResetMqttReconnectCount);
    }

    fx.push(Effect::ExitState(old));
    fx.extend(&states::on_exit(old, m, ctx));

    m.state = target;
    // Stamped **before** `onEntry`, so a state's entry code sees its own
    // elapsed time as zero (`StateMachine.cpp:145-148`).
    m.entry_at = m.now;

    fx.push(Effect::EnterState(target));
    fx.extend(&states::on_entry(target, m, ctx));
}

/// `standbyCoordinator().update()` (`StandbyCoordinator.h:31-74`).
///
/// The C++ recomputes the countdown at most once a second
/// (`StandbyCoordinator.h:39`) and only when standby is enabled. Preserved: a
/// port that recomputed every loop would enter standby up to a second earlier,
/// and the once-a-second log line (`StandbyCoordinator.h:50`) is part of what
/// the monitor shows.
fn standby_update(m: &mut Machine, ctx: &Context<'_>, now: Millis) {
    if !ctx.config.standby.enabled {
        return;
    }
    let Some(started) = m.standby.started_at else {
        // Not initialised: `StandbyCoordinator::update` leaves everything alone,
        // and `shouldEnterStandby` is false because the start time is unset.
        return;
    };
    let elapsed = now.since(started);
    if elapsed.raw() < timing::STANDBY_UPDATE_GRANULARITY_MS {
        return;
    }
    let Some(last) = m.standby.last_update else {
        m.standby.last_update = Some(now);
        return;
    };
    if now.since(last).raw() < timing::STANDBY_UPDATE_GRANULARITY_MS {
        return;
    }
    m.standby.last_update = Some(now);

    if m.standby.remaining_ms != 0 {
        let timeout = ctx.standby_timeout_ms();
        if timeout > elapsed.raw() {
            m.standby.remaining_ms = timeout - elapsed.raw();
        } else {
            m.standby.remaining_ms = 0;
        }
        return;
    }

    // The display-off countdown (`StandbyCoordinator.h:56-72`). It runs only
    // once the standby countdown itself has expired, and it counts from the
    // *same* `standbyModeStartTimeMillis_` against a longer deadline
    // (`standbyTimeout + displayOffTimeout`), which is why the machine sits in
    // standby with the standby screen lit for another ten minutes before the
    // panel is blanked.
    //
    // The human's report was "standby should show the screen for a while and
    // only then turn the display off", and the firmware blanked the panel the
    // instant it entered standby — the behaviour of the un-ported branch above.
    if m.standby.display_off_remaining_ms == 0 {
        return;
    }
    let deadline = ctx
        .standby_timeout_ms()
        .saturating_add(timing::STANDBY_DISPLAY_OFF_MS);
    if deadline > elapsed.raw() {
        m.standby.display_off_remaining_ms = deadline - elapsed.raw();
    } else {
        m.standby.display_off_remaining_ms = 0;
    }
}

/// `ProcessController::updatePIDState` + `handleBrewPIDDelay`
/// (`ProcessController.cpp:151-172` and `:465-487`), the two halves of
/// `updateProcessControl` that are state-machine decisions.
///
/// `testEmergencyConditions` and `computePID` are **not** here: the first is
/// `Event::Safety`, the second is `cc_domain::Controller`'s job. `updateSetpoint`
/// is the shell's.
///
/// # The second, independent PID gate
///
/// [`should_pid_be_enabled`] is a *different* question from the state machine's
/// global guard. The guard asks "should the machine be in `PID_DISABLED`?"; this
/// asks "may the heater be energised?". They overlap and neither contains the
/// other: `WATER_TANK_EMPTY` is excluded from the guard (so the machine can
/// recover) but blocks the heater unless `keep_heater_on_empty`.
fn process_control(m: &mut Machine, ctx: &Context<'_>, fx: &mut Effects) {
    // `updatePIDState` (`ProcessController.cpp:151-172`).
    let permitted =
        guards::should_pid_be_enabled(m.state, ctx.keep_heater_on_empty(), m.pid.brew_disabled);
    if !permitted && m.pid.mode_enabled {
        // "Force PID shutdown": zero the output, then the heater, through the
        // hardware manager so `heaterEnabled_` stays in sync.
        m.pid.mode_enabled = false;
        m.pid.output = 0.0;
        fx.push(Effect::DisableHeater);
    } else if permitted && !m.pid.mode_enabled {
        m.pid.mode_enabled = true;
    }

    // `handleBrewPIDDelay` (`ProcessController.cpp:465-487`).
    handle_brew_pid_delay(m, ctx, fx);
}

/// `ProcessController::handleBrewPIDDelay` (`ProcessController.cpp:465-487`).
///
/// ```cpp
/// const bool inBrewState = isBrewState(machineState);
/// const double brewPidDelayMs = config_.brewPidDelay.get() * 1000.0;
/// const double currentBrewTime = processCurrentBrewTime();
/// const bool brewDelayEnabled = config_.brewPidDelay.get() > 0;
/// if (inBrewState) {
///     if (brewDelayEnabled && currentBrewTime > 0 && currentBrewTime < brewPidDelayMs) {
///         disablePIDForBrewDelay();
///     } else {
///         enablePIDAfterBrewDelay();
///     }
/// } else {
///     reEnablePIDAfterBrewAbort();
/// }
/// ```
///
/// # The `currentBrewTime > 0` clause is not a no-op
///
/// It means the delay window does not open until the brew timer is *non-zero*,
/// and the timer is only non-zero from the second loop of a brew state. A brew
/// that is aborted within one iteration therefore never disables the PID at
/// all — which is the point of `reEnablePIDAfterBrewAbort` existing, and why
/// the abort path is not merely the inverse.
fn handle_brew_pid_delay(m: &mut Machine, ctx: &Context<'_>, fx: &mut Effects) {
    let delay_ms = ctx.brew_pid_delay_ms();
    let in_brew = m.state.is_brew_state();

    if in_brew {
        let in_window = delay_ms > 0.0 && m.brew.elapsed_ms > 0.0 && m.brew.elapsed_ms < delay_ms;
        if in_window {
            if !m.pid.brew_disabled {
                // `disablePIDForBrewDelay` (`ProcessController.cpp:410-424`).
                m.pid.brew_disabled = true;
                m.pid.mode_enabled = false;
                m.pid.output = 0.0;
                fx.push(Effect::DisableHeater);
            }
        } else if m.pid.brew_disabled {
            // `enablePIDAfterBrewDelay` (`ProcessController.cpp:432-450`).
            // The tunings it selects are `cc_domain::Controller`'s business; the
            // machine only needs the flag.
            m.pid.brew_disabled = false;
            m.pid.mode_enabled = true;
        }
    } else if m.pid.brew_disabled {
        // `reEnablePIDAfterBrewAbort` (`ProcessController.cpp:459-469`).
        m.pid.brew_disabled = false;
        m.pid.mode_enabled = true;
    }
}

/// Fold a fresh PID output in, and say what may reach the heater.
///
/// `Event::PidOutput` is the shell handing over what `cc_domain::Controller`
/// computed. The C++ publishes it unconditionally and relies on
/// `updatePIDState` zeroing it later in the same `updateProcessControl`
/// (`ProcessController.cpp:113` then `:151-172`).
///
/// # One deliberate difference in *what is emitted*
///
/// This gates the emitted [`Effect::SetHeaterDuty`] on the heater permission,
/// where the C++ emits the value and zeroes it a few microseconds later. The
/// machine state is identical either way; the only difference is that this port
/// never asks the heater to be on when it must not be, even for the microseconds
/// between the two C++ statements. That is the "fail safe, not fail fast"
/// direction, and it is recorded here rather than left to be discovered.
fn apply_pid_output(m: &mut Machine, ctx: &Context<'_>, output: f32, fx: &mut Effects) {
    m.pid.output = output;
    let permitted =
        guards::should_pid_be_enabled(m.state, ctx.keep_heater_on_empty(), m.pid.brew_disabled)
            && m.pid.mode_enabled
            && !m.is_emergency_stop()
            && m.verdict.may_heat;
    fx.push(Effect::SetHeaterDuty(if permitted { output } else { 0.0 }));
}

/// The clear, uninitialised machine. Convenience over [`Machine::cold`].
#[must_use]
pub fn new() -> Machine {
    Machine::cold()
}

/// `StateMachine::initialize(MachineStateId::INIT)` (`StateMachine.cpp:28-69`).
///
/// The C++ calls `onEntry` on the initial state, stamps the entry time, and sets
/// `initialized_ = true`. This is the same thing as a value, and it is the only
/// way to get an initialised machine: [`reduce`] ignores every event until then,
/// which is what `StateMachine::update()`'s guard
/// (`StateMachine.cpp:72-75`) does.
///
/// The state is `INIT` and the runtime PID flag is the config value, which is
/// the "no power switch" arm of
/// `SystemInitializer::finalizeMachineState` (`SystemInitializer.cpp:633-641`).
/// The first [`Event::Tick`] then runs `InitState::checkSpecificTransitions`
/// and lands in `PID_NORMAL` or `PID_DISABLED` — one loop after boot, exactly as
/// the C++ does.
#[must_use]
pub fn boot(now: Millis, ctx: &Context<'_>) -> (Machine, Effects) {
    boot_in(MachineState::Init, ctx.pid_config_enabled(), now, ctx)
}

/// Boot into a specific state with a specific runtime-PID flag.
///
/// The three other arms of `SystemInitializer::finalizeMachineState`
/// (`SystemInitializer.cpp:604-641`), which reads the power switch *before* the
/// state machine exists and then hands it both an initial state and a runtime
/// PID state:
///
/// | power switch | initial state | runtime PID |
/// | --- | --- | --- |
/// | momentary | `PID_NORMAL` | `true` (via `setUserPidEnabled`) |
/// | toggle, ON | `PID_NORMAL` | `true` |
/// | toggle, OFF | `PID_DISABLED` | `false` |
/// | absent | `INIT` | `pid.enabled` |
///
/// # A note on the C++'s two state fields
///
/// `SystemInitializer` sets `MachineStateContext::currentStateId_` *and*
/// `StateMachine::initialize()` creates an `InitState` — so immediately after
/// boot the two disagree, and the first
/// `LoopManager::updateStateMachine` reconciles them by overwriting the context
/// with the state machine's value (`LoopManager.cpp:605-611`). The end state is
/// `INIT` either way, because `InitState` routes on `isPidRuntimeEnabled()`,
/// which `finalizeMachineState` has already set. Ported as a single field: the
/// disagreement is not observable and one field cannot hold two truths.
#[must_use]
pub fn boot_in(
    initial: MachineState,
    pid_runtime_enabled: bool,
    now: Millis,
    ctx: &Context<'_>,
) -> (Machine, Effects) {
    let mut m = Machine::cold();
    m.state = initial;
    m.now = now;
    m.entry_at = now;
    m.initialized = true;
    m.pid.runtime_enabled = pid_runtime_enabled;
    let fx = states::on_entry(initial, &mut m, ctx);
    (m, fx)
}
