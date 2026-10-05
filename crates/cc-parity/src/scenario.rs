//! The scenario file format: parse it, validate it, and reject it loudly.
//!
//! The format is specified in
//! [`10-scenario-format.md`](../../docs/history/scenario-format.md).
//! This module is the normative implementation of that document; where the two
//! disagree, this module is the one that is wrong and the document is the one
//! that is right.
//!
//! # Why the parser is strict
//!
//! A scenario is a *contract* about firmware behaviour. A typo in one is not a
//! cosmetic problem: `never: { effect: EnablePmp }` would silently assert
//! nothing, and a scenario that asserts nothing is worse than no scenario,
//! because it reads as coverage. So every name in a scenario — state, effect,
//! switch, command — is resolved against the real enumerations at load time and
//! an unknown one is a hard error.
//!
//! The same applies to `config.set`: every dotted key is looked up in
//! [`cc_config::schema::SCHEMA`] and rejected if it is unknown, mistyped, or out
//! of range. The C++ ignores an unknown configuration key
//! ([`cc_config::json`](../../crates/cc-config/src/json.rs)); a scenario file
//! must not, or a renamed parameter would leave a scenario quietly testing
//! nothing.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use cc_domain::state::MachineState;
use serde::Deserialize;

/// The control-loop period a `dry_run` scenario advances by.
///
/// 10 ms, which is the C++'s `Timing::MAIN_LOOP_INTERVAL_MS`
/// (`include/clevercoffee/constants/Timing.h`). A scenario that says "wait five
/// seconds" means five hundred iterations against that, so the number is a
/// parity-relevant constant and not a harness detail.
pub const DEFAULT_TICK_MS: u32 = 10;

/// One scenario file, parsed and validated.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// The scenario's identifier. Also the baseline file's stem.
    pub name: String,
    /// A one-line human summary.
    pub title: String,
    /// Where the scenario runs. See the module docs.
    pub mode: Mode,
    /// The safety paths this scenario exercises, e.g. `["S1", "S4"]`.
    ///
    /// Free-form on purpose: the S-numbering is 01 §6's and a scenario may
    /// legitimately cover a path that is not in it. The runner does not
    /// validate the set, because a closed list here would make adding a safety
    /// path a two-file change.
    #[serde(default)]
    pub covers: Vec<String>,
    /// Why this scenario exists. The C++ line it pins, in prose.
    #[serde(default)]
    pub why: String,
    /// Configuration overrides on top of [`cc_config::Config::default`].
    #[serde(default)]
    pub config: ConfigOverrides,
    /// The script.
    pub stimuli: Vec<Stimulus>,
    /// What to observe.
    #[serde(default)]
    pub capture: Capture,
    /// The pass/fail criteria.
    #[serde(default)]
    pub assert: Vec<Assertion>,
    /// The control-loop period, in milliseconds.
    #[serde(default = "default_tick_ms")]
    pub tick_ms: u32,
    /// How long to run, in milliseconds.
    ///
    /// Optional, and a scenario whose subject takes a known time must set it. A
    /// `brew_by_time` is 3 s of pre-infusion, 2 s of pause and 25 s of flow, and
    /// it is *quiet* — one state, no transitions — for all 25 of them, so a
    /// runner that stopped on quiescence would end it three seconds in.
    ///
    /// Absent, the runner stops when the machine quiesces: every stimulus
    /// delivered and the state unchanged for 3 s. That is right for the
    /// event-shaped scenarios (a trip, an abort, a state change) and wrong for
    /// the timed ones, which is exactly why it is the scenario's choice.
    #[serde(default)]
    pub duration_ms: Option<u32>,
}

fn default_tick_ms() -> u32 {
    DEFAULT_TICK_MS
}

/// Where a scenario runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The reducer and safety monitor, in process on the host. No actuator is
    /// ever energised; the effect stream is asserted instead.
    DryRun,
    /// The flashed firmware on the attached device.
    Hardware,
}

/// Configuration overrides, as the C++'s own dotted keys.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigOverrides {
    /// The keys to set. Values are the YAML scalars; they are coerced to the
    /// type [`cc_config::schema::SCHEMA`] declares for that key.
    ///
    /// [`serde_json::Value`] rather than `serde_yaml_ng::Value` so the value
    /// can be handed to `cc-config`'s own deserialiser without a conversion
    /// step — see [`crate::run::Runner`], which patches the configuration as a
    /// JSON document precisely so that it cannot drift from `cc-config`'s shape.
    #[serde(default)]
    pub set: BTreeMap<String, serde_json::Value>,
}

/// What a scenario observes.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    /// Record the ordered list of entered states.
    #[serde(default = "yes")]
    pub state_transitions: bool,
    /// Record the effect stream. `dry_run` only; the C++ has no effect stream.
    #[serde(default = "yes")]
    pub effects: bool,
    /// HTTP paths to snapshot. `hardware` only.
    #[serde(default)]
    pub endpoints: Vec<String>,
    /// Log lines to collect, as plain substrings.
    #[serde(default)]
    pub log_patterns: Vec<String>,
}

impl Default for Capture {
    fn default() -> Self {
        Self {
            state_transitions: true,
            effects: true,
            endpoints: Vec::new(),
            log_patterns: Vec::new(),
        }
    }
}

fn yes() -> bool {
    true
}

/// One timed input.
///
/// No `deny_unknown_fields` here: serde does not support it on a struct with a
/// `#[serde(flatten)]` field — it would reject `kind` itself. The strictness
/// moves to [`StimulusKind`], which is where a stimulus's fields actually live,
/// and [`Scenario::validate`] rejects a stimulus whose `kind` is not one of the
/// seven.
#[derive(Debug, Clone, Deserialize)]
pub struct Stimulus {
    /// Milliseconds since the scenario's first tick.
    pub at_ms: u32,
    /// What kind of stimulus.
    #[serde(flatten)]
    pub kind: StimulusKind,
}

/// The seven stimulus kinds.
///
/// `deny_unknown_fields` is what makes a typo in a stimulus's *fields* — not its
/// `kind` — a hard error. `at_ms: 0, kind: wait, duration_mss: 500` is rejected
/// rather than read as a 1000 ms wait.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StimulusKind {
    /// Nothing. The loop keeps ticking; this is a deliberate pause.
    Rest,
    /// Tick for a fixed interval.
    Wait {
        /// How long. Defaults to 1000 ms.
        #[serde(default = "default_wait_ms")]
        duration_ms: u32,
    },
    /// A switch edge.
    Button {
        /// `brew`, `steam`, `power`, or `hot_water`.
        switch: String,
        /// `press` or `release`.
        action: ButtonAction,
        /// The hardware long-press flag at the moment of the edge.
        #[serde(default)]
        long_press: bool,
    },
    /// A new sensor sample.
    Sensor {
        /// Degrees Celsius.
        #[serde(default = "default_temperature")]
        temperature_c: f32,
        /// Whether the tank float switch reads full.
        #[serde(default = "yes")]
        water_tank_full: bool,
        /// Whether the probe is faulted.
        #[serde(default)]
        has_temperature_error: bool,
        /// Whether the scale is faulted. Carried for C++ name parity (F13/F14
        /// are dropped at R2-07, so it is always false on the Rust side).
        #[serde(default)]
        has_scale_error: bool,
        /// Brew weight in grams.
        #[serde(default)]
        brew_weight: f32,
    },
    /// A configuration write, as `/api/parameters` would do it.
    Config {
        /// The keys to set, by C++ dotted name.
        set: BTreeMap<String, serde_json::Value>,
    },
    /// A request from outside the switch layer.
    Mqtt {
        /// A [`cc_machine::Command`] variant name, `SCREAMING_SNAKE`.
        command: String,
    },
    /// An OTA session.
    Ota {
        /// `begin` or `end`.
        action: OtaAction,
        /// The image being flashed. Recorded, not fetched.
        #[serde(default)]
        path: String,
    },
}

fn default_wait_ms() -> u32 {
    1000
}

fn default_temperature() -> f32 {
    25.0
}

/// Whether a switch went down or came up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonAction {
    /// Released to pressed.
    Press,
    /// Pressed to released.
    Release,
}

/// Whether an OTA session is starting or ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OtaAction {
    /// The session opens. The heater must be off; see format §7.
    Begin,
    /// The session closes and the machine resumes.
    End,
}

/// One expected observable outcome.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Assertion {
    /// These states were entered, in this relative order.
    VisitedStates {
        /// C++ `SCREAMING_SNAKE` state names.
        value: Vec<String>,
    },
    /// None of these was entered.
    NotVisited {
        /// C++ `SCREAMING_SNAKE` state names.
        value: Vec<String>,
    },
    /// The machine ends in this state.
    FinalState {
        /// A C++ `SCREAMING_SNAKE` state name.
        value: String,
    },
    /// The effect was emitted at least once.
    Always {
        /// An [`cc_machine::Effect`] name.
        effect: String,
    },
    /// The effect was never emitted, optionally within a time window.
    Never {
        /// An [`cc_machine::Effect`] name.
        effect: String,
        /// Only consider emissions at or after this time.
        #[serde(default)]
        after_ms: Option<u32>,
        /// Only consider emissions strictly before this time.
        ///
        /// The other half of `after_ms`, and needed whenever the window has an
        /// end. The OTA scenarios are the case: inside the flash session the
        /// pump must be off, and after it closes the machine is *supposed* to
        /// resume, so a `never` with no upper bound would fail on correct
        /// behaviour six seconds after the thing it is about.
        #[serde(default)]
        before_ms: Option<u32>,
    },
    /// How many times the effect was emitted.
    Count {
        /// An [`cc_machine::Effect`] name.
        effect: String,
        /// Inclusive lower bound.
        min: usize,
        /// Inclusive upper bound.
        #[serde(default)]
        max: Option<usize>,
    },
    /// The first of these occurred before the second.
    Ordering {
        /// An [`cc_machine::Effect`] name.
        before: String,
        /// An [`cc_machine::Effect`] name.
        after: String,
    },
    /// The machine ended in 06 §Definitions' *known-safe state*: heater duty
    /// 0, pump off, both valves closed, emergency latch **not** set.
    ActuatorSafe,
    /// The pump is off and both valves are closed, and the latch is not set.
    ///
    /// **Not** a weaker `actuator_safe`. It is the right assertion for a
    /// scenario that ends in `PID_NORMAL` with the PID enabled, where a
    /// non-zero heater duty is the machine doing its job and
    /// `actuator_safe` would fail for correct behaviour. Water is the thing
    /// that must not be moving: a pump running against an empty tank or a valve
    /// open with no flow behind it is the failure this catches, and a boiler
    /// holding its setpoint is not.
    ///
    /// Use `actuator_safe` where the machine is supposed to be idle or in an
    /// error state, and this where it is supposed to be heating.
    NoFluidFlow,
    /// A captured endpoint contains these keys with these values.
    Http {
        /// The path that was captured.
        path: String,
        /// Key/value pairs that must be present.
        expect: BTreeMap<String, serde_json::Value>,
    },
}

/// A scenario that could not be loaded.
#[derive(Debug)]
pub enum ScenarioError {
    /// The file could not be read.
    Io {
        /// The path.
        path: String,
        /// The underlying message.
        message: String,
    },
    /// The file is not valid YAML, or not the shape this format defines.
    Parse {
        /// The path.
        path: String,
        /// The underlying message.
        message: String,
    },
    /// The file parsed but describes something impossible or unknown.
    Invalid {
        /// The path.
        path: String,
        /// What is wrong.
        message: String,
    },
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => write!(f, "{path}: cannot read: {message}"),
            Self::Parse { path, message } => write!(f, "{path}: cannot parse: {message}"),
            Self::Invalid { path, message } => write!(f, "{path}: {message}"),
        }
    }
}

impl std::error::Error for ScenarioError {}

impl Scenario {
    /// Read, parse, and validate a scenario file.
    ///
    /// # Errors
    ///
    /// Returns [`ScenarioError`] if the file cannot be read, is not valid YAML,
    /// or names something that does not exist. The last of those is the
    /// important one: see the module docs on why the parser is strict.
    pub fn load(path: &Path) -> Result<Self, ScenarioError> {
        let text = std::fs::read_to_string(path).map_err(|e| ScenarioError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        Self::parse(&text, &path.display().to_string())
    }

    /// Parse and validate a scenario from a string.
    ///
    /// `origin` is only used in error messages.
    ///
    /// # Errors
    ///
    /// As [`Self::load`].
    pub fn parse(text: &str, origin: &str) -> Result<Self, ScenarioError> {
        let scenario: Self = serde_yaml_ng::from_str(text).map_err(|e| ScenarioError::Parse {
            path: origin.to_string(),
            message: e.to_string(),
        })?;
        scenario.validate(origin)?;
        Ok(scenario)
    }

    /// Check everything the parser cannot: names, ordering, and mode coherence.
    ///
    /// # Errors
    ///
    /// [`ScenarioError::Invalid`] naming the first problem found.
    pub fn validate(&self, origin: &str) -> Result<(), ScenarioError> {
        let invalid = |m: String| ScenarioError::Invalid {
            path: origin.to_string(),
            message: m,
        };

        if self.tick_ms == 0 {
            return Err(invalid("tick_ms must be greater than zero".into()));
        }
        if self.duration_ms == Some(0) {
            return Err(invalid(
                "duration_ms: 0 would run no control-loop iterations at all".into(),
            ));
        }
        if let Some(end) = self.duration_ms {
            if let Some(last) = self.stimuli.last() {
                if last.at_ms >= end {
                    return Err(invalid(format!(
                        "the last stimulus is at {} ms, at or after duration_ms {end}: the run \
                         would stop before delivering it",
                        last.at_ms
                    )));
                }
            }
        }

        // Stimuli must be in ascending time order. Applying them out of order
        // would make "at_ms" a lie, and the runner would have to sort — which
        // would silently make two stimuli at the same timestamp order-dependent
        // on the file rather than on the text.
        let mut last = None;
        for (i, s) in self.stimuli.iter().enumerate() {
            if let Some(prev) = last {
                if s.at_ms < prev {
                    return Err(invalid(format!(
                        "stimulus {i} is at {} ms, after one at {prev} ms: stimuli must be \
                         in ascending at_ms order",
                        s.at_ms
                    )));
                }
            }
            last = Some(s.at_ms);
        }

        for s in &self.stimuli {
            match &s.kind {
                StimulusKind::Button { switch, .. } => {
                    parse_switch(switch).ok_or_else(|| {
                        invalid(format!(
                            "unknown switch {switch:?}: expected one of brew, steam, power, \
                             hot_water"
                        ))
                    })?;
                }
                StimulusKind::Mqtt { command } => {
                    parse_command(command).ok_or_else(|| {
                        invalid(format!(
                            "unknown command {command:?}: expected a cc_machine::Command \
                             variant name, e.g. BREW_START"
                        ))
                    })?;
                }
                StimulusKind::Config { set } => {
                    for (key, value) in set {
                        validate_config_key(key, value)
                            .map_err(|m| invalid(format!("config.set: {m}")))?;
                    }
                }
                StimulusKind::Ota { action, .. } => {
                    // An OTA session has no reducer event, so a `dry_run` driver
                    // cannot produce one from the firmware. It **models** one:
                    // `SafeHardwareShutdown`, then the control loop suspended
                    // for the session's duration. That is the *fixed* behaviour
                    // 01 §6 asks for, so the scenario asserts the Rust answer
                    // and the C++ baseline records the gap. Allowed here, and
                    // the modelling is stated in the driver and in the format
                    // doc rather than hidden.
                    let _ = action;
                }
                StimulusKind::Rest | StimulusKind::Wait { .. } | StimulusKind::Sensor { .. } => {}
            }
        }

        for a in &self.assert {
            match a {
                Assertion::VisitedStates { value } | Assertion::NotVisited { value } => {
                    for name in value {
                        parse_state(name).ok_or_else(|| {
                            invalid(format!(
                                "unknown state {name:?}: expected a C++ SCREAMING_SNAKE \
                                 MachineStateId enumerator, e.g. BREW_RUNNING"
                            ))
                        })?;
                    }
                }
                Assertion::FinalState { value } => {
                    parse_state(value).ok_or_else(|| {
                        invalid(format!(
                            "unknown state {value:?}: expected a C++ SCREAMING_SNAKE \
                             MachineStateId enumerator"
                        ))
                    })?;
                }
                Assertion::Always { effect } | Assertion::Count { effect, .. } => {
                    if !is_known_effect(effect) {
                        return Err(invalid(format!("unknown effect {effect:?}")));
                    }
                }
                Assertion::Never {
                    effect,
                    after_ms,
                    before_ms,
                } => {
                    if !is_known_effect(effect) {
                        return Err(invalid(format!("unknown effect {effect:?}")));
                    }
                    // An empty window is a vacuous assertion: it can never
                    // fail, so it is a scenario that reads as coverage and
                    // tests nothing.
                    if let (Some(a), Some(b)) = (*after_ms, *before_ms) {
                        if a >= b {
                            return Err(invalid(format!(
                                "never: after_ms {a} is at or after before_ms {b}: the window \
                                 is empty, so the assertion could never fail"
                            )));
                        }
                    }
                }
                Assertion::Ordering { before, after } => {
                    if !is_known_effect(before) {
                        return Err(invalid(format!("unknown effect {before:?}")));
                    }
                    if !is_known_effect(after) {
                        return Err(invalid(format!("unknown effect {after:?}")));
                    }
                }
                Assertion::ActuatorSafe | Assertion::NoFluidFlow | Assertion::Http { .. } => {}
            }
        }

        for (key, value) in &self.config.set {
            validate_config_key(key, value).map_err(|m| invalid(format!("config.set: {m}")))?;
        }

        // A hardware scenario that captures nothing observes nothing, and a
        // dry-run scenario that captures no effects cannot assert on the
        // decision it exists to test.
        if self.mode == Mode::Hardware
            && !self.capture.state_transitions
            && self.capture.endpoints.is_empty()
            && self.capture.log_patterns.is_empty()
        {
            return Err(invalid(
                "a hardware scenario must capture something: state_transitions, endpoints, or \
                 log_patterns"
                    .into(),
            ));
        }
        if self.mode == Mode::DryRun && !self.capture.effects && !self.capture.state_transitions {
            return Err(invalid(
                "a dry_run scenario must capture effects or state_transitions — with neither it \
                 observes no decision"
                    .into(),
            ));
        }
        if self.assert.is_empty() {
            return Err(invalid(
                "a scenario with no assertions is a recording, not a test".into(),
            ));
        }

        Ok(())
    }

    /// The scenario's name, for reporting.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Resolve a switch name to its enumeration.
fn parse_switch(name: &str) -> Option<cc_machine::SwitchId> {
    use cc_machine::SwitchId;
    match name {
        "brew" => Some(SwitchId::Brew),
        "steam" => Some(SwitchId::Steam),
        "power" => Some(SwitchId::Power),
        "hot_water" => Some(SwitchId::HotWater),
        _ => None,
    }
}

/// Resolve a state name to its enumeration.
///
/// The names are the C++'s, because parity is measured against the C++ strings
/// ([`MachineState::name`]).
fn parse_state(name: &str) -> Option<MachineState> {
    cc_domain::state::ALL.into_iter().find(|s| s.name() == name)
}

/// Resolve a command name to its enumeration.
fn parse_command(name: &str) -> Option<cc_machine::Command> {
    use cc_machine::Command::*;
    Some(match name {
        "BREW_START" => BrewStart,
        "BREW_STOP" => BrewStop,
        "STEAM_START" => SteamStart,
        "STEAM_STOP" => SteamStop,
        "MANUAL_FLUSH_START" => ManualFlushStart,
        "MANUAL_FLUSH_STOP" => ManualFlushStop,
        "BACKFLUSH_ENTER" => BackflushEnter,
        "BACKFLUSH_CYCLE_START" => BackflushCycleStart,
        "BACKFLUSH_STOP" => BackflushStop,
        "STANDBY" => Standby,
        "NORMAL_OPERATION" => NormalOperation,
        "SET_USER_PID_ENABLED" => SetUserPidEnabled(true),
        "REBOOT" => Reboot,
        _ => return None,
    })
}

/// Every effect name the format accepts.
///
/// Spelled out rather than derived, because `Effect::name()` is a `&'static str`
/// behind a `match` and there is no way to enumerate the variants from outside
/// the crate. `effect_names_is_complete` in the tests below is what keeps this
/// list honest: a new effect that is not added here makes that test fail.
const KNOWN_EFFECTS: &[&str] = &[
    "EnablePump",
    "DisablePump",
    "OpenWaterValve",
    "CloseWaterValve",
    "OpenSteamValve",
    "CloseSteamValve",
    "EnableHeater",
    "DisableHeater",
    "SetHeaterDuty",
    "EmergencyShutdown",
    "SafeHardwareShutdown",
    "ExitState",
    "EnterState",
    "SetPidRuntime",
    "SetSteamMode",
    "RecordBrew",
    "ResetShotsSinceBackflush",
    "ClearActionRequests",
    "ClearStaleStopRequests",
    "ResetStandbyTimer",
    "ResetMqttReconnectCount",
    "WakeDisplay",
    "RequestReboot",
    "PumpTimeoutFired",
];

fn is_known_effect(name: &str) -> bool {
    KNOWN_EFFECTS.contains(&name)
}

/// Check one `config.set` entry against the registered schema.
///
/// # Errors
///
/// A message naming the problem, for [`ScenarioError::Invalid`].
fn validate_config_key(key: &str, value: &serde_json::Value) -> Result<(), String> {
    use cc_config::schema::{self, ParamKind, ParamValue};

    let Some(spec) = schema::find(key) else {
        // The closest match, so a typo is a one-word fix rather than a hunt.
        let mut best: Option<&str> = None;
        for candidate in schema::SCHEMA {
            if candidate.key.starts_with(key) || key.starts_with(candidate.key) {
                best = Some(candidate.key);
                break;
            }
        }
        return Err(match best {
            Some(s) => format!("unknown parameter {key:?}; did you mean {s:?}?"),
            None => format!(
                "unknown parameter {key:?}: not in the {} registered parameters",
                schema::PARAM_COUNT
            ),
        });
    };

    let as_text = || {
        value
            .as_str()
            .map(ToString::to_string)
            .or_else(|| value.as_f64().map(|f| f.to_string()))
            .unwrap_or_else(|| format!("{value:?}"))
    };

    let parsed: ParamValue<'_> = match spec.kind {
        ParamKind::Bool => {
            let Some(b) = value.as_bool() else {
                return Err(format!("{key}: expected a boolean, got {}", as_text()));
            };
            ParamValue::Bool(b)
        }
        ParamKind::Int => {
            let Some(i) = value.as_i64() else {
                return Err(format!("{key}: expected an integer, got {}", as_text()));
            };
            let Ok(i) = i32::try_from(i) else {
                return Err(format!("{key}: {i} does not fit in the C++'s int32"));
            };
            ParamValue::Int(i)
        }
        ParamKind::Float => {
            let Some(f) = value.as_f64() else {
                return Err(format!("{key}: expected a number, got {}", as_text()));
            };
            ParamValue::Float(f)
        }
        // A text parameter in a scenario is a smell — the four credentials are
        // the only text parameters, and a scenario has no business setting
        // them. Refusing is safer than accepting a password in a git-tracked
        // file, and the error says so.
        ParamKind::Text => {
            return Err(format!(
                "{key}: text parameters are the four credentials and a scenario must not set \
                 them; read the value from the environment instead"
            ));
        }
        ParamKind::Enum => {
            let Some(i) = value.as_i64() else {
                return Err(format!(
                    "{key}: expected the integer discriminant, got {}",
                    as_text()
                ));
            };
            let Ok(i) = i8::try_from(i) else {
                return Err(format!("{key}: {i} is not an enumeration discriminant"));
            };
            ParamValue::Enum(i)
        }
    };

    if !spec.accepts(parsed) {
        return Err(format!(
            "{key}: {} is outside the registered range",
            as_text()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
name: t
title: T
mode: dry_run
stimuli:
  - { at_ms: 0, kind: rest }
assert:
  - { kind: final_state, value: PID_NORMAL }
"#;

    #[test]
    fn a_minimal_scenario_parses() {
        let s = Scenario::parse(MINIMAL, "test").expect("parses");
        assert_eq!(s.name(), "t");
        assert_eq!(s.mode, Mode::DryRun);
        assert_eq!(s.tick_ms, DEFAULT_TICK_MS);
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        // A typo in a stimulus must not become a stimulus that does nothing.
        let text = MINIMAL.replace("kind: rest", "kind: restt");
        let err = Scenario::parse(&text, "test").expect_err("must reject");
        assert!(matches!(err, ScenarioError::Parse { .. }), "{err}");
    }

    #[test]
    fn an_unknown_state_is_rejected() {
        let text = MINIMAL.replace("PID_NORMAL", "PIDNORMAL");
        let err = Scenario::parse(&text, "test").expect_err("must reject");
        assert!(format!("{err}").contains("unknown state"), "{err}");
    }

    #[test]
    fn an_unknown_effect_is_rejected() {
        let text = format!("{MINIMAL}\n");
        let text = text.replace("assert:", "assert:\n  - { kind: never, effect: EnablePmp }");
        let err = Scenario::parse(&text, "test").expect_err("must reject");
        assert!(format!("{err}").contains("unknown effect"), "{err}");
    }

    #[test]
    fn an_unknown_switch_is_rejected() {
        let text = MINIMAL.replace(
            "- { at_ms: 0, kind: rest }",
            "- { at_ms: 0, kind: button, switch: brews, action: press }",
        );
        let err = Scenario::parse(&text, "test").expect_err("must reject");
        assert!(format!("{err}").contains("unknown switch"), "{err}");
    }

    #[test]
    fn an_unknown_command_is_rejected() {
        let text = MINIMAL.replace(
            "- { at_ms: 0, kind: rest }",
            "- { at_ms: 0, kind: mqtt, command: BREW_STRT }",
        );
        let err = Scenario::parse(&text, "test").expect_err("must reject");
        assert!(format!("{err}").contains("unknown command"), "{err}");
    }

    #[test]
    fn an_unknown_config_key_is_rejected_with_a_suggestion() {
        let text = MINIMAL.replace(
            "stimuli:",
            "config:\n  set:\n    brew.pre_infusion.tim: 3.0\nstimuli:",
        );
        let err = Scenario::parse(&text, "test").expect_err("must reject");
        let msg = format!("{err}");
        assert!(msg.contains("unknown parameter"), "{msg}");
        assert!(msg.contains("brew.pre_infusion.time"), "{msg}");
    }

    #[test]
    fn an_out_of_range_config_value_is_rejected() {
        let text = MINIMAL.replace(
            "stimuli:",
            "config:\n  set:\n    safety.emergency_temp: 900.0\nstimuli:",
        );
        let err = Scenario::parse(&text, "test").expect_err("must reject");
        assert!(
            format!("{err}").contains("outside the registered range"),
            "{err}"
        );
    }

    #[test]
    fn a_credential_is_refused() {
        let text = MINIMAL.replace(
            "stimuli:",
            "config:\n  set:\n    system.wifi.password: hunter2\nstimuli:",
        );
        let err = Scenario::parse(&text, "test").expect_err("must refuse");
        let msg = format!("{err}");
        assert!(msg.contains("must not set"), "{msg}");
        // And the value must not appear in the message.
        assert!(!msg.contains("hunter2"), "{msg}");
    }

    #[test]
    fn every_credential_key_is_refused() {
        // All four of them, not just one. A scenario file is a git-tracked
        // artifact and a password in one is a leak that survives every later
        // "we'll clean that up".
        for key in [
            "system.wifi.ssid",
            "system.wifi.password",
            "system.auth.password",
            "system.ota_password",
        ] {
            let text = MINIMAL.replace(
                "stimuli:",
                &format!("config:\n  set:\n    {key}: x\nstimuli:"),
            );
            let err = Scenario::parse(&text, "test").expect_err("must refuse");
            assert!(format!("{err}").contains("must not set"), "{key}: {err}");
        }
    }

    #[test]
    fn out_of_order_stimuli_are_rejected() {
        let text = MINIMAL.replace(
            "- { at_ms: 0, kind: rest }",
            "- { at_ms: 100, kind: rest }\n  - { at_ms: 50, kind: rest }",
        );
        let err = Scenario::parse(&text, "test").expect_err("must reject");
        assert!(format!("{err}").contains("ascending"), "{err}");
    }

    #[test]
    fn a_scenario_with_no_assertions_is_rejected() {
        let text = "name: t\ntitle: T\nmode: dry_run\nstimuli:\n  - { at_ms: 0, kind: rest }\n";
        let err = Scenario::parse(text, "test").expect_err("must reject");
        assert!(format!("{err}").contains("not a test"), "{err}");
    }

    #[test]
    fn a_dry_run_ota_begin_is_accepted_because_the_driver_models_it() {
        // The OTA session has no reducer event. The `dry_run` driver models the
        // fixed behaviour, so a scenario may open one and assert the Rust answer
        // against a C++ baseline that records the gap.
        let text = MINIMAL.replace(
            "- { at_ms: 0, kind: rest }",
            "- { at_ms: 0, kind: ota, action: begin, path: firmware.bin }",
        );
        let s = Scenario::parse(&text, "test").expect("must accept");
        assert!(matches!(s.stimuli[0].kind, StimulusKind::Ota { .. }));
    }

    #[test]
    fn an_ota_stimulus_with_a_typo_in_a_field_is_rejected() {
        let text = MINIMAL.replace(
            "- { at_ms: 0, kind: rest }",
            "- { at_ms: 0, kind: ota, action: begun, path: firmware.bin }",
        );
        let err = Scenario::parse(&text, "test").expect_err("must reject");
        assert!(matches!(err, ScenarioError::Parse { .. }), "{err}");
    }

    #[test]
    fn a_hardware_scenario_that_captures_nothing_is_rejected() {
        let text = r#"
name: t
title: T
mode: hardware
capture:
  state_transitions: false
stimuli:
  - { at_ms: 0, kind: rest }
assert:
  - { kind: actuator_safe }
"#;
        let err = Scenario::parse(text, "test").expect_err("must reject");
        assert!(format!("{err}").contains("must capture something"), "{err}");
    }

    #[test]
    fn every_effect_name_is_accepted() {
        for name in KNOWN_EFFECTS {
            assert!(
                is_known_effect(name),
                "{name} is in the list but not accepted"
            );
        }
    }

    #[test]
    fn the_known_effect_list_has_no_duplicates() {
        let mut seen = std::collections::BTreeSet::new();
        for name in KNOWN_EFFECTS {
            assert!(seen.insert(*name), "{name} appears twice");
        }
    }

    #[test]
    fn every_cpp_state_name_is_accepted() {
        // The scenario files spell states the C++'s way, so all eighteen must
        // resolve. A new state that is not added here fails this.
        for state in cc_domain::state::ALL {
            let name = state.name();
            assert_eq!(parse_state(name), Some(state), "{name} did not resolve");
        }
        assert_eq!(parse_state("NOT_A_STATE"), None);
    }
}
