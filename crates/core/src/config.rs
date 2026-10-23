//! User configuration: one typed [`Config`] plus the parameter registry the web UI,
//! MQTT and config files address it through.
//!
//! The registry is generated from the single table in [`define_params!`]: field, key,
//! type with limits, default, and the UI metadata (label, section, order, help). Adding
//! a parameter means adding one row.

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
    pub label: &'static str,
    pub section: u8,
    pub order: u16,
    pub help: &'static str,
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
    (bool) => { Kind::Bool };
    (int($min:expr, $max:expr)) => { Kind::Int { min: $min, max: $max } };
    (double($min:expr, $max:expr)) => { Kind::Double { min: $min, max: $max } };
    (text($len:literal)) => { Kind::Text { max_len: $len, secret: false } };
    (secret($len:literal)) => { Kind::Text { max_len: $len, secret: true } };
    (choice($t:ty)) => { Kind::Choice { options: <$t as Choice>::OPTIONS } };
}
macro_rules! param_default {
    (text($len:literal), $d:expr) => { text($d) };
    (secret($len:literal), $d:expr) => { text($d) };
    ($k:ident $(($($a:tt)*))?, $d:expr) => { $d };
}
macro_rules! param_get {
    (bool, $v:expr) => { Value::Bool($v) };
    (int($min:expr, $max:expr), $v:expr) => { Value::Int($v) };
    (double($min:expr, $max:expr), $v:expr) => { Value::Double($v) };
    (text($len:literal), $v:expr) => { Value::Text($v.as_str()) };
    (secret($len:literal), $v:expr) => { Value::Text($v.as_str()) };
    (choice($t:ty), $v:expr) => { Value::Choice(Choice::to_i32($v)) };
}
macro_rules! param_set {
    (bool, $slot:expr, $v:expr) => {{
        match $v { Value::Bool(b) => { $slot = b; Ok(()) } _ => Err(SetError::WrongType) }
    }};
    (int($min:expr, $max:expr), $slot:expr, $v:expr) => {{
        match $v {
            Value::Int(i) if ($min..=$max).contains(&i) => { $slot = i; Ok(()) }
            Value::Int(_) => Err(SetError::OutOfRange),
            _ => Err(SetError::WrongType),
        }
    }};
    (double($min:expr, $max:expr), $slot:expr, $v:expr) => {{
        match $v {
            Value::Double(d) if d.is_finite() && d >= $min && d <= $max => { $slot = d; Ok(()) }
            Value::Int(i) if f64::from(i) >= $min && f64::from(i) <= $max => { $slot = f64::from(i); Ok(()) }
            Value::Double(_) | Value::Int(_) => Err(SetError::OutOfRange),
            _ => Err(SetError::WrongType),
        }
    }};
    (text($len:literal), $slot:expr, $v:expr) => { set_text(&mut $slot, $v) };
    (secret($len:literal), $slot:expr, $v:expr) => { set_text(&mut $slot, $v) };
    (choice($t:ty), $slot:expr, $v:expr) => {{
        match $v {
            Value::Choice(i) | Value::Int(i) => match <$t as Choice>::from_i32(i) {
                Some(c) => { $slot = c; Ok(()) }
                None => Err(SetError::NotAnOption),
            },
            _ => Err(SetError::WrongType),
        }
    }};
}

macro_rules! define_params {
    ($(
        $field:ident: $kind:ident $(($($arg:tt)*))? = $default:expr,
        $key:literal, $label:literal, $section:literal, $order:literal $(, reboot = $reboot:literal)?;
        $help:literal;
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
                label: $label,
                section: $section,
                order: $order,
                help: $help,
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
    let Value::Text(t) = v else { return Err(SetError::WrongType) };
    let mut s = String::new();
    s.push_str(t).map_err(|_| SetError::TooLong)?;
    *slot = s;
    Ok(())
}

define_params! {
    // === PID (section 0) ===
    pid_enabled: bool = false, "pid.enabled", "Enable PID Controller", 0, 101;
        "Enables or disables the PID temperature controller";
    pid_use_ponm: bool = false, "pid.use_ponm", "Enable PonM", 0, 102;
        "Use PonM mode (Proportional on Measurement)";
    pid_ema_factor: double(0.0, 1.0) = 0.6, "pid.ema_factor", "PID EMA Factor", 0, 111;
        "Smoothing of input for derivative component. Smaller = less smoothing but less delay";
    pid_regular_kp: double(0.0, 200.0) = 62.0, "pid.regular.kp", "PID Kp", 0, 112;
        "Proportional gain (in Watts/°C) for the main PID controller";
    pid_regular_tn: double(0.0, 200.0) = 52.0, "pid.regular.tn", "PID Tn", 0, 113;
        "Integral time constant (in seconds) for the main PID controller";
    pid_regular_tv: double(0.0, 200.0) = 11.5, "pid.regular.tv", "PID Tv", 0, 114;
        "Differential time constant (in seconds) for the main PID controller";
    pid_regular_i_max: double(0.0, 100.0) = 55.0, "pid.regular.i_max", "PID Integrator Max", 0, 115;
        "Internal integrator limit to prevent windup (in Watts)";
    pid_steam_kp: double(0.0, 500.0) = 150.0, "pid.steam.kp", "Steam Kp", 0, 116;
        "Proportional gain for the steaming mode";

    // === Brew temperature and safety (section 1) ===
    brew_setpoint: double(20.0, 110.0) = 95.0, "brew.setpoint", "Setpoint (°C)", 1, 201;
        "The temperature that the PID will attempt to reach and hold";
    brew_temp_offset: double(0.0, 20.0) = 0.0, "brew.temp_offset", "Offset (°C)", 1, 202;
        "Optional offset added to the user-visible setpoint to compensate sensor offsets";
    steam_setpoint: double(100.0, 140.0) = 120.0, "steam.setpoint", "Steam Setpoint (°C)", 1, 203;
        "The temperature that the PID will use for steam mode";
    safety_emergency_temp: double(120.0, 180.0) = 150.0, "safety.emergency_temp", "Emergency Temperature (°C)", 1, 204;
        "Temperature threshold that triggers emergency stop";
    safety_emergency_hysteresis: double(1.0, 15.0) = 5.0, "safety.emergency_hysteresis", "Emergency Hysteresis (°C)", 1, 205;
        "Temperature drop required to reset emergency counter";

    // === Brew detection PID (section 2) ===
    pid_bd_enabled: bool = false, "pid.bd.enabled", "Enable Brew PID", 2, 701;
        "Use separate PID parameters while brew is running";
    brew_pid_delay: double(0.0, 60.0) = 10.0, "brew.pid_delay", "Brew PID Delay (s)", 2, 711;
        "Delay time during which PID will be disabled once brew is detected";
    pid_bd_kp: double(0.0, 200.0) = 50.0, "pid.bd.kp", "BD Kp", 2, 712;
        "Proportional gain for PID when brewing has been detected";
    pid_bd_tn: double(0.0, 200.0) = 0.0, "pid.bd.tn", "BD Tn", 2, 713;
        "Integral time constant for PID when brewing has been detected";
    pid_bd_tv: double(0.0, 200.0) = 20.0, "pid.bd.tv", "BD Tv", 2, 714;
        "Differential time constant for PID when brewing has been detected";

    // === Brewing (section 3) ===
    brew_mode: choice(BrewMode) = BrewMode::Manual, "brew.mode", "Brew Mode", 3, 310;
        "Brewing mode selection";
    brew_by_time_enabled: bool = false, "brew.by_time.enabled", "Brew by Time", 3, 311;
        "Enable brewing by time control";
    brew_by_time_target_time: double(1.0, 120.0) = 25.0, "brew.by_time.target_time", "Target Brew Time (s)", 3, 301;
        "Target brew time in seconds";
    brew_by_weight_enabled: bool = false, "brew.by_weight.enabled", "Brew by Weight", 3, 321;
        "Enable brewing by weight control";
    brew_by_weight_target_weight: double(0.0, 500.0) = 36.0, "brew.by_weight.target_weight", "Target Brew Weight (g)", 3, 322;
        "Brew is running until this weight has been measured";
    brew_by_weight_auto_tare: bool = false, "brew.by_weight.auto_tare", "Auto-tare", 3, 323;
        "Automatically tare scale before brewing";
    brew_pre_infusion_enabled: bool = false, "brew.pre_infusion.enabled", "Pre-Infusion", 3, 304;
        "Enable pre-infusion phase";
    brew_pre_infusion_time: double(0.0, 60.0) = 2.0, "brew.pre_infusion.time", "Preinfusion Time (s)", 3, 302;
        "Pre-infusion time in seconds";
    brew_pre_infusion_pause: double(0.0, 60.0) = 5.0, "brew.pre_infusion.pause", "Preinfusion Pause (s)", 3, 303;
        "Pre-infusion pause time in seconds";
    display_fullscreen_brew_timer: bool = false, "display.fullscreen_brew_timer", "Enable Fullscreen Brew Timer", 3, 904;
        "Enable fullscreen overlay during brew";
    display_fullscreen_manual_flush_timer: bool = false, "display.fullscreen_manual_flush_timer", "Enable Fullscreen Manual Flush Timer", 3, 905;
        "Enable fullscreen overlay during manual flush";
    display_fullscreen_hot_water_timer: bool = false, "display.fullscreen_hot_water_timer", "Enable Fullscreen Hot Water Timer", 3, 906;
        "Enable fullscreen overlay during hot water mode";
    display_post_brew_timer_duration: double(0.0, 60.0) = 3.0, "display.post_brew_timer_duration", "Post Brew Timer Duration (s)", 3, 907;
        "Post brew timer will be shown for this many seconds after brew finished";
    display_heating_logo: bool = true, "display.heating_logo", "Enable Heating Logo", 3, 908;
        "Full screen logo will be shown if temperature is 5°C below setpoint";
    display_pid_off_logo: bool = true, "display.pid_off_logo", "Enable 'PID Disabled' Logo", 3, 909;
        "Full screen logo will be shown if PID is disabled";

    // === LEDs and sensors shown under hardware (section 4) ===
    hardware_leds_status_enabled: bool = false, "hardware.leds.status.enabled", "Enable Status LED", 4, 2301, reboot = true;
        "Enable status indicator LED";
    hardware_leds_status_inverted: bool = false, "hardware.leds.status.inverted", "Invert Status LED", 4, 2302, reboot = true;
        "Invert the status LED logic (for common anode LEDs)";
    hardware_leds_brew_enabled: bool = false, "hardware.leds.brew.enabled", "Enable Brew LED", 4, 2311, reboot = true;
        "Enable brew indicator LED";
    hardware_leds_brew_inverted: bool = false, "hardware.leds.brew.inverted", "Invert Brew LED", 4, 2312, reboot = true;
        "Invert the brew LED logic";
    hardware_leds_steam_enabled: bool = false, "hardware.leds.steam.enabled", "Enable Steam LED", 4, 2321, reboot = true;
        "Enable steam indicator LED";
    hardware_leds_steam_inverted: bool = false, "hardware.leds.steam.inverted", "Invert Steam LED", 4, 2322, reboot = true;
        "Invert the steam LED logic";

    // === Display (section 5) ===
    display_template: choice(DisplayTemplate) = DisplayTemplate::Standard, "display.template", "Display Template", 5, 901;
        "Set the display template";
    display_inverted: bool = false, "display.inverted", "Invert Display", 5, 902, reboot = true;
        "Set the display rotation, changes require a reboot";
    display_language: choice(Language) = Language::English, "display.language", "Display Language", 5, 903;
        "Set the language for the OLED display";
    display_blinking_delta: double(0.2, 10.0) = 0.3, "display.blinking.delta", "Status LED Delta", 5, 910;
        "Delta from setpoint for status LED and blinking temperature display";

    // === Backflush and maintenance (section 6) ===
    backflush_cycles: int(2, 20) = 5, "backflush.cycles", "Backflush Cycles", 6, 401;
        "Number of backflush cycles to perform";
    backflush_fill_time: double(3.0, 10.0) = 5.0, "backflush.fill_time", "Backflush Fill Time (s)", 6, 402;
        "Time to fill during backflush cycle";
    backflush_flush_time: double(5.0, 20.0) = 10.0, "backflush.flush_time", "Backflush Flush Time (s)", 6, 403;
        "Time to flush during backflush cycle";
    maintenance_backflush_reminder_enabled: bool = true, "maintenance.backflush_reminder.enabled", "Backflush Reminder", 6, 404;
        "Show a reminder when the shot count since last backflush reaches the threshold";
    maintenance_backflush_reminder_threshold: int(1, 500) = 50, "maintenance.backflush_reminder.threshold", "Backflush Reminder Threshold", 6, 405;
        "Number of counted brews before a backflush reminder is shown (default ~monthly at 2 shots/day)";

    // === Standby (section 7) ===
    standby_enabled: bool = false, "standby.enabled", "Enable Standby Timer", 7, 801;
        "Turn heater off after standby time has elapsed";
    standby_time: double(1.0, 120.0) = 35.0, "standby.time", "Standby Time", 7, 802;
        "Time in minutes until the heater is turned off";

    // === MQTT (section 8) ===
    mqtt_enabled: bool = false, "mqtt.enabled", "MQTT Enabled", 8, 1001, reboot = true;
        "Enables MQTT, change requires a restart";
    mqtt_broker: text(64) = "", "mqtt.broker", "MQTT Broker", 8, 1011, reboot = true;
        "IP address or hostname of your MQTT broker";
    mqtt_port: int(1, 65535) = 1883, "mqtt.port", "MQTT Port", 8, 1012, reboot = true;
        "Port number of your MQTT broker";
    mqtt_username: text(32) = "rancilio", "mqtt.username", "Username", 8, 1013, reboot = true;
        "Username for your MQTT broker";
    mqtt_password: secret(64) = "silvia", "mqtt.password", "Password", 8, 1014, reboot = true;
        "Password for your MQTT broker";
    mqtt_topic: text(48) = "custom/kitchen/", "mqtt.topic", "Topic Prefix", 8, 1015, reboot = true;
        "Custom MQTT topic prefix";
    mqtt_hassio_enabled: bool = false, "mqtt.hassio.enabled", "Hass.io enabled", 8, 1021, reboot = true;
        "Enables Home Assistant integration";
    mqtt_hassio_prefix: text(24) = "homeassistant", "mqtt.hassio.prefix", "Hass.io Prefix", 8, 1022, reboot = true;
        "Custom MQTT topic prefix for Home Assistant";

    // === System (section 9) ===
    system_hostname: text(32) = "silvia", "system.hostname", "Hostname", 9, 1101, reboot = true;
        "Hostname of your machine, changes require a restart";
    system_offline_mode: bool = false, "system.offline_mode", "Offline Mode", 9, 1102, reboot = true;
        "Run in offline mode without WiFi connection";
    system_log_level: choice(LogLevel) = LogLevel::Info, "system.log_level", "Log Level", 9, 1103;
        "Set the logging level for debug output";
    system_auth_enabled: bool = false, "system.auth.enabled", "Enable Authentication", 9, 1201;
        "Enables authentication for accessing certain parts of the website";
    system_auth_username: text(32) = "admin", "system.auth.username", "Website Username", 9, 1202;
        "Username for accessing the website and authenticating web requests";
    system_auth_password: secret(64) = "admin", "system.auth.password", "Website Password", 9, 1203;
        "Password for accessing the website and authenticating web requests";
    system_wifi_ssid: text(32) = "", "system.wifi.ssid", "WiFi SSID", 9, 1204, reboot = true;
        "WiFi SSID to connect to directly (leave empty to use the configuration portal)";
    system_wifi_password: secret(64) = "", "system.wifi.password", "WiFi Password", 9, 1205, reboot = true;
        "WiFi password for direct connection (leave empty for open networks)";
    system_timing_debug_enabled: bool = false, "system.timing_debug.enabled", "Loop timing in console", 9, 1301;
        "Log the control loop timing to the console";

    // === OLED (section 11) ===
    hardware_oled_enabled: bool = true, "hardware.oled.enabled", "Enable OLED Display", 11, 2001, reboot = true;
        "Enable or disable the OLED display";
    hardware_oled_type: choice(OledType) = OledType::Ssd1306, "hardware.oled.type", "OLED Type", 11, 2002, reboot = true;
        "Select your OLED display type";
    hardware_oled_address: choice(OledAddress) = OledAddress::X3C, "hardware.oled.address", "I2C Address", 11, 2003, reboot = true;
        "I2C address of the OLED display";

    // === Relays (section 12) ===
    hardware_relays_heater_trigger_type: choice(RelayTrigger) = RelayTrigger::High, "hardware.relays.heater.trigger_type", "Heater Relay Trigger Type", 12, 2101, reboot = true;
        "Relay trigger type for heater control";
    hardware_relays_valve_trigger_type: choice(RelayTrigger) = RelayTrigger::High, "hardware.relays.valve.trigger_type", "Valve Relay Trigger Type", 12, 2102, reboot = true;
        "Relay trigger type for valve control";
    hardware_relays_pump_trigger_type: choice(RelayTrigger) = RelayTrigger::High, "hardware.relays.pump.trigger_type", "Pump Relay Trigger Type", 12, 2103, reboot = true;
        "Relay trigger type for pump control";

    // === Switches (section 13) ===
    hardware_switches_brew_enabled: bool = false, "hardware.switches.brew.enabled", "Enable Brew Switch", 13, 2201, reboot = true;
        "Enable physical brew switch";
    hardware_switches_brew_type: choice(SwitchType) = SwitchType::Toggle, "hardware.switches.brew.type", "Brew Switch Type", 13, 2202, reboot = true;
        "Type of brew switch connected";
    hardware_switches_brew_mode: choice(SwitchMode) = SwitchMode::NormallyOpen, "hardware.switches.brew.mode", "Brew Switch Mode", 13, 2203, reboot = true;
        "Electrical configuration of brew switch";
    hardware_switches_steam_enabled: bool = false, "hardware.switches.steam.enabled", "Enable Steam Switch", 13, 2211, reboot = true;
        "Enable physical steam switch";
    hardware_switches_steam_type: choice(SwitchType) = SwitchType::Toggle, "hardware.switches.steam.type", "Steam Switch Type", 13, 2212, reboot = true;
        "Type of steam switch connected";
    hardware_switches_steam_mode: choice(SwitchMode) = SwitchMode::NormallyOpen, "hardware.switches.steam.mode", "Steam Switch Mode", 13, 2213, reboot = true;
        "Electrical configuration of steam switch";
    hardware_switches_power_enabled: bool = false, "hardware.switches.power.enabled", "Enable Power Switch", 13, 2221, reboot = true;
        "Enable physical power switch";
    hardware_switches_power_type: choice(SwitchType) = SwitchType::Toggle, "hardware.switches.power.type", "Power Switch Type", 13, 2222, reboot = true;
        "Type of power switch connected";
    hardware_switches_power_mode: choice(SwitchMode) = SwitchMode::NormallyOpen, "hardware.switches.power.mode", "Power Switch Mode", 13, 2223, reboot = true;
        "Electrical configuration of power switch";
    hardware_switches_hot_water_enabled: bool = false, "hardware.switches.hot_water.enabled", "Enable Water Switch", 13, 2231, reboot = true;
        "Enable physical water switch";
    hardware_switches_hot_water_type: choice(SwitchType) = SwitchType::Toggle, "hardware.switches.hot_water.type", "Water Switch Type", 13, 2232, reboot = true;
        "Type of water switch connected";
    hardware_switches_hot_water_mode: choice(SwitchMode) = SwitchMode::NormallyOpen, "hardware.switches.hot_water.mode", "Water Switch Mode", 13, 2233, reboot = true;
        "Electrical configuration of water switch";

    // === Sensors (sections 4 and 15) ===
    hardware_sensors_temperature_type: choice(TempSensorType) = TempSensorType::Tsic306, "hardware.sensors.temperature.type", "Temperature Sensor Type", 15, 2401, reboot = true;
        "Type of temperature sensor connected";
    hardware_sensors_pressure_enabled: bool = false, "hardware.sensors.pressure.enabled", "Enable Pressure Sensor", 4, 2411, reboot = true;
        "Enable pressure sensor functionality";
    hardware_sensors_watertank_enabled: bool = false, "hardware.sensors.watertank.enabled", "Enable Water Tank Sensor", 4, 2421, reboot = true;
        "Enable water tank level sensor";
    hardware_sensors_watertank_mode: choice(SwitchMode) = SwitchMode::NormallyClosed, "hardware.sensors.watertank.mode", "Water Tank Sensor Mode", 15, 2422, reboot = true;
        "Electrical configuration of water tank sensor";
    hardware_sensors_watertank_keep_heater_on_empty: bool = false, "hardware.sensors.watertank.keep_heater_on_empty", "Keep Heater On When Tank Empty", 4, 2423;
        "Warning: keeps the PID/heater active even when the water tank is reported empty. Only the external reservoir is protected by this sensor, not the boiler.";
    hardware_sensors_scale_enabled: bool = false, "hardware.sensors.scale.enabled", "Enable Scale", 4, 2501, reboot = true;
        "Enable scale functionality";
    hardware_sensors_scale_samples: int(1, 20) = 2, "hardware.sensors.scale.samples", "Scale Samples", 4, 2502;
        "Number of samples averaged per weight reading";
    hardware_sensors_scale_type: choice(ScaleType) = ScaleType::Hx711Dual, "hardware.sensors.scale.type", "Scale Type", 15, 2503, reboot = true;
        "Integrated HX711-based scale with different load cell configurations or Bluetooth Low Energy scales";
    hardware_sensors_scale_calibration: double(-999_999.0, 999_999.0) = 1.0, "hardware.sensors.scale.calibration", "Scale Calibration", 4, 2504;
        "Raw data is divided by this value to convert to readable data";
    hardware_sensors_scale_calibration2: double(-999_999.0, 999_999.0) = 1.0, "hardware.sensors.scale.calibration2", "Scale Calibration 2", 4, 2505;
        "Second calibration factor for dual load cell scales";
    hardware_sensors_scale_known_weight: double(1.0, 2000.0) = 267.0, "hardware.sensors.scale.known_weight", "Scale Known Weight", 4, 2506;
        "Calibration weight for scale (weight of the tray)";
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
    fn every_default_is_valid() {
        let mut c = Config::default();
        let defaults = Config::default();
        for i in 0..PARAMS.len() {
            let v = defaults.get(i);
            assert_eq!(c.set(i, v), Ok(()), "{}", PARAMS[i].key);
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
        assert_eq!(c.set_from_str("pid.enabled", "yes please"), Err(SetError::Unparsable));
        assert_eq!(c.set_from_str("display.template", "9"), Err(SetError::NotAnOption));
        assert_eq!(c.set_from_str("backflush.cycles", "2.5"), Err(SetError::Unparsable));
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
