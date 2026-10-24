//! Behaviour of the machine, end to end through `Machine::tick`.
//!
//! These replace the C++ suites (state flow, brew handler, pre-infusion pause,
//! backflush, steam water injection, water tank, emergency stop, power handler,
//! standby). Tests named `fix_*` pin a deliberate difference from the C++; see
//! docs/decisions/0004-behaviour-changes.md.

use cc_core::config::{BrewMode, SwitchMode, SwitchType};
use cc_core::machine::{
    Command, Event, FINISHED_DISPLAY_MS, Inputs, MAX_BREW_MS, MAX_HOT_WATER_MS, Machine, Outputs, Reply,
    TICK_MS,
};
use cc_core::{Config, State};

struct Sim {
    m: Machine,
    cfg: Config,
    inp: Inputs,
    events: Vec<Event>,
}

impl Sim {
    fn new(cfg: Config) -> Self {
        let m = Machine::new(&cfg, 0, 0);
        let inp = Inputs {
            water_tank: true,
            ..Inputs::default()
        };
        Self {
            m,
            cfg,
            inp,
            events: Vec::new(),
        }
    }

    /// PID on, all switches wired, a warm boiler.
    fn ready() -> Self {
        let cfg = Config {
            pid_enabled: true,
            hardware_switches_brew_enabled: true,
            hardware_switches_steam_enabled: true,
            hardware_switches_hot_water_enabled: true,
            ..Config::default()
        };
        let mut s = Self::new(cfg);
        s.temp(93.0);
        s.run(100);
        assert_eq!(s.state(), State::PidNormal);
        s
    }

    fn state(&self) -> State {
        self.m.state()
    }

    fn out(&self) -> Outputs {
        self.m.outputs()
    }

    fn temp(&mut self, t: f64) {
        self.inp.temperature = Some(t);
    }

    /// Runs for `ms`, delivering the current temperature every 100 ms like a sensor.
    fn run(&mut self, ms: u64) {
        let end = self.inp.now_ms + ms;
        let mut last_t = self.inp.temperature;
        while self.inp.now_ms < end {
            self.inp.now_ms += TICK_MS;
            if self.inp.now_ms.is_multiple_of(100) && last_t.is_some() {
                self.inp.temperature = last_t;
            }
            let ev = self.m.tick(&mut self.cfg, &self.inp);
            self.events.extend(ev);
            last_t = self.inp.temperature.or(last_t);
            self.inp.temperature = None;
            self.inp.scale = None;
        }
        self.inp.temperature = last_t;
    }

    fn cmd(&mut self, c: Command) -> Reply {
        let mut ev = heapless::Vec::new();
        let r = self.m.command(c, &mut self.cfg, &mut ev);
        self.events.extend(ev);
        r
    }

    fn brew(&mut self, on: bool) {
        self.inp.brew_switch = on;
        self.run(100);
    }

    fn shots(&self) -> u32 {
        self.m.status(&self.cfg).shots_since_backflush
    }
}

#[test]
fn boot_waits_for_the_first_temperature() {
    let mut s = Sim::new(Config {
        pid_enabled: true,
        ..Config::default()
    });
    s.run(500);
    assert_eq!(s.state(), State::Init);
    assert!(!s.out().heater);
    s.temp(25.0);
    s.run(50);
    assert_eq!(s.state(), State::PidNormal);
}

#[test]
fn boot_with_pid_off_goes_to_pid_disabled() {
    let mut s = Sim::new(Config::default());
    s.temp(25.0);
    s.run(50);
    assert_eq!(s.state(), State::PidDisabled);
}

#[test]
fn no_temperature_at_boot_is_a_sensor_error() {
    let mut s = Sim::new(Config {
        pid_enabled: true,
        ..Config::default()
    });
    s.run(2_500);
    assert_eq!(s.state(), State::SensorError);
    assert!(!s.out().heater);
}

#[test]
fn cold_boiler_heats_at_full_duty() {
    let mut s = Sim::ready();
    s.temp(20.0);
    s.run(1_000);
    let st = s.m.status(&s.cfg);
    assert_eq!(st.heater_power, 100.0);
}

#[test]
fn heater_follows_the_time_proportioning_window() {
    let mut s = Sim::ready();
    s.temp(94.0);
    s.run(30_000); // let the derivative of the 93 -> 94 °C step decay
    let duty = s.m.status(&s.cfg).heater_power;
    assert!(duty > 0.0 && duty < 100.0, "duty {duty}");
    let mut on = 0;
    for _ in 0..100 {
        s.run(TICK_MS);
        on += u32::from(s.out().heater);
    }
    let expected = (duty).round() as i64;
    assert!(
        (i64::from(on) - expected).abs() <= 2,
        "on {on} of 100, duty {duty}"
    );
}

#[test]
fn sensor_dropout_stops_heating_and_recovers_after_delay() {
    let mut s = Sim::ready();
    s.inp.temperature = None;
    // Stop delivering readings.
    let mut sim_t = None;
    std::mem::swap(&mut s.inp.temperature, &mut sim_t);
    for _ in 0..300 {
        s.inp.now_ms += TICK_MS;
        s.m.tick(
            &mut s.cfg,
            &Inputs {
                temperature: None,
                ..s.inp
            },
        );
    }
    assert_eq!(s.state(), State::SensorError);
    assert!(!s.out().heater);
    s.temp(93.0);
    s.run(4_000);
    assert_eq!(
        s.state(),
        State::SensorError,
        "recovery needs 5 s of good readings"
    );
    s.run(1_500);
    assert_eq!(s.state(), State::PidNormal);
}

#[test]
fn over_temperature_trips_after_three_readings_and_clears_when_cool() {
    let mut s = Sim::ready();
    s.temp(151.0);
    s.run(150);
    assert_eq!(s.state(), State::PidNormal, "one reading is not enough");
    s.run(300);
    assert_eq!(s.state(), State::EmergencyStop);
    assert_eq!(
        s.out(),
        Outputs {
            status_led: false,
            ..Outputs::default()
        }
    );
    s.temp(120.0);
    s.run(500);
    assert_eq!(s.state(), State::EmergencyStop, "must cool to 100 °C");
    s.temp(99.0);
    s.run(200);
    assert_eq!(s.state(), State::PidNormal);
}

#[test]
fn implausible_reading_trips_immediately() {
    let mut s = Sim::ready();
    s.temp(250.0);
    s.run(110);
    assert_eq!(s.state(), State::EmergencyStop);
}

#[test]
fn manual_toggle_brew_runs_pump_and_valve_until_released() {
    let mut s = Sim::ready();
    s.brew(true);
    assert_eq!(s.state(), State::BrewRunning);
    assert!(s.out().pump && s.out().valve);
    s.run(26_000);
    s.brew(false);
    assert_eq!(s.state(), State::BrewFinished);
    assert!(!s.out().pump && !s.out().valve);
    s.run(FINISHED_DISPLAY_MS);
    assert_eq!(s.state(), State::PidNormal);
    assert_eq!(s.shots(), 1);
}

#[test]
fn momentary_brew_starts_and_stops_on_presses() {
    let mut s = Sim::ready();
    s.cfg.hardware_switches_brew_type = SwitchType::Momentary;
    s.m = Machine::new(&s.cfg, 0, s.inp.now_ms);
    s.run(100);
    s.brew(true);
    s.brew(false);
    assert_eq!(s.state(), State::BrewRunning);
    s.run(1_000);
    s.brew(true);
    assert_eq!(s.state(), State::BrewFinished);
}

#[test]
fn automatic_brew_with_preinfusion_pause_and_time_target() {
    let mut s = Sim::ready();
    s.cfg.brew_mode = BrewMode::Automatic;
    s.cfg.brew_pre_infusion_enabled = true;
    s.cfg.brew_pre_infusion_time = 2.0;
    s.cfg.brew_pre_infusion_pause = 5.0;
    s.cfg.brew_by_time_enabled = true;
    s.cfg.brew_by_time_target_time = 25.0;
    s.brew(true);
    assert_eq!(s.state(), State::BrewPreinfusion);
    assert!(s.out().pump && s.out().valve);
    s.run(2_000);
    assert_eq!(s.state(), State::BrewPreinfusionPause);
    assert!(
        !s.out().pump && s.out().valve,
        "pause soaks: valve open, pump off"
    );
    s.run(5_000);
    assert_eq!(s.state(), State::BrewRunning);
    let st = s.m.status(&s.cfg);
    assert_eq!(st.brew_target_ms, 32_000.0);
    assert!(st.brew_time_ms >= 7_000.0);
    s.run(25_100);
    assert_eq!(s.state(), State::BrewFinished, "stops at the total target time");
}

#[test]
fn preinfusion_without_pause_goes_straight_to_running() {
    let mut s = Sim::ready();
    s.cfg.brew_mode = BrewMode::Automatic;
    s.cfg.brew_pre_infusion_enabled = true;
    s.cfg.brew_pre_infusion_pause = 0.0;
    s.brew(true);
    s.run(2_100);
    assert_eq!(s.state(), State::BrewRunning);
}

#[test]
fn brew_stops_at_target_weight() {
    let mut s = Sim::ready();
    s.cfg.brew_mode = BrewMode::Automatic;
    s.cfg.brew_by_weight_enabled = true;
    s.cfg.brew_by_weight_target_weight = 36.0;
    s.cfg.hardware_sensors_scale_enabled = true;
    s.cfg.hardware_sensors_scale_type = cc_core::config::ScaleType::Bluetooth;
    s.inp.scale = Some(cc_core::scale::ScaleReading::Grams(0.0));
    s.run(20);
    s.brew(true);
    assert_eq!(s.state(), State::BrewRunning);
    s.inp.scale = Some(cc_core::scale::ScaleReading::Grams(20.0));
    s.run(20);
    assert_eq!(s.state(), State::BrewRunning);
    s.inp.scale = Some(cc_core::scale::ScaleReading::Grams(36.2));
    s.run(20);
    assert_eq!(s.state(), State::BrewFinished);
}

#[test]
fn short_shots_are_not_counted() {
    let mut s = Sim::ready();
    s.brew(true);
    s.run(2_000);
    s.brew(false);
    s.run(FINISHED_DISPLAY_MS + 100);
    assert_eq!(s.shots(), 0);
}

#[test]
fn brew_pid_delay_keeps_the_heater_off_at_shot_start() {
    let mut s = Sim::ready();
    s.temp(80.0);
    s.run(2_000);
    assert!(s.m.status(&s.cfg).heater_power > 0.0);
    s.brew(true);
    s.run(2_000);
    assert_eq!(s.m.status(&s.cfg).heater_power, 0.0);
    assert!(!s.out().heater);
    s.run(9_000);
    assert!(s.m.status(&s.cfg).heater_power > 0.0, "PID resumes after 10 s");
}

#[test]
fn switching_pid_off_aborts_a_brew() {
    let mut s = Sim::ready();
    s.brew(true);
    assert_eq!(s.cmd(Command::SetPid(false)), Reply::Pid(false));
    s.run(20);
    assert_eq!(s.state(), State::PidDisabled);
    assert!(!s.out().pump && !s.out().valve && !s.out().heater);
    assert!(s.events.contains(&Event::ConfigChanged));
}

#[test]
fn empty_tank_stops_the_pump_and_heater() {
    let mut s = Sim::ready();
    s.cfg.hardware_sensors_watertank_enabled = true;
    s.m = Machine::new(&s.cfg, 0, s.inp.now_ms);
    s.inp.water_tank = false; // normally closed: low = water present
    s.run(200);
    assert_eq!(s.state(), State::PidNormal);
    s.brew(true);
    assert_eq!(s.state(), State::BrewRunning);
    s.inp.water_tank = true;
    s.run(200);
    assert_eq!(s.state(), State::WaterTankEmpty);
    assert!(!s.out().pump && !s.out().heater);
    s.cfg.hardware_sensors_watertank_keep_heater_on_empty = true;
    s.temp(80.0);
    s.run(1_100);
    assert!(
        s.m.status(&s.cfg).heater_power > 0.0,
        "keep_heater_on_empty heats"
    );
    s.inp.water_tank = false;
    s.run(200);
    assert_eq!(s.state(), State::PidNormal);
}

// The tank switch read "empty" until its first debounce period was over, so every
// boot logged INIT -> WATER_TANK_EMPTY -> PID_NORMAL (seen on the bench).
#[test]
fn boot_with_a_full_tank_skips_water_tank_empty() {
    let cfg = Config {
        pid_enabled: true,
        hardware_sensors_watertank_enabled: true,
        hardware_sensors_watertank_mode: SwitchMode::NormallyOpen,
        ..Config::default()
    };
    let mut s = Sim::new(cfg);
    s.inp.water_tank = true; // normally open: high = full
    s.temp(93.0);
    s.run(200);
    assert_eq!(s.state(), State::PidNormal);
    assert!(
        !s.events.iter().any(|e| matches!(
            e,
            Event::StateChanged {
                to: State::WaterTankEmpty,
                ..
            }
        )),
        "{:?}",
        s.events
    );
}

#[test]
fn boot_with_an_empty_tank_reports_it_after_debounce() {
    let cfg = Config {
        pid_enabled: true,
        hardware_sensors_watertank_enabled: true,
        hardware_sensors_watertank_mode: SwitchMode::NormallyOpen,
        ..Config::default()
    };
    let mut s = Sim::new(cfg);
    s.inp.water_tank = false;
    s.temp(93.0);
    s.run(200);
    assert_eq!(s.state(), State::WaterTankEmpty);
}

#[test]
fn fix_brew_toggle_flipped_while_tank_empty_does_not_brew_after_refill() {
    let mut s = Sim::ready();
    s.cfg.hardware_sensors_watertank_enabled = true;
    s.m = Machine::new(&s.cfg, 0, s.inp.now_ms);
    s.inp.water_tank = true; // empty
    s.run(200);
    assert_eq!(s.state(), State::WaterTankEmpty);
    s.brew(true);
    s.inp.water_tank = false;
    s.run(300);
    assert_eq!(s.state(), State::PidNormal);
}

#[test]
fn hot_water_switch_runs_the_pump_with_valve_closed() {
    let mut s = Sim::ready();
    s.inp.hot_water_switch = true;
    s.run(200);
    assert!(s.out().pump && !s.out().valve);
    assert!(s.m.status(&s.cfg).hot_water_ms > 0);
    s.inp.hot_water_switch = false;
    s.run(100);
    assert!(!s.out().pump);
}

#[test]
fn fix_hot_water_is_capped() {
    let mut s = Sim::ready();
    s.inp.hot_water_switch = true;
    s.run(MAX_HOT_WATER_MS + 500);
    assert!(!s.out().pump);
}

#[test]
fn fix_brew_is_capped() {
    let mut s = Sim::ready();
    s.brew(true);
    s.run(MAX_BREW_MS + 200);
    assert_ne!(s.state(), State::BrewRunning);
}

#[test]
fn steam_switch_uses_steam_setpoint_and_gains() {
    let mut s = Sim::ready();
    s.inp.steam_switch = true;
    s.run(100);
    assert_eq!(s.state(), State::Steam);
    let st = s.m.status(&s.cfg);
    assert_eq!(st.setpoint, 120.0);
    assert_eq!(st.tunings.kp, 150.0);
    assert_eq!(st.tunings.ki, 0.0);
    assert!(s.out().steam_led);
    s.inp.steam_switch = false;
    s.run(100);
    assert_eq!(s.state(), State::PidNormal);
}

#[test]
fn hot_water_during_steam_injects_water() {
    let mut s = Sim::ready();
    s.inp.steam_switch = true;
    s.run(100);
    s.inp.hot_water_switch = true;
    s.run(100);
    assert_eq!(s.state(), State::Steam);
    assert!(s.out().pump && !s.out().valve);
}

#[test]
fn fix_api_steam_toggle_enters_the_steam_state() {
    let mut s = Sim::ready();
    assert_eq!(s.cmd(Command::ToggleSteam), Reply::Steam(true));
    s.run(20);
    assert_eq!(s.state(), State::Steam);
    assert_eq!(s.cmd(Command::ToggleSteam), Reply::Steam(false));
    s.run(20);
    assert_eq!(s.state(), State::PidNormal);
}

#[test]
fn fix_brew_press_during_steam_is_not_replayed() {
    let mut s = Sim::ready();
    s.inp.steam_switch = true;
    s.run(100);
    s.brew(true);
    assert_eq!(s.state(), State::Steam);
    s.inp.brew_switch = false;
    s.inp.steam_switch = false;
    s.run(200);
    assert_eq!(s.state(), State::PidNormal);
}

#[test]
fn backflush_runs_the_configured_cycles() {
    let mut s = Sim::ready();
    s.cfg.backflush_cycles = 2;
    s.cfg.backflush_fill_time = 3.0;
    s.cfg.backflush_flush_time = 5.0;
    s.brew(true);
    s.run(6_000);
    s.brew(false);
    s.run(FINISHED_DISPLAY_MS + 100);
    assert_eq!(s.shots(), 1);
    assert_eq!(s.cmd(Command::ToggleBackflush), Reply::Backflush(true));
    s.run(20);
    assert_eq!(s.state(), State::BackflushIdle);
    assert!(!s.out().heater, "no heating in backflush");
    s.brew(true);
    assert_eq!(s.state(), State::BackflushFilling);
    assert!(s.out().pump && s.out().valve);
    s.run(3_000);
    assert_eq!(s.state(), State::BackflushFlushing);
    assert!(!s.out().pump && !s.out().valve);
    s.run(5_000);
    assert_eq!(s.state(), State::BackflushFilling);
    assert_eq!(s.m.status(&s.cfg).backflush_cycle, 2);
    s.run(8_000);
    assert_eq!(s.state(), State::BackflushFinished);
    assert_eq!(s.shots(), 0, "finished backflush resets the counter");
    s.run(FINISHED_DISPLAY_MS);
    assert_eq!(s.state(), State::BackflushIdle);
    s.cmd(Command::SetBackflush(false));
    s.run(20);
    assert_eq!(s.state(), State::PidNormal);
}

#[test]
fn backflush_toggle_switch_off_stops_the_cycle() {
    let mut s = Sim::ready();
    s.cmd(Command::SetBackflush(true));
    s.run(20);
    s.brew(true);
    assert_eq!(s.state(), State::BackflushFilling);
    s.brew(false);
    assert_eq!(s.state(), State::BackflushIdle);
}

#[test]
fn fix_momentary_long_press_in_backflush_idle_flushes_while_held() {
    let mut s = Sim::ready();
    s.cfg.hardware_switches_brew_type = SwitchType::Momentary;
    s.m = Machine::new(&s.cfg, 0, s.inp.now_ms);
    s.run(100);
    s.cmd(Command::SetBackflush(true));
    s.run(20);
    s.inp.brew_switch = true;
    s.run(700);
    assert_eq!(s.state(), State::ManualFlush);
    assert!(s.out().pump && s.out().valve);
    s.brew(false);
    assert_eq!(s.state(), State::BackflushIdle);
    // A short press starts a cycle instead.
    s.brew(true);
    s.brew(false);
    assert_eq!(s.state(), State::BackflushFilling);
}

#[test]
fn standby_timer_turns_the_heater_off_and_brew_press_wakes() {
    let mut s = Sim::ready();
    s.cfg.standby_enabled = true;
    s.cfg.standby_time = 1.0;
    s.run(61_000);
    assert_eq!(s.state(), State::Standby);
    assert!(!s.out().heater);
    s.brew(true);
    assert_eq!(s.state(), State::PidNormal);
}

#[test]
fn fix_wake_by_brew_press_does_not_start_a_shot() {
    let mut s = Sim::ready();
    s.cmd(Command::Sleep);
    s.run(20);
    assert_eq!(s.state(), State::Standby);
    s.brew(true);
    s.run(200);
    assert_eq!(s.state(), State::PidNormal);
}

#[test]
fn fix_standby_sticks_after_web_activity() {
    let mut s = Sim::ready();
    s.cfg.standby_enabled = true;
    s.cfg.standby_time = 1.0;
    s.run(10);
    s.cmd(Command::ConfigUpdated);
    s.run(61_000);
    assert_eq!(s.state(), State::Standby);
    s.run(1_000);
    assert_eq!(s.state(), State::Standby);
}

#[test]
fn power_toggle_switch_sleeps_and_wakes() {
    let cfg = Config {
        pid_enabled: true,
        hardware_switches_power_enabled: true,
        ..Config::default()
    };
    let mut s = Sim::new(cfg);
    s.inp.power_switch = true;
    s.temp(90.0);
    s.run(200);
    assert_eq!(s.state(), State::PidNormal);
    s.inp.power_switch = false;
    s.run(200);
    assert_eq!(s.state(), State::Standby);
    s.cfg.pid_enabled = false; // power-on re-enables the PID
    s.inp.power_switch = true;
    s.run(200);
    assert_eq!(s.state(), State::PidNormal);
    assert!(s.cfg.pid_enabled);
}

#[test]
fn power_momentary_long_press_requests_reboot() {
    let cfg = Config {
        pid_enabled: true,
        hardware_switches_power_enabled: true,
        hardware_switches_power_type: SwitchType::Momentary,
        ..Config::default()
    };
    let mut s = Sim::new(cfg);
    s.temp(90.0);
    s.run(6_000);
    s.inp.power_switch = true;
    s.run(1_800);
    assert!(s.events.contains(&Event::Reboot));
}

#[test]
fn gain_changes_apply_without_a_state_change() {
    let mut s = Sim::ready();
    s.cfg.pid_regular_kp = 10.0;
    s.run(20);
    assert_eq!(s.m.status(&s.cfg).tunings.kp, 10.0);
}

#[test]
fn ota_turns_everything_off() {
    let mut s = Sim::ready();
    s.temp(20.0);
    s.brew(true);
    s.inp.ota_active = true;
    s.run(20);
    let o = s.out();
    assert!(!o.heater && !o.pump && !o.valve);
    // The PID still wants to heat a cold boiler; the status reports what the heater gets.
    assert_eq!(s.m.status(&s.cfg).heater_power, 0.0);
    s.inp.ota_active = false;
    s.brew(false);
    s.run(FINISHED_DISPLAY_MS + 2_000);
    assert!(s.m.status(&s.cfg).heater_power > 0.0);
}

#[test]
fn setpoint_command_is_range_checked() {
    let mut s = Sim::ready();
    assert_eq!(s.cmd(Command::SetBrewSetpoint(93.5)), Reply::Ok);
    assert_eq!(s.cfg.brew_setpoint, 93.5, "fractional setpoints are kept");
    assert!(matches!(
        s.cmd(Command::SetBrewSetpoint(300.0)),
        Reply::Rejected(_)
    ));
}

#[test]
fn brew_offset_is_subtracted_outside_steam() {
    let mut s = Sim::ready();
    s.cfg.brew_temp_offset = 2.0;
    s.temp(95.0);
    s.run(200);
    assert_eq!(s.m.status(&s.cfg).temperature, 93.0);
}

#[test]
fn idle_for_restart_waits_for_shots_steam_and_hot_water() {
    let mut s = Sim::ready();
    let idle = |s: &Sim| s.m.status(&s.cfg).idle_for_restart();
    assert!(idle(&s));

    s.brew(true);
    assert!(s.state().is_brew_active());
    assert!(!idle(&s));
    s.brew(false);
    assert_eq!(s.state(), State::BrewFinished);
    assert!(!idle(&s), "the shot result is still on the display");
    s.run(FINISHED_DISPLAY_MS + 100);
    assert!(idle(&s));

    s.inp.hot_water_switch = true;
    s.run(100);
    assert!(s.out().pump);
    assert!(!idle(&s));
    s.inp.hot_water_switch = false;
    s.run(100);
    assert!(idle(&s));

    s.inp.steam_switch = true;
    s.run(100);
    assert_eq!(s.state(), State::Steam);
    assert!(!idle(&s));
}
