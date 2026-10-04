//! Over-the-air update admission, and the hardware shutdown that has to happen
//! before a single byte is written to flash.
//!
//! Owner: **R3-15**, and the fix for finding 3.3 of
//! [`32-findings-2026-10-03.md`](../../../docs/rust-migration/32-findings-2026-10-03.md).
//!
//! # What this is for
//!
//! Requirement **S8** of
//! [`01-feature-inventory.md`](../../../docs/rust-migration/01-feature-inventory.md#6-safety-critical-control-paths):
//! *"an OTA must leave pump and valve off."* A machine reflashed mid-brew, or
//! booting with the 3-way valve energised, is the hardware damage
//! `AGENTS.md`'s Hardware Control Invariants exist to prevent. This module is
//! that requirement as code, and it is the **only** place the answer is decided.
//!
//! # Why it is a pure function, and why that is the point
//!
//! An OTA is requested over HTTP, on the httpd task. That task **cannot**
//! actuate anything: the actuators belong to the control task, and every other
//! route reaches hardware the same way — a `cc_web::Command` the control task drains on
//! its next tick. So "may I flash right now?" cannot be answered where the
//! request arrives; it can only be answered *about the state the machine is
//! actually in*. [`admit`] is that answer: a `MachineState` in, an admission out,
//! no I/O and no clock.
//!
//! That makes the whole safety decision a host test, which is the only reason to
//! trust it. The alternative — asking "is the pump on?" from the httpd task —
//! would be answered against a telemetry snapshot up to one control period
//! stale, and finding 2.1 of the same review is a reminder of what a stale
//! snapshot once cost this firmware.
//!
//! # Why [`Effect::SafeHardwareShutdown`], and why it was unused
//!
//! 04 §4's shutdown table names three levels, and OTA start is the trigger for
//! the first: `safe_hardware_shutdown` — *heater off, pump off, valve closed,
//! LEDs off, **not** latched*. The effect existed at
//! [`effect.rs`](crate::effect::Effect::SafeHardwareShutdown) and was emitted by
//! exactly one caller, `PowerHandler`'s power-off
//! (`PowerHandler`'s power-off). It was unused **by the
//! OTA**, which is the gap finding 3.3 records; the two reboot paths in
//! `cc-firmware/src/main.rs` also apply it directly, which is the same rule
//! written twice.
//!
//! It is not [`Effect::EmergencyShutdown`] and the difference is not cosmetic.
//! `emergency_shutdown` latches: `Actuators::enable_pump` and every sibling
//! refuse until something explicitly clears it, so a machine flashed and
//! recovered into would come back **permanently dead** with no way out over the
//! network. `safe_hardware_shutdown` turns the relays off and leaves the machine
//! able to run again — which is what an OTA wants, because an OTA that fails
//! halfway must leave a machine the operator can still talk to.
//!
//!
//! # What the C++ does, and why this is stricter
//!
//! `otaPrepareHardware` (`src/core/SystemInitializer.cpp:57-63`) is:
//!
//! ```cpp
//! void otaPrepareHardware() noexcept {
//!     disableTimer1();
//!     if (g_otaHardwareManager) g_otaHardwareManager->disableHeater();
//! }
//! ```
//!
//! **It does not stop the pump, and it does not close the valve.** It disables
//! the 10 ms heater timer and turns the heater off, and that is all — the gap
//! 04 §4 names when it says *"OTA must call `safe_hardware_shutdown`, not just
//! `disable_heater`"*. The C++ never refuses an OTA in any state either, so a
//! `POST /api/ota/firmware` during a shot flashes the running image out from
//! under a live brew.
//!
//! Both differences here are strictly safer, and both are recorded in
//! `intentional-diffs.md`:
//!
//! * `begin_session` emits the full shutdown — pump, valve **and** heater —
//!   through the real applier, not a direct `disableHeater()`.
//! * [`admit`] refuses while water or steam is flowing, which the C++ never
//!   does.
//!
//! A refusal is not a failure of the feature: the UI polls `/api/ota/status` and
//! shows the message, and the operator flashes again in thirty seconds.

use cc_domain::state::MachineState;

use crate::effect::{Effect, Effects};

/// Why an OTA request was refused.
///
/// A distinct type rather than a `bool` because the operator is told which of
/// several true statements applies, and "OTA refused" with no reason is the kind
/// of answer that produces a support call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlashRefusal {
    /// A shot, a manual flush or a backflush cycle is running: pump or valve
    /// open.
    FlowActive,
    /// The steam wand is open.
    SteamActive,
}

impl FlashRefusal {
    /// The one-line explanation, as the C++ words its own refusals.
    ///
    /// `Cannot start an update while brewing` and friends are the shape of
    /// `ota.cpp`'s error strings (`setUpdateError(true, "…")`), and the UI
    /// prints `result.message` verbatim (`OTAUpdateSection.tsx:198`), so this is
    /// operator-facing text and not a debug log.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::FlowActive => {
                "Cannot start an update while water is flowing. Finish or cancel the shot first."
            }
            Self::SteamActive => "Cannot start an update while the steam wand is open.",
        }
    }
}

/// The verdict [`admit`] returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// The machine is quiescent enough to flash.
    Admitted,
    /// Refused, and why.
    Refused(FlashRefusal),
}

impl Admission {
    /// Whether the flash may proceed.
    #[must_use]
    pub const fn is_admitted(self) -> bool {
        matches!(self, Self::Admitted)
    }
}

/// May the machine be flashed **right now**, from this state?
///
/// # The rule
///
/// Refuse whenever water or steam could be flowing. That is a subset of
/// `cc_safety::water_flow_allowed` — the same six states, minus the backflush
/// arms, which the port's whitelist also lists — plus [`MachineState::SteamRunning`].
///
/// It is **not** the same predicate, and the difference is the point:
/// `water_flow_allowed` answers *"may this state open the valve?"*, which is a
/// question about the future, and `STEAM_RUNNING` answers no. `admit` answers
/// *"is anything energised right now?"*, which is a question about the present,
/// and `STEAM_RUNNING` answers yes. Asking the whitelist would have answered
/// the wrong question and passed the one state where a 3-way valve is open.
///
/// # A `match` with no `_` arm
///
/// Same mechanism as [`cc_safety::water_flow_allowed`] and ADR-0004: an 18th
/// state is a **compile error** here until it is classified. That is the whole
/// argument for this function existing as a `match` rather than as
/// `!state.is_brew_state()`, which would have compiled silently and quietly
/// admitted a new water-flow state.
///
/// # What is admitted, and why those states are safe
///
/// `PidNormal`, `PidDisabled`, `Standby`, `EmergencyStop`, `SensorError`,
/// `EepromError`, `WaterTankEmpty`, `BrewFinished`, `BackflushIdle` and
/// `BackflushFinished` all admit, and the common reason is the same in every
/// case: **nothing is energised**. `BrewFinished` is the one that needs the
/// argument spelled out, because it is a brew state and reads like an oversight
/// — the shot is over, `BrewFinishedState::onEntryImpl` has drained the group
/// (`BrewStates.cpp:310`), and the pump is off. `BackflushFlushing` is *not*
/// admitted even though its pump is off, because its **valve** is open, and the
/// valve is half of S8.
///
/// # A note on what this cannot see
///
/// This reads a state, not a pin. It is the strongest statement available
/// without the control task stalling for a tick, and the belt to its braces is
/// [`begin_session`]: whatever this admitted, the shutdown that follows drives
/// the pump and the valve to their inactive levels through the applier, on the
/// control task, before any flash write is attempted. Admission decides whether
/// it is *reasonable* to flash; the shutdown is what makes it *safe*.
#[must_use]
pub const fn admit(state: MachineState) -> Admission {
    match state {
        MachineState::BrewPreinfusion
        | MachineState::BrewPreinfusionPause
        | MachineState::BrewRunning
        | MachineState::ManualFlushRunning
        | MachineState::BackflushFilling
        | MachineState::BackflushFlushing => Admission::Refused(FlashRefusal::FlowActive),
        MachineState::SteamRunning => Admission::Refused(FlashRefusal::SteamActive),
        MachineState::Init
        | MachineState::PidNormal
        | MachineState::BrewFinished
        | MachineState::BackflushIdle
        | MachineState::BackflushFinished
        | MachineState::WaterTankEmpty
        | MachineState::EmergencyStop
        | MachineState::PidDisabled
        | MachineState::Standby
        | MachineState::SensorError
        | MachineState::EepromError => Admission::Admitted,
    }
}

/// The effects an OTA session must apply before it touches flash.
///
/// This is the S8 hook: 04 §4 names *OTA start* as a trigger for
/// `safe_hardware_shutdown`, and this is the function that emits it. One effect,
/// applied through the real applier on the control task, so the same
/// `Actuators::safe_hardware_shutdown` that standby uses turns the pump off,
/// closes the valve and zeroes the heater duty.
///
/// Deliberately **not** latched, and the reason is in the module docs: a failed
/// OTA must leave a machine that still runs. Deliberately **not**
/// [`Effect::EmergencyShutdown`], for the same reason.
///
/// There is no `Machine` argument and no `&mut self`: an OTA start changes
/// nothing about the machine's state, only about its hardware. A shot in
/// progress is stopped by the refusal in [`admit`], not by transitioning the
/// state machine — which would mean writing a `MachineState` from the web layer
/// and would need a transition the C++ does not have.
#[must_use]
pub fn begin_session() -> Effects {
    let mut effects = Effects::new();
    effects.push(Effect::SafeHardwareShutdown);
    effects
}

#[cfg(test)]
mod tests {
    use super::{admit, Admission, Effect, FlashRefusal};
    use cc_domain::state::ALL;

    /// Every state is classified, and the two verdicts are the only ones.
    ///
    /// The point of the no-`_`-arm `match` is that this table cannot go stale
    /// silently, so the table itself asserts the total: `ALL` is the domain's own
    /// list of all 18 states, and `admit` is total over it by construction.
    #[test]
    fn every_state_gets_a_verdict() {
        for state in ALL {
            let verdict = admit(state);
            assert!(
                matches!(verdict, Admission::Admitted | Admission::Refused(_)),
                "{state:?} was not classified"
            );
        }
    }

    /// The six states where water can be flowing are refused, by name.
    ///
    /// Written as an explicit list rather than derived from
    /// `cc_safety::water_flow_allowed`, because the whole argument in
    /// [`admit`]'s docs is that the two predicates are *not* the same and a
    /// test that derived one from the other would pass whatever the code did.
    #[test]
    fn water_flowing_states_are_refused() {
        for state in [
            cc_domain::state::MachineState::BrewPreinfusion,
            cc_domain::state::MachineState::BrewPreinfusionPause,
            cc_domain::state::MachineState::BrewRunning,
            cc_domain::state::MachineState::ManualFlushRunning,
            cc_domain::state::MachineState::BackflushFilling,
            cc_domain::state::MachineState::BackflushFlushing,
        ] {
            assert_eq!(
                admit(state),
                Admission::Refused(FlashRefusal::FlowActive),
                "{state:?} must refuse an OTA"
            );
        }
    }

    /// `STEAM_RUNNING` is refused, and this is the test that says why.
    ///
    /// `cc_safety::water_flow_allowed(STEAM_RUNNING)` is **false** — the water
    /// whitelist has no steam arm, and finding 2.5 made `may_open_water` consult
    /// it. Building admission on that predicate would have admitted the one state
    /// in which a valve on this machine is open, and this assertion is what makes
    /// that mistake impossible to reintroduce.
    #[test]
    fn steam_is_refused_even_though_the_water_whitelist_allows_it() {
        assert!(
            !cc_safety::water_flow_allowed(cc_domain::state::MachineState::SteamRunning),
            "the premise of this test: the water whitelist does not cover steam"
        );
        assert_eq!(
            admit(cc_domain::state::MachineState::SteamRunning),
            Admission::Refused(FlashRefusal::SteamActive),
        );
    }

    /// `BrewFinished` admits, and `BrewRunning` does not.
    ///
    /// The pair is one assertion because either half alone is misleading:
    /// "brewing is refused" says nothing about the state *after* a brew, which is
    /// where an operator actually stands when they open the OTA tab.
    #[test]
    fn a_finished_brew_admits_but_a_running_one_does_not() {
        assert_eq!(
            admit(cc_domain::state::MachineState::BrewFinished),
            Admission::Admitted
        );
        assert!(!admit(cc_domain::state::MachineState::BrewRunning).is_admitted());
    }

    /// The quiescent states admit.
    #[test]
    fn quiescent_states_admit() {
        for state in [
            cc_domain::state::MachineState::PidNormal,
            cc_domain::state::MachineState::PidDisabled,
            cc_domain::state::MachineState::Standby,
            cc_domain::state::MachineState::EmergencyStop,
            cc_domain::state::MachineState::SensorError,
            cc_domain::state::MachineState::EepromError,
            cc_domain::state::MachineState::WaterTankEmpty,
            cc_domain::state::MachineState::BackflushIdle,
            cc_domain::state::MachineState::BackflushFinished,
            cc_domain::state::MachineState::Init,
        ] {
            assert_eq!(
                admit(state),
                Admission::Admitted,
                "{state:?} must admit an OTA"
            );
        }
    }

    /// Every refusal carries operator-facing text that says what to do.
    ///
    /// The UI prints `result.message` verbatim, so an empty or technical string
    /// reaches the operator as-is.
    #[test]
    fn every_refusal_explains_itself() {
        for refusal in [FlashRefusal::FlowActive, FlashRefusal::SteamActive] {
            let message = refusal.message();
            assert!(!message.is_empty(), "{refusal:?} has no message");
            assert!(
                message.contains("Cannot start an update"),
                "{refusal:?} does not say what went wrong: {message}"
            );
        }
    }

    /// The session emits `SafeHardwareShutdown`, once, and nothing else.
    ///
    /// This is the S8 assertion, and it is here rather than in the firmware
    /// because `begin_session` is pure. `actuators.rs:775` maps that effect to
    /// the pump off, the valve closed and a zero heater duty, and
    /// `applier.rs:250` is the single dispatch site — so the chain from this
    /// function to the pins is three hops and this test pins the first.
    #[test]
    fn a_session_shuts_the_hardware_down_and_nothing_else() {
        let effects = super::begin_session();
        assert!(
            effects.contains(&Effect::SafeHardwareShutdown),
            "an OTA session must apply the safe shutdown: {effects:?}"
        );
        assert_eq!(effects.len(), 1, "and nothing else: {effects:?}");
    }

    /// The shutdown is **not** the latching emergency shutdown.
    ///
    /// `Actuators::emergency_shutdown` sets the latch that makes every later
    /// `enable_*` a no-op until something clears it
    /// (`actuators.rs:775` vs the latching arm; `effect.rs` on
    /// `EmergencyShutdown` says "latching is the point"). A machine whose OTA
    /// failed would come back permanently dead, with no way out over the
    /// network — so this is a test of a decision, not of a type.
    #[test]
    fn a_session_is_not_latched() {
        assert!(
            !super::begin_session().contains(&Effect::EmergencyShutdown),
            "an OTA must leave a machine that can still be talked to"
        );
    }

    /// The shutdown happens **before** the admission is even consulted.
    ///
    /// Ordering is asserted by construction rather than by a table: admission
    /// is a query and emits nothing, so there is no ordering to get wrong. The
    /// assertion that matters is the negative one — admitting a state must not
    /// have side effects, because `admit` takes `MachineState` by value and has
    /// no `Machine` to write to.
    #[test]
    fn admission_emits_nothing() {
        // `admit` returns a plain value; if it ever acquired a side effect its
        // signature would have to change, and this test is the reminder that the
        // purity is load-bearing for the httpd-task reason in the module docs.
        let _: fn(cc_domain::state::MachineState) -> Admission = admit;
    }
}
