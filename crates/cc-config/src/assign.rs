//! One parameter, written from a string — the C++'s `ConfigParamDef::fromString`.
//!
//! Owner: **R3-14**. `POST /api/parameters`'s writer, and the writer R3-13's
//! inbound MQTT will use.
//!
//! # What this replaces
//!
//! Three pieces of `include/clevercoffee/Config.h`, all reached from
//! `WebServerManager.cpp:841-844`:
//!
//! ```cpp
//! ConfigParamDef* paramDef = Config::getInstance().findConfigParameter(varName);
//! if (paramDef) { bool updateSuccess = paramDef->fromString(value); }
//! ```
//!
//! * `ParamDef<T>::fromString` (`:242-262`) — the string → typed value
//!   conversion, per type.
//! * `ParamDef<T>::set` (`:156-180`) — the range check, the assignment, and the
//!   **immediate NVS write**.
//! * `ParamDef<T>::isValid` (`:190-200`) — `>=`/`<=` on the numeric kinds, and
//!   unconditionally `true` for `bool` and `String`.
//!
//! # Why the three are one function here
//!
//! The C++ spreads "what does this value mean", "is it allowed" and "put it in
//! the struct" across a virtual call, a range check and an assignment. This port
//! has no virtual dispatch — [`SCHEMA`](crate::schema::SCHEMA) is the type — so
//! the same three steps are [`parse`] (string → typed, checked),
//! [`set`] (typed → the struct field) and [`apply`] (the handler's loop over
//! every pair). Nothing else in the workspace may write a parameter: a second
//! writer is a second set of type rules, and the two drift.
//!
//! # Two deliberate differences from the C++
//!
//! **1. A value that does not parse is a rejection, not a zero.** The C++'s
//! `fromString` cannot fail on a scalar:
//!
//! ```cpp
//! newValue = value.equalsIgnoreCase("true") || value == "1";  // bool: anything else is false
//! newValue = value.toInt();                                    // int:  "12abc" is 12, "abc" is 0
//! newValue = value.toDouble();                                 // double: likewise
//! ```
//!
//! so `POST /api/parameters?pid.regular.kp=hello` writes **0** and answers
//! `200 {"success":true}` if 0 is in range, and `400` if it is not. An operator
//! who fat-fingers a PID gain is told it worked. Here `hello` is
//! `AssignError::Rejected(RejectReason::WrongType)` and the answer is `400`,
//! which is the same status the C++ gives for the out-of-range case and for an
//! unknown key. The *shape* of the response is the C++'s; the arithmetic behind
//! it is not a footgun.
//!
//! **2. An enumeration is written by discriminant only.** The C++'s
//! `EnumParamDef::fromString` (`:437-455`) falls back to matching the option's
//! **label** ("High", "Momentary"). [`ParamSpec`] carries no label table — the
//! React editor renders enums from `type: 5` and an integer, and
//! `json::enum_discriminants_known` validates against the Rust enum — so
//! `"brew.mode=Momentary"` is
//! `UnknownEnumDiscriminant` here and `?brew.mode=1` is the write. Every
//! enumeration this firmware has is a `u8`-wide discriminant, so the integer
//! form is also what `/api/parameters` reports as the current `value`.
//!
//! # The C++'s persistence, and where it moved
//!
//! `set` writes NVS **inside the setter**, once per parameter, and returns
//! `false` if that write failed — so a NVS failure is reported to the operator
//! as a parameter failure even though the in-memory value had already changed.
//! Here [`set`] only touches the struct, and the single [`crate::ConfigStore`]
//! write happens once for the whole request, in the task that owns the store.
//! One blob, one write, one CRC — which is the reason this port does not have
//! 98 keys to write one at a time ([`crate::store`]). A store failure is
//! therefore a property of the request, not of one parameter, and it is
//! reported as such by the caller.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

use crate::config::Config;
use crate::json::{LiveValue, RejectReason, MAX_TEXT_LEN};
use crate::schema::{self, ParamKind, ParamSpec};

/// Why a parameter was not written.
///
/// The two cases the C++ distinguishes are "no such parameter" and "the value
/// did not survive validation" (`WebServerManager.cpp:843-859`), and it answers
/// the same `400` for both. They are kept apart here because the second carries
/// a reason worth logging and the first never does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AssignError {
    /// `findConfigParameter` returned `nullptr` (`Config.h:1550`): the schema
    /// does not register this key.
    UnknownKey,
    /// The value was not acceptable for this parameter.
    Rejected(RejectReason),
}

impl core::fmt::Display for AssignError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownKey => f.write_str("no such parameter"),
            Self::Rejected(reason) => {
                f.write_str("value rejected: ")?;
                f.write_str(&crate::json::describe_reason(*reason))
            }
        }
    }
}

/// Convert one parameter's string value into the typed value to store.
///
/// Port of `ParamDef<T>::fromString` (`Config.h:242-262`) followed by
/// `isValid` (`:190-200`). The two are one step here because the C++'s
/// `fromString` ends by calling `set`, which validates — splitting them would
/// leave a caller able to skip the check.
///
/// `raw` is borrowed into the result for the text kinds, which is why this takes
/// a `&str` and not a `String`.
///
/// # Errors
///
/// [`AssignError::UnknownKey`] if [`SCHEMA`](crate::schema::SCHEMA) does not
/// register `key`, and [`AssignError::Rejected`] with the reason if the value is
/// not of the parameter's kind, is outside its bounds, is not a variant of its
/// enumeration, or is longer than the store.
pub fn parse<'a>(key: &str, raw: &'a str) -> Result<LiveValue<'a>, AssignError> {
    let spec = schema::find(key).ok_or(AssignError::UnknownKey)?;
    let rejected = AssignError::Rejected;

    match spec.kind {
        // `value.equalsIgnoreCase("true") || value == "1"`
        // (`Config.h:246`) accepts exactly two spellings of each. The C++ treats
        // every other string as `false` — see the module's first difference.
        ParamKind::Bool => Ok(LiveValue::Bool(match raw {
            "1" => true,
            "0" => false,
            other if other.eq_ignore_ascii_case("true") => true,
            other if other.eq_ignore_ascii_case("false") => false,
            _ => return Err(rejected(RejectReason::WrongType)),
        })),
        // `value.toInt()` is lenient; `str::parse` is not. The whole field must
        // be the number, which is what a browser's number input sends.
        ParamKind::Int => {
            let value = raw
                .parse::<i32>()
                .map_err(|_| rejected(RejectReason::WrongType))?;
            crate::json::check_range(spec, f64::from(value)).map_err(rejected)?;
            Ok(LiveValue::Int(value))
        }
        ParamKind::Float => {
            let value = raw
                .parse::<f64>()
                .map_err(|_| rejected(RejectReason::WrongType))?;
            // `f64::parse` accepts `NaN` and `inf`, and every comparison against
            // `NaN` is false — so `check_range` would wave it through and a
            // `NaN` PID gain would reach `cc-machine`'s integrator. The C++
            // cannot store one (`String::toDouble` on a machine without
            // `NAN` support yields 0), and the range check is documented as the
            // only validation, so the finiteness test belongs here rather than
            // in the bounds.
            if !value.is_finite() {
                return Err(rejected(RejectReason::WrongType));
            }
            crate::json::check_range(spec, value).map_err(rejected)?;
            Ok(LiveValue::Float(value))
        }
        // `newValue = value` with no validation at all (`Config.h:253-255`):
        // a string is always valid, so the only rule is the store's own length.
        ParamKind::Text => {
            if raw.len() > MAX_TEXT_LEN {
                return Err(rejected(RejectReason::TooLong));
            }
            Ok(LiveValue::Text(raw))
        }
        // `intValue = value.toInt()` then `isValid(enumValue)`
        // (`Config.h:439-444`), minus the label fallback — see the module's
        // second difference.
        ParamKind::Enum => {
            let value = raw
                .parse::<i32>()
                .map_err(|_| rejected(RejectReason::WrongType))?;
            let value =
                i8::try_from(value).map_err(|_| rejected(RejectReason::UnknownEnumDiscriminant))?;
            if !crate::json::enum_discriminants_known(key, value) {
                return Err(rejected(RejectReason::UnknownEnumDiscriminant));
            }
            Ok(LiveValue::Enum(value))
        }
    }
}

/// Write one already-validated value into a [`Config`].
///
/// The write half of [`crate::json::live_value`], and the table is the same 98
/// keys in the same grouping. `false` means the key is not in the table or the
/// value is not of the field's type; after [`parse`] has accepted a value, only
/// the first is reachable, and
/// `every_schema_key_round_trips_through_set_and_live_value` is what holds the
/// two tables to the same 98 keys.
///
/// # Note on the four macros
///
/// They exist so each entry of the table is one line — `"pid.enabled" =>
/// pid.enabled` — which is what makes a key that exists in
/// [`SCHEMA`](crate::schema::SCHEMA) and not
/// here visible as a missing line rather than as a missing block. Each expands
/// to a comparison and an early `return`, so a key is found by scanning the
/// five groups; at 98 keys of ~20 bytes that is a few microseconds, on a path
/// that runs once per parameter write.
#[allow(
    clippy::too_many_lines,
    reason = "this IS the key table: one line per registered parameter, and the \
              C++ spells the same thing as a 98-entry vector of parameter \
              definitions (`Config::getAllConfigParams`, `Config.cpp:438-560`). \
              A 98-arm match is that table in Rust form; splitting it would hide \
              the property the function exists to provide, which is that every \
              key in `SCHEMA` is accounted for"
)]
pub fn set(config: &mut Config, key: &str, value: &LiveValue<'_>) -> bool {
    use cc_domain::hardware::{
        OledAddress as A, OledType as T, RelayTriggerType as R, ScaleType as Sc, SwitchMode as M,
        SwitchType as Sw, TemperatureSensorType as Ts,
    };
    use cc_domain::process::BrewMode as B;
    use cc_domain::system::{DisplayTemplate as D, Language as L, LogLevel as G};

    macro_rules! put {
        ($extract:ident, $value:ident, $config:ident, $($field:ident).+) => {{
            if eq_key(key, stringify!($($field).+)) {
                return match $extract($value) {
                    Some(v) => {
                        $config.$($field).+ = v;
                        true
                    }
                    None => false,
                };
            }
        }};
    }
    macro_rules! put_secret {
        ($value:ident, $config:ident, $($field:ident).+) => {{
            if eq_key(key, stringify!($($field).+)) {
                return match as_text($value) {
                    Some(v) => {
                        $config.$($field).+.set(v.to_owned());
                        true
                    }
                    None => false,
                };
            }
        }};
    }
    macro_rules! put_enum {
        ($variant:ty, $value:ident, $config:ident, $($field:ident).+) => {{
            if eq_key(key, stringify!($($field).+)) {
                return match as_enum($value).and_then(<$variant>::from_raw) {
                    Some(v) => {
                        $config.$($field).+ = v;
                        true
                    }
                    None => false,
                };
            }
        }};
    }

    // --- bool (36) ----------------------------------------------------------
    put!(as_bool, value, config, pid.enabled);
    put!(as_bool, value, config, pid.use_ponm);
    put!(as_bool, value, config, pid.bd.enabled);
    put!(as_bool, value, config, brew.by_time.enabled);
    put!(as_bool, value, config, brew.by_weight.enabled);
    put!(as_bool, value, config, brew.by_weight.auto_tare);
    put!(as_bool, value, config, brew.pre_infusion.enabled);
    put!(as_bool, value, config, display.fullscreen_brew_timer);
    put!(
        as_bool,
        value,
        config,
        display.fullscreen_manual_flush_timer
    );
    put!(as_bool, value, config, display.fullscreen_hot_water_timer);
    put!(as_bool, value, config, display.heating_logo);
    put!(as_bool, value, config, display.pid_off_logo);
    put!(as_bool, value, config, display.inverted);
    put!(as_bool, value, config, hardware.oled.enabled);
    put!(as_bool, value, config, hardware.leds.status.enabled);
    put!(as_bool, value, config, hardware.leds.status.inverted);
    put!(as_bool, value, config, hardware.leds.brew.enabled);
    put!(as_bool, value, config, hardware.leds.brew.inverted);
    put!(as_bool, value, config, hardware.leds.steam.enabled);
    put!(as_bool, value, config, hardware.leds.steam.inverted);
    put!(as_bool, value, config, hardware.switches.brew.enabled);
    put!(as_bool, value, config, hardware.switches.steam.enabled);
    put!(as_bool, value, config, hardware.switches.power.enabled);
    put!(as_bool, value, config, hardware.switches.hot_water.enabled);
    put!(as_bool, value, config, hardware.sensors.pressure.enabled);
    put!(as_bool, value, config, hardware.sensors.watertank.enabled);
    put!(
        as_bool,
        value,
        config,
        hardware.sensors.watertank.keep_heater_on_empty
    );
    put!(as_bool, value, config, hardware.sensors.scale.enabled);
    put!(
        as_bool,
        value,
        config,
        maintenance.backflush_reminder.enabled
    );
    put!(as_bool, value, config, standby.enabled);
    put!(as_bool, value, config, mqtt.enabled);
    put!(as_bool, value, config, mqtt.hassio.enabled);
    put!(as_bool, value, config, system.offline_mode);
    put!(as_bool, value, config, system.auth.enabled);
    put!(as_bool, value, config, system.timing_debug.enabled);
    put!(as_bool, value, config, system.showdisplay.enabled);

    // --- int (4) -----------------------------------------------------------
    put!(as_int, value, config, backflush.cycles);
    put!(
        as_int,
        value,
        config,
        maintenance.backflush_reminder.threshold
    );
    put!(as_int, value, config, mqtt.port);
    put!(as_int, value, config, hardware.sensors.scale.samples);

    // --- float (27) --------------------------------------------------------
    put!(as_float, value, config, pid.ema_factor);
    put!(as_float, value, config, pid.regular.kp);
    put!(as_float, value, config, pid.regular.tn);
    put!(as_float, value, config, pid.regular.tv);
    put!(as_float, value, config, pid.regular.i_max);
    put!(as_float, value, config, pid.steam.kp);
    put!(as_float, value, config, pid.bd.kp);
    put!(as_float, value, config, pid.bd.tn);
    put!(as_float, value, config, pid.bd.tv);
    put!(as_float, value, config, brew.setpoint);
    put!(as_float, value, config, brew.temp_offset);
    put!(as_float, value, config, brew.pid_delay);
    put!(as_float, value, config, brew.by_time.target_time);
    put!(as_float, value, config, brew.by_weight.target_weight);
    put!(as_float, value, config, brew.pre_infusion.time);
    put!(as_float, value, config, brew.pre_infusion.pause);
    put!(as_float, value, config, steam.setpoint);
    put!(as_float, value, config, display.post_brew_timer_duration);
    put!(as_float, value, config, display.blinking.delta);
    put!(as_float, value, config, backflush.fill_time);
    put!(as_float, value, config, backflush.flush_time);
    put!(as_float, value, config, standby.time);
    put!(as_float, value, config, hardware.sensors.scale.calibration);
    put!(as_float, value, config, hardware.sensors.scale.calibration2);
    put!(as_float, value, config, hardware.sensors.scale.known_weight);
    put!(as_float, value, config, safety.emergency_temp);
    put!(as_float, value, config, safety.emergency_hysteresis);

    // --- text (11) ---------------------------------------------------------
    put!(as_text_owned, value, config, mqtt.broker);
    put!(as_text_owned, value, config, mqtt.username);
    put!(as_text_owned, value, config, mqtt.topic);
    put!(as_text_owned, value, config, mqtt.hassio.prefix);
    put!(as_text_owned, value, config, system.hostname);
    put!(as_text_owned, value, config, system.auth.username);
    put!(as_text_owned, value, config, system.wifi.ssid);
    put_secret!(value, config, mqtt.password);
    put_secret!(value, config, system.ota_password);
    put_secret!(value, config, system.auth.password);
    put_secret!(value, config, system.wifi.password);

    // --- enum (20) ---------------------------------------------------------
    put_enum!(B, value, config, brew.mode);
    put_enum!(D, value, config, display.template);
    put_enum!(L, value, config, display.language);
    put_enum!(G, value, config, system.log_level);
    put_enum!(T, value, config, hardware.oled.r#type);
    put_enum!(A, value, config, hardware.oled.address);
    put_enum!(R, value, config, hardware.relays.heater.trigger_type);
    put_enum!(R, value, config, hardware.relays.valve.trigger_type);
    put_enum!(R, value, config, hardware.relays.pump.trigger_type);
    put_enum!(Sw, value, config, hardware.switches.brew.r#type);
    put_enum!(M, value, config, hardware.switches.brew.mode);
    put_enum!(Sw, value, config, hardware.switches.steam.r#type);
    put_enum!(M, value, config, hardware.switches.steam.mode);
    put_enum!(Sw, value, config, hardware.switches.power.r#type);
    put_enum!(M, value, config, hardware.switches.power.mode);
    put_enum!(Sw, value, config, hardware.switches.hot_water.r#type);
    put_enum!(M, value, config, hardware.switches.hot_water.mode);
    put_enum!(Ts, value, config, hardware.sensors.temperature.r#type);
    put_enum!(M, value, config, hardware.sensors.watertank.mode);
    put_enum!(Sc, value, config, hardware.sensors.scale.r#type);

    false
}

/// Whether the dotted `key` is the field path `mangled` names.
///
/// `stringify!` renders a raw identifier with its `r#` — the key
/// [`SCHEMA`](crate::schema::SCHEMA) registers is `hardware.oled.type` and
/// `stringify!(hardware.oled.r#type)` is `hardware.oled.r#type`. Writing the key
/// a second time as a string literal to compare against would make the table two
/// facts per entry, and a table of two facts per entry is a table that can
/// disagree with itself; comparing with this instead keeps one.
fn eq_key(key: &str, mangled: &str) -> bool {
    let mut rest = key;
    for segment in mangled.split('.') {
        let segment = segment.strip_prefix("r#").unwrap_or(segment);
        let Some(tail) = rest.strip_prefix(segment) else {
            return false;
        };
        rest = tail.strip_prefix('.').unwrap_or(tail);
    }
    rest.is_empty()
}

/// What one [`apply`] call did.
///
/// The two counters are the C++'s `hasUpdates` and `hasErrors`
/// (`WebServerManager.cpp:826-827`), and `failed` is kept whole rather than
/// reduced to a bool because the log line that follows a `400` has to say
/// *which* parameter was wrong.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    /// How many parameters were written.
    pub updated: usize,
    /// Every rejection, in request order.
    pub failed: Vec<(String, AssignError)>,
}

impl Applied {
    /// Whether any parameter was rejected — the C++'s `hasErrors`.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.failed.is_empty()
    }
}

/// Write every pair, independently, and report what happened.
///
/// The C++'s handler loop (`WebServerManager.cpp:829-865`), with its two
/// properties preserved deliberately:
///
/// * **One bad parameter does not roll back the others.** The C++ applies each
///   pair as it walks them and only collects `hasErrors`; a request that sets
///   four parameters and misspells the fifth leaves four written. That is what
///   an operator who fixes a form and resubmits expects, and it is the opposite
///   of [`crate::json::json_import`], which is all-or-nothing because it is
///   validating a whole document rather than a list of instructions.
/// * **A repeated key is applied twice**, last one winning, because the C++
///   iterates `request->params()` and does not deduplicate.
///
/// An empty or valueless field never reaches here: the C++ skips those before
/// the loop (`p->name().length() > 0 && p->value().length() > 0`, `:830`), which
/// is why a text parameter cannot be set to the empty string over HTTP.
pub fn apply(config: &mut Config, pairs: &[crate::form::Field]) -> Applied {
    let mut out = Applied::default();
    for (key, raw) in pairs {
        if key.is_empty() || raw.is_empty() {
            continue;
        }
        match parse(key, raw) {
            Ok(value) => {
                if set(config, key, &value) {
                    out.updated += 1;
                } else {
                    out.failed.push((key.clone(), AssignError::UnknownKey));
                }
            }
            Err(err) => out.failed.push((key.clone(), err)),
        }
    }
    out
}

/// The `bool` a [`LiveValue`] carries, if that is what it is.
fn as_bool(value: &LiveValue<'_>) -> Option<bool> {
    match value {
        LiveValue::Bool(v) => Some(*v),
        _ => None,
    }
}

/// The `i32` a [`LiveValue`] carries, if that is what it is.
fn as_int(value: &LiveValue<'_>) -> Option<i32> {
    match value {
        LiveValue::Int(v) => Some(*v),
        _ => None,
    }
}

/// The `f64` a [`LiveValue`] carries, if that is what it is.
fn as_float(value: &LiveValue<'_>) -> Option<f64> {
    match value {
        LiveValue::Float(v) => Some(*v),
        _ => None,
    }
}

/// The `&str` a [`LiveValue`] carries, if that is what it is.
fn as_text<'a>(value: &LiveValue<'a>) -> Option<&'a str> {
    match value {
        LiveValue::Text(v) => Some(v),
        _ => None,
    }
}

/// The `&str` a [`LiveValue`] carries, owned — for a `String` field.
fn as_text_owned(value: &LiveValue<'_>) -> Option<String> {
    as_text(value).map(ToOwned::to_owned)
}

/// The discriminant a [`LiveValue`] carries, if that is what it is.
fn as_enum(value: &LiveValue<'_>) -> Option<i8> {
    match value {
        LiveValue::Enum(v) => Some(*v),
        _ => None,
    }
}

/// Whether `spec` would accept a value parsed from `raw`.
///
/// [`parse`] without the write, for a caller that needs the verdict before it
/// has a [`Config`] to write into — the HTTP handler, which answers `400` for a
/// rejected value and does not own the configuration.
#[must_use]
pub fn accepts(spec: &ParamSpec, raw: &str) -> bool {
    parse(spec.key, raw).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::live_value;
    use crate::schema::{ParamValue, SCHEMA};
    use alloc::format;
    use alloc::string::ToString;

    /// The four kinds, written and read back.
    ///
    /// `hardware.switches.brew.enabled` (bool), `mqtt.port` (int),
    /// `pid.regular.kp` (float) and `system.hostname` (text) — one of each kind,
    /// chosen because none of them is a value the running machine depends on for
    /// safety, and because a text parameter is the one that needs a `String` and
    /// a bool the one that needs nothing.
    #[test]
    fn all_four_kinds_are_writable_and_read_back() {
        let mut config = Config::default();
        let applied = apply(
            &mut config,
            &[
                ("pid.enabled".to_owned(), "false".to_owned()),
                ("mqtt.port".to_owned(), "1884".to_owned()),
                ("pid.regular.kp".to_owned(), "3.5".to_owned()),
                ("system.hostname".to_owned(), "kettle".to_owned()),
            ],
        );
        assert_eq!(applied.updated, 4, "{:?}", applied.failed);
        assert!(!applied.has_errors());
        assert!(!config.pid.enabled);
        assert_eq!(config.mqtt.port, 1884);
        assert!((config.pid.regular.kp - 3.5).abs() < f64::EPSILON);
        assert_eq!(config.system.hostname, "kettle");
        // And through the read path, which is what `/api/parameters` reports.
        assert_eq!(live_value(&config, "mqtt.port"), Some(LiveValue::Int(1884)));
        assert_eq!(
            live_value(&config, "system.hostname"),
            Some(LiveValue::Text("kettle"))
        );
    }

    /// The table in [`set`] and the table in [`live_value`] must be the same 98
    /// keys, or a parameter is readable and not writable (or the reverse).
    ///
    /// Every key is given a value of its own kind that is in range — the
    /// mid-point of the schema's own bounds, or the default when it has none —
    /// then read back with [`live_value`]. A key missing from either table shows
    /// up as `set` returning `false`, or as a read-back that is not the value
    /// that was written.
    #[test]
    fn every_schema_key_round_trips_through_set_and_live_value() {
        for spec in SCHEMA {
            let probe = probe_for(spec);
            let mut config = Config::default();
            assert!(
                set(&mut config, spec.key, &probe),
                "{} is not writable",
                spec.key
            );
            assert_eq!(
                live_value(&config, spec.key),
                Some(probe),
                "{} did not read back",
                spec.key
            );
        }
    }

    /// A key that is not registered is an unknown key, not a silent no-op.
    #[test]
    fn an_unknown_key_is_rejected() {
        assert_eq!(parse("pid.kp", "1"), Err(AssignError::UnknownKey));
        assert_eq!(parse("", "1"), Err(AssignError::UnknownKey));
        let mut config = Config::default();
        let applied = apply(&mut config, &[("nope".to_owned(), "1".to_owned())]);
        assert_eq!(applied.updated, 0);
        assert_eq!(applied.failed.len(), 1);
    }

    /// A value outside the parameter's range is rejected, and the parameter is
    /// left alone.
    #[test]
    fn a_value_out_of_range_is_rejected_and_nothing_is_written() {
        // 1e300 is outside every float parameter's bounds, and -1000 °C is
        // outside `safety.emergency_temp`'s.
        let mut config = Config::default();
        let before = config.pid.regular.kp;
        let applied = apply(
            &mut config,
            &[
                ("pid.regular.kp".to_owned(), "1e300".to_owned()),
                ("safety.emergency_temp".to_owned(), "-1000".to_owned()),
            ],
        );
        assert_eq!(applied.updated, 0);
        assert_eq!(applied.failed.len(), 2);
        assert!(
            (config.pid.regular.kp - before).abs() < f64::EPSILON,
            "{} was written by a rejected request",
            "pid.regular.kp"
        );
    }

    /// A value that does not parse is a rejection here and a silent zero in the
    /// C++ (`Config.h:249-252`). Pinned because it is the module's first
    /// documented difference and it is the one that would otherwise be "fixed"
    /// back into a `toInt()`-alike by a later reader who has not read it.
    #[test]
    fn a_value_that_does_not_parse_is_rejected_rather_than_becoming_zero() {
        for raw in ["", "abc", "12abc", "4.5", " 7", "7 "] {
            assert_eq!(
                parse("mqtt.port", raw),
                Err(AssignError::Rejected(RejectReason::WrongType)),
                "mqtt.port={raw:?}"
            );
        }
        for raw in ["", "maybe", "1.2.3", "NaN", "inf"] {
            assert_eq!(
                parse("pid.regular.kp", raw),
                Err(AssignError::Rejected(RejectReason::WrongType)),
                "pid.regular.kp={raw:?}"
            );
        }
    }

    /// The C++'s `equalsIgnoreCase("true") || value == "1"` (`Config.h:246`),
    /// and nothing else.
    #[test]
    fn a_bool_accepts_exactly_the_cpp_two_spellings() {
        for (raw, expected) in [
            ("1", true),
            ("0", false),
            ("true", true),
            ("TRUE", true),
            ("True", true),
            ("false", false),
            ("FALSE", false),
        ] {
            assert_eq!(
                parse("pid.enabled", raw),
                Ok(LiveValue::Bool(expected)),
                "pid.enabled={raw:?}"
            );
        }
        // The C++ would answer `false` for every one of these and store it.
        for raw in ["", "yes", "no", "on", "2", " true"] {
            assert_eq!(
                parse("pid.enabled", raw),
                Err(AssignError::Rejected(RejectReason::WrongType)),
                "pid.enabled={raw:?}"
            );
        }
    }

    /// An enumeration is written by discriminant; the C++'s label fallback is
    /// not reproduced (the module's second difference).
    #[test]
    fn an_enum_is_written_by_discriminant_only() {
        assert_eq!(parse("brew.mode", "1"), Ok(LiveValue::Enum(1)));
        let mut config = Config::default();
        assert!(set(&mut config, "brew.mode", &LiveValue::Enum(1)));
        assert_eq!(live_value(&config, "brew.mode"), Some(LiveValue::Enum(1)));
        // A discriminant no variant has, and a label — the C++'s fallback, which
        // is the one part of `EnumParamDef::fromString` this port drops.
        assert_eq!(
            parse("brew.mode", "99"),
            Err(AssignError::Rejected(RejectReason::UnknownEnumDiscriminant))
        );
        assert_eq!(
            parse("brew.mode", "Pressure"),
            Err(AssignError::Rejected(RejectReason::WrongType))
        );
    }

    /// `f64::parse` takes `NaN` and `inf`, and every comparison against `NaN` is
    /// false — so without an explicit finiteness test a `NaN` PID gain passes the
    /// range check and reaches the integrator.
    #[test]
    fn a_non_finite_float_is_rejected() {
        for raw in ["NaN", "nan", "inf", "-inf", "infinity"] {
            assert_eq!(
                parse("pid.regular.kp", raw),
                Err(AssignError::Rejected(RejectReason::WrongType)),
                "pid.regular.kp={raw:?}"
            );
        }
    }

    /// The C++'s loop (`WebServerManager.cpp:829-865`): each pair is applied on
    /// its own, and one failure does not undo the others.
    #[test]
    fn one_bad_parameter_does_not_roll_back_the_good_ones() {
        let mut config = Config::default();
        let applied = apply(
            &mut config,
            &[
                ("mqtt.port".to_owned(), "1884".to_owned()),
                ("pid.regular.kp".to_owned(), "nope".to_owned()),
                ("pid.enabled".to_owned(), "1".to_owned()),
                ("not.a.parameter".to_owned(), "1".to_owned()),
            ],
        );
        assert_eq!(applied.updated, 2, "{:?}", applied.failed);
        assert!(applied.has_errors());
        assert_eq!(applied.failed.len(), 2);
        assert_eq!(applied.failed[0].0, "pid.regular.kp");
        assert_eq!(applied.failed[1].0, "not.a.parameter");
        assert_eq!(config.mqtt.port, 1884);
        assert!(config.pid.enabled);
    }

    /// A repeated key is applied twice and the last one wins, because the C++
    /// walks `request->params()` without deduplicating.
    #[test]
    fn a_repeated_key_takes_the_last_value() {
        let mut config = Config::default();
        let applied = apply(
            &mut config,
            &[
                ("mqtt.port".to_owned(), "1884".to_owned()),
                ("mqtt.port".to_owned(), "8883".to_owned()),
            ],
        );
        assert_eq!(applied.updated, 2);
        assert_eq!(config.mqtt.port, 8883);
    }

    /// The C++ skips a field with no name or no value (`:830`), so an empty
    /// value is "not mentioned" rather than "set to nothing" — and a request
    /// that mentions nothing updates nothing, which is the C++'s third response
    /// (`{"success":true,"message":"No parameters updated"}`, `:877`).
    #[test]
    fn an_empty_or_valueless_field_is_skipped_not_rejected() {
        let mut config = Config::default();
        let applied = apply(
            &mut config,
            &[
                (String::new(), "1".to_owned()),
                ("mqtt.port".to_owned(), String::new()),
            ],
        );
        assert_eq!(applied, Applied::default());
        assert!(!applied.has_errors());
    }

    /// A credential is written through the same path as everything else, and is
    /// readable afterwards — the four `Secret<String>` fields are the only ones
    /// whose write goes through a setter rather than an assignment.
    #[test]
    fn a_credential_is_writable_and_readable() {
        let mut config = Config::default();
        let applied = apply(
            &mut config,
            &[("system.wifi.password".to_owned(), "hunter2".to_owned())],
        );
        assert_eq!(applied.updated, 1, "{:?}", applied.failed);
        assert_eq!(
            live_value(&config, "system.wifi.password"),
            Some(LiveValue::Text("hunter2"))
        );
        assert_eq!(config.wifi_password(), "hunter2");
    }

    /// A text parameter longer than the store can hold is rejected rather than
    /// written, which is the C++'s `String` bound in all but spelling.
    #[test]
    fn an_over_long_text_is_rejected() {
        let long = "x".repeat(MAX_TEXT_LEN + 1);
        assert_eq!(
            parse("system.hostname", &long),
            Err(AssignError::Rejected(RejectReason::TooLong))
        );
        assert!(parse("system.hostname", &"x".repeat(MAX_TEXT_LEN)).is_ok());
    }

    /// [`accepts`] is [`parse`] without the write, and the HTTP handler's reason
    /// for existing: a verdict it can turn into a status code without holding a
    /// `Config`.
    #[test]
    fn accepts_is_parse_without_the_write() {
        assert!(accepts(
            SCHEMA
                .iter()
                .find(|s| s.key == "mqtt.port")
                .expect("mqtt.port is registered"),
            "1883"
        ));
        assert!(!accepts(
            SCHEMA
                .iter()
                .find(|s| s.key == "mqtt.port")
                .expect("mqtt.port is registered"),
            "-"
        ));
    }

    /// The error text a log line carries.
    #[test]
    fn the_error_says_which_kind_of_failure_it_was() {
        assert_eq!(AssignError::UnknownKey.to_string(), "no such parameter");
        assert!(format!(
            "{}",
            AssignError::Rejected(RejectReason::OutOfRange { min: 0.0, max: 1.0 })
        )
        .contains("out of range [0 .. 1]"));
    }

    /// The `r#` in a `stringify!`d field path is not part of the key, and a key
    /// that is a prefix of a field path is not a match.
    #[test]
    fn eq_key_sees_through_a_raw_identifier() {
        assert!(eq_key("pid.enabled", "pid.enabled"));
        assert!(eq_key("hardware.oled.type", "hardware.oled.r#type"));
        assert!(!eq_key("hardware.oled.r#type", "hardware.oled.type"));
        // A prefix in either direction, and a shared first segment.
        assert!(!eq_key("pid", "pid.enabled"));
        assert!(!eq_key("pid.enable", "pid.enabled"));
        assert!(!eq_key("pidx.enabled", "pid.enabled"));
        assert!(!eq_key("", "pid.enabled"));
    }

    /// A value of the right shape but the wrong kind cannot be written into the
    /// wrong field: `set` is the only writer, and it checks.
    #[test]
    fn a_value_of_the_wrong_kind_is_not_written() {
        let mut config = Config::default();
        assert!(!set(&mut config, "pid.enabled", &LiveValue::Int(1)));
        assert!(!set(&mut config, "mqtt.port", &LiveValue::Bool(true)));
        assert!(!set(&mut config, "pid.regular.kp", &LiveValue::Int(1)));
        assert!(!set(&mut config, "system.hostname", &LiveValue::Float(1.0)));
        assert!(!set(&mut config, "brew.mode", &LiveValue::Int(1)));
        assert!(!config.pid.enabled);
        assert_eq!(config.mqtt.port, Config::default().mqtt.port);
    }

    /// A value in range for a parameter whose field is an enumeration still has
    /// to name a variant, so the two checks cannot be collapsed into one.
    #[test]
    fn an_enum_rejects_a_discriminant_no_variant_has_at_the_write_too() {
        let mut config = Config::default();
        assert!(!set(&mut config, "brew.mode", &LiveValue::Enum(99)));
        assert_eq!(live_value(&config, "brew.mode"), Some(LiveValue::Enum(0)));
    }

    /// A probe value for `spec` that is in range and **differs from its
    /// default**, so a read-back of the probe proves the assignment reached the
    /// field rather than proving the field already held that value.
    ///
    /// The enumeration probe is `default ^ 1` — the neighbouring variant. Every
    /// `IntEnum` in `cc-domain` has both discriminant 0 and 1
    /// (`hardware.rs:37-128`, `system.rs:35-80`, `process.rs:35-41`), and
    /// `the_probe_names_a_variant_for_every_enumeration` is the test that keeps
    /// that true if a two-variant enum is ever added.
    fn probe_for(spec: &ParamSpec) -> LiveValue<'static> {
        match spec.kind {
            ParamKind::Bool => {
                // `as_bool` is `None` for every other kind, and `spec.kind` is
                // `Bool` here, so the `unwrap_or` arm is unreachable — and the
                // `!` is a boolean operation on a value the schema says is one.
                let default = spec.default.as_bool().unwrap_or(false);
                LiveValue::Bool(!default)
            }
            ParamKind::Int => LiveValue::Int(int_probe(spec)),
            ParamKind::Float => LiveValue::Float(float_probe(spec)),
            ParamKind::Text => LiveValue::Text("kettle"),
            ParamKind::Enum => {
                let raw = spec.default.as_enum().unwrap_or(0);
                // 0 -> 1 and anything else -> 0, which is "the other variant"
                // for every two-or-more-variant enum in `cc-domain`.
                LiveValue::Enum(i8::from(raw == 0))
            }
        }
    }

    /// An in-range `i32` that is not the default.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the mid-point of the schema's own bounds, and every Int \
                  parameter in this firmware is a count or a port number"
    )]
    fn int_probe(spec: &ParamSpec) -> i32 {
        let min = spec.min.unwrap_or(f64::from(i32::MIN));
        let max = spec.max.unwrap_or(f64::from(i32::MAX));
        let mid = ((min + max) / 2.0).clamp(f64::from(i32::MIN), f64::from(i32::MAX));
        let mid = mid as i32;
        if Some(mid) == spec.default.as_int() {
            mid.saturating_add(1)
        } else {
            mid
        }
    }

    /// An in-range `f64` that is not the default.
    #[allow(
        clippy::cast_precision_loss,
        reason = "the mid-point of the schema's own bounds; a float parameter \
                  whose bounds are both above 2^53 does not exist here"
    )]
    fn float_probe(spec: &ParamSpec) -> f64 {
        let mid = (spec.min.unwrap_or(0.0) + spec.max.unwrap_or(1.0)) / 2.0;
        if Some(mid) == spec.default.as_f64() {
            mid + 1.0
        } else {
            mid
        }
    }

    /// The probe is in range for every one of the 98 and is a value the field
    /// does not already hold, so a failure in
    /// `every_schema_key_round_trips_through_set_and_live_value` is a table bug
    /// and not a bad probe. Kept as its own test so diagnosis is one line.
    #[test]
    fn the_probe_is_in_range_and_new_for_every_registered_parameter() {
        let defaults = Config::default();
        for spec in SCHEMA {
            let probe = probe_for(spec);
            assert!(
                spec.accepts(ParamValue::from(probe)),
                "{} has no in-range probe",
                spec.key
            );
            assert_ne!(
                live_value(&defaults, spec.key),
                Some(probe),
                "{} already holds its probe, so the round trip would prove nothing",
                spec.key
            );
        }
    }

    /// The enumeration probe has to name a variant of *that* enum, and
    /// `the_probe_is_in_range_and_new_for_every_registered_parameter` would
    /// report the failure as a missing `set` arm rather than as a bad probe.
    #[test]
    fn the_probe_names_a_variant_for_every_enumeration() {
        for spec in SCHEMA {
            let ParamKind::Enum = spec.kind else {
                continue;
            };
            let LiveValue::Enum(raw) = probe_for(spec) else {
                panic!("{} produced a non-enum probe", spec.key);
            };
            assert!(
                crate::json::enum_discriminants_known(spec.key, raw),
                "{}: discriminant {raw} names no variant",
                spec.key
            );
        }
    }
}
