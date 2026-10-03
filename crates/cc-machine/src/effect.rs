//! The only way out of the reducer: what the machine wants done.
//!
//! # Why effects and not calls
//!
//! An [`Effect`] is a *description*. The C++ state classes call
//! `context.enablePump()` / `context.openWaterValve()` directly
//! (`BrewStates.cpp:70-71`), and `MachineStateContext` forwards straight to
//! `HardwareManager` (`MachineStateContext.cpp:544-578`). That is why
//! `HardwareManager`'s `valveState_` bookkeeping is a thing that can drift, and
//! why 04 §3.1 insists "a new state cannot accidentally poke a relay, because it
//! has no way to reach one — it can only return an `Effect`".
//!
//! In this crate nothing is called. [`reduce`](crate::reduce) returns a list,
//! and `crate::applier::apply` is the single function that turns a
//! list into actuator calls. A test asserts hardware by inspecting the list, and
//! needs no mock object, no GPIO, and no `HardwareManager`.
//!
//! # Order
//!
//! Order is part of the contract, not an implementation detail: the C++ runs
//! `onExit` → swap → `onEntry` (`StateMachine.cpp:136-148`) and
//! `update()` → `checkTransitions()` (`StateMachine.cpp:81-86`), and the S5
//! valve fail-safe runs *after* all of it (`LoopManager.cpp:616-619`). Two
//! effects that both write the same relay in the same tick resolve differently
//! depending on order, so the applier applies the vector front to back and the
//! tests assert on exact sequences, not sets.

use cc_domain::state::MachineState;

/// Something the machine wants done.
///
/// The three groups are, in the order the C++ performs them: actuator writes,
/// state-machine bookkeeping, and everything else. The grouping is a comment,
/// not a type — a single enum keeps one `Vec` and one match in the applier, and
/// the applier is the only place that has to be exhaustive over hardware.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    // ---- actuator writes: the only things that may change a pin ------------
    /// `context.enablePump()` → `HardwareManager::enablePump()`
    /// (`MachineStateContext.cpp:544`).
    EnablePump,
    /// `context.disablePump()` (`MachineStateContext.cpp:548`).
    DisablePump,
    /// `context.openWaterValve()` (`MachineStateContext.cpp:564`).
    OpenWaterValve,
    /// `context.closeWaterValve()` (`MachineStateContext.cpp:568`).
    CloseWaterValve,
    /// `context.openSteamValve()` (`MachineStateContext.cpp:556`).
    ///
    /// **Deliberate divergence, see 09 §2 / `intentional-diffs.md` #2.** The C++
    /// has no steam-valve whitelist at all and nothing ever calls it. This port
    /// gates it on `cc_safety::steam_flow_allowed` — `STEAM_RUNNING` and nothing
    /// else — in two places: the reducer's tail closes the valve in every other
    /// state (the `steamValveSafetyShutdownCheck` this port adds), and the
    /// verdict's `may_open_steam` refuses the actuator call. The effect still
    /// exists because the state machine must be able to express it, and because
    /// the shared-relay argument means "the C++ never calls it" is a fact about
    /// the call graph, not a safety property.
    OpenSteamValve,
    /// `context.closeSteamValve()` (`MachineStateContext.cpp:560`).
    CloseSteamValve,
    /// `context.enableHeater()` (`MachineStateContext.cpp:532`).
    EnableHeater,
    /// `context.disableHeater()` (`MachineStateContext.cpp:536`).
    DisableHeater,
    /// The heater duty, in the C++'s 0-1000 units.
    ///
    /// Not `HardwareManager::setHeaterPower` (`uint8_t` percent): the C++ heater
    /// path is a PWM window compared against the PID output
    /// (`isr.h:96-118`), and R1-07 settles the method. This is the value the
    /// applier will hand to `HeaterOutput`, not a relay pin.
    SetHeaterDuty(f32),
    /// `context.emergencyShutdown()` → `HardwareManager::emergencyShutdown()`
    /// (`MachineStateContext.cpp:580`).
    ///
    /// Distinct from `DisablePump + CloseWaterValve + DisableHeater` because it
    /// also sets the actuator facade's own `emergencyMode_`, which makes every
    /// later `Enable*` call a no-op until cleared
    /// (`HardwareManager.cpp:278,306,321,353,398,443`). Latching is the point.
    EmergencyShutdown,
    /// `context.safeHardwareShutdown()` → `HardwareManager::safeHardwareShutdown()`
    /// (`MachineStateContext.cpp:584`).
    ///
    /// Also distinct: it turns the relays off **without** setting
    /// `emergencyMode_` (`ProcessController.cpp:489-500` says so explicitly),
    /// so the machine can come back after standby.
    SafeHardwareShutdown,

    // ---- state-machine bookkeeping ----------------------------------------
    /// The state being left. Emitted before the old state's exit effects.
    ExitState(MachineState),
    /// The state being entered. Emitted before the new state's entry effects.
    EnterState(MachineState),
    /// `context.setPidRuntimeState(bool)` (`MachineStateContext.cpp:275`).
    ///
    /// Runtime only — it never writes `pid.enabled`. That distinction is the
    /// whole reason `EmergencyStopState::onExitImpl` can restore the PID from
    /// config (`EmergencyStopState.cpp:29-30`).
    SetPidRuntime {
        /// The new runtime PID state.
        enabled: bool,
    },
    /// `context.setSteamMode(bool)` (`MachineStateContext.cpp:271`).
    ///
    /// Also sets `steamFirstON_` to the same value, exactly as
    /// `SystemUtils.h:42-53` does.
    SetSteamMode {
        /// Whether steam mode is now on.
        enabled: bool,
    },
    // NOTE: the brew timer (`ProcessController::currBrewTime_` /
    // `totalTargetBrewTime_`) and the backflush cycle counter
    // (`MachineStateContext::currBackflushCycles_`) are deliberately **not**
    // effects. In the C++ they are values in other objects that the display and
    // MQTT read; here they are `Machine::brew` and `Machine::backflush`, and
    // the applier is handed the new `Machine` by `reduce`. An effect that
    // restates a field the applier can already read is a second source of
    // truth waiting to disagree with the first.
    /// `maintenanceCoordinator().recordBrewIfQualified(...)` from
    /// `BrewFinishedState::onEntryImpl` (`BrewStates.cpp:310-311`).
    RecordBrew {
        /// Total brew time for this shot, milliseconds.
        elapsed_ms: f64,
        /// Brew weight, grams. Only meaningful with `scale_enabled`.
        weight: f32,
        /// `config.hardware.sensors.scale.enabled`, via
        /// `config.hardwareSensorsScaleEnabled.get()`
        /// (`BrewStates.cpp:309`).
        scale_enabled: bool,
    },
    /// `maintenanceCoordinator().resetSinceBackflush()` from
    /// `BackflushFinishedState::onEntryImpl` (`BackflushStates.cpp:144`).
    ResetShotsSinceBackflush,
    /// `context.clearAllActionRequests()` (`MachineStateContext.h:615-627`).
    ///
    /// S11. The *effect* is emitted so the applier's flag mirror stays in step
    /// with [`Machine::requests`](crate::machine::Machine::requests); the
    /// reducer has already applied it.
    ClearActionRequests,
    /// `context.clearStaleStopRequests()` (`MachineStateContext.h:634-639`).
    ///
    /// Not the same as [`Effect::ClearActionRequests`]: `STANDBY` keeps the
    /// start flags as wake triggers and drains only the stops.
    ClearStaleStopRequests,
    /// `standbyCoordinator().reset()`, via every
    /// `setXxxRequested(true)` that counts as user activity
    /// (`MachineStateContext.cpp:217-267`).
    ResetStandbyTimer,
    /// `networkCoordinator().resetMqttConnectionAttempts()`
    /// (`MachineStateContext.cpp:327-329`), on the way out of standby.
    ResetMqttReconnectCount,
    /// `MachineStateContext::exitStandbyMode()`'s one real side effect:
    /// `display->setPowerSave(0)` (`MachineStateContext.cpp:411-417`).
    WakeDisplay,
    /// `PowerHandler::triggerSystemReboot()` (`PowerHandler.h:177-192`).
    ///
    /// The reducer does not restart the chip. The applier shows the message,
    /// performs the safe shutdown, and restarts — and the
    /// `POWER_REBOOT_DISPLAY_MS` delay is a *shell* concern, because a pure
    /// function may not sleep.
    RequestReboot,
    /// A pump watchdog expired.
    ///
    /// **Divergence, see 09 §11 / `intentional-diffs.md` #1.** In the C++ both
    /// watchdogs are dead code — `PumpTimer::start()` is never called, so
    /// `isExpired()` is unconditionally `false` and the two `logError` lines
    /// that would announce a trip are unreachable. The port arms both on the
    /// activating edge, so this effect is reachable, and it is emitted *so the
    /// trip is visible in the field log* rather than being a silent hardware
    /// change: an operator needs to be able to tell that a five-minute brew
    /// timeout exists and fired, which is the only way to diagnose a
    /// marginal-boiler machine.
    ///
    /// The two C++ messages this stands for, verbatim:
    ///
    /// * `BrewHandler::checkPumpTimeout` — `"Pump timeout - stopping for safety"`
    ///   (`BrewHandler.h:256`)
    /// * `HotWaterHandler::checkPumpTimeout` — `"Hot water pump timeout - stopping
    ///   for safety"` (`HotWaterHandler.h:117`; also recovered verbatim from the
    ///   previous Rust firmware, [08 §4.2](../../docs/rust-migration/08-recovered-oracle.md))
    ///
    /// The applier turns it into the log line; the *action* that follows
    /// (`Request::BrewStop` / `Effect::DisablePump`) is a separate effect, in
    /// the C++'s order.
    PumpTimeoutFired {
        /// Which watchdog fired.
        watchdog: PumpWatchdog,
    },
}

/// Which of the C++'s two `PumpTimer` instances expired.
///
/// `BrewHandler` owns a `pumpTimer_(300000)` and `HotWaterHandler` a
/// `pumpTimer_(60000)`; they are separate objects with separate deadlines, so
/// they are separate variants rather than one flag. See
/// [`timing::BREW_PUMP_TIMEOUT_MS`](crate::timing::BREW_PUMP_TIMEOUT_MS)
/// and [`timing::HOT_WATER_PUMP_TIMEOUT_MS`](crate::timing::HOT_WATER_PUMP_TIMEOUT_MS).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PumpWatchdog {
    /// `BrewHandler::pumpTimer_` — 300 000 ms.
    Brew,
    /// `HotWaterHandler::pumpTimer_` — 60 000 ms.
    HotWater,
}

impl PumpWatchdog {
    /// The C++'s `logError` text, verbatim.
    ///
    /// Quoted rather than re-worded so a grep for the C++ message finds it, and
    /// so a log line from the Rust firmware is recognisable as the same event.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            // BrewHandler.h:256
            Self::Brew => "Pump timeout - stopping for safety",
            // HotWaterHandler.h:117
            Self::HotWater => "Hot water pump timeout - stopping for safety",
        }
    }

    /// The deadline, for a log line that says how far over it ran.
    #[must_use]
    pub const fn timeout_ms(self) -> u32 {
        match self {
            Self::Brew => crate::timing::BREW_PUMP_TIMEOUT_MS,
            Self::HotWater => crate::timing::HOT_WATER_PUMP_TIMEOUT_MS,
        }
    }
}

impl Effect {
    /// Whether this effect writes an actuator pin.
    ///
    /// The S6/S2/S4/S5 test tables use this to answer "did this event reach the
    /// hardware at all?" without matching on variants, and the applier's
    /// audit log uses it to separate actuator writes from bookkeeping.
    #[must_use]
    pub const fn is_actuator_write(self) -> bool {
        matches!(
            self,
            Self::EnablePump
                | Self::DisablePump
                | Self::OpenWaterValve
                | Self::CloseWaterValve
                | Self::OpenSteamValve
                | Self::CloseSteamValve
                | Self::EnableHeater
                | Self::DisableHeater
                | Self::SetHeaterDuty(_)
                | Self::EmergencyShutdown
                | Self::SafeHardwareShutdown
        )
    }

    /// A short, stable name for logs and test failure messages.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::EnablePump => "EnablePump",
            Self::DisablePump => "DisablePump",
            Self::OpenWaterValve => "OpenWaterValve",
            Self::CloseWaterValve => "CloseWaterValve",
            Self::OpenSteamValve => "OpenSteamValve",
            Self::CloseSteamValve => "CloseSteamValve",
            Self::EnableHeater => "EnableHeater",
            Self::DisableHeater => "DisableHeater",
            Self::SetHeaterDuty(_) => "SetHeaterDuty",
            Self::EmergencyShutdown => "EmergencyShutdown",
            Self::SafeHardwareShutdown => "SafeHardwareShutdown",
            Self::ExitState(_) => "ExitState",
            Self::EnterState(_) => "EnterState",
            Self::SetPidRuntime { .. } => "SetPidRuntime",
            Self::SetSteamMode { .. } => "SetSteamMode",
            Self::RecordBrew { .. } => "RecordBrew",
            Self::ResetShotsSinceBackflush => "ResetShotsSinceBackflush",
            Self::ClearActionRequests => "ClearActionRequests",
            Self::ClearStaleStopRequests => "ClearStaleStopRequests",
            Self::ResetStandbyTimer => "ResetStandbyTimer",
            Self::ResetMqttReconnectCount => "ResetMqttReconnectCount",
            Self::WakeDisplay => "WakeDisplay",
            Self::RequestReboot => "RequestReboot",
            Self::PumpTimeoutFired { .. } => "PumpTimeoutFired",
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn actuator_writes_are_exactly_the_eleven_pin_writes() {
        let writes: Vec<Effect> = [
            Effect::EnablePump,
            Effect::DisablePump,
            Effect::OpenWaterValve,
            Effect::CloseWaterValve,
            Effect::OpenSteamValve,
            Effect::CloseSteamValve,
            Effect::EnableHeater,
            Effect::DisableHeater,
            Effect::SetHeaterDuty(500.0),
            Effect::EmergencyShutdown,
            Effect::SafeHardwareShutdown,
        ]
        .into_iter()
        .filter(|e| e.is_actuator_write())
        .collect();
        assert_eq!(writes.len(), 11);
    }

    #[test]
    fn bookkeeping_is_not_an_actuator_write() {
        assert!(!Effect::EnterState(MachineState::Init).is_actuator_write());
        assert!(!Effect::SetPidRuntime { enabled: true }.is_actuator_write());
        assert!(!Effect::ClearActionRequests.is_actuator_write());
        assert!(!Effect::RequestReboot.is_actuator_write());
    }

    #[test]
    fn every_effect_has_a_name() {
        // Guards against a variant being added without a log name, which is
        // the only way a new effect stays invisible in a trace.
        let all = [
            Effect::EnablePump,
            Effect::DisablePump,
            Effect::OpenWaterValve,
            Effect::CloseWaterValve,
            Effect::OpenSteamValve,
            Effect::CloseSteamValve,
            Effect::EnableHeater,
            Effect::DisableHeater,
            Effect::SetHeaterDuty(0.0),
            Effect::EmergencyShutdown,
            Effect::SafeHardwareShutdown,
            Effect::ExitState(MachineState::Init),
            Effect::EnterState(MachineState::Init),
            Effect::SetPidRuntime { enabled: false },
            Effect::SetSteamMode { enabled: false },
            Effect::RecordBrew {
                elapsed_ms: 0.0,
                weight: 0.0,
                scale_enabled: false,
            },
            Effect::ResetShotsSinceBackflush,
            Effect::ClearActionRequests,
            Effect::ClearStaleStopRequests,
            Effect::ResetStandbyTimer,
            Effect::ResetMqttReconnectCount,
            Effect::WakeDisplay,
            Effect::RequestReboot,
            Effect::PumpTimeoutFired {
                watchdog: PumpWatchdog::Brew,
            },
        ];
        for effect in all {
            assert!(!effect.name().is_empty());
        }
    }
}
