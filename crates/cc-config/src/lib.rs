//! The typed configuration schema, the configuration value, the store trait,
//! and JSON import/export — task R2-06.
//!
//! # What this crate replaces
//!
//! The C++ has a `Config` **singleton** whose 98 `ParamDef` members are global
//! mutable objects that write themselves to NVS from inside their own setter
//! (`Config.h:170-186`). Every one of those 98 members is constructed before
//! `main()` runs, each holds an Arduino `String`, and each `set()` opens the
//! NVS namespace, writes one key, and closes it. The consequences are visible
//! throughout the C++: `Config::getInstance()` appears in test fixtures and in
//! `showCondition` lambdas, `loadAll` reports "loaded 41/96 parameters" on a
//! healthy machine, and the S1 emergency threshold cannot be persisted at all
//! because it is not in the list.
//!
//! Here configuration is a **value**. [`Config`] is a plain struct of typed
//! fields, `Config` implements `Serialize`/`Deserialize`, and
//! [`BlobConfigStore`] moves whole values. There is no global mutable state,
//! so a test needs no fixture reset, and a failed write cannot leave a
//! half-updated machine.
//!
//! # What is deliberately the same
//!
//! * **The keys.** The C++ dotted names, verbatim. See [`schema`].
//! * **The defaults and ranges.** Transcribed from `Config.h` and `defaults.h`.
//! * **The JSON shape.** Nested objects, integers for enums, so
//!   `docs/example_config.json` imports unchanged.
//! * **The secret fields.** Four parameters are plaintext in NVS in the C++ and
//!   in the oracle (08 §3: 2071 bytes, unencrypted). They are plaintext here
//!   too — the machine has to be able to *use* them — but wrapped in
//!   [`Secret`] so that no log line, `Debug` dump or panic message can print
//!   one. The type itself now lives in `cc-domain`, because the UART
//!   provisioning parser (R3-12) has a fifth credential that is not part of a
//!   `Config` at all.
//!
//! # What is deliberately different
//!
//! | # | Change | Why |
//! | --- | --- | --- |
//! | 1 | `safety.emergency_temp` and `safety.emergency_hysteresis` are registered | The C++ defines them, reads them, and never registers them, so they reset on every reboot. Finding 1 of 01 §10. |
//! | 2 | One JSON blob instead of 98 FNV-1a-hashed NVS keys | Atomic, readable, and 1/98th of the NVS traffic. 08 §6. [`blob_store`] |
//! | 3 | A bad value rejects the whole import | The C++ logs a warning and continues, then reports success. |
//! | 4 | Text parameters have one storage-length bound | The C++ has eight length constants and checks none of them. See [`json`]. |
//! | 5 | Credentials redact in `Debug`/`Display` | Skill rule 7. |
//! | 6 | A C++-written NVS is ignored, not migrated | Decided 2026-09-28. See [`blob_store`]. |
//!
//! Cross-parameter safety validation lives in `cc-safety`
//! (`validate_config` / `load_or_default`), not here, because `cc-config` may
//! not depend on it. [`Config::safety_view`] is the hand-off.
//!
//! # Dependencies
//!
//! `serde` + `serde_json` (with `alloc`) and `cc-domain`. `serde_json` is
//! `alloc`-based and the configuration is a single serialised blob read once at
//! boot, so the crate is `no_std + alloc` rather than `no_std`; the 20 KB NVS
//! partition and the 2071-byte oracle blob are the sizing evidence. `heapless`
//! is **not** used: its only natural home here would be the dotted key type, and
//! a `&'static str` in a `const` schema plus a borrowed key for JSON lookup is
//! both smaller and simpler than a `heapless::String<64>`. The four credential
//! fields stay `alloc::string::String` because the C++ accepts unbounded
//! strings and a fixed-capacity type would reject configurations the running
//! firmware accepts.

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
pub use schema::{ParamKind, ParamSpec, ParamValue, SCHEMA};
pub use secret::Secret;
pub use store::StoreError;

/// An enumeration that travels over the wire as its integer discriminant.
///
/// The C++ serialises every enum parameter as `static_cast<int>(value)`
/// (`Config.h:423`, `Config.h:212`), and `docs/example_config.json` shows
/// `"mode": 1`. Keeping integers means an exported configuration is byte-for-byte
/// comparable with one the C++ produced, and it means a hand-edited file that
/// says `"mode": 7` is *rejected* rather than silently mapped onto a variant.
///
/// This trait lives here rather than in `cc-domain` because `cc-domain` has no
/// dependencies at all (04 §1) and must not acquire `serde` for one trait.
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
/// real Rust enums — so `config.hardware.relays.heater.trigger_type` is a
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
                    // discriminants, so this is the wire value the C++ writes.
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
