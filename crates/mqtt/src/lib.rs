//! MQTT topics, payloads and Home Assistant discovery.
//!
//! One table ([`ENTITIES`]) defines every topic: what it publishes, whether it accepts
//! `<topic>/set`, and how Home Assistant shows it. The C++ firmware kept the
//! registration and the discovery in two lists that had drifted apart (the "Use PonM"
//! switch listened on `usePonM` while the value lived on `pidUsePonM`); here they
//! cannot.
//!
//! Topic layout (unchanged): `<prefix><hostname>/<name>` for state,
//! `<prefix><hostname>/<name>/set` for commands, `<prefix><hostname>/status` for
//! availability (`online`/`offline`, retained, also the last will).

#![no_std]

use core::fmt::{self, Write};

use cc_api::json::Writer;
use cc_core::config::{self, Config, ScaleType};
use cc_core::{Command, Status};

/// When an entity exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    Always,
    BrewSwitch,
    Scale,
    DualScale,
    Pressure,
    WaterTank,
}

impl Needs {
    pub fn met(self, cfg: &Config) -> bool {
        match self {
            Needs::Always => true,
            Needs::BrewSwitch => cfg.hardware_switches_brew_enabled,
            Needs::Scale => cfg.hardware_sensors_scale_enabled,
            Needs::DualScale => {
                cfg.hardware_sensors_scale_enabled && cfg.hardware_sensors_scale_type == ScaleType::Hx711Dual
            }
            Needs::Pressure => cfg.hardware_sensors_pressure_enabled,
            Needs::WaterTank => cfg.hardware_sensors_watertank_enabled,
        }
    }
}

/// Live values published read-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    Temperature,
    HeaterPower,
    StandbyRemaining,
    ShotsSinceBackflush,
    BackflushReminderDue,
    Kp,
    Ki,
    Kd,
    MachineState,
    BrewTime,
    Weight,
    BrewWeight,
    Pressure,
    WaterTankFull,
}

/// Machine actions with their own on/off state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Steam,
    Backflush,
    Tare,
    Calibrate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Param(&'static str),
    Action(Action),
    Reading(Reading),
}

/// Home Assistant presentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ha {
    None,
    Switch(&'static str),
    Button(&'static str),
    Number {
        name: &'static str,
        step: f64,
        unit: &'static str,
    },
    Sensor {
        name: &'static str,
        unit: &'static str,
        class: &'static str,
    },
    BinarySensor(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Entity {
    pub topic: &'static str,
    pub source: Source,
    pub needs: Needs,
    pub ha: Ha,
}

const fn param(topic: &'static str, key: &'static str, needs: Needs, ha: Ha) -> Entity {
    Entity {
        topic,
        source: Source::Param(key),
        needs,
        ha,
    }
}
const fn number(name: &'static str, step: f64, unit: &'static str) -> Ha {
    Ha::Number { name, step, unit }
}
const fn reading(topic: &'static str, r: Reading, needs: Needs, ha: Ha) -> Entity {
    Entity {
        topic,
        source: Source::Reading(r),
        needs,
        ha,
    }
}
const fn sensor(name: &'static str, unit: &'static str, class: &'static str) -> Ha {
    Ha::Sensor { name, unit, class }
}

use Needs::{Always, BrewSwitch, DualScale, Pressure, Scale, WaterTank};

pub static ENTITIES: &[Entity] = &[
    param("pidON", "pid.enabled", Always, Ha::Switch("Use PID")),
    param(
        "brewSetpoint",
        "brew.setpoint",
        Always,
        number("Brew setpoint", 0.1, "°C"),
    ),
    param(
        "brewTempOffset",
        "brew.temp_offset",
        Always,
        number("Brew Temp. Offset", 0.1, "°C"),
    ),
    Entity {
        topic: "steamON",
        source: Source::Action(Action::Steam),
        needs: Always,
        ha: Ha::Switch("Steam"),
    },
    param(
        "steamSetpoint",
        "steam.setpoint",
        Always,
        number("Steam setpoint", 0.1, "°C"),
    ),
    param("pidUsePonM", "pid.use_ponm", Always, Ha::Switch("Use PonM")),
    param("aggKp", "pid.regular.kp", Always, number("aggKp", 0.1, "")),
    param("aggTn", "pid.regular.tn", Always, number("aggTn", 0.1, "")),
    param("aggTv", "pid.regular.tv", Always, number("aggTv", 0.1, "")),
    param("aggIMax", "pid.regular.i_max", Always, number("aggIMax", 0.1, "")),
    param("steamKp", "pid.steam.kp", Always, number("Steam Kp", 0.1, "")),
    param(
        "standbyModeOn",
        "standby.enabled",
        Always,
        Ha::Switch("Standby Timer"),
    ),
    param("aggbKp", "pid.bd.kp", BrewSwitch, Ha::None),
    param("aggbTn", "pid.bd.tn", BrewSwitch, Ha::None),
    param("aggbTv", "pid.bd.tv", BrewSwitch, Ha::None),
    param("pidUseBD", "pid.bd.enabled", BrewSwitch, Ha::None),
    param(
        "brewPidDelay",
        "brew.pid_delay",
        BrewSwitch,
        number("Brew Pid Delay", 0.1, "s"),
    ),
    param(
        "targetBrewTime",
        "brew.by_time.target_time",
        BrewSwitch,
        number("Target Brew time", 0.1, "s"),
    ),
    param(
        "preinfusion",
        "brew.pre_infusion.time",
        BrewSwitch,
        number("Preinfusion filling time", 0.1, "s"),
    ),
    param(
        "preinfusionPause",
        "brew.pre_infusion.pause",
        BrewSwitch,
        number("Preinfusion pause time", 0.1, "s"),
    ),
    Entity {
        topic: "backflushOn",
        source: Source::Action(Action::Backflush),
        needs: BrewSwitch,
        ha: Ha::Switch("Backflush"),
    },
    param(
        "backflushCycles",
        "backflush.cycles",
        BrewSwitch,
        number("Backflush Cycles", 1.0, ""),
    ),
    param(
        "backflushFillTime",
        "backflush.fill_time",
        BrewSwitch,
        number("Backflush filling time", 0.1, "s"),
    ),
    param(
        "backflushFlushTime",
        "backflush.flush_time",
        BrewSwitch,
        number("Backflush flushing time", 0.1, "s"),
    ),
    param(
        "backflushReminderEnabled",
        "maintenance.backflush_reminder.enabled",
        BrewSwitch,
        Ha::None,
    ),
    param(
        "backflushReminderThreshold",
        "maintenance.backflush_reminder.threshold",
        BrewSwitch,
        Ha::None,
    ),
    param(
        "targetBrewWeight",
        "brew.by_weight.target_weight",
        Scale,
        number("Brew Weight Target", 0.1, "g"),
    ),
    param(
        "scaleCalibration",
        "hardware.sensors.scale.calibration",
        Scale,
        Ha::None,
    ),
    param(
        "scale2Calibration",
        "hardware.sensors.scale.calibration2",
        DualScale,
        Ha::None,
    ),
    param(
        "scaleKnownWeight",
        "hardware.sensors.scale.known_weight",
        Scale,
        Ha::None,
    ),
    Entity {
        topic: "scaleTareOn",
        source: Source::Action(Action::Tare),
        needs: Scale,
        ha: Ha::Button("Tare Scale"),
    },
    Entity {
        topic: "scaleCalibrationOn",
        source: Source::Action(Action::Calibrate),
        needs: Scale,
        ha: Ha::Button("Calibrate Scale"),
    },
    reading(
        "temperature",
        Reading::Temperature,
        Always,
        sensor("Boiler Temperature", "°C", "temperature"),
    ),
    reading(
        "heaterPower",
        Reading::HeaterPower,
        Always,
        sensor("Heater Power", "%", "power_factor"),
    ),
    reading(
        "standbyModeTimeRemaining",
        Reading::StandbyRemaining,
        Always,
        Ha::None,
    ),
    reading(
        "shotsSinceBackflush",
        Reading::ShotsSinceBackflush,
        Always,
        sensor("Shots Since Backflush", "shots", ""),
    ),
    reading(
        "backflushReminderDue",
        Reading::BackflushReminderDue,
        Always,
        sensor("Backflush Reminder Due", "", ""),
    ),
    reading("currentKp", Reading::Kp, Always, Ha::None),
    reading("currentKi", Reading::Ki, Always, Ha::None),
    reading("currentKd", Reading::Kd, Always, Ha::None),
    reading(
        "machineState",
        Reading::MachineState,
        Always,
        sensor("Machine State", "", "enum"),
    ),
    reading(
        "currBrewTime",
        Reading::BrewTime,
        BrewSwitch,
        sensor("Current Brew Time", "s", "duration"),
    ),
    reading(
        "currReadingWeight",
        Reading::Weight,
        Scale,
        sensor("Weight", "g", "weight"),
    ),
    reading(
        "currBrewWeight",
        Reading::BrewWeight,
        Scale,
        sensor("current Brew Weight", "g", "weight"),
    ),
    reading(
        "pressure",
        Reading::Pressure,
        Pressure,
        sensor("Pressure", "bar", "pressure"),
    ),
    reading(
        "waterTankFull",
        Reading::WaterTankFull,
        WaterTank,
        Ha::BinarySensor("Water Tank Full"),
    ),
];

/// Publishing interval while a shot runs, normally, and in standby.
pub const INTERVAL_BREW_MS: u64 = 500;
pub const INTERVAL_MS: u64 = 5_000;
pub const INTERVAL_STANDBY_MS: u64 = 10_000;
/// Discovery is re-sent this often (Home Assistant restarts lose it otherwise).
pub const DISCOVERY_INTERVAL_MS: u64 = 5 * 60 * 1000;

pub fn interval_ms(st: &Status) -> u64 {
    if st.state.is_brew_active() {
        INTERVAL_BREW_MS
    } else if st.state == cc_core::State::Standby {
        INTERVAL_STANDBY_MS
    } else {
        INTERVAL_MS
    }
}

/// Writes `<prefix><hostname>/<name>[/<suffix>]`.
pub fn topic(cfg: &Config, name: &str, suffix: Option<&str>, out: &mut impl Write) -> fmt::Result {
    write!(out, "{}{}/{}", cfg.mqtt_topic, cfg.system_hostname, name)?;
    if let Some(s) = suffix {
        write!(out, "/{s}")?;
    }
    Ok(())
}

/// The state payload of an entity. Params use the C++ format (`1`/`0`, `%d`,
/// `%.2f`), readings `%.2f`, binary sensors `ON`/`OFF`.
pub fn payload(e: &Entity, cfg: &Config, st: &Status, out: &mut impl Write) -> fmt::Result {
    match e.source {
        Source::Param(key) => match config::find(key) {
            Some(i) => cfg.format_value(i, out),
            None => Ok(()),
        },
        Source::Action(a) => out.write_str(match a {
            Action::Steam => one(st.state == cc_core::State::Steam),
            Action::Backflush => one(st.backflush_mode),
            Action::Tare => "0",
            Action::Calibrate => one(matches!(
                st.scale_calibration,
                cc_core::scale::Calibration::Taring { .. } | cc_core::scale::Calibration::Placing { .. }
            )),
        }),
        Source::Reading(r) => {
            let v = match r {
                Reading::WaterTankFull => {
                    return out.write_str(if st.water_tank_full { "ON" } else { "OFF" });
                }
                Reading::Temperature => st.temperature,
                Reading::HeaterPower => st.heater_power,
                Reading::StandbyRemaining => st.standby_remaining_ms as f64 / 1000.0,
                Reading::ShotsSinceBackflush => f64::from(st.shots_since_backflush),
                Reading::BackflushReminderDue => f64::from(u8::from(st.backflush_reminder_due(cfg))),
                Reading::Kp => st.tunings.kp,
                Reading::Ki => st.tunings.ki,
                Reading::Kd => st.tunings.kd,
                Reading::MachineState => f64::from(st.state.code()),
                Reading::BrewTime => st.brew_time_ms / 1000.0,
                Reading::Weight => f64::from(st.weight.unwrap_or(0.0)),
                Reading::BrewWeight => f64::from(st.brew_weight),
                Reading::Pressure => f64::from(st.pressure.unwrap_or(0.0)),
            };
            write!(out, "{v:.2}")
        }
    }
}

fn one(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

/// What an incoming `<name>/set` message means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Incoming<'a> {
    /// Write a parameter (through the registry, with its validation).
    Param {
        key: &'static str,
        value: &'a str,
    },
    Command(Command),
}

/// Parses a message on `topic`. Returns `None` for foreign topics, read-only
/// entities and unparsable payloads.
pub fn parse_incoming<'a>(cfg: &Config, topic: &str, payload: &'a str) -> Option<Incoming<'a>> {
    let rest = topic.strip_prefix(cfg.mqtt_topic.as_str())?;
    let rest = rest
        .strip_prefix(cfg.system_hostname.as_str())?
        .strip_prefix('/')?;
    let name = rest.strip_suffix("/set")?;
    let e = ENTITIES.iter().find(|e| e.topic == name && e.needs.met(cfg))?;
    let payload = payload.trim();
    let on = || -> Option<bool> {
        match payload {
            "1" | "ON" | "on" | "true" => Some(true),
            "0" | "OFF" | "off" | "false" => Some(false),
            p => p.parse::<f64>().ok().map(|v| v != 0.0),
        }
    };
    match e.source {
        Source::Param(key) => Some(Incoming::Param { key, value: payload }),
        Source::Action(Action::Steam) => Some(Incoming::Command(Command::SetSteam(on()?))),
        Source::Action(Action::Backflush) => Some(Incoming::Command(Command::SetBackflush(on()?))),
        Source::Action(Action::Tare) => on()?.then_some(Incoming::Command(Command::ToggleScaleTare)),
        Source::Action(Action::Calibrate) => {
            on()?.then_some(Incoming::Command(Command::ToggleScaleCalibration))
        }
        Source::Reading(_) => None,
    }
}

/// Home Assistant component of an entity.
pub fn ha_component(ha: &Ha) -> Option<&'static str> {
    Some(match ha {
        Ha::None => return None,
        Ha::Switch(_) => "switch",
        Ha::Button(_) => "button",
        Ha::Number { .. } => "number",
        Ha::Sensor { .. } => "sensor",
        Ha::BinarySensor(_) => "binary_sensor",
    })
}

/// Discovery topic: `<ha prefix>/<component>/clevercoffee-<host>/<name>/config`.
pub fn discovery_topic(cfg: &Config, e: &Entity, out: &mut impl Write) -> Option<fmt::Result> {
    let c = ha_component(&e.ha)?;
    Some(write!(
        out,
        "{}/{}/clevercoffee-{}/{}/config",
        cfg.mqtt_hassio_prefix, c, cfg.system_hostname, e.topic
    ))
}

/// Discovery payload, retained by the broker.
pub fn discovery_payload(cfg: &Config, e: &Entity, w: &mut Writer<impl Write>) -> Option<fmt::Result> {
    ha_component(&e.ha)?;
    Some((|| {
        let mut base: heapless::String<128> = heapless::String::new();
        let _ = write!(base, "{}{}", cfg.mqtt_topic, cfg.system_hostname);
        let mut t: heapless::String<160> = heapless::String::new();
        w.begin_object()?;
        let (name, writable) = match e.ha {
            Ha::Switch(n) | Ha::Button(n) => (n, true),
            Ha::Number { name, .. } => (name, true),
            Ha::Sensor { name, .. } | Ha::BinarySensor(name) => (name, false),
            Ha::None => unreachable!(),
        };
        w.kv_str("name", name)?;
        let _ = write!(t, "{base}/{}", e.topic);
        w.kv_str("state_topic", &t)?;
        if writable {
            t.clear();
            let _ = write!(t, "{base}/{}/set", e.topic);
            w.kv_str("command_topic", &t)?;
        }
        t.clear();
        let _ = write!(t, "clevercoffee-{}-{}", cfg.system_hostname, e.topic);
        w.kv_str("unique_id", &t)?;
        match e.ha {
            Ha::Switch(_) => {
                w.kv_str("payload_on", "1")?;
                w.kv_str("payload_off", "0")?;
            }
            Ha::Button(_) => w.kv_str("payload_press", "1")?,
            Ha::Number { step, unit, .. } => {
                if let Source::Param(key) = e.source
                    && let Some(p) = config::PARAMS.iter().find(|p| p.key == key)
                {
                    match p.kind {
                        config::Kind::Double { min, max } => {
                            w.kv_num("min", min)?;
                            w.kv_num("max", max)?;
                        }
                        config::Kind::Int { min, max } => {
                            w.kv_int("min", i64::from(min))?;
                            w.kv_int("max", i64::from(max))?;
                        }
                        _ => {}
                    }
                }
                w.kv_num("step", step)?;
                w.kv_str("unit_of_measurement", unit)?;
                w.kv_str("mode", "box")?;
            }
            Ha::Sensor { unit, class, .. } => {
                if !unit.is_empty() {
                    w.kv_str("unit_of_measurement", unit)?;
                }
                if !class.is_empty() {
                    w.kv_str("device_class", class)?;
                }
            }
            Ha::BinarySensor(_) => {
                w.kv_str("payload_on", "ON")?;
                w.kv_str("payload_off", "OFF")?;
            }
            Ha::None => {}
        }
        t.clear();
        let _ = write!(t, "{base}/status");
        w.kv_str("availability_topic", &t)?;
        w.kv_str("payload_available", "online")?;
        w.kv_str("payload_not_available", "offline")?;
        w.key("device")?;
        w.begin_object()?;
        w.kv_str("identifiers", &cfg.system_hostname)?;
        w.kv_str("manufacturer", "CleverCoffee")?;
        w.kv_str("name", &cfg.system_hostname)?;
        w.end_object()?;
        w.end_object()
    })())
}

/// Remembers the last payload per entity, so only changes are published.
#[derive(Debug)]
pub struct ChangeFilter {
    hashes: [u32; ENTITY_SLOTS],
    known: [bool; ENTITY_SLOTS],
}

const ENTITY_SLOTS: usize = 64;

impl Default for ChangeFilter {
    fn default() -> Self {
        Self {
            hashes: [0; ENTITY_SLOTS],
            known: [false; ENTITY_SLOTS],
        }
    }
}

impl ChangeFilter {
    /// `true` if `payload` differs from what entity `index` last published.
    pub fn changed(&mut self, index: usize, payload: &str) -> bool {
        let h = payload.bytes().fold(2_166_136_261u32, |h, b| {
            (h ^ u32::from(b)).wrapping_mul(16_777_619)
        });
        let changed = !self.known[index] || self.hashes[index] != h;
        self.hashes[index] = h;
        self.known[index] = true;
        changed
    }

    /// Forget everything (after a reconnect, publish all again).
    pub fn reset(&mut self) {
        self.known = [false; ENTITY_SLOTS];
    }
}

const _: () = assert!(ENTITY_SLOTS >= 47);

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::string::String;

    fn cfg() -> Config {
        Config {
            hardware_switches_brew_enabled: true,
            hardware_sensors_scale_enabled: true,
            ..Config::default()
        }
    }

    #[test]
    fn table_fits_the_change_filter() {
        assert!(ENTITIES.len() <= ENTITY_SLOTS);
    }

    #[test]
    fn every_param_entity_names_a_real_parameter() {
        for e in ENTITIES {
            if let Source::Param(k) = e.source {
                assert!(config::find(k).is_some(), "{k}");
            }
        }
    }

    #[test]
    fn topics_are_unique() {
        for (i, a) in ENTITIES.iter().enumerate() {
            assert!(
                ENTITIES[i + 1..].iter().all(|b| b.topic != a.topic),
                "{}",
                a.topic
            );
        }
    }

    #[test]
    fn state_topic_layout_is_unchanged() {
        let mut s = String::new();
        topic(&cfg(), "temperature", None, &mut s).unwrap();
        assert_eq!(s, "custom/kitchen/silvia/temperature");
    }

    #[test]
    fn set_messages_parse() {
        let c = cfg();
        assert_eq!(
            parse_incoming(&c, "custom/kitchen/silvia/brewSetpoint/set", "93.5"),
            Some(Incoming::Param {
                key: "brew.setpoint",
                value: "93.5"
            })
        );
        assert_eq!(
            parse_incoming(&c, "custom/kitchen/silvia/steamON/set", "1"),
            Some(Incoming::Command(Command::SetSteam(true)))
        );
        assert_eq!(
            parse_incoming(&c, "custom/kitchen/silvia/scaleTareOn/set", "1"),
            Some(Incoming::Command(Command::ToggleScaleTare))
        );
        assert_eq!(
            parse_incoming(&c, "custom/kitchen/silvia/temperature/set", "1"),
            None,
            "read-only"
        );
        assert_eq!(parse_incoming(&c, "other/silvia/brewSetpoint/set", "1"), None);
        let no_scale = Config::default();
        assert_eq!(
            parse_incoming(&no_scale, "custom/kitchen/silvia/targetBrewWeight/set", "36"),
            None
        );
    }

    #[test]
    fn fix_ponm_switch_discovery_matches_its_command_topic() {
        let c = cfg();
        let e = ENTITIES.iter().find(|e| e.ha == Ha::Switch("Use PonM")).unwrap();
        let mut w = Writer::new(String::new());
        discovery_payload(&c, e, &mut w).unwrap().unwrap();
        let v: serde_json::Value = serde_json::from_str(&w.into_inner()).unwrap();
        assert_eq!(v["command_topic"], "custom/kitchen/silvia/pidUsePonM/set");
        assert_eq!(
            parse_incoming(&c, "custom/kitchen/silvia/pidUsePonM/set", "1"),
            Some(Incoming::Param {
                key: "pid.use_ponm",
                value: "1"
            })
        );
    }

    #[test]
    fn discovery_for_numbers_carries_limits() {
        let c = cfg();
        let e = ENTITIES.iter().find(|e| e.topic == "brewSetpoint").unwrap();
        let mut t = String::new();
        discovery_topic(&c, e, &mut t).unwrap().unwrap();
        assert_eq!(t, "homeassistant/number/clevercoffee-silvia/brewSetpoint/config");
        let mut w = Writer::new(String::new());
        discovery_payload(&c, e, &mut w).unwrap().unwrap();
        let v: serde_json::Value = serde_json::from_str(&w.into_inner()).unwrap();
        assert_eq!(v["min"], 20);
        assert_eq!(v["max"], 110);
        assert_eq!(v["device"]["manufacturer"], "CleverCoffee");
        assert_eq!(v["availability_topic"], "custom/kitchen/silvia/status");
    }

    #[test]
    fn payloads_use_the_cpp_formats() {
        let c = cfg();
        let m = cc_core::Machine::new(&c, 0, 0);
        let st = m.status(&c);
        let get = |name: &str| {
            let e = ENTITIES.iter().find(|e| e.topic == name).unwrap();
            let mut s = String::new();
            payload(e, &c, &st, &mut s).unwrap();
            s
        };
        assert_eq!(get("pidON"), "0");
        assert_eq!(get("brewSetpoint"), "95.00");
        assert_eq!(get("backflushCycles"), "5");
        assert_eq!(get("machineState"), "0.00");
    }

    #[test]
    fn change_filter_suppresses_repeats() {
        let mut f = ChangeFilter::default();
        assert!(f.changed(3, "1"));
        assert!(!f.changed(3, "1"));
        assert!(f.changed(3, "2"));
        f.reset();
        assert!(f.changed(3, "2"));
    }
}
