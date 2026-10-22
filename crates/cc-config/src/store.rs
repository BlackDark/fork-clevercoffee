//! Storage errors and the configuration safety boundary.
//!
//! [`BlobConfigStore`](crate::BlobConfigStore) writes a whole configuration atomically.
//! It cannot depend on `cc-safety`; callers must validate
//! `cc_machine::safety_config` of a stored configuration before applying it and fail closed
//! when validation rejects them. The backend-independent format lives in
//! [`blob_store`](crate::blob_store).

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
