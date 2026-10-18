//! The one place that writes hardware.
//!
//! # The rule
//!
//! `Effect` is a description. Something has to turn descriptions into GPIO writes, and this
//! module is the only thing allowed to. There is exactly one function that does it --
//! [`apply`] -- and it is a `match` over [`Effect`] with no wildcard arm, so a new effect
//! cannot be added without someone deciding what it does to the hardware.
//!
//! Nothing else in this crate may call an actuator, and `tests/state_flow_integration.rs`
//! greps the crate for actuator call sites to keep that true. Without that check, "a state
//! cannot poke a relay" would be a convention rather than a property.
//!
//! # Three traits, and why
//!
//! * [`Actuators`] is safety-critical. **No method has a default body**, so a new actuator
//!   method is a compile error in every implementation rather than a silently-skipped call.
//! * [`MachineChannels`] is the machine's own bookkeeping: everything whose loss is a *defect*
//!   rather than a missing log line -- the shot counter, the reboot. **No method has a default
//!   body either**, and that is the point of the split. A default body on a method whose
//!   absence changes what the machine *does* is a lie the type system tells at every call
//!   site: a side channel that forgets one would still compile, drop the write, and show up
//!   only as a status field that never moves.
//! * [`Diagnostics`] is observability -- the state-transition log, the flag mirrors, the
//!   watchdog line. It is reached through one optional accessor so that "this side channel has
//!   no diagnostics" is a single `None` instead of a body per method, and it does keep default
//!   bodies: a gap in a log is visible in the log's absence, which is the property
//!   [`MachineChannels`] cannot afford.
//!
//! The dividing line is therefore not "important" versus "unimportant". It is **whether the
//! reducer has already applied the change to [`Machine`][`crate::machine::Machine`]**. If it
//! has, the channel is told about it and dropping the call loses a line of telemetry. If it
//! has not, dropping the call loses the change -- and that is [`MachineChannels`].
use cc_domain::state::MachineState;

use crate::effect::Effect;
use crate::machine::Machine;

/// The safety-critical port: pump, valves, heater, and the two shutdowns.
///
/// Every method is mandatory. The trait is deliberately *not* `dyn`-only: the firmware
/// implements it over `cc-hal-esp32`'s actuators and heater output, and a host test over a
/// recorder.
pub trait Actuators {
    /// Energise the pump relay.
    fn enable_pump(&mut self);
    /// De-energise the pump relay.
    fn disable_pump(&mut self);
    /// Open the water valve.
    ///
    /// The implementation owns the open/closed bookkeeping, which is why the valve is
    /// switched through this trait rather than by poking the relay: a relay forced off while
    /// the bookkeeping still says "open" leaves the next open call short-circuiting as
    /// already done, and the valve stays shut.
    fn open_water_valve(&mut self);
    /// Close the water valve.
    fn close_water_valve(&mut self);
    /// Open the steam valve.
    ///
    /// **Gated, deliberately: §2 of `docs/history/divergences.md`.** The gate is
    /// `cc_safety::steam_flow_allowed`, enforced in the reducer's tail (`CloseSteamValve`
    /// outside `STEAM_RUNNING`) and again in the device implementation's `may_open_steam`
    /// check. The method exists because the machine has to be able to express the action even
    /// where the whitelist will refuse it.
    fn open_steam_valve(&mut self);
    /// Close the steam valve.
    fn close_steam_valve(&mut self);
    /// Enable the heater output, so the PID's duty may reach it.
    fn enable_heater(&mut self);
    /// Drive the heater off.
    fn disable_heater(&mut self);
    /// The heater duty, 0-1000, as the ISR's PWM window expects.
    fn set_heater_duty(&mut self, duty: f32);
    /// All relays off, and **latched**.
    ///
    /// After this, `enable_pump` / `open_water_valve` / `set_heater_duty` are refused until the
    /// latch is cleared. That is S2, and it is why [`Effect::EmergencyShutdown`] is a distinct
    /// variant from the three "off" effects it also implies.
    fn emergency_shutdown(&mut self);
    /// The same relays off **without** the latch, so the machine can come back after standby.
    fn safe_hardware_shutdown(&mut self);
}

/// The machine's own bookkeeping: the effects whose loss is a defect.
///
/// **Every method is mandatory.** That is the entire contract, and it is why
/// this trait exists separately from [`Diagnostics`]: an implementation that
/// forgets to handle one of these does not compile. See the module documentation
/// for the bug that motivated the split.
pub trait MachineChannels {
    /// A brew finished, and the reducer has already decided whether it counts.
    ///
    /// `counted` is the reducer's answer -- `Machine::shots_since_backflush` has already been
    /// incremented if it is `true` -- and `shots_since_backflush` is that counter. An
    /// implementation must **persist `shots_since_backflush` when `counted`**, and must not
    /// re-derive the decision: [`crate::maintenance::qualifies_as_counted_shot`] is the rule,
    /// and the reducer is where it is evaluated.
    fn on_record_brew(&mut self, counted: bool, shots_since_backflush: i32);

    /// The shot counter was cleared, and `shots_since_backflush` is its new value.
    ///
    /// The same write as [`Self::on_record_brew`], and a separate effect because the two
    /// arrive from different places: a finished brew, a finished backflush, and the
    /// `POST /api/maintenance/reset-backflush-counter` route.
    fn on_reset_shots_since_backflush(&mut self, shots_since_backflush: i32);

    /// A reboot was requested.
    ///
    /// The device implementation shows [`crate::timing::POWER_REBOOT_DISPLAY_MS`] of
    /// "REBOOTING", performs a safe shutdown, waits again, and restarts. It is the only place
    /// in the firmware allowed to sleep.
    fn on_request_reboot(&mut self);

    /// The optional observability sink, or `None`.
    ///
    /// This is the one method here with a default body, and it has one because
    /// it is the opt-in rather than an obligation: an implementation that has
    /// nothing to say returns `None` and implements nothing else, and an
    /// implementation that does say something returns `Some(self)` and owns
    /// every [`Diagnostics`] method. What it must not do is return `None` while
    /// claiming, by implementing [`MachineChannels`], that the machine's
    /// behaviour is handled.
    fn diagnostics(&mut self) -> Option<&mut dyn Diagnostics> {
        None
    }
}

/// Everything that is a notification rather than a change: the state-transition log, the flag
/// mirrors the reducer has already applied, and the free-form log line.
///
/// Reached through [`MachineChannels::diagnostics`], and deliberately allowed to have default
/// bodies -- see the module documentation. Everything the reducer has **not** already written
/// into [`Machine`] belongs on [`MachineChannels`] instead.
///
/// Five methods, and every one of them earns its place by having an implementation:
/// `on_enter_state` and `on_exit_state` are the state-transition lines, `on_pid_runtime` and
/// `on_steam_mode` mirror two flags, and `on_log` is where [`Effect::PumpTimeoutFired`]
/// reaches the field log. §1 of `docs/history/divergences.md` is a claim about a *log line*,
/// so a method nobody implements would make that record false.
pub trait Diagnostics {
    /// The state was left.
    fn on_exit_state(&mut self, _state: MachineState) {}
    /// The state was entered.
    fn on_enter_state(&mut self, _state: MachineState) {}
    /// The runtime PID flag changed.
    ///
    /// A mirror, not a change: `Machine::pid::runtime_enabled` is already written by the time
    /// this runs, in `handlers::SetUserPidEnabled`.
    fn on_pid_runtime(&mut self, _enabled: bool) {}
    /// Steam mode changed. A mirror of [`Machine::steam_mode`], likewise.
    fn on_steam_mode(&mut self, _enabled: bool) {}
    /// A free-form log line for the transition reason, and for a watchdog that
    /// fired.
    fn on_log(&mut self, _message: &str) {}
}

/// Apply one event's worth of effects, in order.
///
/// # Order
///
/// Front to back, no reordering, no coalescing. Two effects that write the same relay in one
/// tick resolve by position, and the reducer's emission order is the contract: the old
/// state's exit, the new state's entry, then the tick's own effects and the S5 valve check.
///
/// Coalescing (`EnablePump` then `DisablePump` -> do nothing) would be faster and wrong:
/// `EmergencyShutdown` followed by `EnablePump` is not a no-op, it is an energise request that
/// the latch is supposed to refuse, and collapsing it would hide the refusal.
///
/// # Why generic rather than `&mut dyn`
///
/// Both bounds are `?Sized`, so a caller holding a trait object still compiles, but the
/// firmware passes concrete types and so gets **static** dispatch: this monomorphises once per
/// `(A, M)` pair and every `actuators.…` below inlines into the match. That is the whole point
/// on the safety path: four to five `reduce` calls plus this call over up to
/// [`MAX_EFFECTS_PER_EVENT`](crate::effect::MAX_EFFECTS_PER_EVENT) effects run every 10 ms
/// tick, so indirect calls would number in the thousands per second, and `dyn` is precisely
/// what stops the optimiser seeing that `EnablePump` is one pin write.
///
/// The trait seam is untouched, so host-testability is too: a recorder is still just a type
/// implementing [`Actuators`], and the device build still has one implementation of each
/// trait.
pub fn apply<A: Actuators + ?Sized, M: MachineChannels + ?Sized>(
    actuators: &mut A,
    side: &mut M,
    machine: &Machine,
    effects: &[Effect],
) {
    for effect in effects {
        apply_one(actuators, side, machine, *effect);
    }
}

/// Apply exactly one effect.
///
/// Split out from [`apply`] so a caller that has one effect to deliver — the
/// shell's own [`Effect::RequestReboot`] on the way out, say — does not have to
/// build a `Vec`. Generic for the same reason as [`apply`].
pub fn apply_one<A: Actuators + ?Sized, M: MachineChannels + ?Sized>(
    actuators: &mut A,
    side: &mut M,
    machine: &Machine,
    effect: Effect,
) {
    match effect {
        // ---- actuator writes ------------------------------------------------
        Effect::EnablePump => actuators.enable_pump(),
        Effect::DisablePump => actuators.disable_pump(),
        Effect::OpenWaterValve => actuators.open_water_valve(),
        Effect::CloseWaterValve => actuators.close_water_valve(),
        Effect::OpenSteamValve => actuators.open_steam_valve(),
        Effect::CloseSteamValve => actuators.close_steam_valve(),
        Effect::EnableHeater => actuators.enable_heater(),
        Effect::DisableHeater => actuators.disable_heater(),
        Effect::SetHeaterDuty(duty) => actuators.set_heater_duty(duty),
        Effect::EmergencyShutdown => actuators.emergency_shutdown(),
        Effect::SafeHardwareShutdown => actuators.safe_hardware_shutdown(),

        // ---- the counter the machine keeps but cannot write down itself -----
        //
        // These two carry `machine`'s value rather than being asked to compute it: the reducer
        // owns `shots_since_backflush` and has already applied the qualification rule, and the
        // side channel's job is to remember it.
        Effect::RecordBrew { counted } => {
            side.on_record_brew(counted, machine.shots_since_backflush);
        }
        Effect::ResetShotsSinceBackflush => {
            side.on_reset_shots_since_backflush(machine.shots_since_backflush);
        }
        Effect::RequestReboot => side.on_request_reboot(),

        // ---- observability ---------------------------------------------------
        //
        // Every arm below reports something the reducer has **already** written
        // into `Machine`, or a line of text. That is the line between this
        // group and the three above, and it is why a `None` diagnostics sink
        // loses a log and never a change. `if let` rather than an `else` arm on
        // the optional trait, because there is no obligation here to satisfy.
        Effect::ExitState(state) => observe(side, |d| d.on_exit_state(state)),
        Effect::EnterState(state) => observe(side, |d| d.on_enter_state(state)),
        Effect::SetPidRuntime { enabled } => observe(side, |d| d.on_pid_runtime(enabled)),
        Effect::SetSteamMode { enabled } => observe(side, |d| d.on_steam_mode(enabled)),

        // These five report a change the reducer has already written into `Machine` and that
        // nothing outside `Machine` can want, so they are delivered to no one. They were
        // [`Diagnostics`] obligations with default bodies, which read as "a side channel may
        // implement this" and in practice none did. A no-op arm says the same thing with no
        // trait surface to keep in step:
        //
        // * `ClearActionRequests` / `ClearStaleStopRequests` mirror
        //   [`Machine::requests`](crate::machine::Machine::requests), which the reducer drained
        //   itself.
        // * `ResetStandbyTimer` mirrors [`Machine::standby`], which `set_request` has already
        //   re-armed.
        // * `ResetMqttReconnectCount`: reconnection is `esp-mqtt`'s own, configured in
        //   `cc-hal-esp32::mqtt`, so there is no attempt counter here to reset.
        // * `WakeDisplay` does not need doing: the panel is blanked from
        //   [`Machine::standby`]'s `should_turn_off_display()` at frame-publish time, and both
        //   wake paths out of standby request normal operation, which re-arms that timer.
        Effect::ClearActionRequests
        | Effect::ClearStaleStopRequests
        | Effect::ResetStandbyTimer
        | Effect::ResetMqttReconnectCount
        | Effect::WakeDisplay => {}

        // The log line for a pump watchdog trip is the only way a field operator learns that
        // one fired, which is why the effect exists. The action that follows (stop the brew,
        // disable the pump) is a separate effect, so the order is deliberate rather than a
        // side effect of logging.
        Effect::PumpTimeoutFired { watchdog } => observe(side, |d| d.on_log(watchdog.message())),
    }
}

/// Hand a diagnostics sink to `f`, if this side channel has one.
///
/// A function rather than five `if let`s so that "there is no sink" is decided
/// in exactly one place and cannot be got wrong per-effect, and rather than a
/// shared no-op object because a `&mut` to a `static` is a question this crate
/// has no reason to ask (`#![forbid(unsafe_code)]`, and aliasing a zero-sized
/// value is only harmless by accident).
fn observe<M: MachineChannels + ?Sized>(side: &mut M, f: impl FnOnce(&mut dyn Diagnostics)) {
    if let Some(diagnostics) = side.diagnostics() {
        f(diagnostics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::PumpWatchdog;
    use alloc::string::String;
    use alloc::vec::Vec;

    /// A recorder, so the exhaustive table can assert on the effect stream
    /// without a hardware port.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<&'static str>,
    }

    impl Actuators for Recorder {
        fn enable_pump(&mut self) {
            self.calls.push("enable_pump");
        }
        fn disable_pump(&mut self) {
            self.calls.push("disable_pump");
        }
        fn open_water_valve(&mut self) {
            self.calls.push("open_water_valve");
        }
        fn close_water_valve(&mut self) {
            self.calls.push("close_water_valve");
        }
        fn open_steam_valve(&mut self) {
            self.calls.push("open_steam_valve");
        }
        fn close_steam_valve(&mut self) {
            self.calls.push("close_steam_valve");
        }
        fn enable_heater(&mut self) {
            self.calls.push("enable_heater");
        }
        fn disable_heater(&mut self) {
            self.calls.push("disable_heater");
        }
        fn set_heater_duty(&mut self, _duty: f32) {
            self.calls.push("set_heater_duty");
        }
        fn emergency_shutdown(&mut self) {
            self.calls.push("emergency_shutdown");
        }
        fn safe_hardware_shutdown(&mut self) {
            self.calls.push("safe_hardware_shutdown");
        }
    }

    #[test]
    fn effects_are_applied_in_order_without_coalescing() {
        let mut act = Recorder::default();
        let mut side = Bookkeeping::default();
        let machine = Machine::cold();
        apply(
            &mut act,
            &mut side,
            &machine,
            &[
                Effect::EnablePump,
                Effect::OpenWaterValve,
                Effect::DisablePump,
            ],
        );
        assert_eq!(
            act.calls,
            ["enable_pump", "open_water_valve", "disable_pump"]
        );
    }

    #[test]
    fn an_enable_then_disable_pair_is_not_collapsed() {
        // Collapsing would be an optimisation and would be wrong: both effects are applied,
        // and the second is what actually leaves the relay off.
        let mut act = Recorder::default();
        let mut side = Bookkeeping::default();
        let machine = Machine::cold();
        apply(
            &mut act,
            &mut side,
            &machine,
            &[Effect::EnablePump, Effect::DisablePump],
        );
        assert_eq!(act.calls.len(), 2);
    }

    #[test]
    fn a_recorded_brew_reaches_the_mandatory_channel_with_the_counter() {
        // The regression test for the defect that made the trait split
        // necessary. `machine` is what the applier is handed, so the channel is
        // told the value the reducer produced rather than being asked to derive
        // it — and a side channel that implements only the three mandatory
        // methods receives it, with no diagnostics sink in sight.
        let mut act = Recorder::default();
        let mut side = Bookkeeping::default();
        let mut machine = Machine::cold();
        machine.shots_since_backflush = 51;
        apply(
            &mut act,
            &mut side,
            &machine,
            &[Effect::RecordBrew { counted: true }],
        );
        assert_eq!(side.calls, ["record_brew:51"]);
    }

    #[test]
    fn an_uncounted_brew_is_still_offered_so_the_channel_can_say_why() {
        let mut act = Recorder::default();
        let mut side = Bookkeeping::default();
        let mut machine = Machine::cold();
        machine.shots_since_backflush = 51;
        apply(
            &mut act,
            &mut side,
            &machine,
            &[Effect::RecordBrew { counted: false }],
        );
        assert_eq!(side.calls, ["record_brew:51:no"]);
    }

    #[test]
    fn a_reset_reaches_the_mandatory_channel_with_the_new_value() {
        let mut act = Recorder::default();
        let mut side = Bookkeeping::default();
        let mut machine = Machine::cold();
        machine.shots_since_backflush = 0;
        apply(
            &mut act,
            &mut side,
            &machine,
            &[Effect::ResetShotsSinceBackflush],
        );
        assert_eq!(side.calls, ["reset:0"]);
    }

    #[test]
    fn every_observability_effect_is_a_no_op_without_a_diagnostics_sink() {
        // The other half of the split's argument: dropping a `Diagnostics` call
        // loses a log line and nothing else, which is why it may have a default
        // body where [`MachineChannels`] may not. Each of these effects has
        // already been applied to `machine` by the reducer.
        let mut act = Recorder::default();
        let mut side = Bookkeeping::default();
        let machine = Machine::cold();
        apply(
            &mut act,
            &mut side,
            &machine,
            &[
                Effect::ExitState(MachineState::Init),
                Effect::EnterState(MachineState::Init),
                Effect::SetPidRuntime { enabled: true },
                Effect::SetSteamMode { enabled: true },
                Effect::ClearActionRequests,
                Effect::ClearStaleStopRequests,
                Effect::ResetStandbyTimer,
                Effect::ResetMqttReconnectCount,
                Effect::WakeDisplay,
                Effect::PumpTimeoutFired {
                    watchdog: PumpWatchdog::Brew,
                },
            ],
        );
        assert!(side.calls.is_empty(), "{:?}", side.calls);
        assert!(act.calls.is_empty(), "{:?}", act.calls);
    }

    #[test]
    fn an_opted_in_diagnostics_sink_sees_the_same_effects() {
        let mut act = Recorder::default();
        let mut side = Bookkeeping::loud();
        let machine = Machine::cold();
        apply(
            &mut act,
            &mut side,
            &machine,
            &[Effect::EnterState(MachineState::BrewRunning)],
        );
        assert_eq!(side.diagnostics_seen, 1);
    }

    /// The mandatory half only — no diagnostics sink, which is what a host
    /// double wants and what the firmware had before this was fixed.
    #[derive(Default)]
    struct Bookkeeping {
        calls: Vec<String>,
        diagnostics_seen: u32,
        loud: bool,
    }

    impl Bookkeeping {
        /// The same double, opted **in** to diagnostics.
        fn loud() -> Self {
            Self {
                loud: true,
                ..Self::default()
            }
        }
    }

    impl MachineChannels for Bookkeeping {
        fn on_record_brew(&mut self, counted: bool, shots_since_backflush: i32) {
            let suffix = if counted { "" } else { ":no" };
            self.calls.push(alloc::format!(
                "record_brew:{shots_since_backflush}{suffix}"
            ));
        }

        fn on_reset_shots_since_backflush(&mut self, shots_since_backflush: i32) {
            self.calls
                .push(alloc::format!("reset:{shots_since_backflush}"));
        }

        fn on_request_reboot(&mut self) {
            self.calls.push(alloc::string::String::from("reboot"));
        }

        fn diagnostics(&mut self) -> Option<&mut dyn Diagnostics> {
            self.loud.then_some(self as &mut dyn Diagnostics)
        }
    }

    impl Diagnostics for Bookkeeping {
        fn on_enter_state(&mut self, _state: MachineState) {
            self.diagnostics_seen = self.diagnostics_seen.saturating_add(1);
        }
    }
}
