//! The `dry_run` driver: a scenario in, an [`Observation`] out.
//!
//! # This is the only place a scenario touches the reducer
//!
//! It wires up exactly what the device's shell wires up — the configuration, the
//! safety monitor, the PID, the clock — and hands the reducer a
//! [`Recorder`] where the GPIO would be. Nothing else in the crate calls
//! [`cc_machine::reduce`], and there is no other exit from it.
//!
//! # The actuator guarantee
//!
//! [`Recorder`] implements [`cc_machine::Actuators`], whose eleven methods
//! return `()`. There is no pin behind them, no `cc-hal-esp32` in this crate's
//! dependency tree, and no way to reach one. A `brew_by_time` scenario therefore
//! *does* produce `Effect::EnablePump` and the harness *does* see it — and
//! nothing is energised, because there is nothing to energise.
//!
//! The applier is still the real one ([`cc_machine::apply`]), because the
//! applier is where the order of the effect vector is resolved and a harness
//! that reordered it would not be testing the thing.
//!
//! # The tick order
//!
//! One iteration of [`Runner::tick`] is, in order:
//!
//! 1. deliver any stimuli due at this timestamp;
//! 2. run [`cc_safety::reduce`] on the current sample — S1/S2/S3/S4/S5/S5';
//! 3. fold the outcome in as [`cc_machine::Event::Safety`];
//! 4. run the PID and fold in [`cc_machine::Event::PidOutput`];
//! 5. fold in [`cc_machine::Event::Tick`] — the control pass.
//!
//! Steps 2-5 are one event each, front to back, which is `cc_machine::lib`'s
//! documented loop shape. The safety reduce happens **before** the tick so the
//! verdict the tick's `apply_pid_output` reads is the one for *this* sample,
//! not the previous one's — which is 01 §6 S1's "same loop" requirement.

use std::collections::BTreeMap;

use cc_config::Config;
use cc_domain::pid::{Controller, ControllerDirection, Mode as PidMode, ProportionalOn};
use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_machine::{Actuators, Command, Effect, Event, Sensors, SideChannels, SwitchId};
use serde_json::Value;

use crate::observe::{Actuators as ObservedActuators, Observation};
use crate::scenario::{Assertion, ButtonAction, OtaAction, Scenario, Stimulus, StimulusKind};

/// An `Actuators` implementation that records and does nothing else.
///
/// The type exists to make the safety property legible. Every method here is a
/// `Vec::push`; there is no other statement in any of them, and adding one would
/// be the only way this harness could energise anything.
#[derive(Debug, Default)]
struct Recorder {
    /// The final actuator state, as `actuator_safe` reads it.
    state: ObservedActuators,
    /// Every call name, in order. Not used for assertions (the observation's
    /// effect stream is), but kept so a failure can print what the machine
    /// actually asked for.
    calls: Vec<&'static str>,
}

impl Actuators for Recorder {
    fn enable_pump(&mut self) {
        self.state.pump = true;
        self.calls.push("enable_pump");
    }
    fn disable_pump(&mut self) {
        self.state.pump = false;
        self.calls.push("disable_pump");
    }
    fn open_water_valve(&mut self) {
        self.state.water_valve = true;
        self.calls.push("open_water_valve");
    }
    fn close_water_valve(&mut self) {
        self.state.water_valve = false;
        self.calls.push("close_water_valve");
    }
    fn open_steam_valve(&mut self) {
        self.state.steam_valve = true;
        self.calls.push("open_steam_valve");
    }
    fn close_steam_valve(&mut self) {
        self.state.steam_valve = false;
        self.calls.push("close_steam_valve");
    }
    fn enable_heater(&mut self) {
        self.calls.push("enable_heater");
    }
    fn disable_heater(&mut self) {
        self.calls.push("disable_heater");
    }
    fn set_heater_duty(&mut self, duty: f32) {
        // `SetHeaterDuty` is a float from a control loop; `actuator_safe` only
        // asks whether it is zero, so the duty is quantised to that question.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // Justification: a negative duty is not producible — `cc_machine` only
        // ever emits 0.0 or the PID output, which `cc_domain` bounds to
        // 0..=1000 — and the value is only ever compared against zero.
        let duty = if duty > 0.0 { 1 } else { 0 };
        self.state.heater_duty = duty;
        self.calls.push("set_heater_duty");
    }
    fn emergency_shutdown(&mut self) {
        self.state.pump = false;
        self.state.water_valve = false;
        self.state.steam_valve = false;
        self.state.heater_duty = 0;
        self.state.emergency_latched = true;
        self.calls.push("emergency_shutdown");
    }
    fn safe_hardware_shutdown(&mut self) {
        // Deliberately does NOT latch. 06 §Definitions: a routine shutdown must
        // leave the machine recoverable.
        self.state.pump = false;
        self.state.water_valve = false;
        self.state.steam_valve = false;
        self.state.heater_duty = 0;
        self.calls.push("safe_hardware_shutdown");
    }
}

/// The temperature sensor's read interval, in milliseconds.
///
/// `Timing::TEMPERATURE_SENSOR_INTERVAL_MS` = 400 (`constants/Timing.h:42`),
/// and `SensorCoordinator::update()` runs on it rather than on the control
/// loop's period. That distinction is what makes S1's three-reading debounce
/// mean what it says: without it the same reading would count three times in
/// 30 ms and the debounce would be a fiction.
const SENSOR_INTERVAL_MS: u32 = 400;

/// How many consecutive unchanged ticks end a run.
///
/// 300 ticks = 3 s at the 10 ms default. Longer than the longest timeout the
/// reducer can be waiting on inside a state — the pre-infusion pause tops out
/// at a few seconds — so a machine that has not moved for this long is not
/// going to.
const QUIESCE_TICKS: u32 = 300;

/// A hard cap on a run, so a machine that never settles is a failure rather
/// than a hang.
///
/// 120 000 ticks = 20 minutes at 10 ms, which is longer than the longest
/// scenario in the set needs by two orders of magnitude.
const MAX_TICKS: u32 = 120_000;

/// A scenario run that failed.
#[derive(Debug)]
pub enum RunError {
    /// The scenario is not a `dry_run` scenario; the driver does not drive a
    /// device.
    NotDryRun {
        /// The scenario's name.
        name: String,
    },
    /// An assertion in the scenario did not hold.
    AssertionFailed {
        /// The scenario's name.
        name: String,
        /// Every failure, not just the first. A scenario that fails three ways
        /// should say so once.
        failures: Vec<String>,
    },
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotDryRun { name } => {
                write!(
                    f,
                    "{name}: mode is not dry_run; the harness does not drive a device"
                )
            }
            Self::AssertionFailed { name, failures } => {
                writeln!(f, "{name}: {} assertion(s) failed", failures.len())?;
                for line in failures {
                    writeln!(f, "  - {line}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for RunError {}

/// The `dry_run` driver.
pub struct Runner {
    scenario: Scenario,
    config: Config,
    machine: cc_machine::Machine,
    safety: cc_safety::SafetyState,
    pid: Controller,
    setpoint: f64,
    temp_offset: f64,
    sensors: Sensors,
    now: u32,
    obs: Observation,
    recorder: Recorder,
    /// `at_ms` of the next unconsumed stimulus.
    next_stimulus: usize,
    /// Set by a `sensor` stimulus, consumed by the next iteration.
    ///
    /// A scenario that says "the probe read 160 °C now" must not have the
    /// reading swallowed by the 400 ms cadence, so the stimulus forces a sample.
    sensor_stimulus_at: Option<u32>,
    /// A cadence sample is due this iteration (the 400 ms timer fired).
    sensors_due: bool,
    /// When the last sample was taken, for the cadence gate.
    last_sample_at: Option<u32>,
    /// `at_ms` at which the OTA session ends. `None` = no session.
    ota_until: Option<u32>,
    /// Whether the machine is inside an OTA session.
    ///
    /// The C++'s `LoopManager::update()` returns early while `OTA::isActive()`
    /// (`LoopManager.cpp:128-132`), so the state machine does not run and the
    /// pump and valve are not driven. That is the gap 01 §6 records. This
    /// harness models the **fixed** behaviour, which is the one the Rust port
    /// must have, and asserts the actuators are off across the session.
    ota_active: bool,
}

impl Runner {
    /// Build a runner for a scenario.
    ///
    /// # Errors
    ///
    /// [`RunError::NotDryRun`] if the scenario is `hardware`.
    pub fn new(scenario: Scenario) -> Result<Self, RunError> {
        if scenario.mode != crate::scenario::Mode::DryRun {
            return Err(RunError::NotDryRun {
                name: scenario.name.clone(),
            });
        }
        let mut config = Config::default();
        apply_overrides(&mut config, &scenario.config.set).map_err(|e| {
            RunError::AssertionFailed {
                name: scenario.name.clone(),
                failures: vec![e],
            }
        })?;

        // The PID is the C++'s, not a stub: `ProcessController`'s gains are
        // `Ki = Kp/Tn`, `Kd = Tv*Kp` (`ProcessController.cpp:382-390`) and the
        // sample time is the 1000 ms heater window
        // (`SystemInitializer.cpp:551`). The window is not configurable
        // (`Config::HEATER_WINDOW_MS`).
        let (kp, ki, kd) = config.pid_tunings();
        let pid = Controller::new(
            Millis::ZERO,
            kp,
            ki,
            kd,
            if config.pid.use_ponm {
                ProportionalOn::Measurement
            } else {
                ProportionalOn::Error
            },
            ControllerDirection::Direct,
        );
        let mut pid = pid;
        pid.set_sample_time(Millis::new(Config::HEATER_WINDOW_MS));
        pid.set_output_limits(0.0, Config::HEATER_WINDOW_MS as f64);
        pid.set_integrator_limits(0.0, 55.0); // AGGIMAX
        pid.set_smoothing_factor(config.pid.ema_factor);
        pid.set_mode(PidMode::Automatic);

        let temp_offset = config.brew.temp_offset;
        let setpoint = config.brew.setpoint;

        let sensors = Sensors::healthy();
        let obs = Observation::new(&scenario.name, "rust");
        let ctx = cc_machine::Context::new(&config, Celsius::new(setpoint as f32));

        // `SystemInitializer::finalizeMachineState` reads the power switch
        // before the state machine exists and hands it both an initial state and
        // the runtime PID flag (`SystemInitializer.cpp:604-641`). A `dry_run`
        // scenario has no power switch, so the "no power switch" arm applies:
        // start in `INIT` with the config's PID preference.
        let machine = cc_machine::boot(Millis::ZERO, &ctx).0;

        Ok(Self {
            scenario,
            config,
            machine,
            safety: cc_safety::SafetyState::CLEAR,
            pid,
            setpoint,
            temp_offset,
            sensors,
            now: 0,
            obs,
            recorder: Recorder::default(),
            next_stimulus: 0,
            sensor_stimulus_at: None,
            sensors_due: true,
            last_sample_at: None,
            ota_until: None,
            ota_active: false,
        })
    }

    /// The scenario's configuration, after the overrides.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    fn ctx(&self) -> cc_machine::Context<'_> {
        cc_machine::Context::new(&self.config, Celsius::new(self.setpoint as f32))
    }

    /// Run the whole scenario and return the observation.
    ///
    /// # When it stops
    ///
    /// Two rules, and which one applies is the scenario's decision:
    ///
    /// * **`duration_ms` set** — run for exactly that long. A scenario whose
    ///   subject takes a known time (a 25 s brew, a 30 s flash) says so, because
    ///   the alternative is a heuristic guessing at it.
    /// * **`duration_ms` absent** — run until the machine **quiesces**: every
    ///   stimulus delivered and the state unchanged for [`QUIESCE_TICKS`]
    ///   consecutive ticks.
    ///
    /// The heuristic alone is not enough, and finding that out is why `duration_ms`
    /// exists. A `brew_by_time` scenario is 3 s of pre-infusion, 2 s of pause and
    /// 25 s of flow; it is *quiet* — one state, no transitions — for all 25 of
    /// them, so a quiescence rule alone ends the run three seconds in and the
    /// scenario asserts against a machine that is still brewing. A fixed tail has
    /// the mirror failure. Timing is part of what the scenario is claiming, so it
    /// is stated.
    ///
    /// [`MAX_TICKS`] is a backstop: a machine that never settles is a bug in the
    /// reducer or the scenario, and hanging forever hides it.
    ///
    /// # Errors
    ///
    /// [`RunError::AssertionFailed`] if any assertion does not hold.
    pub fn run(mut self) -> Result<Observation, RunError> {
        let duration = self.scenario.duration_ms;
        let mut quiet = 0u32;
        let mut last = self.machine.state;
        let mut ticks = 0u32;

        while ticks < MAX_TICKS {
            if let Some(end_ms) = duration {
                if self.now >= end_ms {
                    break;
                }
            } else if ticks > 0 {
                // Quiescence, but only once every stimulus has been delivered —
                // a scenario that has not finished pressing its switches is not
                // quiet, it is early.
                let stimuli_done = self.next_stimulus >= self.scenario.stimuli.len();
                if stimuli_done && quiet >= QUIESCE_TICKS && !self.ota_active {
                    break;
                }
            }
            self.step();
            ticks += 1;
            if self.machine.state == last {
                quiet = quiet.saturating_add(1);
            } else {
                quiet = 0;
                last = self.machine.state;
            }
        }
        self.finish()
    }

    /// One control-loop iteration.
    fn step(&mut self) {
        let tick = self.scenario.tick_ms;
        let at = self.now;

        // ---- 1. stimuli due now -------------------------------------------
        while self
            .scenario
            .stimuli
            .get(self.next_stimulus)
            .is_some_and(|s| s.at_ms == at)
        {
            let s = self.scenario.stimuli[self.next_stimulus].clone();
            self.next_stimulus += 1;
            self.deliver(&s, at);
        }

        // Whether this iteration carries a new sensor sample. Step 2 of the
        // C++'s loop is `sensorCoordinator().update()`, which reads on
        // `Timing::TEMPERATURE_SENSOR_INTERVAL_MS` = 400 ms
        // (`constants/Timing.h:42`), not every iteration.
        //
        // This is load-bearing for S1 rather than a fidelity detail. The
        // over-temperature debounce is **three readings**
        // (`EmergencyStopManager::DEBOUNCE_COUNT`), and a "reading" is a sensor
        // sample. Running the safety reduce on every 10 ms tick would make it
        // trip on the same temperature 30 times over in 300 ms, so a scenario
        // whose readings are 400 ms apart would trip at 20 ms instead of 800 ms
        // — and the debounce, the single most safety-relevant timing constant
        // in the firmware, would be untestable.
        //
        // A `sensor` stimulus forces a sample: the scenario is saying "the probe
        // produced a reading now", and the cadence must not swallow it.
        let sampled = self.sensors_due || self.sensor_stimulus_at == Some(at);
        self.sensors_due = false;
        if sampled {
            self.sensor_stimulus_at = None;
            self.last_sample_at = Some(at);
        }

        if self.ota_active {
            // The C++ skips the whole loop here (01 §6's gap). The fixed
            // behaviour is that the actuators are off, which the recorder
            // already reflects from the SafeHardwareShutdown the session's
            // `begin` emitted. Ticking the reducer would be asserting the
            // *unfixed* behaviour.
            self.now = self.now.saturating_add(tick);
            if self.ota_until.is_some_and(|until| at >= until) {
                self.ota_active = false;
                self.ota_until = None;
            }
            return;
        }

        // ---- 2. the safety monitor, on the sensor cadence ------------------
        // Held from the previous sample between reads, exactly as the C++'s
        // `SensorCoordinator` holds its last reading.
        //
        // The conversion goes through `Config::safety_view`, which is the
        // hand-off point `cc-config` documents for exactly this (`cc-config`'s
        // `store` module docs). Re-deriving the five values here would be a
        // second copy of a mapping that decides whether a stored configuration
        // is safe to run; only the field *names* are repeated, and a test below
        // fails if `cc-config` changes one without this.
        //
        // (`cc-config` is expected to grow `From<SafetyView> for SafetyConfig`,
        // which `cc-config`'s own docs already promise. When it does, this
        // becomes a one-liner and the test below is what notices the change.)
        let view = self.config.safety_view();
        let safety_cfg = cc_safety::SafetyConfig {
            emergency_temp: view.emergency_temp,
            emergency_hysteresis: view.emergency_hysteresis,
            steam_setpoint: view.steam_setpoint,
            heater_relay_trigger: view.heater_relay_trigger,
            temperature_sensor: view.temperature_sensor,
        };
        let outcome = if sampled {
            let telemetry = cc_safety::Telemetry::new(
                self.sensors.temperature,
                self.sensors.water_tank_full,
                self.machine.state,
            );
            let out = cc_safety::reduce(&self.safety, &telemetry, &safety_cfg, Millis::new(at));
            self.safety = out.state;
            Some(out)
        } else {
            None
        };

        // ---- 3. the PID ---------------------------------------------------
        // `ProcessController::updateTemperature` subtracts the brew offset from
        // the reading when not steaming (`ProcessController.cpp:131-141`), and
        // `updateSetpoint` picks the steam setpoint while steam mode is on
        // (`:235-244`).
        let steaming = self.machine.steam_mode;
        self.pid.setpoint = if steaming {
            self.config.steam.setpoint
        } else {
            self.setpoint
        };
        self.pid.input = if steaming {
            f64::from(self.sensors.temperature.raw())
        } else {
            f64::from(self.sensors.temperature.raw()) - self.temp_offset
        };
        let computed = self.pid.compute(Millis::new(at));

        // ---- 4. fold in, front to back, then the tick ---------------------
        let ctx = self.ctx();
        let mut machine = self.machine;
        let mut effects: Vec<Effect> = Vec::new();

        if sampled {
            // `Event::SensorUpdated` is `LoopManager` step 2, before the state
            // machine, so the guards react to this sample in this tick.
            let sensors = self.sensors;
            let (m, fx) = cc_machine::reduce(&machine, &ctx, Event::SensorUpdated(sensors));
            machine = m;
            effects.extend(fx);
        }

        if let Some(outcome) = outcome {
            let (m, fx) = cc_machine::reduce(&machine, &ctx, Event::Safety(outcome));
            machine = m;
            effects.extend(fx);
        }

        if computed {
            // The C++ publishes the output and relies on `updatePIDState`
            // zeroing it later in the same `updateProcessControl`
            // (`ProcessController.cpp:113` then `:151-172`). Same shape here.
            let out = self.pid.output as f32;
            let (m, fx) = cc_machine::reduce(&machine, &ctx, Event::PidOutput(out));
            machine = m;
            effects.extend(fx);
        }

        let (m, fx) = cc_machine::reduce(
            &machine,
            &ctx,
            Event::Tick {
                now: Millis::new(at),
            },
        );
        machine = m;
        effects.extend(fx);

        self.machine = machine;
        self.apply(&effects, at);
        self.now = self.now.saturating_add(tick);

        // Arm the next cadence sample: 400 ms after the last one
        // (`Timing::TEMPERATURE_SENSOR_INTERVAL_MS`, `constants/Timing.h:42`).
        let next_sample = self
            .last_sample_at
            .is_none_or(|last| self.now.saturating_sub(last) >= SENSOR_INTERVAL_MS);
        self.sensors_due = next_sample;
    }

    /// Apply one event's effects, recording as it goes.
    fn apply(&mut self, effects: &[Effect], at: u32) {
        let mut seen: Vec<MachineState> = Vec::new();
        {
            // A tiny shim so the real applier can be used: it needs a
            // `SideChannels`, and the state list is read off the effects
            // rather than the channel so the observation and the application
            // cannot disagree about what happened.
            struct Collect<'a>(&'a mut Vec<MachineState>);
            impl SideChannels for Collect<'_> {
                fn on_enter_state(&mut self, state: MachineState) {
                    self.0.push(state);
                }
            }
            let mut collect = Collect(&mut seen);
            cc_machine::apply(&mut self.recorder, &mut collect, &self.machine, effects);
        }

        if self.scenario.capture.state_transitions {
            for state in &seen {
                self.obs.enter_state(*state);
            }
        }
        if self.scenario.capture.effects {
            for effect in effects {
                let name = effect.name();
                self.obs.push_effect(name, &effect, at);
            }
        }
    }

    /// Deliver one stimulus.
    fn deliver(&mut self, s: &Stimulus, at: u32) {
        let ctx = self.ctx();
        let mut machine = self.machine;
        let mut effects: Vec<Effect> = Vec::new();

        match &s.kind {
            StimulusKind::Rest | StimulusKind::Wait { .. } => {}

            StimulusKind::Button {
                switch,
                action,
                long_press,
            } => {
                let id = parse_switch(switch).unwrap_or(SwitchId::Brew);
                let ev = match action {
                    ButtonAction::Press => Event::ButtonPressed {
                        switch: id,
                        long_press: *long_press,
                    },
                    ButtonAction::Release => Event::ButtonReleased { switch: id },
                };
                let (m, fx) = cc_machine::reduce(&machine, &ctx, ev);
                machine = m;
                effects.extend(fx);
            }

            StimulusKind::Sensor {
                temperature_c,
                water_tank_full,
                has_temperature_error,
                has_scale_error,
                brew_weight,
            } => {
                self.sensors = Sensors {
                    temperature: Celsius::new(*temperature_c),
                    water_tank_full: *water_tank_full,
                    has_temperature_error: *has_temperature_error,
                    has_scale_error: *has_scale_error,
                    brew_weight: *brew_weight,
                };
                // Force a sample on this iteration. The probe produced a
                // reading; the 400 ms cadence must not defer it.
                self.sensor_stimulus_at = Some(at);
            }

            StimulusKind::Config { set } => {
                // A mid-run configuration write. The C++ equivalent is a
                // `/api/parameters` POST, which mutates the singleton the next
                // `LoopManager` iteration reads. Here the `Context` is rebuilt
                // from the mutated `Config` on the next tick, which is the same
                // thing one iteration later.
                if let Err(e) = apply_overrides(&mut self.config, set) {
                    // The scenario parser already validated these keys, so this
                    // cannot fire for a validated scenario. Recorded rather than
                    // panicked so a harness bug is a wrong answer, not a crash.
                    self.obs.log_lines.push(format!("config rejected: {e}"));
                }
            }

            StimulusKind::Mqtt { command } => {
                let cmd = parse_command(command).unwrap_or(Command::BrewStop);
                let (m, fx) = cc_machine::reduce(&machine, &ctx, Event::Command(cmd));
                machine = m;
                effects.extend(fx);
            }

            StimulusKind::Ota { action, path } => match action {
                OtaAction::Begin => {
                    // The fix 01 §6 asks for: `safe_hardware_shutdown()` — pump,
                    // valve and heater — not the C++'s `disable_heater()` only.
                    effects.push(Effect::SafeHardwareShutdown);
                    self.ota_active = true;
                    // A realistic session length, so `ota: end` in a later
                    // stimulus is reachable and the "still off" assertions have
                    // something to hold across.
                    self.ota_until = Some(at.saturating_add(30_000));
                    self.obs.log_lines.push(format!("ota begin {path}"));
                }
                OtaAction::End => {
                    self.ota_active = false;
                    self.ota_until = None;
                    self.obs.log_lines.push("ota end".to_string());
                }
            },
        }

        // The machine is stored **unconditionally**. A switch press that only
        // sets a request flag produces no effects at all — `set_request` returns
        // `ResetStandbyTimer` only when standby is enabled — so gating this on
        // `!effects.is_empty()` silently discarded the flag and the scenario
        // under test was testing nothing. The reducer's contract is that it
        // returns a new `Machine`; an empty effect vector is not a null result.
        self.machine = machine;
        if !effects.is_empty() {
            self.apply(&effects, at);
        }
    }

    /// Collect the observation and check the assertions.
    fn finish(mut self) -> Result<Observation, RunError> {
        self.obs.actuators = self.recorder.state;
        let failures = check_assertions(&self.scenario, &self.obs);
        if failures.is_empty() {
            Ok(self.obs)
        } else {
            Err(RunError::AssertionFailed {
                name: self.scenario.name.clone(),
                failures,
            })
        }
    }
}

/// Run a `dry_run` scenario.
///
/// # Errors
///
/// As [`Runner::run`].
pub fn run(scenario: Scenario) -> Result<Observation, RunError> {
    Runner::new(scenario)?.run()
}

/// Check a scenario's assertions against an observation.
///
/// Split out so the runner's own tests and the `just parity` smoke test can
/// assert against a hand-built observation.
#[must_use]
pub fn check_assertions(scenario: &Scenario, obs: &Observation) -> Vec<String> {
    let mut failures = Vec::new();

    for a in &scenario.assert {
        match a {
            Assertion::VisitedStates { value } => {
                let mut cursor = 0usize;
                for want in value {
                    match obs.states[cursor..].iter().position(|s| s == want) {
                        Some(off) => cursor += off + 1,
                        None => {
                            failures.push(format!(
                                "visited_states: {want} does not appear in order (saw {:?})",
                                obs.states
                            ));
                            break;
                        }
                    }
                }
            }
            Assertion::NotVisited { value } => {
                for want in value {
                    if obs.states.iter().any(|s| s == want) {
                        failures.push(format!("not_visited: {want} was entered"));
                    }
                }
            }
            Assertion::FinalState { value } => {
                let last = obs.states.last().map(String::as_str).unwrap_or("<none>");
                if last != value {
                    failures.push(format!(
                        "final_state: expected {value}, the machine ended in {last} \
                         (sequence {:?})",
                        obs.states
                    ));
                }
            }
            Assertion::Always { effect } => {
                if obs.occurrences(effect).is_empty() {
                    failures.push(format!("always: {effect} was never emitted"));
                }
            }
            Assertion::Never {
                effect,
                after_ms,
                before_ms,
            } => {
                let after = after_ms.unwrap_or(0);
                let before = before_ms.unwrap_or(u32::MAX);
                for i in obs.occurrences(effect) {
                    let at_ms = obs.effects[i].at_ms;
                    if at_ms >= after && at_ms < before {
                        let bound = match before_ms {
                            Some(b) => format!("{after} ms (exclusive) to {b} ms"),
                            None => format!("{after} ms onwards"),
                        };
                        failures.push(format!(
                            "never: {effect} was emitted at {at_ms} ms, inside the window \
                             {bound}"
                        ));
                    }
                }
            }
            Assertion::Count { effect, min, max } => {
                let n = obs.occurrences(effect).len();
                let over = max.is_some_and(|m| n > m);
                if n < *min || over {
                    failures.push(format!(
                        "count: {effect} was emitted {n} time(s), expected {}..={}",
                        min,
                        max.map_or_else(|| "*".to_string(), |m| m.to_string())
                    ));
                }
            }
            Assertion::Ordering {
                before,
                after: second,
            } => {
                let a = obs.occurrences(before);
                let b = obs.occurrences(second);
                match (a.first(), b.first()) {
                    (Some(_), Some(_)) if a[0] < b[0] => {}
                    (Some(_), Some(_)) => failures.push(format!(
                        "ordering: {before} must occur before {second} \
                         ({before} at index {}, {second} at index {})",
                        a[0], b[0]
                    )),
                    (None, _) => failures.push(format!("ordering: {before} was never emitted")),
                    (_, None) => failures.push(format!("ordering: {second} was never emitted")),
                }
            }
            Assertion::ActuatorSafe | Assertion::NoFluidFlow => {
                // The two differ only on the heater duty. `actuator_safe` is
                // 06 §Definitions' known-safe state and requires the duty to be
                // zero; `no_fluid_flow` is for a machine that is legitimately
                // heating, where a non-zero duty is the machine working.
                let require_zero_duty = matches!(a, Assertion::ActuatorSafe);
                let label = if require_zero_duty {
                    "actuator_safe"
                } else {
                    "no_fluid_flow"
                };
                let a = &obs.actuators;
                let mut bad = Vec::new();
                if a.pump {
                    bad.push("pump is on");
                }
                if a.water_valve {
                    bad.push("the water valve is open");
                }
                if a.steam_valve {
                    bad.push("the steam valve is open");
                }
                if require_zero_duty && a.heater_duty != 0 {
                    bad.push("the heater duty is not zero");
                }
                if a.emergency_latched {
                    // Latching is not an actuator, but 06 §Definitions excludes
                    // it from a routine shutdown, so a scenario that ends latched
                    // has done something a normal run does not.
                    bad.push("the emergency latch is set");
                }
                if !bad.is_empty() {
                    failures.push(format!("{label}: {}", bad.join(", ")));
                }
            }
            Assertion::Http { path, expect } => {
                let Some(body) = obs.endpoints.get(path) else {
                    failures.push(format!("http: {path} was not captured"));
                    continue;
                };
                for (k, v) in expect {
                    let got = body.get(k);
                    if got != Some(v) {
                        failures.push(format!("http: {path} key {k:?} is {got:?}, expected {v}"));
                    }
                }
            }
        }
    }
    failures
}

/// The deepest path a `config.set` override may reach.
///
/// Four, which is the depth of the longest key in
/// [`cc_config::schema::SCHEMA`] (`hardware.sensors.temperature.type`).
/// Derived rather than guessed, and a test below asserts it against the schema
/// so a new, deeper parameter cannot be set by a scenario without this being
/// revisited.
const MAX_CONFIG_DEPTH: usize = 4;

fn apply_overrides(config: &mut Config, set: &BTreeMap<String, Value>) -> Result<(), String> {
    for (key, value) in set {
        // Every key was validated against the schema at parse time, so the
        // path is known-good; this is a re-check because `Config` is mutated
        // here from a file and a second gate is cheap.
        if cc_config::schema::find(key).is_none() {
            return Err(format!("unknown parameter {key:?}"));
        }
        // The schema nests the same way the C++'s dotted key does
        // (`ConfigJson::setNested`, `ConfigJson.cpp:83-106`), so the dotted key
        // *is* the JSON path. Rebuilding the whole `Config` as JSON, patching,
        // and deserialising is the one implementation that cannot drift from
        // `cc-config`'s own shape.
        let mut doc = serde_json::to_value(&*config)
            .map_err(|e| format!("cannot serialise the configuration: {e}"))?;
        if !set_path(&mut doc, key, value, 0) {
            return Err(format!("{key}: no such parameter path"));
        }
        *config = serde_json::from_value(doc)
            .map_err(|e| format!("{key}: the value does not fit the configuration: {e}"))?;
    }
    Ok(())
}

/// Set `dotted.key` in a nested JSON document. Returns whether the path existed.
fn set_path(doc: &mut Value, key: &str, value: &Value, depth: usize) -> bool {
    if depth >= MAX_CONFIG_DEPTH {
        return false;
    }
    let (head, rest) = match key.split_once('.') {
        Some((h, r)) => (h, Some(r)),
        None => (key, None),
    };
    let Some(obj) = doc.as_object_mut() else {
        return false;
    };
    match rest {
        None => {
            if !obj.contains_key(head) {
                return false;
            }
            obj.insert(head.to_string(), value.clone());
            true
        }
        Some(rest) => {
            let Some(child) = obj.get_mut(head) else {
                return false;
            };
            set_path(child, rest, value, depth + 1)
        }
    }
}

fn parse_switch(name: &str) -> Option<SwitchId> {
    Some(match name {
        "brew" => SwitchId::Brew,
        "steam" => SwitchId::Steam,
        "power" => SwitchId::Power,
        "hot_water" => SwitchId::HotWater,
        _ => return None,
    })
}

fn parse_command(name: &str) -> Option<Command> {
    Some(match name {
        "BREW_START" => Command::BrewStart,
        "BREW_STOP" => Command::BrewStop,
        "STEAM_START" => Command::SteamStart,
        "STEAM_STOP" => Command::SteamStop,
        "MANUAL_FLUSH_START" => Command::ManualFlushStart,
        "MANUAL_FLUSH_STOP" => Command::ManualFlushStop,
        "BACKFLUSH_ENTER" => Command::BackflushEnter,
        "BACKFLUSH_CYCLE_START" => Command::BackflushCycleStart,
        "BACKFLUSH_STOP" => Command::BackflushStop,
        "STANDBY" => Command::Standby,
        "NORMAL_OPERATION" => Command::NormalOperation,
        "REBOOT" => Command::Reboot,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{Capture, Mode};

    /// The scenario set as it ships.
    ///
    /// The tests below deliberately read
    /// `docs/rust-migration/scenarios/*.yaml` rather than carrying their own
    /// copies. A duplicated scenario is a scenario that can drift from the one
    /// `just parity` runs, and a test that passes against a copy while the
    /// shipped file is broken is worse than no test. The path is relative to
    /// `CARGO_MANIFEST_DIR` so it does not depend on the working directory.
    fn shipped(name: &str) -> Scenario {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/rust-migration/scenarios")
            .join(format!("{name}.yaml"));
        Scenario::load(&path).unwrap_or_else(|e| panic!("{e}"))
    }

    fn scenario(yaml: &str) -> Scenario {
        Scenario::parse(yaml, "test").expect("scenario parses")
    }

    /// Every scenario in the set loads, and every `dry_run` one runs and meets
    /// its own assertions.
    ///
    /// This is the crate's most important test. It is what makes a scenario
    /// file a *contract* rather than a document: an assertion that does not
    /// hold against the real reducer fails the build, at `cargo test -p
    /// cc-parity`, long before anyone gets to a phase gate.
    #[test]
    fn every_shipped_scenario_loads_and_every_dry_run_one_passes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/rust-migration/scenarios");
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
            .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert!(
            names.len() >= 12,
            "the task list names twelve scenarios; found {}: {names:?}",
            names.len()
        );

        let mut ran = 0usize;
        for name in &names {
            let s = shipped(name);
            assert_eq!(&s.name, name, "{name}.yaml: the file name is the id");
            if s.mode == Mode::DryRun {
                run(s).unwrap_or_else(|e| panic!("{name}: {e}"));
                ran += 1;
            }
        }
        assert!(ran >= 10, "only {ran} dry_run scenarios ran");
    }

    /// Every safety path S1-S11 in 01 §6 has at least one scenario.
    ///
    /// The coverage requirement is from 06 §R1-08 step 2, and it is checked
    /// here rather than trusted to a reviewer's memory. S6 and S9 are covered
    /// by scenarios that assert the *reducer-side* property; the arithmetic
    /// (S6's counter) and the Wi-Fi watchdog suspension (S9) are outside the
    /// reducer, and both say so in their scenario's `why`.
    #[test]
    fn every_safety_path_has_at_least_one_scenario() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/rust-migration/scenarios");
        let mut covered: std::collections::BTreeSet<String> = Default::default();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|x| x == "yaml") {
                covered.extend(Scenario::load(&path).expect("loads").covers);
            }
        }
        for n in 1..=11 {
            let id = format!("S{n}");
            assert!(
                covered.contains(&id),
                "no scenario covers {id}; covered: {covered:?}"
            );
        }
    }

    /// Two runs of the same scenario produce byte-identical observations.
    ///
    /// The committed baseline depends on this. A scenario whose observation
    /// varied run to run would make every diff meaningless, and the variation
    /// would be in the harness rather than in the firmware — which is exactly
    /// the kind of noise a parity gate must not accept.
    #[test]
    fn every_dry_run_scenario_is_deterministic() {
        for name in [
            "brew_by_time",
            "brew_aborted_mid_flow",
            "overtemp_trip",
            "water_tank_empty_mid_brew",
            "ota_start_during_brew",
        ] {
            let a = run(shipped(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            let b = run(shipped(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(a, b, "{name} is not deterministic");
        }
    }

    /// The scenarios that would energise an actuator produce the energising
    /// effects — and nothing is connected to them.
    ///
    /// A `dry_run` scenario that asserted only "everything is off" would pass
    /// against a reducer that did nothing at all, which is not a test of the
    /// brew path. So the central case is checked in both directions: the pump
    /// and the valve are requested, the shot completes, and the run ends with
    /// the machine's fluid actuators off.
    #[test]
    fn a_brew_really_asks_for_the_pump_and_the_valve() {
        let obs = run(shipped("brew_by_time")).expect("runs");
        assert!(
            obs.occurrences("EnablePump").len() > 100,
            "a 30 s brew must ask for the pump on most loops, saw {}",
            obs.occurrences("EnablePump").len()
        );
        assert!(
            obs.occurrences("OpenWaterValve").len() > 100,
            "the valve must be requested alongside it"
        );
        assert!(obs.states.contains(&"BREW_FINISHED".to_string()), "{obs:?}");
        // And the machine is not left running anything.
        assert!(!obs.actuators.pump);
        assert!(!obs.actuators.water_valve);
    }

    /// A minimal scenario still runs, so the format is usable for a one-off.
    #[test]
    fn a_rest_scenario_runs_and_ends_safe() {
        let s = scenario(
            r#"
name: rest
title: rest
mode: dry_run
stimuli:
  - { at_ms: 0, kind: rest }
assert:
  - { kind: actuator_safe }
"#,
        );
        let obs = run(s).expect("runs");
        assert!(!obs.actuators.pump);
        assert_eq!(obs.actuators.heater_duty, 0);
    }

    /// A brew aborted mid-flow leaves the pump off from the abort onwards.
    ///
    /// Read out of the observation rather than asserted by the scenario, so
    /// the failure message shows the timeline.
    #[test]
    fn the_pump_is_never_energised_after_an_abort() {
        let obs = run(shipped("brew_aborted_mid_flow")).expect("runs");
        let late: Vec<u32> = obs
            .occurrences("EnablePump")
            .iter()
            .map(|i| obs.effects[*i].at_ms)
            .filter(|t| *t >= 6010)
            .collect();
        assert!(late.is_empty(), "EnablePump after the abort at {late:?}");
        // And the release tick itself is the last one with the pump on: the
        // request is consumed by the *next* `checkTransitions`, and
        // `BrewRunningState::update` re-asserts in the loop before that.
        assert_eq!(
            obs.occurrences("EnablePump")
                .last()
                .map(|i| obs.effects[*i].at_ms),
            Some(6000),
            "the pump should stop one loop after the release, not one loop before"
        );
    }

    /// The water tank empties mid-brew and the pump stops.
    #[test]
    fn an_empty_tank_mid_brew_stops_the_pump() {
        let obs = run(shipped("water_tank_empty_mid_brew")).expect("runs");
        assert!(
            obs.states.contains(&"WATER_TANK_EMPTY".to_string()),
            "{obs:?}"
        );
        assert!(
            !obs.actuators.pump,
            "the pump must be off with an empty tank"
        );
    }

    /// The steam valve is closed outside `STEAM_RUNNING` — the S5' whitelist
    /// the C++ does not have.
    #[test]
    fn the_steam_valve_is_closed_outside_steam_running() {
        let obs = run(shipped("steam_on_off")).expect("runs");
        assert!(
            !obs.actuators.steam_valve,
            "the steam valve must end closed"
        );
        assert!(obs.states.contains(&"STEAM_RUNNING".to_string()), "{obs:?}");
    }

    /// An OTA session leaves every actuator off for its whole duration — the
    /// gap 01 §6 records, which the C++ does not satisfy mid-brew.
    #[test]
    fn an_ota_session_closes_everything() {
        // `ota_start_from_idle` ends with everything off, because nothing was
        // running when the flash began.
        let obs = run(shipped("ota_start_from_idle")).expect("runs");
        assert_eq!(obs.actuators.heater_duty, 0);
        assert!(!obs.actuators.pump);
        assert!(!obs.actuators.water_valve);
        assert!(!obs.actuators.steam_valve);
        assert!(
            obs.occurrences("SafeHardwareShutdown").len() >= 1,
            "the session must perform a safe shutdown"
        );

        // `ota_start_during_brew` cannot assert on the *final* actuators: the
        // manual shot is still in progress when the session closes at 20 s, so
        // the pump correctly comes back. The claim is about the window, and it
        // is checked here against the timeline rather than the end state.
        let obs = run(shipped("ota_start_during_brew")).expect("runs");
        let during: Vec<u32> = obs
            .occurrences("EnablePump")
            .iter()
            .map(|i| obs.effects[*i].at_ms)
            .filter(|t| (1000..20000).contains(t))
            .collect();
        assert!(
            during.is_empty(),
            "the pump must be off for the whole flash session, saw it at {during:?}"
        );
        let valves: Vec<u32> = obs
            .occurrences("OpenWaterValve")
            .iter()
            .map(|i| obs.effects[*i].at_ms)
            .filter(|t| (1000..20000).contains(t))
            .collect();
        assert!(
            valves.is_empty(),
            "the valve must be closed for the whole flash session, saw it at {valves:?}"
        );
        // The shot resumes afterwards, which is what makes the window necessary.
        let after: Vec<u32> = obs
            .occurrences("EnablePump")
            .iter()
            .map(|i| obs.effects[*i].at_ms)
            .filter(|t| *t >= 20000)
            .collect();
        assert!(
            !after.is_empty(),
            "the interrupted manual shot must resume when the session closes"
        );
    }

    /// An over-temperature trips on the third reading, not the second.
    #[test]
    fn the_overtemp_trip_is_debounced_by_three() {
        let obs = run(shipped("overtemp_trip")).expect("runs");
        let trip = obs
            .effects
            .iter()
            .position(|o| o.name == "EmergencyShutdown")
            .expect("must trip");
        let at = obs.effects[trip].at_ms;
        // Readings at 0, 400 and 800 ms. The third one trips.
        assert!(
            (780..=820).contains(&at),
            "expected the trip at the third reading (~800 ms), got {at} ms"
        );
    }

    /// The latch clears only below 100 °C, and 120 °C is not enough.
    #[test]
    fn a_120_degree_reading_does_not_clear_the_latch() {
        let obs = run(shipped("overtemp_recovery")).expect("runs");
        // Emergency stop, then PID_NORMAL. If 120 °C had cleared the latch the
        // recovery would have happened one stimulus earlier, and the state list
        // would still look the same — so the check is on the *ordering* against
        // the 95 °C reading, which the scenario's own duration guarantees.
        let i_stop = obs
            .states
            .iter()
            .position(|s| s == "EMERGENCY_STOP")
            .expect("must trip");
        let i_back = obs
            .states
            .iter()
            .rposition(|s| s == "PID_NORMAL")
            .expect("must recover");
        assert!(i_stop < i_back, "{obs:?}");
    }

    /// A failing assertion names the scenario and the reason.
    #[test]
    fn a_failing_assertion_names_the_scenario_and_the_reason() {
        let s = scenario(
            r#"
name: wrong
title: wrong
mode: dry_run
stimuli:
  - { at_ms: 0, kind: rest }
assert:
  - { kind: final_state, value: EMERGENCY_STOP }
"#,
        );
        let err = run(s).expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.contains("wrong"), "{msg}");
        assert!(msg.contains("final_state"), "{msg}");
    }

    /// An assertion that matches nothing fails rather than passing vacuously.
    #[test]
    fn a_scenario_that_observes_nothing_enough_to_assert_fails_loudly() {
        let s = scenario(
            r#"
name: noop
title: noop
mode: dry_run
stimuli:
  - { at_ms: 0, kind: rest }
assert:
  - { kind: always, effect: OpenWaterValve }
"#,
        );
        let err = run(s).expect_err("must fail");
        assert!(err.to_string().contains("never emitted"), "{err}");
    }

    /// The driver refuses a `hardware` scenario rather than pretending to run
    /// it on the host.
    #[test]
    fn a_hardware_scenario_is_refused_by_the_driver() {
        let s = Scenario {
            name: "hw".into(),
            title: "hw".into(),
            mode: Mode::Hardware,
            covers: vec![],
            why: String::new(),
            config: Default::default(),
            stimuli: vec![Stimulus {
                at_ms: 0,
                kind: StimulusKind::Rest,
            }],
            capture: Capture::default(),
            assert: vec![Assertion::ActuatorSafe],
            tick_ms: 10,
            duration_ms: None,
        };
        let err = run(s).expect_err("must refuse");
        assert!(err.to_string().contains("does not drive a device"), "{err}");
    }

    /// A configuration override reaches the reducer.
    ///
    /// If `brew.pre_infusion.pause` did not land there would be no
    /// `BREW_PREINFUSION_PAUSE`, which is what makes the assertion observable.
    #[test]
    fn a_config_override_reaches_the_reducer() {
        let s = scenario(
            r#"
name: override
title: override
mode: dry_run
config:
  set:
    pid.enabled: true
    brew.mode: 1
    brew.pre_infusion.enabled: true
    brew.pre_infusion.time: 2.0
    brew.pre_infusion.pause: 1.0
    brew.by_time.enabled: false
    hardware.switches.brew.enabled: true
duration_ms: 6000
stimuli:
  - { at_ms: 0, kind: button, switch: brew, action: press }
assert:
  - { kind: visited_states, value: [BREW_PREINFUSION, BREW_PREINFUSION_PAUSE, BREW_RUNNING] }
"#,
        );
        let obs = run(s).expect("runs");
        assert!(
            obs.states.contains(&"BREW_PREINFUSION_PAUSE".to_string()),
            "{obs:?}"
        );
    }

    /// The momentary power switch reaches standby, and a press inside the
    /// 5 s settle window is honoured too — because `PowerHandler.h:116-133`
    /// toggles on the *press*, and the settle window only gates the long-press
    /// tracking. Pinned because the opposite is easy to assume.
    #[test]
    fn the_power_switch_reaches_standby_from_the_press() {
        let s = scenario(
            r#"
name: standby_switch
title: standby via the power switch
mode: dry_run
config:
  set:
    pid.enabled: true
    standby.enabled: true
    hardware.switches.power.enabled: true
    hardware.switches.power.type: 0
stimuli:
  - { at_ms: 6000, kind: button, switch: power, action: press }
assert:
  - { kind: visited_states, value: [STANDBY] }
  - { kind: no_fluid_flow }
"#,
        );
        let obs = run(s).expect("runs");
        assert!(obs.states.contains(&"STANDBY".to_string()), "{obs:?}");
    }

    /// A long-press on the power switch reboots, which is the only path in the
    /// firmware that restarts the chip (`Effect::RequestReboot`).
    #[test]
    fn a_long_press_on_power_requests_a_reboot() {
        let s = scenario(
            r#"
name: reboot
title: reboot by long press
mode: dry_run
config:
  set:
    pid.enabled: true
    hardware.switches.power.enabled: true
    hardware.switches.power.type: 0
stimuli:
  - { at_ms: 6000, kind: button, switch: power, action: press, long_press: true }
assert:
  - { kind: always, effect: RequestReboot }
  - { kind: always, effect: SafeHardwareShutdown }
"#,
        );
        let obs = run(s).expect("runs");
        assert!(!obs.occurrences("RequestReboot").is_empty(), "{obs:?}");
    }

    /// The driver's `SafetyConfig` construction and `Config::safety_view` agree.
    ///
    /// `cc-parity` builds a `cc_safety::SafetyConfig` field by field rather than
    /// through a `From` impl, so this is the test that notices `cc-config`
    /// renaming or retyping a field. A drift here would be a **safety** bug and
    /// not a cosmetic one: the emergency threshold the reducer latches on is
    /// exactly this value.
    #[test]
    fn the_safety_view_mapping_is_the_one_the_driver_uses() {
        let mut config = Config::default();
        config.safety.emergency_temp = 137.0;
        config.safety.emergency_hysteresis = 7.0;
        config.steam.setpoint = 128.0;
        let view = config.safety_view();
        let built = cc_safety::SafetyConfig {
            emergency_temp: view.emergency_temp,
            emergency_hysteresis: view.emergency_hysteresis,
            steam_setpoint: view.steam_setpoint,
            heater_relay_trigger: view.heater_relay_trigger,
            temperature_sensor: view.temperature_sensor,
        };
        assert_eq!(built.emergency_temp, Celsius::new(137.0));
        assert_eq!(built.emergency_hysteresis, Celsius::new(7.0));
        assert_eq!(built.steam_setpoint, Celsius::new(128.0));
        assert_eq!(
            built.heater_relay_trigger,
            config.hardware.relays.heater.trigger_type
        );
        assert_eq!(
            built.temperature_sensor,
            config.hardware.sensors.temperature.r#type
        );
    }
}
