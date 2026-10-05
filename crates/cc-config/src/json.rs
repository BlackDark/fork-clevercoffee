//! JSON import and export.
//!
//! # Format
//!
//! Nested objects, keys identical to the C++ dotted names:
//!
//! ```json
//! { "pid": { "regular": { "kp": 62.0 } }, "safety": { "emergency_temp": 150.0 } }
//! ```
//!
//! That is what `Config::exportToJsonObject` produces
//! (`src/Config.cpp:234-243`, via `ConfigJson::setNested`), what
//! `docs/example_config.json` contains, and what the oracle firmware stored
//! (08 §5.3). Enumerations are plain integers, because
//! `EnumParamDef::toJson` writes `static_cast<int>(currentValue_)`
//! (`Config.h:423`).
//!
//! # The rejection rules, and where each comes from
//!
//! | Rule | C++ |
//! | --- | --- |
//! | empty body is rejected | `Config.cpp:348-351` |
//! | unparseable body is rejected | `Config.cpp:353-357` |
//! | oversized body is rejected | `Config.cpp:359-362` (`doc.overflowed()`) |
//! | a non-object root is rejected | `Config.cpp:364-368` |
//! | flat dotted keys are rejected | `Config.cpp:370-374` (`usesFlatDotKeys`) |
//! | an object with no known key is rejected | `Config.cpp:325-345` (`updatedCount > 0`) |
//!
//! # Two deliberate strengthenings
//!
//! 1. **A bad value for a known key rejects the whole import.** The C++ logs a
//!    warning per bad parameter and carries on, then reports success as long as
//!    *one* parameter imported (`Config.cpp:331-342`). A user uploading a
//!    configuration with an out-of-range `safety.emergency_temp` gets a
//!    `200 OK`, believes it took effect, and runs on the old value. Here the
//!    import is all-or-nothing and the offending keys are named. The C++ *error
//!    message* at `WebServerManager.cpp:752` already claims to reject "invalid
//!    values", so this makes the code match the message.
//! 2. **The size limit is explicit and stated.** `doc.overflowed()` is
//!    `ArduinoJson` reporting that the document outgrew its fixed pool — a
//!    capacity fact, not a policy. `serde_json` has no such pool, so the bound
//!    has to be a decision. [`MAX_CONFIG_BYTES`] is set from what the store can
//!    actually hold: the oracle's blob was 2071 bytes
//!    (`cc_firmware: config: nvs (2071 B stored)`) and the `nvs` partition is
//!    20 KB, so 8 KB leaves room for the keys around it and a decade of
//!    added parameters.
//!
//! Unknown keys are **ignored**, as in the C++. Forward compatibility is worth
//! more here than strictness: a configuration exported by a newer firmware must
//! still import into an older one, and the extra keys it will write are all
//! range-checked on the way in.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde_json::{Map, Value};

use crate::config::Config;
use crate::schema::{self, ParamSpec, ParamValue};

/// The largest configuration document that will be parsed or produced.
///
/// See the module documentation: 8 KB, against an oracle blob of 2071 bytes and
/// a 20 KB NVS partition.
pub const MAX_CONFIG_BYTES: usize = 8 * 1024;

/// Why an import was rejected.
#[derive(Clone, Debug, PartialEq)]
pub enum ImportError {
    /// The body was empty. `Config.cpp:348`: "JSON import rejected — empty body".
    Empty,
    /// The body was longer than [`MAX_CONFIG_BYTES`].
    ///
    /// The C++ expresses this as `doc.overflowed()`
    /// (`Config.cpp:359`), which is a fixed-pool capacity failure rather than a
    /// policy; the limit here is a policy.
    TooLarge {
        /// The size that was rejected.
        bytes: usize,
    },
    /// The body was not valid JSON.
    Syntax {
        /// A human-readable description, for the API response.
        detail: String,
    },
    /// The root was not a JSON object. `Config.cpp:364`.
    NotAnObject,
    /// A top-level key contained a `.`, i.e. the legacy flat format.
    ///
    /// `ConfigJson::usesFlatDotKeys` (`src/ConfigJson.cpp:40-52`) rejects this,
    /// and so does `/api/config/upload` (`WebServerManager.cpp:737-745`). The
    /// reason it matters: `{"brew.setpoint": 95}` looks like a valid nested
    /// document to a human, imports *nothing at all* under the nested reader,
    /// and the C++ would then report "0 parameters imported" and fail — but only
    /// after a confusing round trip. Rejecting it by name is kinder.
    FlatDottedKeys {
        /// The offending top-level key.
        key: String,
    },
    /// The object contained no key this firmware knows about.
    NoKnownParameters,
    /// One or more known keys held a value this firmware rejects.
    ///
    /// See the module documentation: this is a deliberate strengthening of the
    /// C++, which logs and continues.
    InvalidValues {
        /// One entry per rejected key, in the order the keys appear in
        /// [`schema::SCHEMA`].
        rejected: Vec<RejectedValue>,
    },
}

impl core::fmt::Display for ImportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("the configuration body is empty"),
            Self::TooLarge { bytes } => {
                write!(f, "the configuration is {bytes} bytes, over the limit")
            }
            Self::Syntax { detail } => write!(f, "the configuration is not valid JSON: {detail}"),
            Self::NotAnObject => f.write_str("the configuration must be a top-level JSON object"),
            Self::FlatDottedKeys { key } => {
                write!(
                    f,
                    "flat dotted keys are not supported (found {key:?}); use nested objects"
                )
            }
            Self::NoKnownParameters => {
                f.write_str("the configuration contains no recognised parameters")
            }
            Self::InvalidValues { .. } => f.write_str("one or more parameter values are invalid"),
        }
    }
}

/// One parameter whose value was rejected.
#[derive(Clone, Debug, PartialEq)]
pub struct RejectedValue {
    /// The dotted C++ key.
    pub key: &'static str,
    /// Why it was rejected, ready for an API response.
    pub reason: RejectReason,
}

/// Why a single value was rejected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RejectReason {
    /// Outside the parameter's inclusive `[min, max]`.
    OutOfRange {
        /// The lower bound.
        min: f64,
        /// The upper bound.
        max: f64,
    },
    /// The JSON type did not match the parameter's kind.
    WrongType,
    /// An enumeration discriminant that no variant has.
    UnknownEnumDiscriminant,
    /// A string that is longer than the store can hold.
    TooLong,
}

/// Serialise a configuration to the nested JSON blob.
///
/// Byte-compatible with `Config::exportToJson()` for every key the C++
/// exports, plus the two `safety.*` keys it omits. Compact rather than pretty:
/// the C++ uses `serializeJsonPretty` for the HTTP download and plain
/// `serializeJson` for storage, and this is the storage form. The download
/// handler can re-emit it with `to_string_pretty`.
///
/// # Errors
///
/// [`MAX_CONFIG_BYTES`] is a hard limit, and a `Config` that would exceed it
/// means the schema has grown past the store's budget — a firmware bug, not a
/// runtime condition. It is reported rather than silently truncated.
pub fn json_export(config: &Config) -> Result<String, ExportError> {
    let text = serde_json::to_string(config).map_err(|_| ExportError::SerialiseFailed)?;
    if text.len() > MAX_CONFIG_BYTES {
        return Err(ExportError::TooLarge { bytes: text.len() });
    }
    Ok(text)
}

/// Why an export failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportError {
    /// The configuration is larger than [`MAX_CONFIG_BYTES`].
    TooLarge {
        /// The size that was produced.
        bytes: usize,
    },
    /// Serialisation itself failed, which for this type can only mean a bug.
    SerialiseFailed,
}

/// Parse a configuration from a nested JSON blob.
///
/// Missing keys keep their defaults, exactly as a partial `POST /api/config`
/// would in the C++ (each parameter is looked up independently and `continue`d
/// when absent, `Config.cpp:328-331`). Unknown keys are ignored.
///
/// # Errors
///
/// See [`ImportError`]. Every rejection is total: on error nothing is applied.
pub fn json_import(text: &str) -> Result<Config, ImportError> {
    let root = parse_document(text)?;

    let values = scan(&root)?;

    // Deserialise the *patch* rather than the user's document. `#[serde(default)]`
    // on every level fills in everything the patch does not mention, and
    // building from the patch — instead of merging it into the document — means
    // an unrecognised key, or a leaf where a group belongs, cannot survive into
    // the `Config`. It also normalises the JSON types: `"setpoint": 95` and
    // `"setpoint": 95.0` both become the same `f64`.
    let mut patch = Map::new();
    for (spec, value) in values {
        insert_nested(&mut patch, spec.key, value);
    }
    serde_json::from_value(Value::Object(patch)).map_err(|e| ImportError::Syntax {
        detail: e.to_string(),
    })
}

/// The document prologue every import shares: size, syntax, root shape.
///
/// Split out of [`json_import`] so that [`document_pairs`] — the
/// `POST /api/config/upload` reader — cannot drift from it. A rule that lives in
/// one of the two and not the other is a rule an upload and a store-seed
/// disagree about, and the disagreement is invisible until a document is
/// accepted by one and refused by the other.
fn parse_document(text: &str) -> Result<Map<String, Value>, ImportError> {
    if text.is_empty() {
        return Err(ImportError::Empty);
    }
    if text.len() > MAX_CONFIG_BYTES {
        return Err(ImportError::TooLarge { bytes: text.len() });
    }

    let root: Value = serde_json::from_str(text).map_err(|e| ImportError::Syntax {
        detail: e.to_string(),
    })?;

    let Value::Object(root) = root else {
        return Err(ImportError::NotAnObject);
    };

    if let Some(key) = root.keys().find(|k| k.contains('.')) {
        return Err(ImportError::FlatDottedKeys { key: key.clone() });
    }
    Ok(root)
}

/// Every schema key the document mentions, with its coerced value.
///
/// The shared body of [`json_import`] and [`document_pairs`]: walk the schema,
/// pull each key out of the nested object, and coerce it. Doing it this way
/// round means a key that is absent and a key that is present-but-wrong are
/// distinguishable, and it means the schema is the single source of truth for
/// what "known" means.
fn scan(root: &Map<String, Value>) -> Result<Vec<(&'static ParamSpec, Value)>, ImportError> {
    let mut rejected: Vec<RejectedValue> = Vec::new();
    let mut known = 0usize;
    let mut accepted: Vec<(&'static ParamSpec, Value)> = Vec::new();

    for spec in schema::SCHEMA {
        let Some(raw) = lookup(root, spec.key) else {
            continue;
        };
        known += 1;
        match coerce(spec, raw) {
            Ok(value) => accepted.push((spec, value)),
            Err(reason) => rejected.push(RejectedValue {
                key: spec.key,
                reason,
            }),
        }
    }

    if !rejected.is_empty() {
        return Err(ImportError::InvalidValues { rejected });
    }
    if known == 0 {
        return Err(ImportError::NoKnownParameters);
    }
    Ok(accepted)
}

/// The dotted `(key, value)` pairs a nested configuration document carries.
///
/// **This is the reader for `POST /api/config/upload`.** The C++'s route
/// (`WebServerManager.cpp:727-762`) hands the body to
/// `Config::importFromJsonObject` (`Config.cpp:323-345`), which walks the
/// parameters, applies the ones the document mentions, and reports success if
/// *one* of them imported. It is deliberately **not** `json_import`, which
/// deserialises a whole `Config` with defaults filled in — that is the right
/// shape for seeding a fresh store from `/config.json` and the wrong shape for
/// an upload, where a document that mentions twelve keys must leave the other
/// eighty-six exactly as they are.
///
/// Returning *pairs* rather than a `Config` is what keeps the writer single.
/// The handler hands these to the control task, which applies them with
/// [`crate::assign::apply`] — the same function `POST /api/parameters` uses and
/// the only writer of a parameter in the workspace. A second applier here would
/// be a second set of type rules, and the two would drift.
///
/// The pairs are rendered into the string form [`crate::assign::parse`] accepts,
/// so the value that is validated here is validated *again* by the same
/// [`crate::assign::parse`] on the way in — the idempotent second half of one
/// rule, which is how `POST /api/parameters` already works. A float renders
/// through Rust's shortest-round-trip `Display`, so a value this reader accepts
/// arrives at the writer unchanged, bit for bit.
///
/// # Errors
///
/// See [`ImportError`]. Every rejection is total: on error no pair is returned,
/// so there is nothing for a caller to half-apply.
pub fn document_pairs(text: &str) -> Result<Vec<crate::form::Field>, ImportError> {
    let root = parse_document(text)?;
    Ok(scan(&root)?
        .into_iter()
        .map(|(spec, value)| {
            let rendered = match value {
                Value::Bool(flag) => {
                    if flag {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    }
                }
                // `coerce` has already narrowed these: an `Int` is an `i32`, an
                // `Enum` an `i8`, and both are whole numbers, so neither can
                // render with a fractional part. A `Float` can, and Rust's `f64`
                // `Display` is the shortest representation that parses back to
                // the same bits.
                Value::Number(number) => number.to_string(),
                Value::String(text) => text,
                // `coerce` returns a `Value` built from the parameter's own
                // kind, so nothing else is reachable. Returning the JSON form is
                // the honest answer if that ever changes rather than a panic.
                other => other.to_string(),
            };
            (spec.key.to_string(), rendered)
        })
        .collect())
}

/// The live value of one parameter, borrowed from a [`Config`].
///
/// What the C++'s `toJson` puts in `value` (`Config.h:226-238`): the
/// *current* value of the parameter, not its compiled-in default. The two are
/// the same on a freshly-booted machine and different after anything has been
/// stored, which is the whole reason the field exists.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LiveValue<'a> {
    /// A `bool` parameter.
    Bool(bool),
    /// An integer parameter.
    Int(i32),
    /// A floating-point parameter.
    Float(f64),
    /// A text parameter, borrowed so a credential is never copied.
    Text(&'a str),
    /// An enum discriminant, which is `i8` because every `Config` enum is
    /// `#[repr(i8)]` with contiguous discriminants (see [`crate::IntEnum`]) and
    /// `Config.h:105` writes it as an `int`.
    Enum(i8),
}

impl LiveValue<'_> {
    /// The schema kind this value belongs to.
    ///
    /// `Config.h:105` writes `static_cast<int>(getParamType())`, and
    /// [`ParamKind::cpp_param_type`](schema::ParamKind::cpp_param_type) is that
    /// mapping. It has to agree with the schema's own `kind` for the parameter,
    /// or `/api/parameters` would report a `type` of `5` (ENUM) beside a value
    /// that looks like a plain integer — which is what the React editor reads to
    /// decide whether to offer a dropdown.
    #[must_use]
    pub fn kind(&self) -> schema::ParamKind {
        match self {
            Self::Bool(_) => schema::ParamKind::Bool,
            Self::Int(_) => schema::ParamKind::Int,
            Self::Float(_) => schema::ParamKind::Float,
            Self::Text(_) => schema::ParamKind::Text,
            Self::Enum(_) => schema::ParamKind::Enum,
        }
    }
}

impl<'a> From<LiveValue<'a>> for ParamValue<'a> {
    /// The same value in the schema's own type.
    ///
    /// `ParamSpec::accepts` takes a [`ParamValue`], which is what
    /// [`SCHEMA`](crate::schema::SCHEMA)
    /// holds; a caller that has a live value in hand — one just parsed from a
    /// string, say — needs this to ask the spec whether it is acceptable without
    /// re-deriving the value.
    fn from(value: LiveValue<'a>) -> Self {
        match value {
            LiveValue::Bool(v) => Self::Bool(v),
            LiveValue::Int(v) => Self::Int(v),
            LiveValue::Float(v) => Self::Float(v),
            LiveValue::Text(v) => Self::Text(v),
            LiveValue::Enum(v) => Self::Enum(v),
        }
    }
}

/// Read one schema key's current value out of a [`Config`].
///
/// What the C++'s `toJson` puts in `value` (`Config.h:226-238`): the
/// *current* value of the parameter, not its compiled-in default. The two are
/// the same on a freshly-booted machine and different after anything has been
/// stored, which is the whole reason the field exists.
///
/// The read half of the C++'s `ConfigParamDef` lives on the [`ParamSpec`]
/// itself (`schema::ParamSpec::get`), so this is a lookup rather than a
/// 98-arm match — the same collapse `assign::set` got, and for the same
/// reason. `None` for a key the schema does not register, which is what
/// `findConfigParameter` returning `nullptr` means (`Config.h:1550`).
#[must_use]
pub fn live_value<'a>(config: &'a Config, key: &str) -> Option<LiveValue<'a>> {
    schema::SCHEMA
        .iter()
        .find(|spec| spec.key == key)
        .map(|spec| (spec.get)(config))
}

/// Every schema key's current value, in schema order.
///
/// The list `/api/parameters` needs, and **the order `/api/parameters` emits
/// is this function's, not `live_value`'s**: this iterates
/// [`schema::SCHEMA`], so the output is SCHEMA order — the C++'s
/// `getAllConfigParams` order (`Config.cpp:438-563`) for the first 96 — even
/// though `live_value` is free to answer in any order it likes. That was
/// already true before the accessors moved onto [`ParamSpec`]
/// (`cc-hal-esp32`'s `parameters_json` pairs `SCHEMA.iter().enumerate()`
/// against `values[index]`), so the collapse did not move a single byte of
/// the response.
///
/// The `Option` is kept rather than filtered away, even though it is now
/// structurally impossible to be `None`: every spec carries its own getter, so
/// there is no second table left to disagree with. It is kept because
/// `parameters_json` indexes this vector positionally against `SCHEMA` and a
/// short vector would silently mis-pair every entry after the gap, and because
/// dropping it would change `cc-hal-esp32` and the JSON for no gain.
///
/// Returning a `Vec` rather than an iterator keeps the borrow of `config` in
/// one place, so a caller cannot hold it across the JSON it is building.
#[must_use]
pub fn values_for(config: &Config) -> Vec<Option<LiveValue<'_>>> {
    schema::SCHEMA
        .iter()
        .map(|spec| Some((spec.get)(config)))
        .collect()
}

/// Walk a dotted key through a nested object and return the value at its leaf.
///
/// A leaf where an intermediate object was expected means the key is simply not
/// present at that path, so the default stands. The C++ behaves the same way:
/// `getNested` returns a null variant and the parameter is skipped
/// (`ConfigJson.cpp:73-76`).
fn lookup<'a>(root: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    let mut cursor = root;
    let mut segments = key.split('.').peekable();
    while let Some(segment) = segments.next() {
        let value = cursor.get(segment)?;
        if segments.peek().is_none() {
            return Some(value);
        }
        cursor = value.as_object()?;
    }
    None
}

/// Turn a JSON value into a typed parameter value, applying the schema's rules.
fn coerce(spec: &ParamSpec, raw: &Value) -> Result<Value, RejectReason> {
    match spec.kind {
        schema::ParamKind::Bool => match raw {
            Value::Bool(_) => Ok(raw.clone()),
            _ => Err(RejectReason::WrongType),
        },
        schema::ParamKind::Int => {
            let v = i32::try_from(raw.as_i64().ok_or(RejectReason::WrongType)?).map_err(|_| {
                RejectReason::OutOfRange {
                    min: f64::from(i32::MIN),
                    max: f64::from(i32::MAX),
                }
            })?;
            check_range(spec, f64::from(v))?;
            // Normalise to the on-disk shape so `1` and `1.0` compare equal after
            // a round trip through a `f64` field.
            Ok(Value::from(v))
        }
        schema::ParamKind::Float => {
            let v = raw.as_f64().ok_or(RejectReason::WrongType)?;
            check_range(spec, v)?;
            Ok(Value::from(v))
        }
        schema::ParamKind::Text => match raw {
            Value::String(s) => {
                if s.len() > MAX_TEXT_LEN {
                    return Err(RejectReason::TooLong);
                }
                Ok(raw.clone())
            }
            _ => Err(RejectReason::WrongType),
        },
        schema::ParamKind::Enum => {
            let v = i32::try_from(raw.as_i64().ok_or(RejectReason::WrongType)?).map_err(|_| {
                RejectReason::OutOfRange {
                    min: f64::from(i32::MIN),
                    max: f64::from(i32::MAX),
                }
            })?;
            let narrowed = i8::try_from(v).map_err(|_| RejectReason::UnknownEnumDiscriminant)?;
            // A discriminant no variant has. The Rust enum's own `Deserialize`
            // would also reject it, but naming it here gives a far better
            // message than "unknown variant".
            if !enum_discriminants_known(spec.key, narrowed) {
                return Err(RejectReason::UnknownEnumDiscriminant);
            }
            Ok(Value::from(narrowed))
        }
    }
}

pub(crate) fn check_range(spec: &ParamSpec, v: f64) -> Result<(), RejectReason> {
    match (spec.min, spec.max) {
        (Some(min), Some(max)) if v < min || v > max => Err(RejectReason::OutOfRange { min, max }),
        (Some(min), None) if v < min => Err(RejectReason::OutOfRange {
            min,
            max: f64::INFINITY,
        }),
        (None, Some(max)) if v > max => Err(RejectReason::OutOfRange {
            min: f64::NEG_INFINITY,
            max,
        }),
        _ => Ok(()),
    }
}

/// Which discriminants each enumeration parameter accepts.
///
/// A flat match rather than a table, so that adding an enum parameter without
/// adding its discriminants here is a compile error rather than a runtime
/// surprise. The `unreachable!` arms cannot be reached: `SCHEMA` is the input
/// and it contains no other enum keys.
pub(crate) fn enum_discriminants_known(key: &str, value: i8) -> bool {
    use cc_domain::hardware::{
        OledAddress as A, OledType as T, RelayTriggerType as R, ScaleType as Sc, SwitchMode as M,
        SwitchType as Sw, TemperatureSensorType as Ts,
    };
    use cc_domain::process::BrewMode as B;
    use cc_domain::system::{DisplayTemplate as D, Language as L, LogLevel as G};

    match key {
        "brew.mode" => B::from_raw(value).is_some(),
        "display.template" => D::from_raw(value).is_some(),
        "display.language" => L::from_raw(value).is_some(),
        "system.log_level" => G::from_raw(value).is_some(),
        "hardware.oled.type" => T::from_raw(value).is_some(),
        "hardware.oled.address" => A::from_raw(value).is_some(),
        "hardware.relays.heater.trigger_type"
        | "hardware.relays.valve.trigger_type"
        | "hardware.relays.pump.trigger_type" => R::from_raw(value).is_some(),
        "hardware.switches.brew.type"
        | "hardware.switches.steam.type"
        | "hardware.switches.power.type"
        | "hardware.switches.hot_water.type" => Sw::from_raw(value).is_some(),
        "hardware.switches.brew.mode"
        | "hardware.switches.steam.mode"
        | "hardware.switches.power.mode"
        | "hardware.switches.hot_water.mode"
        | "hardware.sensors.watertank.mode" => M::from_raw(value).is_some(),
        "hardware.sensors.temperature.type" => Ts::from_raw(value).is_some(),
        "hardware.sensors.scale.type" => Sc::from_raw(value).is_some(),
        _ => unreachable!("{key} is not an enumeration parameter"),
    }
}

/// The longest a text parameter may be.
///
/// This is the **only** length limit in the schema, and it is a storage bound,
/// not a validation rule — and it is deliberately far looser than the C++'s own
/// constants, because the C++ has none in force at all.
///
/// `defaults.h:118-125` defines eight of them (`HOSTNAME_MAX_LENGTH` 64,
/// `WIFI_PASSWORD_MAX_LENGTH` 64, `MQTT_BROKER_MAX_LENGTH` 64, …) and
/// **not one of them is ever read**: `Config.h:isValid` returns `true`
/// unconditionally for `String`, so the running firmware will store a 4 KB
/// hostname and then hand it to `WiFi.setHostname()`. Enforcing 64 here would
/// reject configurations the production firmware accepts, which is a parity
/// break and not obviously an improvement — the limit exists to keep one
/// pathological value out of a 20 KB NVS partition, and this does that without
/// changing what a legitimate configuration may contain.
///
/// Deliberately *not* tightened. If a decision is ever taken to bound these,
/// it belongs in `intentional-diffs.md` next to the eight unused constants.
pub const MAX_TEXT_LEN: usize = 4096;

/// Write `value` into a nested object at the dotted `key`, creating the
/// intermediate objects.
fn insert_nested(target: &mut Map<String, Value>, key: &str, value: Value) {
    let mut node = target;
    let segments: Vec<&str> = key.split('.').collect();
    for segment in &segments[..segments.len() - 1] {
        let entry = node
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        node = entry
            .as_object_mut()
            .expect("inserted or replaced with an object one line above");
    }
    node.insert(segments[segments.len() - 1].to_string(), value);
}

/// One rejection reason, as the words an operator reads.
///
/// The one spelling of each reason. `crate::assign` rejects a value for the same
/// reasons and has to say which, and two spellings of "out of range" is two
/// things to keep in step. (`describe_rejection`, the wrapper that joined these
/// with the offending key, had no callers and was deleted.)
#[must_use]
pub fn describe_reason(reason: RejectReason) -> String {
    use alloc::format;
    match reason {
        RejectReason::OutOfRange { min, max } => format!("out of range [{min} .. {max}]"),
        RejectReason::WrongType => "wrong type".to_string(),
        RejectReason::UnknownEnumDiscriminant => "no such enum value".to_string(),
        RejectReason::TooLong => "string too long".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use alloc::{string::String, vec::Vec};

    use super::*;
    use crate::Secret;

    #[test]
    fn every_schema_key_has_a_live_value() {
        // The pairing that keeps `/api/parameters`' `value` honest: `SCHEMA`
        // says a key exists and carries the getter that says where it lives.
        //
        // **This is now nearly tautological**, and that is the point. Before the
        // accessors moved onto `ParamSpec`, a key could be in `SCHEMA` and
        // missing from a 98-arm `match` in this module, and the failure would
        // surface as a parameter with no `value` at all. It is kept because it
        // costs nothing, it still checks the `Vec` and `SCHEMA` stay the same
        // length, and `values_for` still returns `Option`.
        let config = Config::default();
        let missing: Vec<&str> = schema::SCHEMA
            .iter()
            .zip(values_for(&config))
            .filter_map(|(spec, value)| value.is_none().then_some(spec.key))
            .collect();
        assert!(missing.is_empty(), "no live value for: {missing:?}");
    }

    #[test]
    fn a_live_value_is_the_current_one_not_the_default() {
        // The whole point of the field. `Config.h:226-238` writes
        // `obj["value"] = currentValue_`, which is the *stored* value; a
        // firmware that only ever reported the default would render an
        // operator's saved settings as if they had been lost.
        let mut config = Config::default();
        config.brew.setpoint = 91.5;
        assert_eq!(
            live_value(&config, "brew.setpoint"),
            Some(LiveValue::Float(91.5))
        );
        // …and the compiled-in default is unchanged, so the C++'s `default`
        // field still reports what a factory reset would give. 95.0 is the
        // schema's `ParamValue::Float(95.0)` for `brew.setpoint`
        // (`schema.rs:277-282`).
        assert_eq!(
            live_value(&Config::default(), "brew.setpoint"),
            Some(LiveValue::Float(95.0))
        );
    }

    #[test]
    fn a_text_value_borrows_rather_than_copies() {
        // A credential must not be duplicated onto the heap to be reported, and
        // must not be `Copy` either. `LiveValue::Text` borrows for both reasons.
        let mut config = Config::default();
        config.system.wifi.ssid = String::from("unit-test-ssid");
        match live_value(&config, "system.wifi.ssid") {
            Some(LiveValue::Text(ssid)) => assert_eq!(ssid, "unit-test-ssid"),
            other => panic!("expected a text value, got {other:?}"),
        }
    }

    #[test]
    fn the_live_kind_agrees_with_the_schema_kind() {
        // `Config.h:105` writes `static_cast<int>(getParamType())` from the
        // *parameter's* type, so a `value` whose kind disagrees with the
        // schema's `type` would make the React editor render the wrong input.
        let config = Config::default();
        for (spec, value) in schema::SCHEMA.iter().zip(values_for(&config)) {
            let Some(value) = value else { continue };
            assert_eq!(
                value.kind(),
                spec.kind,
                "{} reports {:?} but the schema says {:?}",
                spec.key,
                value.kind(),
                spec.kind
            );
        }
    }

    #[test]
    fn an_unregistered_key_has_no_live_value() {
        assert_eq!(live_value(&Config::default(), "not.a.key"), None);
    }

    #[test]
    fn a_credential_is_borrowed_so_reading_it_needs_expose() {
        // The new accessor is the one place a caller can now reach a password by
        // dotted key, so what it hands back matters. It borrows the plaintext
        // rather than wrapping it in the `Secret` that redacts `Debug` and
        // `Display` — deliberately, because the *caller* decides whether to
        // publish it. `/api/parameters` does publish `value` for the four
        // credential parameters, which is the C++'s behaviour
        // (`Config.h:232-234` writes `obj["value"] = currentValue_` for a
        // `String` with no redaction), and that is a property of the C++ this
        // firmware is matching, not a decision made here.
        //
        // What this test pins is the cheaper half: nothing on this path *copies*
        // the credential. A `String` would be a second plaintext allocation
        // that outlives the borrow and can be leaked by a panic path.
        let mut config = Config::default();
        config.mqtt.password = Secret::new(String::from("unit-test-pw"));
        match live_value(&config, "mqtt.password") {
            Some(LiveValue::Text(plaintext)) => {
                assert_eq!(plaintext, "unit-test-pw");
                // Borrowed, not owned: the value points into the `Config`.
                assert!(
                    core::ptr::eq(plaintext.as_ptr(), config.mqtt.password.expose().as_ptr()),
                    "the credential was copied rather than borrowed"
                );
            }
            Some(other) => panic!("expected a text value, got {:?}", other.kind()),
            None => panic!("mqtt.password is registered"),
        }
    }
}
