//! Reduced observations avoid asserting on irrelevant floating-point differences.
//!
//! State transitions retain [`MachineState::name`] strings; pump timeouts retain
//! their watchdog. Other effect values are dropped. Millisecond timestamps remain
//! so assertions can distinguish immediate actions from one-tick delays.

use std::collections::BTreeMap;

use cc_domain::state::MachineState;
use cc_machine::{Effect, PumpWatchdog};
use serde::{Deserialize, Serialize};

/// One effect, as the observation records it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "e", rename_all = "PascalCase")]
pub enum ObservedEffect {
    /// A bookkeeping effect with no argument, e.g. `EnablePump`.
    Plain,
    /// `EnterState` / `ExitState` — the state name is the observation.
    State {
        /// The `SCREAMING_SNAKE` state name.
        state: String,
    },
    /// `SetPidRuntime` / `SetSteamMode` — the flag is the observation.
    Flag {
        /// The flag's new value.
        enabled: bool,
    },
    /// `SetHeaterDuty` — the duty is deliberately dropped. See the module docs.
    Duty,
    /// `PumpTimeoutFired` — the watchdog is the observation.
    PumpTimeout {
        /// `brew` or `hot_water`.
        watchdog: String,
    },
    /// `RecordBrew` — the elapsed time and weight are dropped for the same
    /// reason the duty is.
    Brew,
}

impl ObservedEffect {
    /// Reduce a real effect to its observation.
    #[must_use]
    pub fn of(effect: &Effect) -> Self {
        match *effect {
            Effect::EnterState(s) | Effect::ExitState(s) => Self::State {
                state: s.name().to_string(),
            },
            Effect::SetPidRuntime { enabled } | Effect::SetSteamMode { enabled } => {
                Self::Flag { enabled }
            }
            Effect::SetHeaterDuty(_) => Self::Duty,
            Effect::PumpTimeoutFired { watchdog } => Self::PumpTimeout {
                watchdog: watchdog_name(watchdog).to_string(),
            },
            Effect::RecordBrew { .. } => Self::Brew,
            _ => Self::Plain,
        }
    }

    /// The effect's stable name, as `Effect::name()` spells it.
    ///
    /// Needed because an assertion names the effect (`never: { effect: EnablePump }`)
    /// while the observation records which one it was.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Plain | Self::State { .. } | Self::Flag { .. } => "",
            Self::Duty => "SetHeaterDuty",
            Self::PumpTimeout { .. } => "PumpTimeoutFired",
            Self::Brew => "RecordBrew",
        }
    }
}

fn watchdog_name(w: PumpWatchdog) -> &'static str {
    match w {
        PumpWatchdog::Brew => "brew",
        PumpWatchdog::HotWater => "hot_water",
    }
}

/// The final actuator state, as [`ActuatorSafe`](crate::scenario::Assertion::ActuatorSafe)
/// checks it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one flag per relay the assertion inspects; a state enum would hide which"
)]
pub struct Actuators {
    /// Whether the pump is running.
    pub pump: bool,
    /// Whether the water valve is open.
    pub water_valve: bool,
    /// Whether the steam valve is open. It is the same physical relay as the
    /// water valve, which
    /// is why both are recorded separately: a scenario that opened the steam
    /// valve outside `STEAM_RUNNING` would be invisible if they were one field.
    pub steam_valve: bool,
    /// The heater duty, 0..=1000. `actuator_safe` requires `0`.
    pub heater_duty: u32,
    /// Whether emergency mode is latched. `actuator_safe` requires it **not**
    /// set — a routine shutdown must not latch.
    pub emergency_latched: bool,
}

/// Everything a run observed.
///
/// Two runs of the same scenario must produce equal observations: the control loop
/// is deterministic, and a test asserts that over every scenario in
/// `scenarios/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    /// The scenario this came from.
    pub scenario: String,
    /// The ordered states entered, deduplicated on consecutive repeats.
    ///
    /// Consecutive repeats are collapsed because the reducer's self-transition
    /// skip makes them a bookkeeping artefact, not
    /// a behaviour. A state that is entered, left, and re-entered still appears
    /// twice, which is the case that matters.
    pub states: Vec<String>,
    /// The effect stream, in order, with the effect's name alongside.
    pub effects: Vec<Observed>,
    /// The actuator state at the end of the run.
    pub actuators: Actuators,
    /// Captured HTTP endpoint bodies, keyed by path. `hardware` only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub endpoints: BTreeMap<String, serde_json::Value>,
    /// Log lines matching a `capture.log_patterns` substring, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log_lines: Vec<String>,
}

/// One observed effect: its name, its reduced form, and when.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Observed {
    /// The effect name, e.g. `EnablePump`. What an assertion matches on.
    pub name: String,
    /// The reduced payload.
    pub effect: ObservedEffect,
    /// Milliseconds since the scenario's first tick.
    pub at_ms: u32,
}

impl Observation {
    /// An empty observation for a scenario.
    #[must_use]
    pub fn new(scenario: &str) -> Self {
        Self {
            scenario: scenario.to_string(),
            states: Vec::new(),
            effects: Vec::new(),
            actuators: Actuators::default(),
            endpoints: BTreeMap::new(),
            log_lines: Vec::new(),
        }
    }

    /// Record a state entry, collapsing consecutive repeats.
    pub fn enter_state(&mut self, state: MachineState) {
        let name = state.name().to_string();
        if self.states.last() == Some(&name) {
            return;
        }
        self.states.push(name);
    }

    /// Record an effect.
    pub fn push_effect(&mut self, name: &str, effect: &Effect, at_ms: u32) {
        self.effects.push(Observed {
            name: name.to_string(),
            effect: ObservedEffect::of(effect),
            at_ms,
        });
    }

    /// The indices at which an effect name occurs, in order.
    #[must_use]
    pub fn occurrences(&self, name: &str) -> Vec<usize> {
        self.effects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.name == name)
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_name_reduction_keeps_the_state() {
        let e = ObservedEffect::of(&Effect::EnterState(MachineState::BrewRunning));
        assert_eq!(
            e,
            ObservedEffect::State {
                state: "BREW_RUNNING".to_string()
            }
        );
    }

    #[test]
    fn the_duty_value_is_dropped() {
        // The duty is a float from a live probe; two runs never agree on it, so
        // recording it would make every diff noise.
        assert_eq!(
            ObservedEffect::of(&Effect::SetHeaterDuty(437.25)),
            ObservedEffect::Duty
        );
        assert_eq!(
            ObservedEffect::of(&Effect::SetHeaterDuty(0.0)),
            ObservedEffect::Duty
        );
    }

    #[test]
    fn the_pump_watchdog_name_is_kept() {
        // Which watchdog fired is the whole point.
        let e = ObservedEffect::of(&Effect::PumpTimeoutFired {
            watchdog: PumpWatchdog::Brew,
        });
        assert_eq!(
            e,
            ObservedEffect::PumpTimeout {
                watchdog: "brew".to_string()
            }
        );
    }

    #[test]
    fn actuator_effects_reduce_to_plain() {
        for effect in [Effect::EnablePump, Effect::CloseWaterValve] {
            assert_eq!(ObservedEffect::of(&effect), ObservedEffect::Plain);
        }
    }

    #[test]
    fn consecutive_repeat_states_collapse() {
        let mut o = Observation::new("t");
        o.enter_state(MachineState::PidNormal);
        o.enter_state(MachineState::PidNormal);
        assert_eq!(o.states, ["PID_NORMAL"]);
    }

    #[test]
    fn a_re_entered_state_is_kept_twice() {
        let mut o = Observation::new("t");
        o.enter_state(MachineState::PidNormal);
        o.enter_state(MachineState::Standby);
        o.enter_state(MachineState::PidNormal);
        assert_eq!(o.states, ["PID_NORMAL", "STANDBY", "PID_NORMAL"]);
    }

    #[test]
    fn occurrences_finds_every_emission_in_order() {
        let mut o = Observation::new("t");
        o.push_effect("EnablePump", &Effect::EnablePump, 0);
        o.push_effect("DisablePump", &Effect::DisablePump, 1);
        o.push_effect("EnablePump", &Effect::EnablePump, 2);
        assert_eq!(o.occurrences("EnablePump"), [0, 2]);
        assert!(o.occurrences("OpenSteamValve").is_empty());
    }

    #[test]
    fn an_observation_round_trips_through_json() {
        // Observations cross the process boundary as JSON, so the round trip is
        // what makes the runner output usable.
        let mut o = Observation::new("brew_by_time");
        o.enter_state(MachineState::BrewPreinfusion);
        o.push_effect("EnablePump", &Effect::EnablePump, 10);
        o.push_effect(
            "EnterState",
            &Effect::EnterState(MachineState::BrewPreinfusion),
            10,
        );
        let text = serde_json::to_string(&o).expect("serialises");
        let back: Observation = serde_json::from_str(&text).expect("deserialises");
        assert_eq!(o, back);
    }
}
