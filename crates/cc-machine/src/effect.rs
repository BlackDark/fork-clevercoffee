//! The only way out of the reducer: what the machine wants done.
//!
//! # Why effects and not calls
//!
//! An [`Effect`] is a *description*. Nothing in the machine calls an actuator, so a new
//! state cannot poke a relay even by accident: it has no way to reach one, and can only
//! return an `Effect`. The bookkeeping that makes direct calls dangerous -- a valve flag
//! that can drift from the pin it describes -- has nowhere to live here.
//!
//! [`reduce`](crate::reduce) returns a list, and `crate::applier::apply` is the single
//! function that turns a list into actuator calls. A test asserts hardware by inspecting the
//! list, and needs no mock object and no GPIO.
//!
//! # Order
//!
//! Order is part of the contract, not an implementation detail: the old state's exit runs
//! before the new state's entry, the tick's own effects follow, and the S5 valve fail-safe
//! runs *after* all of it. Two effects that write the same relay in the same tick resolve
//! differently depending on order, so the applier applies the vector front to back and the
//! tests assert on exact sequences, not sets.
//!
//! # The list does not allocate -- see [`Effects`]
//!
//! `Effect` is `Copy` and small; the *list* of them is what the control loop churned
//! through. One allocation per [`reduce`](crate::reduce), four to five `reduce` calls per
//! 10 ms tick, plus two more in the shell. See [`MAX_EFFECTS_PER_EVENT`] for the measurement
//! and [`Effects::dropped`] for what happens if the ceiling is ever reached.
use core::fmt;
use core::ops::Deref;

use cc_domain::state::MachineState;

/// Something the machine wants done.
///
/// Three groups, declared in the order they are applied: actuator writes, state-machine
/// bookkeeping, and everything else. The grouping is a comment, not a type -- a single enum
/// keeps one list and one match in the applier, and the applier is the only place that has
/// to be exhaustive over hardware.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    // ---- actuator writes: the only things that may change a pin ------------
    /// Energise the pump relay.
    EnablePump,
    /// De-energise the pump relay.
    DisablePump,
    /// Open the water valve.
    OpenWaterValve,
    /// Close the water valve.
    CloseWaterValve,
    /// Open the steam valve.
    ///
    /// **Gated, deliberately.**
    ///
    /// `cc_safety::steam_flow_allowed` admits `STEAM_RUNNING` and nothing else, and it is
    /// consulted in two places: the reducer's tail closes the valve in every other state,
    /// and the applier's verdict refuses the actuator call. The effect still exists because
    /// the state machine must be able to express it even where the whitelist will refuse it
    /// -- and on this machine it shares a relay with the water valve, so "nothing opens it"
    /// is a fact about the call graph rather than a safety property.
    OpenSteamValve,
    /// Close the steam valve.
    CloseSteamValve,
    /// Command the heater on. Only the chopper decides the duty; see [`Self::SetHeaterDuty`].
    EnableHeater,
    /// Drive the heater off.
    DisableHeater,
    /// The heater duty, in milliseconds of on-time per 1000 ms window: `0..=1000`.
    ///
    /// Not a percentage and not a count of 10 ms ticks: this is the PID output's own unit,
    /// the one the chopper compares against its counter. The applier hands it to the heater
    /// output, which owns the conversion.
    SetHeaterDuty(f32),
    /// All relays off, and **latched**: every later actuator effect is refused until the
    /// latch is cleared.
    ///
    /// Distinct from `DisablePump + CloseWaterValve + DisableHeater`, which leaves the
    /// machine able to run again. Latching is the point of this one: nothing may re-energise
    /// the hardware while the emergency stop is in force, and a machine that stayed latched
    /// has to have its latch cleared deliberately.
    EmergencyShutdown,
    /// All relays off, **without** latching.
    ///
    /// The counterpart of [`Effect::EmergencyShutdown`]: the hardware stops, and the machine
    /// can still run afterwards, which is what standby recovery, a reboot and an OTA session
    /// all need.
    SafeHardwareShutdown,

    // ---- state-machine bookkeeping ----------------------------------------
    /// The state being left. Emitted before the old state's exit effects.
    ExitState(MachineState),
    /// The state being entered. Emitted before the new state's entry effects.
    EnterState(MachineState),
    /// Turn the PID's runtime participation on or off.
    ///
    /// Runtime only: it never writes `configuration.pid.enabled`. That distinction is what
    /// lets a state's exit restore the PID from the configuration, rather than from whatever
    /// value the previous state happened to leave behind.
    SetPidRuntime {
        /// The new runtime PID state.
        enabled: bool,
    },
    /// Turn steam mode on or off, and with it the "steam came first" flag the display's
    /// status line uses.
    SetSteamMode {
        /// Whether steam mode is now on.
        enabled: bool,
    },
    // NOTE: the brew timer and the backflush cycle counter are deliberately **not**
    // effects. They are `Machine::brew` and `Machine::backflush`, and `reduce` hands the
    // applier the new `Machine`, so the applier can already read them. An effect that
    // restates a field the applier can read is a second source of truth waiting to disagree
    // with the first.
    /// Persist a counted shot.
    ///
    /// **The decision, not the facts.** The counter is
    /// [`Machine::shots_since_backflush`](crate::machine::Machine::shots_since_backflush),
    /// which only the reducer may write, so
    /// [`crate::maintenance::record_brew_if_qualified`] has already run by the time this
    /// effect is emitted and `counted` is its answer. The effect is therefore the
    /// *durability* half -- the number to write down -- and nothing else.
    ///
    /// It carries `counted` rather than the three inputs it was derived from so that the
    /// shell never re-derives it. With the inputs on the effect instead, the qualification
    /// rule would be evaluated only by a side channel that had to be *wired* to it, and a
    /// rule that has to be wired can be forgotten: the counter would sit at 0 for the life
    /// of the firmware with nothing to fail. A rule evaluated once, where the value it
    /// changes lives, cannot be forgotten that way.
    RecordBrew {
        /// Whether the shot qualified.
        counted: bool,
    },
    /// Reset the shot counter: a backflush cycle finished.
    ResetShotsSinceBackflush,
    /// Drain every action request.
    ///
    /// What S11 requires of a state that cannot act on requests. The *effect* is emitted so
    /// the applier's flag mirror stays in step with
    /// [`Machine::requests`](crate::machine::Machine::requests); the reducer has already
    /// applied it.
    ClearActionRequests,
    /// Drain the stop requests and leave the starts.
    ///
    /// Not the same as [`Effect::ClearActionRequests`]: `STANDBY` keeps the start flags as
    /// wake triggers, so only the stops are stale there.
    ClearStaleStopRequests,
    /// Restart the standby countdown: something counted as user activity.
    ResetStandbyTimer,
    /// Reset the MQTT reconnect backoff, on the way out of standby.
    ResetMqttReconnectCount,
    /// Bring the panel out of power save.
    WakeDisplay,
    /// Show the reboot message, shut the hardware down safely, and restart the chip.
    ///
    /// The reducer itself cannot restart anything: the applier does it, and the
    /// [`POWER_REBOOT_DISPLAY_MS`](crate::timing::POWER_REBOOT_DISPLAY_MS) pause is a
    /// *shell* concern, because a pure function may not sleep.
    RequestReboot,
    /// A pump watchdog expired.
    ///
    /// Arming this watchdog is deliberate,
    /// which is what makes this effect reachable at all. It exists *so the trip is visible
    /// in the field log*, rather than being a silent hardware change: without a log line, a
    /// machine that stops mid-shot looks like a fault rather than a limit.
    ///
    /// The applier turns it into the log line, via [`PumpWatchdog::message`]. The *action*
    /// that follows (`Request::BrewStop` / `Effect::DisablePump`) is a separate effect, so
    /// the order is the reducer's choice rather than a side effect of logging.
    PumpTimeoutFired {
        /// Which watchdog fired.
        watchdog: PumpWatchdog,
    },
}

/// Which pump watchdog expired.
///
/// Two separate timers with two separate deadlines, so two variants rather than one flag.
/// See [`timing::BREW_PUMP_TIMEOUT_MS`](crate::timing::BREW_PUMP_TIMEOUT_MS) and
/// [`timing::HOT_WATER_PUMP_TIMEOUT_MS`](crate::timing::HOT_WATER_PUMP_TIMEOUT_MS).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PumpWatchdog {
    /// The brew pump's watchdog: five minutes.
    Brew,
    /// The hot-water pump's watchdog: one minute.
    HotWater,
}

impl PumpWatchdog {
    /// The operator-facing line for a trip.
    ///
    /// It is the only place that says *which* watchdog fired, so re-wording one
    /// changes what the log shows.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Brew => "Pump timeout - stopping for safety",
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
/// Because it was, and it cost four heap allocations per 10 ms control tick. See
/// `tests/tick_allocations.rs`, which is the measurement and the regression test in one.
///
/// The control task calls [`reduce`](crate::reduce) four to five times per tick (sensor,
/// safety, switch edges, PID output, tick), and each call built a fresh `Vec`. Measured on
/// the host against the real code, in a `Control::tick`-shaped harness: **4.00 allocations
/// and ~192 bytes per tick**, i.e. roughly 400 allocations per second, on a device with
/// about 320 KB of RAM, driven by the same loop that runs the heater deadman.
/// `Control::tick` allocated a second list and `cc-firmware`'s `main.rs` a third.
///
/// `heapless::Vec` with a `const` capacity is the same shape with the storage in the value:
/// no allocator call, no fragmentation, and the capacity is a compile-time constant.
/// `heapless` with `default-features = false` is core-only -- the `alloc` feature the
/// workspace entry enables adds `Vec`/`String` conversions this crate does not use, and it is
/// already in the device graph via `cc-hal-esp32`, so naming it here adds no second copy.
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
    /// and every test failure message in this crate quotes that form.
    /// `[.., dropped: 2]` would be more informative when something has
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
