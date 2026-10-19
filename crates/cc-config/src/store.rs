//! Where the configuration lives.
//!
//! # One blob, not one key per parameter
//!
//! [`BlobConfigStore`](crate::BlobConfigStore) moves one
//! [`Config`](crate::Config) value in one NVS entry. A store that wrote each
//! parameter separately, under a hashed key, would fail in three ways:
//!
//! * A partial write is possible. Writing the parameters one at a time means a
//!   power cut mid-way leaves some parameters new and the rest old. For a machine
//!   that heats to 150 °C, "some new" is a configuration nobody ever chose.
//! * The keys are unreadable. A hashed key tells an operator nothing, so
//!   diagnosing a customer's NVS needs a key-derivation program.
//! * A hash collision would silently alias two parameters.
//!
//! # Wiring the fail-closed rule
//!
//! [`BlobConfigStore`](crate::BlobConfigStore) deliberately does **not** know
//! about `cc-safety`, because `cc-config` may not depend on it. Discarding a
//! stored configuration that is unsafe to run is therefore the caller's rule,
//! applied in three lines:
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
