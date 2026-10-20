//! The four switch handlers, as pure edge-to-flag functions.
//!
//! # Why a handler is a function here
//!
//! A handler's whole observable contract is "given an edge, the current state and the
//! configuration, which flags does it set?". That is a pure function, and it is one here: the
//! previous switch reading is [`Machine::switches`], the configuration arrives through a
//! [`Context`] field, and the flags it sets are [`Machine::requests`]. Hardware stays out of
//! reach, which is what makes every one of these decisions a host test.
//!
//! # What this module deliberately does *not* do
//!
//! It does not touch an actuator. [`pump_timeouts`] is the only handler that produces a
//! hardware effect, and it produces an `Effect` like everything else: the applier is still the
//! only thing that writes a pin.
//!
//! # The permission layer
//!
//! Each handler checks, in order: the switch is enabled, the state permits the handler, and the
//! switch exists. `brew_switch` and `hot_water_switch` both refuse in `WATER_TANK_EMPTY`.
//! "The switch exists" needs no representation in Rust: a switch that does not exist produces
//! no event at all.
use cc_domain::hardware::SwitchType;
use cc_domain::state::MachineState;

use crate::context::Context;
use crate::effect::{Effect, Effects, PumpWatchdog};
use crate::event::{Event, SwitchId};
use crate::machine::{Machine, Request};

/// Apply one switch edge or command, and produce the effects that follow immediately.
///
/// The caller has already updated [`Machine::switches`] and [`Machine::now`], and has already
/// applied [`Event::Safety`] and [`Event::PidOutput`]. This is the handler layer: a switch
/// edge becomes request flags, and commands from outside become the same thing.
#[must_use]
pub fn apply_input(machine: &mut Machine, ctx: &Context<'_>, ev: Event) -> Effects {
    match ev {
        Event::ButtonPressed { switch, .. } | Event::ButtonReleased { switch } => {
            handler(machine, ctx, switch)
        }
        Event::Command(cmd) => apply_command(machine, ctx, cmd),
        // A command is also how an external caller applies a backflush-mode
        // change, which is a three-way decision rather than a flag.
        _ => Effects::new(),
    }
}

/// Run the handler for one switch.
fn handler(machine: &mut Machine, ctx: &Context<'_>, switch: SwitchId) -> Effects {
    match switch {
        SwitchId::Brew => brew_switch(machine, ctx),
        SwitchId::Steam => steam_switch(machine, ctx),
        SwitchId::Power => power_switch(machine, ctx),
        SwitchId::HotWater => hot_water_switch(machine, ctx),
    }
}

/// Set a request flag, and re-arm the standby countdown if that flag is one of the ones that
/// does.
///
/// [`Request::resets_standby_timer`] is the table of which flags count as user activity, and
/// this is the one function that applies it, so the table and the behaviour cannot drift.
///
/// A disabled standby produces no effect, and with it no [`Effect::ResetStandbyTimer`].
fn set_request(machine: &mut Machine, ctx: &Context<'_>, request: Request) -> Effects {
    machine.requests.set(request, true);
    if request.resets_standby_timer() && ctx.config.standby.enabled {
        machine.standby.reset(machine.now, ctx.standby_timeout_ms());
        return one(Effect::ResetStandbyTimer);
    }
    Effects::new()
}

/// The brew switch, which is also the backflush switch and the manual-flush switch.
///
/// The most tangled of the four: which flag the switch sets depends on the state. The order of
/// the tests is the contract, because it decides which flag wins when more than one could
/// apply:
///
/// ```text
/// if backflush mode is on, or the state is a backflush or manual-flush state {
///     ... backflush / manual-flush paths ...
///     return;                       // <-- early return
/// }
/// ... normal brew paths ...
/// ```
fn brew_switch(machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    if !ctx.brew_switch_enabled() {
        return Effects::new();
    }

    // Permission is denied in WATER_TANK_EMPTY, before the switch is even read. This is the
    // second layer of the tank interlock -- the first is the global tank guard, which moves
    // the machine to WATER_TANK_EMPTY -- and it is why a pressed switch sets no flags here.
    if machine.state == MachineState::WaterTankEmpty {
        return Effects::new();
    }

    let pressed = machine.switches.brew;
    let switch_type = ctx.brew_switch_type();
    let mut fx = Effects::new();

    // The backflush / manual-flush branch, entered when backflush *mode* is on or the state
    // is a backflush or manual-flush state.
    if machine.is_backflush_mode_active()
        || machine.state.is_backflush_state()
        || machine.state.is_manual_flush_state()
    {
        if pressed {
            if machine.state == MachineState::BackflushIdle && machine.switches.brew_long_press {
                // Long press in BACKFLUSH_IDLE = manual flush.
                fx.extend(&set_request(machine, ctx, Request::ManualFlushStart));
            } else if machine.state == MachineState::BackflushIdle
                || machine.state == MachineState::BackflushFinished
            {
                fx.extend(&set_request(machine, ctx, Request::BackflushCycleStart));
            } else if machine.state.is_backflush_state() {
                fx.extend(&set_request(machine, ctx, Request::BackflushStop));
            }
        } else {
            // Switch released — stop manual flush or the backflush cycle.
            if machine.state.is_manual_flush_state() {
                machine.requests.manual_flush_stop = true;
            } else if switch_type == SwitchType::Toggle
                && machine.state.is_backflush_state()
                && machine.state != MachineState::BackflushIdle
                && machine.state != MachineState::BackflushFinished
            {
                fx.extend(&set_request(machine, ctx, Request::BackflushStop));
            }
        }
        return fx;
    }

    // The normal brew paths.
    if pressed {
        if switch_type == SwitchType::Momentary {
            // A momentary press starts a brew unless one is already running; a second press
            // while brewing stops it.
            if !machine.state.is_brew_state() || machine.state == MachineState::BrewFinished {
                fx.extend(&set_request(machine, ctx, Request::BrewStart));
            } else {
                machine.requests.brew_stop = true;
            }
        } else if !machine.state.is_brew_state() || machine.state == MachineState::BrewFinished {
            // A toggle switched on starts a brew unless one is already running. A toggle
            // left on during a brew requests nothing.
            fx.extend(&set_request(machine, ctx, Request::BrewStart));
        }
    } else if switch_type == SwitchType::Toggle
        && machine.state.is_brew_state()
        && machine.state != MachineState::BrewFinished
    {
        // A toggle switched off stops the brew.
        machine.requests.brew_stop = true;
    }
    // A momentary release does not stop the brew: the second press does.
    fx
}

/// The steam switch.
///
/// The one surprising branch is the standby one: with a **toggle** steam switch, a machine in
/// standby that has been left with the switch on would otherwise request steam start on every
/// later reading, because only a rising edge counts.
///
/// Note the consequence, which is worth stating because it looks like a bug: waking a sleeping
/// machine with the steam switch already on does **not** steam. The operator has to toggle it
/// off and on again. Deliberate.
fn steam_switch(machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    if !ctx.steam_switch_enabled() {
        return Effects::new();
    }

    let pressed = machine.switches.steam;
    let switch_type = ctx.steam_switch_type();
    let mut fx = Effects::new();

    if pressed {
        if switch_type == SwitchType::Momentary {
            if machine.state == MachineState::SteamRunning {
                machine.requests.steam_stop = true;
            } else {
                fx.extend(&set_request(machine, ctx, Request::SteamStart));
            }
        } else if machine.state == MachineState::Standby {
            // In standby only a rising edge counts, so a left-on toggle does not wake steam.
            // The switch levels hold the *current* reading and the previous one is already
            // overwritten by the time the edge arrives, so "rising edge" is exactly "this
            // event is a press", which is what routes the call here.
            fx.extend(&set_request(machine, ctx, Request::SteamStart));
        } else if machine.state != MachineState::SteamRunning {
            fx.extend(&set_request(machine, ctx, Request::SteamStart));
        }
    } else if switch_type == SwitchType::Toggle && machine.state == MachineState::SteamRunning {
        machine.requests.steam_stop = true;
    }
    fx
}

/// The power switch, in one of two shapes chosen by `hardware.switches.power.type`.
///
/// A momentary switch acts on the press and the release; a toggle switch acts on the position
/// change.
fn power_switch(machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    if !ctx.power_switch_enabled() {
        return Effects::new();
    }

    match ctx.power_switch_type() {
        SwitchType::Toggle => toggle_power(machine, ctx),
        SwitchType::Momentary => momentary_power(machine, ctx),
    }
}

/// Record the boot time, once.
///
/// Called from the tick pass rather than from a switch edge: the boot time has to be set on
/// the **first** pass, before any press, and the reducer only runs a handler when there is an
/// edge. Gated on the handler being enabled, like every other handler.
pub fn record_power_handler_initialisation(machine: &mut Machine, ctx: &Context<'_>) {
    if !ctx.power_switch_enabled() {
        return;
    }
    machine.boot_at.get_or_insert(machine.now);
}

/// The power switch as a toggle switch: the position *is* the state.
///
/// The reducer is handed only edges, so "the toggle was sampled at an unchanged position" is
/// inexpressible here rather than merely unlikely.
fn toggle_power(machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    if machine.switches.power {
        power_on(machine, ctx)
    } else {
        power_off(machine)
    }
}

/// The power switch as a momentary switch: the press.
///
/// The press does two things, in order:
///
/// 1. arm long-press tracking, but only once the machine has been up for
///    [`timing::POWER_SWITCH_SETTLE_MS`](crate::timing::POWER_SWITCH_SETTLE_MS);
/// 2. toggle power: `STANDBY` -> on, anything else -> off.
///
/// The release clears the tracking, and the reboot is evaluated by [`long_press_reboot`] on
/// every tick rather than here, because a hold has to be noticed while the switch stays down.
///
/// The settle window gates **long-press tracking only**. The power toggle itself is not gated,
/// so a momentary press during the first five seconds after boot still switches the machine
/// off. Deliberate: "the machine ignores the power switch for five seconds after boot" would
/// be a support call.
fn momentary_power(machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    let settled = power_settle_elapsed(machine);
    let mut fx = Effects::new();
    if machine.switches.power {
        if settled {
            machine.power_press_started_at = Some(machine.now);
        }
        if machine.state == MachineState::Standby {
            fx.extend(&power_on(machine, ctx));
        } else {
            fx.extend(&power_off(machine));
        }
    } else {
        // The release stops the hold being tracked.
        machine.power_press_started_at = None;
    }
    fx
}

/// Whether the machine has been up long enough for a press to be tracked: strictly more than
/// [`timing::POWER_SWITCH_SETTLE_MS`](crate::timing::POWER_SWITCH_SETTLE_MS) since boot.
fn power_settle_elapsed(machine: &Machine) -> bool {
    match machine.boot_at {
        None => false,
        Some(boot) => machine.now.since(boot).raw() > crate::timing::POWER_SWITCH_SETTLE_MS,
    }
}

/// Whether a held power switch should reboot the machine.
///
/// Four conditions, all required: the switch is pressed; the machine has been up past the
/// settle window (`power_settle_elapsed`); a press is being tracked in
/// [`Machine::power_press_started_at`]; the hold has lasted longer than
/// [`timing::POWER_LONG_PRESS_REBOOT_MS`](crate::timing::POWER_LONG_PRESS_REBOOT_MS); and the
/// switch layer reports the hardware long press.
///
/// # Where this runs
///
/// On **every** tick, not on the press edge: the hold lasts about a second, so by the time the
/// duration condition is true the edge is long gone. That is the whole reason this is a
/// tick-time check and not a switch-time one.
#[must_use]
pub fn long_press_reboot(machine: &Machine) -> Effects {
    let pressed = machine.switches.power;
    let settled = power_settle_elapsed(machine);
    let held_long_enough = match machine.power_press_started_at {
        None => false,
        Some(started) => {
            machine.now.since(started).raw() > crate::timing::POWER_LONG_PRESS_REBOOT_MS
        }
    };
    if pressed && settled && held_long_enough && machine.switches.power_long_press {
        one(Effect::RequestReboot)
    } else {
        Effects::new()
    }
}

/// The power switch's "on" path, from `STANDBY` or `PID_DISABLED` only:
///
/// ```text
/// normal operation is requested;
/// the runtime PID is enabled;
/// the display is woken.
/// ```
///
/// Three consequences rather than one, because they are three different things: the request
/// flag is what the state machine consumes, the runtime flag is what the PID acts on, and the
/// display wake is a user-visible change of its own.
///
/// Note the state gate: the power switch does nothing at all in `PID_NORMAL`, `BREW_RUNNING`
/// or any other state. An operator who toggles the power switch while brewing gets no response
/// and the machine keeps brewing. Deliberate.
fn power_on(machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    if machine.state != MachineState::Standby && machine.state != MachineState::PidDisabled {
        return Effects::new();
    }
    let mut fx = set_request(machine, ctx, Request::NormalOperation);
    // The runtime flag, and the mirror effect that reports it. The persisted `pid.enabled` is
    // the parameter path's write, and the control task pushes this command when it drains that
    // write, so the two cannot diverge.
    machine.pid.runtime_enabled = true;
    fx.push(Effect::SetPidRuntime { enabled: true });
    fx.push(Effect::WakeDisplay);
    fx
}

/// The power switch's "off" path, from any state but `STANDBY`:
///
/// ```text
/// the hardware is shut down safely;
/// standby is requested;
/// the standby timer is expired.
/// ```
///
/// Note the ordering: the safe shutdown happens **before** the standby request, and the
/// request is not consumed until the next state-machine pass. So for one tick the hardware is
/// already off while the state is still `PID_NORMAL`, and in that tick the state's own update
/// will enable the pump if the water switch is held. Narrow, real, and deliberate.
///
/// The expired timer is [`StandbyTimer::expire_now`](crate::machine::StandbyTimer::expire_now),
/// so the standby transition happens on the very next pass regardless of `standby.time` --
/// which is the point: the power switch is immediate and the configured timeout is not.
fn power_off(machine: &mut Machine) -> Effects {
    if machine.state == MachineState::Standby {
        return Effects::new();
    }
    machine.requests.standby = true;
    machine.standby.expire_now();
    one(Effect::SafeHardwareShutdown)
}

/// The hot-water switch.
///
/// It sets **no** request flag. [`Machine::hot_water_activity`] is what it marks, which resets
/// the standby timer and nothing else; the pumping itself is done by the states that read the
/// switch level. The tank permission layer denies the whole handler in `WATER_TANK_EMPTY`.
///
/// # It does not wake the machine
///
/// Nothing reads [`Machine::hot_water_activity`] as a wake trigger, so pressing the water
/// switch while in standby resets the standby countdown and does nothing else: no pump, no
/// wake. Deliberate.
fn hot_water_switch(machine: &mut Machine, ctx: &Context<'_>) -> Effects {
    if !ctx.hot_water_switch_enabled() {
        return Effects::new();
    }
    if machine.state == MachineState::WaterTankEmpty {
        return Effects::new();
    }
    machine.hot_water_activity = true;
    // Marking hot-water activity resets the standby timer, which is a no-op when standby is
    // disabled.
    if ctx.config.standby.enabled {
        machine.standby.reset(machine.now, ctx.standby_timeout_ms());
        return one(Effect::ResetStandbyTimer);
    }
    Effects::new()
}

/// The two pump watchdogs, run on every tick.
///
/// # A deliberate difference
///
/// Both watchdogs are **armed on the activating edge** (see [`arm_pump_watchdogs`]), so a
/// five-minute brew or a one-minute hot-water run is stopped rather than running until the
/// switch is released. A trip is announced through [`Effect::PumpTimeoutFired`] before the
/// action it causes, so the field log says why the machine stopped.
///
/// # What counts as "the activating edge"
///
/// Arming happens where the pump is actually **commanded on**, not where the state merely
/// *permits* it:
///
/// | watchdog | armed while |
/// | --- | --- |
/// | brew | the current state is one whose update pushes `Effect::EnablePump` -- `BREW_PREINFUSION` or `BREW_RUNNING` |
/// | hot water | the water switch is held in a state that dispenses hot water -- `PID_NORMAL` |
///
/// `BREW_PREINFUSION_PAUSE` deliberately does **not** arm the brew watchdog: its update pushes
/// `Effect::DisablePump`, so a pause is not pump run time. [`brew_is_active`] *does* accept
/// that state, which is exactly why arming on the state alone would count the pause against
/// the five minutes.
///
/// The hot-water watchdog is armed on the switch, not on the state: its expiry test is
/// "expired **and** the water switch is still held". The state is in the arming condition so
/// that `STEAM_RUNNING` -- where the *same* switch means water injection, not hot water --
/// does not start the hot-water clock.
///
/// **Not covered, deliberately:** `MANUAL_FLUSH_RUNNING`, `BACKFLUSH_FILLING` and
/// `BACKFLUSH_FLUSHING` also run the pump, and neither watchdog covers them. Extending a
/// watchdog is a specification change rather than a fix, so it is recorded as a follow-up
/// instead of smuggled in here.
///
/// # The brew watchdog requests a stop, it does not transition
///
/// A tripped brew watchdog sets the stop request; the transition happens on the **next** pass.
/// `brew_pump_timeout_requests_a_stop_rather_than_transitioning` pins that, because a
/// same-tick transition would skip the running brew's exit ordering relative to the valve
/// safety check.
#[must_use]
pub fn pump_timeouts(machine: &mut Machine) -> Effects {
    let mut fx = Effects::new();

    arm_pump_watchdogs(machine);

    // Brew: expired, and the state is one a brew watchdog covers.
    if brew_timer_expired(machine) && brew_is_active(machine.state) {
        fx.push(Effect::PumpTimeoutFired {
            watchdog: PumpWatchdog::Brew,
        });
        machine.requests.brew_stop = true;
    }

    // Hot water: expired, and the switch that commands the pump is still held.
    if hot_water_timer_expired(machine) && machine.switches.hot_water {
        fx.push(Effect::PumpTimeoutFired {
            watchdog: PumpWatchdog::HotWater,
        });
        fx.push(Effect::DisablePump);
    }

    fx
}

/// Start and stop the two pump watchdogs on the pump-on / pump-off edges.
///
/// The deadline check itself is in [`pump_timeouts`] above; this only decides *when the clock
/// runs*, and it is deliberately a latch: a watchdog that re-armed itself every tick could
/// never expire.
///
/// A named `pub fn` so the arming rule is testable on its own, rather than four lines buried
/// in the tick.
pub fn arm_pump_watchdogs(machine: &mut Machine) {
    machine.brew_pump_started_at = if brew_pump_running(machine) {
        machine.brew_pump_started_at.or(Some(machine.now))
    } else {
        None
    };
    machine.hot_water_pump_started_at = if hot_water_pump_running(machine) {
        machine.hot_water_pump_started_at.or(Some(machine.now))
    } else {
        None
    };
}

/// Is the pump being commanded on by a *brew* state this loop?
fn brew_pump_running(machine: &Machine) -> bool {
    matches!(
        machine.state,
        MachineState::BrewPreinfusion | MachineState::BrewRunning
    )
}

/// Is the pump being commanded on by the hot-water switch this tick?
///
/// `PID_NORMAL` is the only state in which the water switch means "hot water". In
/// `STEAM_RUNNING` the same switch means water injection, which the hot-water watchdog does
/// not cover.
fn hot_water_pump_running(machine: &Machine) -> bool {
    machine.state == MachineState::PidNormal && machine.switches.hot_water
}

/// Whether the brew watchdog has expired.
fn brew_timer_expired(machine: &Machine) -> bool {
    match machine.brew_pump_started_at {
        // Never armed, so never expired.
        None => false,
        Some(started) => machine.now.since(started).raw() > crate::timing::BREW_PUMP_TIMEOUT_MS,
    }
}

/// Whether the hot-water watchdog has expired.
fn hot_water_timer_expired(machine: &Machine) -> bool {
    match machine.hot_water_pump_started_at {
        None => false,
        Some(started) => {
            machine.now.since(started).raw() > crate::timing::HOT_WATER_PUMP_TIMEOUT_MS
        }
    }
}

/// Whether the state is a brew in progress, as opposed to finished or unrelated.
#[must_use]
pub fn brew_is_active(state: MachineState) -> bool {
    state.is_brew_state() && state != MachineState::BrewFinished
}

/// Apply one `Command` from outside the switch layer.
///
/// The one entry point for external requests: the web UI, MQTT and the power handler's derived
/// actions all arrive here, so a request sets its flag the same way whichever asked for it.
fn apply_command(
    machine: &mut Machine,
    ctx: &Context<'_>,
    command: crate::event::Command,
) -> Effects {
    use crate::event::Command as C;
    let mut fx = Effects::new();
    match command {
        C::BrewStart => fx.extend(&set_request(machine, ctx, Request::BrewStart)),
        C::BrewStop => machine.requests.set(Request::BrewStop, true),
        C::SteamStart => fx.extend(&set_request(machine, ctx, Request::SteamStart)),
        C::SteamStop => machine.requests.set(Request::SteamStop, true),
        C::ManualFlushStart => machine.requests.set(Request::ManualFlushStart, true),
        C::ManualFlushStop => machine.requests.set(Request::ManualFlushStop, true),
        C::BackflushEnter => fx.extend(&apply_backflush_mode(machine, ctx, true)),
        // One command for both directions: the value *is* the request. `BackflushEnter` above
        // is kept because the cycle-count paths use it as the request flag.
        C::SetBackflushMode(active) => fx.extend(&apply_backflush_mode(machine, ctx, active)),
        C::BackflushCycleStart => {
            fx.extend(&set_request(machine, ctx, Request::BackflushCycleStart));
        }
        C::BackflushStop => fx.extend(&set_request(machine, ctx, Request::BackflushStop)),
        C::Standby => {
            // The web/MQTT standby path. Requesting standby does **not** reset the standby
            // timer.
            machine.requests.standby = true;
        }
        C::NormalOperation => {
            machine.requests.normal_operation = true;
            // Leaving standby *does* reset the timer.
            if ctx.config.standby.enabled {
                machine.standby.reset(machine.now, ctx.standby_timeout_ms());
                fx.push(Effect::ResetStandbyTimer);
            }
        }
        C::SetUserPidEnabled(enabled) => {
            machine.pid.runtime_enabled = enabled;
            fx.push(Effect::SetPidRuntime { enabled });
        }
        C::Reboot => {
            // The applier shows the message, shuts the hardware down, waits, and restarts:
            // the reducer only says what is wanted, because a pure function may not sleep.
            fx.push(Effect::RequestReboot);
        }
    }
    fx
}

/// Apply a backflush-mode request and write the outcome into the machine.
///
/// The mode toggle is the one external request that is a *decision* rather than a flag, so it
/// goes through [`crate::backflush::apply_backflush_mode`] first.
fn apply_backflush_mode(machine: &mut Machine, ctx: &Context<'_>, active: bool) -> Effects {
    let outcome = crate::backflush::apply_backflush_mode(
        machine.backflush.on,
        machine.backflush.cycle,
        active,
        ctx.backflush_cycles(),
        &machine.requests,
    );
    machine.backflush.on = outcome.on;
    machine.backflush.cycle = outcome.cycle;
    machine.requests.backflush_enter = outcome.enter_requested;
    machine.requests.backflush_cycle_start = outcome.cycle_start_requested;
    machine.requests.backflush_stop = outcome.stop_requested;

    if outcome.enter_requested {
        // Entering backflush mode counts as user activity, so it resets the standby timer and
        // goes through `set_request`.
        return set_request(machine, ctx, Request::BackflushEnter);
    }
    Effects::new()
}

/// A one-element [`Effects`]. A named helper rather than a `from_iter` at each
/// of the dozen call sites, so "this handler produced exactly one effect" reads
/// the same everywhere.
fn one(effect: Effect) -> Effects {
    core::iter::once(effect).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::SwitchLevels;
    use cc_domain::units::Celsius;

    use crate::event::Command as C;

    #[test]
    fn brew_is_active_excludes_brew_finished() {
        assert!(brew_is_active(MachineState::BrewPreinfusion));
        assert!(brew_is_active(MachineState::BrewRunning));
        assert!(!brew_is_active(MachineState::BrewFinished));
        assert!(!brew_is_active(MachineState::PidNormal));
    }

    #[test]
    fn an_unarmed_pump_timer_never_expires() {
        // Never armed, so never expired.
        let m = Machine::cold();
        assert!(!brew_timer_expired(&m));
        assert!(!hot_water_timer_expired(&m));
    }

    #[test]
    fn the_switch_levels_helper_is_used_for_the_bottle() {
        // A guard against a future refactor making the fields private without
        // updating the handlers.
        let levels = SwitchLevels::ALL_RELEASED;
        assert!(!levels.level(SwitchId::Brew));
    }

    #[test]
    fn the_backflush_mode_toggle_turns_the_mode_off_again() {
        // **The bug this pins.** The API toggle fed the mode-on command and the cycle-stop
        // command, and the cycle-stop stops a running cycle without clearing `backflush.on`.
        // So the toggle had no off switch: four presses on a bench ESP32 all answered
        // `{"backflushOn":true}` and the machine stayed in `BACKFLUSH_IDLE`.
        //
        // `SetBackflushMode(bool)` carries the value in both directions, so the off direction
        // exists.
        let config = cc_config::Config::default();
        let ctx = Context::new(&config, Celsius::new(95.0));
        let mut machine = Machine::cold();

        apply_command(&mut machine, &ctx, C::SetBackflushMode(true));
        assert!(machine.backflush.on, "the first press must turn mode on");

        apply_command(&mut machine, &ctx, C::SetBackflushMode(false));
        assert!(
            !machine.backflush.on,
            "the second press must turn mode OFF -- this is what the bench showed \
             stuck on forever"
        );

        // And it is a toggle, not a one-way door: back on again.
        apply_command(&mut machine, &ctx, C::SetBackflushMode(true));
        assert!(machine.backflush.on);
    }
}
