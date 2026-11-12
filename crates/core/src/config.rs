//! User configuration: one typed [`Config`] plus the parameter registry the web UI,
//! MQTT and config files address it through.
//!
//! The registry is generated from the single table in [`define_params!`]: field, key,
//! type with limits, default and reboot flag. Labels and help texts live in the UI.
//! Adding a parameter means adding one row.

use core::fmt::Write as _;
use core::str::FromStr;

use heapless::String;
use serde::{Deserialize, Serialize};

/// Bumped whenever [`Config`]'s serialized layout changes. A stored config with another
/// version is discarded (Wi-Fi credentials live in a separate record and survive).
pub const CONFIG_VERSION: u16 = 1;

/// A parameter's value, as read from or written to the registry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value<'a> {
    Bool(bool),
    Int(i32),
    Double(f64),
    Text(&'a str),
    /// The option's numeric value.
    Choice(i32),
}

/// Type and limits of a parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Bool,
    Int { min: i32, max: i32 },
    Double { min: f64, max: f64 },
    Text { max_len: usize, secret: bool },
    Choice { options: &'static [(i32, &'static str)] },
}

/// Wire codes of the kinds, as the web UI expects them (`ParameterTypes` in the UI).
impl Kind {
    pub const fn ui_type(&self) -> u8 {
        match self {
            Kind::Int { .. } => 0,
            Kind::Double { .. } => 2,
            Kind::Text { .. } => 4,
            Kind::Choice { .. } => 5,
            Kind::Bool => 6,
        }
    }
}

/// Static description of one parameter.
#[derive(Debug, Clone, Copy)]
pub struct ParamDef {
    pub key: &'static str,
    pub kind: Kind,
    /// Only read at boot: changing it needs a restart to take effect.
    pub reboot: bool,
}

/// Why a write was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetError {
    UnknownKey,
    WrongType,
    OutOfRange,
    TooLong,
    NotAnOption,
    Unparsable,
}

impl core::fmt::Display for SetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            SetError::UnknownKey => "unknown parameter",
            SetError::WrongType => "wrong value type",
            SetError::OutOfRange => "value out of range",
            SetError::TooLong => "text too long",
            SetError::NotAnOption => "not a valid option",
            SetError::Unparsable => "value cannot be parsed",
        })
    }
}

/// Keys older firmware and the web UI's built-in list know but this firmware dropped.
/// Writes to them succeed and do nothing, so a UI save never fails on them.
pub const RETIRED_KEYS: &[&str] = &["system.ota_password", "system.showdisplay.enabled"];

/// An enum parameter: a closed set of options with stable numeric values.
pub trait Choice: Sized + Copy {
    const OPTIONS: &'static [(i32, &'static str)];
    fn to_i32(self) -> i32;
    fn from_i32(v: i32) -> Option<Self>;
}

macro_rules! choice {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $val:literal => $label:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[repr(i32)]
        pub enum $name { $($variant = $val),+ }

        impl Choice for $name {
            const OPTIONS: &'static [(i32, &'static str)] = &[$(($val, $label)),+];
            fn to_i32(self) -> i32 { self as i32 }
            fn from_i32(v: i32) -> Option<Self> {
                match v { $($val => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

choice!(
    /// How a switch behaves mechanically.
    SwitchType { Momentary = 0 => "Momentary", Toggle = 1 => "Toggle" }
);
choice!(
    /// How a switch is wired electrically.
    SwitchMode { NormallyOpen = 0 => "Normally Open", NormallyClosed = 1 => "Normally Closed" }
);
choice!(
    /// Which GPIO level energises a relay.
    RelayTrigger { Low = 0 => "Low Trigger", High = 1 => "High Trigger" }
);
choice!(DisplayTemplate {
    Standard = 0 => "Standard",
    Minimal = 1 => "Minimal",
    TemperatureOnly = 2 => "Temperature Only",
    Scale = 3 => "Scale",
    Upright = 4 => "Upright",
    Modern = 5 => "Modern",
});
choice!(Language { English = 0 => "English", German = 1 => "German", Spanish = 2 => "Spanish" });
choice!(OledType { Ssd1306 = 0 => "SSD1306", Sh1106 = 1 => "SH1106" });
choice!(OledAddress { X3C = 0 => "0x3C", X3D = 1 => "0x3D" });
choice!(TempSensorType { Tsic306 = 0 => "TSIC 306", Ds18b20 = 1 => "Dallas DS18B20" });
choice!(ScaleType {
    Hx711Dual = 0 => "HX711 (2 load cells)",
    Hx711Single = 1 => "HX711 (1 load cell)",
    Bluetooth = 2 => "Bluetooth",
});
choice!(LogLevel {
    Trace = 0 => "TRACE",
    Debug = 1 => "DEBUG",
    Info = 2 => "INFO",
    Warning = 3 => "WARNING",
    Error = 4 => "ERROR",
    Fatal = 5 => "FATAL",
    Silent = 6 => "SILENT",
});
choice!(
    /// Manual: the switch alone controls the shot. Automatic: pre-infusion and the
    /// stop-by-time/weight targets apply.
    BrewMode { Manual = 0 => "Manual", Automatic = 1 => "Automatic" }
);

impl OledAddress {
    pub const fn i2c_address(self) -> u8 {
        match self {
            OledAddress::X3C => 0x3C,
            OledAddress::X3D => 0x3D,
        }
    }
}

// Per-kind expansion used by `define_params!`.
macro_rules! param_ty {
    (bool) => { bool };
    (int($min:expr, $max:expr)) => { i32 };
    (double($min:expr, $max:expr)) => { f64 };
    (text($len:literal)) => { String<$len> };
    (secret($len:literal)) => { String<$len> };
    (choice($t:ty)) => { $t };
}
macro_rules! param_kind {
    (bool) => {
        Kind::Bool
    };
    (int($min:expr, $max:expr)) => {
        Kind::Int { min: $min, max: $max }
    };
    (double($min:expr, $max:expr)) => {
        Kind::Double { min: $min, max: $max }
    };
    (text($len:literal)) => {
        Kind::Text {
            max_len: $len,
            secret: false,
        }
    };
    (secret($len:literal)) => {
        Kind::Text {
            max_len: $len,
            secret: true,
        }
    };
    (choice($t:ty)) => {
        Kind::Choice {
            options: <$t as Choice>::OPTIONS,
        }
    };
}
macro_rules! param_default {
    (text($len:literal), $d:expr) => {
        text($d)
    };
    (secret($len:literal), $d:expr) => {
        text($d)
    };
    ($k:ident $(($($a:tt)*))?, $d:expr) => {
        $d
    };
}
macro_rules! param_get {
    (bool, $v:expr) => {
        Value::Bool($v)
    };
    (int($min:expr, $max:expr), $v:expr) => {
        Value::Int($v)
    };
    (double($min:expr, $max:expr), $v:expr) => {
        Value::Double($v)
    };
    (text($len:literal), $v:expr) => {
        Value::Text($v.as_str())
    };
    (secret($len:literal), $v:expr) => {
        Value::Text($v.as_str())
    };
    (choice($t:ty), $v:expr) => {
        Value::Choice(Choice::to_i32($v))
    };
}
macro_rules! param_set {
    (bool, $slot:expr, $v:expr) => {{
        match $v {
            Value::Bool(b) => {
                $slot = b;
                Ok(())
            }
            _ => Err(SetError::WrongType),
        }
    }};
    (int($min:expr, $max:expr), $slot:expr, $v:expr) => {{
        match $v {
            Value::Int(i) if ($min..=$max).contains(&i) => {
                $slot = i;
                Ok(())
            }
            Value::Int(_) => Err(SetError::OutOfRange),
            _ => Err(SetError::WrongType),
        }
    }};
    (double($min:expr, $max:expr), $slot:expr, $v:expr) => {{
        match $v {
            Value::Double(d) if d.is_finite() && ($min..=$max).contains(&d) => {
                $slot = d;
                Ok(())
            }
            Value::Int(i) if ($min..=$max).contains(&f64::from(i)) => {
                $slot = f64::from(i);
                Ok(())
            }
            Value::Double(_) | Value::Int(_) => Err(SetError::OutOfRange),
            _ => Err(SetError::WrongType),
        }
    }};
    (text($len:literal), $slot:expr, $v:expr) => {
        set_text(&mut $slot, $v)
    };
    (secret($len:literal), $slot:expr, $v:expr) => {
        set_text(&mut $slot, $v)
    };
    (choice($t:ty), $slot:expr, $v:expr) => {{
        match $v {
            Value::Choice(i) | Value::Int(i) => match <$t as Choice>::from_i32(i) {
                Some(c) => {
                    $slot = c;
                    Ok(())
                }
                None => Err(SetError::NotAnOption),
            },
            _ => Err(SetError::WrongType),
        }
    }};
}

macro_rules! define_params {
    ($(
        $field:ident: $kind:ident $(($($arg:tt)*))? = $default:expr,
        $key:literal $(, reboot = $reboot:literal)?;
    )*) => {
        /// Everything the user can configure. Field names follow the parameter keys.
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct Config {
            $(pub $field: param_ty!($kind $(($($arg)*))?),)*
        }

        impl Default for Config {
            fn default() -> Self {
                Self { $($field: param_default!($kind $(($($arg)*))?, $default),)* }
            }
        }

        /// All parameters, in the order the API lists them.
        pub static PARAMS: &[ParamDef] = &[$(
            ParamDef {
                key: $key,
                kind: param_kind!($kind $(($($arg)*))?),
                reboot: false $(|| $reboot)?,
            },
        )*];

        impl Config {
            /// Value of the parameter at `index` in [`PARAMS`].
            pub fn get(&self, index: usize) -> Value<'_> {
                let mut i = 0usize;
                $(
                    if index == i { return param_get!($kind $(($($arg)*))?, self.$field); }
                    i += 1;
                )*
                let _ = i;
                panic!("parameter index out of range");
            }

            /// Writes the parameter at `index` after validating it.
            pub fn set(&mut self, index: usize, value: Value<'_>) -> Result<(), SetError> {
                let mut i = 0usize;
                $(
                    if index == i { return param_set!($kind $(($($arg)*))?, self.$field, value); }
                    i += 1;
                )*
                let _ = i;
                Err(SetError::UnknownKey)
            }
        }
    };
}

fn text<const N: usize>(s: &str) -> String<N> {
    let mut out = String::new();
    out.push_str(s).expect("default fits");
    out
}

fn set_text<const N: usize>(slot: &mut String<N>, v: Value<'_>) -> Result<(), SetError> {
    let Value::Text(t) = v else {
        return Err(SetError::WrongType);
    };
    let mut s = String::new();
    s.push_str(t).map_err(|_| SetError::TooLong)?;
    *slot = s;
    Ok(())
}

define_params! {
    // === PID ===
    pid_enabled: bool = false, "pid.enabled";
    pid_use_ponm: bool = false, "pid.use_ponm";
    pid_ema_factor: double(0.0, 1.0) = 0.6, "pid.ema_factor";
    pid_regular_kp: double(0.0, 200.0) = 62.0, "pid.regular.kp";
    pid_regular_tn: double(0.0, 200.0) = 52.0, "pid.regular.tn";
    pid_regular_tv: double(0.0, 200.0) = 11.5, "pid.regular.tv";
    pid_regular_i_max: double(0.0, 100.0) = 55.0, "pid.regular.i_max";
    pid_steam_kp: double(0.0, 500.0) = 150.0, "pid.steam.kp";

    // === Brew temperature and safety ===
    brew_setpoint: double(20.0, 110.0) = 95.0, "brew.setpoint";
    brew_temp_offset: double(0.0, 20.0) = 0.0, "brew.temp_offset";
    steam_setpoint: double(100.0, 140.0) = 120.0, "steam.setpoint";
    safety_emergency_temp: double(120.0, 180.0) = 150.0, "safety.emergency_temp";
    safety_emergency_hysteresis: double(1.0, 15.0) = 5.0, "safety.emergency_hysteresis";

    // === Brew detection PID ===
    pid_bd_enabled: bool = false, "pid.bd.enabled";
    brew_pid_delay: double(0.0, 60.0) = 10.0, "brew.pid_delay";
    pid_bd_kp: double(0.0, 200.0) = 50.0, "pid.bd.kp";
    pid_bd_tn: double(0.0, 200.0) = 0.0, "pid.bd.tn";
    pid_bd_tv: double(0.0, 200.0) = 20.0, "pid.bd.tv";

    // === Brewing ===
    brew_mode: choice(BrewMode) = BrewMode::Manual, "brew.mode";
    brew_by_time_enabled: bool = false, "brew.by_time.enabled";
    brew_by_time_target_time: double(1.0, 120.0) = 25.0, "brew.by_time.target_time";
    brew_by_weight_enabled: bool = false, "brew.by_weight.enabled";
    brew_by_weight_target_weight: double(0.0, 500.0) = 36.0, "brew.by_weight.target_weight";
    brew_by_weight_auto_tare: bool = false, "brew.by_weight.auto_tare";
    brew_pre_infusion_enabled: bool = false, "brew.pre_infusion.enabled";
    brew_pre_infusion_time: double(0.0, 60.0) = 2.0, "brew.pre_infusion.time";
    brew_pre_infusion_pause: double(0.0, 60.0) = 5.0, "brew.pre_infusion.pause";
    display_fullscreen_brew_timer: bool = false, "display.fullscreen_brew_timer";
    display_fullscreen_manual_flush_timer: bool = false, "display.fullscreen_manual_flush_timer";
    display_fullscreen_hot_water_timer: bool = false, "display.fullscreen_hot_water_timer";
    display_post_brew_timer_duration: double(0.0, 60.0) = 3.0, "display.post_brew_timer_duration";
    display_heating_logo: bool = true, "display.heating_logo";
    display_pid_off_logo: bool = true, "display.pid_off_logo";

    // === LEDs and sensors shown under hardware ===
    hardware_leds_status_enabled: bool = false, "hardware.leds.status.enabled", reboot = true;
    hardware_leds_status_inverted: bool = false, "hardware.leds.status.inverted", reboot = true;
    hardware_leds_brew_enabled: bool = false, "hardware.leds.brew.enabled", reboot = true;
    hardware_leds_brew_inverted: bool = false, "hardware.leds.brew.inverted", reboot = true;
    hardware_leds_steam_enabled: bool = false, "hardware.leds.steam.enabled", reboot = true;
    hardware_leds_steam_inverted: bool = false, "hardware.leds.steam.inverted", reboot = true;

    // === Display ===
    display_template: choice(DisplayTemplate) = DisplayTemplate::Standard, "display.template";
    display_inverted: bool = false, "display.inverted", reboot = true;
    display_language: choice(Language) = Language::English, "display.language";
    display_blinking_delta: double(0.2, 10.0) = 0.3, "display.blinking.delta";

    // === Backflush and maintenance ===
    backflush_cycles: int(2, 20) = 5, "backflush.cycles";
    backflush_fill_time: double(3.0, 10.0) = 5.0, "backflush.fill_time";
    backflush_flush_time: double(5.0, 20.0) = 10.0, "backflush.flush_time";
    maintenance_backflush_reminder_enabled: bool = true, "maintenance.backflush_reminder.enabled";
    maintenance_backflush_reminder_threshold: int(1, 500) = 50, "maintenance.backflush_reminder.threshold";

    // === Standby ===
    standby_enabled: bool = false, "standby.enabled";
    standby_time: double(1.0, 120.0) = 35.0, "standby.time";

    // === MQTT ===
    mqtt_enabled: bool = false, "mqtt.enabled", reboot = true;
    mqtt_broker: text(64) = "", "mqtt.broker", reboot = true;
    mqtt_port: int(1, 65535) = 1883, "mqtt.port", reboot = true;
    mqtt_username: text(32) = "rancilio", "mqtt.username", reboot = true;
    mqtt_password: secret(64) = "silvia", "mqtt.password", reboot = true;
    mqtt_topic: text(48) = "custom/kitchen/", "mqtt.topic", reboot = true;
    mqtt_hassio_enabled: bool = false, "mqtt.hassio.enabled", reboot = true;
    mqtt_hassio_prefix: text(24) = "homeassistant", "mqtt.hassio.prefix", reboot = true;

    // === System ===
    system_hostname: text(32) = "silvia", "system.hostname", reboot = true;
    system_offline_mode: bool = false, "system.offline_mode", reboot = true;
    system_log_level: choice(LogLevel) = LogLevel::Info, "system.log_level";
    system_auth_enabled: bool = false, "system.auth.enabled";
    system_auth_username: text(32) = "admin", "system.auth.username";
    system_auth_password: secret(64) = "admin", "system.auth.password";
    system_wifi_ssid: text(32) = "", "system.wifi.ssid", reboot = true;
    system_wifi_password: secret(64) = "", "system.wifi.password", reboot = true;
    system_timing_debug_enabled: bool = false, "system.timing_debug.enabled";

    // === OLED ===
    hardware_oled_enabled: bool = true, "hardware.oled.enabled", reboot = true;
    hardware_oled_type: choice(OledType) = OledType::Ssd1306, "hardware.oled.type", reboot = true;
    hardware_oled_address: choice(OledAddress) = OledAddress::X3C, "hardware.oled.address", reboot = true;

    // === Relays ===
    hardware_relays_heater_trigger_type: choice(RelayTrigger) = RelayTrigger::High, "hardware.relays.heater.trigger_type", reboot = true;
    hardware_relays_valve_trigger_type: choice(RelayTrigger) = RelayTrigger::High, "hardware.relays.valve.trigger_type", reboot = true;
    hardware_relays_pump_trigger_type: choice(RelayTrigger) = RelayTrigger::High, "hardware.relays.pump.trigger_type", reboot = true;

    // === Switches ===
    hardware_switches_brew_enabled: bool = false, "hardware.switches.brew.enabled", reboot = true;
    hardware_switches_brew_type: choice(SwitchType) = SwitchType::Toggle, "hardware.switches.brew.type", reboot = true;
    hardware_switches_brew_mode: choice(SwitchMode) = SwitchMode::NormallyOpen, "hardware.switches.brew.mode", reboot = true;
    hardware_switches_steam_enabled: bool = false, "hardware.switches.steam.enabled", reboot = true;
    hardware_switches_steam_type: choice(SwitchType) = SwitchType::Toggle, "hardware.switches.steam.type", reboot = true;
    hardware_switches_steam_mode: choice(SwitchMode) = SwitchMode::NormallyOpen, "hardware.switches.steam.mode", reboot = true;
    hardware_switches_power_enabled: bool = false, "hardware.switches.power.enabled", reboot = true;
    hardware_switches_power_type: choice(SwitchType) = SwitchType::Toggle, "hardware.switches.power.type", reboot = true;
    hardware_switches_power_mode: choice(SwitchMode) = SwitchMode::NormallyOpen, "hardware.switches.power.mode", reboot = true;
    hardware_switches_hot_water_enabled: bool = false, "hardware.switches.hot_water.enabled", reboot = true;
    hardware_switches_hot_water_type: choice(SwitchType) = SwitchType::Toggle, "hardware.switches.hot_water.type", reboot = true;
    hardware_switches_hot_water_mode: choice(SwitchMode) = SwitchMode::NormallyOpen, "hardware.switches.hot_water.mode", reboot = true;

    // === Sensors ===
    hardware_sensors_temperature_type: choice(TempSensorType) = TempSensorType::Tsic306, "hardware.sensors.temperature.type", reboot = true;
    hardware_sensors_pressure_enabled: bool = false, "hardware.sensors.pressure.enabled", reboot = true;
    hardware_sensors_watertank_enabled: bool = false, "hardware.sensors.watertank.enabled", reboot = true;
    hardware_sensors_watertank_mode: choice(SwitchMode) = SwitchMode::NormallyClosed, "hardware.sensors.watertank.mode", reboot = true;
    hardware_sensors_watertank_keep_heater_on_empty: bool = false, "hardware.sensors.watertank.keep_heater_on_empty";
    hardware_sensors_scale_enabled: bool = false, "hardware.sensors.scale.enabled", reboot = true;
    hardware_sensors_scale_samples: int(1, 20) = 2, "hardware.sensors.scale.samples";
    hardware_sensors_scale_type: choice(ScaleType) = ScaleType::Hx711Dual, "hardware.sensors.scale.type", reboot = true;
    hardware_sensors_scale_calibration: double(-999_999.0, 999_999.0) = 1.0, "hardware.sensors.scale.calibration";
    hardware_sensors_scale_calibration2: double(-999_999.0, 999_999.0) = 1.0, "hardware.sensors.scale.calibration2";
    hardware_sensors_scale_known_weight: double(1.0, 2000.0) = 267.0, "hardware.sensors.scale.known_weight";
}

/// Index of `key` in [`PARAMS`].
pub fn find(key: &str) -> Option<usize> {
    PARAMS.iter().position(|p| p.key == key)
}

impl Config {
    /// Writes a parameter from its textual form (form posts, MQTT payloads, files).
    ///
    /// Bools accept `true/false/1/0/on/off`; enums accept the numeric value or the
    /// label; ints accept integral numbers (`5` or `5.0`). Retired keys are accepted
    /// and ignored (`Ok(false)`); a real change returns `Ok(true)`.
    pub fn set_from_str(&mut self, key: &str, raw: &str) -> Result<bool, SetError> {
        if RETIRED_KEYS.contains(&key) {
            return Ok(false);
        }
        let index = find(key).ok_or(SetError::UnknownKey)?;
        let raw = raw.trim();
        let value = match PARAMS[index].kind {
            Kind::Bool => Value::Bool(parse_bool(raw)?),
            Kind::Int { .. } => Value::Int(parse_int(raw)?),
            Kind::Double { .. } => Value::Double(f64::from_str(raw).map_err(|_| SetError::Unparsable)?),
            Kind::Text { .. } => Value::Text(raw),
            Kind::Choice { options } => Value::Choice(match parse_int(raw) {
                Ok(i) => i,
                Err(_) => options
                    .iter()
                    .find(|(_, label)| label.eq_ignore_ascii_case(raw))
                    .map(|(v, _)| *v)
                    .ok_or(SetError::NotAnOption)?,
            }),
        };
        let before = self.get(index) == value;
        self.set(index, value)?;
        Ok(!before)
    }

    /// The parameter formatted the way MQTT and `/api/config` text fields carry it.
    pub fn format_value(&self, index: usize, out: &mut impl core::fmt::Write) -> core::fmt::Result {
        match self.get(index) {
            Value::Bool(b) => out.write_str(if b { "1" } else { "0" }),
            Value::Int(i) | Value::Choice(i) => write!(out, "{i}"),
            Value::Double(d) => write!(out, "{d:.2}"),
            Value::Text(t) => out.write_str(t),
        }
    }

    /// Ki and Kd derived from the regular Kp/Tn/Tv, for display.
    pub fn regular_ki_kd(&self) -> (f64, f64) {
        let t = cc_pid::Tunings::from_kp_tn_tv(
            self.pid_regular_kp,
            self.pid_regular_tn,
            self.pid_regular_tv,
            cc_pid::Proportional::OnError,
        );
        (t.ki, t.kd)
    }
}

fn parse_bool(raw: &str) -> Result<bool, SetError> {
    match raw {
        "1" | "on" | "ON" => Ok(true),
        "0" | "off" | "OFF" => Ok(false),
        r if r.eq_ignore_ascii_case("true") => Ok(true),
        r if r.eq_ignore_ascii_case("false") => Ok(false),
        _ => Err(SetError::Unparsable),
    }
}

fn parse_int(raw: &str) -> Result<i32, SetError> {
    if let Ok(i) = i32::from_str(raw) {
        return Ok(i);
    }
    let d = f64::from_str(raw).map_err(|_| SetError::Unparsable)?;
    if d >= f64::from(i32::MIN) && d <= f64::from(i32::MAX) && f64::from(d as i32) == d {
        Ok(d as i32)
    } else {
        Err(SetError::Unparsable)
    }
}

/// Short text used for log lines and display messages.
pub type ShortText = String<64>;

/// Writes a `ShortText`, truncating silently.
pub fn short_text(args: core::fmt::Arguments<'_>) -> ShortText {
    let mut s = ShortText::new();
    let _ = s.write_fmt(args);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique() {
        for (i, a) in PARAMS.iter().enumerate() {
            for b in &PARAMS[i + 1..] {
                assert_ne!(a.key, b.key);
            }
        }
    }

    #[test]
    fn stored_values_round_trip() {
        let c = Config {
            pid_ema_factor: 0.012_345,
            system_hostname: "bench-1".try_into().unwrap(),
            ..Config::default()
        };
        let mut buf = [0u8; 128];
        for (i, p) in PARAMS.iter().enumerate() {
            let v = c.get(i);
            let n = stored::encode(&v, &mut buf).unwrap();
            assert_eq!(stored::decode(p.kind, &buf[..n]), Some(v), "{}", p.key);
        }
        assert_eq!(stored::decode(Kind::Bool, &[1, 0]), None);
        assert_eq!(stored::decode(Kind::Double { min: 0.0, max: 1.0 }, &[0; 4]), None);
    }

    #[test]
    fn every_default_is_valid() {
        let mut c = Config::default();
        let defaults = Config::default();
        for (i, p) in PARAMS.iter().enumerate() {
            let v = defaults.get(i);
            assert_eq!(c.set(i, v), Ok(()), "{}", p.key);
        }
    }

    #[test]
    fn defaults_match_the_cpp_firmware() {
        let c = Config::default();
        assert_eq!(c.brew_setpoint, 95.0);
        assert_eq!(c.pid_regular_kp, 62.0);
        assert_eq!(c.backflush_cycles, 5);
        assert_eq!(c.system_hostname.as_str(), "silvia");
        assert_eq!(c.hardware_sensors_watertank_mode, SwitchMode::NormallyClosed);
        assert_eq!(PARAMS.len(), 96);
    }

    #[test]
    fn set_from_str_parses_each_kind() {
        let mut c = Config::default();
        assert_eq!(c.set_from_str("pid.enabled", "true"), Ok(true));
        assert!(c.pid_enabled);
        assert_eq!(c.set_from_str("pid.enabled", "1"), Ok(false));
        assert_eq!(c.set_from_str("brew.setpoint", "93.5"), Ok(true));
        assert_eq!(c.brew_setpoint, 93.5);
        assert_eq!(c.set_from_str("backflush.cycles", "7.0"), Ok(true));
        assert_eq!(c.backflush_cycles, 7);
        assert_eq!(c.set_from_str("brew.mode", "Automatic"), Ok(true));
        assert_eq!(c.brew_mode, BrewMode::Automatic);
        assert_eq!(c.set_from_str("display.template", "5"), Ok(true));
        assert_eq!(c.display_template, DisplayTemplate::Modern);
        assert_eq!(c.set_from_str("mqtt.broker", "10.0.0.2"), Ok(true));
        assert_eq!(c.mqtt_broker.as_str(), "10.0.0.2");
    }

    #[test]
    fn set_from_str_rejects_bad_values() {
        let mut c = Config::default();
        assert_eq!(c.set_from_str("brew.setpoint", "200"), Err(SetError::OutOfRange));
        assert_eq!(c.set_from_str("brew.setpoint", "hot"), Err(SetError::Unparsable));
        assert_eq!(
            c.set_from_str("pid.enabled", "yes please"),
            Err(SetError::Unparsable)
        );
        assert_eq!(
            c.set_from_str("display.template", "9"),
            Err(SetError::NotAnOption)
        );
        assert_eq!(
            c.set_from_str("backflush.cycles", "2.5"),
            Err(SetError::Unparsable)
        );
        assert_eq!(c.set_from_str("nope", "1"), Err(SetError::UnknownKey));
        let long = "x".repeat(40);
        assert_eq!(c.set_from_str("system.hostname", &long), Err(SetError::TooLong));
        assert_eq!(c, Config::default());
    }

    #[test]
    fn retired_keys_are_accepted_and_ignored() {
        let mut c = Config::default();
        assert_eq!(c.set_from_str("system.ota_password", "x"), Ok(false));
        assert_eq!(c, Config::default());
    }

    #[test]
    fn config_round_trips_through_postcard() {
        let mut c = Config::default();
        c.set_from_str("mqtt.broker", "broker.local").unwrap();
        let mut buf = [0u8; 2048];
        let bytes = postcard::to_slice(&c, &mut buf).unwrap();
        let back: Config = postcard::from_bytes(bytes).unwrap();
        assert_eq!(back, c);
    }
}

/// Binary form of a parameter value for flash storage: exact (unlike
/// [`Config::format_value`], which rounds doubles) and independent of the
/// parameter's position, so adding or removing parameters keeps stored values.
pub mod stored {
    use super::{Kind, Value};

    /// Writes `v` into `out`, returning the used length.
    pub fn encode(v: &Value<'_>, out: &mut [u8]) -> Option<usize> {
        let bytes: &[u8] = match v {
            Value::Bool(b) => &[u8::from(*b)],
            Value::Int(i) | Value::Choice(i) => &i.to_le_bytes(),
            Value::Double(d) => &d.to_le_bytes(),
            Value::Text(t) => t.as_bytes(),
        };
        out.get_mut(..bytes.len())?.copy_from_slice(bytes);
        Some(bytes.len())
    }

    /// Reads a value of `kind`; `None` when the bytes do not fit the kind (the
    /// parameter changed type since it was stored).
    pub fn decode(kind: Kind, b: &[u8]) -> Option<Value<'_>> {
        Some(match kind {
            Kind::Bool => match b {
                [x] => Value::Bool(*x != 0),
                _ => return None,
            },
            Kind::Int { .. } => Value::Int(i32::from_le_bytes(b.try_into().ok()?)),
            Kind::Choice { .. } => Value::Choice(i32::from_le_bytes(b.try_into().ok()?)),
            Kind::Double { .. } => Value::Double(f64::from_le_bytes(b.try_into().ok()?)),
            Kind::Text { .. } => Value::Text(core::str::from_utf8(b).ok()?),
        })
    }
}
