//! Where the configuration lives.
//!
//! # One blob, not 98 keys
//!
//! The C++ writes each parameter separately into the `config` NVS namespace
//! under an FNV-1a-hashed key (`Config.h:318-332, 487-501`): `"p" + 8 hex
//! digits` of `fnv1a(key)`. That has three consequences worth naming:
//!
//! * A partial write is possible. `saveAll()` writes 98 keys one at a time
//!   (`Config.cpp:154-171`), so a power cut mid-way leaves a configuration
//!   where some parameters are new and the rest are old. For a machine that
//!   heats to 150 °C, "some new" is a configuration nobody ever chose.
//! * The keys are unreadable. `p1a2b3c4d` tells you nothing, so a field
//!   diagnosis of a customer's NVS is impossible without this source tree.
//! * The hashes are not stable against reordering. They are, actually, but they
//!   are *opaque*, and a collision would silently alias two parameters.
//!
//! The oracle firmware stored **one nested JSON blob** in a single namespace,
//! 2071 bytes (08 §3, 08 §5.3), and 08 §6 recommends it. This port does the
//! same: [`BlobConfigStore`](crate::BlobConfigStore) moves one
//! [`Config`](crate::Config) value. Atomic, readable, and one NVS entry instead
//! of 98.
//!
//! # Wiring the fail-closed rule
//!
//! [`BlobConfigStore`](crate::BlobConfigStore) deliberately does **not** know
//! about `cc-safety`, because `cc-config` may not depend on it (04 §6). The
//! fail-closed rule from 08 §4.1 — discard a stored configuration that is
//! unsafe to run — is therefore applied by the caller, in three lines:
//!
//! ```text
//! let Some(stored) = store.load()? else { return Ok(Config::default()) };
//! let safety = cc_safety::SafetyConfig::from(stored.safety_view());
//! match cc_safety::load_or_default(Some(&safety)) {
//!     cc_safety::ConfigOrigin::Stored => Ok(stored),
//!     _ => Ok(Config::default()),   // discarded, or nothing stored
//! }
//! ```
//!
//! [`crate::Config::safety_view`] is the hand-off point. This module holds only
//! the error type; the store itself is [`blob_store`](crate::blob_store), which
//! is generic over its medium so that the format is host-testable without an
//! ESP32.

/// Why a store operation failed.
///
/// Deliberately a small closed set with no payload: an `NVS` error code is a
/// number the firmware logs, not something a caller can branch on usefully, and
/// carrying a `String` per failure path on a 320 KB heap is not worth it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// The backing store could not be opened at all — the NVS partition is
    /// missing or encrypted with an unknown key.
    Unavailable,
    /// Something is stored but it could not be decoded: a truncated write, a
    /// blob from a future firmware, or bit rot.
    ///
    /// A `Config` that fails to deserialise is treated exactly like one that
    /// fails safety validation: discarded, defaults used. Half a configuration
    /// is not a safer configuration than none.
    Corrupt,
    /// The store is read-only. Writing to it is a programming error, not a
    /// runtime condition to recover from.
    ReadOnly,
    /// The write did not complete.
    WriteFailed,
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let text = match self {
            Self::Unavailable => "configuration store unavailable",
            Self::Corrupt => "stored configuration is corrupt",
            Self::ReadOnly => "configuration store is read-only",
            Self::WriteFailed => "failed to write the configuration",
        };
        f.write_str(text)
    }
}
