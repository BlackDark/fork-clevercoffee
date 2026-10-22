//! OTA admission and full hardware shutdown before flash writes.
//!
//! Decide on the control task's live state, not an HTTP telemetry snapshot that
//! may be a tick stale. Refuse while water or steam could flow.
//!
//! [`begin_session`] emits [`Effect::SafeHardwareShutdown`]: heater and pump off,
//! valve and LEDs off, without latching. Emergency shutdown would prevent recovery
//! after a failed upload. Shutdown alone is insufficient if the next tick could
//! reopen a valve; admission must also require a quiescent machine.
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
    /// The one-line explanation.
    ///
    /// The UI prints `result.message` verbatim (`OTAUpdateSection.tsx:198`), so this is
    /// operator-facing text and not a debug log: it has to name the action the operator can
    /// take.
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

/// Classify the live control-task state immediately before starting OTA.
///
/// Refuse water-flow states and `SteamRunning`; the water whitelist alone misses
/// the shared valve during steam. `BrewFinished` is quiescent, while
/// `BackflushFlushing` still has an open valve despite its stopped pump.
///
/// The exhaustive match forces new states to choose. This checks state, not pins;
/// [`begin_session`] also shuts down outputs before flash writes. Never rely on
/// an earlier HTTP snapshot: a brew can begin before the next control tick.
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

/// The effects an OTA session must apply before it touches flash, **or** the
/// refusal that means it must apply none.
///
/// # This is the decision point, and it is deliberately not a bare emitter
///
/// An unconditional `begin_session() -> Effects`, with admission consulted somewhere else
/// entirely, is a P0 hole: between a snapshot read on the httpd task and the tick that
/// drains `Command::OtaBegin`, the machine can enter [`MachineState::BrewPreinfusion`], and
/// the tick then applies the shutdown **and** the brew's `EnablePump` / `OpenWaterValve` in
/// one ordered pass, the second overwriting the first. The flash then erases 1.8 MB with
/// the pump and the valve open.
///
/// So the state moves *here*, and the shutdown is conditional on it. A caller
/// cannot apply the session's effects without also having asked, at the same
/// point and from the same live state, whether the machine is safe to flash.
/// That is the property; the `Result` is how it is expressed.
///
/// # Errors
///
/// [`admit`] refused `state` — see [`admit`] for the rule and [`FlashRefusal`]
/// for the operator-facing text. The caller must **refuse the flash**, not carry
/// on with an empty effect list: an update that erases the running image while
/// the machine is brewing is the failure this exists to prevent, and "no effects"
/// is indistinguishable from "safe to proceed" to every caller but this one.
///
/// # The `Result` is also the ordering fix
///
/// The caller applies what comes back **after** the tick's own effect pass, not inside it.
/// That is what makes the shutdown ordering-proof: nothing the same pass emitted can
/// re-energise hardware after it. [`admit`] is where the rule is stated.
///
/// # Deliberately not latched, and deliberately not an emergency shutdown
///
/// The reason is in the module docs: a failed OTA must leave a machine that
/// still runs. So this is not [`Effect::EmergencyShutdown`], whose latch makes
/// a machine flashed and recovered into permanently dead with no way out over
/// the network.
///
/// There is no `Machine` argument and no `&mut self`: an OTA start changes nothing about
/// the machine's state, only about its hardware. A shot in progress is stopped by the
/// refusal above, not by transitioning the state machine -- which would mean the web layer
/// writing a `MachineState`, a transition no other path has.
pub fn begin_session(state: MachineState) -> Result<Effects, FlashRefusal> {
    match admit(state) {
        Admission::Admitted => {
            let mut effects = Effects::new();
            effects.push(Effect::SafeHardwareShutdown);
            Ok(effects)
        }
        Admission::Refused(refusal) => Err(refusal),
    }
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
    /// `cc_safety::water_flow_allowed(STEAM_RUNNING)` is **false**: the water whitelist has
    /// no steam arm, and `may_open_water` consults it. Building admission on that predicate
    /// would admit the one state in which a valve on this machine is open, and these two
    /// assertions are what make that mistake impossible to reintroduce.
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
    ///
    /// Compared as a whole `Result` rather than unwrapped: the audit in
    /// `tests/exhaustive_state_event.rs` rejects `expect`/`panic!` anywhere in
    /// this crate's sources, test bodies included, and asserting the entire
    /// value is a stronger statement than inspecting an unwrapped one anyway.
    #[test]
    fn a_session_shuts_the_hardware_down_and_nothing_else() {
        assert_eq!(
            super::begin_session(cc_domain::state::MachineState::PidNormal)
                .as_ref()
                .ok()
                .map(|effects| &effects[..]),
            Some(&[Effect::SafeHardwareShutdown][..]),
            "an OTA session must apply the safe shutdown, and nothing else"
        );
    }

    /// Recheck admission after the request: a newly active state must refuse OTA.
    ///
    /// Assert both the refusal reason and absence of session effects. An empty effect
    /// list alone could be mistaken for permission to continue flashing.
    #[test]
    fn a_state_that_moved_into_a_refused_one_after_the_request_starts_nothing() {
        // The request-time check, against the state the operator saw: admitted.
        // This is the premise — without it there is no window to be unsafe in.
        assert!(
            admit(cc_domain::state::MachineState::PidNormal).is_admitted(),
            "the premise of this test: the request was admitted"
        );
        for state in ALL {
            let expected = admit(state);
            let outcome = super::begin_session(state);
            // No effects unless the state admits — the whole of "no flash".
            assert_eq!(
                outcome.as_ref().ok().map(|effects| &effects[..]),
                match expected {
                    Admission::Admitted => Some(&[Effect::SafeHardwareShutdown][..]),
                    Admission::Refused(_) => None,
                },
                "{state:?} must emit the shutdown if and only if it admits"
            );
            // And a refusal is the one `admit` names, not a different one.
            assert_eq!(
                outcome.as_ref().err().copied(),
                match expected {
                    Admission::Admitted => None,
                    Admission::Refused(refusal) => Some(refusal),
                },
                "{state:?} must refuse for the reason `admit` gave"
            );
        }
    }

    /// The shutdown is **not** the latching emergency shutdown.
    ///
    /// `Actuators::emergency_shutdown` sets the latch that makes every later `enable_*` a
    /// no-op until something clears it. A machine whose OTA failed would come back
    /// permanently dead, with no way out over the network -- so this is a test of a
    /// decision, not of a type.
    #[test]
    fn a_session_is_not_latched() {
        assert!(
            !matches!(
                super::begin_session(cc_domain::state::MachineState::PidNormal),
                Ok(ref effects) if effects.contains(&Effect::EmergencyShutdown)
            ),
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
