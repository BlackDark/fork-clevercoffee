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
use alloc::vec::Vec;

use cc_domain::hardware::{
    OledAddress, OledType, RelayTriggerType, ScaleType, SwitchMode, SwitchType,
    TemperatureSensorType,
};
use cc_domain::process::BrewMode;
use cc_domain::system::{DisplayTemplate, Language, LogLevel};
use cc_domain::units::Celsius;
use serde::{Deserialize, Serialize};

use crate::secret::Secret;

/// Put one configuration key back to its compiled-in default.
///
/// **Only the keys a safety violation can implicate** are handled, and the
/// function is total over them: it answers `false` for anything else rather than
/// guessing, so a caller cannot revert a key by accident because a string
/// happened to match. The set is
/// `cc_safety::ConfigViolation::implicated_keys()` plus
/// [`REPAIR_ESCALATION_KEYS`], and the loop that walks it is
/// `cc_firmware::config_io::repair_unsafe` — it lives in `cc-firmware` rather
/// than here because `cc-config` does not depend on `cc-safety`, by design.
///
/// The defaults are the *safe* direction in every case — a higher emergency
/// threshold, `HIGH_TRIGGER` relays, a lower setpoint — so a repair cannot move
/// the machine away from safety.
pub fn revert_key(config: &mut Config, key: &str) -> bool {
    let defaults = Config::default();
    match key {
        "safety.emergency_temp" => config.safety.emergency_temp = defaults.safety.emergency_temp,
        "safety.emergency_hysteresis" => {
            config.safety.emergency_hysteresis = defaults.safety.emergency_hysteresis;
        }
        "hardware.relays.heater.trigger_type" => {
            config.hardware.relays.heater.trigger_type =
                defaults.hardware.relays.heater.trigger_type;
        }
        "hardware.relays.valve.trigger_type" => {
            config.hardware.relays.valve.trigger_type = defaults.hardware.relays.valve.trigger_type;
        }
        "hardware.relays.pump.trigger_type" => {
            config.hardware.relays.pump.trigger_type = defaults.hardware.relays.pump.trigger_type;
        }
        "brew.by_weight.enabled" => config.brew.by_weight.enabled = defaults.brew.by_weight.enabled,
        "steam.setpoint" => config.steam.setpoint = defaults.steam.setpoint,
        "brew.setpoint" => config.brew.setpoint = defaults.brew.setpoint,
        "brew.temp_offset" => config.brew.temp_offset = defaults.brew.temp_offset,
        _ => return false,
    }
    true
}

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
    /// Enable physical brew switch.
    ///
    /// **Defaults to `true`, diverging from the C++'s `false`**
    /// (`Config.h:985`). The human owns this machine, asked for the four
    /// operator switches to work, and they were all disabled. Recorded in
    /// `intentional-diffs.md`; the floating-input risk of an enabled switch on
    /// an unwired input-only pin is recorded there too.
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
            enabled: true,
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
            enabled: true,
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
            enabled: true,
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
            enabled: true,
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
    /// `TSIC_306`, matching `Config.h:1085-1092`:
    ///
    /// ```cpp
    /// EnumParamDef<Hardware::TemperatureSensorType> hardwareSensorsTemperatureType{
    ///     "hardware.sensors.temperature.type",
    ///     Hardware::TemperatureSensorType::TSIC_306, ...
    /// ```
    ///
    /// An earlier revision defaulted this to `DALLAS_DS18B20`, on the grounds
    /// that the probe fitted to the development machine is a DS18B20 (family
    /// `0x28`, ROM `286937aacd78af41`, measured) and that
    /// `cc_safety::validate_config` rejected `TSIC_306` — so the C++'s default
    /// would have been a value the machine refused to run.
    ///
    /// **Reversed, because the reason no longer holds.** The TSIC-306 driver now
    /// exists (`cc_protocol::sensor::tsic306`, R3-07) and `validate_config` no
    /// longer refuses it, so there is no longer a default that the validator
    /// rejects and no reason to prefer a development-machine fact over the C++'s
    /// shipped default. The C++'s value is restored for parity.
    ///
    /// **What this does not mean** is that a TSIC-306 is fitted anywhere. The
    /// probe on the attached machine is a DS18B20 and always has been. A
    /// configuration left at this default on such a machine reports a
    /// not-connected temperature sensor and says which sensor it asked for —
    /// which is the correct, visible failure, and the thing the C++ got wrong.
    fn default() -> Self {
        Self {
            r#type: TemperatureSensorType::Tsic306,
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
            hostname: String::from(crate::schema::DEFAULT_HOSTNAME),
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
    /// `safety.emergency_temp`, as a [`Celsius`].
    ///
    /// A `Celsius` and not the `f64` the field holds, because the consumer is
    /// `cc_safety::SafetyConfig`, which is `Celsius`-typed, and a `f64`-typed
    /// view of an `f32`-typed consumer pushes a narrowing cast into the one
    /// caller that does the conversion. Converting here makes the loss visible
    /// and happens once: 0.01 °C at the extremes of the parameter's range.
    pub emergency_temp: Celsius,
    /// `safety.emergency_hysteresis`, as a [`Celsius`]. See
    /// [`SafetyView::emergency_temp`] for why.
    pub emergency_hysteresis: Celsius,
    /// `steam.setpoint`, as a [`Celsius`]. See [`SafetyView::emergency_temp`]
    /// for why.
    pub steam_setpoint: Celsius,
    /// `brew.setpoint + brew.temp_offset`, as a [`Celsius`] — the temperature
    /// the PID is actually told to hold in brew mode, which is what
    /// `cc_safety::validate_config` needs to compare against the emergency
    /// threshold.
    ///
    /// The offset is included deliberately. `cc_safety` asks "is the threshold
    /// above the temperature the boiler is driven to?", and the offset is part
    /// of that number (`Config::effective_brew_setpoint`); carrying the raw
    /// `brew.setpoint` would leave a 0..=20 degree gap the validator cannot see.
    pub effective_brew_setpoint: Celsius,
    /// `brew.mode`, as a [`BrewMode`] — so the join with `cc_safety` is a copy
    /// rather than a re-derivation of "is this brew automatic".
    ///
    /// Needed only by `cc_safety::validate_config`, and for one reason: an
    /// automatic brew is supposed to end by itself, so a configuration that
    /// leaves it with no reachable stop condition is a machine holding a pump
    /// and an open valve with nothing to end it.
    pub brew_mode: BrewMode,
    /// `brew.by_time.enabled`. See [`SafetyView::brew_by_weight_enabled`] for
    /// why these two travel together.
    pub brew_by_time_enabled: bool,
    /// `brew.by_weight.enabled` — the stop condition that needs a scale to mean
    /// anything, which is why [`SafetyView::scale_fitted`] is here.
    pub brew_by_weight_enabled: bool,
    /// `hardware.sensors.scale.enabled` — **the C++'s own definition of a
    /// fitted scale**, and this port's.
    ///
    /// It is the guard on every scale command in the C++
    /// (`WebServerManager.cpp:540,563`) and the third argument of
    /// `recordBrewIfQualified` (`BrewStates.cpp:311`), so it is what "this
    /// machine has a scale" means in the oracle this crate is measured against.
    ///
    /// A configuration value rather than a live probe, and that is deliberate:
    /// `cc_safety::validate_config` is a pure function of configuration, so the
    /// fact it reasons about has to be readable before any driver has produced
    /// a sample. What the driver then does — starts, faults, answers — is
    /// reported as `Sensors::has_scale_error` and as the weight itself, and
    /// neither reaches `cc_safety`.
    pub scale_fitted: bool,
    /// `hardware.relays.heater.trigger_type`.
    pub heater_relay_trigger: RelayTriggerType,
    /// `hardware.relays.pump.trigger_type`. See
    /// [`SafetyView::heater_relay_trigger`].
    pub pump_relay_trigger: RelayTriggerType,
    /// `hardware.relays.valve.trigger_type`.
    pub valve_relay_trigger: RelayTriggerType,
    /// `hardware.sensors.temperature.type`.
    ///
    /// Carried for completeness rather than for validation: both sensor types
    /// have a driver (R1-03, R3-07), so `cc_safety::validate_config` has nothing
    /// to say about this value and no longer takes it as a reason to refuse a
    /// configuration. The firmware still needs to know which driver the
    /// configuration asked for, and it reads it from the `Config`, not from the
    /// `SafetyView`.
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
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the four narrowing casts are the point of this method, and \
                  this is the one place they happen. The `Config` fields are \
                  `f64` because the C++ uses `double` for every non-integer \
                  parameter (`Config.h`); `cc_safety::SafetyConfig` is \
                  `Celsius`-typed, which is `f32`. Over the schema's ranges \
                  (emergency_temp 120..180, emergency_hysteresis 1..15, \
                  steam_setpoint 100..140, effective_brew_setpoint 20..130) the \
                  loss is under 1e-5 C, which is four orders of magnitude \
                  below the probe's 0.0625 C resolution. Doing it here rather \
                  than in the caller is what keeps the loss in one place and \
                  out of `cc-firmware`."
    )]
    pub fn safety_view(&self) -> SafetyView {
        SafetyView {
            emergency_temp: Celsius::new(self.safety.emergency_temp as f32),
            emergency_hysteresis: Celsius::new(self.safety.emergency_hysteresis as f32),
            steam_setpoint: Celsius::new(self.steam.setpoint as f32),
            effective_brew_setpoint: Celsius::new(self.effective_brew_setpoint() as f32),
            brew_mode: self.brew.mode,
            brew_by_time_enabled: self.brew.by_time.enabled,
            brew_by_weight_enabled: self.brew.by_weight.enabled,
            scale_fitted: self.hardware.sensors.scale.enabled,
            heater_relay_trigger: self.hardware.relays.heater.trigger_type,
            pump_relay_trigger: self.hardware.relays.pump.trigger_type,
            valve_relay_trigger: self.hardware.relays.valve.trigger_type,
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
    ///
    /// # An `i_max` of 0 also gives `Ki = 0`, and that rule is ours
    ///
    /// The C++ derives `Ki` from `Tn` alone and then hands `(0, i_max)` to
    /// `SetIntegratorLimits`, which **refuses** a window whose `min >= max`
    /// (`PID_v1.cpp:220-231`). So `pid.regular.i_max = 0` — legal in both
    /// firmwares, `PID_I_MAX_REGULAR_MIN` is `0.0` (`defaults.h:71`) — left the
    /// controller on `PID_v1`'s own `-100 ..= +100` (`PID_v1.cpp:35`) while
    /// reporting a configured ceiling of zero: an operator asking for no
    /// integral action got an integrator free to wind to either end.
    ///
    /// The schema cannot rule the value out, because Home Assistant's number
    /// entity for `aggIMax` publishes this bound
    /// (`MQTTManager.cpp:869`, through [`crate::discovery::bounds`]) and a floor
    /// above zero would diverge from the C++ **and** from this firmware's own
    /// MQTT surface. `Ki = 0` is instead the honest translation, and it is the
    /// one this codebase already uses for "no integral action": it is what a
    /// `Tn` of 0 gives, and `Controller::set_tunings` pins the accumulator to
    /// zero for it (`PID_v1.cpp:167-169`), so the integral term cannot
    /// contribute at all.
    #[must_use]
    pub fn pid_tunings(&self) -> (f64, f64, f64) {
        let kp = self.pid.regular.kp;
        // ProcessController.cpp:383-387 guards the division explicitly rather
        // than relying on IEEE infinity, so a Tn of 0 gives Ki = 0.
        let ki = if self.pid.regular.tn == 0.0 || self.pid.regular.i_max == 0.0 {
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

    /// The Wi-Fi password, for the network stack.
    ///
    /// One of the few places a credential legitimately leaves [`Secret`]. Grep
    /// for `expose` to find them all.
    #[must_use]
    pub fn wifi_password(&self) -> &str {
        self.system.wifi.password.expose()
    }

    /// Store the `system.wifi.*` credential that a provisioning session captured.
    ///
    /// `ssid` and `password` are the two halves of one thing — a network name
    /// without its key (or the reverse) cannot connect — so they are set
    /// together and there is no way to write half a credential.
    ///
    /// An empty `password` is a **valid** value and means an open network; the
    /// schema says so (`system.wifi.password`, *"leave empty for open
    /// networks"*), and the machine will try to associate without a key rather
    /// than refusing. Use [`Config::clear_wifi_credential`] to go back to
    /// unprovisioned.
    pub fn set_wifi_credential(&mut self, ssid: String, password: String) {
        self.system.wifi.ssid = ssid;
        self.system.wifi.password.set(password);
    }

    /// Forget the stored `system.wifi.*` credential.
    ///
    /// The machine is unprovisioned again: the next boot finds no SSID, brings
    /// no radio up, and spawns the provisioning task. The password becomes the
    /// empty string rather than being left dangling, because a blank `Secret`
    /// and a `Secret` holding an old key are different states and only one of
    /// them means "no credentials".
    pub fn clear_wifi_credential(&mut self) {
        self.set_wifi_credential(String::new(), String::new());
    }

    /// Whether a `system.wifi.ssid` is stored.
    ///
    /// The definition of "provisioned" the boot sequence uses
    /// (04 §3.2: *"the provisioning task is only spawned when no valid
    /// credentials exist"*), and the same predicate `POST /api/wifi-reset`
    /// restores.
    #[must_use]
    pub fn is_wifi_provisioned(&self) -> bool {
        !self.system.wifi.ssid.is_empty()
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

/// The keys a repair escalates to when `cc_safety::ConfigViolation::implicated_keys`
/// does not resolve the violation.
///
/// `steam.setpoint` may legally be 140 and `safety.emergency_hysteresis` 15, and
/// 140 + 15 is above the emergency threshold's 150 default — so reverting the
/// implicated keys alone can leave the configuration unsafe. These are the values
/// that can pull the threshold's territory away, which is why they are the second
/// pass rather than the first.
pub const REPAIR_ESCALATION_KEYS: &[&str] = &[
    "steam.setpoint",
    "safety.emergency_hysteresis",
    "brew.setpoint",
    "brew.temp_offset",
];

/// What a repair did, and whether it worked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Repair {
    /// Every key reverted, in the order it was reverted. This is what the boot
    /// log prints: the operator's only way to learn what happened without a
    /// serial console.
    pub reverted: Vec<String>,
    /// The configuration is safe to run.
    pub resolved: bool,
}

/// Make a stored configuration safe to run, keeping everything not implicated.
///
/// Finding #12: the boot path used to discard **all** of a stored configuration
/// and run the compiled-in defaults, which cost a bench its sensor configuration
/// and put the machine in `SENSOR_ERROR` with `NaN`. One wrong number should cost
/// one number.
///
/// **Why the validator is a parameter.** `cc-config` and `cc-safety` are peers —
/// both leaf crates on `cc-domain`, neither depending on the other — and this
/// function needs both. Taking the verdict as a closure keeps that true: the
/// caller passes `|c| cc_safety::validate_config(...).err().map(|v| v.implicated_keys())`,
/// and the loop below stays testable on the host against the real validator
/// without a dependency edge that would invert the layering.
///
/// **Every default is the conservative value**, so every reversion moves the
/// machine toward safety rather than away from it: a higher emergency threshold,
/// `HIGH_TRIGGER` relays, lower setpoints.
///
/// Two passes, then it gives up — see [`REPAIR_ESCALATION_KEYS`] for why one is
/// not enough. The caller falls back to the full-defaults behaviour it already
/// had, so the machine is never left running something the validator refuses.
pub fn repair_unsafe(
    config: &mut Config,
    validate: impl Fn(&Config) -> Option<&'static [&'static str]>,
) -> Repair {
    let mut repair = Repair::default();
    for pass in 0..2 {
        let Some(keys) = validate(config) else {
            repair.resolved = true;
            return repair;
        };
        let keys: &[&str] = if pass == 0 {
            keys
        } else {
            REPAIR_ESCALATION_KEYS
        };
        let mut reverted_any = false;
        for key in keys {
            if revert_key(config, key) && !repair.reverted.iter().any(|k| k == key) {
                repair.reverted.push(String::from(*key));
                reverted_any = true;
            }
        }
        if !reverted_any {
            break;
        }
    }
    repair.resolved = validate(config).is_none();
    repair
}
