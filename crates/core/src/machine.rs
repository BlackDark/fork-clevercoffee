//! The espresso machine: switch handling, state machine, temperature control and
//! actuator outputs, advanced by one [`Machine::tick`] every few milliseconds.
//!
//! The behaviour follows the C++ firmware (v1.3.4); every deliberate difference is
//! listed in `docs/decisions/0004-behaviour-changes.md`. The structure differs:
//! - outputs are computed from the state each tick ([`Machine::outputs`]) instead of
//!   being switched on and off by state entry/exit code, so no path can leave the pump
//!   running or the valve open after the state that wanted it is gone;
//! - requests (brew start, steam stop, ...) live for one tick only, so a press that
//!   could not act is dropped instead of firing much later.

use cc_pid::{Pid, Proportional, Tunings};
use heapless::Vec;

use crate::config::{BrewMode, Config, SwitchType};
use crate::scale::{Scale, ScaleAction, ScaleReading};
use crate::state::State;
use crate::switch::Switch;
use crate::timers::{BrewTimer, Standby};

/// Control period the firmware runs [`Machine::tick`] at.
pub const TICK_MS: u64 = 10;
/// Heater time-proportioning window; PID output is in promille of it.
pub const HEATER_WINDOW_MS: u64 = 1000;
/// PID sample time (one heater window).
pub const PID_SAMPLE_MS: u32 = 1000;
/// No valid temperature for this long is a sensor error.
pub const TEMP_TIMEOUT_MS: u64 = 2_000;
/// A recovered sensor must stay good this long before heating resumes.
pub const SENSOR_RECOVERY_MS: u64 = 5_000;
/// `BrewFinished` and `BackflushFinished` are shown this long.
pub const FINISHED_DISPLAY_MS: u64 = 3_000;
/// Safety cap on one shot; the C++ declared it but never started the timer.
pub const MAX_BREW_MS: u64 = 5 * 60 * 1000;
/// Safety cap on hot-water dispensing (same: declared, never enforced in C++).
pub const MAX_HOT_WATER_MS: u64 = 60 * 1000;
/// Emergency stop clears only at or below this temperature.
pub const EMERGENCY_SAFE_C: f64 = 100.0;
/// Readings outside this range are physically implausible: emergency stop.
pub const VALID_TEMP_RANGE_C: (f64, f64) = (0.0, 200.0);
/// Consecutive readings above the emergency temperature before tripping.
pub const EMERGENCY_DEBOUNCE: u8 = 3;
/// Below setpoint by more than this shows the heating screen.
pub const HEATING_LOGO_DELTA_C: f64 = 5.0;
/// Status-LED tolerance while steaming.
pub const STEAM_LED_TOLERANCE_C: f64 = 5.0;
/// Shots shorter than this (and lighter than the weight below) are not counted.
pub const MIN_COUNTED_SHOT_MS: f64 = 5_000.0;
pub const MIN_COUNTED_SHOT_G: f32 = 10.0;
/// Power-switch long press reboots, but not in the first seconds after boot.
pub const POWER_REBOOT_HOLD_MS: u64 = 1_000;
pub const POWER_REBOOT_ARM_AFTER_BOOT_MS: u64 = 5_000;

/// Hardware readings for one tick.
#[derive(Debug, Clone, Copy, Default)]
pub struct Inputs {
    pub now_ms: u64,
    /// A new temperature reading since the last tick, °C.
    pub temperature: Option<f64>,
    /// The driver reports a hard fault (no sensor, bus error).
    pub temperature_fault: bool,
    /// Raw pin levels (`true` = high). Ignored when the switch is disabled.
    pub brew_switch: bool,
    pub steam_switch: bool,
    pub hot_water_switch: bool,
    pub power_switch: bool,
    pub water_tank: bool,
    /// A new scale reading since the last tick.
    pub scale: Option<ScaleReading>,
    /// A new pressure reading (bar) since the last tick.
    pub pressure: Option<f32>,
    /// A firmware update is being written: all actuators off.
    pub ota_active: bool,
}

/// Logical actuator states. The firmware maps them to pin levels (relay trigger
/// type, LED inversion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outputs {
    pub heater: bool,
    pub pump: bool,
    pub valve: bool,
    pub status_led: bool,
    pub brew_led: bool,
    pub steam_led: bool,
}

/// Side effects the firmware performs after a tick or command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// `Config` changed and should be persisted.
    ConfigChanged,
    /// The shot counter changed and should be persisted.
    ShotCounterChanged,
    StateChanged {
        from: State,
        to: State,
    },
    Scale(ScaleAction),
    /// Power switch long press.
    Reboot,
}

pub type Events = Vec<Event, 8>;

/// User commands from the web UI and MQTT.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    SetPid(bool),
    TogglePid,
    SetSteam(bool),
    ToggleSteam,
    SetBackflush(bool),
    ToggleBackflush,
    SetBrewSetpoint(f64),
    Wake,
    Sleep,
    ResetShotCounter,
    ToggleScaleTare,
    ToggleScaleCalibration,
    /// Parameters were written through the registry.
    ConfigUpdated,
}

/// Outcome of a [`Command`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reply {
    Ok,
    Pid(bool),
    Steam(bool),
    Backflush(bool),
    ScaleTare(bool),
    ScaleCalibration(bool),
    Rejected(&'static str),
}

#[derive(Debug, Clone, Copy, Default)]
struct Requests {
    brew_start: bool,
    brew_stop: bool,
    steam_start: bool,
    steam_stop: bool,
    flush_start: bool,
    flush_stop: bool,
    backflush_cycle_start: bool,
    backflush_stop: bool,
    normal_operation: bool,
}

#[derive(Debug, Clone, Default)]
struct Emergency {
    active: bool,
    over_count: u8,
}

impl Emergency {
    fn on_reading(&mut self, t: f64, cfg: &Config) {
        let (lo, hi) = VALID_TEMP_RANGE_C;
        if t < lo || t > hi {
            self.active = true;
        } else if t > cfg.safety_emergency_temp {
            self.over_count = self.over_count.saturating_add(1);
            if self.over_count >= EMERGENCY_DEBOUNCE {
                self.active = true;
            }
        } else if t < cfg.safety_emergency_temp - cfg.safety_emergency_hysteresis {
            self.over_count = 0;
            if self.active && t <= EMERGENCY_SAFE_C {
                self.active = false;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Switches {
    brew: Option<Switch>,
    steam: Option<Switch>,
    hot_water: Option<Switch>,
    power: Option<Switch>,
    tank: Option<Switch>,
    brew_was: bool,
    steam_was: bool,
    hot_water_was: bool,
    power_was: bool,
    /// A momentary press in backflush idle: a short press starts a cycle, a long
    /// one a manual flush.
    backflush_press_pending: bool,
    power_press_ms: Option<u64>,
}

/// The snapshot readers (web, MQTT, display) see.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Status {
    pub now_ms: u64,
    pub state: State,
    /// Shown temperature (sensor minus offset, as the PID sees it).
    pub temperature: f64,
    pub temperature_valid: bool,
    pub setpoint: f64,
    /// Heater duty, 0..=100 %.
    pub heater_power: f64,
    pub pid_runtime: bool,
    pub tunings: Tunings,
    pub emergency: bool,
    pub backflush_mode: bool,
    pub backflush_cycle: i32,
    pub brew_time_ms: f64,
    pub brew_target_ms: f64,
    pub brew_timer_visible: bool,
    pub post_brew: bool,
    pub hot_water_ms: u64,
    pub weight: Option<f32>,
    pub brew_weight: f32,
    pub scale_fault: bool,
    pub scale_calibration: crate::scale::Calibration,
    pub pressure: Option<f32>,
    pub water_tank_full: bool,
    pub standby_remaining_ms: u64,
    pub display_off: bool,
    pub shots_since_backflush: u32,
    pub outputs: Outputs,
}

impl Status {
    /// Blinking elements are visible in the first half of each heater window.
    pub fn blink_on(&self) -> bool {
        self.now_ms % HEATER_WINDOW_MS < HEATER_WINDOW_MS / 2
    }

    pub fn near_setpoint(&self, delta: f64) -> bool {
        (self.temperature - self.setpoint).abs() < delta
    }

    /// The heating screen is due: well below the brew setpoint while idle.
    pub fn heating_logo_due(&self, cfg: &Config) -> bool {
        cfg.display_heating_logo
            && self.state == State::PidNormal
            && self.setpoint - self.temperature > HEATING_LOGO_DELTA_C
    }

    pub fn backflush_reminder_due(&self, cfg: &Config) -> bool {
        cfg.maintenance_backflush_reminder_enabled
            && self.shots_since_backflush >= cfg.maintenance_backflush_reminder_threshold as u32
    }
    /// Nothing the user started is running: no shot (nor its result on the display),
    /// flush, backflush cycle, steam or hot water. A restart the user did not ask for
    /// (the WiFi fallback ending) waits for this.
    pub fn idle_for_restart(&self) -> bool {
        !self.state.is_brew()
            && !matches!(
                self.state,
                State::ManualFlush
                    | State::Steam
                    | State::BackflushFilling
                    | State::BackflushFlushing
                    | State::BackflushFinished
            )
            && !self.outputs.pump
            && !self.outputs.valve
    }
}

/// The machine. Owns all control state; the firmware owns the hardware.
#[derive(Debug, Clone)]
pub struct Machine {
    state: State,
    state_since_ms: u64,
    now_ms: u64,
    boot_ms: u64,
    req: Requests,
    standby_requested: bool,
    pid_runtime: bool,
    backflush_mode: bool,
    backflush_cycle: i32,
    switches: Switches,
    emergency: Emergency,
    raw_temp: Option<f64>,
    temp_at_ms: Option<u64>,
    temp_fault: bool,
    temp_good_since_ms: Option<u64>,
    pid: Pid,
    pid_input: f64,
    setpoint: f64,
    brew_delay_active: bool,
    brew_time_ms: f64,
    brew_target_ms: f64,
    brew_started_ms: Option<u64>,
    hot_water_since_ms: Option<u64>,
    hot_water_ms: u64,
    hot_water_timed_out: bool,
    pressure_filtered: Option<f32>,
    water_tank_full: bool,
    scale: Scale,
    standby: Standby,
    brew_timer: BrewTimer,
    shots_since_backflush: u32,
    outputs: Outputs,
}

impl Machine {
    /// Builds the machine. Switch types and wiring are read here only (they need a
    /// reboot to change, like the GPIO setup they mirror).
    pub fn new(cfg: &Config, shots_since_backflush: u32, now_ms: u64) -> Self {
        let mk = |enabled: bool, kind, mode| enabled.then(|| Switch::new(kind, mode));
        let switches = Switches {
            brew: mk(
                cfg.hardware_switches_brew_enabled,
                cfg.hardware_switches_brew_type,
                cfg.hardware_switches_brew_mode,
            ),
            steam: mk(
                cfg.hardware_switches_steam_enabled,
                cfg.hardware_switches_steam_type,
                cfg.hardware_switches_steam_mode,
            ),
            hot_water: mk(
                cfg.hardware_switches_hot_water_enabled,
                cfg.hardware_switches_hot_water_type,
                cfg.hardware_switches_hot_water_mode,
            ),
            power: mk(
                cfg.hardware_switches_power_enabled,
                cfg.hardware_switches_power_type,
                cfg.hardware_switches_power_mode,
            ),
            tank: mk(
                cfg.hardware_sensors_watertank_enabled,
                SwitchType::Toggle,
                cfg.hardware_sensors_watertank_mode,
            ),
            brew_was: false,
            steam_was: false,
            hot_water_was: false,
            power_was: false,
            backflush_press_pending: false,
            power_press_ms: None,
        };
        let mut pid = Pid::new(
            Self::regular_tunings(cfg),
            PID_SAMPLE_MS,
            0.0,
            HEATER_WINDOW_MS as f64,
        );
        pid.set_integrator_limits(0.0, cfg.pid_regular_i_max);
        pid.set_smoothing(cfg.pid_ema_factor);
        Self {
            state: State::Init,
            state_since_ms: now_ms,
            now_ms,
            boot_ms: now_ms,
            req: Requests::default(),
            standby_requested: false,
            pid_runtime: cfg.pid_enabled,
            backflush_mode: false,
            backflush_cycle: 1,
            switches,
            emergency: Emergency::default(),
            raw_temp: None,
            temp_at_ms: None,
            temp_fault: false,
            temp_good_since_ms: None,
            pid,
            pid_input: 0.0,
            setpoint: cfg.brew_setpoint,
            brew_delay_active: false,
            brew_time_ms: 0.0,
            brew_target_ms: 0.0,
            brew_started_ms: None,
            hot_water_since_ms: None,
            hot_water_ms: 0,
            hot_water_timed_out: false,
            pressure_filtered: None,
            water_tank_full: true,
            scale: Scale::new(),
            standby: Standby::default(),
            brew_timer: BrewTimer::Idle,
            shots_since_backflush,
            outputs: Outputs::default(),
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn outputs(&self) -> Outputs {
        self.outputs
    }

    /// Advances the machine by one tick.
    pub fn tick(&mut self, cfg: &mut Config, inp: &Inputs) -> Events {
        let mut ev = Events::new();
        let now = inp.now_ms;
        self.now_ms = now;

        self.read_sensors(cfg, inp, &mut ev);
        self.handle_switches(cfg, inp, &mut ev);
        self.standby.update(cfg, now);
        self.update_state_data(cfg);
        if let Some(next) = self.next_state(cfg) {
            self.transition(next, cfg, &mut ev);
        }
        self.req = Requests::default();

        if let Some(a) = self.scale.track_brew(self.state.is_brew_active(), cfg, now) {
            let _ = ev.push(Event::Scale(a));
        }
        self.brew_timer.update(self.state.is_brew_active(), cfg, now);
        self.control_temperature(cfg);
        self.outputs = self.compute_outputs(cfg, inp);
        ev
    }

    /// Applies a user command. Commands that change `cfg` push [`Event::ConfigChanged`].
    pub fn command(&mut self, cmd: Command, cfg: &mut Config, ev: &mut Events) -> Reply {
        let now = self.now_ms;
        match cmd {
            Command::SetPid(on) => {
                if cfg.pid_enabled != on {
                    cfg.pid_enabled = on;
                    let _ = ev.push(Event::ConfigChanged);
                }
                self.pid_runtime = on;
                self.activity(cfg);
                Reply::Pid(on)
            }
            Command::TogglePid => self.command(Command::SetPid(!cfg.pid_enabled), cfg, ev),
            Command::SetSteam(on) => {
                if on {
                    self.req.steam_start = true;
                } else {
                    self.req.steam_stop = true;
                }
                self.activity(cfg);
                Reply::Steam(on)
            }
            Command::ToggleSteam => self.command(Command::SetSteam(self.state != State::Steam), cfg, ev),
            Command::SetBackflush(on) => {
                if on && cfg.backflush_cycles <= 0 {
                    return Reply::Rejected("backflush.cycles must be greater than 0");
                }
                if on != self.backflush_mode {
                    self.backflush_mode = on;
                    self.backflush_cycle = 1;
                    self.activity(cfg);
                }
                Reply::Backflush(self.backflush_mode)
            }
            Command::ToggleBackflush => self.command(Command::SetBackflush(!self.backflush_mode), cfg, ev),
            Command::SetBrewSetpoint(t) => {
                if cfg.set_from_str_value_brew_setpoint(t) {
                    let _ = ev.push(Event::ConfigChanged);
                    self.activity(cfg);
                    Reply::Ok
                } else {
                    Reply::Rejected("Invalid setpoint value")
                }
            }
            Command::Wake | Command::ConfigUpdated => {
                self.activity(cfg);
                Reply::Ok
            }
            Command::Sleep => {
                self.standby_requested = true;
                Reply::Ok
            }
            Command::ResetShotCounter => {
                if self.shots_since_backflush != 0 {
                    self.shots_since_backflush = 0;
                    let _ = ev.push(Event::ShotCounterChanged);
                }
                self.standby.reset(cfg, now);
                Reply::Ok
            }
            Command::ToggleScaleTare => {
                if let Some(a) = self.scale.tare(cfg) {
                    let _ = ev.push(Event::Scale(a));
                }
                self.activity(cfg);
                Reply::ScaleTare(true)
            }
            Command::ToggleScaleCalibration => {
                self.activity(cfg);
                if matches!(
                    self.scale.calibration(),
                    crate::scale::Calibration::Taring { .. } | crate::scale::Calibration::Placing { .. }
                ) {
                    self.scale.cancel_calibration();
                    Reply::ScaleCalibration(false)
                } else {
                    Reply::ScaleCalibration(self.scale.start_calibration(cfg, now))
                }
            }
        }
    }

    pub fn status(&self, cfg: &Config) -> Status {
        Status {
            now_ms: self.now_ms,
            state: self.state,
            temperature: self.pid_input,
            temperature_valid: !self.temp_fault && self.raw_temp.is_some(),
            setpoint: self.setpoint,
            heater_power: self.pid.output() / 10.0,
            pid_runtime: self.pid_runtime,
            tunings: self.pid.tunings(),
            emergency: self.emergency.active,
            backflush_mode: self.backflush_mode,
            backflush_cycle: self.backflush_cycle,
            brew_time_ms: self.brew_time_ms,
            brew_target_ms: self.brew_target_ms,
            brew_timer_visible: self.brew_timer.visible(),
            post_brew: matches!(self.brew_timer, BrewTimer::PostBrew { .. }),
            hot_water_ms: self.hot_water_ms,
            weight: self.scale.weight(),
            brew_weight: self.scale.brew_weight(),
            scale_fault: cfg.hardware_sensors_scale_enabled && self.scale.is_faulty(self.now_ms),
            scale_calibration: self.scale.calibration(),
            pressure: self.pressure_filtered,
            water_tank_full: self.water_tank_full,
            standby_remaining_ms: self.standby.remaining_ms(),
            display_off: self.standby.display_off(),
            shots_since_backflush: self.shots_since_backflush,
            outputs: self.outputs,
        }
    }

    // --- sensors -------------------------------------------------------------

    fn read_sensors(&mut self, cfg: &mut Config, inp: &Inputs, ev: &mut Events) {
        let now = inp.now_ms;
        if let Some(t) = inp.temperature {
            self.raw_temp = Some(t);
            self.temp_at_ms = Some(now);
            self.emergency.on_reading(t, cfg);
        }
        let stale = now.saturating_sub(self.temp_at_ms.unwrap_or(self.boot_ms)) > TEMP_TIMEOUT_MS;
        self.temp_fault = inp.temperature_fault || stale;
        if self.temp_fault {
            self.temp_good_since_ms = None;
        } else if self.temp_good_since_ms.is_none() && self.raw_temp.is_some() {
            self.temp_good_since_ms = Some(now);
        }

        self.water_tank_full = match &mut self.switches.tank {
            // Until the first reading settles the tank counts as full (as at boot):
            // otherwise every start passed through WATER_TANK_EMPTY.
            Some(s) => {
                let full = s.update(inp.water_tank, now);
                if s.is_settled() {
                    full
                } else {
                    self.water_tank_full
                }
            }
            None => true,
        };

        if let Some(r) = inp.scale
            && cfg.hardware_sensors_scale_enabled
            && self.scale.on_reading(r, cfg, now)
        {
            let _ = ev.push(Event::ConfigChanged);
        }
        if let Some(p) = inp.pressure {
            // The C++ filter: 30 % new, 70 % previous.
            self.pressure_filtered = Some(p * 0.3 + self.pressure_filtered.unwrap_or(0.0) * 0.7);
        }
    }

    // --- switches ------------------------------------------------------------

    fn activity(&mut self, cfg: &Config) {
        self.standby.reset(cfg, self.now_ms);
        self.req.normal_operation = true;
    }

    fn handle_switches(&mut self, cfg: &mut Config, inp: &Inputs, ev: &mut Events) {
        let now = inp.now_ms;
        let tank_ok = self.state != State::WaterTankEmpty;

        if let Some(sw) = &mut self.switches.steam {
            let pressed = sw.update(inp.steam_switch, now);
            if pressed != self.switches.steam_was {
                self.switches.steam_was = pressed;
                let kind = sw.kind();
                self.on_steam_edge(pressed, kind, cfg);
            }
        }

        if let Some(sw) = &mut self.switches.power {
            let pressed = sw.update(inp.power_switch, now);
            let long = sw.is_long_press(now);
            let kind = sw.kind();
            if pressed != self.switches.power_was {
                self.switches.power_was = pressed;
                self.on_power_edge(pressed, kind, cfg, ev);
            }
            if pressed
                && long
                && let Some(t) = self.switches.power_press_ms
                && now.saturating_sub(t) > POWER_REBOOT_HOLD_MS
            {
                self.switches.power_press_ms = None;
                let _ = ev.push(Event::Reboot);
            }
        }

        if let Some(sw) = &mut self.switches.hot_water {
            let pressed = sw.update(inp.hot_water_switch, now);
            if pressed != self.switches.hot_water_was {
                self.switches.hot_water_was = pressed;
                self.activity(cfg);
            }
        }

        if let Some(sw) = &mut self.switches.brew {
            let pressed = sw.update(inp.brew_switch, now);
            let long = sw.is_long_press(now);
            let kind = sw.kind();
            if pressed != self.switches.brew_was {
                self.switches.brew_was = pressed;
                if tank_ok {
                    self.on_brew_edge(pressed, kind, cfg);
                }
            } else if pressed && long && self.switches.backflush_press_pending {
                self.switches.backflush_press_pending = false;
                self.req.flush_start = true;
            }
        }

        if let Some(started) = self.brew_started_ms
            && self.state.is_brew_active()
            && now.saturating_sub(started) > MAX_BREW_MS
        {
            self.req.brew_stop = true;
        }
    }

    fn on_brew_edge(&mut self, pressed: bool, kind: SwitchType, cfg: &Config) {
        let s = self.state;
        self.activity(cfg);
        if self.backflush_mode || s.is_backflush() || s == State::ManualFlush {
            if pressed {
                if kind == SwitchType::Momentary && s == State::BackflushIdle {
                    self.switches.backflush_press_pending = true;
                } else if matches!(s, State::BackflushIdle | State::BackflushFinished) {
                    self.req.backflush_cycle_start = true;
                } else if s.is_backflush() {
                    self.req.backflush_stop = true;
                }
            } else if s == State::ManualFlush {
                self.req.flush_stop = true;
            } else if self.switches.backflush_press_pending {
                self.switches.backflush_press_pending = false;
                self.req.backflush_cycle_start = true;
            } else if kind == SwitchType::Toggle
                && matches!(s, State::BackflushFilling | State::BackflushFlushing)
            {
                self.req.backflush_stop = true;
            }
            return;
        }
        match (pressed, kind) {
            (true, SwitchType::Momentary) => {
                if s.is_brew_active() {
                    self.req.brew_stop = true;
                } else {
                    self.req.brew_start = true;
                }
            }
            (true, SwitchType::Toggle) if !s.is_brew_active() => self.req.brew_start = true,
            (false, SwitchType::Toggle) if s.is_brew_active() => self.req.brew_stop = true,
            _ => {}
        }
    }

    fn on_steam_edge(&mut self, pressed: bool, kind: SwitchType, cfg: &Config) {
        self.activity(cfg);
        let steaming = self.state == State::Steam;
        match (pressed, kind) {
            (true, SwitchType::Momentary) => {
                if steaming {
                    self.req.steam_stop = true;
                } else {
                    self.req.steam_start = true;
                }
            }
            (true, SwitchType::Toggle) if !steaming => self.req.steam_start = true,
            (false, SwitchType::Toggle) if steaming => self.req.steam_stop = true,
            _ => {}
        }
    }

    fn on_power_edge(&mut self, pressed: bool, kind: SwitchType, cfg: &mut Config, ev: &mut Events) {
        let now = self.now_ms;
        let on = match kind {
            SwitchType::Toggle => pressed,
            SwitchType::Momentary => {
                if !pressed {
                    self.switches.power_press_ms = None;
                    return;
                }
                if now.saturating_sub(self.boot_ms) > POWER_REBOOT_ARM_AFTER_BOOT_MS {
                    self.switches.power_press_ms = Some(now);
                }
                self.state == State::Standby
            }
        };
        if on {
            if matches!(self.state, State::Standby | State::PidDisabled) {
                self.req.normal_operation = true;
                self.standby_requested = false;
                self.standby.reset(cfg, now);
                if !cfg.pid_enabled {
                    cfg.pid_enabled = true;
                    let _ = ev.push(Event::ConfigChanged);
                }
                self.pid_runtime = true;
            }
        } else if self.state != State::Standby {
            self.standby_requested = true;
            self.standby.expire();
        }
    }

    // --- state machine ----------------------------------------------------------

    fn elapsed(&self) -> u64 {
        self.now_ms.saturating_sub(self.state_since_ms)
    }

    fn pid_state(cfg: &Config) -> State {
        if cfg.pid_enabled {
            State::PidNormal
        } else {
            State::PidDisabled
        }
    }

    fn preinfusion_base_ms(cfg: &Config) -> f64 {
        if cfg.brew_pre_infusion_enabled {
            (cfg.brew_pre_infusion_time + cfg.brew_pre_infusion_pause.max(0.0)) * 1000.0
        } else {
            0.0
        }
    }

    fn target_brew_ms(cfg: &Config) -> f64 {
        if cfg.brew_mode == BrewMode::Automatic && cfg.brew_by_time_enabled {
            Self::preinfusion_base_ms(cfg) + cfg.brew_by_time_target_time * 1000.0
        } else {
            0.0
        }
    }

    /// Per-state bookkeeping that runs every tick (the C++ `update()` methods).
    fn update_state_data(&mut self, cfg: &Config) {
        let elapsed = self.elapsed() as f64;
        let automatic = cfg.brew_mode == BrewMode::Automatic;
        match self.state {
            State::BrewPreinfusion => self.brew_time_ms = elapsed,
            State::BrewPreinfusionPause => self.brew_time_ms = cfg.brew_pre_infusion_time * 1000.0 + elapsed,
            State::BrewRunning => {
                let base = if automatic {
                    Self::preinfusion_base_ms(cfg)
                } else {
                    0.0
                };
                self.brew_time_ms = base + elapsed;
            }
            State::ManualFlush => self.brew_time_ms = elapsed,
            State::PidDisabled => self.pid_runtime = false,
            _ => {}
        }

        let pumping_allowed = matches!(self.state, State::PidNormal | State::Steam) && self.water_tank_full;
        if self.switches.hot_water_was && pumping_allowed && !self.hot_water_timed_out {
            let since = *self.hot_water_since_ms.get_or_insert(self.now_ms);
            self.hot_water_ms = self.now_ms.saturating_sub(since);
            if self.hot_water_ms > MAX_HOT_WATER_MS {
                self.hot_water_timed_out = true;
            }
        } else {
            self.hot_water_since_ms = None;
            self.hot_water_ms = 0;
            if !self.switches.hot_water_was {
                self.hot_water_timed_out = false;
            }
        }
    }

    /// The transition this tick, if any: global safety checks first, then the
    /// state's own rules.
    fn next_state(&mut self, cfg: &Config) -> Option<State> {
        let s = self.state;
        if self.emergency.active && s != State::EmergencyStop {
            return Some(State::EmergencyStop);
        }
        if self.temp_fault && !matches!(s, State::SensorError | State::EmergencyStop) {
            return Some(State::SensorError);
        }
        if !self.water_tank_full
            && !matches!(
                s,
                State::WaterTankEmpty | State::Standby | State::EmergencyStop | State::SensorError
            )
        {
            return Some(State::WaterTankEmpty);
        }
        if s.needs_pid() && !self.pid_runtime {
            return Some(State::PidDisabled);
        }
        let r = self.req;
        let pid_state = Self::pid_state(cfg);
        let elapsed = self.elapsed();
        let automatic = cfg.brew_mode == BrewMode::Automatic;

        let backflush_mode = self.backflush_mode;
        let backflush_off = || (!backflush_mode).then_some(pid_state);
        match s {
            State::Init => self.raw_temp.is_some().then_some(if self.pid_runtime {
                State::PidNormal
            } else {
                State::PidDisabled
            }),
            State::PidNormal => {
                if !self.pid_runtime {
                    Some(State::PidDisabled)
                } else if r.brew_start {
                    Some(if automatic {
                        State::BrewPreinfusion
                    } else {
                        State::BrewRunning
                    })
                } else if r.steam_start {
                    Some(State::Steam)
                } else if r.flush_start {
                    Some(State::ManualFlush)
                } else if self.backflush_mode {
                    Some(State::BackflushIdle)
                } else if self.standby_requested {
                    Some(State::Standby)
                } else {
                    self.standby.arm(cfg, self.now_ms);
                    self.standby.should_enter(cfg).then_some(State::Standby)
                }
            }
            State::PidDisabled => {
                if self.pid_runtime {
                    Some(State::PidNormal)
                } else if self.standby_requested {
                    Some(State::Standby)
                } else {
                    self.standby.arm(cfg, self.now_ms);
                    self.standby.should_enter(cfg).then_some(State::Standby)
                }
            }
            State::BrewPreinfusion => {
                if r.brew_stop {
                    Some(pid_state)
                } else if !automatic || !cfg.brew_pre_infusion_enabled {
                    Some(State::BrewRunning)
                } else if elapsed >= (cfg.brew_pre_infusion_time * 1000.0) as u64 {
                    Some(if cfg.brew_pre_infusion_pause > 0.0 {
                        State::BrewPreinfusionPause
                    } else {
                        State::BrewRunning
                    })
                } else {
                    None
                }
            }
            State::BrewPreinfusionPause => {
                if r.brew_stop {
                    Some(pid_state)
                } else if !automatic || elapsed >= (cfg.brew_pre_infusion_pause * 1000.0) as u64 {
                    Some(State::BrewRunning)
                } else {
                    None
                }
            }
            State::BrewRunning => {
                let by_time = automatic
                    && cfg.brew_by_time_enabled
                    && self.brew_target_ms > 0.0
                    && self.brew_time_ms >= self.brew_target_ms;
                let target_g = cfg.brew_by_weight_target_weight as f32;
                let by_weight = automatic
                    && cfg.brew_by_weight_enabled
                    && cfg.hardware_sensors_scale_enabled
                    && !self.scale.is_faulty(self.now_ms)
                    && target_g > 0.0
                    && self.scale.brew_weight() >= target_g;
                (r.brew_stop || by_time || by_weight).then_some(State::BrewFinished)
            }
            State::BrewFinished => {
                if r.brew_start {
                    Some(State::BrewPreinfusion)
                } else {
                    (elapsed >= FINISHED_DISPLAY_MS).then_some(pid_state)
                }
            }
            State::ManualFlush => r.flush_stop.then_some(if self.backflush_mode {
                State::BackflushIdle
            } else {
                pid_state
            }),
            State::Steam => r.steam_stop.then_some(pid_state),
            State::BackflushIdle => backflush_off().or_else(|| {
                if r.flush_start {
                    Some(State::ManualFlush)
                } else {
                    r.backflush_cycle_start.then_some(State::BackflushFilling)
                }
            }),
            State::BackflushFilling => backflush_off().or_else(|| {
                if r.backflush_stop {
                    Some(State::BackflushIdle)
                } else {
                    (elapsed >= (cfg.backflush_fill_time * 1000.0) as u64).then_some(State::BackflushFlushing)
                }
            }),
            State::BackflushFlushing => backflush_off().or({
                if r.backflush_stop {
                    Some(State::BackflushIdle)
                } else if elapsed >= (cfg.backflush_flush_time * 1000.0) as u64 {
                    Some(if self.backflush_cycle < cfg.backflush_cycles {
                        State::BackflushFilling
                    } else {
                        State::BackflushFinished
                    })
                } else {
                    None
                }
            }),
            State::BackflushFinished => backflush_off().or_else(|| {
                if r.backflush_stop {
                    Some(State::BackflushIdle)
                } else if r.backflush_cycle_start {
                    Some(State::BackflushFilling)
                } else {
                    (elapsed >= FINISHED_DISPLAY_MS).then_some(State::BackflushIdle)
                }
            }),
            State::WaterTankEmpty => {
                if self.water_tank_full {
                    Some(pid_state)
                } else if self.standby_requested {
                    Some(State::Standby)
                } else {
                    self.standby.arm(cfg, self.now_ms);
                    self.standby.should_enter(cfg).then_some(State::Standby)
                }
            }
            State::EmergencyStop => {
                let cleared = self
                    .raw_temp
                    .is_some_and(|t| t >= VALID_TEMP_RANGE_C.0 && t <= EMERGENCY_SAFE_C)
                    && !self.emergency.active;
                cleared.then_some(State::Init)
            }
            State::Standby => {
                let wake = r.normal_operation || r.brew_start || r.steam_start;
                wake.then_some(pid_state)
            }
            State::SensorError => {
                let recovered = self
                    .temp_good_since_ms
                    .is_some_and(|t| self.now_ms.saturating_sub(t) > SENSOR_RECOVERY_MS);
                recovered.then_some(pid_state)
            }
        }
    }

    fn transition(&mut self, to: State, cfg: &Config, ev: &mut Events) {
        let from = self.state;
        if from == to {
            return;
        }
        // Exit actions.
        match from {
            State::Standby | State::EmergencyStop => self.pid_runtime = cfg.pid_enabled,
            _ => {}
        }
        // Entry actions.
        match to {
            State::BrewPreinfusion => {
                self.brew_time_ms = 0.0;
                self.brew_target_ms = Self::target_brew_ms(cfg);
                self.brew_started_ms = Some(self.now_ms);
            }
            State::BrewPreinfusionPause => self.brew_time_ms = cfg.brew_pre_infusion_time * 1000.0,
            State::BrewRunning => {
                let automatic = cfg.brew_mode == BrewMode::Automatic;
                self.brew_time_ms = if automatic {
                    Self::preinfusion_base_ms(cfg)
                } else {
                    0.0
                };
                self.brew_target_ms = Self::target_brew_ms(cfg);
                if !from.is_brew_active() {
                    self.brew_started_ms = Some(self.now_ms);
                }
            }
            State::BrewFinished => {
                self.brew_started_ms = None;
                let weight_counts =
                    cfg.hardware_sensors_scale_enabled && self.scale.brew_weight() >= MIN_COUNTED_SHOT_G;
                if self.brew_time_ms >= MIN_COUNTED_SHOT_MS || weight_counts {
                    self.shots_since_backflush = self.shots_since_backflush.saturating_add(1);
                    let _ = ev.push(Event::ShotCounterChanged);
                }
            }
            State::ManualFlush => self.brew_time_ms = 0.0,
            State::BackflushFilling if from == State::BackflushFlushing => self.backflush_cycle += 1,
            State::BackflushIdle => self.backflush_cycle = 1,
            State::BackflushFinished => {
                self.backflush_cycle = 1;
                if self.shots_since_backflush != 0 {
                    self.shots_since_backflush = 0;
                    let _ = ev.push(Event::ShotCounterChanged);
                }
            }
            State::PidDisabled => self.pid_runtime = false,
            State::Standby => {
                self.pid_runtime = false;
                self.standby_requested = false;
            }
            State::EmergencyStop => self.pid_runtime = false,
            _ => {}
        }
        if !to.is_brew_active() && to != State::BrewFinished {
            self.brew_started_ms = None;
        }
        self.state = to;
        self.state_since_ms = self.now_ms;
        let _ = ev.push(Event::StateChanged { from, to });
    }

    // --- temperature control ------------------------------------------------------

    fn regular_tunings(cfg: &Config) -> Tunings {
        let p = if cfg.pid_use_ponm {
            Proportional::OnMeasurement
        } else {
            Proportional::OnError
        };
        Tunings::from_kp_tn_tv(cfg.pid_regular_kp, cfg.pid_regular_tn, cfg.pid_regular_tv, p)
    }

    fn tunings_for(&self, cfg: &Config) -> Tunings {
        match self.state {
            State::Steam => Tunings {
                kp: cfg.pid_steam_kp,
                ki: 0.0,
                kd: 0.0,
                proportional: Proportional::OnError,
            },
            s if s.is_brew() && cfg.pid_bd_enabled => {
                Tunings::from_kp_tn_tv(cfg.pid_bd_kp, cfg.pid_bd_tn, cfg.pid_bd_tv, Proportional::OnError)
            }
            _ => Self::regular_tunings(cfg),
        }
    }

    fn control_temperature(&mut self, cfg: &Config) {
        let steaming = self.state == State::Steam;
        let raw = self.raw_temp.unwrap_or(0.0);
        self.pid_input = if steaming { raw } else { raw - cfg.brew_temp_offset };
        self.setpoint = if steaming {
            cfg.steam_setpoint
        } else {
            cfg.brew_setpoint
        };

        let delay_ms = cfg.brew_pid_delay * 1000.0;
        self.brew_delay_active = self.state.is_brew()
            && cfg.brew_pid_delay > 0.0
            && self.brew_time_ms > 0.0
            && self.brew_time_ms < delay_ms;

        // Gains are re-applied every tick, so a parameter write takes effect at once.
        let tunings = self.tunings_for(cfg);
        if tunings != self.pid.tunings() {
            self.pid.set_tunings(tunings);
        }
        self.pid.set_integrator_limits(0.0, cfg.pid_regular_i_max);
        self.pid.set_smoothing(cfg.pid_ema_factor);

        let heat = self
            .state
            .allows_heating(cfg.hardware_sensors_watertank_keep_heater_on_empty)
            && !self.brew_delay_active
            && !self.temp_fault
            && !self.emergency.active
            && self.raw_temp.is_some();
        if heat {
            self.pid.set_automatic(true, self.pid_input);
            self.pid.compute(self.now_ms, self.pid_input, self.setpoint);
        } else {
            self.pid.set_automatic(false, self.pid_input);
            self.pid.set_output(0.0);
        }
    }

    // --- outputs ------------------------------------------------------------------

    fn compute_outputs(&self, cfg: &Config, inp: &Inputs) -> Outputs {
        let s = self.state;
        let hot_water =
            self.switches.hot_water_was && self.hot_water_since_ms.is_some() && !self.hot_water_timed_out;
        let (mut pump, mut valve) = match s {
            State::BrewPreinfusion | State::BrewRunning | State::ManualFlush | State::BackflushFilling => {
                (true, true)
            }
            State::BrewPreinfusionPause => (false, true),
            State::PidNormal | State::Steam => (hot_water, false),
            _ => (false, false),
        };
        debug_assert!(!valve || s.is_water_flow());
        let window_pos = (self.now_ms % HEATER_WINDOW_MS) as f64;
        let mut heater = self.pid.is_automatic() && window_pos < self.pid.output();

        // Interlocks: nothing moves in an emergency or during a firmware update; the
        // pump never runs dry.
        let halt = self.emergency.active || inp.ota_active || s == State::EmergencyStop;
        if halt {
            heater = false;
            pump = false;
            valve = false;
        }
        if !self.water_tank_full {
            pump = false;
        }

        let blink = self.now_ms % HEATER_WINDOW_MS < HEATER_WINDOW_MS / 2;
        let tolerance = if s == State::Steam {
            STEAM_LED_TOLERANCE_C
        } else {
            cfg.display_blinking_delta
        };
        let led_state_ok = s.code() <= State::BackflushFinished.code();
        Outputs {
            heater,
            pump,
            valve,
            status_led: led_state_ok && (self.pid_input - self.setpoint).abs() <= tolerance,
            brew_led: s.is_brew()
                || (blink && (s == State::ManualFlush || (s.is_backflush() && s != State::BackflushIdle))),
            steam_led: s == State::Steam,
        }
    }
}

impl Config {
    /// Sets the brew setpoint from the setpoint endpoint (range checked).
    fn set_from_str_value_brew_setpoint(&mut self, t: f64) -> bool {
        let Some(i) = crate::config::find("brew.setpoint") else {
            return false;
        };
        self.set(i, crate::config::Value::Double(t)).is_ok()
    }
}
