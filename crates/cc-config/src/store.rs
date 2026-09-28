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
//! same: [`ConfigStore`] moves one [`Config`] value. Atomic, readable, and one
//! NVS entry instead of 98.
//!
//! # Wiring the fail-closed rule
//!
//! [`ConfigStore`] deliberately does **not** know about `cc-safety`, because
//! `cc-config` may not depend on it (04 §6). The fail-closed rule from
//! 08 §4.1 — discard a stored configuration that is unsafe to run — is
//! therefore applied by the caller, in three lines:
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
//! [`crate::Config::safety_view`] is the hand-off point. The `NvsStore`
//! implementation itself is task R3-08 and is **not** in this crate.

use crate::config::Config;

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

/// A place a [`Config`] can be loaded from and saved to.
///
/// Deliberately object-safe and three methods wide. The firmware implementation
/// (`NvsStore`, task R3-08) is the only production implementation; the host
/// tests use the in-memory store in `tests/support/`.
pub trait ConfigStore {
    /// Read the stored configuration.
    ///
    /// `Ok(None)` means nothing has been stored yet — first boot, or after a
    /// factory reset. That is **not** an error: the caller uses
    /// [`Config::default`].
    ///
    /// # Errors
    ///
    /// [`StoreError::Unavailable`] if the store cannot be opened, and
    /// [`StoreError::Corrupt`] if a blob is present but undecodable. Both mean
    /// "use the defaults", and neither is fatal.
    fn load(&mut self) -> Result<Option<Config>, StoreError>;

    /// Write the configuration, replacing whatever was there.
    ///
    /// Must be atomic from the reader's point of view: a reader either sees the
    /// previous value or the new one, never a mixture. This is the whole reason
    /// the store holds one blob rather than 98 keys.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unavailable`], [`StoreError::ReadOnly`] or
    /// [`StoreError::WriteFailed`]. A failed save leaves the previous
    /// configuration in place; it must not clear it.
    fn save(&mut self, config: &Config) -> Result<(), StoreError>;

    /// Remove everything, returning the store to its never-written state.
    ///
    /// Backs `POST /api/factory-reset`. After this, [`ConfigStore::load`]
    /// returns `Ok(None)`.
    ///
    /// # Errors
    ///
    /// As [`ConfigStore::save`].
    fn erase_all(&mut self) -> Result<(), StoreError>;

    /// Reset the in-memory configuration to the compiled-in defaults and write
    /// them, so the device reboots into a known state rather than into whatever
    /// NVS happened to contain.
    ///
    /// # Errors
    ///
    /// As [`ConfigStore::save`].
    fn reset_to_defaults(&mut self) -> Result<(), StoreError> {
        self.save(&Config::default())
    }
}
