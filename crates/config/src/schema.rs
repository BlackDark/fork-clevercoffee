//! The parameter schema: every setting, its type, range, default and whether it is a secret.
//!
//! One declarative table. The C++ firmware had 96 parameters spread across a 1 567-line header,
//! a hand-maintained pointer list in `Config::getAllConfigParams()` that a new parameter could be
//! silently missing from, and a second hand-maintained list in the frontend. That is how
//! `safety.emergency_temp` came to be read by the emergency-stop manager while being absent from
//! the registry, so a user who lowered the emergency threshold was silently ignored (defect D12).
//!
//! Here the table *is* the registry. A field that is not in this table does not exist, and the
//! import, the export, the API and the MQTT bridge all read the same rows.

use heapless::String;

/// The type of a parameter's value. Four types, because those are the four the wire format
/// supports. The C++ `ParamType` also declared `UINT8`, `FLOAT` and a second `DOUBLE` that
/// nothing ever produced and that `ParamDef<float>` could not actually load or save (defect D43).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueType {
    Bool,
    Int,
    /// A `f64`. JSON has one number type, so this covers anything fractional.
    Number,
    Enum,
    /// A UTF-8 string with a length limit that is actually enforced. The C++ `isValid` returned
    /// true for every string and the eight length constants were referenced nowhere (defect D23).
    Text,
}

/// Which domain a parameter belongs to. Used by the API's `filter` query, which the C++
/// implementation got wrong because it filtered on a hand-written section number (defect D26).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Pid,
    Brew,
    Steam,
    Display,
    Backflush,
    Maintenance,
    Standby,
    Mqtt,
    Hardware,
    System,
    Safety,
}

impl Group {
    pub const fn name(self) -> &'static str {
        match self {
            Group::Pid => "pid",
            Group::Brew => "brew",
            Group::Steam => "steam",
            Group::Display => "display",
            Group::Backflush => "backflush",
            Group::Maintenance => "maintenance",
            Group::Standby => "standby",
            Group::Mqtt => "mqtt",
            Group::Hardware => "hardware",
            Group::System => "system",
            Group::Safety => "safety",
        }
    }

    /// The ten groups the C++ export emitted. `safety` is new: the C++ firmware had the two
    /// parameters but never exported them.
    pub const ALL: [Group; 10] = [
        Group::Pid,
        Group::Brew,
        Group::Steam,
        Group::Display,
        Group::Backflush,
        Group::Maintenance,
        Group::Standby,
        Group::Mqtt,
        Group::Hardware,
        Group::System,
    ];

    /// Every group, including , which the C++ firmware never exported because its two
    /// parameters were never registered (defect D12). An export that omits them cannot be
    /// re-imported with them set, so they have to be in the document.
    pub const ALL_ALL: [Group; 11] = [
        Group::Pid,
        Group::Brew,
        Group::Steam,
        Group::Safety,
        Group::Display,
        Group::Backflush,
        Group::Maintenance,
        Group::Standby,
        Group::Mqtt,
        Group::Hardware,
        Group::System,
    ];
}

/// A parameter's value. Small enough to copy, and it needs no allocation.
///
/// The lifetime is on the string variant because a value comes from three places with three
/// lifetimes: the compiled defaults, the flash, and a document the user just uploaded. Borrowing
/// keeps all three free, and a value never outlives the buffer it was decoded from.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Value<'a> {
    Bool(bool),
    Int(i32),
    Number(f64),
    Enum(i32),
    /// At most [`MAX_TEXT`] bytes. Longer values are rejected at the boundary, so a value that
    /// reaches the store is always this long or shorter.
    Text(&'a str),
}

/// The length limit for a text parameter. The longest allowed value in the table is the MQTT
/// topic prefix at 24, so 64 is a round number that covers every field with headroom, and the
/// per-field limit is separate and enforced against the value's own field.
pub const MAX_TEXT: usize = 64;

/// One row of the schema.
#[derive(Clone, Copy, Debug)]
pub struct Param<'a> {
    /// The dotted path, which is the key in the JSON, the name in the API and the MQTT topic.
    pub key: &'static str,
    pub group: Group,
    pub kind: ValueType,
    pub default: Value<'a>,
    /// Inclusive lower bound for `Int` and `Number`. Meaningless for the others.
    pub min: f64,
    /// Inclusive upper bound. Meaningless for the others.
    pub max: f64,
    /// Maximum bytes for `Text`, enforced.
    pub max_len: usize,
    /// True when the value must never appear in an export, a status response or a log.
    pub secret: bool,
    /// True when zero is not a usable value even though it is inside the range. Only the scale
    /// calibration, which is a divisor: a negative calibration is a real inverted load cell and
    /// appears in the shipped `config.json` as -1750.05, but a zero divisor destroys every
    /// reading, so the range cannot express the rule and a flag does.
    pub forbid_zero: bool,
    /// Short help text, shown by the UI.
    pub help: &'static str,
}

impl<'a> Param<'a> {
    /// The display order, which is this parameter's position in [`PARAMS`].
    ///
    /// A method rather than a field, so the value cannot drift from the table. The C++ table
    /// carried a hand-written `order` with two pairs of duplicates, so those four parameters came
    /// out in a non-deterministic sequence (defect D42).
    pub fn order(&self) -> u16 {
        self.index() as u16
    }

    fn index(&self) -> usize {
        PARAMS
            .iter()
            .position(|q| core::ptr::eq(q, self))
            .unwrap_or(0)
    }
}

impl<'a> Param<'a> {
    /// Whether a value is acceptable for this parameter.
    ///
    /// One function, called on every write and on every decode, so a value that reaches the
    /// machine has been checked. The C++ `set()` checked and `loadFromNvs()` did not, which is
    /// how a corrupted blob put an arbitrary setpoint into live control (defect D11).
    pub fn accepts(&self, value: Value<'_>) -> bool {
        match (self.kind, value) {
            (ValueType::Bool, Value::Bool(_)) | (ValueType::Enum, Value::Enum(_)) => true,
            (ValueType::Int, Value::Int(v)) => {
                let f = v as f64;
                f >= self.min && f <= self.max
            }
            (ValueType::Number, Value::Number(v)) => {
                v.is_finite() && v >= self.min && v <= self.max && !(self.forbid_zero && v == 0.0)
            }
            (ValueType::Text, Value::Text(v)) => v.len() <= self.max_len && is_valid_utf8(v),
            // A type mismatch is never acceptable, rather than being coerced. The C++ importer
            // routed numbers through `String(double)`, which truncated to two decimals, so an
            // ema_factor of 0.005 imported as 0.01 (defect D27).
            _ => false,
        }
    }

    /// Clamps a numeric value into range. Only reachable for a value the user asked to clamp;
    /// the default behaviour is to reject, because a silently clamped setpoint is a safety
    /// problem the user cannot see.
    pub fn clamp(&self, value: f64) -> f64 {
        value.clamp(self.min, self.max)
    }
}

/// Rejects control characters other than none at all.
///
/// A hostname, an SSID or an MQTT topic is written to a C string, put in a URL and printed in a
/// log. A value carrying a newline or a NUL can corrupt a log line or truncate a value
/// mid-string, so the schema refuses those bytes at the boundary rather than trusting every
/// caller. `str` is already valid UTF-8, so only the control-character check is needed.
fn is_valid_utf8(s: &str) -> bool {
    !s.chars().any(|c| c.is_control())
}

macro_rules! params {
    ($($konst:ident => $key:literal, $group:ident, $kind:ident, $default:expr, $min:expr, $max:expr, $max_len:expr, $secret:expr, $forbid_zero:expr, $help:literal;)*) => {
        /// The complete parameter list. One row per setting; nothing outside this table exists.
        pub static PARAMS: &[Param<'static>] = &[
            $(Param {
                key: $key,
                group: Group::$group,
                kind: ValueType::$kind,
                default: $default,
                min: $min,
                max: $max,
                max_len: $max_len,
                secret: $secret,
                forbid_zero: $forbid_zero,
                help: $help,
            },)*
        ];
    };
}

/// The parameters in display order, which is their position in the table.
pub fn ordered() -> impl Iterator<Item = &'static Param<'static>> {
    PARAMS.iter()
}

params! {
    // --- pid ---------------------------------------------------------------------------------
    P_PID_ENABLED => "pid.enabled", Pid, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Run the PID controller";
    P_PID_USE_PONM => "pid.use_ponm", Pid, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Proportional on measurement instead of on error";
    P_PID_EMA => "pid.ema_factor", Pid, Number, Value::Number(0.6), 0.0, 1.0, 0, false, false, "Input filter smoothing, 0 to 1";
    P_PID_KP => "pid.regular.kp", Pid, Number, Value::Number(62.0), 0.0, 200.0, 0, false, false, "Proportional gain";
    P_PID_TN => "pid.regular.tn", Pid, Number, Value::Number(52.0), 0.0, 200.0, 0, false, false, "Integral time constant, seconds";
    P_PID_TV => "pid.regular.tv", Pid, Number, Value::Number(11.5), 0.0, 200.0, 0, false, false, "Derivative time constant, seconds";
    P_PID_IMAX => "pid.regular.i_max", Pid, Number, Value::Number(55.0), 0.0, 100.0, 0, false, false, "Integral term limit";
    P_PID_STEAM_KP => "pid.steam.kp", Pid, Number, Value::Number(150.0), 0.0, 500.0, 0, false, false, "Proportional gain in steam mode";
    P_PID_BD_ENABLED => "pid.bd.enabled", Pid, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Use the brew-detection gain set";
    P_PID_BD_KP => "pid.bd.kp", Pid, Number, Value::Number(50.0), 0.0, 200.0, 0, false, false, "Brew-detection proportional gain";
    P_PID_BD_TN => "pid.bd.tn", Pid, Number, Value::Number(0.0), 0.0, 200.0, 0, false, false, "Brew-detection integral time constant";
    P_PID_BD_TV => "pid.bd.tv", Pid, Number, Value::Number(20.0), 0.0, 200.0, 0, false, false, "Brew-detection derivative time constant";

    // --- brew --------------------------------------------------------------------------------
    P_BREW_SETPOINT => "brew.setpoint", Brew, Number, Value::Number(95.0), 20.0, 110.0, 0, false, false, "Brew temperature, degrees C";
    P_BREW_TEMP_OFFSET => "brew.temp_offset", Brew, Number, Value::Number(0.0), 0.0, 20.0, 0, false, false, "Brew temperature offset, degrees C";
    P_BREW_PID_DELAY => "brew.pid_delay", Brew, Number, Value::Number(10.0), 0.0, 60.0, 0, false, false, "Delay before the heater engages during a brew, seconds";
    P_BREW_MODE => "brew.mode", Brew, Enum, Value::Enum(0), 0.0, 1.0, 0, false, false, "0 manual, 1 automatic";
    P_BREW_BY_TIME_ENABLED => "brew.by_time.enabled", Brew, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Stop the shot on a timer";
    P_BREW_TARGET_TIME => "brew.by_time.target_time", Brew, Number, Value::Number(25.0), 1.0, 120.0, 0, false, false, "Target brew time, seconds, including pre-infusion";
    P_BREW_BY_WEIGHT_ENABLED => "brew.by_weight.enabled", Brew, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Stop the shot on the scale";
    P_BREW_TARGET_WEIGHT => "brew.by_weight.target_weight", Brew, Number, Value::Number(36.0), 0.0, 500.0, 0, false, false, "Target brew weight, grams";
    P_BREW_AUTO_TARE => "brew.by_weight.auto_tare", Brew, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Tare the scale when the shot starts";
    P_BREW_PREINFUSION_ENABLED => "brew.pre_infusion.enabled", Brew, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Pre-infuse before the shot";
    P_BREW_PREINFUSION_TIME => "brew.pre_infusion.time", Brew, Number, Value::Number(2.0), 0.0, 60.0, 0, false, false, "Pre-infusion time, seconds";
    P_BREW_PREINFUSION_PAUSE => "brew.pre_infusion.pause", Brew, Number, Value::Number(5.0), 0.0, 60.0, 0, false, false, "Pause after pre-infusion, seconds";

    // --- steam -------------------------------------------------------------------------------
    P_STEAM_SETPOINT => "steam.setpoint", Steam, Number, Value::Number(120.0), 100.0, 140.0, 0, false, false, "Steam temperature, degrees C";

    // --- safety ------------------------------------------------------------------------------
    // These two existed in the C++ code and were read by the emergency-stop manager, but were
    // never registered, so they were never stored, exported, imported, or settable through the
    // API. Defect D12.
    P_SAFETY_EMERGENCY_TEMP => "safety.emergency_temp", Safety, Number, Value::Number(150.0), 120.0, 180.0, 0, false, false, "Emergency stop temperature, degrees C";
    P_SAFETY_EMERGENCY_HYSTERESIS => "safety.emergency_hysteresis", Safety, Number, Value::Number(5.0), 1.0, 15.0, 0, false, false, "Emergency stop hysteresis, degrees C";

    // --- display -----------------------------------------------------------------------------
    P_DISPLAY_TEMPLATE => "display.template", Display, Enum, Value::Enum(0), 0.0, 5.0, 0, false, false, "0 standard, 1 minimal, 2 temperature, 3 scale, 4 upright, 5 modern";
    P_DISPLAY_INVERTED => "display.inverted", Display, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Invert the display";
    P_DISPLAY_LANGUAGE => "display.language", Display, Enum, Value::Enum(0), 0.0, 2.0, 0, false, false, "0 English, 1 German, 2 Spanish";
    P_DISPLAY_FS_BREW => "display.fullscreen_brew_timer", Display, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Full screen during the brew timer";
    P_DISPLAY_FS_FLUSH => "display.fullscreen_manual_flush_timer", Display, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Full screen during the manual flush timer";
    P_DISPLAY_FS_HOT_WATER => "display.fullscreen_hot_water_timer", Display, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Full screen during the hot water timer";
    P_DISPLAY_POST_BREW => "display.post_brew_timer_duration", Display, Number, Value::Number(3.0), 0.0, 60.0, 0, false, false, "Brew timer shown after the shot, seconds";
    P_DISPLAY_HEATING_LOGO => "display.heating_logo", Display, Bool, Value::Bool(true), 0.0, 0.0, 0, false, false, "Show the heating logo";
    P_DISPLAY_PID_OFF_LOGO => "display.pid_off_logo", Display, Bool, Value::Bool(true), 0.0, 0.0, 0, false, false, "Show a logo when the heater is off";
    P_DISPLAY_BLINK_DELTA => "display.blinking.delta", Display, Number, Value::Number(0.3), 0.2, 10.0, 0, false, false, "Temperature swing that blinks the display";

    // --- backflush ---------------------------------------------------------------------------
    P_BACKFLUSH_CYCLES => "backflush.cycles", Backflush, Int, Value::Int(5), 2.0, 20.0, 0, false, false, "Number of backflush cycles";
    P_BACKFLUSH_FILL => "backflush.fill_time", Backflush, Number, Value::Number(5.0), 3.0, 10.0, 0, false, false, "Fill time per cycle, seconds";
    P_BACKFLUSH_FLUSH => "backflush.flush_time", Backflush, Number, Value::Number(10.0), 5.0, 20.0, 0, false, false, "Flush time per cycle, seconds";

    // --- maintenance -------------------------------------------------------------------------
    P_MAINT_REMINDER_ENABLED => "maintenance.backflush_reminder.enabled", Maintenance, Bool, Value::Bool(true), 0.0, 0.0, 0, false, false, "Remind the user to backflush";
    P_MAINT_REMINDER_THRESHOLD => "maintenance.backflush_reminder.threshold", Maintenance, Int, Value::Int(50), 1.0, 500.0, 0, false, false, "Shots before the backflush reminder";

    // --- standby -----------------------------------------------------------------------------
    P_STANDBY_ENABLED => "standby.enabled", Standby, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Enter standby after a period of inactivity";
    P_STANDBY_TIME => "standby.time", Standby, Number, Value::Number(35.0), 1.0, 120.0, 0, false, false, "Idle time before standby, minutes";

    // --- mqtt --------------------------------------------------------------------------------
    P_MQTT_ENABLED => "mqtt.enabled", Mqtt, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Publish to an MQTT broker";
    P_MQTT_BROKER => "mqtt.broker", Mqtt, Text, Value::Text(""), 0.0, 0.0, 64, false, false, "Broker host name";
    P_MQTT_PORT => "mqtt.port", Mqtt, Int, Value::Int(1883), 1.0, 65535.0, 0, false, false, "Broker port";
    P_MQTT_USERNAME => "mqtt.username", Mqtt, Text, Value::Text("rancilio"), 0.0, 0.0, 32, false, false, "Broker user name";
    P_MQTT_PASSWORD => "mqtt.password", Mqtt, Text, Value::Text("silvia"), 0.0, 0.0, 64, true, false, "Broker password";
    P_MQTT_TOPIC => "mqtt.topic", Mqtt, Text, Value::Text("custom/kitchen/"), 0.0, 0.0, 48, false, false, "Topic prefix";
    P_MQTT_HASSIO_ENABLED => "mqtt.hassio.enabled", Mqtt, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Publish Home Assistant discovery";
    P_MQTT_HASSIO_PREFIX => "mqtt.hassio.prefix", Mqtt, Text, Value::Text("homeassistant"), 0.0, 0.0, 24, false, false, "Home Assistant discovery prefix";

    // --- hardware ---------------------------------------------------------------------------
    P_HW_BOARD => "hardware.board", Hardware, Enum, Value::Enum(0), 0.0, 2.0, 0, false, false, "0 ESP32, 1 ESP32-S3, 2 ESP32-C6";
    P_HW_OLED_ENABLED => "hardware.oled.enabled", Hardware, Bool, Value::Bool(true), 0.0, 0.0, 0, false, false, "Fit an OLED";
    P_HW_OLED_TYPE => "hardware.oled.type", Hardware, Enum, Value::Enum(0), 0.0, 1.0, 0, false, false, "0 SSD1306, 1 SH1106";
    P_HW_OLED_ADDRESS => "hardware.oled.address", Hardware, Enum, Value::Enum(0), 0.0, 1.0, 0, false, false, "0 for 0x3C, 1 for 0x3D";
    P_HW_RELAY_HEATER => "hardware.relays.heater.trigger_type", Hardware, Enum, Value::Enum(1), 0.0, 1.0, 0, false, false, "0 low trigger, 1 high trigger";
    P_HW_RELAY_VALVE => "hardware.relays.valve.trigger_type", Hardware, Enum, Value::Enum(1), 0.0, 1.0, 0, false, false, "0 low trigger, 1 high trigger";
    P_HW_RELAY_PUMP => "hardware.relays.pump.trigger_type", Hardware, Enum, Value::Enum(1), 0.0, 1.0, 0, false, false, "0 low trigger, 1 high trigger";
    P_HW_SW_BREW_ENABLED => "hardware.switches.brew.enabled", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Fit a brew switch";
    P_HW_SW_BREW_TYPE => "hardware.switches.brew.type", Hardware, Enum, Value::Enum(1), 0.0, 1.0, 0, false, false, "0 momentary, 1 toggle";
    P_HW_SW_BREW_MODE => "hardware.switches.brew.mode", Hardware, Enum, Value::Enum(0), 0.0, 1.0, 0, false, false, "0 normally open, 1 normally closed";
    P_HW_SW_STEAM_ENABLED => "hardware.switches.steam.enabled", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Fit a steam switch";
    P_HW_SW_STEAM_TYPE => "hardware.switches.steam.type", Hardware, Enum, Value::Enum(1), 0.0, 1.0, 0, false, false, "0 momentary, 1 toggle";
    P_HW_SW_STEAM_MODE => "hardware.switches.steam.mode", Hardware, Enum, Value::Enum(0), 0.0, 1.0, 0, false, false, "0 normally open, 1 normally closed";
    P_HW_SW_POWER_ENABLED => "hardware.switches.power.enabled", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Fit a power switch";
    P_HW_SW_POWER_TYPE => "hardware.switches.power.type", Hardware, Enum, Value::Enum(1), 0.0, 1.0, 0, false, false, "0 momentary, 1 toggle";
    P_HW_SW_POWER_MODE => "hardware.switches.power.mode", Hardware, Enum, Value::Enum(0), 0.0, 1.0, 0, false, false, "0 normally open, 1 normally closed";
    P_HW_SW_HOTWATER_ENABLED => "hardware.switches.hot_water.enabled", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Fit a hot water switch";
    P_HW_SW_HOTWATER_TYPE => "hardware.switches.hot_water.type", Hardware, Enum, Value::Enum(1), 0.0, 1.0, 0, false, false, "0 momentary, 1 toggle";
    P_HW_SW_HOTWATER_MODE => "hardware.switches.hot_water.mode", Hardware, Enum, Value::Enum(0), 0.0, 1.0, 0, false, false, "0 normally open, 1 normally closed";
    P_HW_LED_STATUS_ENABLED => "hardware.leds.status.enabled", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Fit a status LED";
    P_HW_LED_STATUS_INVERTED => "hardware.leds.status.inverted", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Invert the status LED";
    P_HW_LED_BREW_ENABLED => "hardware.leds.brew.enabled", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Fit a brew LED";
    P_HW_LED_BREW_INVERTED => "hardware.leds.brew.inverted", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Invert the brew LED";
    P_HW_LED_STEAM_ENABLED => "hardware.leds.steam.enabled", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Fit a steam LED";
    P_HW_LED_STEAM_INVERTED => "hardware.leds.steam.inverted", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Invert the steam LED";
    P_HW_TEMP_TYPE => "hardware.sensors.temperature.type", Hardware, Enum, Value::Enum(0), 0.0, 1.0, 0, false, false, "0 TSIC 306, 1 DS18B20";
    P_HW_PRESSURE_ENABLED => "hardware.sensors.pressure.enabled", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Fit a pressure sensor";
    P_HW_TANK_ENABLED => "hardware.sensors.watertank.enabled", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Fit a water tank sensor";
    P_HW_TANK_MODE => "hardware.sensors.watertank.mode", Hardware, Enum, Value::Enum(1), 0.0, 1.0, 0, false, false, "0 normally open, 1 normally closed";
    P_HW_TANK_KEEP_HEATER => "hardware.sensors.watertank.keep_heater_on_empty", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Keep heating while the tank is empty";
    P_HW_SCALE_ENABLED => "hardware.sensors.scale.enabled", Hardware, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Fit a scale";
    P_HW_SCALE_SAMPLES => "hardware.sensors.scale.samples", Hardware, Int, Value::Int(2), 1.0, 20.0, 0, false, false, "Readings averaged into a weight";
    P_HW_SCALE_TYPE => "hardware.sensors.scale.type", Hardware, Enum, Value::Enum(0), 0.0, 2.0, 0, false, false, "0 dual cell, 1 single cell, 2 Bluetooth";
    P_HW_SCALE_CAL => "hardware.sensors.scale.calibration", Hardware, Number, Value::Number(1.0), -999999.0, 999999.0, 0, false, true, "Calibration divisor, applied to the raw counts";
    P_HW_SCALE_CAL2 => "hardware.sensors.scale.calibration2", Hardware, Number, Value::Number(1.0), -999999.0, 999999.0, 0, false, true, "Calibration divisor for the second load cell";
    P_HW_SCALE_KNOWN => "hardware.sensors.scale.known_weight", Hardware, Number, Value::Number(267.0), 1.0, 2000.0, 0, false, false, "Weight used for calibration, grams";

    // --- system ------------------------------------------------------------------------------
    P_SYS_HOSTNAME => "system.hostname", System, Text, Value::Text("silvia"), 0.0, 0.0, 64, false, false, "Network host name";
    P_SYS_OTA_PASSWORD => "system.ota_password", System, Text, Value::Text("otapass"), 0.0, 0.0, 64, true, false, "Password required to push an update";
    P_SYS_OFFLINE => "system.offline_mode", System, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Do not join a network";
    P_SYS_LOG_LEVEL => "system.log_level", System, Enum, Value::Enum(2), 0.0, 6.0, 0, false, false, "0 trace, 1 debug, 2 info, 3 warning, 4 error, 5 fatal, 6 silent";
    P_SYS_AUTH_ENABLED => "system.auth.enabled", System, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Require a user name and password on the web API";
    P_SYS_AUTH_USER => "system.auth.username", System, Text, Value::Text("admin"), 0.0, 0.0, 32, false, false, "Web API user name";
    P_SYS_AUTH_PASSWORD => "system.auth.password", System, Text, Value::Text("admin"), 0.0, 0.0, 64, true, false, "Web API password";
    P_SYS_WIFI_SSID => "system.wifi.ssid", System, Text, Value::Text(""), 0.0, 0.0, 32, false, false, "Wireless network name";
    P_SYS_WIFI_PASSWORD => "system.wifi.password", System, Text, Value::Text(""), 0.0, 0.0, 64, true, false, "Wireless network password";
    P_SYS_TIMING_DEBUG => "system.timing_debug.enabled", System, Bool, Value::Bool(false), 0.0, 0.0, 0, false, false, "Log loop timing";
    P_SYS_SHOW_DISPLAY => "system.showdisplay.enabled", System, Bool, Value::Bool(true), 0.0, 0.0, 0, false, false, "Show the display";
}

/// Looks a parameter up by its dotted key.
pub fn find(key: &str) -> Option<&'static Param<'static>> {
    PARAMS.iter().find(|p| p.key == key)
}

/// Looks a parameter up by index into [`PARAMS`].
pub fn by_index(index: usize) -> Option<&'static Param<'static>> {
    PARAMS.get(index)
}

/// How many parameters the schema holds.
pub const fn count() -> usize {
    PARAMS.len()
}

/// The parameters in one group, in declaration order.
pub fn group(group: Group) -> impl Iterator<Item = &'static Param<'static>> {
    PARAMS.iter().filter(move |p| p.group == group)
}

/// Every secret parameter's key. Used to redact a response and to check a log line.
pub fn secret_keys() -> impl Iterator<Item = &'static str> {
    PARAMS.iter().filter(|p| p.secret).map(|p| p.key)
}

/// True when a key names a secret, so a caller redacting by name does not need the table.
pub fn is_secret(key: &str) -> bool {
    find(key).is_some_and(|p| p.secret)
}

/// The C++ export's default hostname, kept because a user who has never set one will recognise
/// it in the UI.
pub const DEFAULT_HOSTNAME: &str = "silvia";

/// Renders a value for a log line, with a secret replaced. Nothing that reaches a log or an
/// export may contain one.
pub fn display_for_log<'a>(key: &str, value: &Value<'a>) -> String<96> {
    let mut out = String::new();
    if is_secret(key) {
        let _ = out.push_str("<redacted>");
        return out;
    }
    match value {
        Value::Bool(b) => {
            let _ = out.push_str(if *b { "true" } else { "false" });
        }
        Value::Int(i) => {
            let _ = out.push_str(&itoa(*i));
        }
        Value::Number(n) => {
            let _ = out.push_str(&fmt_float(*n));
        }
        Value::Enum(e) => {
            let _ = out.push_str(&itoa(*e));
        }
        Value::Text(t) => {
            let _ = out.push_str(t);
        }
    }
    out
}

// Small helpers, so this crate needs no formatting dependency in a `no_std` build.
fn itoa(v: i32) -> heapless::String<16> {
    let mut s = heapless::String::new();
    let _ = core::fmt::Write::write_fmt(&mut s, format_args!("{v}"));
    s
}

fn fmt_float(v: f64) -> heapless::String<32> {
    let mut s = heapless::String::new();
    let _ = core::fmt::Write::write_fmt(&mut s, format_args!("{v:.3}"));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use heapless::Vec;

    #[test]
    fn every_key_is_unique() {
        for (i, a) in PARAMS.iter().enumerate() {
            for b in PARAMS.iter().skip(i + 1) {
                assert_ne!(a.key, b.key, "duplicate key {}", a.key);
            }
        }
    }

    #[test]
    fn every_default_satisfies_its_own_range() {
        for p in PARAMS {
            assert!(
                p.accepts(p.default),
                "{}: default {:?} is outside {} to {}",
                p.key,
                p.default,
                p.min,
                p.max
            );
        }
    }

    #[test]
    fn every_numeric_bound_is_ordered() {
        for p in PARAMS {
            if matches!(p.kind, ValueType::Int | ValueType::Number) {
                assert!(
                    p.min <= p.max,
                    "{}: min {} is above max {}",
                    p.key,
                    p.min,
                    p.max
                );
            }
        }
    }

    #[test]
    fn every_parameter_is_reachable_by_key() {
        for p in PARAMS {
            assert_eq!(
                find(p.key).map(|q| q.key),
                Some(p.key),
                "{} is not findable",
                p.key
            );
        }
        assert!(find("brew.nonexistent").is_none());
    }

    #[test]
    fn the_display_order_is_unique_and_strictly_increasing() {
        let mut previous: Option<u16> = None;
        for p in PARAMS {
            let order = p.order();
            if let Some(prev) = previous {
                assert!(order > prev, "{} order {order} is not after {prev}", p.key);
            }
            previous = Some(order);
        }
    }

    #[test]
    fn the_order_is_the_table_position_not_a_hand_written_number() {
        for (i, p) in PARAMS.iter().enumerate() {
            assert_eq!(p.order(), i as u16, "{} has the wrong position", p.key);
        }
    }

    #[test]
    fn the_two_safety_parameters_are_registered() {
        // They existed in the C++ code, were read by the emergency-stop manager, and were absent
        // from the registry, so a user could not set them (defect D12).
        let p = find("safety.emergency_temp").expect("emergency_temp must be registered");
        assert!(!p.secret);
        assert!(p.accepts(Value::Number(150.0)));
        assert!(
            !p.accepts(Value::Number(50.0)),
            "below the floor must be rejected"
        );
        assert!(find("safety.emergency_hysteresis").is_some());
    }

    #[test]
    fn the_four_secrets_are_marked_and_nothing_else_is() {
        let mut names: Vec<&str, 16> = Vec::new();
        for key in secret_keys() {
            let _ = names.push(key);
        }
        assert_eq!(
            names.len(),
            4,
            "expected exactly four secrets, got {names:?}"
        );
        for expected in [
            "mqtt.password",
            "system.ota_password",
            "system.auth.password",
            "system.wifi.password",
        ] {
            assert!(names.contains(&expected), "{expected} must be a secret");
        }
    }

    #[test]
    fn a_secret_never_renders_into_a_log_line() {
        for key in secret_keys() {
            let v = find(key).unwrap().default;
            let rendered = display_for_log(key, &v);
            assert_eq!(
                rendered.as_str(),
                "<redacted>",
                "{key} leaked into a log line"
            );
        }
    }

    #[test]
    fn a_non_secret_does_render() {
        assert_eq!(
            display_for_log("brew.setpoint", &Value::Number(92.0)).as_str(),
            "92.000"
        );
        assert_eq!(
            display_for_log("pid.enabled", &Value::Bool(true)).as_str(),
            "true"
        );
    }

    #[test]
    fn a_number_outside_its_range_is_rejected() {
        let p = find("brew.setpoint").unwrap();
        assert!(p.accepts(Value::Number(92.0)));
        assert!(!p.accepts(Value::Number(19.9)));
        assert!(!p.accepts(Value::Number(110.1)));
        assert!(
            !p.accepts(Value::Number(150.0)),
            "the /api/setpoint handler allowed 150"
        );
    }

    #[test]
    fn a_not_a_number_is_rejected_by_every_numeric_parameter() {
        for p in PARAMS {
            if matches!(p.kind, ValueType::Int | ValueType::Number) {
                assert!(
                    !p.accepts(Value::Number(f64::NAN)),
                    "{} accepted NaN",
                    p.key
                );
                assert!(
                    !p.accepts(Value::Number(f64::INFINITY)),
                    "{} accepted infinity",
                    p.key
                );
            }
        }
    }

    #[test]
    fn a_text_value_over_its_length_is_rejected() {
        // The C++ `isValid` returned true for every string and the length constants were
        // referenced nowhere, so an arbitrarily long hostname could be stored (defect D23).
        let p = find("system.hostname").unwrap();
        assert!(p.accepts(Value::Text("kitchen")));
        assert!(!p.accepts(Value::Text(
            "0123456789012345678901234567890123456789012345678901234567890123456789"
        )));
    }

    #[test]
    fn a_type_mismatch_is_never_accepted() {
        let p = find("brew.setpoint").unwrap();
        assert!(
            !p.accepts(Value::Int(92)),
            "an int is not a number field's value"
        );
        assert!(!p.accepts(Value::Bool(true)));
        assert!(!p.accepts(Value::Text("92")));
    }

    #[test]
    fn the_scale_calibration_divisor_cannot_be_zero_or_negative() {
        // The C++ range was -999999 to 999999 and the value is a divisor, so a negative or zero
        // calibration inverted or destroyed every weight reading.
        let p = find("hardware.sensors.scale.calibration").unwrap();
        assert!(p.accepts(Value::Number(1000.0)));
        assert!(
            p.accepts(Value::Number(-1750.05)),
            "a negative calibration is a valid device polarity"
        );
        assert!(
            !p.accepts(Value::Number(0.0)),
            "a zero divisor is not usable"
        );
    }

    #[test]
    fn group_filters_return_their_own_parameters() {
        let brew: Vec<&str, 32> = group(Group::Brew).map(|p| p.key).collect();
        assert!(brew.contains(&"brew.setpoint"));
        assert!(!brew.contains(&"pid.enabled"));
        // The ten groups the C++ export emitted, all present.
        for g in Group::ALL {
            assert!(group(g).next().is_some(), "{} has no parameters", g.name());
        }
    }

    #[test]
    fn the_shipped_config_values_are_all_acceptable() {
        // A config the machine actually ran with must import without a single rejection. The
        // values below are transcribed from the repository's config.json, which is a real export
        // with the credentials masked. A schema that rejected one of these would strand a user
        // mid-migration, which is the one thing the import must never do.
        let real: &[(&str, Value)] = &[
            ("pid.regular.kp", Value::Number(50.0)),
            ("pid.regular.tn", Value::Number(200.0)),
            ("pid.regular.tv", Value::Number(20.0)),
            ("pid.regular.i_max", Value::Number(75.0)),
            ("brew.setpoint", Value::Number(92.0)),
            ("brew.mode", Value::Enum(1)),
            ("brew.by_time.target_time", Value::Number(27.0)),
            ("brew.pre_infusion.enabled", Value::Bool(true)),
            ("standby.enabled", Value::Bool(true)),
            ("standby.time", Value::Number(15.0)),
            ("mqtt.broker", Value::Text("xxx")),
            ("mqtt.port", Value::Int(1883)),
            ("mqtt.hassio.enabled", Value::Bool(true)),
            (
                "hardware.sensors.scale.calibration",
                Value::Number(-1750.05),
            ),
            (
                "hardware.sensors.scale.calibration2",
                Value::Number(-1685.21),
            ),
            ("hardware.sensors.scale.known_weight", Value::Number(456.0)),
            ("hardware.sensors.scale.samples", Value::Int(2)),
            ("hardware.sensors.temperature.type", Value::Enum(0)),
            ("system.hostname", Value::Text("silvia")),
            ("system.log_level", Value::Enum(2)),
            ("system.auth.enabled", Value::Bool(false)),
        ];
        for (key, value) in real {
            let p = find(key)
                .unwrap_or_else(|| panic!("{key} is in config.json but not in the schema"));
            assert!(
                p.accepts(*value),
                "{key} = {value:?} is rejected by the schema"
            );
        }
    }

    #[test]
    fn a_key_in_the_example_config_that_never_existed_is_not_in_the_schema() {
        // docs/example_config.json carries display.blescale_brew_timer, which is not a parameter
        // anywhere in the C++ code. The old importer ignored it silently; this importer rejects it
        // and names it, which is the difference T-19 has to demonstrate.
        assert!(find("display.blescale_brew_timer").is_none());
    }

    #[test]
    fn the_table_holds_the_expected_number_of_parameters() {
        // The C++ firmware registered 96. This table is larger because it adds the two safety
        // parameters the C++ read but never registered, plus hardware.board.
        assert!(
            count() >= 96,
            "expected at least the C++ 96 parameters, found {}",
            count()
        );
    }
}
