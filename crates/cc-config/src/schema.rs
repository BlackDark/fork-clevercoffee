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
//! [`docs/history/feature-inventory.md` §10](../../docs/history/feature-inventory.md)
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

use alloc::borrow::ToOwned;

use cc_domain::hardware::{
    OledAddress, OledType, RelayTriggerType, ScaleType, SwitchMode, SwitchType,
    TemperatureSensorType,
};
use cc_domain::process::BrewMode;
use cc_domain::system::{DisplayTemplate, Language, LogLevel};

use crate::config::Config;
use crate::json::LiveValue;
use crate::IntEnum;

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

/// One registered parameter: what it is, and how to read and write it.
///
/// **This is the whole parameter table.** Before, the parameter set was
/// described three times over — here, as a `set` dispatch in
/// [`crate::assign`], and as a `match` in [`crate::json::live_value`] — and
/// adding a parameter meant three edits in three files, with a missed one
/// showing up only as a parameter that validates but does not read, or reads
/// but does not write. The C++ has one table, not three:
/// `Config::getAllConfigParams` (`git show main:src/Config.cpp:438-563`)
/// returns 96 `ConfigParamDef*`, and each carries its key, default, min, max
/// **and** the `toJson`/`fromString` virtual pair. `get` and `set` are those
/// two virtuals, as function pointers.
///
/// So a key that exists in [`SCHEMA`] and is not readable is no longer
/// expressible: the accessor and the declaration are one entry, written once.
/// `every_schema_key_has_a_live_value` and
/// `every_schema_default_matches_the_config_default` still exist and still
/// run, but they are now nearly tautological — that is the point of them, not
/// a reason to delete them.
///
/// **No `PartialEq`.** The derive was on this struct while the accessors were
/// a separate table, and it compared the data fields, which was meaningful.
/// Carrying two `fn` pointers makes the derived equality compare function
/// addresses, which is not a stable property of a program (`rustc`'s
/// `unpredictable_function_pointer_comparisons` warns about exactly this), and
/// nothing in the crate compared two specs anyway — the properties that matter
/// are [`ParamSpec::key`] and [`ParamSpec::default`], and those are read
/// directly.
#[derive(Clone, Copy, Debug)]
pub struct ParamSpec {
    /// The dotted C++ key, e.g. `"pid.regular.kp"`.
    pub key: &'static str,
    /// The operator-facing help text, transcribed verbatim from the C++'s
    /// `helpText_` constructor argument (`Config.h:69`, `:536`, `:661`).
    ///
    /// This is the whole of `GET /api/parameter-help`: the C++ answers with
    /// `paramDef->getHelpText()` (`WebServerManager.cpp:598-599`) and nothing
    /// else, so this field *is* that route's body. It was missing here until
    /// finding 3.4 of
    /// [`32-findings-2026-10-03.md`](../../../docs/history/review-2026-10-03.md)
    /// — `web.rs` answered that route with an error object and HTTP 200 rather
    /// than admit it had no data. The bytes are the C++'s, so a client reading
    /// the Rust firmware gets the same string it got from the C++ one.
    pub help: &'static str,
    /// The value's type.
    pub kind: ParamKind,
    /// The compiled-in default.
    pub default: ParamValue<'static>,
    /// Inclusive lower bound, for the kinds that have one.
    pub min: Option<f64>,
    /// Inclusive upper bound, for the kinds that have one.
    pub max: Option<f64>,
    /// Read this key's current value out of a [`Config`].
    ///
    /// The read half of the C++'s `toJson` (`Config.h:215-238`), which writes
    /// `obj["value"] = currentValue_` — the *stored* value, not the default.
    ///
    /// Higher-ranked in the config's lifetime rather than a bare
    /// `fn(&Config)`: the four credential parameters hand back a `&str`
    /// borrowed from a `Secret<String>` inside the config, and a getter that
    /// could not tie that borrow to its argument would have to copy the
    /// plaintext onto the heap. It is expressible as a field because a
    /// `for<'a> fn(&'a Config) -> LiveValue<'a>` is a higher-ranked function
    /// pointer, which is const-constructible like any other.
    pub get: for<'a> fn(&'a Config) -> LiveValue<'a>,
    /// Write a value into this parameter's field. `false` = not applicable.
    ///
    /// The write half of the C++'s `fromString` (`Config.h:242-262`) without
    /// the parsing and without the NVS write, which moved to
    /// [`crate::BlobConfigStore::save`]. `false` covers a value of the wrong
    /// [`ParamKind`] and — for an enumeration — a discriminant that names no
    /// variant, which is the analogue of `EnumParamDef::isValid`
    /// (`Config.h:379-388`). The bounds live in [`ParamSpec::accepts`] and are
    /// checked before a value gets this far, by [`crate::assign::parse`].
    pub set: fn(&mut Config, &LiveValue<'_>) -> bool,
}

impl ParamSpec {
    /// Build a spec. Used by the [`SCHEMA`] table.
    ///
    /// `accessors` is the `(get, set)` pair the private `accessors!` macro
    /// produces, passed as
    /// a tuple so that one macro invocation describes both halves of a
    /// parameter and the compiler can check that they agree on the type.
    #[must_use]
    pub const fn new(
        key: &'static str,
        help: &'static str,
        kind: ParamKind,
        default: ParamValue<'static>,
        min: Option<f64>,
        max: Option<f64>,
        accessors: Accessors,
    ) -> Self {
        Self {
            key,
            help,
            kind,
            default,
            min,
            max,
            get: accessors.get,
            set: accessors.set,
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

/// The read and write halves of one parameter, as handed to
/// [`ParamSpec::new`] by the private `accessors!` macro.
///
/// `Copy` because it is two function pointers and nothing else; it is built in
/// a `const` initialiser and immediately destructured into
/// [`ParamSpec::get`] and [`ParamSpec::set`].
#[derive(Clone, Copy, Debug)]
pub struct Accessors {
    /// The getter. See [`ParamSpec::get`].
    pub get: for<'a> fn(&'a Config) -> LiveValue<'a>,
    /// The setter. See [`ParamSpec::set`].
    pub set: fn(&mut Config, &LiveValue<'_>) -> bool,
}

/// Does the dotted `key` name the same `Config` field as the `path`?
///
/// `stringify!` renders a raw identifier with its `r#` — the key
/// [`SCHEMA`] registers is `hardware.oled.type` and
/// `stringify!(hardware.oled.r#type)` is `hardware.oled.r#type`. Comparing
/// them needs to step over the `r#`, which is what this does; the alternative,
/// writing the key a second time as a string literal to match against, makes
/// the table two facts per entry, and a table of two facts per entry is a
/// table that can disagree with itself.
///
/// `const`, so the `accessors!` macro can assert the pairing at compile
/// time. This is
/// what replaced `assign::eq_key`, which made the same comparison at runtime
/// on every write of every parameter.
const fn key_is_path(key: &str, path: &str) -> bool {
    let (k, p) = (key.as_bytes(), path.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < k.len() && j < p.len() {
        // `r#` at the head of a segment is Rust's escape, not part of the key.
        if p[j] == b'r' && j + 1 < p.len() && p[j + 1] == b'#' && (j == 0 || p[j - 1] == b'.') {
            j += 2;
            continue;
        }
        if k[i] != p[j] {
            return false;
        }
        i += 1;
        j += 1;
    }
    i == k.len() && j == p.len()
}

/// The accessor pair for one parameter: a getter and a setter over one
/// `Config` field path.
///
/// Private: it exists to be written once per [`SCHEMA`] entry and consumed by
/// [`ParamSpec::new`], which is the only way to build a spec.
///
/// Six arms, one per way a `Config` field differs from a plain scalar, and
/// each is the only place that difference is written down:
///
/// * `bool` / `int` / `float` — a `Copy` field. Read and write directly.
/// * `text` — an `alloc::string::String`. Read borrows (`as_str`), write owns.
/// * `secret` — a [`crate::Secret`]<`String`>. Read borrows the plaintext out
///   of `expose()`; write goes through `set`, which is the only way to get at
///   the inner `String`. **This arm is why `get` is higher-ranked**: the
///   returned `&str` points into the `Secret` inside the `Config`.
/// * `enum` — a `cc-domain` enum. Read is `to_raw`, write is `from_raw`, and
///   the `from_raw` failure is what makes `set` return `false` for a
///   discriminant no variant has.
///
/// The `key` argument is only used by the `const` assertion below; it is not
/// stored, because [`ParamSpec::new`] already has it and a table of two facts
/// per entry is a table that can disagree with itself.
/// Assert that `$key` names `$field`, then yield `$value`.
///
/// The compile-time half of `accessors!`. A key that disagrees with the field
/// it is registered against is the failure mode the three separate tables made
/// invisible — it would type-check, and the parameter would read and write the
/// wrong `Config` field — so it is worth five lines here rather than 98
/// runtime comparisons per parameter write (`assign::eq_key`, which this
/// replaced).
macro_rules! checked {
    ($key:literal, $($field:tt).+, $value:expr) => {{
        const _: () = assert!(
            key_is_path($key, stringify!($($field).+)),
            "the key does not name the field it is registered against"
        );
        $value
    }};
}

/// The accessor pair for one parameter: a getter and a setter over one
/// `Config` field path.
///
/// Private: it exists to be written once per [`SCHEMA`] entry and consumed by
/// [`ParamSpec::new`], which is the only way to build a spec.
///
/// Six arms, one per way a `Config` field differs from a plain scalar, and
/// each is the only place that difference is written down:
///
/// * `bool` / `int` / `float` — a `Copy` field. Read and write directly.
/// * `text` — an `alloc::string::String`. Read borrows (`as_str`), write owns.
/// * `secret` — a [`crate::Secret`]<`String`>. Read borrows the plaintext out
///   of `expose()`; write goes through `set`, which is the only way to get at
///   the inner `String`. **This arm is why `get` is higher-ranked**: the
///   returned `&str` points into the `Secret` inside the `Config`.
/// * `enum` — a `cc-domain` enum. Read is `to_raw`, write is `from_raw`, and
///   the `from_raw` failure is what makes `set` return `false` for a
///   discriminant no variant has.
///
/// The `key` argument is only used by the `const` assertion; it is not stored,
/// because [`ParamSpec::new`] already has it and a table of two facts per entry
/// is a table that can disagree with itself.
macro_rules! accessors {
    (bool, $key:literal, $($field:tt).+) => {
        checked!(
            $key,
            $($field).+,
            Accessors {
                get: |config: &Config| LiveValue::Bool(config.$($field).+),
                set: |config: &mut Config, value: &LiveValue<'_>| match value {
                    LiveValue::Bool(v) => {
                        config.$($field).+ = *v;
                        true
                    }
                    _ => false,
                },
            }
        )
    };
    (int, $key:literal, $($field:tt).+) => {
        checked!(
            $key,
            $($field).+,
            Accessors {
                get: |config: &Config| LiveValue::Int(config.$($field).+),
                set: |config: &mut Config, value: &LiveValue<'_>| match value {
                    LiveValue::Int(v) => {
                        config.$($field).+ = *v;
                        true
                    }
                    _ => false,
                },
            }
        )
    };
    (float, $key:literal, $($field:tt).+) => {
        checked!(
            $key,
            $($field).+,
            Accessors {
                get: |config: &Config| LiveValue::Float(config.$($field).+),
                set: |config: &mut Config, value: &LiveValue<'_>| match value {
                    LiveValue::Float(v) => {
                        config.$($field).+ = *v;
                        true
                    }
                    _ => false,
                },
            }
        )
    };
    (text, $key:literal, $($field:tt).+) => {
        checked!(
            $key,
            $($field).+,
            Accessors {
                get: |config: &Config| LiveValue::Text(config.$($field).+.as_str()),
                set: |config: &mut Config, value: &LiveValue<'_>| match value {
                    LiveValue::Text(v) => {
                        config.$($field).+ = (*v).to_owned();
                        true
                    }
                    _ => false,
                },
            }
        )
    };
    (secret, $key:literal, $($field:tt).+) => {
        checked!(
            $key,
            $($field).+,
            Accessors {
                get: |config: &Config| LiveValue::Text(config.$($field).+.expose()),
                set: |config: &mut Config, value: &LiveValue<'_>| match value {
                    LiveValue::Text(v) => {
                        config.$($field).+.set((*v).to_owned());
                        true
                    }
                    _ => false,
                },
            }
        )
    };
    (enum, $key:literal, $enum:ty, $($field:tt).+) => {
        checked!(
            $key,
            $($field).+,
            Accessors {
                get: |config: &Config| LiveValue::Enum(config.$($field).+.to_raw()),
                set: |config: &mut Config, value: &LiveValue<'_>| match value {
                    LiveValue::Enum(v) => match <$enum>::from_raw(*v) {
                        Some(v) => {
                            config.$($field).+ = v;
                            true
                        }
                        None => false,
                    },
                    _ => false,
                },
            }
        )
    };
}
/// Every parameter the firmware persists, in the C++ registration order.
///
/// The order is the C++ order (`getAllConfigParams`) with the two `safety.*`
/// parameters appended, so `/api/parameters` output keeps the same ordering as
/// the C++ build for the first 96 entries.
pub const SCHEMA: &[ParamSpec] = &[
    ParamSpec::new(
        "pid.enabled",
        "Enables or disables the PID temperature controller",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "pid.enabled", pid.enabled),
    ),
    ParamSpec::new(
        "pid.use_ponm",
        "Use PonM mode (Proportional on Measurement)",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "pid.use_ponm", pid.use_ponm),
    ),
    ParamSpec::new(
        "pid.ema_factor",
        "Smoothing of input for derivative component. Smaller = less smoothing but less delay",
        ParamKind::Float,
        ParamValue::Float(0.6),
        Some(0.0),
        Some(1.0),
        accessors!(float, "pid.ema_factor", pid.ema_factor),
    ),
    ParamSpec::new(
        "pid.regular.kp",
        "Proportional gain (in Watts/°C) for the main PID controller",
        ParamKind::Float,
        ParamValue::Float(62.0),
        Some(0.0),
        Some(200.0),
        accessors!(float, "pid.regular.kp", pid.regular.kp),
    ),
    ParamSpec::new(
        "pid.regular.tn",
        "Integral time constant (in seconds) for the main PID controller",
        ParamKind::Float,
        ParamValue::Float(52.0),
        Some(0.0),
        Some(200.0),
        accessors!(float, "pid.regular.tn", pid.regular.tn),
    ),
    ParamSpec::new(
        "pid.regular.tv",
        "Differential time constant (in seconds) for the main PID controller",
        ParamKind::Float,
        ParamValue::Float(11.5),
        Some(0.0),
        Some(200.0),
        accessors!(float, "pid.regular.tv", pid.regular.tv),
    ),
    ParamSpec::new(
        "pid.regular.i_max",
        "Internal integrator limit to prevent windup (in Watts)",
        ParamKind::Float,
        ParamValue::Float(55.0),
        Some(0.0),
        Some(100.0),
        accessors!(float, "pid.regular.i_max", pid.regular.i_max),
    ),
    ParamSpec::new(
        "pid.steam.kp",
        "Proportional gain for the steaming mode",
        ParamKind::Float,
        ParamValue::Float(150.0),
        Some(0.0),
        Some(500.0),
        accessors!(float, "pid.steam.kp", pid.steam.kp),
    ),
    ParamSpec::new(
        "brew.setpoint",
        "The temperature that the PID will attempt to reach and hold",
        ParamKind::Float,
        ParamValue::Float(95.0),
        Some(20.0),
        Some(110.0),
        accessors!(float, "brew.setpoint", brew.setpoint),
    ),
    ParamSpec::new(
        "brew.temp_offset",
        "Optional offset added to the user-visible setpoint to compensate sensor offsets",
        ParamKind::Float,
        ParamValue::Float(0.0),
        Some(0.0),
        Some(20.0),
        accessors!(float, "brew.temp_offset", brew.temp_offset),
    ),
    ParamSpec::new(
        "steam.setpoint",
        "The temperature that the PID will use for steam mode",
        ParamKind::Float,
        ParamValue::Float(120.0),
        Some(100.0),
        Some(140.0),
        accessors!(float, "steam.setpoint", steam.setpoint),
    ),
    ParamSpec::new(
        "pid.bd.enabled",
        "Use separate PID parameters while brew is running",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "pid.bd.enabled", pid.bd.enabled),
    ),
    ParamSpec::new(
        "brew.pid_delay",
        "Delay time during which PID will be disabled once brew is detected",
        ParamKind::Float,
        ParamValue::Float(10.0),
        Some(0.0),
        Some(60.0),
        accessors!(float, "brew.pid_delay", brew.pid_delay),
    ),
    ParamSpec::new(
        "pid.bd.kp",
        "Proportional gain for PID when brewing has been detected",
        ParamKind::Float,
        ParamValue::Float(50.0),
        Some(0.0),
        Some(200.0),
        accessors!(float, "pid.bd.kp", pid.bd.kp),
    ),
    ParamSpec::new(
        "pid.bd.tn",
        "Integral time constant for PID when brewing has been detected",
        ParamKind::Float,
        ParamValue::Float(0.0),
        Some(0.0),
        Some(200.0),
        accessors!(float, "pid.bd.tn", pid.bd.tn),
    ),
    ParamSpec::new(
        "pid.bd.tv",
        "Differential time constant for PID when brewing has been detected",
        ParamKind::Float,
        ParamValue::Float(20.0),
        Some(0.0),
        Some(200.0),
        accessors!(float, "pid.bd.tv", pid.bd.tv),
    ),
    ParamSpec::new(
        "brew.mode",
        "Brewing mode selection",
        ParamKind::Enum,
        ParamValue::Enum(BrewMode::Manual as i8),
        None,
        None,
        accessors!(enum, "brew.mode", BrewMode, brew.mode),
    ),
    ParamSpec::new(
        "brew.by_time.enabled",
        "Enable brewing by time control",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "brew.by_time.enabled", brew.by_time.enabled),
    ),
    ParamSpec::new(
        "brew.by_time.target_time",
        "Target brew time in seconds",
        ParamKind::Float,
        ParamValue::Float(25.0),
        Some(1.0),
        Some(120.0),
        accessors!(float, "brew.by_time.target_time", brew.by_time.target_time),
    ),
    ParamSpec::new(
        "brew.by_weight.enabled",
        "Enable brewing by weight control",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "brew.by_weight.enabled", brew.by_weight.enabled),
    ),
    ParamSpec::new(
        "brew.by_weight.target_weight",
        "Brew is running until this weight has been measured",
        ParamKind::Float,
        ParamValue::Float(36.0),
        Some(0.0),
        Some(500.0),
        accessors!(
            float,
            "brew.by_weight.target_weight",
            brew.by_weight.target_weight
        ),
    ),
    ParamSpec::new(
        "brew.by_weight.auto_tare",
        "Automatically tare scale before brewing",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "brew.by_weight.auto_tare", brew.by_weight.auto_tare),
    ),
    ParamSpec::new(
        "brew.pre_infusion.enabled",
        "Enable pre-infusion phase",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "brew.pre_infusion.enabled", brew.pre_infusion.enabled),
    ),
    ParamSpec::new(
        "brew.pre_infusion.time",
        "Pre-infusion time in seconds",
        ParamKind::Float,
        ParamValue::Float(2.0),
        Some(0.0),
        Some(60.0),
        accessors!(float, "brew.pre_infusion.time", brew.pre_infusion.time),
    ),
    ParamSpec::new(
        "brew.pre_infusion.pause",
        "Pre-infusion pause time in seconds",
        ParamKind::Float,
        ParamValue::Float(5.0),
        Some(0.0),
        Some(60.0),
        accessors!(float, "brew.pre_infusion.pause", brew.pre_infusion.pause),
    ),
    ParamSpec::new(
        "display.fullscreen_brew_timer",
        "Enable fullscreen overlay during brew",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "display.fullscreen_brew_timer",
            display.fullscreen_brew_timer
        ),
    ),
    ParamSpec::new(
        "display.fullscreen_manual_flush_timer",
        "Enable fullscreen overlay during manual flush",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "display.fullscreen_manual_flush_timer",
            display.fullscreen_manual_flush_timer
        ),
    ),
    ParamSpec::new(
        "display.fullscreen_hot_water_timer",
        "Enable fullscreen overlay during hot water mode",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "display.fullscreen_hot_water_timer",
            display.fullscreen_hot_water_timer
        ),
    ),
    ParamSpec::new(
        "display.post_brew_timer_duration",
        "Post brew timer will be shown for this many seconds after brew finished",
        ParamKind::Float,
        ParamValue::Float(3.0),
        Some(0.0),
        Some(60.0),
        accessors!(
            float,
            "display.post_brew_timer_duration",
            display.post_brew_timer_duration
        ),
    ),
    ParamSpec::new(
        "display.heating_logo",
        "Full screen logo will be shown if temperature is 5°C below setpoint",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
        accessors!(bool, "display.heating_logo", display.heating_logo),
    ),
    ParamSpec::new(
        "display.pid_off_logo",
        "Full screen logo will be shown if PID is disabled",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
        accessors!(bool, "display.pid_off_logo", display.pid_off_logo),
    ),
    ParamSpec::new(
        "hardware.leds.status.enabled",
        "Enable status indicator LED",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "hardware.leds.status.enabled",
            hardware.leds.status.enabled
        ),
    ),
    ParamSpec::new(
        "hardware.leds.status.inverted",
        "Invert the status LED logic (for common anode LEDs)",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "hardware.leds.status.inverted",
            hardware.leds.status.inverted
        ),
    ),
    ParamSpec::new(
        "hardware.leds.brew.enabled",
        "Enable brew indicator LED",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "hardware.leds.brew.enabled",
            hardware.leds.brew.enabled
        ),
    ),
    ParamSpec::new(
        "hardware.leds.brew.inverted",
        "Invert the brew LED logic",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "hardware.leds.brew.inverted",
            hardware.leds.brew.inverted
        ),
    ),
    ParamSpec::new(
        "hardware.leds.steam.enabled",
        "Enable steam indicator LED",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "hardware.leds.steam.enabled",
            hardware.leds.steam.enabled
        ),
    ),
    ParamSpec::new(
        "hardware.leds.steam.inverted",
        "Invert the steam LED logic",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "hardware.leds.steam.inverted",
            hardware.leds.steam.inverted
        ),
    ),
    ParamSpec::new(
        "display.template",
        "Set the display template, changes require a reboot",
        ParamKind::Enum,
        ParamValue::Enum(DisplayTemplate::Standard as i8),
        None,
        None,
        accessors!(enum, "display.template", DisplayTemplate, display.template),
    ),
    ParamSpec::new(
        "display.inverted",
        "Set the display rotation, changes require a reboot",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "display.inverted", display.inverted),
    ),
    ParamSpec::new(
        "display.language",
        "Set the language for the OLED display",
        ParamKind::Enum,
        ParamValue::Enum(Language::English as i8),
        None,
        None,
        accessors!(enum, "display.language", Language, display.language),
    ),
    ParamSpec::new(
        "display.blinking.delta",
        "Delta from setpoint for status LED and blinking temperature display",
        ParamKind::Float,
        ParamValue::Float(0.3),
        Some(0.2),
        Some(10.0),
        accessors!(float, "display.blinking.delta", display.blinking.delta),
    ),
    ParamSpec::new(
        "backflush.cycles",
        "Number of backflush cycles to perform",
        ParamKind::Int,
        ParamValue::Int(5),
        Some(2.0),
        Some(20.0),
        accessors!(int, "backflush.cycles", backflush.cycles),
    ),
    ParamSpec::new(
        "backflush.fill_time",
        "Time to fill during backflush cycle",
        ParamKind::Float,
        ParamValue::Float(5.0),
        Some(3.0),
        Some(10.0),
        accessors!(float, "backflush.fill_time", backflush.fill_time),
    ),
    ParamSpec::new(
        "backflush.flush_time",
        "Time to flush during backflush cycle",
        ParamKind::Float,
        ParamValue::Float(10.0),
        Some(5.0),
        Some(20.0),
        accessors!(float, "backflush.flush_time", backflush.flush_time),
    ),
    ParamSpec::new(
        "maintenance.backflush_reminder.enabled",
        "Show a reminder when the shot count since last backflush reaches the threshold",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
        accessors!(
            bool,
            "maintenance.backflush_reminder.enabled",
            maintenance.backflush_reminder.enabled
        ),
    ),
    ParamSpec::new(
        "maintenance.backflush_reminder.threshold",
        "Number of counted brews before a backflush reminder is shown (default ~monthly at 2 shots/day)",
        ParamKind::Int,
        ParamValue::Int(50),
        Some(1.0),
        Some(500.0),
        accessors!(
            int,
            "maintenance.backflush_reminder.threshold",
            maintenance.backflush_reminder.threshold
        ),
    ),
    ParamSpec::new(
        "standby.enabled",
        "Turn heater off after standby time has elapsed",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "standby.enabled", standby.enabled),
    ),
    ParamSpec::new(
        "standby.time",
        "Time in minutes until the heater is turned off",
        ParamKind::Float,
        ParamValue::Float(35.0),
        Some(1.0),
        Some(120.0),
        accessors!(float, "standby.time", standby.time),
    ),
    ParamSpec::new(
        "mqtt.enabled",
        "Enables MQTT, change requires a restart",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "mqtt.enabled", mqtt.enabled),
    ),
    ParamSpec::new(
        "mqtt.broker",
        "IP address or hostname of your MQTT broker",
        ParamKind::Text,
        ParamValue::Text(""),
        None,
        None,
        accessors!(text, "mqtt.broker", mqtt.broker),
    ),
    ParamSpec::new(
        "mqtt.port",
        "Port number of your MQTT broker",
        ParamKind::Int,
        ParamValue::Int(1883),
        Some(1.0),
        Some(65535.0),
        accessors!(int, "mqtt.port", mqtt.port),
    ),
    ParamSpec::new(
        "mqtt.username",
        "Username for your MQTT broker",
        ParamKind::Text,
        ParamValue::Text("rancilio"),
        None,
        None,
        accessors!(text, "mqtt.username", mqtt.username),
    ),
    ParamSpec::new(
        "mqtt.password",
        "Password for your MQTT broker",
        ParamKind::Text,
        ParamValue::Text("silvia"),
        None,
        None,
        accessors!(secret, "mqtt.password", mqtt.password),
    ),
    ParamSpec::new(
        "mqtt.topic",
        "Custom MQTT topic prefix",
        ParamKind::Text,
        ParamValue::Text("custom/kitchen/"),
        None,
        None,
        accessors!(text, "mqtt.topic", mqtt.topic),
    ),
    ParamSpec::new(
        "mqtt.hassio.enabled",
        "Enables Home Assistant integration",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "mqtt.hassio.enabled", mqtt.hassio.enabled),
    ),
    ParamSpec::new(
        "mqtt.hassio.prefix",
        "Custom MQTT topic prefix for Home Assistant",
        ParamKind::Text,
        ParamValue::Text("homeassistant"),
        None,
        None,
        accessors!(text, "mqtt.hassio.prefix", mqtt.hassio.prefix),
    ),
    ParamSpec::new(
        "system.hostname",
        "Hostname of your machine, changes require a restart",
        ParamKind::Text,
        ParamValue::Text(DEFAULT_HOSTNAME),
        None,
        None,
        accessors!(text, "system.hostname", system.hostname),
    ),
    ParamSpec::new(
        "system.ota_password",
        "Password for over-the-air updates, changes require a restart",
        ParamKind::Text,
        ParamValue::Text("otapass"),
        None,
        None,
        accessors!(secret, "system.ota_password", system.ota_password),
    ),
    ParamSpec::new(
        "system.offline_mode",
        "Run in offline mode without WiFi connection",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "system.offline_mode", system.offline_mode),
    ),
    ParamSpec::new(
        "system.log_level",
        "Set the logging level for debug output",
        ParamKind::Enum,
        ParamValue::Enum(LogLevel::Info as i8),
        None,
        None,
        accessors!(enum, "system.log_level", LogLevel, system.log_level),
    ),
    ParamSpec::new(
        "system.auth.enabled",
        "Enables authentication for accessing certain parts of the website",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(bool, "system.auth.enabled", system.auth.enabled),
    ),
    ParamSpec::new(
        "system.auth.username",
        "Username for accessing the website and authenticating web requests",
        ParamKind::Text,
        ParamValue::Text("admin"),
        None,
        None,
        accessors!(text, "system.auth.username", system.auth.username),
    ),
    ParamSpec::new(
        "system.auth.password",
        "Password for accessing the website and authenticating web requests",
        ParamKind::Text,
        ParamValue::Text("admin"),
        None,
        None,
        accessors!(secret, "system.auth.password", system.auth.password),
    ),
    ParamSpec::new(
        "system.timing_debug.enabled",
        "Enable or disable the process loop time debugging in console",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "system.timing_debug.enabled",
            system.timing_debug.enabled
        ),
    ),
    ParamSpec::new(
        "system.showdisplay.enabled",
        "Enable or disable showing sendBuffer loops in debug logs",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
        accessors!(
            bool,
            "system.showdisplay.enabled",
            system.showdisplay.enabled
        ),
    ),
    ParamSpec::new(
        "system.wifi.ssid",
        "WiFi SSID to connect to directly (leave empty to use the configuration portal)",
        ParamKind::Text,
        ParamValue::Text(""),
        None,
        None,
        accessors!(text, "system.wifi.ssid", system.wifi.ssid),
    ),
    ParamSpec::new(
        "system.wifi.password",
        "WiFi password for direct connection (leave empty for open networks)",
        ParamKind::Text,
        ParamValue::Text(""),
        None,
        None,
        accessors!(secret, "system.wifi.password", system.wifi.password),
    ),
    ParamSpec::new(
        "hardware.oled.enabled",
        "Enable or disable the OLED display",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
        accessors!(bool, "hardware.oled.enabled", hardware.oled.enabled),
    ),
    ParamSpec::new(
        "hardware.oled.type",
        "Select your OLED display type",
        ParamKind::Enum,
        ParamValue::Enum(OledType::Ssd1306 as i8),
        None,
        None,
        accessors!(enum, "hardware.oled.type", OledType, hardware.oled.r#type),
    ),
    ParamSpec::new(
        "hardware.oled.address",
        "I2C address of the OLED display",
        ParamKind::Enum,
        ParamValue::Enum(OledAddress::Addr3c as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.oled.address",
            OledAddress,
            hardware.oled.address
        ),
    ),
    ParamSpec::new(
        "hardware.relays.heater.trigger_type",
        "Relay trigger type for heater control",
        ParamKind::Enum,
        ParamValue::Enum(RelayTriggerType::HighTrigger as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.relays.heater.trigger_type",
            RelayTriggerType,
            hardware.relays.heater.trigger_type
        ),
    ),
    ParamSpec::new(
        "hardware.relays.valve.trigger_type",
        "Relay trigger type for valve control",
        ParamKind::Enum,
        ParamValue::Enum(RelayTriggerType::HighTrigger as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.relays.valve.trigger_type",
            RelayTriggerType,
            hardware.relays.valve.trigger_type
        ),
    ),
    ParamSpec::new(
        "hardware.relays.pump.trigger_type",
        "Relay trigger type for pump control",
        ParamKind::Enum,
        ParamValue::Enum(RelayTriggerType::HighTrigger as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.relays.pump.trigger_type",
            RelayTriggerType,
            hardware.relays.pump.trigger_type
        ),
    ),
    ParamSpec::new(
        "hardware.switches.brew.enabled",
        "Enable physical brew switch",
        ParamKind::Bool,
        // `true`, not the C++'s `false` (`Config.h:985`) — see
        // `HardwareSwitchesBrew::enabled` and intentional-diffs.md.
        ParamValue::Bool(true),
        None,
        None,
        accessors!(
            bool,
            "hardware.switches.brew.enabled",
            hardware.switches.brew.enabled
        ),
    ),
    ParamSpec::new(
        "hardware.switches.brew.type",
        "Type of brew switch connected",
        ParamKind::Enum,
        ParamValue::Enum(SwitchType::Toggle as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.switches.brew.type",
            SwitchType,
            hardware.switches.brew.r#type
        ),
    ),
    ParamSpec::new(
        "hardware.switches.brew.mode",
        "Electrical configuration of brew switch",
        ParamKind::Enum,
        ParamValue::Enum(SwitchMode::NormallyOpen as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.switches.brew.mode",
            SwitchMode,
            hardware.switches.brew.mode
        ),
    ),
    ParamSpec::new(
        "hardware.switches.steam.enabled",
        "Enable physical steam switch",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
        accessors!(
            bool,
            "hardware.switches.steam.enabled",
            hardware.switches.steam.enabled
        ),
    ),
    ParamSpec::new(
        "hardware.switches.steam.type",
        "Type of steam switch connected",
        ParamKind::Enum,
        ParamValue::Enum(SwitchType::Toggle as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.switches.steam.type",
            SwitchType,
            hardware.switches.steam.r#type
        ),
    ),
    ParamSpec::new(
        "hardware.switches.steam.mode",
        "Electrical configuration of steam switch",
        ParamKind::Enum,
        ParamValue::Enum(SwitchMode::NormallyOpen as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.switches.steam.mode",
            SwitchMode,
            hardware.switches.steam.mode
        ),
    ),
    ParamSpec::new(
        "hardware.switches.power.enabled",
        "Enable physical power switch",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
        accessors!(
            bool,
            "hardware.switches.power.enabled",
            hardware.switches.power.enabled
        ),
    ),
    ParamSpec::new(
        "hardware.switches.power.type",
        "Type of power switch connected",
        ParamKind::Enum,
        ParamValue::Enum(SwitchType::Toggle as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.switches.power.type",
            SwitchType,
            hardware.switches.power.r#type
        ),
    ),
    ParamSpec::new(
        "hardware.switches.power.mode",
        "Electrical configuration of power switch",
        ParamKind::Enum,
        ParamValue::Enum(SwitchMode::NormallyOpen as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.switches.power.mode",
            SwitchMode,
            hardware.switches.power.mode
        ),
    ),
    ParamSpec::new(
        "hardware.switches.hot_water.enabled",
        "Enable physical water switch",
        ParamKind::Bool,
        ParamValue::Bool(true),
        None,
        None,
        accessors!(
            bool,
            "hardware.switches.hot_water.enabled",
            hardware.switches.hot_water.enabled
        ),
    ),
    ParamSpec::new(
        "hardware.switches.hot_water.type",
        "Type of water switch connected",
        ParamKind::Enum,
        ParamValue::Enum(SwitchType::Toggle as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.switches.hot_water.type",
            SwitchType,
            hardware.switches.hot_water.r#type
        ),
    ),
    ParamSpec::new(
        "hardware.switches.hot_water.mode",
        "Electrical configuration of water switch",
        ParamKind::Enum,
        ParamValue::Enum(SwitchMode::NormallyOpen as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.switches.hot_water.mode",
            SwitchMode,
            hardware.switches.hot_water.mode
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.temperature.type",
        "Type of temperature sensor connected",
        ParamKind::Enum,
        // TSIC_306, the C++'s value at `Config.h:1085-1092`. Both the schema
        // default and the struct default must agree, or `Config::default()` and
        // `schema::SCHEMA` diverge and the export test fails. See
        // `HardwareSensorsTemperature`'s Default impl for why this was
        // `DALLAS_DS18B20` in an earlier revision and is not now.
        ParamValue::Enum(TemperatureSensorType::Tsic306 as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.sensors.temperature.type",
            TemperatureSensorType,
            hardware.sensors.temperature.r#type
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.pressure.enabled",
        "Enable pressure sensor functionality",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "hardware.sensors.pressure.enabled",
            hardware.sensors.pressure.enabled
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.watertank.enabled",
        "Enable water tank level sensor",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "hardware.sensors.watertank.enabled",
            hardware.sensors.watertank.enabled
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.watertank.mode",
        "Electrical configuration of water tank sensor",
        ParamKind::Enum,
        ParamValue::Enum(SwitchMode::NormallyClosed as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.sensors.watertank.mode",
            SwitchMode,
            hardware.sensors.watertank.mode
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.watertank.keep_heater_on_empty",
        "Warning: keeps the PID/heater active even when the water tank is reported empty. Only the external reservoir is protected by this sensor, not the boiler.",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "hardware.sensors.watertank.keep_heater_on_empty",
            hardware.sensors.watertank.keep_heater_on_empty
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.scale.enabled",
        "Enable scale functionality",
        ParamKind::Bool,
        ParamValue::Bool(false),
        None,
        None,
        accessors!(
            bool,
            "hardware.sensors.scale.enabled",
            hardware.sensors.scale.enabled
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.scale.samples",
        "Number of samples used for calibration",
        ParamKind::Int,
        ParamValue::Int(2),
        Some(1.0),
        Some(20.0),
        accessors!(
            int,
            "hardware.sensors.scale.samples",
            hardware.sensors.scale.samples
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.scale.type",
        "Integrated HX711-based scale with different load cell configurations or Bluetooth Low Energy scales",
        ParamKind::Enum,
        ParamValue::Enum(ScaleType::Hx711Dual as i8),
        None,
        None,
        accessors!(
            enum,
            "hardware.sensors.scale.type",
            ScaleType,
            hardware.sensors.scale.r#type
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.scale.calibration",
        "Raw data is divided by this value to convert to readable data",
        ParamKind::Float,
        ParamValue::Float(1.0),
        Some(-999_999.0),
        Some(999_999.0),
        accessors!(
            float,
            "hardware.sensors.scale.calibration",
            hardware.sensors.scale.calibration
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.scale.calibration2",
        "Second calibration factor for dual load cell scales",
        ParamKind::Float,
        ParamValue::Float(1.0),
        Some(-999_999.0),
        Some(999_999.0),
        accessors!(
            float,
            "hardware.sensors.scale.calibration2",
            hardware.sensors.scale.calibration2
        ),
    ),
    ParamSpec::new(
        "hardware.sensors.scale.known_weight",
        "Calibration weight for scale (weight of the tray)",
        ParamKind::Float,
        ParamValue::Float(267.0),
        Some(1.0),
        Some(2000.0),
        accessors!(
            float,
            "hardware.sensors.scale.known_weight",
            hardware.sensors.scale.known_weight
        ),
    ),
    // ---- the two parameters the C++ defines but never registers ----------
    // `Config.h:813` and `Config.h:822`; read by `EmergencyStopManager.cpp:18-19`
    // and missing from `Config::getAllConfigParams()`. See the module docs.
    ParamSpec::new(
        "safety.emergency_temp",
        "Temperature threshold that triggers emergency stop",
        ParamKind::Float,
        ParamValue::Float(150.0),
        Some(120.0),
        Some(180.0),
        accessors!(float, "safety.emergency_temp", safety.emergency_temp),
    ),
    ParamSpec::new(
        "safety.emergency_hysteresis",
        "Temperature drop required to reset emergency counter",
        ParamKind::Float,
        ParamValue::Float(5.0),
        Some(1.0),
        Some(15.0),
        accessors!(
            float,
            "safety.emergency_hysteresis",
            safety.emergency_hysteresis
        ),
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
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;
    use cc_domain::hardware::RelayTriggerType;

    /// The UI's own copy of the enum table, read at compile time.
    ///
    /// `ui/packages/frontend/src/lib/parameter-metadata.ts` is a **hand-written
    /// list** of `{ value, label }` pairs for every enum parameter. It drifted
    /// once: `display.language` shipped as `0 = Deutsch, 1 = English` against the
    /// firmware's `English = 0, German = 1`, so choosing English in the UI wrote
    /// German and the panel came up in German. Nothing caught it because the two
    /// lists are in different languages in different directories with no shared
    /// source of truth.
    ///
    /// So this parses that file and asserts, per enum parameter, that the
    /// firmware's discriminants and the UI's labels still describe the same
    /// thing in the same order. A swapped pair, a renamed variant or a dropped
    /// option fails the build.
    const UI_METADATA: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../ui/packages/frontend/src/lib/parameter-metadata.ts"
    ));

    /// The `{ value: N, label: "X" }` pairs the UI declares for one parameter.
    fn ui_options(source: &str, key: &str) -> Vec<(i64, String)> {
        // The table's entries are objects; find the one naming `key`, then read
        // its `options` array. A hand-rolled scan rather than a parser: the file
        // is TypeScript, and a dependency to read it would be worse than twenty
        // lines that fail loudly if the shape changes.
        let anchor = format!("name: \"{key}\"");
        let start = source
            .find(&anchor)
            .unwrap_or_else(|| panic!("{key} is not in the UI's enum table at all"));
        let options_at = source[start..]
            .find("options: [")
            .unwrap_or_else(|| panic!("{key} has an entry but no options array"))
            + start;
        let end = options_at
            + source[options_at..]
                .find(']')
                .expect("an options array that closes");
        source[options_at..end]
            .split('{')
            .skip(1)
            .filter_map(|entry| {
                let value = entry
                    .split("value: ")
                    .nth(1)?
                    .split(&[',', ' '][..])
                    .next()?;
                let label = entry.split("label: \"").nth(1)?.split('"').next()?;
                Some((value.trim().parse().ok()?, label.to_string()))
            })
            .collect()
    }

    /// Every enum parameter the firmware declares, with the labels the UI shows.
    ///
    /// Built by hand from the schema rather than parsed out of it, because the
    /// thing being checked is exactly the mapping between the schema and the UI
    /// — parsing both sides the same way would hide a mistake in the parser.
    fn enum_parameters() -> Vec<(&'static str, Vec<&'static str>)> {
        vec![
            ("display.language", vec!["English", "Deutsch", "Español"]),
            (
                "display.template",
                vec![
                    "Standard",
                    "Minimal",
                    "Temp only",
                    "Scale",
                    "Upright",
                    "Modern",
                ],
            ),
            ("brew.mode", vec!["Manual", "Automatic"]),
            ("hardware.switches.brew.type", vec!["Momentary", "Toggle"]),
            (
                "hardware.switches.brew.mode",
                vec!["Normally Open", "Normally Closed"],
            ),
            ("hardware.switches.steam.type", vec!["Momentary", "Toggle"]),
            (
                "hardware.switches.steam.mode",
                vec!["Normally Open", "Normally Closed"],
            ),
            ("hardware.switches.power.type", vec!["Momentary", "Toggle"]),
            (
                "hardware.switches.power.mode",
                vec!["Normally Open", "Normally Closed"],
            ),
            (
                "hardware.switches.hot_water.type",
                vec!["Momentary", "Toggle"],
            ),
            (
                "hardware.switches.hot_water.mode",
                vec!["Normally Open", "Normally Closed"],
            ),
            (
                "hardware.sensors.temperature.type",
                vec!["TSIC306", "Dallas DS18B20"],
            ),
            (
                "hardware.sensors.watertank.mode",
                vec!["Normally Open", "Normally Closed"],
            ),
            (
                "hardware.sensors.scale.type",
                vec!["2 load cells", "1 load cell", "Bluetooth"],
            ),
            ("hardware.oled.type", vec!["SH1106", "SSD1306"]),
            (
                "hardware.relays.heater.trigger_type",
                vec!["Low Trigger", "High Trigger"],
            ),
            (
                "hardware.relays.valve.trigger_type",
                vec!["Low Trigger", "High Trigger"],
            ),
            (
                "hardware.relays.pump.trigger_type",
                vec!["Low Trigger", "High Trigger"],
            ),
        ]
    }

    #[test]
    fn the_ui_enum_labels_match_the_firmware_discriminants() {
        for (key, expected) in enum_parameters() {
            let options = ui_options(UI_METADATA, key);
            assert_eq!(
                options.len(),
                expected.len(),
                "{key}: the firmware has {} enum values and the UI offers {} options",
                expected.len(),
                options.len()
            );
            for (index, (value, label)) in options.iter().enumerate() {
                assert_eq!(
                    *value,
                    i64::try_from(index).unwrap_or(i64::MAX),
                    "{key}: the UI's option {index} has value {value}; the firmware's \
                     discriminants are consecutive from 0, so a gap means one of the \
                     two sides has a variant the other does not"
                );
                assert_eq!(
                    label, &expected[index],
                    "{key}: the UI calls value {value} {:?} and the firmware calls it \
                     {:?}. This is the bug that made the panel come up German when the \
                     UI said English.",
                    label, expected[index]
                );
            }
        }
    }

    #[test]
    fn every_enum_parameter_the_ui_lists_is_one_the_firmware_has() {
        // The other direction: a parameter the UI offers options for that the
        // schema does not declare is a control that writes a key nothing reads.
        for (key, _) in enum_parameters() {
            assert!(
                SCHEMA.iter().any(|spec| spec.key == key),
                "{key} has a UI enum table but no schema entry"
            );
        }
    }

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

    /// Every getter reads the field its own `default` was taken from.
    ///
    /// **This is the test that was missing, and the reason the collapse needed
    /// one.** Two facts about a parameter were checked and a third was not:
    ///
    /// * `every_schema_default_matches_the_config_default` compares
    ///   `Config::default()` against the **spec's** `default`, so it is
    ///   satisfied by two wrong values that agree with each other.
    /// * `every_schema_key_has_a_live_value` checks a key resolves to *some*
    ///   value.
    /// * Nothing checked that `ParamSpec::get` reads the field the spec is
    ///   *about*.
    ///
    /// So a getter pointed at a neighbouring field — `pid.regular.kp`'s reading
    /// `pid.regular.tn` — passed every test in the crate and put the wrong
    /// number in `/api/parameters`.
    ///
    /// Cheap, and not redundant: it walks the *accessor* rather than the table,
    /// so it is the one place the two halves of the collapsed table are held to
    /// each other. It does not subsume
    /// `every_schema_key_round_trips_through_set_and_live_value`, which proves
    /// the getter tracks a *write* rather than the initial value.
    #[test]
    fn every_getter_reads_the_field_its_own_default_names() {
        let config = Config::default();
        for spec in SCHEMA {
            assert_eq!(
                ParamValue::from((spec.get)(&config)),
                spec.default,
                "{}: the getter does not read the field the default was taken from",
                spec.key
            );
        }
    }
}
