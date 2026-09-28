//! The one place that writes hardware.
//!
//! # The rule
//!
//! `Effect` is a description. Something has to turn descriptions into GPIO
//! writes, and this module is the only thing allowed to. There is exactly one
//! function that does it — [`apply`] — and it is a `match` over [`Effect`] with
//! no wildcard arm, so a new effect cannot be added without someone deciding
//! what it does to the hardware.
//!
//! 04 §3.1: "A new state cannot accidentally poke a relay, because it has no
//! way to reach one — it can only return an `Effect`." That sentence is only
//! true if this module is the sole exit. `tests/ported_state_flow_integration.rs`
//! greps the crate for the actuator call sites to keep it that way honest.
//!
//! # Two traits, deliberately
//!
//! * [`Actuators`] is safety-critical. **No method has a default body**, so a new
//!   actuator method is a compile error in every implementation rather than a
//!   silently-skipped call.
//! * [`SideChannels`] is everything else — logs, the display, the brew timer the
//!   outside world reads, the maintenance store, MQTT, the reboot. These have
//!   default no-op bodies because a host test has no display and no MQTT broker,
//!   and forcing every test double to implement twelve cosmetic methods would
//!   push people to implement the wrong trait.

use cc_domain::state::MachineState;

use crate::effect::Effect;
use crate::machine::Machine;

/// The safety-critical port: pump, valves, heater, and the two shutdowns.
///
/// Every method is mandatory. The trait is deliberately *not* `dyn`-only — the
/// firmware will implement it over `cc-hal-esp32`'s `Actuators`
/// (R3-03) and `HeaterOutput` (R3-04), and a host test over a recorder.
pub trait Actuators {
    /// `HardwareManager::enablePump()`.
    fn enable_pump(&mut self);
    /// `HardwareManager::disablePump()`.
    fn disable_pump(&mut self);
    /// `HardwareManager::openWaterValve()`.
    ///
    /// Must update the facade's own `valveState_`, which is the whole reason
    /// the C++ closes the valve through `HardwareManager` and not through the
    /// relay: poking the relay leaves `valveState_ == WATER_OPEN` while the relay
    /// is off, and the next `openWaterValve()` short-circuits as "already open".
    /// That regression is `test_brew_handler`'s
    /// `ValveSafetyShutdownClosesValveViaHardwareAbstraction`.
    fn open_water_valve(&mut self);
    /// `HardwareManager::closeWaterValve()`.
    fn close_water_valve(&mut self);
    /// `HardwareManager::openSteamValve()`.
    ///
    /// **Never emitted by the reducer** — see [`Effect::OpenSteamValve`] and
    /// `09-cpp-findings.md` §2. The gate is `cc_safety::steam_flow_allowed`,
    /// enforced in the reducer's tail (`CloseSteamValve` outside
    /// `STEAM_RUNNING`) and again in the device implementation's
    /// `may_open_steam` check (R3-03). It exists on the port because the port
    /// has to be able to say it.
    fn open_steam_valve(&mut self);
    /// `HardwareManager::closeSteamValve()`.
    fn close_steam_valve(&mut self);
    /// `HardwareManager::enableHeater()`.
    fn enable_heater(&mut self);
    /// `HardwareManager::disableHeater()`.
    fn disable_heater(&mut self);
    /// The heater duty, 0-1000, as the ISR's PWM window expects (`isr.h:96-118`).
    fn set_heater_duty(&mut self, duty: f32);
    /// `HardwareManager::emergencyShutdown()`.
    ///
    /// Must **latch**: after this, `enable_pump` / `open_water_valve` /
    /// `set_heater_duty` are refused until the latch is cleared. That is S2, and
    /// it is why [`Effect::EmergencyShutdown`] is a distinct variant from the
    /// three "off" effects it also implies.
    fn emergency_shutdown(&mut self);
    /// `HardwareManager::safeHardwareShutdown()`.
    ///
    /// The same relays off **without** the latch, so the machine can come back
    /// after standby (`ProcessController.cpp:489-500`).
    fn safe_hardware_shutdown(&mut self);
}

/// Everything that is not a pin: logs, the display, the brew timer, the
/// maintenance store, MQTT, the reboot.
///
/// All methods default to a no-op. A real implementation overrides what it owns;
/// a test double overrides what it asserts on.
pub trait SideChannels {
    /// The state was left. The C++'s `logStateExit` plus the state-name log.
    fn on_exit_state(&mut self, _state: MachineState) {}
    /// The state was entered. The C++'s `logStateEntry`.
    fn on_enter_state(&mut self, _state: MachineState) {}
    /// The runtime PID flag changed.
    fn on_pid_runtime(&mut self, _enabled: bool) {}
    /// Steam mode changed.
    fn on_steam_mode(&mut self, _enabled: bool) {}
    /// A brew finished and may count as a shot.
    fn on_record_brew(&mut self, _elapsed_ms: f64, _weight: f32, _scale_enabled: bool) {}
    /// The shot-since-backflush counter was reset.
    fn on_reset_shots_since_backflush(&mut self) {}
    /// Every action request was drained (S11).
    fn on_clear_action_requests(&mut self) {}
    /// The stale stop requests were drained.
    fn on_clear_stale_stop_requests(&mut self) {}
    /// The standby countdown was re-armed.
    fn on_reset_standby_timer(&mut self) {}
    /// The MQTT reconnect counter was reset.
    fn on_reset_mqtt_reconnect_count(&mut self) {}
    /// The display must leave power-save.
    fn on_wake_display(&mut self) {}
    /// A reboot was requested.
    ///
    /// The device implementation shows `POWER_REBOOT_DISPLAY_MS` of
    /// "REBOOTING", performs a safe shutdown, waits again, and restarts
    /// (`PowerHandler.h:177-192`). It is the only place in the firmware allowed
    /// to sleep.
    fn on_request_reboot(&mut self) {}
    /// A free-form log line for the transition reason.
    fn on_log(&mut self, _message: &str) {}
    /// The state machine's own snapshot, for the once-per-10-seconds log
    /// (`StateMachine.cpp:90-98`).
    fn on_snapshot(&mut self, _machine: &Machine) {}
}

/// Apply one event's worth of effects, in order.
///
/// # Order
///
/// Front to back, no reordering, no coalescing. Two effects that write the same
/// relay in one tick resolve by position, and the C++'s order is the reference:
/// `update()` → `onExit` → `onEntry` → the S5 valve check. Coalescing
/// (`EnablePump` then `DisablePump` → do nothing) would be faster and wrong:
/// `EmergencyShutdown` followed by `EnablePump` is not a no-op, it is an
/// energise request that the latch is supposed to refuse, and collapsing it
/// would hide the refusal.
pub fn apply(
    actuators: &mut dyn Actuators,
    side: &mut dyn SideChannels,
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
/// build a `Vec`.
pub fn apply_one(
    actuators: &mut dyn Actuators,
    side: &mut dyn SideChannels,
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

        // ---- bookkeeping ----------------------------------------------------
        Effect::ExitState(state) => side.on_exit_state(state),
        Effect::EnterState(state) => side.on_enter_state(state),
        Effect::SetPidRuntime { enabled } => side.on_pid_runtime(enabled),
        Effect::SetSteamMode { enabled } => side.on_steam_mode(enabled),
        Effect::RecordBrew {
            elapsed_ms,
            weight,
            scale_enabled,
        } => side.on_record_brew(elapsed_ms, weight, scale_enabled),
        Effect::ResetShotsSinceBackflush => side.on_reset_shots_since_backflush(),
        Effect::ClearActionRequests => side.on_clear_action_requests(),
        Effect::ClearStaleStopRequests => side.on_clear_stale_stop_requests(),
        Effect::ResetStandbyTimer => side.on_reset_standby_timer(),
        Effect::ResetMqttReconnectCount => side.on_reset_mqtt_reconnect_count(),
        Effect::WakeDisplay => side.on_wake_display(),
        Effect::RequestReboot => side.on_request_reboot(),

        // ---- observability ---------------------------------------------------
        // `BrewHandler::checkPumpTimeout`'s `logError("Pump timeout - stopping
        // for safety")` and the hot-water equivalent. In the C++ these lines are
        // unreachable (09 §11); here they are the only way a field operator can
        // learn that a watchdog fired, which is the point of the effect. The
        // action that follows is a separate effect, in the C++'s order.
        Effect::PumpTimeoutFired { watchdog } => side.on_log(watchdog.message()),
    }

    let _ = machine;
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut side = SideChannelsNone;
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
        // Collapsing would be an optimisation and would be wrong: the C++ calls
        // both, and the second call is what actually leaves the relay off.
        let mut act = Recorder::default();
        let mut side = SideChannelsNone;
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
    fn a_side_channel_default_implementation_is_a_no_op() {
        let mut side = SideChannelsNone;
        side.on_wake_display();
        side.on_request_reboot();
        side.on_log("x");
    }

    /// A side-channel sink that implements nothing, to prove the defaults are
    /// genuinely optional.
    struct SideChannelsNone;

    impl SideChannels for SideChannelsNone {}
}
