//! The typed configuration schema, the configuration value, the store trait,
//! and JSON import/export.
//!
//! # Why configuration is a value
//!
//! [`Config`] is a plain struct of typed fields which implements
//! `Serialize`/`Deserialize`, and [`BlobConfigStore`] moves whole values. Every
//! other decision here follows from that: with no global mutable state a test
//! needs no fixture reset, and a failed write cannot leave a half-updated
//! machine.
//!
//! The rejected alternative is one global mutable object per parameter, each
//! writing itself to NVS from inside its own setter. It fails in two ways that
//! show up as defects rather than as design taste: a per-parameter write opens
//! and closes the NVS namespace once per value, and a parameter that is not in
//! the store's list cannot be persisted at all, so it silently resets on every
//! reboot. That second failure is why `safety.emergency_temp` and
//! `safety.emergency_hysteresis` are registered here (see the table below),
//! *and* why a registration is the thing the schema test pins rather than the
//! presence of the field.
//!
//! # What is deliberately the same
//!
//! * **The keys.** The dotted names, verbatim, because an operator's existing
//!   `config.json` has to keep resolving. See [`schema`].
//! * **The defaults and ranges.** [`Config::default`] sets the factory values
//!   and [`schema`] carries the range each parameter accepts.
//! * **The JSON shape.** Nested objects, integers for enums, so
//!   `docs/example_config.json` imports unchanged.
//! * **The secret fields.** Four parameters are plaintext in NVS. They are
//!   plaintext here too — the machine has to be able to *use* them — but wrapped
//!   in [`Secret`] so that no log line, `Debug` dump or panic message can print
//!   one. The type itself lives in `cc-domain` ([`cc_domain::secret`]) because
//!   the UART provisioning parser has a fifth credential that is not part of a
//!   `Config` at all.
//!
//! # What is deliberately different
//!
//! | # | Change | Why |
//! | --- | --- | --- |
//! | 1 | `safety.emergency_temp` and `safety.emergency_hysteresis` are registered | a parameter that is not registered cannot be persisted, so it resets on every reboot |
//! | 2 | One JSON blob instead of one NVS key per parameter | atomic, readable, and 1/98th of the NVS traffic. [`blob_store`] |
//! | 3 | A bad value rejects the whole import | a warn-and-continue import answers `success` and leaves the machine on defaults |
//! | 4 | Text parameters have one storage-length bound | one bound the writer enforces beats eight constants nothing checks. See [`json`]. |
//! | 5 | Credentials redact in `Debug`/`Display` | a credential in a log line or a panic message is a disclosure with no local symptom |
//! | 6 | An NVS written by another firmware is ignored, not migrated — but a boot that finds one says so | §32 of `docs/history/divergences.md`; see [`predecessor`] |
//!
//! Cross-parameter safety validation lives in `cc-safety`
//! (`validate_config` / `load_or_default`), not here, because `cc-config` may
//! not depend on it. [`Config::safety_view`] is the hand-off.
//!
//! # Dependencies
//!
//! `serde` + `serde_json` (with `alloc`) and `cc-domain`. `serde_json` is
//! `alloc`-based and the configuration is a single serialised blob read once at
//! boot, so the crate is `no_std + alloc` rather than `no_std`; the measurement
//! that sizes it is `docs/example_config.json` — 1814 B in compact JSON, against
//! the 20 KB NVS partition and [`MAX_CONFIG_BYTES`]. `heapless` is **not** used:
//! its only natural home here would be the dotted key type, and a `&'static str`
//! in a `const` schema plus a borrowed key for JSON lookup is both smaller and
//! simpler than a `heapless::String<64>`. The four credential fields stay
//! `alloc::string::String` because a credential has no length the firmware can
//! bound: a fixed-capacity type would reject configurations the running firmware
//! accepts.

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
