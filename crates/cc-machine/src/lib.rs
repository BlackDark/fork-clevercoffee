//! Pure reducer: `reduce(machine, context, event) -> (machine, effects)`.
//!
//! Drain input events before the final [`Event::Tick`] so a press can take effect
//! in the same iteration. One tick makes at most one transition; exit and entry
//! hooks run fully before state updates and the final valve fail-safe.
//! Slow network and display work belongs outside the reducer.

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
pub mod ota;
pub mod states;
pub mod timing;

pub use applier::{apply, apply_one, Actuators, Diagnostics, MachineChannels};
pub use backflush::{
    apply_backflush_mode, resolve_cycle_advance, resolve_mode_change, CycleAdvanceEffect,
    ModeChangeEffect, ModeChangeInput, ModeChangeOutcome,
};
pub use cc_safety::steam_flow_allowed;
pub use cc_safety::water_flow_allowed;
pub use context::{safety_config, Context};
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

/// Pure control decision; leaves the input unchanged and transitions at most once.
///
/// Every state/event pair is defined. Effects retain their order in fixed-capacity
/// [`Effects`]; callers must apply them front to back. Time arrives through
/// [`Event::Tick`], reboot through [`Effect::RequestReboot`], and blocking work
/// belongs to the shell.
#[must_use]
pub fn reduce(machine: &Machine, ctx: &Context<'_>, ev: Event) -> (Machine, Effects) {
    // An uninitialised machine must not act on a stale emergency request, so every event is
    // ignored until `boot` has run.
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
            // The long-press flag arrives with the edge and is latched here. By the time a
            // hold is long enough to matter the press event is history, which is why
            // `handlers::long_press_reboot` reads the latched flag rather than this event.
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
            // The whole of `cc_safety`'s memory, in one value. S1 is a three-reading
            // debounce, so the counter is part of the machine's state and the shell keeps no
            // copy: it passes `&machine.safety` in and stores the result.
            m.safety = outcome.state;
            m.verdict = outcome.verdict;
        }
    }

    // ---- 2. the handler layer ---------------------------------------------
    fx.extend(&handlers::apply_input(&mut m, ctx, ev));

    // ---- 3. the control pass, once per iteration ---------------------------
    if let Event::Tick { now } = ev {
        tick(&mut m, ctx, now, &mut fx);
    }

    (m, fx)
}

/// Everything that happens once per control-loop iteration, in order: the handlers' per-loop
/// work, standby, the state machine, the handler tail with the valve fail-safes, and process
/// control.
fn tick(m: &mut Machine, ctx: &Context<'_>, now: Millis, fx: &mut Effects) {
    // ---- the switch handlers' per-loop work -------------------------------
    //
    // [`handlers::record_power_handler_initialisation`] is the only per-loop work any handler
    // does; the rest of the handler layer reacts to edges and is reached through
    // `Event::ButtonPressed`/`ButtonReleased`/`Command`.
    handlers::record_power_handler_initialisation(m, ctx);

    // ---- the standby countdown -------------------------------------------
    standby_update(m, ctx, now);

    // ---- the current state's own update ----------------------------------
    fx.extend(&states::update(m.state, m, ctx));

    // ---- the transition check -------------------------------------------
    // At most one transition: the global guards are consulted first, and only then the
    // state-specific rules.
    if let Some(target) = next_state(m, ctx) {
        transition(m, ctx, target, fx);
    }

    // ---- the handler tail, AFTER the state machine ------------------------
    //
    // The order of these two lines is the whole design: a state that asserts "valve open" and
    // the S5 fail-safe that asserts "valve closed" are both applied in the same tick, and the
    // *whitelist* is what decides.
    fx.extend(&handlers::pump_timeouts(m));

    // ---- the water-valve fail-safe ---------------------------------------
    //
    // Runs every iteration, after the state machine, and closes the water valve unless the
    // current state is on the whitelist. The whitelist *is* `cc_safety::water_flow_allowed`:
    // calling it rather than re-deriving the list is the whole reason the list lives in one
    // place.
    if !cc_safety::water_flow_allowed(m.state) {
        fx.push(Effect::CloseWaterValve);
    }

    // ---- the steam-valve fail-safe ---------------------------------------
    //
    // The mirror of the check above, and the reason it exists at all is that steam and water
    // share **one relay** (GPIO17), so an ungated steam valve is an ungated *water* valve. The
    // whitelist is `cc_safety::steam_flow_allowed`, whose derivation is in that function's
    // documentation: `STEAM_RUNNING` and nothing else. The gate is deliberate.
    if !cc_safety::steam_flow_allowed(m.state) {
        fx.push(Effect::CloseSteamValve);
    }

    // ---- the held power switch -------------------------------------------
    fx.extend(&handlers::long_press_reboot(m));

    // ---- process control -------------------------------------------------
    process_control(m, ctx, fx);
}

/// The destination for this tick, or `None` to stay put.
///
/// The destination for this iteration: [`guards::global_guard`] first, then
/// [`states::check_specific`].
///
/// # Flags are consumed even when the transition is discarded
///
/// [`states::check_specific`] clears the request flag it acts on *before* returning the
/// destination, so the self-transition check in [`transition`] runs **after** the flag is
/// gone. Flags are consumed by the transition check; only the application of the result is
/// skipped.
///
/// That is load-bearing, not cosmetic. The emergency guard has **no** exclusion, so in
/// `EMERGENCY_STOP` it returns `EMERGENCY_STOP`; the sensor-error guard likewise returns
/// `SENSOR_ERROR` from `SENSOR_ERROR`. Without the skip, every iteration in those two states
/// would re-run exit and entry -- which for `EMERGENCY_STOP` would mean exit restoring the
/// runtime PID from the configuration and entry forcing it off again, and for `PID_DISABLED`
/// would mean draining the flags on every iteration.
fn next_state(machine: &mut Machine, ctx: &Context<'_>) -> Option<MachineState> {
    let guard = guards::global_guard(machine);
    if let Some(target) = guards::guard_destination(guard) {
        return Some(target);
    }
    let current = machine.state;
    states::check_specific(current, machine, ctx)
}

/// Apply one transition: exit, swap, stamp, entry.
///
/// The self-transition check is **not** repeated here: [`next_state`] has already applied it,
/// so a guard that names the current state produces no effects at all.
fn transition(m: &mut Machine, ctx: &Context<'_>, target: MachineState, fx: &mut Effects) {
    // `MachineState` is an enum, so an unknown state cannot be constructed: the id is known by
    // construction, and `MachineState::from_id` is the only place a raw id enters, where it
    // returns `None`.
    let old = m.state;
    if old == target {
        return;
    }

    // Both wake paths out of standby reset the reconnect count, and the exit being *decided*
    // is the transition, so it goes at the top of the exit half.
    if old == MachineState::Standby {
        fx.push(Effect::ResetMqttReconnectCount);
    }

    fx.push(Effect::ExitState(old));
    fx.extend(&states::on_exit(old, m, ctx));

    m.state = target;
    // Stamped **before** `on_entry`, so a state's entry code sees its own elapsed time as zero.
    m.entry_at = m.now;

    fx.push(Effect::EnterState(target));
    fx.extend(&states::on_entry(target, m, ctx));
}

/// Advance the standby countdown.
///
/// Recomputed at most once a second, and only when standby is enabled: recomputing every
/// iteration would enter standby up to a second earlier, which matters because the remaining
/// time is displayed.
fn standby_update(m: &mut Machine, ctx: &Context<'_>, now: Millis) {
    if !ctx.config.standby.enabled {
        return;
    }
    let Some(started) = m.standby.started_at else {
        // Not started, so nothing is counted and [`StandbyTimer::should_enter`] is false.
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

    // The display-off countdown. It runs only once the standby countdown itself has expired,
    // and it counts from the *same* start time against a longer deadline
    // (`standby.time` + [`timing::STANDBY_DISPLAY_OFF_MS`]) -- which is why the machine sits
    // in standby with the standby screen lit for another ten minutes before the panel is
    // blanked. The operator asked for exactly that: a screen that stays visible for a while,
    // rather than one that goes dark the instant standby is entered.
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

/// The two per-iteration process-control decisions: whether the PID may run at all, and the
/// brew-delay window.
///
/// The safety verdict and the PID computation are **not** here: the first arrives as
/// [`Event::Safety`], the second is `cc_domain::Controller`'s job. Setpoint selection is the
/// shell's.
///
/// # The second, independent PID gate
///
/// [`should_pid_be_enabled`] is a *different* question from the state machine's global guard.
/// The guard asks "should the machine be in `PID_DISABLED`?"; this asks "may the heater be
/// energised?". They overlap and neither contains the other: `WATER_TANK_EMPTY` is excluded
/// from the guard (so the machine can recover) but blocks the heater unless
/// `keep_heater_on_empty`.
fn process_control(m: &mut Machine, ctx: &Context<'_>, fx: &mut Effects) {
    let permitted =
        guards::should_pid_be_enabled(m.state, ctx.keep_heater_on_empty(), m.pid.brew_disabled);
    if !permitted && m.pid.mode_enabled {
        // Zero the output before the heater is disabled, so a later re-enable starts from a
        // known duty rather than the previous one.
        m.pid.mode_enabled = false;
        m.pid.output = 0.0;
        fx.push(Effect::DisableHeater);
    } else if permitted && !m.pid.mode_enabled {
        m.pid.mode_enabled = true;
    }

    handle_brew_pid_delay(m, ctx, fx);
}

/// Hold the PID off for the first `brew.pid_delay` milliseconds of a brew.
///
/// ```text
/// if the state is a brew state {
///     if the delay is configured, the brew timer is running, and it is still inside the window
///     { disable the PID }
///     else { enable it }
/// } else if the PID is disabled { re-enable it }
/// ```
///
/// # The `elapsed_ms > 0` clause is not a no-op
///
/// It means the window does not open until the brew timer is *non-zero*, and the timer is only
/// non-zero from the second iteration of a brew state. A brew aborted within one iteration
/// therefore never disables the PID at all, which is why the abort path is not merely the
/// inverse: it exists to re-enable a PID this function may have disabled, not to undo a state
/// it cannot have reached.
fn handle_brew_pid_delay(m: &mut Machine, ctx: &Context<'_>, fx: &mut Effects) {
    let delay_ms = ctx.brew_pid_delay_ms();
    let in_brew = m.state.is_brew_state();

    if in_brew {
        let in_window = delay_ms > 0.0 && m.brew.elapsed_ms > 0.0 && m.brew.elapsed_ms < delay_ms;
        if in_window {
            if !m.pid.brew_disabled {
                // Start the delay: the heater goes off until the window closes.
                m.pid.brew_disabled = true;
                m.pid.mode_enabled = false;
                m.pid.output = 0.0;
                fx.push(Effect::DisableHeater);
            }
        } else if m.pid.brew_disabled {
            // The window has closed. The tunings it selects are `cc_domain::Controller`'s
            // business; the machine only needs the flag.
            m.pid.brew_disabled = false;
            m.pid.mode_enabled = true;
        }
    } else if m.pid.brew_disabled {
        // The brew ended or never started the delay, and the PID is disabled for a window
        // that is no longer open.
        m.pid.brew_disabled = false;
        m.pid.mode_enabled = true;
    }
}

/// Fold a fresh PID output in, and say what may reach the heater.
///
/// `Event::PidOutput` is the shell handing over what `cc_domain::Controller` computed. The
/// value is stored either way, so the reported output does not depend on the permission, and
/// the emitted [`Effect::SetHeaterDuty`] is gated on it: the heater is never asked to be on
/// when the machine may not heat, not even for the interval before the next decision. That is
/// the "fail safe, not fail fast" direction.
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

/// The machine with no power switch configured: `INIT`, runtime PID from `pid.enabled`.
///
/// [`reduce`] ignores every event until a machine is initialised, so [`boot`] and [`boot_in`]
/// are the only ways in.
///
/// The first [`Event::Tick`] then leaves `INIT` and lands in `PID_NORMAL` or `PID_DISABLED`,
/// in the iteration after boot.
#[must_use]
pub fn boot(now: Millis, ctx: &Context<'_>) -> (Machine, Effects) {
    boot_in(MachineState::Init, ctx.pid_config_enabled(), now, ctx)
}

/// Boot into a specific state with a specific runtime-PID flag.
///
/// The pair is the caller's decision, because the power switch has to be read before the
/// machine exists: `Control::boot` in `cc-firmware` reads it and passes the matching pair
/// (`PID_NORMAL`/`true` for a momentary switch or a toggle in the on position,
/// `PID_DISABLED`/`false` for a toggle in the off position, `INIT`/`pid.enabled` when there is
/// no power switch).
///
/// The machine holds **one** state, so the initialised state and the runtime PID flag cannot
/// disagree. `boot` is the convenience wrapper for the no-switch case.
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
