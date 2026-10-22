//! Typed configuration, schema validation, atomic storage, and JSON exchange.
//!
//! [`Config`] is a value; [`BlobConfigStore`] persists it as one blob to avoid
//! partial updates. [`schema`] controls persistence as well as validation: a field
//! missing from the registry cannot survive a reboot.
//!
//! Credentials remain plaintext in storage and JSON, but [`Secret`] redacts logs.
//! Other firmware's NVS namespace is ignored; [`predecessor`] selects the boot warning.
//! Cross-parameter safety validation belongs to `cc-safety`, through
//! [`Config::safety_view`], to keep dependencies one-way.
//!
//! The crate uses `no_std + alloc`: JSON and credential strings need owned storage;
//! schema keys are static strings, so fixed-capacity key buffers would add overhead.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

use cc_domain::hardware::{
    OledAddress, OledType, RelayTriggerType, ScaleType, SwitchMode, SwitchType,
    TemperatureSensorType,
};
use cc_domain::process::BrewMode;
use cc_domain::system::{DisplayTemplate, Language, LogLevel};

pub mod assign;
pub mod blob_store;
pub mod config;
pub mod discovery;
pub mod form;
pub mod json;
pub mod predecessor;
pub mod schema;
pub mod secret;
pub mod store;

pub use assign::{parse as parse_parameter, Applied, AssignError};
pub use blob_store::{BlobBackend, BlobConfigStore, KEY as NVS_KEY, NAMESPACE as NVS_NAMESPACE};
pub use config::Config;
pub use json::{
    document_pairs, json_export, json_import, live_value, values_for, ImportError, LiveValue,
    MAX_CONFIG_BYTES,
};
pub use predecessor::{PredecessorProbe, CPP_NAMESPACE};
pub use schema::{ParamKind, ParamSpec, ParamValue, SCHEMA};
pub use secret::Secret;
pub use store::StoreError;

/// An enumeration that travels over the wire as its integer discriminant.
///
/// Integers, not variant names, because the wire shape is a number — see
/// `"mode"` in `docs/example_config.json`, the file the import test parses. A name
/// on the wire would make the format depend on the Rust variant spelling, and it
/// would also stop a hand-edited `"mode": 7` from being *rejected* rather than
/// silently mapped onto a variant.
///
/// This trait lives here rather than in `cc-domain` because `cc-domain` has no
/// dependencies at all and must not acquire `serde` for one trait.
pub trait IntEnum: Copy {
    /// The discriminant on the wire.
    fn to_raw(self) -> i8;

    /// Recover a variant from its discriminant, or `None` if there is no such
    /// variant.
    fn from_raw(raw: i8) -> Option<Self>;
}

/// `serde` adapter that reads and writes an [`IntEnum`] as an `i8`.
///
/// Applied per field with `#[serde(with = "crate::as_int")]`, which keeps
/// `cc-domain` free of `serde` while leaving the `Config` field types as the
/// real Rust enums — so `hardware.relays.heater.trigger_type` is a
/// `RelayTriggerType` in Rust code, and an integer only on the wire.
pub mod as_int {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::IntEnum;

    /// Write the discriminant.
    ///
    /// # Errors
    ///
    /// Propagates any serializer error.
    pub fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: IntEnum,
    {
        serializer.serialize_i8(value.to_raw())
    }

    /// Read the discriminant, rejecting one that names no variant.
    ///
    /// # Errors
    ///
    /// Returns a serde error if the value is not an integer, or is an integer
    /// that no variant of the enum has.
    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: IntEnum,
    {
        let raw = i8::deserialize(deserializer)?;
        T::from_raw(raw).ok_or_else(|| serde::de::Error::custom("no such enum value"))
    }
}

macro_rules! impl_int_enum {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntEnum for $ty {
                fn to_raw(self) -> i8 {
                    // The enums are all `#[repr(i8)]` with contiguous
                    // discriminants, so this is the discriminant the wire format
                    // carries.
                    self as i8
                }

                fn from_raw(raw: i8) -> Option<Self> {
                    <$ty>::from_raw(raw)
                }
            }
        )*
    };
}

impl_int_enum!(
    BrewMode,
    DisplayTemplate,
    Language,
    LogLevel,
    OledType,
    OledAddress,
    RelayTriggerType,
    ScaleType,
    SwitchMode,
    SwitchType,
    TemperatureSensorType,
);
