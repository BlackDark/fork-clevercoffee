//! The in-memory configuration value.
//!
//! # Generated structure, hand-checked
//!
//! The 98 fields and their defaults below are transcribed from
//! `include/clevercoffee/Config.h` and `include/clevercoffee/defaults.h`. The
//! nesting is chosen so that a `serde_json` serialisation of [`Config`] is
//! **byte-compatible with the C++ `Config::exportToJson()`** for every key the
//! C++ exports: `ConfigJson::setNested` (`src/ConfigJson.cpp:83-106`) turns the
//! dotted key `pid.regular.kp` into `{"pid":{"regular":{"kp":…}}}`, and so does
//! the struct layout here. `schema::SCHEMA` is the flat view of the same
//! registry, and a test asserts the two agree.
//!
//! # Field types
//!
//! * `bool`, `i32` and `f64` for the three scalar kinds. The C++ uses `double`
//!   for every numeric parameter that is not an `int`, so `f64` is the faithful
//!   choice and avoids a lossy round trip through `f32`.
//! * The C++ enumerations, as Rust enums from `cc-domain`. They serialise as
//!   their integer discriminant, matching `EnumParamDef::toJson`
//!   (`Config.h:420-424`).
//! * `Secret<String>` for the four credentials. See [`crate::Secret`].
//! * `String` for the remaining text parameters. The C++ does not length-check
//!   them at all, so a fixed-capacity type here would reject configurations the
//!   C++ accepts; see `schema`'s module documentation.
//!
//! # Accessors
//!
//! The fields are public, so the typed accessor *is* the field access
//! (`config.brew.setpoint` is a `f64`, and it is impossible to pass it where a
//! `Bar` belongs). On top of that, [`Config`] provides named accessors for the
//! handful of values that are *derived* rather than stored — the PID gains the
//! firmware actually computes, the safety view, the heater window — so that
//! nobody has to re-derive them and get it subtly wrong.

use alloc::string::String;

use cc_domain::hardware::{
    OledAddress, OledType, RelayTriggerType, ScaleType, SwitchMode, SwitchType,
    TemperatureSensorType,
};
use cc_domain::process::BrewMode;
use cc_domain::system::{DisplayTemplate, Language, LogLevel};
use serde::{Deserialize, Serialize};

use crate::secret::Secret;

/// The whole configuration: 98 typed fields mirroring the C++
/// `ParamDef` members. See the module documentation for the mapping and
/// for the six ways this differs from the C++.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The `pid.*` parameters.
    pub pid: Pid,
    /// The `brew.*` parameters.
    pub brew: Brew,
    /// The `steam.*` parameters.
    pub steam: Steam,
    /// The `display.*` parameters.
    pub display: Display,
    /// The `hardware.*` parameters.
    pub hardware: Hardware,
    /// The `backflush.*` parameters.
    pub backflush: Backflush,
    /// The `maintenance.*` parameters.
    pub maintenance: Maintenance,
    /// The `standby.*` parameters.
    pub standby: Standby,
    /// The `mqtt.*` parameters.
    pub mqtt: Mqtt,
    /// The `system.*` parameters.
    pub system: System,
    /// The `safety.*` parameters.
    pub safety: Safety,
}

/// The `pid.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pid {
    /// `pid.enabled` - Enable PID Controller
    ///
    /// Enables or disables the PID temperature controller
    pub enabled: bool,
    /// `pid.use_ponm` - Enable `PonM`
    ///
    /// Use `PonM` mode (Proportional on Measurement)
    pub use_ponm: bool,
    /// `pid.ema_factor` - PID EMA Factor
    ///
    /// Smoothing of input for derivative component. Smaller = less smoothing but less delay
    ///
    /// Range: `0 ..= 1`.
    pub ema_factor: f64,
    /// The `pid.regular.*` parameters.
    pub regular: PidRegular,
    /// The `pid.steam.*` parameters.
    pub steam: PidSteam,
    /// The `pid.bd.*` parameters.
    pub bd: PidBd,
}

impl Default for Pid {
    fn default() -> Self {
        Self {
            enabled: false,
            use_ponm: false,
            ema_factor: 0.6,
            regular: PidRegular::default(),
            steam: PidSteam::default(),
            bd: PidBd::default(),
        }
    }
}

/// The `pid.regular.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PidRegular {
    /// `pid.regular.kp` - PID Kp
    ///
    /// Proportional gain (in Watts/°C) for the main PID controller
    ///
    /// Range: `0 ..= 200`.
    pub kp: f64,
    /// `pid.regular.tn` - PID Tn
    ///
    /// Integral time constant (in seconds) for the main PID controller
    ///
    /// Range: `0 ..= 200`.
    pub tn: f64,
    /// `pid.regular.tv` - PID Tv
    ///
    /// Differential time constant (in seconds) for the main PID controller
    ///
    /// Range: `0 ..= 200`.
    pub tv: f64,
    /// `pid.regular.i_max` - PID Integrator Max
    ///
    /// Internal integrator limit to prevent windup (in Watts)
    ///
    /// Range: `0 ..= 100`.
    pub i_max: f64,
}

impl Default for PidRegular {
    fn default() -> Self {
        Self {
            kp: 62.0,
            tn: 52.0,
            tv: 11.5,
            i_max: 55.0,
        }
    }
}

/// The `pid.steam.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PidSteam {
    /// `pid.steam.kp` - Steam Kp
    ///
    /// Proportional gain for the steaming mode
    ///
    /// Range: `0 ..= 500`.
    pub kp: f64,
}

impl Default for PidSteam {
    fn default() -> Self {
        Self { kp: 150.0 }
    }
}

/// The `pid.bd.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PidBd {
    /// `pid.bd.enabled` - Enable Brew PID
    ///
    /// Use separate PID parameters while brew is running
    pub enabled: bool,
    /// `pid.bd.kp` - BD Kp
    ///
    /// Proportional gain for PID when brewing has been detected
    ///
    /// Range: `0 ..= 200`.
    pub kp: f64,
    /// `pid.bd.tn` - BD Tn
    ///
    /// Integral time constant for PID when brewing has been detected
    ///
    /// Range: `0 ..= 200`.
    pub tn: f64,
    /// `pid.bd.tv` - BD Tv
    ///
    /// Differential time constant for PID when brewing has been detected
    ///
    /// Range: `0 ..= 200`.
    pub tv: f64,
}

impl Default for PidBd {
    fn default() -> Self {
        Self {
            enabled: false,
            kp: 50.0,
            tn: 0.0,
            tv: 20.0,
        }
    }
}

/// The `brew.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Brew {
    /// `brew.setpoint` - Setpoint (°C)
    ///
    /// The temperature that the PID will attempt to reach and hold
    ///
    /// Range: `20 ..= 110`.
    pub setpoint: f64,
    /// `brew.temp_offset` - Offset (°C)
    ///
    /// Optional offset added to the user-visible setpoint to compensate sensor offsets
    ///
    /// Range: `0 ..= 20`.
    pub temp_offset: f64,
    /// `brew.pid_delay` - Brew PID Delay (s)
    ///
    /// Delay time during which PID will be disabled once brew is detected
    ///
    /// Range: `0 ..= 60`.
    pub pid_delay: f64,
    /// `brew.mode` - Brew Mode
    ///
    /// Brewing mode selection
    #[serde(with = "crate::as_int")]
    pub mode: BrewMode,
    /// The `brew.by_time.*` parameters.
    pub by_time: BrewByTime,
    /// The `brew.by_weight.*` parameters.
    pub by_weight: BrewByWeight,
    /// The `brew.pre_infusion.*` parameters.
    pub pre_infusion: BrewPreInfusion,
}

impl Default for Brew {
    fn default() -> Self {
        Self {
            setpoint: 95.0,
            temp_offset: 0.0,
            pid_delay: 10.0,
            mode: BrewMode::Manual,
            by_time: BrewByTime::default(),
            by_weight: BrewByWeight::default(),
            pre_infusion: BrewPreInfusion::default(),
        }
    }
}

/// The `brew.by_time.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrewByTime {
    /// `brew.by_time.enabled` - Brew by Time
    ///
    /// Enable brewing by time control
    pub enabled: bool,
    /// `brew.by_time.target_time` - Target Brew Time (s)
    ///
    /// Target brew time in seconds
    ///
    /// Range: `1 ..= 120`.
    pub target_time: f64,
}

impl Default for BrewByTime {
    fn default() -> Self {
        Self {
            enabled: false,
            target_time: 25.0,
        }
    }
}

/// The `brew.by_weight.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrewByWeight {
    /// `brew.by_weight.enabled` - Brew by Weight
    ///
    /// Enable brewing by weight control
    pub enabled: bool,
    /// `brew.by_weight.target_weight` - Target Brew Weight (g)
    ///
    /// Brew is running until this weight has been measured
    ///
    /// Range: `0 ..= 500`.
    pub target_weight: f64,
    /// `brew.by_weight.auto_tare` - Auto-tare
    ///
    /// Automatically tare scale before brewing
    pub auto_tare: bool,
}

impl Default for BrewByWeight {
    fn default() -> Self {
        Self {
            enabled: false,
            target_weight: 36.0,
            auto_tare: false,
        }
    }
}

/// The `brew.pre_infusion.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrewPreInfusion {
    /// `brew.pre_infusion.enabled` - Pre-Infusion
    ///
    /// Enable pre-infusion phase
    pub enabled: bool,
    /// `brew.pre_infusion.time` - Preinfusion Time (s)
    ///
    /// Pre-infusion time in seconds
    ///
    /// Range: `0 ..= 60`.
    pub time: f64,
    /// `brew.pre_infusion.pause` - Preinfusion Pause (s)
    ///
    /// Pre-infusion pause time in seconds
    ///
    /// Range: `0 ..= 60`.
    pub pause: f64,
}

impl Default for BrewPreInfusion {
    fn default() -> Self {
        Self {
            enabled: false,
            time: 2.0,
            pause: 5.0,
        }
    }
}

/// The `steam.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Steam {
    /// `steam.setpoint` - Steam Setpoint (°C)
    ///
    /// The temperature that the PID will use for steam mode
    ///
    /// Range: `100 ..= 140`.
    pub setpoint: f64,
}

impl Default for Steam {
    fn default() -> Self {
        Self { setpoint: 120.0 }
    }
}

/// The `display.*` parameters.
// A configuration group is a bag of independent user-settable flags, not a
// state machine: `heating_logo`, `pid_off_logo` and the three fullscreen-timer
// toggles have no relationship to one another, and collapsing them into
// two-variant enums would make the JSON diverge from the C++ for no gain.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    /// `display.fullscreen_brew_timer` - Enable Fullscreen Brew Timer
    ///
    /// Enable fullscreen overlay during brew
    pub fullscreen_brew_timer: bool,
    /// `display.fullscreen_manual_flush_timer` - Enable Fullscreen Manual Flush Timer
    ///
    /// Enable fullscreen overlay during manual flush
    pub fullscreen_manual_flush_timer: bool,
    /// `display.fullscreen_hot_water_timer` - Enable Fullscreen Hot Water Timer
    ///
    /// Enable fullscreen overlay during hot water mode
    pub fullscreen_hot_water_timer: bool,
    /// `display.post_brew_timer_duration` - Post Brew Timer Duration (s)
    ///
    /// Post brew timer will be shown for this many seconds after brew finished
    ///
    /// Range: `0 ..= 60`.
    pub post_brew_timer_duration: f64,
    /// `display.heating_logo` - Enable Heating Logo
    ///
    /// Full screen logo will be shown if temperature is 5°C below setpoint
    pub heating_logo: bool,
    /// `display.pid_off_logo` - Enable 'PID Disabled' Logo
    ///
    /// Full screen logo will be shown if PID is disabled
    pub pid_off_logo: bool,
    /// `display.template` - Display Template
    #[serde(with = "crate::as_int")]
    pub template: DisplayTemplate,
    /// `display.inverted` - Invert Display
    pub inverted: bool,
    /// `display.language` - Display Language
    ///
    /// Set the language for the OLED display
    #[serde(with = "crate::as_int")]
    pub language: Language,
    /// The `display.blinking.*` parameters.
    pub blinking: DisplayBlinking,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            fullscreen_brew_timer: false,
            fullscreen_manual_flush_timer: false,
            fullscreen_hot_water_timer: false,
            post_brew_timer_duration: 3.0,
            heating_logo: true,
            pid_off_logo: true,
            template: DisplayTemplate::Standard,
            inverted: false,
            language: Language::English,
            blinking: DisplayBlinking::default(),
        }
    }
}

/// The `display.blinking.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayBlinking {
    /// `display.blinking.delta` - Status LED Delta
    ///
    /// Delta from setpoint for status LED and blinking temperature display
    ///
    /// Range: `0.2 ..= 10`.
    pub delta: f64,
}

impl Default for DisplayBlinking {
    fn default() -> Self {
        Self { delta: 0.3 }
    }
}

/// The `hardware.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hardware {
    /// The `hardware.leds.*` parameters.
    pub leds: HardwareLeds,
    /// The `hardware.oled.*` parameters.
    pub oled: HardwareOled,
    /// The `hardware.relays.*` parameters.
    pub relays: HardwareRelays,
    /// The `hardware.switches.*` parameters.
    pub switches: HardwareSwitches,
    /// The `hardware.sensors.*` parameters.
    pub sensors: HardwareSensors,
}

/// The `hardware.leds.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareLeds {
    /// The `hardware.leds.status.*` parameters.
    pub status: HardwareLedsStatus,
    /// The `hardware.leds.brew.*` parameters.
    pub brew: HardwareLedsBrew,
    /// The `hardware.leds.steam.*` parameters.
    pub steam: HardwareLedsSteam,
}

/// The `hardware.leds.status.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareLedsStatus {
    /// `hardware.leds.status.enabled` - Enable Status LED
    ///
    /// Enable status indicator LED
    pub enabled: bool,
    /// `hardware.leds.status.inverted` - Invert Status LED
    ///
    /// Invert the status LED logic (for common anode LEDs)
    pub inverted: bool,
}

/// The `hardware.leds.brew.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareLedsBrew {
    /// `hardware.leds.brew.enabled` - Enable Brew LED
    ///
    /// Enable brew indicator LED
    pub enabled: bool,
    /// `hardware.leds.brew.inverted` - Invert Brew LED
    ///
    /// Invert the brew LED logic
    pub inverted: bool,
}

/// The `hardware.leds.steam.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareLedsSteam {
    /// `hardware.leds.steam.enabled` - Enable Steam LED
    ///
    /// Enable steam indicator LED
    pub enabled: bool,
    /// `hardware.leds.steam.inverted` - Invert Steam LED
    ///
    /// Invert the steam LED logic
    pub inverted: bool,
}

/// The `hardware.oled.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareOled {
    /// `hardware.oled.enabled` - Enable OLED Display
    ///
    /// Enable or disable the OLED display
    pub enabled: bool,
    /// `hardware.oled.type` - OLED Type
    ///
    /// Select your OLED display type
    #[serde(with = "crate::as_int")]
    pub r#type: OledType,
    /// `hardware.oled.address` - I2C Address
    ///
    /// I2C address of the OLED display
    #[serde(with = "crate::as_int")]
    pub address: OledAddress,
}

impl Default for HardwareOled {
    fn default() -> Self {
        Self {
            enabled: true,
            r#type: OledType::Ssd1306,
            address: OledAddress::Addr3c,
        }
    }
}

/// The `hardware.relays.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareRelays {
    /// The `hardware.relays.heater.*` parameters.
    pub heater: HardwareRelaysHeater,
    /// The `hardware.relays.valve.*` parameters.
    pub valve: HardwareRelaysValve,
    /// The `hardware.relays.pump.*` parameters.
    pub pump: HardwareRelaysPump,
}

/// The `hardware.relays.heater.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareRelaysHeater {
    /// `hardware.relays.heater.trigger_type` - Heater Relay Trigger Type
    ///
    /// Relay trigger type for heater control
    #[serde(with = "crate::as_int")]
    pub trigger_type: RelayTriggerType,
}

impl Default for HardwareRelaysHeater {
    fn default() -> Self {
        Self {
            trigger_type: RelayTriggerType::HighTrigger,
        }
    }
}

/// The `hardware.relays.valve.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareRelaysValve {
    /// `hardware.relays.valve.trigger_type` - Valve Relay Trigger Type
    ///
    /// Relay trigger type for valve control
    #[serde(with = "crate::as_int")]
    pub trigger_type: RelayTriggerType,
}

impl Default for HardwareRelaysValve {
    fn default() -> Self {
        Self {
            trigger_type: RelayTriggerType::HighTrigger,
        }
    }
}

/// The `hardware.relays.pump.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareRelaysPump {
    /// `hardware.relays.pump.trigger_type` - Pump Relay Trigger Type
    ///
    /// Relay trigger type for pump control
    #[serde(with = "crate::as_int")]
    pub trigger_type: RelayTriggerType,
}

impl Default for HardwareRelaysPump {
    fn default() -> Self {
        Self {
            trigger_type: RelayTriggerType::HighTrigger,
        }
    }
}

/// The `hardware.switches.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareSwitches {
    /// The `hardware.switches.brew.*` parameters.
    pub brew: HardwareSwitchesBrew,
    /// The `hardware.switches.steam.*` parameters.
    pub steam: HardwareSwitchesSteam,
    /// The `hardware.switches.power.*` parameters.
    pub power: HardwareSwitchesPower,
    /// The `hardware.switches.hot_water.*` parameters.
    pub hot_water: HardwareSwitchesHotWater,
}

/// The `hardware.switches.brew.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareSwitchesBrew {
    /// `hardware.switches.brew.enabled` - Enable Brew Switch
    ///
    /// Enable physical brew switch
    pub enabled: bool,
    /// `hardware.switches.brew.type` - Brew Switch Type
    ///
    /// Type of brew switch connected
    #[serde(with = "crate::as_int")]
    pub r#type: SwitchType,
    /// `hardware.switches.brew.mode` - Brew Switch Mode
    ///
    /// Electrical configuration of brew switch
    #[serde(with = "crate::as_int")]
    pub mode: SwitchMode,
}

impl Default for HardwareSwitchesBrew {
    fn default() -> Self {
        Self {
            enabled: false,
            r#type: SwitchType::Toggle,
            mode: SwitchMode::NormallyOpen,
        }
    }
}

/// The `hardware.switches.steam.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareSwitchesSteam {
    /// `hardware.switches.steam.enabled` - Enable Steam Switch
    ///
    /// Enable physical steam switch
    pub enabled: bool,
    /// `hardware.switches.steam.type` - Steam Switch Type
    ///
    /// Type of steam switch connected
    #[serde(with = "crate::as_int")]
    pub r#type: SwitchType,
    /// `hardware.switches.steam.mode` - Steam Switch Mode
    ///
    /// Electrical configuration of steam switch
    #[serde(with = "crate::as_int")]
    pub mode: SwitchMode,
}

impl Default for HardwareSwitchesSteam {
    fn default() -> Self {
        Self {
            enabled: false,
            r#type: SwitchType::Toggle,
            mode: SwitchMode::NormallyOpen,
        }
    }
}

/// The `hardware.switches.power.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareSwitchesPower {
    /// `hardware.switches.power.enabled` - Enable Power Switch
    ///
    /// Enable physical power switch
    pub enabled: bool,
    /// `hardware.switches.power.type` - Power Switch Type
    ///
    /// Type of power switch connected
    #[serde(with = "crate::as_int")]
    pub r#type: SwitchType,
    /// `hardware.switches.power.mode` - Power Switch Mode
    ///
    /// Electrical configuration of power switch
    #[serde(with = "crate::as_int")]
    pub mode: SwitchMode,
}

impl Default for HardwareSwitchesPower {
    fn default() -> Self {
        Self {
            enabled: false,
            r#type: SwitchType::Toggle,
            mode: SwitchMode::NormallyOpen,
        }
    }
}

/// The `hardware.switches.hot_water.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareSwitchesHotWater {
    /// `hardware.switches.hot_water.enabled` - Enable Water Switch
    ///
    /// Enable physical water switch
    pub enabled: bool,
    /// `hardware.switches.hot_water.type` - Water Switch Type
    ///
    /// Type of water switch connected
    #[serde(with = "crate::as_int")]
    pub r#type: SwitchType,
    /// `hardware.switches.hot_water.mode` - Water Switch Mode
    ///
    /// Electrical configuration of water switch
    #[serde(with = "crate::as_int")]
    pub mode: SwitchMode,
}

impl Default for HardwareSwitchesHotWater {
    fn default() -> Self {
        Self {
            enabled: false,
            r#type: SwitchType::Toggle,
            mode: SwitchMode::NormallyOpen,
        }
    }
}

/// The `hardware.sensors.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareSensors {
    /// The `hardware.sensors.temperature.*` parameters.
    pub temperature: HardwareSensorsTemperature,
    /// The `hardware.sensors.pressure.*` parameters.
    pub pressure: HardwareSensorsPressure,
    /// The `hardware.sensors.watertank.*` parameters.
    pub watertank: HardwareSensorsWatertank,
    /// The `hardware.sensors.scale.*` parameters.
    pub scale: HardwareSensorsScale,
}

/// The `hardware.sensors.temperature.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareSensorsTemperature {
    /// `hardware.sensors.temperature.type` - Temperature Sensor Type
    ///
    /// Type of temperature sensor connected
    #[serde(with = "crate::as_int")]
    pub r#type: TemperatureSensorType,
}

impl Default for HardwareSensorsTemperature {
    /// **DIVERGENCE from `Config.h:1085-1092`**, which defaults this to
    /// `TSIC_306`.
    ///
    /// The C++ default names a sensor that is not fitted: the probe on this
    /// machine is a DS18B20 (family `0x28`, measured). With the C++ default the
    /// firmware builds a TSIC-306 driver, reads a 1-Wire bus it does not own,
    /// and reports the result as if it came from the configured sensor. The
    /// previous Rust firmware did the same and logged it
    /// ([08 §4.1](../../docs/rust-migration/08-recovered-oracle.md)).
    ///
    /// `cc_safety::validate_config` now **rejects** `TSIC_306`, so the default
    /// has to be a value that is not rejected — a default the machine refuses to
    /// run would be worse than either alternative. `DALLAS_DS18B20` is the
    /// driver that exists and the sensor that is fitted.
    fn default() -> Self {
        Self {
            r#type: TemperatureSensorType::DallasDs18b20,
        }
    }
}

/// The `hardware.sensors.pressure.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareSensorsPressure {
    /// `hardware.sensors.pressure.enabled` - Enable Pressure Sensor
    ///
    /// Enable pressure sensor functionality
    pub enabled: bool,
}

/// The `hardware.sensors.watertank.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareSensorsWatertank {
    /// `hardware.sensors.watertank.enabled` - Enable Water Tank Sensor
    ///
    /// Enable water tank level sensor
    pub enabled: bool,
    /// `hardware.sensors.watertank.mode` - Water Tank Sensor Mode
    ///
    /// Electrical configuration of water tank sensor
    #[serde(with = "crate::as_int")]
    pub mode: SwitchMode,
    /// `hardware.sensors.watertank.keep_heater_on_empty` - Keep Heater On When Tank Empty
    ///
    /// Warning: keeps the PID/heater active even when the water tank is reported empty.
    pub keep_heater_on_empty: bool,
}

impl Default for HardwareSensorsWatertank {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: SwitchMode::NormallyClosed,
            keep_heater_on_empty: false,
        }
    }
}

/// The `hardware.sensors.scale.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HardwareSensorsScale {
    /// `hardware.sensors.scale.enabled` - Enable Scale
    ///
    /// Enable scale functionality
    pub enabled: bool,
    /// `hardware.sensors.scale.samples` - Scale Samples
    ///
    /// Number of samples used for calibration
    ///
    /// Range: `1 ..= 20`.
    pub samples: i32,
    /// `hardware.sensors.scale.type` - Scale Type
    ///
    /// Integrated HX711-based scale with different load cell configurations or Bluetooth Low Energy scales
    #[serde(with = "crate::as_int")]
    pub r#type: ScaleType,
    /// `hardware.sensors.scale.calibration` - Scale Calibration
    ///
    /// Raw data is divided by this value to convert to readable data
    ///
    /// Range: `-999999 ..= 999999`.
    pub calibration: f64,
    /// `hardware.sensors.scale.calibration2` - Scale Calibration 2
    ///
    /// Second calibration factor for dual load cell scales
    ///
    /// Range: `-999999 ..= 999999`.
    pub calibration2: f64,
    /// `hardware.sensors.scale.known_weight` - Scale Known Weight
    ///
    /// Calibration weight for scale (weight of the tray)
    ///
    /// Range: `1 ..= 2000`.
    pub known_weight: f64,
}

impl Default for HardwareSensorsScale {
    fn default() -> Self {
        Self {
            enabled: false,
            samples: 2,
            r#type: ScaleType::Hx711Dual,
            calibration: 1.0,
            calibration2: 1.0,
            known_weight: 267.0,
        }
    }
}

/// The `backflush.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Backflush {
    /// `backflush.cycles` - Backflush Cycles
    ///
    /// Number of backflush cycles to perform
    ///
    /// Range: `2 ..= 20`.
    pub cycles: i32,
    /// `backflush.fill_time` - Backflush Fill Time (s)
    ///
    /// Time to fill during backflush cycle
    ///
    /// Range: `3 ..= 10`.
    pub fill_time: f64,
    /// `backflush.flush_time` - Backflush Flush Time (s)
    ///
    /// Time to flush during backflush cycle
    ///
    /// Range: `5 ..= 20`.
    pub flush_time: f64,
}

impl Default for Backflush {
    fn default() -> Self {
        Self {
            cycles: 5,
            fill_time: 5.0,
            flush_time: 10.0,
        }
    }
}

/// The `maintenance.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Maintenance {
    /// The `maintenance.backflush_reminder.*` parameters.
    pub backflush_reminder: MaintenanceBackflushReminder,
}

/// The `maintenance.backflush_reminder.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaintenanceBackflushReminder {
    /// `maintenance.backflush_reminder.enabled` - Backflush Reminder
    ///
    /// Show a reminder when the shot count since last backflush reaches the threshold
    pub enabled: bool,
    /// `maintenance.backflush_reminder.threshold` - Backflush Reminder Threshold
    ///
    /// Number of counted brews before a backflush reminder is shown (default ~monthly at 2 shots/day)
    ///
    /// Range: `1 ..= 500`.
    pub threshold: i32,
}

impl Default for MaintenanceBackflushReminder {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 50,
        }
    }
}

/// The `standby.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Standby {
    /// `standby.enabled` - Enable Standby Timer
    ///
    /// Turn heater off after standby time has elapsed
    pub enabled: bool,
    /// `standby.time` - Standby Time
    ///
    /// Time in minutes until the heater is turned off
    ///
    /// Range: `1 ..= 120`.
    pub time: f64,
}

impl Default for Standby {
    fn default() -> Self {
        Self {
            enabled: false,
            time: 35.0,
        }
    }
}

/// The `mqtt.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mqtt {
    /// `mqtt.enabled` - MQTT Enabled
    pub enabled: bool,
    /// `mqtt.broker` -
    ///
    /// MQTT Broker
    pub broker: String,
    /// `mqtt.port` - MQTT Port
    ///
    /// Port number of your MQTT broker
    ///
    /// Range: `1 ..= 65535`.
    pub port: i32,
    /// `mqtt.username` - Username
    ///
    /// Username for your MQTT broker
    pub username: String,
    /// `mqtt.password` - Password
    ///
    /// Password for your MQTT broker
    pub password: Secret<String>,
    /// `mqtt.topic` - Topic Prefix
    ///
    /// Custom MQTT topic prefix
    pub topic: String,
    /// The `mqtt.hassio.*` parameters.
    pub hassio: MqttHassio,
}

impl Default for Mqtt {
    fn default() -> Self {
        Self {
            enabled: false,
            broker: String::new(),
            port: 1883,
            username: String::from("rancilio"),
            password: Secret::new(String::from("silvia")),
            topic: String::from("custom/kitchen/"),
            hassio: MqttHassio::default(),
        }
    }
}

/// The `mqtt.hassio.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MqttHassio {
    /// `mqtt.hassio.enabled` - Hass.io enabled
    ///
    /// Enables Home Assistant integration
    pub enabled: bool,
    /// `mqtt.hassio.prefix` - Hass.io Prefix
    ///
    /// Custom MQTT topic prefix for Home Assistant
    pub prefix: String,
}

impl Default for MqttHassio {
    fn default() -> Self {
        Self {
            enabled: false,
            prefix: String::from("homeassistant"),
        }
    }
}

/// The `system.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct System {
    /// `system.hostname` - Hostname
    pub hostname: String,
    /// `system.ota_password` - OTA Password
    pub ota_password: Secret<String>,
    /// `system.offline_mode` - Offline Mode
    ///
    /// Run in offline mode without `WiFi` connection
    pub offline_mode: bool,
    /// `system.log_level` - Log Level
    ///
    /// Set the logging level for debug output
    #[serde(with = "crate::as_int")]
    pub log_level: LogLevel,
    /// The `system.auth.*` parameters.
    pub auth: SystemAuth,
    /// The `system.timing_debug.*` parameters.
    pub timing_debug: SystemTimingDebug,
    /// The `system.showdisplay.*` parameters.
    pub showdisplay: SystemShowdisplay,
    /// The `system.wifi.*` parameters.
    pub wifi: SystemWifi,
}

impl Default for System {
    fn default() -> Self {
        Self {
            hostname: String::from("silvia"),
            ota_password: Secret::new(String::from("otapass")),
            offline_mode: false,
            log_level: LogLevel::Info,
            auth: SystemAuth::default(),
            timing_debug: SystemTimingDebug::default(),
            showdisplay: SystemShowdisplay::default(),
            wifi: SystemWifi::default(),
        }
    }
}

/// The `system.auth.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemAuth {
    /// `system.auth.enabled` - Enable Authentication
    ///
    /// Enables authentication for accessing certain parts of the website
    pub enabled: bool,
    /// `system.auth.username` - Website Username
    ///
    /// Username for accessing the website and authenticating web requests
    pub username: String,
    /// `system.auth.password` - Website Password
    ///
    /// Password for accessing the website and authenticating web requests
    pub password: Secret<String>,
}

impl Default for SystemAuth {
    fn default() -> Self {
        Self {
            enabled: false,
            username: String::from("admin"),
            password: Secret::new(String::from("admin")),
        }
    }
}

/// The `system.timing_debug.*` parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemTimingDebug {
    /// `system.timing_debug.enabled` - Loop timing in console
    ///
    /// Enable or disable the process loop time debugging in console
    pub enabled: bool,
}

/// The `system.showdisplay.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemShowdisplay {
    /// `system.showdisplay.enabled` - Activate display recording
    ///
    /// Enable or disable showing sendBuffer loops in debug logs
    pub enabled: bool,
}

impl Default for SystemShowdisplay {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// The `system.wifi.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemWifi {
    /// `system.wifi.ssid` - `WiFi` SSID
    ///
    /// `WiFi` SSID to connect to directly (leave empty to use the configuration portal)
    pub ssid: String,
    /// `system.wifi.password` - `WiFi` Password
    ///
    /// `WiFi` password for direct connection (leave empty for open networks)
    pub password: Secret<String>,
}

impl Default for SystemWifi {
    fn default() -> Self {
        Self {
            ssid: String::new(),
            password: Secret::new(String::new()),
        }
    }
}

/// The `safety.*` parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Safety {
    /// `safety.emergency_temp` - Emergency Temperature (°C)
    ///
    /// Temperature threshold that triggers emergency stop
    ///
    /// Range: `120 ..= 180`.
    pub emergency_temp: f64,
    /// `safety.emergency_hysteresis` - Emergency Hysteresis (°C)
    ///
    /// Temperature drop required to reset emergency counter
    ///
    /// Range: `1 ..= 15`.
    pub emergency_hysteresis: f64,
}

impl Default for Safety {
    fn default() -> Self {
        Self {
            emergency_temp: 150.0,
            emergency_hysteresis: 5.0,
        }
    }
}
/// The subset of the configuration that decides whether an actuator may be
/// energised.
///
/// `cc-config` cannot depend on `cc-safety` — the dependency direction in 04 §6
/// makes them siblings, both below `cc-machine` — so this is a plain view of the
/// safety-relevant values, and `cc-firmware` (or `cc-hal-esp32`) converts it
/// with `cc_safety::SafetyConfig::from(view)` at wiring time. Keeping the struct
/// here means those values are named in exactly one place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SafetyView {
    /// `safety.emergency_temp`.
    pub emergency_temp: f64,
    /// `safety.emergency_hysteresis`.
    pub emergency_hysteresis: f64,
    /// `steam.setpoint`.
    pub steam_setpoint: f64,
    /// `hardware.relays.heater.trigger_type`.
    pub heater_relay_trigger: RelayTriggerType,
    /// `hardware.sensors.temperature.type`.
    ///
    /// Fifth value, and for the same reason as the other four: a probe the
    /// firmware cannot drive is a temperature reading S1 cannot trust. See
    /// `cc_safety::ConfigViolation::UnsupportedTemperatureSensor`.
    pub temperature_sensor: TemperatureSensorType,
}

impl Config {
    /// The safety-relevant subset of this configuration.
    ///
    /// Read this **once** at boot, convert it to a `cc_safety::SafetyConfig`,
    /// and hand that to `cc_safety::load_or_default` so an unsafe stored
    /// configuration is discarded rather than run. See
    /// [`crate::store`] for where that wiring happens.
    #[must_use]
    pub fn safety_view(&self) -> SafetyView {
        SafetyView {
            emergency_temp: self.safety.emergency_temp,
            emergency_hysteresis: self.safety.emergency_hysteresis,
            steam_setpoint: self.steam.setpoint,
            heater_relay_trigger: self.hardware.relays.heater.trigger_type,
            temperature_sensor: self.hardware.sensors.temperature.r#type,
        }
    }

    /// The brew setpoint the PID should actually hold.
    ///
    /// `brew.temp_offset` exists to compensate a probe that reads low
    /// (`Config.h:804`), and the firmware adds it to the setpoint. Getting this
    /// backwards is the kind of mistake that produces cold coffee and a support
    /// ticket, so it is a named function.
    #[must_use]
    pub fn effective_brew_setpoint(&self) -> f64 {
        self.brew.setpoint + self.brew.temp_offset
    }

    /// The heater chopper window in milliseconds.
    ///
    /// NOT configurable: the C++ hard-codes `windowSize_ = 1000`
    /// (`include/clevercoffee/context/ProcessState.h:183`) and the PID output is
    /// bounded by it (`SystemInitializer.cpp:552`). Exposing it as a constant
    /// rather than a parameter makes it obvious that changing it changes the
    /// control loop, which R1-07 has to do deliberately.
    pub const HEATER_WINDOW_MS: u32 = 1000;

    /// The PID gains the firmware would compute for the normal (non-brew)
    /// phase, as `ProcessController::calculatePIDParameters()` does
    /// (`src/control/ProcessController.cpp:382-390`): `Ki = Kp / Tn` and
    /// `Kd = Tv * Kp`, with `Ki = 0` when `Tn` is 0.
    ///
    /// Returned in the order the C++ constructor takes them
    /// (`SystemInitializer.cpp:297-304`): `(Kp, Ki, Kd)`.
    #[must_use]
    pub fn pid_tunings(&self) -> (f64, f64, f64) {
        let kp = self.pid.regular.kp;
        // ProcessController.cpp:383-387 guards the division explicitly rather
        // than relying on IEEE infinity, so a Tn of 0 gives Ki = 0.
        let ki = if self.pid.regular.tn == 0.0 {
            0.0
        } else {
            kp / self.pid.regular.tn
        };
        let kd = self.pid.regular.tv * kp;
        (kp, ki, kd)
    }

    /// The PID gains for the brew-detection phase, from
    /// `ProcessController::calculateBrewDetectionPIDParameters()`.
    #[must_use]
    pub fn brew_detection_tunings(&self) -> (f64, f64, f64) {
        let kp = self.pid.bd.kp;
        let ki = if self.pid.bd.tn == 0.0 {
            0.0
        } else {
            kp / self.pid.bd.tn
        };
        let kd = self.pid.bd.tv * kp;
        (kp, ki, kd)
    }

    /// Whether the water-tank interlock is active at all.
    ///
    /// `false` means the machine has no tank sensor fitted, which is the
    /// default — and therefore the *unsafe* default, because a machine with a
    /// float switch wired up but switched off in the configuration will happily
    /// run the pump dry.
    #[must_use]
    pub fn water_tank_interlock_enabled(&self) -> bool {
        self.hardware.sensors.watertank.enabled
    }

    /// Whether the heater is allowed to keep running when the tank reads empty.
    ///
    /// Carries a warning in its own name on purpose: the C++ help text for
    /// `hardware.sensors.watertank.keep_heater_on_empty` is explicit that only
    /// the external reservoir is protected by the sensor, not the boiler
    /// (`Config.h:1116-1122`).
    #[must_use]
    pub fn heater_runs_with_empty_tank(&self) -> bool {
        self.hardware.sensors.watertank.keep_heater_on_empty
    }

    /// The Wi-Fi password, for the network stack.
    ///
    /// One of the few places a credential legitimately leaves [`Secret`]. Grep
    /// for `expose` to find them all.
    #[must_use]
    pub fn wifi_password(&self) -> &str {
        self.system.wifi.password.expose()
    }

    /// The MQTT password, for the MQTT client.
    #[must_use]
    pub fn mqtt_password(&self) -> &str {
        self.mqtt.password.expose()
    }

    /// The web-interface password, for HTTP basic auth.
    #[must_use]
    pub fn web_password(&self) -> &str {
        self.system.auth.password.expose()
    }

    /// The OTA password, for the HTTP OTA endpoint.
    #[must_use]
    pub fn ota_password(&self) -> &str {
        self.system.ota_password.expose()
    }

    /// Whether the OTA endpoint should be reachable at all.
    ///
    /// The C++ exposes `/api/ota/firmware` unconditionally and compares
    /// `system.ota_password` at the handler, so an empty stored password means
    /// "no password" rather than "disabled". An empty password on a
    /// network-reachable OTA endpoint is remote code execution, so this is
    /// spelled out rather than left implicit.
    #[must_use]
    pub fn ota_enabled(&self) -> bool {
        !self.ota_password().is_empty()
    }
}
