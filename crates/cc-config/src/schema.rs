//! The parameter registry: every key the firmware persists, with its type,
//! default and validation range.
//!
//! The keys are the **C++ dotted names**, kept verbatim. The oracle firmware
//! (08 §5.3) reached the same conclusion independently — 97 recovered strings,
//! all in the C++ namespace — and 08 §6 records the reasoning: the names are
//! good, familiar, and the incompatibility with C++-written NVS is already
//! accepted (skill §1b, "no NVS backward compatibility"). Inventing a `cc.`
//! prefix would buy nothing and cost every operator who has memorised a key.
//!
//! # The count, and why it is 98
//!
//! `Config::getAllConfigParams()` (`src/Config.cpp:438-563`) returns **96**
//! parameters. Two more are defined in `Config.h` and read by
//! `EmergencyStopManager.cpp:18-19` but are *absent* from that list:
//!
//! * `safety.emergency_temp` (`Config.h:813`)
//! * `safety.emergency_hysteresis` (`Config.h:822`)
//!
//! Because they are never registered, they are never written to NVS, never
//! exported, and never imported. They silently reset to their compiled
//! defaults on every reboot — so a user who lowered the emergency threshold to
//! 130 °C through the web UI got 150 °C back after the next power cycle, with
//! no message. This is finding 1 of
//! [`docs/rust-migration/01-feature-inventory.md` §10](../../docs/rust-migration/01-feature-inventory.md)
//! and this port **fixes** it: both keys are in the schema, both are in
//! [`crate::Config`], and `schema_covers_every_registered_key` asserts the count.
//!
//! # Two C++ quirks preserved verbatim
//!
//! * **`steam.setpoint` and `safety.emergency_temp` both claim order 203** in
//!   section 1 (`Config.h:817` and `Config.h:837`). Harmless, and preserved for
//!   parity of `/api/parameters` ordering.
//! * **`ParamDef<String>` has no length limit.** `defaults.h:118-125` defines
//!   `MQTT_BROKER_MAX_LENGTH`, `USERNAME_MAX_LENGTH`, `PASSWORD_MAX_LENGTH`,
//!   `MQTT_TOPIC_MAX_LENGTH`, `MQTT_HASSIO_PREFIX_MAX_LENGTH`,
//!   `HOSTNAME_MAX_LENGTH`, `WIFI_SSID_MAX_LENGTH` and
//!   `WIFI_PASSWORD_MAX_LENGTH` — and **none of them is ever used**;
//!   `Config.h:isValid` returns `true` unconditionally for `String`. So the C++
//!   will happily store a 4 KB hostname. The constants are reproduced here as
//!   `MAX_KEY_LEN`/`MAX_TEXT_LEN` notes and the bounds are **not** enforced,
//!   because enforcing them would reject configurations the C++ accepts. See
//!   the crate report.
//!
//! # A note on `ParamValue::Text`
//!
//! The defaults are `&'static str` because every one of them is a string
//! literal in `defaults.h`. Values arriving from JSON borrow for the duration of
//! the validation, hence the lifetime parameter.

use cc_domain::hardware::{
    OledAddress, OledType, RelayTriggerType, ScaleType, SwitchMode, SwitchType,
    TemperatureSensorType,
};
use cc_domain::process::BrewMode;
use cc_domain::system::{DisplayTemplate, Language, LogLevel};

/// The longest dotted key in the schema, and the bound the C++ uses for a
/// single path segment.
///
/// `kMaxPathSegment = 64` in `src/ConfigJson.cpp:7`. The longest key here is
/// `hardware.sensors.watertank.keep_heater_on_empty` at 44 bytes, so 64 leaves
/// headroom for keys added later without another migration.
pub const MAX_KEY_LEN: usize = 64;

/// The device's default network name.
///
/// The C++ default is `"silvia"` (`include/clevercoffee/defaults.h:14`), which
/// is the product's own name. **This is a deliberate divergence**, decided
/// 2026-09-29: the development device is named for what it is, so that a
/// hostname on the network is never ambiguous about which firmware is running
/// it. `"test-cc-rust"` says both halves — it is a test device, and it is the
/// Rust port — which matters because the C++ and the Rust firmware are on the
/// same network during the migration and would otherwise collide on
/// `silvia.local`.
///
/// The C++ firmware is unchanged and still answers to `silvia`. An operator who
/// wants the product name back sets `system.hostname` in the config; nothing
/// else in the port depends on the value.
pub const DEFAULT_HOSTNAME: &str = "test-cc-rust";

/// The number of parameters this crate registers.
///
/// 96 as the C++ registers them, plus the two `safety.*` parameters it defines
/// but forgets to register.
pub const PARAM_COUNT: usize = 98;

/// What kind of value a parameter holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamKind {
    /// `bool`. Every boolean is always valid (`Config.h:isValid`).
    Bool,
    /// `i32`, range-checked.
    Int,
    /// `f64`, range-checked. The C++ stores these as `double`.
    Float,
    /// A string. The C++ performs no validation at all — see the module
    /// documentation.
    Text,
    /// An enumeration, stored as its integer discriminant so the JSON matches
    /// the C++ byte for byte (`Config.h:toJson` writes
    /// `static_cast<int>(currentValue_)`).
    Enum,
}

impl ParamKind {
    /// The C++ `ParamType` discriminant, which `/api/parameters` reports.
    ///
    /// From `Config.h:36-44`: `INT = 0, UINT8 = 1, DOUBLE = 2, FLOAT = 3,
    /// STRING = 4, ENUM = 5, BOOL = 6`.
    #[must_use]
    pub const fn cpp_param_type(self) -> u8 {
        match self {
            Self::Int => 0,
            Self::Float => 2,
            Self::Text => 4,
            Self::Enum => 5,
            Self::Bool => 6,
        }
    }
}

/// A parameter value, borrowed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamValue<'a> {
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    Int(i32),
    /// A double-precision float.
    Float(f64),
    /// A string.
    Text(&'a str),
    /// An enumeration discriminant.
    Enum(i8),
}

impl ParamValue<'_> {
    /// Which kind of parameter this value is for.
    #[must_use]
    pub const fn kind(self) -> ParamKind {
        match self {
            Self::Bool(_) => ParamKind::Bool,
            Self::Int(_) => ParamKind::Int,
            Self::Float(_) => ParamKind::Float,
            Self::Text(_) => ParamKind::Text,
            Self::Enum(_) => ParamKind::Enum,
        }
    }

    /// The value as an `f64`, for range checks. Text has no numeric form.
    #[must_use]
    pub fn as_f64(self) -> Option<f64> {
        match self {
            Self::Int(v) => Some(f64::from(v)),
            Self::Float(v) => Some(v),
            Self::Enum(v) => Some(f64::from(v)),
            Self::Bool(_) | Self::Text(_) => None,
        }
    }

    /// The value as a `bool`, if that is what it is.
    ///
    /// The typed counterparts of [`Self::as_f64`], for a caller that has to put
    /// the value into a typed field and so cannot afford the widening. They are
    /// `Option` rather than a defaulting conversion because a `ParamValue` is a
    /// `SCHEMA` default, and the whole point of asking is to tell two of them
    /// apart.
    #[must_use]
    pub fn as_bool(self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(v),
            _ => None,
        }
    }

    /// The value as an `i32`, if that is what it is.
    #[must_use]
    pub fn as_int(self) -> Option<i32> {
        match self {
            Self::Int(v) => Some(v),
            _ => None,
        }
    }

    /// The value as an enumeration discriminant, if that is what it is.
    #[must_use]
    pub fn as_enum(self) -> Option<i8> {
        match self {
            Self::Enum(v) => Some(v),
            _ => None,
        }
    }
}

/// One registered parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    /// The dotted C++ key, e.g. `"pid.regular.kp"`.
    pub key: &'static str,
    /// The value's type.
    pub kind: ParamKind,
    /// The compiled-in default.
    pub default: ParamValue<'static>,
    /// Inclusive lower bound, for the kinds that have one.
    pub min: Option<f64>,
    /// Inclusive upper bound, for the kinds that have one.
    pub max: Option<f64>,
}

impl ParamSpec {
    /// Build a spec. Used by the [`SCHEMA`] table.
    #[must_use]
    pub const fn new(
        key: &'static str,
        kind: ParamKind,
        default: ParamValue<'static>,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Self {
        Self {
            key,
            kind,
            default,
            min,
            max,
        }
    }

    /// The leaf name, i.e. the part after the last dot.
    #[must_use]
    pub fn leaf(&self) -> &'static str {
        match self.key.rsplit_once('.') {
            Some((_, leaf)) => leaf,
            None => self.key,
        }
    }

    /// Whether `value` is acceptable for this parameter.
    ///
    /// Port of `ParamDef::isValid` (`Config.h:190-200`): booleans and strings
    /// are always valid, everything else is range-checked with `>=` and `<=`.
    /// Enumerations are additionally checked against their option list in the
    /// C++ (`EnumParamDef::isValid`, `Config.h:379-388`); here an out-of-range
    /// discriminant fails the `from_raw` conversion before it can reach a
    /// `Config`, and [`SCHEMA`] bounds nothing for enums because the enum type
    /// itself is exhaustive.
    #[must_use]
    pub fn accepts(self, value: ParamValue<'_>) -> bool {
        if value.kind() != self.kind {
            return false;
        }
        match (self.min, self.max, value.as_f64()) {
            (Some(min), Some(max), Some(v)) => v >= min && v <= max,
            (Some(min), None, Some(v)) => v >= min,
            (None, Some(max), Some(v)) => v <= max,
            // Bools and strings never reach here: `as_f64` is `None`.
            _ => true,
        }
    }
}

/// Every parameter the firmware persists, in the C++ registration order.
///
/// The order is the C++ order (`getAllConfigParams`) with the two `safety.*`
/// parameters appended, so `/api/parameters` output keeps the same ordering as
/// the C++ build for the first 96 entries.
pub const SCHEMA: &[ParamSpec] = &[
    ParamSpec::new(
        "pid.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "pid.use_ponm",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "pid.ema_factor",
        ParamKind::Float,
        ParamValue::Float(0.6),
        Some(0.0),
        Some(1.0),
    ),
    ParamSpec::new(
        "pid.regular.kp",
        ParamKind::Float,
        ParamValue::Float(62.0),
        Some(0.0),
        Some(200.0),
    ),
    ParamSpec::new(
        "pid.regular.tn",
        ParamKind::Float,
        ParamValue::Float(52.0),
        Some(0.0),
        Some(200.0),
    ),
    ParamSpec::new(
        "pid.regular.tv",
        ParamKind::Float,
        ParamValue::Float(11.5),
        Some(0.0),
        Some(200.0),
    ),
    ParamSpec::new(
        "pid.regular.i_max",
        ParamKind::Float,
        ParamValue::Float(55.0),
        Some(0.0),
        Some(100.0),
    ),
    ParamSpec::new(
        "pid.steam.kp",
        ParamKind::Float,
        ParamValue::Float(150.0),
        Some(0.0),
        Some(500.0),
    ),
    ParamSpec::new(
        "brew.setpoint",
        ParamKind::Float,
        ParamValue::Float(95.0),
        Some(20.0),
        Some(110.0),
    ),
    ParamSpec::new(
        "brew.temp_offset",
        ParamKind::Float,
        ParamValue::Float(0.0),
        Some(0.0),
        Some(20.0),
    ),
    ParamSpec::new(
        "steam.setpoint",
        ParamKind::Float,
        ParamValue::Float(120.0),
        Some(100.0),
        Some(140.0),
    ),
    ParamSpec::new(
        "pid.bd.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "brew.pid_delay",
        ParamKind::Float,
        ParamValue::Float(10.0),
        Some(0.0),
        Some(60.0),
    ),
    ParamSpec::new(
        "pid.bd.kp",
        ParamKind::Float,
        ParamValue::Float(50.0),
        Some(0.0),
        Some(200.0),
    ),
    ParamSpec::new(
        "pid.bd.tn",
        ParamKind::Float,
        ParamValue::Float(0.0),
        Some(0.0),
        Some(200.0),
    ),
    ParamSpec::new(
        "pid.bd.tv",
        ParamKind::Float,
        ParamValue::Float(20.0),
        Some(0.0),
        Some(200.0),
    ),
    ParamSpec::new(
        "brew.mode",
        ParamKind::Enum,
        ParamValue::Enum(BrewMode::Manual as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "brew.by_time.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "brew.by_time.target_time",
        ParamKind::Float,
        ParamValue::Float(25.0),
        Some(1.0),
        Some(120.0),
    ),
    ParamSpec::new(
        "brew.by_weight.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "brew.by_weight.target_weight",
        ParamKind::Float,
        ParamValue::Float(36.0),
        Some(0.0),
        Some(500.0),
    ),
    ParamSpec::new(
        "brew.by_weight.auto_tare",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "brew.pre_infusion.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "brew.pre_infusion.time",
        ParamKind::Float,
        ParamValue::Float(2.0),
        Some(0.0),
        Some(60.0),
    ),
    ParamSpec::new(
        "brew.pre_infusion.pause",
        ParamKind::Float,
        ParamValue::Float(5.0),
        Some(0.0),
        Some(60.0),
    ),
    ParamSpec::new(
        "display.fullscreen_brew_timer",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "display.fullscreen_manual_flush_timer",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "display.fullscreen_hot_water_timer",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "display.post_brew_timer_duration",
        ParamKind::Float,
        ParamValue::Float(3.0),
        Some(0.0),
        Some(60.0),
    ),
    ParamSpec::new(
        "display.heating_logo",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
    ),
    ParamSpec::new(
        "display.pid_off_logo",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.leds.status.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.leds.status.inverted",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.leds.brew.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.leds.brew.inverted",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.leds.steam.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.leds.steam.inverted",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "display.template",
        ParamKind::Enum,
        ParamValue::Enum(DisplayTemplate::Standard as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "display.inverted",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "display.language",
        ParamKind::Enum,
        ParamValue::Enum(Language::English as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "display.blinking.delta",
        ParamKind::Float,
        ParamValue::Float(0.3),
        Some(0.2),
        Some(10.0),
    ),
    ParamSpec::new(
        "backflush.cycles",
        ParamKind::Int,
        ParamValue::Int(5),
        Some(2.0),
        Some(20.0),
    ),
    ParamSpec::new(
        "backflush.fill_time",
        ParamKind::Float,
        ParamValue::Float(5.0),
        Some(3.0),
        Some(10.0),
    ),
    ParamSpec::new(
        "backflush.flush_time",
        ParamKind::Float,
        ParamValue::Float(10.0),
        Some(5.0),
        Some(20.0),
    ),
    ParamSpec::new(
        "maintenance.backflush_reminder.enabled",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
    ),
    ParamSpec::new(
        "maintenance.backflush_reminder.threshold",
        ParamKind::Int,
        ParamValue::Int(50),
        Some(1.0),
        Some(500.0),
    ),
    ParamSpec::new(
        "standby.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "standby.time",
        ParamKind::Float,
        ParamValue::Float(35.0),
        Some(1.0),
        Some(120.0),
    ),
    ParamSpec::new(
        "mqtt.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "mqtt.broker",
        ParamKind::Text,
        ParamValue::Text(""),
        None,
        None,
    ),
    ParamSpec::new(
        "mqtt.port",
        ParamKind::Int,
        ParamValue::Int(1883),
        Some(1.0),
        Some(65535.0),
    ),
    ParamSpec::new(
        "mqtt.username",
        ParamKind::Text,
        ParamValue::Text("rancilio"),
        None,
        None,
    ),
    ParamSpec::new(
        "mqtt.password",
        ParamKind::Text,
        ParamValue::Text("silvia"),
        None,
        None,
    ),
    ParamSpec::new(
        "mqtt.topic",
        ParamKind::Text,
        ParamValue::Text("custom/kitchen/"),
        None,
        None,
    ),
    ParamSpec::new(
        "mqtt.hassio.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "mqtt.hassio.prefix",
        ParamKind::Text,
        ParamValue::Text("homeassistant"),
        None,
        None,
    ),
    ParamSpec::new(
        "system.hostname",
        ParamKind::Text,
        ParamValue::Text(DEFAULT_HOSTNAME),
        None,
        None,
    ),
    ParamSpec::new(
        "system.ota_password",
        ParamKind::Text,
        ParamValue::Text("otapass"),
        None,
        None,
    ),
    ParamSpec::new(
        "system.offline_mode",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "system.log_level",
        ParamKind::Enum,
        ParamValue::Enum(LogLevel::Info as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "system.auth.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "system.auth.username",
        ParamKind::Text,
        ParamValue::Text("admin"),
        None,
        None,
    ),
    ParamSpec::new(
        "system.auth.password",
        ParamKind::Text,
        ParamValue::Text("admin"),
        None,
        None,
    ),
    ParamSpec::new(
        "system.timing_debug.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "system.showdisplay.enabled",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
    ),
    ParamSpec::new(
        "system.wifi.ssid",
        ParamKind::Text,
        ParamValue::Text(""),
        None,
        None,
    ),
    ParamSpec::new(
        "system.wifi.password",
        ParamKind::Text,
        ParamValue::Text(""),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.oled.enabled",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.oled.type",
        ParamKind::Enum,
        ParamValue::Enum(OledType::Ssd1306 as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.oled.address",
        ParamKind::Enum,
        ParamValue::Enum(OledAddress::Addr3c as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.relays.heater.trigger_type",
        ParamKind::Enum,
        ParamValue::Enum(RelayTriggerType::HighTrigger as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.relays.valve.trigger_type",
        ParamKind::Enum,
        ParamValue::Enum(RelayTriggerType::HighTrigger as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.relays.pump.trigger_type",
        ParamKind::Enum,
        ParamValue::Enum(RelayTriggerType::HighTrigger as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.brew.enabled",
        ParamKind::Bool,
        // `true`, not the C++'s `false` (`Config.h:985`) — see
        // `HardwareSwitchesBrew::enabled` and intentional-diffs.md.
        ParamValue::Bool(true),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.brew.type",
        ParamKind::Enum,
        ParamValue::Enum(SwitchType::Toggle as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.brew.mode",
        ParamKind::Enum,
        ParamValue::Enum(SwitchMode::NormallyOpen as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.steam.enabled",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.steam.type",
        ParamKind::Enum,
        ParamValue::Enum(SwitchType::Toggle as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.steam.mode",
        ParamKind::Enum,
        ParamValue::Enum(SwitchMode::NormallyOpen as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.power.enabled",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.power.type",
        ParamKind::Enum,
        ParamValue::Enum(SwitchType::Toggle as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.power.mode",
        ParamKind::Enum,
        ParamValue::Enum(SwitchMode::NormallyOpen as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.hot_water.enabled",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.hot_water.type",
        ParamKind::Enum,
        ParamValue::Enum(SwitchType::Toggle as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.switches.hot_water.mode",
        ParamKind::Enum,
        ParamValue::Enum(SwitchMode::NormallyOpen as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.sensors.temperature.type",
        ParamKind::Enum,
        // TSIC_306, the C++'s value at `Config.h:1085-1092`. Both the schema
        // default and the struct default must agree, or `Config::default()` and
        // `schema::SCHEMA` diverge and the export test fails. See
        // `HardwareSensorsTemperature`'s Default impl for why this was
        // `DALLAS_DS18B20` in an earlier revision and is not now.
        ParamValue::Enum(TemperatureSensorType::Tsic306 as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.sensors.pressure.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.sensors.watertank.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.sensors.watertank.mode",
        ParamKind::Enum,
        ParamValue::Enum(SwitchMode::NormallyClosed as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.sensors.watertank.keep_heater_on_empty",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.sensors.scale.enabled",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.sensors.scale.samples",
        ParamKind::Int,
        ParamValue::Int(2),
        Some(1.0),
        Some(20.0),
    ),
    ParamSpec::new(
        "hardware.sensors.scale.type",
        ParamKind::Enum,
        ParamValue::Enum(ScaleType::Hx711Dual as i8),
        None,
        None,
    ),
    ParamSpec::new(
        "hardware.sensors.scale.calibration",
        ParamKind::Float,
        ParamValue::Float(1.0),
        Some(-999_999.0),
        Some(999_999.0),
    ),
    ParamSpec::new(
        "hardware.sensors.scale.calibration2",
        ParamKind::Float,
        ParamValue::Float(1.0),
        Some(-999_999.0),
        Some(999_999.0),
    ),
    ParamSpec::new(
        "hardware.sensors.scale.known_weight",
        ParamKind::Float,
        ParamValue::Float(267.0),
        Some(1.0),
        Some(2000.0),
    ),
    // ---- the two parameters the C++ defines but never registers ----------
    // `Config.h:813` and `Config.h:822`; read by `EmergencyStopManager.cpp:18-19`
    // and missing from `Config::getAllConfigParams()`. See the module docs.
    ParamSpec::new(
        "safety.emergency_temp",
        ParamKind::Float,
        ParamValue::Float(150.0),
        Some(120.0),
        Some(180.0),
    ),
    ParamSpec::new(
        "safety.emergency_hysteresis",
        ParamKind::Float,
        ParamValue::Float(5.0),
        Some(1.0),
        Some(15.0),
    ),
];

/// Look up a parameter by its dotted key.
#[must_use]
pub fn find(key: &str) -> Option<&'static ParamSpec> {
    SCHEMA.iter().find(|spec| spec.key == key)
}

/// The default value tree implied by [`SCHEMA`], as a nested JSON object.
///
/// Exists so a test can assert that [`crate::Config::default`] agrees with the
/// registry — the two are written independently and would otherwise drift, and
/// the drift would be invisible: a mistyped default is a machine that heats to
/// the wrong temperature.
#[must_use]
pub fn default_tree() -> serde_json::Value {
    use alloc::{string::String, vec::Vec};
    use serde_json::{Map, Value};

    let mut root = Map::new();
    for spec in SCHEMA {
        let segments: Vec<&str> = spec.key.split('.').collect();
        let mut node = &mut root;
        for segment in &segments[..segments.len() - 1] {
            let key = String::from(*segment);
            let entry = node.entry(key).or_insert_with(|| Value::Object(Map::new()));
            if !entry.is_object() {
                *entry = Value::Object(Map::new());
            }
            // The entry was just inserted, or just replaced, with an object.
            node = match entry.as_object_mut() {
                Some(map) => map,
                None => unreachable!("inserted as an object one line above"),
            };
        }
        let value = match spec.default {
            ParamValue::Bool(v) => Value::Bool(v),
            ParamValue::Int(v) => Value::from(v),
            ParamValue::Float(v) => Value::from(v),
            ParamValue::Text(v) => Value::from(v),
            ParamValue::Enum(v) => Value::from(v),
        };
        node.insert(String::from(segments[segments.len() - 1]), value);
    }
    Value::Object(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_domain::hardware::RelayTriggerType;

    #[test]
    fn the_schema_has_ninety_eight_entries() {
        assert_eq!(SCHEMA.len(), PARAM_COUNT);
        assert_eq!(SCHEMA.len(), 98);
    }

    #[test]
    fn every_key_is_unique() {
        for (i, a) in SCHEMA.iter().enumerate() {
            for b in &SCHEMA[i + 1..] {
                assert_ne!(a.key, b.key, "duplicate key {}", a.key);
            }
        }
    }

    #[test]
    fn every_key_fits_the_cpp_path_segment_bound() {
        for spec in SCHEMA {
            assert!(
                spec.key.len() <= MAX_KEY_LEN,
                "{} exceeds the C++ kMaxPathSegment of {MAX_KEY_LEN}",
                spec.key
            );
            for segment in spec.key.split('.') {
                assert!(!segment.is_empty(), "{} has an empty segment", spec.key);
            }
        }
    }

    /// The regression test for finding 1 of 01 §10: the two `safety.*`
    /// parameters must be registered. Against the C++ behaviour this test does
    /// not exist and the parameters are silently dropped on every reboot.
    #[test]
    fn the_two_safety_parameters_are_registered() {
        let temp = find("safety.emergency_temp").expect("safety.emergency_temp must be registered");
        assert_eq!(temp.kind, ParamKind::Float);
        assert_eq!(temp.default, ParamValue::Float(150.0));
        assert_eq!(temp.min, Some(120.0));
        assert_eq!(temp.max, Some(180.0));

        let hyst = find("safety.emergency_hysteresis")
            .expect("safety.emergency_hysteresis must be registered");
        assert_eq!(hyst.kind, ParamKind::Float);
        assert_eq!(hyst.default, ParamValue::Float(5.0));
        assert_eq!(hyst.min, Some(1.0));
        assert_eq!(hyst.max, Some(15.0));
    }

    #[test]
    fn range_checks_reject_out_of_bounds_values() {
        let setpoint = find("brew.setpoint").expect("registered");
        assert!(setpoint.accepts(ParamValue::Float(95.0)));
        assert!(
            setpoint.accepts(ParamValue::Float(20.0)),
            "min is inclusive"
        );
        assert!(
            setpoint.accepts(ParamValue::Float(110.0)),
            "max is inclusive"
        );
        assert!(!setpoint.accepts(ParamValue::Float(19.9)));
        assert!(!setpoint.accepts(ParamValue::Float(110.1)));
    }

    #[test]
    fn booleans_and_strings_are_always_valid() {
        // Config.h:isValid returns true unconditionally for bool and String.
        let pid = find("pid.enabled").expect("registered");
        assert!(pid.accepts(ParamValue::Bool(true)));
        assert!(pid.accepts(ParamValue::Bool(false)));
        let host = find("system.hostname").expect("registered");
        assert!(host.accepts(ParamValue::Text("")));
        assert!(host.accepts(ParamValue::Text(&"x".repeat(4096))));
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_rejected() {
        let setpoint = find("brew.setpoint").expect("registered");
        assert!(!setpoint.accepts(ParamValue::Bool(true)));
        assert!(!setpoint.accepts(ParamValue::Text("95")));
    }

    #[test]
    fn cpp_param_types_match_the_cpp_enum() {
        // Config.h:36-44
        assert_eq!(ParamKind::Int.cpp_param_type(), 0);
        assert_eq!(ParamKind::Float.cpp_param_type(), 2);
        assert_eq!(ParamKind::Text.cpp_param_type(), 4);
        assert_eq!(ParamKind::Enum.cpp_param_type(), 5);
        assert_eq!(ParamKind::Bool.cpp_param_type(), 6);
    }

    #[test]
    fn the_default_tree_has_every_leaf() {
        let tree = default_tree();
        for spec in SCHEMA {
            let mut node = &tree;
            let mut found = true;
            for segment in spec.key.split('.') {
                let Some(next) = node.get(segment) else {
                    found = false;
                    break;
                };
                node = next;
            }
            assert!(found, "{} is missing from the default tree", spec.key);
        }
    }

    #[test]
    fn the_heater_relay_default_is_high_trigger() {
        let spec = find("hardware.relays.heater.trigger_type").expect("registered");
        assert_eq!(
            spec.default,
            ParamValue::Enum(RelayTriggerType::HighTrigger as i8)
        );
    }

    #[test]
    fn leaf_splits_on_the_last_dot() {
        let spec = find("pid.regular.i_max").expect("registered");
        assert_eq!(spec.leaf(), "i_max");
    }
}
