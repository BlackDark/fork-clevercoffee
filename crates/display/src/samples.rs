//! Sample statuses and configurations covering every screen, for layout tests and
//! the `screens` contact-sheet tool.

extern crate std;

use std::vec::Vec;

use cc_core::config::{BrewMode, Config, DisplayTemplate};
use cc_core::machine::Outputs;
use cc_core::scale::Calibration;
use cc_core::{State, Status};
use cc_pid::{Proportional, Tunings};

use crate::Net;

pub const TEMPLATES: [(&str, DisplayTemplate); 6] = [
    ("standard", DisplayTemplate::Standard),
    ("minimal", DisplayTemplate::Minimal),
    ("temperature-only", DisplayTemplate::TemperatureOnly),
    ("scale", DisplayTemplate::Scale),
    ("upright", DisplayTemplate::Upright),
    ("modern", DisplayTemplate::Modern),
];

pub fn status(state: State) -> Status {
    Status {
        now_ms: 123_456,
        state,
        temperature: 93.4,
        temperature_valid: true,
        setpoint: 95.0,
        heater_power: 42.5,
        pid_runtime: true,
        tunings: Tunings::from_kp_tn_tv(62.0, 52.0, 11.5, Proportional::OnError),
        emergency: false,
        backflush_mode: state.is_backflush(),
        backflush_cycle: 3,
        brew_time_ms: 27_400.0,
        brew_target_ms: 32_000.0,
        brew_timer_visible: state.is_brew(),
        post_brew: false,
        hot_water_ms: 0,
        weight: Some(312.5),
        brew_weight: 36.2,
        scale_fault: false,
        scale_calibration: Calibration::Idle,
        pressure: Some(9.1),
        water_tank_full: true,
        standby_remaining_ms: 600_000,
        display_off: false,
        shots_since_backflush: 12,
        outputs: Outputs::default(),
    }
}

/// Every state a screen exists for, plus variants of the normal screen.
pub fn scenarios() -> Vec<(&'static str, Status)> {
    let mut v = Vec::new();
    for s in [
        State::PidNormal,
        State::BrewPreinfusion,
        State::BrewRunning,
        State::BrewFinished,
        State::ManualFlush,
        State::Steam,
        State::BackflushIdle,
        State::BackflushFilling,
        State::BackflushFinished,
        State::WaterTankEmpty,
        State::EmergencyStop,
        State::PidDisabled,
        State::Standby,
        State::SensorError,
    ] {
        v.push((s.name(), status(s)));
    }
    let mut heating = status(State::PidNormal);
    heating.temperature = 61.7;
    v.push(("HEATING", heating));
    let mut hot = status(State::PidNormal);
    hot.temperature = 120.4;
    v.push(("HOT_120", hot));
    let mut water = status(State::PidNormal);
    water.hot_water_ms = 12_300;
    v.push(("HOT_WATER", water));
    let mut post = status(State::PidNormal);
    post.brew_timer_visible = true;
    post.post_brew = true;
    v.push(("POST_BREW", post));
    let mut due = status(State::PidNormal);
    due.shots_since_backflush = 80;
    v.push(("BACKFLUSH_DUE", due));
    let mut fault = status(State::BrewRunning);
    fault.scale_fault = true;
    v.push(("SCALE_FAULT", fault));
    for (name, cal) in [
        ("CAL_TARE", Calibration::Taring { until_ms: 200_000 }),
        ("CAL_PLACE", Calibration::Placing { until_ms: 200_000 }),
        (
            "CAL_DONE",
            Calibration::Done {
                factor: 1234.5,
                at_ms: 123_000,
            },
        ),
        ("CAL_FAIL", Calibration::Failed { at_ms: 123_000 }),
    ] {
        let mut s = status(State::PidNormal);
        s.scale_calibration = cal;
        v.push((name, s));
    }
    v
}

pub fn configs() -> Vec<(&'static str, Config)> {
    let base = Config {
        hardware_switches_brew_enabled: true,
        mqtt_enabled: true,
        maintenance_backflush_reminder_enabled: true,
        ..Config::default()
    };
    let mut full = base.clone();
    full.hardware_sensors_scale_enabled = true;
    full.hardware_sensors_pressure_enabled = true;
    full.brew_mode = BrewMode::Automatic;
    full.brew_by_weight_enabled = true;
    full.brew_by_time_enabled = true;
    full.display_fullscreen_brew_timer = false;
    let mut fullscreen = full.clone();
    fullscreen.display_fullscreen_brew_timer = true;
    fullscreen.display_fullscreen_hot_water_timer = true;
    fullscreen.display_fullscreen_manual_flush_timer = true;
    let mut fullscreen_no_scale = fullscreen.clone();
    fullscreen_no_scale.hardware_sensors_scale_enabled = false;
    std::vec![
        ("basic", base),
        ("sensors", full),
        ("fullscreen", fullscreen),
        ("fullscreen-noscale", fullscreen_no_scale)
    ]
}

pub fn net() -> Net {
    Net {
        offline: false,
        wifi_connected: true,
        signal: 3,
        wifi_reconnects: 0,
        mqtt_connected: true,
        ble_scale_connected: true,
    }
}
