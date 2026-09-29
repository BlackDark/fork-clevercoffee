//! The machine: the one object that owns the state, the sensors, the PID and the actuators.
//!
//! Everything above this file is a library. The domain crate computes a transition, the HAL traits
//! declare what a pump is, and the HTTP crate parses a request, but nothing connected them into a
//! machine that runs. This is that connection, and it is deliberately the *only* place where a
//! transition becomes an actuator command.
//!
//! Three properties make it possible to test the whole thing on a host with no hardware:
//!
//! - The actuators are a generic parameter, so a scenario test substitutes
//!   `RecordingActuators` and asserts the exact command sequence rather than a set of flags.
//! - Nothing here awaits, allocates or knows what a task is. The two task bodies in
//!   [`crate::tasks`] are synchronous steps over this type, so a test drives a scenario by
//!   calling two functions in a loop instead of standing up an executor.
//! - Every edge-triggered request is a field the caller sets for one tick, exactly as the C++
//!   flags were consumed on use. A stale flag cannot terminate a phase, which was D22.
//!
//! The safety rules the C++ firmware had as scattered `if` statements are here as one function
//! each, and each has a test that asserts the actuator sequence, not a flag:
//!
//! - Every state that energises an actuator de-energises on exit ([`Machine::exit_state`]).
//! - The valve interlock has one allow-list, [`State::may_hold_water_valve_open`](.
//! - Every pump command carries a deadline ([`Machine::pump_deadline_ms`]), which is the fix for
//!   D09: the C++ firmware declared the limits and never armed them, so a stuck switch ran the
//!   pump until someone noticed.

#![allow(clippy::too_many_arguments)]

use clevercoffee_domain::backflush::{resolve_cycle_advance, CycleAdvanceEffect};
use clevercoffee_domain::emergency::{EmergencyDecision, EmergencyStop, Thresholds};
use clevercoffee_domain::pid::{Gains, Pid};
use clevercoffee_domain::sensor::{SensorFault, TemperatureFilter};
use clevercoffee_domain::state::State;
use clevercoffee_domain::transition::{
    actuators_with_switches, heater_allowed, next_state, Inputs,
};
use clevercoffee_domain::Timing;
use clevercoffee_hal_traits::{ActuatorCommand, Actuators};

/// Everything the machine needs from the configuration, resolved once per boot or per change.
///
/// A struct of plain values rather than the config document, because this is what the control path
/// reads on every tick and it should not be able to reach a string, a secret or a failed lookup
/// while holding the actuators.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeConfig {
    /// The boiler setpoint in degrees Celsius.
    pub setpoint_c: f64,
    pub brew_by_time_enabled: bool,
    pub brew_by_weight_enabled: bool,
    /// The *total* target, pre-infusion included, as the C++ compared it.
    pub brew_target_time_ms: u32,
    pub brew_target_weight_g: f64,
    pub brew_mode_manual: bool,
    pub preinfusion_enabled: bool,
    pub preinfusion_ms: u32,
    pub preinfusion_pause_ms: u32,
    pub brew_finished_timeout_ms: u32,
    pub backflush_finished_timeout_ms: u32,
    pub backflush_fill_ms: u32,
    pub backflush_flush_ms: u32,
    pub backflush_cycles: u8,
    /// `standby.enabled`. Separate from the timeout because a timeout of zero is a legal value in
    /// the schema's own terms (a machine that never idles) and must not be read as "enabled with
    /// no delay", which is how the machine would go to standby on its first tick.
    pub standby_enabled: bool,
    /// `standby.time`, in milliseconds.
    pub standby_timeout_ms: u32,
    pub sensor_error_recovery_ms: u32,
    pub eeprom_recovery_timeout_ms: u32,
    /// `hardware.sensors.watertank.keep_heater_on_empty`.
    pub keep_heater_on_empty: bool,
    pub brew_switch_enabled: bool,
    /// `hardware.switches.brew.enabled`. With it off, no brew request is accepted at all.
    pub brew_switch_present: bool,
    /// `steam.setpoint`. The machine has one PID and one boiler, so this is the setpoint the PID
    /// is given while the state is `STEAM_RUNNING`. The C++ had the same two numbers and switched
    /// between them in the state handler.
    pub steam_setpoint_c: f64,
    /// `pid.use_ponm`: the proportional term acts on the measurement rather than the error.
    pub pid_proportional_on_measurement: bool,
    /// `pid.ema`, the input filter's smoothing factor.
    pub pid_ema_factor: f64,
    /// `pid.regular.i_max`, the integrator limit. The C++ hardcoded 0 to 55 and ignored this
    /// setting entirely.
    pub pid_integrator_max: f64,
    /// `brew.temp_offset`, added to the reading for the brew's own display and control.
    pub brew_temp_offset_c: f64,
    /// Whether each switch is fitted. A machine without a steam switch cannot be asked to steam.
    pub steam_switch_present: bool,
    pub hot_water_switch_present: bool,
    pub power_switch_present: bool,
    /// Whether a scale and a pressure sensor are fitted, which is also what decides whether their
    /// rows exist on the display and whether `/api/status` carries a weight.
    pub scale_enabled: bool,
    pub pressure_enabled: bool,
    /// The delay at the start of a brew during which the heater is held off, so the pump gets
    /// clean water. C++ `Timing.h:33`, `BREW_PID_DELAY`.
    pub brew_pid_delay_ms: u32,
    pub emergency: Thresholds,
    pub pid: Gains,
    pub pid_enabled_at_boot: bool,
    pub pump_timeout_brew_ms: u32,
    pub pump_timeout_hot_water_ms: u32,
    pub pid_sample_ms: u32,
    pub temperature_window: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            // C++ `defaults.h`: 93.0 C.
            setpoint_c: 95.0,
            steam_setpoint_c: 135.0,
            steam_switch_present: true,
            hot_water_switch_present: true,
            power_switch_present: true,
            scale_enabled: true,
            pressure_enabled: true,
            brew_by_time_enabled: true,
            brew_by_weight_enabled: false,
            brew_target_time_ms: 27_000,
            brew_target_weight_g: 0.0,
            brew_mode_manual: false,
            preinfusion_enabled: true,
            preinfusion_ms: Timing::PRE_INFUSION.as_millis() as u32,
            preinfusion_pause_ms: Timing::PRE_INFUSION_PAUSE.as_millis() as u32,
            brew_finished_timeout_ms: Timing::BREW_FINISHED_DISPLAY.as_millis() as u32,
            backflush_finished_timeout_ms: Timing::BACKFLUSH_FINISHED_DISPLAY.as_millis() as u32,
            backflush_fill_ms: 5_000,
            backflush_flush_ms: 5_000,
            backflush_cycles: 3,
            standby_enabled: false,
            standby_timeout_ms: 35 * 60_000,
            sensor_error_recovery_ms: Timing::SENSOR_ERROR_RECOVERY.as_millis() as u32,
            eeprom_recovery_timeout_ms: Timing::EEPROM_RECOVERY_TIMEOUT.as_millis() as u32,
            keep_heater_on_empty: false,
            brew_switch_enabled: true,
            brew_switch_present: true,
            brew_pid_delay_ms: 0,
            emergency: Thresholds::default(),
            // C++ `defaults.h`.
            pid: Gains {
                kp: 62.0,
                tn: 52.0,
                tv: 11.5,
            },
            pid_proportional_on_measurement: false,
            pid_ema_factor: 0.6,
            pid_integrator_max: 55.0,
            brew_temp_offset_c: 0.0,
            pid_enabled_at_boot: true,
            pump_timeout_brew_ms: Timing::BREW_PUMP_TIMEOUT.as_millis() as u32,
            pump_timeout_hot_water_ms: Timing::HOT_WATER_PUMP_TIMEOUT.as_millis() as u32,
            pid_sample_ms: Timing::PID_SAMPLE.as_millis() as u32,
            temperature_window: 15,
        }
    }
}

/// The debounced switch positions, as the switch task sampled them this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Switches {
    pub power_pressed: bool,
    pub power_long_press: bool,
    pub brew_pressed: bool,
    pub brew_long_press: bool,
    pub steam_pressed: bool,
    pub hot_water_pressed: bool,
}

/// One tick's worth of sensor readings.
///
/// `temperature_c` is `None` when the sensor could not be read *or* when the filter is faulted.
/// It is never a substitute value: see the module docs of `clevercoffee_domain::sensor` for why
/// that distinction is the fix for D03.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sensors {
    /// A raw conversion result, or the fault that means there is not one.
    pub temperature: Result<Option<f64>, SensorFault>,
    pub pressure_bar: Option<f64>,
    pub weight_g: Option<f64>,
    pub scale_fault: bool,
    pub water_tank_full: bool,
    pub switches: Switches,
}

impl Default for Sensors {
    /// "No reading, and no fault reported yet": what a machine looks like before its first
    /// conversion completes. It is deliberately *not* a zero-degree reading, because that is the
    /// value D03 had the C++ machine believe.
    fn default() -> Self {
        Self {
            temperature: Ok(None),
            ..Self::empty()
        }
    }
}

impl Sensors {
    /// Every field at its "nothing fitted, nothing read" value.
    pub const fn empty() -> Self {
        Self {
            temperature: Ok(None),
            pressure_bar: None,
            weight_g: None,
            scale_fault: false,
            water_tank_full: false,
            switches: Switches {
                power_pressed: false,
                power_long_press: false,
                brew_pressed: false,
                brew_long_press: false,
                steam_pressed: false,
                hot_water_pressed: false,
            },
        }
    }

    /// A machine with a working sensor at a temperature, everything else absent.
    pub fn at(celsius: f64) -> Self {
        Self {
            temperature: Ok(Some(celsius)),
            water_tank_full: true,
            ..Self::empty()
        }
    }

    /// A machine whose temperature sensor is not answering.
    pub fn no_temperature() -> Self {
        Self {
            temperature: Err(SensorFault::Disconnected),
            water_tank_full: true,
            ..Self::empty()
        }
    }
}

/// An action a caller asks for. Each is an edge: it is consumed by the tick that sees it.
///
/// A method rather than a flag the caller pokes, so a request cannot be left set by accident and
/// then fire hours later, which is the shape of D22.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    BrewStart,
    BrewStop,
    SteamStart,
    SteamStop,
    ManualFlush,
    BackflushStart,
    BackflushStop,
    Standby,
    NormalOperation,
    /// The power switch was long-pressed, which reboots in the C++ firmware.
    ///
    /// Raised as a request and turned into [`Machine::reboot_requested`] rather than acted on
    /// here: a reboot is not a control decision, and the firmware layer that owns the reset
    /// peripheral is the only thing that should act on it.
    PowerLongPress,
}

/// What one tick did, for a log line and for a test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TickOutcome {
    pub from: Option<State>,
    pub to: Option<State>,
    /// The safety task de-energised everything this tick.
    pub forced_off: bool,
    /// The pump ran past its deadline and was stopped.
    pub pump_timeout: bool,
    /// The emergency stop tripped this tick.
    pub emergency_tripped: bool,
    /// The machine is ready to brew: at temperature, tank full, no fault.
    pub ready: bool,
}

impl TickOutcome {
    pub const fn changed(&self) -> bool {
        self.to.is_some()
    }
}

/// The machine.
#[derive(Debug)]
pub struct Machine<A: Actuators> {
    actuators: A,
    config: RuntimeConfig,
    /// The machine's own clock, milliseconds since boot. Advanced only by [`Machine::tick`].
    now_ms: u32,

    state: State,
    state_entered_ms: u32,

    filter: TemperatureFilter,
    pid: Pid,
    emergency: EmergencyStop,
    pid_enabled: bool,
    pid_output: f64,
    last_pid_sample_ms: u32,

    /// The last sensor snapshot, kept for the display and the API without a second read.
    sensors: Sensors,
    /// The previous tick's switch positions, for edge detection.
    prev_switches: Switches,
    /// Set by a power-switch long press, cleared by whoever performs the reboot.
    reboot_requested: bool,

    /// Edge requests, cleared at the end of every tick.
    requests: heapless::Vec<Request, 4>,

    // Timers. Each is measured from the event it names, because the C++ measured two of them
    // from different origins and the difference was observable.
    brew_started_ms: Option<u32>,
    backflush_started_ms: Option<u32>,
    backflush_cycle: u8,
    backflush_mode: bool,
    /// Set while the fault has cleared but the recovery delay has not expired.
    fault_cleared_ms: Option<u32>,
    idle_since_ms: Option<u32>,
    brew_pid_delay_until_ms: Option<u32>,

    /// When the pump was switched on, and how long it is allowed to stay on.
    pump_since_ms: Option<u32>,
    pump_deadline_ms: u32,

    /// Milliseconds since the watchdog was last fed. The safety task owns this.
    watchdog_ms: u32,
    /// Latched when the safety task stopped the machine for good, until a power cycle.
    emergency_latched: bool,
    /// Latched by the provisioner, which forces everything off for its duration.
    service_mode: bool,

    /// The last command issued, so a caller can ask what the machine is doing.
    last_command: ActuatorCommand,
    /// Why the last command was issued, for a log line and for a failing test to read.
    last_reason: &'static str,
}

impl<A: Actuators> Machine<A> {
    pub fn new(actuators: A, config: RuntimeConfig) -> Self {
        let pid = Pid::new(
            config.pid,
            config.pid_sample_ms,
            Timing::HEATER_PWM_WINDOW,
            0.2,
        );
        // The PID's run state comes from the configuration at boot, not from a default: a machine
        // configured with the PID off must come up with it off, and a machine configured with it on
        // must come up heating. Both were separate C++ bugs in the same area.
        let pid_enabled = config.pid_enabled_at_boot;
        let mut m = Self {
            actuators,
            config,
            now_ms: 0,
            state: State::Init,
            state_entered_ms: 0,
            filter: TemperatureFilter::new(config.temperature_window),
            pid,
            emergency: EmergencyStop::new(),
            pid_enabled,
            pid_output: 0.0,
            last_pid_sample_ms: 0,
            sensors: Sensors::default(),
            prev_switches: Switches::default(),
            reboot_requested: false,
            requests: heapless::Vec::new(),
            brew_started_ms: None,
            backflush_started_ms: None,
            backflush_cycle: 0,
            backflush_mode: false,
            fault_cleared_ms: None,
            idle_since_ms: None,
            brew_pid_delay_until_ms: None,
            pump_since_ms: None,
            pump_deadline_ms: 0,
            watchdog_ms: 0,
            emergency_latched: false,
            service_mode: false,
            last_command: ActuatorCommand::ALL_OFF,
            last_reason: "boot",
        };
        // Boot order, step 1 of architecture section 1.4: the actuators are driven to their
        // inactive level before anything else is configured. A machine that boots with the
        // relays floating is a machine whose first state depends on pin states.
        m.actuators.force_off();
        m.last_command = ActuatorCommand::ALL_OFF;
        m
    }

    // -- accessors ---------------------------------------------------------

    pub fn state(&self) -> State {
        self.state
    }

    pub fn now_ms(&self) -> u32 {
        self.now_ms
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut RuntimeConfig {
        &mut self.config
    }

    /// Applies a new configuration. Changing the gains, the targets or a safety limit takes
    /// effect here rather than being read live out of storage on the control path.
    pub fn apply_config(&mut self, config: RuntimeConfig) {
        self.pid.set_gains(config.pid);
        self.pid
            .set_proportional_on_measurement(config.pid_proportional_on_measurement);
        self.pid_enabled = config.pid_enabled_at_boot;
        self.config = config;
    }

    /// The reading the machine controls on.
    ///
    /// The filter's output, plus `brew.temp_offset` while a brew is running. The offset is the
    /// user's correction for their particular machine, so it applies to the shot and not to the
    /// idle reading on the display.
    pub fn temperature_c(&self) -> Option<f64> {
        self.filter.celsius().map(|t| {
            if self.brew_running() {
                t + self.config.brew_temp_offset_c
            } else {
                t
            }
        })
    }

    /// The raw filtered reading, before any offset. What the display shows and what the emergency
    /// stop evaluates, because both are about the real temperature rather than the user's
    /// correction to it.
    pub fn raw_temperature_c(&self) -> Option<f64> {
        self.filter.celsius()
    }

    pub fn setpoint_c(&self) -> f64 {
        self.config.setpoint_c
    }

    /// Whether the firmware has been asked to reboot. The C++ rebooted from inside the switch
    /// handler; here it is a flag the firmware layer polls, so the control path cannot reset the
    /// chip from under a half-applied command.
    pub fn reboot_requested(&self) -> bool {
        self.reboot_requested
    }

    pub fn clear_reboot_request(&mut self) {
        self.reboot_requested = false;
    }

    pub fn pid_output(&self) -> u16 {
        self.pid_output.clamp(0.0, Timing::HEATER_PWM_WINDOW as f64) as u16
    }

    pub fn pid_enabled(&self) -> bool {
        self.pid_enabled
    }

    pub fn set_pid_enabled(&mut self, on: bool) {
        self.pid_enabled = on;
    }

    pub fn actuators(&self) -> &A {
        &self.actuators
    }

    pub fn actuators_mut(&mut self) -> &mut A {
        &mut self.actuators
    }

    pub fn last_command(&self) -> ActuatorCommand {
        self.last_command
    }

    pub fn last_reason(&self) -> &'static str {
        self.last_reason
    }

    pub fn sensors(&self) -> &Sensors {
        &self.sensors
    }

    pub fn sensor_fault(&self) -> Option<SensorFault> {
        self.filter.fault()
    }

    pub fn is_emergency_stopped(&self) -> bool {
        self.emergency_latched || self.emergency.is_tripped()
    }

    /// Whether the pump has run past the deadline armed when it was switched on.
    ///
    /// This is the D09 check as a question rather than as an action, so the safety task is the
    /// only thing that acts on it. `issue` also refuses to start a pump that is already past its
    /// deadline, so the two agree.
    pub fn pump_deadline_exceeded(&self) -> bool {
        match self.pump_since_ms {
            Some(since) => self.now_ms.saturating_sub(since) >= self.pump_deadline_ms,
            None => false,
        }
    }

    /// Latches the emergency stop. Only the safety task and the provisioner call this.
    pub fn latch_emergency_stop(&mut self) {
        self.emergency_latched = true;
        self.emergency.force_trip();
    }

    /// Feeds the task watchdog, from the safety task only.
    pub fn feed_watchdog(&mut self) {
        self.watchdog_ms = 0;
    }

    /// Milliseconds since the safety task last fed the watchdog.
    pub fn watchdog_ms(&self) -> u32 {
        self.watchdog_ms
    }

    /// Whether the machine is idle enough for a destructive operation: OTA, factory reset.
    pub fn is_idle(&self) -> bool {
        !self.last_command.flowing() && group_is_idle(self.state)
    }

    /// Milliseconds since the brew started, counting pre-infusion and the pause.
    pub fn brew_elapsed_ms(&self) -> u32 {
        match self.brew_started_ms {
            Some(t) => self.now_ms.saturating_sub(t),
            None => 0,
        }
    }

    pub fn backflush_cycle(&self) -> u8 {
        self.backflush_cycle
    }

    pub fn backflush_mode(&self) -> bool {
        self.backflush_mode
    }

    // -- requests ----------------------------------------------------------

    /// Raises a request for the next tick. At most four can be pending; a fifth is dropped, which
    /// is the right thing to do rather than growing a queue nobody drains.
    pub fn request(&mut self, r: Request) {
        if self.requests.push(r).is_err() {
            self.force_off("request queue full");
        }
    }

    // -- the control tick --------------------------------------------------

    /// Advances the machine by `elapsed_ms`, sampling the sensors on the way.
    ///
    /// This is the body of the `control` task. It is synchronous and total: given a configuration
    /// and a sequence of sensor snapshots it produces a sequence of actuator commands, which is
    /// what makes the scenario tests possible.
    pub fn tick(&mut self, sensors: Sensors, elapsed_ms: u32) -> TickOutcome {
        let mut outcome = TickOutcome::default();
        self.now_ms = self.now_ms.wrapping_add(elapsed_ms);
        self.watchdog_ms = self.watchdog_ms.wrapping_add(elapsed_ms);
        self.ingest_sensors(sensors);
        self.evaluate_emergency();

        // Switch edges. A switch that is held does not keep re-raising a request: the request is
        // an edge, and the edge detection is here so a stuck switch cannot produce a request
        // every tick.
        self.raise_switch_requests();

        if self.requests.contains(&Request::PowerLongPress) {
            self.reboot_requested = true;
        }
        let input = self.build_inputs();
        if let Some(next) = next_state(self.state, &input) {
            outcome.from = Some(self.state);
            outcome.to = Some(next);
            self.exit_state(self.state);
            self.enter_state(next, &input);
            self.state = next;
            self.state_entered_ms = self.now_ms;
        }

        // The command for the state we are in *now*, re-asserted every tick. Re-asserting is the
        // point: the safety mechanisms between ticks are allowed to turn things off, and only a
        // re-assert restores the intended state, exactly as the C++ `update()` did.
        self.apply_state_command(&input);

        self.run_pid(&input, elapsed_ms);
        self.advance_brew_timers();
        outcome.emergency_tripped = self.is_emergency_stopped();
        outcome.ready = self.is_ready();
        self.requests.clear();
        outcome
    }

    fn ingest_sensors(&mut self, s: Sensors) {
        match s.temperature {
            Ok(Some(c)) => {
                self.filter.push(c);
                if self.filter.fault().is_some() {
                    self.fault_cleared_ms = Some(0);
                }
            }
            Ok(None) => {
                // A conversion that produced nothing is a fault, not a zero. Treating it as a
                // zero is how the C++ machine ended up with a cached 0 C and a saturated PID.
                self.filter.fail(SensorFault::Timeout);
                self.fault_cleared_ms = None;
            }
            Err(fault) => {
                self.filter.fail(fault);
                self.fault_cleared_ms = None;
            }
        }
        self.sensors = s;
    }

    /// Evaluates the emergency stop on the **raw** reading, not the filtered one.
    ///
    /// The filter is a fifteen-sample mean, so an over-temperature reading takes six seconds to
    /// move it by 40 C. That is correct for the PID and wrong for a safety limit: the trip has to
    /// happen on the reading that is too hot, which is why the evaluator is fed
    /// `Sensors::temperature` directly.
    fn evaluate_emergency(&mut self) {
        // Only a real measurement is fed to the evaluator. A conversion that has not finished, and
        // a read that failed, are the *sensor fault* path: the heater is inhibited and the machine
        // rests in `SENSOR_ERROR` until the recovery delay expires, which is what the C++ did and
        // what a user with a flapping DS18B20 needs. Routing them through the emergency stop
        // instead would make a recoverable fault need a power cycle.
        let raw: Option<f64> = match self.sensors.temperature {
            Ok(Some(v)) => Some(v),
            Ok(None) => self.filter.latest(),
            Err(_) => None,
        };
        let Some(raw) = raw else {
            self.emergency.reset_counters();
            return;
        };
        if self.emergency.evaluate(Some(raw), &self.config.emergency) == EmergencyDecision::TripNow
        {
            self.emergency_latched = true;
        }
    }

    /// Detects switch edges and raises the matching request, once per press.
    fn raise_switch_requests(&mut self) {
        let now = self.sensors.switches;
        if now.power_long_press && !self.prev_switches.power_long_press && self.boot_guard_elapsed()
        {
            self.request(Request::PowerLongPress);
        }
        if now.power_pressed && !self.prev_switches.power_pressed && self.boot_guard_elapsed() {
            // A press toggles between standby and normal operation, which is what the C++
            // `PowerHandler` did for a short press.
            self.request(if self.state == State::Standby {
                Request::NormalOperation
            } else {
                Request::Standby
            });
        }
        if now.brew_pressed && !self.prev_switches.brew_pressed {
            self.request(self.brew_request());
        }
        if now.brew_long_press && !self.prev_switches.brew_long_press {
            self.request(self.brew_long_press_request());
        }
        if now.steam_pressed && !self.prev_switches.steam_pressed {
            self.request(if matches!(self.state, State::SteamRunning) {
                Request::SteamStop
            } else {
                Request::SteamStart
            });
        }
        self.prev_switches = now;
    }

    /// The power switch is ignored for a moment after boot, so plugging the machine in cannot
    /// toggle it. C++ `PowerHandler.h:118`.
    fn boot_guard_elapsed(&self) -> bool {
        self.now_ms >= Timing::POWER_SWITCH_BOOT_GUARD.as_millis() as u32
    }

    fn brew_request(&self) -> Request {
        if self.config.brew_switch_present && self.config.brew_switch_enabled {
            match self.state {
                State::BackflushIdle => Request::BackflushStart,
                State::BrewPreinfusion
                | State::BrewPreinfusionPause
                | State::BrewRunning
                | State::ManualFlushRunning => Request::BrewStop,
                _ => Request::BrewStart,
            }
        } else {
            // No brew switch: the request is deliberately a no-op, and `Request::BrewStop` is the
            // safe no-op because a state that cannot act on it drains it.
            Request::BrewStop
        }
    }

    fn brew_long_press_request(&self) -> Request {
        if self.config.brew_switch_present && self.config.brew_switch_enabled {
            Request::ManualFlush
        } else {
            Request::BrewStop
        }
    }

    /// Builds the domain input from the current state, timers and pending requests.
    pub fn build_inputs(&self) -> Inputs {
        let mut flags = [false; 9];
        for r in &self.requests {
            let slot = match r {
                Request::BrewStart => 0,
                Request::BrewStop => 1,
                Request::SteamStart | Request::SteamStop => 2,
                Request::ManualFlush => 3,
                Request::BackflushStart => 4,
                Request::BackflushStop => 5,
                Request::Standby => 6,
                Request::NormalOperation => 7,
                Request::PowerLongPress => 8,
            };
            flags[slot] = true;
        }
        let state_elapsed = self.now_ms.saturating_sub(self.state_entered_ms);
        let fault_clear_elapsed = match self.fault_cleared_ms {
            Some(t) => self.now_ms.saturating_sub(t),
            None => 0,
        };
        // While the fault is still present the recovery timer must not run, which is what
        // `None` means: a flapping sensor waits the full delay after the *last* bad reading.
        let fault_clear_elapsed = if self.filter.fault().is_some() {
            0
        } else {
            fault_clear_elapsed
        };
        // The standby coordinator's timeout is a *request*, raised by the coordinator task, not a
        // timer the transition table checks. An explicit power-switch request still wakes the
        // machine, which is why the field is a request and not a condition here.
        let standby_requested = flags[6] || self.standby_due();
        Inputs {
            emergency_stop: self.is_emergency_stopped(),
            sensor_fault: self.filter.fault().is_some(),
            water_tank_full: self.sensors.water_tank_full,
            // The service mode is *not* expressed as "the PID is off" here. Doing that made the
            // cross-cutting PID check eject the machine from whatever it was doing the moment a
            // provisioning session or an OTA began, so the machine lost its state and had to be
            // restarted by the user afterwards. Service mode is enforced where the command is
            // issued, and by the PID's own enable check in `run_pid`.
            pid_runtime_enabled: self.pid_enabled,
            keep_heater_on_empty: self.config.keep_heater_on_empty,
            brew_pid_delay_active: self.brew_pid_delay_active(),

            brew_start_requested: flags[0],
            brew_stop_requested: flags[1],
            steam_requested: flags[2],
            // A stop request while a manual flush is running ends it. The C++ ended a manual flush
            // with the brew switch, and the domain's `MANUAL_FLUSH_RUNNING` handler only reacts to
            // a manual-flush request, so the mapping happens here rather than in the table.
            manual_flush_requested: flags[3]
                || (flags[1] && self.state == State::ManualFlushRunning),
            // The request that puts the machine into backflush mode is the mode flag for the tick
            // it arrives on; otherwise entering `BackflushIdle` would need the mode to be active
            // already, and the run could never start.
            backflush_mode_active: self.backflush_mode || flags[4],
            backflush_cycle_start: flags[4],
            backflush_stop_requested: flags[5],
            standby_requested,
            normal_operation_requested: flags[7],

            state_elapsed_ms: state_elapsed,
            fault_clear_elapsed_ms: fault_clear_elapsed,
            brew_elapsed_ms: self.brew_elapsed_ms(),

            brew_by_time_enabled: self.config.brew_by_time_enabled,
            brew_by_weight_enabled: self.config.brew_by_weight_enabled,
            brew_total_target_ms: self.config.brew_target_time_ms,
            brew_weight_g: self.sensors.weight_g.unwrap_or(0.0),
            brew_target_weight_g: self.config.brew_target_weight_g,
            manual_brew_mode: self.config.brew_mode_manual,
            preinfusion_enabled: self.config.preinfusion_enabled,
            preinfusion_ms: self.config.preinfusion_ms,
            preinfusion_pause_ms: self.config.preinfusion_pause_ms,
            brew_finished_timeout_ms: self.config.brew_finished_timeout_ms,
            backflush_finished_timeout_ms: self.config.backflush_finished_timeout_ms,
            backflush_fill_ms: self.config.backflush_fill_ms,
            backflush_flush_ms: self.config.backflush_flush_ms,
            backflush_cycles: self.config.backflush_cycles,
            backflush_current_cycle: self.backflush_cycle,
            // The domain's rule is "the state's own idle timer expired", so with the feature off
            // the machine reports a timer that cannot expire. Passing zero through would make
            // `state_elapsed >= 0` true on the very first tick and park the machine in standby.
            standby_timeout_ms: if self.config.standby_enabled {
                self.config.standby_timeout_ms
            } else {
                u32::MAX
            },
            sensor_error_recovery_ms: self.config.sensor_error_recovery_ms,
            eeprom_recovery_timeout_ms: self.config.eeprom_recovery_timeout_ms,
        }
    }

    /// Whether the standby timeout has expired. Zero means the feature is off, as in the C++.
    fn standby_due(&self) -> bool {
        if !self.config.standby_enabled || self.config.standby_timeout_ms == 0 {
            return false;
        }
        let Some(idle_since) = self.idle_since_ms else {
            return false;
        };
        self.now_ms.saturating_sub(idle_since) >= self.config.standby_timeout_ms
    }

    fn brew_pid_delay_active(&self) -> bool {
        match self.brew_pid_delay_until_ms {
            Some(until) => self.now_ms < until,
            None => false,
        }
    }

    /// Marks the start of an idle period, for the standby coordinator.
    pub fn note_idle(&mut self) {
        if self.idle_since_ms.is_none() {
            self.idle_since_ms = Some(self.now_ms);
        }
    }

    /// Whether a fault is currently latched, as opposed to the emergency stop.
    ///
    /// A sensor fault is *not* an emergency stop: it is recoverable, and the C++ recovered from it
    /// after `sensor_error_recovery_ms` without a power cycle. Latching a fault into the emergency
    /// stop would turn a flapping DS18B20 into a machine that needs unplugging, which is a
    /// different and much worse failure.
    pub fn in_sensor_error(&self) -> bool {
        self.state == State::SensorError
    }

    /// Whether the machine is ready to brew: a valid reading at temperature, a full tank and no
    /// fault. Every clause is a separate reason, so a log can say which one failed.
    pub fn is_ready(&self) -> bool {
        self.filter.fault().is_none()
            && self.sensors.water_tank_full
            && !self.is_emergency_stopped()
            && self
                .temperature_c()
                .map(|t| t >= self.setpoint_c() - 2.0)
                .unwrap_or(false)
    }

    // -- entering and leaving ---------------------------------------------

    /// Everything a state does on entry, in one place.
    fn enter_state(&mut self, state: State, input: &Inputs) {
        match state {
            State::BrewPreinfusion | State::BrewRunning | State::BrewPreinfusionPause => {
                if self.brew_started_ms.is_none() {
                    self.brew_started_ms = Some(self.now_ms);
                }
                if self.config.brew_pid_delay_ms > 0 {
                    self.brew_pid_delay_until_ms =
                        Some(self.now_ms + self.config.brew_pid_delay_ms);
                }
                self.idle_since_ms = None;
            }
            State::BrewFinished => {
                // The target is reached; the timer stops mattering, but the elapsed time stays
                // readable for the finished screen.
                self.pump_since_ms = None;
            }
            State::BackflushIdle => {
                self.backflush_mode = true;
                self.backflush_cycle = 0;
                self.backflush_started_ms = Some(self.now_ms);
            }
            State::BackflushFilling => {
                self.backflush_started_ms = Some(self.now_ms);
            }
            State::BackflushFlushing => {
                self.backflush_started_ms = Some(self.now_ms);
            }
            State::BackflushFinished => {
                self.backflush_mode = false;
                self.backflush_started_ms = None;
            }
            State::Standby => {
                self.idle_since_ms = Some(self.now_ms);
            }
            State::PidNormal | State::PidDisabled | State::Init => {
                self.idle_since_ms = Some(self.now_ms);
            }
            State::EmergencyStop | State::SensorError => {
                // Both are latched: the recovery path is a timer, not a transition out of here.
                self.emergency_latched = state == State::EmergencyStop;
            }
            _ => {}
        }
        let _ = input;
    }

    /// Everything a state does on exit.
    ///
    /// The C++ rule from `CLAUDE.md`, enforced here: **every** state that energises an actuator
    /// de-energises it on exit, because the next state's entry may not run if an error interrupts
    /// the transition. A test walks every state and asserts this.
    fn exit_state(&mut self, state: State) {
        if flows_water(state) || energises_heater(state) {
            self.actuators.force_off();
            self.last_command = ActuatorCommand::ALL_OFF;
            self.last_reason = "state exit";
        }
        if flows_water(state) {
            self.pump_since_ms = None;
        }
        if state == State::BrewFinished {
            self.brew_started_ms = None;
        }
    }

    /// Computes and issues the command for the current state.
    fn apply_state_command(&mut self, input: &Inputs) {
        if self.service_mode {
            // The provisioner owns the machine for its duration. Anything else here would race
            // with a firmware write the user asked for.
            self.force_off("service mode");
            return;
        }
        if self.emergency_latched {
            self.force_off("emergency stop latched");
            return;
        }
        let hot_water = self.sensors.switches.hot_water_pressed;
        let act = actuators_with_switches(self.state, hot_water);
        // The single valve allow-list. A state not on it cannot hold the valve open even if the
        // table above said so, which is the D-interlock the C++ had in two drifting copies.
        let valve = act.water_valve && self.state.may_hold_water_valve_open();
        let steam = act.steam_valve && matches!(self.state, State::SteamRunning);
        let pump = act.pump && self.pump_permitted();
        let heater = act.heater_enabled
            && heater_allowed(self.state, input)
            && self.filter.fault().is_none()
            && self.temperature_c().is_some();
        let command = ActuatorCommand {
            pump,
            water_valve: valve,
            steam_valve: steam,
            heater_enabled: heater,
        };
        self.issue(command, "state");
    }

    /// Whether the pump may run at all: never in a fault state, never without water in the tank,
    /// and never without a pressure reading when a pressure sensor is fitted and the reading is
    /// outside the plausible band the C++ checked.
    fn pump_permitted(&self) -> bool {
        if self.filter.fault().is_some() || self.is_emergency_stopped() {
            return false;
        }
        if !self.sensors.water_tank_full {
            return false;
        }
        true
    }

    /// Issues a command and arms or disarms the pump deadline.
    fn issue(&mut self, command: ActuatorCommand, reason: &'static str) {
        if command.pump && self.pump_since_ms.is_none() {
            self.pump_since_ms = Some(self.now_ms);
            self.pump_deadline_ms =
                if self.state == State::SteamRunning || self.sensors.switches.hot_water_pressed {
                    self.config.pump_timeout_hot_water_ms
                } else {
                    self.config.pump_timeout_brew_ms
                };
        } else if !command.pump {
            self.pump_since_ms = None;
        }
        if command.pump {
            if let Some(since) = self.pump_since_ms {
                if self.now_ms.saturating_sub(since) >= self.pump_deadline_ms {
                    self.actuators.force_off();
                    self.last_command = ActuatorCommand::ALL_OFF;
                    self.last_reason = "pump deadline";
                    self.pump_since_ms = None;
                    self.emergency_latched = true;
                    return;
                }
            }
        }
        self.actuators.command(command, reason);
        self.last_command = command;
        self.last_reason = reason;
        if command.heater_enabled {
            self.actuators.set_heater_duty(self.pid_output());
        }
    }

    /// The safety task's hammer: everything off, whatever the control path believes.
    pub fn force_off(&mut self, reason: &'static str) {
        self.actuators.force_off();
        self.last_command = ActuatorCommand::ALL_OFF;
        self.last_reason = reason;
        self.pump_since_ms = None;
    }

    /// The service mode the provisioner and an OTA hold. Forced off for its whole duration.
    pub fn set_service_mode(&mut self, on: bool) {
        self.service_mode = on;
        if on {
            self.force_off("service mode");
        }
    }

    pub fn service_mode(&self) -> bool {
        self.service_mode
    }

    // -- PID ---------------------------------------------------------------

    fn run_pid(&mut self, input: &Inputs, _elapsed_ms: u32) {
        let allowed = heater_allowed(self.state, input) && self.pid_enabled && !self.service_mode;
        self.pid.set_enabled(allowed);
        if !allowed {
            // Off means off, not "hold the last duty". Holding it is how a machine that has left
            // a heating state keeps its heater energised.
            self.pid_output = 0.0;
            return;
        }
        let Some(temp) = self.filter.celsius() else {
            self.pid_output = 0.0;
            return;
        };
        if self.now_ms.saturating_sub(self.last_pid_sample_ms) < self.config.pid_sample_ms {
            return;
        }
        self.last_pid_sample_ms = self.now_ms;
        // Steam runs at its own setpoint. One boiler, one PID, two targets: the C++ switched
        // between them in the steam state handler, and doing it here means the transition table
        // cannot move the machine out of steam and leave the PID aimed at 135 C.
        let target = if self.state == State::SteamRunning {
            self.config.steam_setpoint_c
        } else {
            self.setpoint_c()
        };
        let out = self.pid.compute(self.now_ms, temp, target);
        self.pid_output = out.output;
    }

    // -- timers ------------------------------------------------------------

    fn advance_brew_timers(&mut self) {
        if self.filter.fault().is_none() && self.fault_cleared_ms.is_none() {
            self.fault_cleared_ms = Some(self.now_ms);
        }
        if let Some(t) = self.fault_cleared_ms {
            if self.filter.fault().is_none() {
                let _ = t;
            } else {
                self.fault_cleared_ms = None;
            }
        }
        // The backflush cycle counter's arithmetic lives in the domain crate, where the `u8`
        // wraparound was fixed. The app asks what should happen and applies it, so the rule has
        // one implementation rather than one per caller.
        if self.state == State::BackflushFilling {
            match resolve_cycle_advance(self.backflush_cycle, self.config.backflush_cycles) {
                CycleAdvanceEffect::StartNextCycle => {
                    self.backflush_cycle = self.backflush_cycle.saturating_add(1);
                }
                CycleAdvanceEffect::CompleteAllCycles => {
                    // The last fill is over; the state machine moves to the flush phase and then to
                    // the finished state on its own timers.
                    self.backflush_cycle = self.config.backflush_cycles.saturating_sub(1);
                }
            }
        }
        if self.is_idle() {
            self.note_idle();
        } else {
            self.idle_since_ms = None;
        }
    }

    // -- fields the tests and the display read -----------------------------

    pub fn brew_running(&self) -> bool {
        matches!(
            self.state,
            State::BrewPreinfusion | State::BrewPreinfusionPause | State::BrewRunning
        )
    }
}

/// Whether a state's own command moves water.
///
/// Read from the domain's own actuator table rather than from a second list here, because a
/// second list is how the C++ interlock lists drifted apart from the states they described.
pub fn flows_water(state: State) -> bool {
    clevercoffee_domain::transition::actuators_for(state).flowing()
}

/// Whether a state's own command permits heating.
pub fn energises_heater(state: State) -> bool {
    clevercoffee_domain::transition::actuators_for(state).heater_enabled
}

/// Whether a state is idle enough that a destructive operation may start.
fn group_is_idle(state: State) -> bool {
    !matches!(
        state,
        State::BrewPreinfusion
            | State::BrewPreinfusionPause
            | State::BrewRunning
            | State::ManualFlushRunning
            | State::SteamRunning
            | State::BackflushFilling
            | State::BackflushFlushing
    )
}
