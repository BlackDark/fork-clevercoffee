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
//!
//! # The list does not allocate — see [`Effects`]
//!
//! `Effect` is `Copy` and small; the *list* of them is what the control loop
//! churned through. One allocation per [`reduce`](crate::reduce), four to five
//! `reduce` calls per 10 ms tick, plus two more in the shell. See
//! [`MAX_EFFECTS_PER_EVENT`] for the measurement and [`Effects::dropped`] for
//! what happens if the ceiling is ever reached.

use core::fmt;
use core::ops::Deref;

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
    ///
    /// **The decision, not the facts.** The C++'s one call applies the
    /// qualification rule, increments the counter and writes NVS. Here the
    /// counter is [`Machine::shots_since_backflush`](crate::machine::Machine::shots_since_backflush),
    /// which only the reducer may write, so
    /// [`crate::maintenance::record_brew_if_qualified`] has already run by the
    /// time this effect is emitted and `counted` is its answer. The effect is
    /// therefore the *durability* half — the number to write down — and nothing
    /// else.
    ///
    /// It carries `counted` rather than the three inputs it was derived from so
    /// that the shell never re-derives it. This is the whole of the bug this
    /// shape fixes: with the inputs on the effect, the qualification rule was
    /// only evaluated by a side channel that had to be *wired* to it, it was
    /// not, and the counter sat at 0 for the life of the firmware with nothing
    /// to fail. A rule evaluated once, where the value it changes lives, cannot
    /// be forgotten that way.
    RecordBrew {
        /// `qualifiesAsCountedShot(totalBrewTimeMs, brewWeight, scaleEnabled)`.
        counted: bool,
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

/// How many effects one [`reduce`](crate::reduce) call may produce.
///
/// # Where 16 comes from, and why it is a ceiling and not a hope
///
/// Measured against the real code over the whole `state x event` table plus 200
/// consecutive ticks per pair (`tests/exhaustive_state_event.rs`): the longest
/// list any single `reduce` produces is **4** — `DisablePump`, `CloseWaterValve`,
/// The worst effect list the exhaustive table has ever produced.
///
/// **12**, at `pid_off/BrewRunning x SensorUpdated`. That is not the 3-4 a
/// casual measurement suggests: sampling only `PID_NORMAL` -- the state the
/// control loop actually sits in for most of a brew -- gives `[DisablePump,
/// CloseWaterValve, CloseSteamValve]`, three effects. The worst case is
/// `PID_DISABLED`-flavoured `BREW_RUNNING`, whose exit re-asserts every
/// actuator and close every valve, all in one tick as the state changes.
///
/// Recorded as a `const` so the number is a fact about the machine rather than
/// a claim in a comment, and so the headroom assertion in
/// `tests/exhaustive_state_event.rs::ceiling_is_never_reached` has something to
/// compare against.
pub const WORST_EFFECTS_OBSERVED: usize = 12;

/// Capacity of one effect list.
///
/// **32**, i.e. two and a half times [`WORST_EFFECTS_OBSERVED`].
///
/// It is a `const` and not a `heapless::Vec` sized to the measurement on
/// purpose. A ceiling of exactly 12 would be exactly the observed worst case, so
/// any future state that emitted one more effect in one tick would drop one — and
/// the one it dropped could be [`Effect::CloseWaterValve`], which is the S5
/// fail-safe and the single most safety-relevant effect in the list.
///
/// 32 is still 512 bytes of stack in the control task's frame, with **no heap
/// involvement**, against a ~4 s budget of heap traffic that this removes
/// entirely (four `alloc::vec::Vec`s per 10 ms tick, measured at 4.00
/// allocations and ~192 B). The headroom turns "a change we did not predict"
/// from a silent hardware regression into a counted drop that
/// [`Effects::dropped`] reports and the exhaustive test asserts against.
///
/// # What is actually guaranteed
///
/// Nothing at the type level. 32 is a bound the code *currently* respects, and
/// `tests/exhaustive_state_event.rs::ceiling_is_never_reached` is the test that
/// keeps it honest: it drives all 4,140 state/event pairs plus 64 ticks after
/// each, asserts `dropped() == 0`, and asserts the worst case still has at
/// least 2x headroom. If a future change exceeds the ceiling, that test fails
/// rather than the firmware quietly losing an effect in the field.
pub const MAX_EFFECTS_PER_EVENT: usize = 32;

/// The effects produced by one [`reduce`](crate::reduce).
///
/// # Why this is not `alloc::vec::Vec`
///
/// Because it was, and it cost four heap allocations per 10 ms control tick.
///
/// The control task calls [`reduce`](crate::reduce) four to five times per tick
/// (sensor, safety, switch edges, PID output, tick), and each call built a fresh
/// `Vec`. Measured on the host against the real code, in a `Control::tick`-shaped
/// harness: **4.00 allocations and ~192 bytes per tick**, i.e. roughly 400
/// allocations per second, driven by the same loop that runs the heater deadman
/// — on a device with about 320 KB of RAM. `Control::tick` allocated a second
/// list and `cc-firmware`'s `main.rs` a third.
///
/// The C++ allocated nothing here. `LoopManager::update`
/// (`src/core/LoopManager.cpp:90-253`) wrote relays inline into member state,
/// so the effect list only ever existed as the C++ call stack.
///
/// `heapless::Vec` with a `const` capacity is the same shape with the storage in
/// the value: no allocator call, no fragmentation, and the capacity is a
/// compile-time constant. `heapless` with `default-features = false` is
/// core-only — the `alloc` feature the workspace entry enables adds `Vec`/`String`
/// conversions this crate does not use, and it is already in the device graph via
/// `cc-hal-esp32`, so naming it here adds no second copy.
///
/// # Overflow is counted, not ignored
///
/// [`heapless::Vec::push`] returns the effect back when the list is full, and
/// this crate has no `panic!`/`unwrap` in non-test code (asserted by
/// `tests/exhaustive_state_event.rs::no_unreachable_outside_tests`) and forbids
/// `unsafe` outright (`#![forbid(unsafe_code)]`), so `push_unchecked` is not
/// available either. That leaves three honest choices and this is the third:
///
/// 1. `push_unchecked` behind a comment — needs `unsafe`, refused by the crate.
/// 2. `debug_assert!` — fails every test, passes release. A dropped
///    `Effect::CloseWaterValve` in the field, which is the regression this whole
///    change exists to avoid.
/// 3. **Count it** — [`dropped`](Self::dropped) is part of the value, so a lost
///    effect is observable by the caller and by the tests, in every build, with
///    no panic and no `unsafe`.
///
/// The applier applies `self[..]` either way, so an overflow is *not* a silent
/// hardware change; it is a visible count. `ceiling_is_never_reached` asserts
/// the count is zero.
/// `PartialEq` and not `Eq`: [`Effect::SetHeaterDuty`] carries an `f32`, so two
/// lists can compare equal in the IEEE sense and a `NaN` duty makes them
/// unequal. That matches the `Vec<Effect>` this replaced, which had exactly the
/// same property.
#[derive(Clone, PartialEq)]
pub struct Effects {
    list: heapless::Vec<Effect, MAX_EFFECTS_PER_EVENT>,
    dropped: u16,
}

impl Effects {
    /// An empty list, with nothing dropped.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            list: heapless::Vec::new(),
            dropped: 0,
        }
    }

    /// Append one effect, counting it if the list is already full.
    ///
    /// Infallible from the caller's point of view, and total: it always leaves
    /// the list in a valid state. See the type documentation for why overflow is
    /// counted rather than panicking.
    pub fn push(&mut self, effect: Effect) {
        if self.list.push(effect).is_err() {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    /// Append every effect of `other`, in order.
    ///
    /// Same overflow rule as [`push`](Self::push), applied element by element so
    /// that a partially-full list keeps the prefix rather than being replaced.
    pub fn extend(&mut self, other: &Effects) {
        for effect in &other.list {
            self.push(*effect);
        }
    }

    /// How many effects have been lost to a full list.
    ///
    /// Always `0` today; see [`MAX_EFFECTS_PER_EVENT`]. The point of exposing it
    /// is that "can this ever happen?" is then a test assertion
    /// (`tests/exhaustive_state_event.rs::ceiling_is_never_reached`) rather
    /// than an assumption.
    #[must_use]
    pub const fn dropped(&self) -> u16 {
        self.dropped
    }

    /// The effects, front to back. This is what [`applier::apply`](crate::applier::apply) consumes.
    #[must_use]
    pub fn as_slice(&self) -> &[Effect] {
        &self.list
    }
}

impl Default for Effects {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Effects {
    type Target = [Effect];

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl fmt::Debug for Effects {
    /// Formats as the bare list, not as a struct.
    ///
    /// The `Vec<Effect>` this replaced printed `[DisablePump, CloseWaterValve]`,
    /// and every test failure message in this crate and the parity harness quotes
    /// that form. `[.., dropped: 2]` would be more informative when something has
    /// gone wrong and unreadable when it has not; the count has its own accessor
    /// and its own assertion, so the common case wins.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.list.fmt(f)
    }
}

impl FromIterator<Effect> for Effects {
    /// Saturating: a `from_iter` that drops the tail is exactly the overflow rule
    /// of [`push`](Self::push), so a caller cannot get different behaviour here.
    fn from_iter<I: IntoIterator<Item = Effect>>(iter: I) -> Self {
        let mut effects = Self::new();
        for effect in iter {
            effects.push(effect);
        }
        effects
    }
}

impl IntoIterator for Effects {
    type Item = Effect;
    // The third parameter is heapless's `LenT`, which `heapless::Vec` defaults to
    // `usize` but `IntoIter` does not default.
    type IntoIter = heapless::vec::IntoIter<Effect, MAX_EFFECTS_PER_EVENT, usize>;

    fn into_iter(self) -> Self::IntoIter {
        self.list.into_iter()
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
            Effect::RecordBrew { counted: false },
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
            assert_ne!(effect.name(), "");
        }
    }
}
