//! The eighteen states: what each one does on entry, on exit, on update, and when it wants to
//! transition.
//!
//! Four functions, one per hook, each a `match` over [`MachineState`] with **no wildcard arm**.
//! That is the mechanism: adding a 19th state fails to compile here, in [`crate::guards`], and
//! in `cc_safety::water_flow_allowed` until someone has decided what its entry, exit, update
//! and transitions are.
//!
//! # ADR-0003's contract, in three lines per state
//!
//! | | rule | where |
//! | --- | --- | --- |
//! | energise | enable hardware on entry | [`on_entry`] |
//! | reinforce | re-assert it on update | [`update`] |
//! | release | disable it on exit | [`on_exit`] |
//!
//! # What does not follow those three lines
//!
//! * **The two error states have no exit release.** They never energise anything, so there is
//!   nothing to release.
//! * **Reinforcing is not redundant.** The valve fail-safes run after [`update`] in the same
//!   tick, so a state that asserts "valve open" and a check that asserts "valve closed" are
//!   both applied, and the whitelist decides.

use cc_domain::state::MachineState;

use crate::context::Context;
use crate::effect::{Effect, Effects};
use crate::machine::Machine;
use crate::timing;

/// Where an operation goes when it ends: `PID_NORMAL` if `pid.enabled`, else `PID_DISABLED`.
///
/// It reads the **configured preference**, not the runtime flag. Every state that ends an
/// operation asks this: brew stop, steam stop, sensor-error recovery, tank refill,
/// backflush-disabled, standby exit. The two differ exactly after an emergency stop, which is
/// why that state's exit restores the runtime flag from the configuration before anyone
/// asks.
#[must_use]
pub fn pid_state(ctx: &Context<'_>) -> MachineState {
    if ctx.pid_config_enabled() {
        MachineState::PidNormal
    } else {
        MachineState::PidDisabled
    }
}

/// Consume a pending brew-stop request and say where it leads.
///
/// `None` when no stop is pending; otherwise the flag is cleared and the destination returned.
/// Clearing **here** rather than on the transition means a second brew-stop request arriving in
/// the same tick is dropped rather than queued.
fn check_brew_stop_request(machine: &mut Machine, ctx: &Context<'_>) -> Option<MachineState> {
    if !machine.requests.brew_stop {
        return None;
    }
    machine.requests.brew_stop = false;
    Some(pid_state(ctx))
}

/// Drain every action request, and say so.
///
/// A state that cannot act on an action request must not leave one behind for the state it
/// recovers into (AG-REPO-24, ADR-0003 rule 2). Three arms need exactly this, and `on_entry` is
/// at clippy's `too_many_lines` ceiling, so it is a function rather than three copies of two
/// lines.
fn drain_action_requests(machine: &mut Machine, fx: &mut Effects) {
    machine.requests.clear_all();
    fx.push(Effect::ClearActionRequests);
}

/// Consume a backflush-mode-off request and say where it leads.
///
/// `None` while backflush mode is still on. When it is off, the three backflush requests are
/// cleared and the destination is the PID pair.
///
/// Every backflush state calls this **first**, so turning backflush mode off overrides whatever
/// the switch was doing -- including a stop request, which is why
/// `test_backflush_states`' `ModeDisabledMidFillTransitionsToPid` does not need to clear the
/// stop flag first.
fn check_backflush_mode_disabled(machine: &mut Machine, ctx: &Context<'_>) -> Option<MachineState> {
    if machine.is_backflush_mode_active() {
        return None;
    }
    machine.requests.backflush_enter = false;
    machine.requests.backflush_cycle_start = false;
    machine.requests.backflush_stop = false;
    Some(pid_state(ctx))
}

/// What a state does when it is entered.
///
/// The caller has already set [`Machine::entry_at`], so
/// [`state_elapsed_ms`](Machine::state_elapsed_ms) is 0 here.
#[allow(clippy::match_same_arms)]
// Justification: the eighteen arms are kept separate on purpose. Several states do the same
// thing, but each is a different state's contract, and a change to one must not silently
// change another. Merging them would make ADR-0003's table untestable per row.
#[must_use]
pub fn on_entry(state: MachineState, machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    let mut fx = Effects::new();
    match state {
        MachineState::Init => {}

        // The standby countdown is deliberately **not** re-armed here: only user activity
        // re-arms it, so time spent in a brew counts toward standby.
        MachineState::PidNormal => {}

        MachineState::PidDisabled => {
            machine.pid.runtime_enabled = false;
            fx.push(Effect::SetPidRuntime { enabled: false });
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
            // S11: drain every flag, so a request that arrived before the
            // machine went PID-disabled cannot fire the moment it comes back.
            drain_action_requests(machine, &mut fx);
        }

        MachineState::BrewPreinfusion => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
            machine.brew.elapsed_ms = 0.0;
            // The target is set from the very start, so the display shows "0s/30s" rather
            // than "0s" and then jumping.
            machine.brew.target_ms = ctx.total_target_brew_ms();
        }

        MachineState::BrewPreinfusionPause => {
            // Pump off, **valve deliberately open** to hold puck pressure.
            fx.push(Effect::DisablePump);
            fx.push(Effect::OpenWaterValve);
            machine.brew.elapsed_ms = preinfusion_ms(ctx);
        }

        MachineState::BrewRunning => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
            // An automatic shot's timer starts from the pre-infusion time; a manual shot's
            // starts at zero, because a manual shot has no pre-infusion phase to count.
            machine.brew.elapsed_ms = if ctx.brew_is_automatic() {
                ctx.preinfusion_base_ms()
            } else {
                0.0
            };
            machine.brew.target_ms = ctx.total_target_brew_ms();
        }

        MachineState::BrewFinished => {
            // The qualification rule and the increment happen here, where the counter lives;
            // only the NVS write is the shell's, because only the shell has the store. The
            // effect below carries the answer rather than the inputs.
            //
            // Doing the decision here rather than in the applier is the fix for the counter
            // never moving: a rule that has to be wired up can be forgotten.
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

        MachineState::ManualFlushRunning => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
        }

        MachineState::SteamRunning => {
            machine.steam_mode = true;
            machine.steam_first_on = true;
            fx.push(Effect::SetSteamMode { enabled: true });
        }

        MachineState::BackflushIdle => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        MachineState::BackflushFilling => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
        }

        MachineState::BackflushFlushing => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        MachineState::BackflushFinished => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
            machine.shots_since_backflush = 0;
            fx.push(Effect::ResetShotsSinceBackflush);
        }

        // No hardware, deliberately: with `keep_heater_on_empty` the heater is *supposed* to
        // keep running while the tank is empty, so this state must not switch anything off.
        MachineState::WaterTankEmpty => {}

        MachineState::EmergencyStop => {
            // Drained on entry **and** on every tick: a request that arrives during the latch is
            // set after entry has run.
            fx.push(Effect::EmergencyShutdown);
            machine.pid.runtime_enabled = false;
            fx.push(Effect::SetPidRuntime { enabled: false });
            drain_action_requests(machine, &mut fx);
        }

        MachineState::Standby => {
            machine.steam_mode = false;
            machine.steam_first_on = false;
            fx.push(Effect::SetSteamMode { enabled: false });
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
            machine.pid.runtime_enabled = false;
            fx.push(Effect::SetPidRuntime { enabled: false });
            // The stop flags go; a start flag survives, because a brew or steam request that
            // arrived while the machine was asleep is an intentional wake trigger.
            machine.requests.clear_stale_stops();
            fx.push(Effect::ClearStaleStopRequests);
        }

        MachineState::SensorError => {
            // Entry is the only writer of the error start time. The drain is this firmware's
            // addition: the error states recover into `PID_NORMAL`, whose entry is empty, so a
            // request that survived here would start a real brew on recovery (AG-REPO-24).
            // See §37 of `docs/history/divergences.md`.
            drain_action_requests(machine, &mut fx);
            machine.error_since = Some(machine.now);
        }

        MachineState::EepromError => {
            // As `SENSOR_ERROR`, and it also forces the runtime PID off.
            drain_action_requests(machine, &mut fx);
            machine.pid.runtime_enabled = false;
            fx.push(Effect::SetPidRuntime { enabled: false });
            machine.error_since = Some(machine.now);
        }
    }
    fx
}

/// What a state does when it is left.
///
/// ADR-0003: **every state that activates the pump or a valve must deactivate them on exit**,
/// because the next state's entry may not run if an error interrupts the transition
/// (AG-REPO-21).
#[allow(clippy::match_same_arms)]
// Justification: the eighteen arms are kept separate on purpose. Several states do the same
// thing, but each is a different state's contract, and a change to one must not silently
// change another. Merging them would make ADR-0003's table untestable per row.
#[must_use]
pub fn on_exit(state: MachineState, machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    let mut fx = Effects::new();
    match state {
        MachineState::Init => {}

        // Pump only, no valve: this state never opens the water valve itself, but its update
        // runs the pump for hot-water dispensing, so leaving it must stop the pump.
        MachineState::PidNormal => {
            fx.push(Effect::DisablePump);
        }

        MachineState::PidDisabled => {}

        // Pump off only: the valve stays open to hold puck pressure through the pause and the
        // main phase, and closes in `BREW_RUNNING`'s exit or from the valve fail-safe on an
        // abort.
        //
        // On the *abort* path (`BREW_PREINFUSION` -> `PID_NORMAL`) neither applies: this state's
        // successor never ran its own update, so the valve stays open until the fail-safe closes
        // it later in the same tick. `test_state_flow_integration`'s
        // `BrewAbortDuringPreinfusion` pins exactly this.
        MachineState::BrewPreinfusion => {
            fx.push(Effect::DisablePump);
        }

        MachineState::BrewPreinfusionPause => {
            fx.push(Effect::DisablePump);
        }

        MachineState::BrewRunning => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        MachineState::BrewFinished => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        MachineState::ManualFlushRunning => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        MachineState::SteamRunning => {
            machine.steam_mode = false;
            machine.steam_first_on = false;
            fx.push(Effect::SetSteamMode { enabled: false });
            // The pump runs for water injection in this state, so leaving steam mode must stop
            // it.
            fx.push(Effect::DisablePump);
        }

        MachineState::BackflushIdle => {}

        MachineState::BackflushFilling => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        MachineState::BackflushFlushing => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        MachineState::BackflushFinished => {}

        MachineState::WaterTankEmpty => {}

        MachineState::EmergencyStop => {
            // Entry forced the runtime PID off and shut the hardware down. Recovery restores the
            // flag from the configuration, so the machine resumes heating instead of staying in
            // `PID_DISABLED` as though the PID had been turned off by hand.
            let enabled = ctx.pid_config_enabled();
            machine.pid.runtime_enabled = enabled;
            fx.push(Effect::SetPidRuntime { enabled });
        }

        MachineState::Standby => {
            // Waking the display is the only effect leaving standby has of its own; the PID
            // comes back at the configured preference.
            fx.push(Effect::WakeDisplay);
            let enabled = ctx.pid_config_enabled();
            machine.pid.runtime_enabled = enabled;
            fx.push(Effect::SetPidRuntime { enabled });
        }

        MachineState::SensorError => {
            machine.error_since = None;
        }

        MachineState::EepromError => {
            machine.error_since = None;
        }
    }
    fx
}

/// What a state does on every iteration.
///
/// ADR-0003: reinforce the hardware here, as a defence against a safety check having switched it
/// off. A fail-safe may do that between ticks, and the valve check runs after this in the same
/// tick, so a state that asserts "valve open" and a check that asserts "valve closed" are
/// both applied and the *whitelist* decides -- not which call came last.
#[allow(clippy::match_same_arms)]
// Justification: the eighteen arms are kept separate on purpose. Several states do the same
// thing, but each is a different state's contract, and a change to one must not silently
// change another. Merging them would make ADR-0003's table untestable per row.
#[must_use]
pub fn update(state: MachineState, machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    let mut fx = Effects::new();
    match state {
        MachineState::Init => {}

        // Hot-water dispensing has no state of its own: the water switch drives the pump
        // directly while the machine is in this state.
        MachineState::PidNormal => {
            if machine.switches.hot_water {
                fx.push(Effect::EnablePump);
            } else {
                fx.push(Effect::DisablePump);
            }
        }

        // Drain only while the PID stays disabled. An unconditional drain would eat the very
        // request that is about to bring the PID back.
        MachineState::PidDisabled => {
            if !machine.is_pid_runtime_enabled() {
                machine.requests.clear_all();
                fx.push(Effect::ClearActionRequests);
            }
        }

        MachineState::BrewPreinfusion => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
            machine.brew.elapsed_ms = f64::from(machine.state_elapsed_ms());
        }

        MachineState::BrewPreinfusionPause => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::OpenWaterValve);
            machine.brew.elapsed_ms = preinfusion_ms(ctx) + f64::from(machine.state_elapsed_ms());
        }

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

        MachineState::BrewFinished => {}

        MachineState::ManualFlushRunning => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
        }

        // The same water switch, repurposed: in steam mode it injects water into the boiler.
        // `test_steam_water_injection` is the test.
        MachineState::SteamRunning => {
            if machine.switches.hot_water {
                fx.push(Effect::EnablePump);
            } else {
                fx.push(Effect::DisablePump);
            }
        }

        MachineState::BackflushIdle => {}

        // Reinforced every tick: §39 of `docs/history/divergences.md` records that this firmware
        // re-asserts the pump and valve here rather than trusting the entry effects to survive.
        MachineState::BackflushFilling => {
            fx.push(Effect::EnablePump);
            fx.push(Effect::OpenWaterValve);
        }

        // Reinforced every tick, for the same reason as the fill phase above (§39).
        MachineState::BackflushFlushing => {
            fx.push(Effect::DisablePump);
            fx.push(Effect::CloseWaterValve);
        }

        MachineState::BackflushFinished => {}

        // No heater release, deliberately: with `keep_heater_on_empty` the heater is *supposed*
        // to keep running, which is why this state's standby timeout has to keep working.
        MachineState::WaterTankEmpty => {}

        // Re-runs the full emergency shutdown **every tick**, not just on entry: the latch
        // lives in the actuator facade, and something could re-energise a relay between ticks.
        //
        // It also drains on every tick, which the entry drain alone cannot do: a request that
        // arrives *during* the latch is set after entry has run, and without this it survives to
        // the recovery tick. Measured on a bench ESP32 on 2026-10-07 -- the machine tripped,
        // refused the press with `enablePump REFUSED - latched 1`, and then began that same brew
        // with nobody pressing anything once the probe cooled below the threshold.
        //
        // The drain is **unguarded**, and that is deliberate. Guarding it on the latch looks
        // right and is not: the exit test is `cc_safety::can_clear(temperature)`, and
        // `cc_safety::reduce` clears the latch earlier in the *same* control period, so on the
        // recovery tick the latch is already false, a guard would skip the drain, and a request
        // that arrived in that window would survive into `PID_NORMAL` and start a brew. One tick
        // wide, and the whole point of the drain is to close exactly that tick.
        //
        // Nothing downstream of this state wants a preserved request, and the "never drain
        // wake-up signals" rule is scoped to `STANDBY`, which is untouched;
        // [`Requests::clear_all`](crate::machine::Requests::clear_all) deliberately spares
        // `standby`. §36 of `docs/history/divergences.md` records the drain.
        MachineState::EmergencyStop => {
            fx.push(Effect::EmergencyShutdown);
            machine.pid.runtime_enabled = false;
            fx.push(Effect::SetPidRuntime { enabled: false });
            machine.requests.clear_all();
            fx.push(Effect::ClearActionRequests);
        }

        // The heater is off because the PID gate excludes `STANDBY`, not because anything
        // switches it off here.
        MachineState::Standby => {}

        MachineState::SensorError => {}

        MachineState::EepromError => {}
    }

    let _ = ctx;
    fx
}

/// What a state wants to transition to.
///
/// Reached only when no global guard fired: [`crate::guards::global_guard`] runs first and
/// answers for every state it covers.
///
/// The return type is `Option<MachineState>`: `None` means "stay put", and a `Some` equal to
/// the current state is still a self-transition, which the caller discards. The flags a state
/// acts on are cleared here, before the answer is returned, so even a discarded transition
/// consumes its flag.
#[allow(clippy::too_many_lines)]
// Justification: the eighteen arms are the specification, and the order in which each state
// tests its conditions decides which flag wins when several are set. Splitting them across
// private helpers would hide that order.
#[must_use]
pub fn check_specific(
    state: MachineState,
    machine: &mut Machine,
    ctx: &Context<'_>,
) -> Option<MachineState> {
    match state {
        MachineState::Init => {
            // This reads the **runtime** flag rather than the configured preference, which is
            // what makes `INIT` the recovery state for an emergency stop.
            Some(if machine.is_pid_runtime_enabled() {
                MachineState::PidNormal
            } else {
                MachineState::PidDisabled
            })
        }

        MachineState::PidNormal => {
            // The global guard excludes this state, so this is the *only* place the machine
            // notices that the PID was disabled while in it. `test_pid_state_transitions`
            // covers it.
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
            // Hot water has no transition of its own: the water switch's effect is in this
            // state's update.
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

        // A sleep request is honoured here, which is a deliberate divergence.
        MachineState::PidDisabled => {
            if machine.is_pid_runtime_enabled() {
                return Some(MachineState::PidNormal);
            }
            // Without this arm the state's only other exit is a *timeout*, so a machine whose PID
            // is off could not be put to sleep on request at all. Measured on hardware:
            // `POST /api/sleep` returned `202 {"accepted":true}`, the command reached the control
            // task (`control: command Sleep`), and the machine sat in `PID_DISABLED`
            // indefinitely. The UI offers a sleep button unconditionally, and `pid.enabled`
            // defaults to `false`, so that is the out-of-the-box path. §16 of
            // `docs/history/divergences.md` records the decision.
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

        MachineState::BrewPreinfusion => {
            if let Some(target) = check_brew_stop_request(machine, ctx) {
                return Some(target);
            }
            // A manual shot skips pre-infusion entirely.
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

        MachineState::BrewPreinfusionPause => {
            if let Some(target) = check_brew_stop_request(machine, ctx) {
                return Some(target);
            }
            // A manual shot has no pause, so it cannot legitimately reach this state; the branch
            // is the defence rather than the path.
            if !ctx.brew_is_automatic() {
                return Some(MachineState::BrewRunning);
            }
            let pause_ms = secs_to_u32(ctx.config.brew.pre_infusion.pause);
            if machine.state_timeout_elapsed(pause_ms) {
                return Some(MachineState::BrewRunning);
            }
            None
        }

        MachineState::BrewRunning => {
            // A stop here goes to `BREW_FINISHED`, not to the PID pair: the other brew states
            // go straight back, and this one is the exception, so a stop in the main phase
            // still records the shot and shows the result for three seconds.
            if machine.requests.brew_stop {
                machine.requests.brew_stop = false;
                return Some(MachineState::BrewFinished);
            }
            // Stopping by time or by weight is automatic-mode only.
            if ctx.brew_is_automatic() {
                let target = machine.brew.target_ms;
                if ctx.config.brew.by_time.enabled
                    && target > 0.0
                    && machine.brew.elapsed_ms >= target
                {
                    return Some(MachineState::BrewFinished);
                }
                let target_weight = ctx.config.brew.by_weight.target_weight;
                #[allow(clippy::cast_possible_truncation)]
                // Justification: the configured target is a `double` and the reading is an
                // `f32`, so the comparison truncates. The target is displayed to one decimal,
                // which is coarser than the truncation, so the difference cannot be seen.
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

        MachineState::BrewFinished => {
            if machine.requests.brew_start {
                machine.requests.brew_start = false;
                return Some(MachineState::BrewPreinfusion);
            }
            // How long the result stays on the panel. [`timing`] names the constant so it cannot
            // be confused with a configured duration.
            if machine.state_timeout_elapsed(timing::BREW_FINISHED_DISPLAY_TIMEOUT_MS) {
                return Some(pid_state(ctx));
            }
            None
        }

        MachineState::SteamRunning => {
            if machine.requests.steam_stop {
                machine.requests.steam_stop = false;
                return Some(pid_state(ctx));
            }
            // Only the flag is consulted, never the switch level: the handler layer has already
            // turned the level into a flag, which is what keeps a reading and the decision it
            // feeds in the same tick.
            None
        }

        MachineState::ManualFlushRunning => {
            if machine.requests.manual_flush_stop {
                machine.requests.manual_flush_stop = false;
                // A manual flush is only reachable from backflush mode, so it goes back there.
                return Some(if machine.is_backflush_mode_active() {
                    MachineState::BackflushIdle
                } else {
                    pid_state(ctx)
                });
            }
            None
        }

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

        // No emergency arm here, and none in the EEPROM state either: the global guard tests
        // emergency first and with no exclusion, so an arm for it could never run.
        MachineState::WaterTankEmpty => {
            if machine.is_water_tank_full() {
                return Some(pid_state(ctx));
            }
            if machine.requests.standby {
                machine.requests.standby = false;
                return Some(MachineState::Standby);
            }
            // With `keep_heater_on_empty` the heater may still be running in this state, so the
            // standby timeout has to keep working: otherwise the heater runs unattended for as
            // long as the tank stays empty.
            initialize_standby_if_needed(machine, ctx);
            if machine.standby.should_enter(ctx.config.standby.enabled) {
                return Some(MachineState::Standby);
            }
            None
        }

        MachineState::SensorError => {
            // The delay is measured from entry, not from the moment the fault cleared: the sensor
            // guard has no exclusion, so while the fault stands this code is not reached and
            // `error_since` keeps the value entry wrote. What that costs is recorded on
            // `parity_findings::the_sensor_error_recovery_delay_is_measured_from_entry`.
            // Strictly `>`, not `>=`: a full period, not one tick short of it.
            if error_duration_ms(machine) > timing::ERROR_RECOVERY_DELAY_MS {
                return Some(pid_state(ctx));
            }
            // Never `PID_DISABLED`. A persistent sensor fault needs an operator to look at it,
            // and turning temperature control off would hide the fault rather than report it.
            None
        }

        MachineState::EepromError => {
            let error_duration = error_duration_ms(machine);
            // The recovery timeout leads to `PID_DISABLED` **unconditionally**, not to
            // [`pid_state`]. A machine whose storage cannot be read comes back with temperature
            // control off, and only a power-switch press brings it back.
            if error_duration > timing::EEPROM_RECOVERY_TIMEOUT_MS {
                return Some(MachineState::PidDisabled);
            }
            None
        }

        MachineState::EmergencyStop => {
            // Recovery goes through `INIT` rather than straight to the PID pair: `INIT` reads the
            // runtime flag, which the exit above has just restored from the configuration.
            if is_emergency_cleared(machine) {
                return Some(MachineState::Init);
            }
            None
        }

        MachineState::Standby => {
            if machine.requests.normal_operation {
                machine.requests.normal_operation = false;
                return Some(wake_from_standby(machine, ctx));
            }
            // A brew or steam request wakes the machine: it is an intentional wake trigger, which
            // is why the entry drain spares start flags.
            //
            // The hot-water switch does **not** wake it. A press sets
            // [`Machine::hot_water_activity`], which resets the standby countdown and nothing
            // else, so a hold on the water switch in standby produces no pump and no wake.
            if machine.requests.brew_start || machine.requests.steam_start {
                return Some(wake_from_standby(machine, ctx));
            }
            None
        }
    }
}

/// The state to wake into, which is the same for both wake reasons.
fn wake_from_standby(machine: &mut Machine, ctx: &Context<'_>) -> MachineState {
    let _ = machine;
    pid_state(ctx)
}

/// Whether the emergency latch may clear, given the current temperature.
///
/// The reading is validated first -- a shorted or absent probe does not clear the latch -- and
/// neither does any temperature above [`cc_safety::EMERGENCY_SAFE_TEMP_C`]. The comparison is
/// inclusive, so **exactly** the safe temperature clears. [`cc_safety::can_clear`] is the whole
/// rule.
#[must_use]
pub fn is_emergency_cleared(machine: &Machine) -> bool {
    cc_safety::can_clear(machine.temperature())
}

/// Milliseconds since the current error state was entered.
///
/// `0` before entry has stamped the start time -- unreachable, because entry is what sets it and
/// exit is the only thing that clears it.
fn error_duration_ms(machine: &Machine) -> u32 {
    match machine.error_since {
        Some(since) => machine.now.since(since).raw(),
        None => 0,
    }
}

/// Start the standby countdown if it has not been started, and only when standby is enabled.
///
/// A pure mutation of [`Machine::standby`], so it emits no effect: this is a timer being armed
/// rather than user activity, and [`Effect::ResetStandbyTimer`] would tell the applier that an
/// operator had done something.
fn initialize_standby_if_needed(machine: &mut Machine, ctx: &Context<'_>) {
    if !ctx.config.standby.enabled {
        return;
    }
    if machine.standby.started_at.is_none() {
        machine.standby.reset(machine.now, ctx.standby_timeout_ms());
    }
}

/// The configured pre-infusion time, in milliseconds.
///
/// **Not** gated on `brew.pre_infusion.enabled`: the pause and running states add the configured
/// time to their elapsed timer whether or not pre-infusion is on. Harmless -- a disabled
/// pre-infusion skips the pre-infusion state entirely, so the pause state is unreachable -- and
/// the arithmetic is visible on the brew timer, so it is left as it is rather than special-cased.
fn preinfusion_ms(ctx: &Context<'_>) -> f64 {
    ctx.config.brew.pre_infusion.time * 1000.0
}

/// Seconds to whole milliseconds, saturating rather than panicking.
///
/// An absurdly large duration lands on `u32::MAX`, which reads as "already elapsed" at every
/// comparison, and a `NaN` lands on 0.
#[allow(clippy::cast_possible_truncation)]
// Justification: the truncation to whole milliseconds is the intended result; a fractional
// millisecond has no representation here.
#[allow(clippy::cast_sign_loss)]
// Justification: `secs_to_u32` checks `seconds > 0.0` before calling this, so the value cast is
// known to be non-negative.
fn saturating_millis(seconds: f64) -> u32 {
    (seconds * 1000.0) as u32
}

/// A negative duration means "already elapsed", so it clamps to 0 rather than wrapping.
fn secs_to_u32(seconds: f64) -> u32 {
    if seconds <= 0.0 {
        return 0;
    }
    saturating_millis(seconds)
}
