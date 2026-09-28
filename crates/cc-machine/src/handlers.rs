//! The four switch handlers, as pure edge-to-flag functions.
//!
//! # Why handlers are not "handlers"
//!
//! In the C++ a handler is an object with its own `lastSwitchReading_`, its own
//! `PumpTimer`, and a `process()` the loop calls every iteration
//! (`BaseHandler::process`, `BaseHandler.h:76-94`). Its output is *only* a set of
//! request flags on `MachineStateContext` — the comment at
//! `SteamStates.cpp:57-58` says so outright: "handlers set flags, states only
//! check flags. This fixes timing issues with direct hardware checks."
//!
//! So the handler's whole observable contract is "given an edge and the current
//! state and config, which flags does it set?". That is a pure function, and it
//! is one here. `lastSwitchReading_` becomes [`Machine::switches`], the config
//! lookup becomes a [`Context`] field, and the eleven flags become
//! [`Machine::requests`].
//!
//! # What this module deliberately does *not* do
//!
//! It does not touch an actuator. The only handler that reaches hardware in the
//! C++ is `HotWaterHandler::checkPumpTimeout` (`HotWaterHandler.h:114-122`),
//! which calls `context.disablePump()` — and it is dead, see
//! [`crate::timing::PUMP_TIMEOUTS_NEVER_ARM`]. It is ported anyway, as a
//! `DisablePump` effect, so that if the timer is ever armed the behaviour is
//! already right.
//!
//! # The `hasPermission` layer
//!
//! `BaseHandler::process` checks `isEnabled()`, then `hasPermission()`, then
//! `isHardwareValid()`, and `BrewHandler`/`HotWaterHandler` both refuse in
//! `WATER_TANK_EMPTY` (`BrewHandler.h:147-150`, `HotWaterHandler.h:64-67`).
//! `SwitchBasedHandler::isHardwareValid()` is `switch_ != nullptr`
//! (`BaseHandler.h:159-161`) — in Rust that is "the switch exists", which is the
//! same as "there is an event", so it needs no representation.

use alloc::vec::Vec;

use cc_domain::hardware::SwitchType;
use cc_domain::state::MachineState;

use crate::context::Context;
use crate::effect::Effect;
use crate::event::{Event, SwitchId};
use crate::machine::{Machine, Request};

/// Apply one switch edge or command, and produce the effects that follow
/// immediately.
///
/// The caller has already updated [`Machine::switches`] and [`Machine::now`] and
/// has already applied [`Event::Safety`] and [`Event::PidOutput`]; this is the
/// handler layer, which in the C++ is `LoopManager` step 3 plus the tail of
/// step 4 (`LoopManager.cpp:561-569` and `:616-619`).
#[must_use]
pub fn apply_input(machine: &mut Machine, ctx: &Context<'_>, ev: Event) -> Vec<Effect> {
    match ev {
        Event::ButtonPressed { switch, .. } | Event::ButtonReleased { switch } => {
            handler(machine, ctx, switch)
        }
        Event::Command(cmd) => apply_command(machine, ctx, cmd),
        // A command is also how an external caller applies a backflush-mode
        // change, which is a three-way decision rather than a flag.
        _ => Vec::new(),
    }
}

/// Run the handler for one switch.
///
/// Dispatches to the four handlers. Each one is the C++'s `processSwitchInput`,
/// minus the switch-object plumbing.
fn handler(machine: &mut Machine, ctx: &Context<'_>, switch: SwitchId) -> Vec<Effect> {
    match switch {
        SwitchId::Brew => brew_switch(machine, ctx),
        SwitchId::Steam => steam_switch(machine, ctx),
        SwitchId::Power => power_switch(machine, ctx),
        SwitchId::HotWater => hot_water_switch(machine, ctx),
    }
}

/// Set a request flag, and re-arm the standby countdown if that flag is one of
/// the ones that does.
///
/// The C++ spells this out eleven times: every `setXxxRequested(true)` that
/// counts as user activity calls `resetStandbyTimerOnUserActivity()`
/// (`MachineStateContext.cpp:217-267`), and the ones that do not are bare
/// assignments (`:483`, `:509`, `:536`, `:587`, `:523`). [`Request::resets_standby_timer`]
/// is that table and this is the one function that applies it.
///
/// `resetStandbyTimerOnUserActivity` forwards to `standbyCoordinator().reset()`,
/// which itself starts with `if (!standbyEnabled.get()) return;`
/// (`StandbyCoordinator.h:88`), so a disabled standby produces no effect.
fn set_request(machine: &mut Machine, ctx: &Context<'_>, request: Request) -> Vec<Effect> {
    machine.requests.set(request, true);
    if request.resets_standby_timer() && ctx.config.standby.enabled {
        machine.standby.reset(machine.now, ctx.standby_timeout_ms());
        return vec_of(Effect::ResetStandbyTimer);
    }
    Vec::new()
}

/// `BrewHandler::processSwitchInput` (`BrewHandler.h:165-252`).
///
/// The most tangled of the four: the brew switch is also the backflush switch
/// and the manual-flush switch, and which one it means depends on the state.
/// The C++'s structure is preserved exactly, including the order of the tests,
/// because the order is what decides which flag wins when several apply.
///
/// # The dispatch, transcribed
///
/// ```cpp
/// if (context->isBackflushModeActive() || isBackflushState(currentState) ||
///     isManualFlushState(currentState)) {
///     ... backflush / manual-flush paths ...
///     lastSwitchReading_ = reading;
///     return;                       // <-- early return
/// }
/// ... normal brew paths ...
/// ```
fn brew_switch(machine: &mut Machine, ctx: &Context<'_>) -> Vec<Effect> {
    if !ctx.brew_switch_enabled() {
        return Vec::new();
    }

    // `BrewHandler.h:147-150`: permission is denied in WATER_TANK_EMPTY, before
    // the switch is even read. This is the second layer of the tank interlock
    // (the first is `HardwareManager::enablePump`), and it is why
    // `test_brew_handler`'s `ProcessDeniesPermissionWhenWaterTankEmpty` passes
    // with a *pressed* switch.
    if machine.state == MachineState::WaterTankEmpty {
        return Vec::new();
    }

    let pressed = machine.switches.brew;
    let switch_type = ctx.brew_switch_type();
    let mut fx = Vec::new();

    // `BrewHandler.h:195-219`: the backflush / manual-flush branch, entered when
    // backflush *mode* is on or the state is a backflush or manual-flush state.
    if machine.is_backflush_mode_active()
        || machine.state.is_backflush_state()
        || machine.state.is_manual_flush_state()
    {
        if pressed {
            if machine.state == MachineState::BackflushIdle && machine.switches.brew_long_press {
                // Long press in BACKFLUSH_IDLE = manual flush.
                fx.extend(set_request(machine, ctx, Request::ManualFlushStart));
            } else if machine.state == MachineState::BackflushIdle
                || machine.state == MachineState::BackflushFinished
            {
                fx.extend(set_request(machine, ctx, Request::BackflushCycleStart));
            } else if machine.state.is_backflush_state() {
                fx.extend(set_request(machine, ctx, Request::BackflushStop));
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
                fx.extend(set_request(machine, ctx, Request::BackflushStop));
            }
        }
        return fx;
    }

    // `BrewHandler.h:222-247`: the normal brew paths.
    if pressed {
        if switch_type == SwitchType::Momentary {
            // "Momentary: press = start brew (if not already brewing)"; a second
            // press while brewing means stop.
            if !machine.state.is_brew_state() || machine.state == MachineState::BrewFinished {
                fx.extend(set_request(machine, ctx, Request::BrewStart));
            } else {
                machine.requests.brew_stop = true;
            }
        } else if !machine.state.is_brew_state() || machine.state == MachineState::BrewFinished {
            // "Toggle: activated = start brew (if not already brewing)". A
            // toggle set on during a brew requests nothing at all.
            fx.extend(set_request(machine, ctx, Request::BrewStart));
        }
    } else if switch_type == SwitchType::Toggle
        && machine.state.is_brew_state()
        && machine.state != MachineState::BrewFinished
    {
        // "Toggle: deactivated = stop brew (if brewing)".
        machine.requests.brew_stop = true;
    }
    // "Momentary: release doesn't trigger stop (handled by second press)".
    fx
}

/// `SteamHandler::processSwitchInput` (`SteamHandler.h:98-158`).
///
/// The one genuinely surprising branch is the standby one
/// (`SteamHandler.h:137-141`): with a **toggle** steam switch, a machine in
/// standby that has been left with the switch on will otherwise request steam
/// start on the next level read forever, because the C++ only looks at the edge
/// when `lastSwitchReading_ == LOW`. The comment says "In standby, only react
/// to a rising edge so a left-on toggle does not wake steam".
///
/// Note the consequence, which is worth stating because it looks like a bug:
/// waking a sleeping machine with the steam switch already on does **not** steam.
/// The operator has to toggle it off and on again. Preserved.
fn steam_switch(machine: &mut Machine, ctx: &Context<'_>) -> Vec<Effect> {
    if !ctx.steam_switch_enabled() {
        return Vec::new();
    }

    let pressed = machine.switches.steam;
    let switch_type = ctx.steam_switch_type();
    let mut fx = Vec::new();

    if pressed {
        if switch_type == SwitchType::Momentary {
            if machine.state == MachineState::SteamRunning {
                machine.requests.steam_stop = true;
            } else {
                fx.extend(set_request(machine, ctx, Request::SteamStart));
            }
        } else if machine.state == MachineState::Standby {
            // `SteamHandler.h:137-141`. The edge test: a press while in standby
            // only counts if the previous reading was LOW. `SwitchLevels.brew`
            // and `.steam` hold the *current* level, and the shell has already
            // overwritten the previous one, so the "rising edge" condition is
            // exactly "this event is a press" — which is what
            // `apply_input` routes here. See the note in `apply_input`'s caller
            // for why this is still not exactly the C++'s condition.
            fx.extend(set_request(machine, ctx, Request::SteamStart));
        } else if machine.state != MachineState::SteamRunning {
            fx.extend(set_request(machine, ctx, Request::SteamStart));
        }
    } else if switch_type == SwitchType::Toggle && machine.state == MachineState::SteamRunning {
        machine.requests.steam_stop = true;
    }
    fx
}

/// `PowerHandler::processTogglePowerSwitch` / `processMomentaryPowerSwitch`
/// (`PowerHandler.h:83-147`).
///
/// Two shapes, chosen by `hardware.switches.power.type`
/// (`PowerHandler.h:65-69`).
fn power_switch(machine: &mut Machine, ctx: &Context<'_>) -> Vec<Effect> {
    if !ctx.power_switch_enabled() {
        return Vec::new();
    }

    match ctx.power_switch_type() {
        SwitchType::Toggle => toggle_power(machine, ctx),
        SwitchType::Momentary => momentary_power(machine, ctx),
    }
}

/// `PowerHandler::recordSystemInitialization` (`PowerHandler.h:73-81`).
///
/// The first statement of `PowerHandler::processImpl`, so the C++ records the
/// boot time on the **first loop**, not on the first press. The reducer only
/// runs a handler on a switch edge, so this is called from the tick pass instead
/// — which is the same loop, and is the only place it would otherwise be missed.
///
/// Gated on the handler being enabled, because in the C++ the call is inside
/// `processImpl` and `isEnabled()` is checked first (`BaseHandler.h:77-80`).
pub fn record_power_handler_initialisation(machine: &mut Machine, ctx: &Context<'_>) {
    if !ctx.power_switch_enabled() {
        return;
    }
    machine.boot_at.get_or_insert(machine.now);
}

/// `PowerHandler::processTogglePowerSwitch` (`PowerHandler.h:83-95`).
///
/// A toggle switch's position is the state, so an edge is the state changing.
/// The C++ has to track `lastPowerSwitchPressed_` to notice; the reducer is
/// handed only edges, which makes the `ToggleSwitchNoChangeWhenSameState` case
/// inexpressible rather than merely unlikely.
fn toggle_power(machine: &mut Machine, ctx: &Context<'_>) -> Vec<Effect> {
    if machine.switches.power {
        power_on(machine, ctx)
    } else {
        power_off(machine)
    }
}

/// `PowerHandler::handlePowerButtonPress` / `handlePowerButtonRelease`
/// (`PowerHandler.h:116-140`).
///
/// The press does three things, in the C++'s order:
/// 1. arm long-press tracking, but only once the machine has been up for
///    `POWER_SWITCH_SETTLE_MS` (`PowerHandler.h:117-123`);
/// 2. toggle power: `STANDBY` → on, anything else → off
///    (`PowerHandler.h:125-132`);
/// 3. nothing else. The reboot is evaluated by [`long_press_reboot`] on every
///    loop, not here, because the C++'s
///    `checkForLongPressReboot(pressed, currentMillis)` runs from
///    `processMomentaryPowerSwitch` *after* `handlePowerButtonPress` and *also*
///    on loops where nothing changed.
///
/// The 5-second window gates **long-press tracking only**. The power toggle
/// itself is not gated, so a momentary press during the first five seconds after
/// boot still switches the machine off. Preserved: "the machine ignores the
/// power switch for five seconds after boot" would be a support call.
fn momentary_power(machine: &mut Machine, ctx: &Context<'_>) -> Vec<Effect> {
    let settled = power_settle_elapsed(machine);
    let mut fx = Vec::new();
    if machine.switches.power {
        if settled {
            machine.power_press_started_at = Some(machine.now);
        }
        if machine.state == MachineState::Standby {
            fx.extend(power_on(machine, ctx));
        } else {
            fx.extend(power_off(machine));
        }
    } else {
        // `PowerHandler::handlePowerButtonRelease` (`PowerHandler.h:135-140`).
        machine.power_press_started_at = None;
    }
    fx
}

/// Whether the machine has been up long enough for a press to be tracked.
///
/// `(currentMillis - systemInitializedTime_) > 5000` (`PowerHandler.h:118` and
/// `:143`), strictly greater.
fn power_settle_elapsed(machine: &Machine) -> bool {
    match machine.boot_at {
        None => false,
        Some(boot) => machine.now.since(boot).raw() > crate::timing::POWER_SWITCH_SETTLE_MS,
    }
}

/// `PowerHandler::checkForLongPressReboot` (`PowerHandler.h:142-147`).
///
/// ```cpp
/// if (pressed && (currentMillis - systemInitializedTime_ > 5000) && trackingLongPress_ &&
///     (currentMillis - longPressStartTime_ > 1000) && switch_->longPressDetected()) {
///     triggerSystemReboot();
/// }
/// ```
///
/// Four conditions, all required. `trackingLongPress_` is the C++'s name for
/// "a press was tracked", which is [`power_press_started_at`].
///
/// # Where this runs
///
/// On **every** tick, not on the press edge, because the C++ evaluates it on
/// every `process()` call: the hold lasts about a second, so by the time the
/// duration condition is true the edge is long gone. That is the whole reason
/// this is a tick-time check and not a switch-time one.
#[must_use]
pub fn long_press_reboot(machine: &Machine) -> Vec<Effect> {
    let pressed = machine.switches.power;
    let settled = power_settle_elapsed(machine);
    let held_long_enough = match machine.power_press_started_at {
        None => false,
        Some(started) => {
            machine.now.since(started).raw() > crate::timing::POWER_LONG_PRESS_REBOOT_MS
        }
    };
    if pressed && settled && held_long_enough && machine.switches.power_long_press {
        vec_of(Effect::RequestReboot)
    } else {
        Vec::new()
    }
}

/// `PowerHandler::powerOn` (`PowerHandler.h:149-161`).
///
/// ```cpp
/// if ((state == STANDBY) || (state == PID_DISABLED)) {
///     context->setNormalOperationRequested(true);
///     setUserPidEnabled(systemContext_, true);
///     ... display power save off ...
/// }
/// ```
///
/// The C++ has this as one call with two consequences; they are kept as two here
/// because they are two different things: the request flag is what the state
/// machine consumes, and the persisted preference is what `STANDBY`'s and
/// `EMERGENCY_STOP`'s exits read to restore the runtime PID. The display wake is
/// `display()->setPowerSave(0)` at `:158`.
///
/// Note the state gate: the power switch does nothing at all in `PID_NORMAL`,
/// `BREW_RUNNING`, or any other state. A user who toggles the power switch while
/// brewing gets no response whatsoever, and the machine keeps brewing. That is
/// the C++'s behaviour (`PowerHandler.h:152-154`) and it is preserved.
fn power_on(machine: &mut Machine, ctx: &Context<'_>) -> Vec<Effect> {
    if machine.state != MachineState::Standby && machine.state != MachineState::PidDisabled {
        return Vec::new();
    }
    let mut fx = set_request(machine, ctx, Request::NormalOperation);
    // `setUserPidEnabled(..., true)` (`SystemUtils.h:34-40`): persists
    // `pid.enabled` **and** sets the runtime flag.
    machine.pid.runtime_enabled = true;
    fx.push(Effect::SetPidRuntime { enabled: true });
    fx.push(Effect::WakeDisplay);
    fx
}

/// `PowerHandler::powerOff` (`PowerHandler.h:163-175`).
///
/// ```cpp
/// if (state != STANDBY) {
///     processController->performSafeShutdown();
///     context->setStandbyRequested(true);
///     standbyCoordinator().setRemainingTimeMillis(0);
/// }
/// ```
///
/// Note the ordering: the safe shutdown happens **before** the standby request,
/// and the request is not consumed until the next state-machine pass. So for one
/// tick the hardware is already off while the state is still `PID_NORMAL`, and in
/// that tick `PidNormalState::update` will `enablePump()` if the water switch is
/// held. Narrow, real, and preserved.
///
/// `setRemainingTimeMillis(0)` is [`StandbyTimer::expire_now`](crate::machine::StandbyTimer::expire_now),
/// so `shouldEnterStandby` is true on the very next pass regardless of the
/// configured timeout — which is the point: the power switch is immediate and
/// `standby.time` is not.
fn power_off(machine: &mut Machine) -> Vec<Effect> {
    if machine.state == MachineState::Standby {
        return Vec::new();
    }
    machine.requests.standby = true;
    machine.standby.expire_now();
    vec_of(Effect::SafeHardwareShutdown)
}

/// `HotWaterHandler::processImpl` (`HotWaterHandler.h:73-76`).
///
/// The hot-water handler sets **no** request flag. It calls
/// `setHotWaterActivity(true)` (`HotWaterHandler.h:89`), which resets the
/// standby timer and nothing else, and the actual pumping is done by
/// `PidNormalState::update` and `SteamRunningState::update` reading the switch
/// level. The C++'s permission layer denies the whole handler in
/// `WATER_TANK_EMPTY` (`HotWaterHandler.h:64-67`).
///
/// # It does not wake the machine
///
/// `hasUserActivity()` — the flag `StandbyState` checks — is a stub that returns
/// false (`MachineStateContext.cpp:419-429`). So pressing the water switch while
/// in standby resets the standby countdown and does nothing else: no pump, no
/// wake. Preserved; reported.
fn hot_water_switch(machine: &mut Machine, ctx: &Context<'_>) -> Vec<Effect> {
    if !ctx.hot_water_switch_enabled() {
        return Vec::new();
    }
    if machine.state == MachineState::WaterTankEmpty {
        return Vec::new();
    }
    machine.hot_water_activity = true;
    // `setHotWaterActivity(true)` → `resetStandbyTimerOnUserActivity()`
    // (`MachineStateContext.cpp:262-267`), which is a no-op when standby is
    // disabled (`StandbyCoordinator.h:88`).
    if ctx.config.standby.enabled {
        machine.standby.reset(machine.now, ctx.standby_timeout_ms());
        return vec_of(Effect::ResetStandbyTimer);
    }
    Vec::new()
}

/// The four switch-handler pump watchdogs, run on every loop.
///
/// `BrewHandler::checkPumpTimeout` (`BrewHandler.h:254-262`) and
/// `HotWaterHandler::checkPumpTimeout` (`HotWaterHandler.h:114-122`).
///
/// # Preserved deliberately, and dead — see `09-cpp-findings.md` §11
///
/// `PumpTimer::isExpired()` is false unless `start()` was called, and nothing
/// calls it. Both functions are therefore unreachable in the shipped firmware.
///
/// They are ported anyway, and *are* reachable here, because the reducer has an
/// explicit `brew_pump_started_at` field where the C++ has a timer that is never
/// started. The difference is deliberate and one-directional: the Rust can trip
/// a watchdog the C++ cannot. A port that dropped the check would have removed a
/// safety function the C++ *intended* to have, and the whole point of a
/// parity-preserving port is that such a decision gets made on the record rather
/// than by accident.
///
/// # The brew one sets a flag, it does not transition
///
/// `BrewHandler.h:257-260` calls `setBrewStopRequested(true)` — the transition
/// happens on the **next** `checkTransitions`, not this loop. The test
/// `brew_pump_timeout_requests_a_stop_rather_than_transitioning` pins that the
/// Rust does the same, because a same-tick transition would skip
/// `BREW_RUNNING::onExitImpl`'s valve close ordering relative to the safety
/// check.
#[must_use]
pub fn pump_timeouts(machine: &mut Machine) -> Vec<Effect> {
    let mut fx = Vec::new();

    // `BrewHandler.h:255`: `if (pumpTimer_.isExpired() && isBrewActive())`.
    // `isBrewActive` is "a brew state other than BREW_FINISHED"
    // (`BrewHandler.h:98-103`).
    if brew_timer_expired(machine) && brew_is_active(machine.state) {
        machine.requests.brew_stop = true;
    }

    // `HotWaterHandler.h:115`: `if (pumpTimer_.isExpired() && isHotWaterActive())`.
    // `isHotWaterActive` is the switch level (`HotWaterHandler.h:39-44`).
    if hot_water_timer_expired(machine) && machine.switches.hot_water {
        fx.push(Effect::DisablePump);
    }

    fx
}

/// `PumpTimer::isExpired()` for the brew handler, given that the reducer arms the
/// timer where the C++ never does.
fn brew_timer_expired(machine: &Machine) -> bool {
    match machine.brew_pump_started_at {
        // The C++'s `isRunning_ == false` case: never armed, never expired.
        None => false,
        Some(started) => machine.now.since(started).raw() > crate::timing::BREW_PUMP_TIMEOUT_MS,
    }
}

/// `PumpTimer::isExpired()` for the hot-water handler.
fn hot_water_timer_expired(machine: &Machine) -> bool {
    match machine.hot_water_pump_started_at {
        None => false,
        Some(started) => {
            machine.now.since(started).raw() > crate::timing::HOT_WATER_PUMP_TIMEOUT_MS
        }
    }
}

/// `BrewHandler::isBrewActive` (`BrewHandler.h:98-103`).
#[must_use]
pub fn brew_is_active(state: MachineState) -> bool {
    state.is_brew_state() && state != MachineState::BrewFinished
}

/// Apply one `Command` from outside the switch layer.
///
/// The C++ has no single entry point for this — the web handlers and
/// `MQTTManager` call the `setXxxRequested` setters directly — so this is the
/// Rust's boundary. Each arm names the C++ call site it stands for.
fn apply_command(
    machine: &mut Machine,
    ctx: &Context<'_>,
    command: crate::event::Command,
) -> Vec<Effect> {
    use crate::event::Command as C;
    let mut fx = Vec::new();
    match command {
        C::BrewStart => fx.extend(set_request(machine, ctx, Request::BrewStart)),
        C::BrewStop => machine.requests.set(Request::BrewStop, true),
        C::SteamStart => fx.extend(set_request(machine, ctx, Request::SteamStart)),
        C::SteamStop => machine.requests.set(Request::SteamStop, true),
        C::ManualFlushStart => machine.requests.set(Request::ManualFlushStart, true),
        C::ManualFlushStop => machine.requests.set(Request::ManualFlushStop, true),
        C::BackflushEnter => fx.extend(apply_backflush_mode(machine, ctx, true)),
        C::BackflushCycleStart => {
            fx.extend(set_request(machine, ctx, Request::BackflushCycleStart));
        }
        C::BackflushStop => fx.extend(set_request(machine, ctx, Request::BackflushStop)),
        C::Standby => {
            // The web/MQTT standby path. `setStandbyRequested` does **not**
            // reset the standby timer (`MachineStateContext.h:587`).
            machine.requests.standby = true;
        }
        C::NormalOperation => {
            machine.requests.normal_operation = true;
            // `setNormalOperationRequested` *does* reset the timer
            // (`MachineStateContext.h:254-260`).
            if ctx.config.standby.enabled {
                machine.standby.reset(machine.now, ctx.standby_timeout_ms());
                fx.push(Effect::ResetStandbyTimer);
            }
        }
        C::SetUserPidEnabled(enabled) => {
            // `SystemUtils.h:34-40`.
            machine.pid.runtime_enabled = enabled;
            fx.push(Effect::SetPidRuntime { enabled });
        }
        C::Reboot => {
            // `PowerHandler::triggerSystemReboot` (`PowerHandler.h:177-192`).
            // The C++ shows a message, `delay(1000)`, shuts down, `delay(1000)`
            // and restarts. The two delays and the restart belong to the
            // applier; the reducer only says what is wanted.
            fx.push(Effect::RequestReboot);
        }
    }
    fx
}

/// `MachineStateContext::applyBackflushMode` (`MachineStateContext.cpp:354-381`).
///
/// The mode toggle is the one external request that is a *decision* rather than
/// a flag, so it goes through [`crate::backflush::apply_backflush_mode`] and then
/// applies the outcome.
fn apply_backflush_mode(machine: &mut Machine, ctx: &Context<'_>, active: bool) -> Vec<Effect> {
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
        // `setBackflushEnterRequested(true)` resets the standby timer
        // (`MachineStateContext.cpp:217-222`).
        return set_request(machine, ctx, Request::BackflushEnter);
    }
    Vec::new()
}

/// A one-element [`Vec`]. A named helper rather than a `vec!` at each of the
/// dozen call sites, so "this handler produced exactly one effect" reads the
/// same everywhere.
fn vec_of(effect: Effect) -> Vec<Effect> {
    alloc::vec![effect]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::SwitchLevels;

    #[test]
    fn brew_is_active_excludes_brew_finished() {
        // `BrewHandler.h:98-103`.
        assert!(brew_is_active(MachineState::BrewPreinfusion));
        assert!(brew_is_active(MachineState::BrewRunning));
        assert!(!brew_is_active(MachineState::BrewFinished));
        assert!(!brew_is_active(MachineState::PidNormal));
    }

    #[test]
    fn an_unarmed_pump_timer_never_expires() {
        // The C++'s `if (!isRunning_ || startTime_ == 0) return false;`
        // (`PumpTimer.h:24`).
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
}
