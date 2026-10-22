//! NVS transport for [`cc_config::BlobConfigStore`].
//!
//! `EspDefaultNvsPartition::take()` erases and reinitializes full or newer-version
//! partitions, with a warning. Namespace and wire-format policy belong to `cc-config`.
//!
//! `set_blob` erases, writes, then commits; NVS copy-on-write preserves the previous
//! page before commit. `get_blob` returns a shortened slice without shrinking the
//! buffer, so query `blob_len` first to size it.
//!
//! `EspNvs` is `Send`, not `Sync`: move ownership to one task and use command queues
//! instead of sharing the store.

use alloc::vec;
use alloc::vec::Vec;

use cc_config::blob_store::BlobBackend;
use cc_config::predecessor::{PredecessorProbe, CPP_NAMESPACE};
use cc_config::store::StoreError;
use cc_protocol::sensor::hx711::{decode_tare, encode_tare, TareRecord};
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition, EspNvs};

/// An NVS namespace opened read-write, behind [`BlobBackend`].
///
/// A newtype rather than a type alias, and the reason is the orphan rule:
/// `impl BlobBackend for EspNvs<NvsDefault>` is not allowed from this crate,
/// because neither the trait nor the type is local. One local type with one impl
/// is the whole of the fix, and it has the side benefit of naming what the
/// firmware actually holds.
///
/// It also means `BlobConfigStore::erase_all` and the `raw()` diagnostics are
/// reachable without a downcast, and that the store handed to the HTTP server
/// has one concrete type.
///
/// # Why it holds the partition as well as the handle
///
/// Because [`probe_predecessor`] has to open a **second** namespace, and
/// `EspDefaultNvsPartition::take()` is a one-shot: `NvsDefault::new` returns
/// `ESP_ERR_INVALID_STATE` if `DEFAULT_TAKEN` is already set
/// (`nvs.rs:74-83`). There is no second `take()` to call at boot, and no public
/// accessor that hands the partition back out of an `EspNvs` — so the handle
/// alone cannot answer the question, and re-taking would have failed the very
/// first boot.
///
/// `EspNvsPartition<NvsDefault>` is `Clone` (`nvs.rs:296-302`, an `Arc` clone),
/// so the partition is kept here and the namespace opened from a clone of it.
/// The stored clone is never dropped: `EspNvs` holds its own, and this one is
/// released with the store.
pub struct EspNvsBlob {
    handle: EspDefaultNvs,
    partition: EspDefaultNvsPartition,
}

impl core::fmt::Debug for EspNvsBlob {
    /// Names the namespace and key, never the value: the value is the whole
    /// configuration, four of whose fields are credentials.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (namespace, key) = Self::location();
        write!(f, "EspNvsBlob({namespace}/{key})")
    }
}

impl EspNvsBlob {
    /// The namespace and key the configuration occupies, for the boot log.
    #[must_use]
    pub const fn location() -> (&'static str, &'static str) {
        (cc_config::blob_store::NAMESPACE, cc_config::blob_store::KEY)
    }
}

/// Look at [`CPP_NAMESPACE`] and report whether it holds anything.
///
/// The **only** NVS enumeration this firmware performs outside its own
/// namespace, and it is deliberately the smallest thing that answers the
/// question: open `config` **read-only**, take the first key name, drop the
/// handle. Key names and values are never read, nothing is written, and
/// nothing is erased: a partition written by an earlier firmware keeps every
/// byte it holds, which is what makes the boot line
/// [`cc_config::predecessor::startup_notice`] promises when it says "not
/// deleted".
///
/// Read-only is load-bearing. `nvs_open` with `NVS_READWRITE` **creates** a
/// namespace that does not exist (`nvs.rs:341-347`), so opening `config`
/// read-write on a machine whose settings were never written would manufacture
/// the very namespace whose absence is the signal. Read-only on a missing namespace
/// returns `ESP_ERR_NVS_NOT_FOUND` instead, which is [`PredecessorProbe::Absent`].
///
/// The decision of **whether** to look — and the words to print if one does —
/// are not here: both are [`cc_config::predecessor`]'s, and are host-tested.
/// This function cannot fail: every error becomes
/// [`PredecessorProbe::Unreadable`], because a diagnostic that could stop the
/// firmware booting would be a worse fault than the one it reports.
#[must_use]
pub fn probe_predecessor(nvs: &EspNvsBlob) -> PredecessorProbe {
    // `keys()` exists from ESP-IDF 5.2; this tree pins 5.5.5 (`just doctor`).
    let opened = EspNvs::new(nvs.partition.clone(), CPP_NAMESPACE, false);
    let handle = match opened {
        Ok(handle) => handle,
        // Not an error: no `config` namespace means no earlier firmware wrote here,
        // or
        // something erased it. Both are "nothing there", which is the answer
        // the caller wants and not a fault to report.
        Err(err) if err.code() == esp_idf_sys::ESP_ERR_NVS_NOT_FOUND => {
            return PredecessorProbe::Absent;
        }
        Err(err) => {
            log::warn!(
                "nvs: the {CPP_NAMESPACE:?} namespace could not be opened: {err:?} — this \
                 firmware cannot tell whether a previous firmware's settings are present"
            );
            return PredecessorProbe::Unreadable;
        }
    };
    let listing = handle.keys(None);
    let probe = match listing {
        Ok(mut keys) => {
            if keys.next_key().is_some() {
                PredecessorProbe::Populated
            } else {
                PredecessorProbe::Absent
            }
        }
        Err(err) => {
            log::warn!("nvs: the {CPP_NAMESPACE:?} namespace could not be listed: {err:?}");
            PredecessorProbe::Unreadable
        }
    };
    probe
}

/// Open the default NVS partition and one namespace within it.
///
/// # Errors
///
/// [`StoreError::Unavailable`] if the partition cannot be opened or the
/// namespace cannot be created. Neither is fatal: the firmware falls back to
/// the compiled-in defaults and says so, which is the same path a fresh device
/// takes.
pub fn open(namespace: &str) -> Result<EspNvsBlob, StoreError> {
    let partition = EspDefaultNvsPartition::take().map_err(|err| {
        log::error!("nvs: default partition unavailable: {err:?}");
        StoreError::Unavailable
    })?;
    EspNvs::new(partition.clone(), namespace, true)
        .map(|handle| EspNvsBlob { handle, partition })
        .map_err(|err| {
            log::error!("nvs: namespace {namespace:?} unavailable: {err:?}");
            StoreError::Unavailable
        })
}

impl BlobBackend for EspNvsBlob {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        // `blob_len` first, so the read buffer is sized to the stored value and
        // not to `MAX_BLOB_BYTES`. A 2 KB configuration read into an 8 KB buffer
        // on every boot is 6 KB of peak heap for nothing, on a 320 KB heap.
        let Some(len) = self.handle.blob_len(key).map_err(|err| {
            log::error!("nvs: blob_len({key:?}) failed: {err:?}");
            StoreError::Unavailable
        })?
        else {
            return Ok(None);
        };
        let mut buf = vec![0u8; len];
        match self.handle.get_blob(key, &mut buf) {
            Ok(Some(bytes)) => Ok(Some(Vec::from(bytes))),
            Ok(None) => Ok(None),
            Err(err) => {
                log::error!("nvs: get_blob({key:?}) failed: {err:?}");
                Err(StoreError::Unavailable)
            }
        }
    }

    fn set(&mut self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        self.handle.set_blob(key, value).map_err(|err| {
            log::error!("nvs: set_blob({key:?}) failed: {err:?}");
            StoreError::WriteFailed
        })
    }

    fn erase_all(&mut self) -> Result<(), StoreError> {
        self.handle.erase_all().map_err(|err| {
            log::error!("nvs: erase_all failed: {err:?}");
            StoreError::WriteFailed
        })
    }
}

/// The NVS key the scale's tare lives under.
///
/// `cc.scale.tare` — 12 characters, inside the 15-character NVS limit, in the
/// same `cc` namespace as the configuration blob and carrying the same `cc.`
/// prefix so a human reading `nvs_dump` can tell which firmware wrote what.
///
/// **A separate key rather than a field in the configuration blob.** A tare is
/// not a parameter: it has no range, no default and no UI, and
/// `GET /api/parameters` would gain an entry that the web UI would then have to
/// render. It also has to survive a reset, which a RAM member does not; this is
/// where it goes instead.
pub const TARE_KEY: &str = "cc.scale.tare";

/// Read the stored tare, if there is a readable one.
///
/// # Errors
///
/// [`StoreError::Unavailable`] if the namespace could not be read. The caller
/// substitutes "no stored tare", because a scale that cannot restore its tare
/// can still tare at start-up — losing a tare is an inconvenience, not a fault.
pub fn load_tare(nvs: &EspNvsBlob) -> Result<Option<TareRecord>, StoreError> {
    let Some(bytes) = nvs.get(TARE_KEY)? else {
        return Ok(None);
    };
    let Some(record) = decode_tare(&bytes) else {
        // Not an error: a blob this firmware did not write, or one written by an
        // older encoding. The start-up tare covers it.
        log::warn!("nvs: the stored scale tare is not readable — ignoring it");
        return Ok(None);
    };
    Ok(Some(record))
}

/// Write the tare.
///
/// # Errors
///
/// [`StoreError::WriteFailed`] or [`StoreError::Unavailable`]. A failed write
/// leaves the previous tare in place, which is the same atomicity
/// [`cc_config::BlobConfigStore::save`] relies on: `set_blob` erases, sets and
/// commits, and the commit is the only thing that publishes the new page.
pub fn save_tare(nvs: &mut EspNvsBlob, record: TareRecord) -> Result<(), StoreError> {
    nvs.set(TARE_KEY, &encode_tare(record))
}

/// The NVS key the backflush shot counter lives under.
///
/// `cc.maint.shots` — 13 characters, inside the same 15-character NVS limit as
/// [`TARE_KEY`], in the same `cc` namespace and under the same `cc.` prefix.
///
/// **A separate key rather than a field in the configuration blob**, like
/// [`TARE_KEY`]: this firmware writes one configuration blob and puts the
/// handful of values that are *not* configuration beside it. One namespace, one
/// owner, one place to look. The predecessor's namespace is a different one and
/// is not read; see `cc_config::blob_store`'s module documentation on why a
/// partition written by another firmware must be invisible.
pub const SHOTS_SINCE_BACKFLUSH_KEY: &str = "cc.maint.shots";

/// Read the stored shot count, or `None` if there is not a readable one.
///
/// A device whose partition was erased, or that is running this firmware for
/// the first time, starts at zero.
///
/// # Errors
///
/// [`StoreError::Unavailable`] if the key could not be read. A counter that
/// cannot be restored is not a fault — the reminder starts counting from
/// wherever it is, which is a lost count, not a broken machine — so the caller
/// substitutes 0 and says so.
pub fn load_shots_since_backflush(nvs: &EspNvsBlob) -> Result<Option<i32>, StoreError> {
    let Some(bytes) = nvs.get(SHOTS_SINCE_BACKFLUSH_KEY)? else {
        return Ok(None);
    };
    let Some(shots) = cc_machine::maintenance::decode_shot_count(&bytes) else {
        // Same shape as an unreadable tare: not an error, just not ours.
        log::warn!("nvs: the stored shot count is not readable — starting from 0");
        return Ok(None);
    };
    Ok(Some(shots))
}

/// Write the shot count.
///
/// The count is four bytes, little-endian, like the tare.
///
/// # Errors
///
/// [`StoreError::WriteFailed`] or [`StoreError::Unavailable`]. **A failed write
/// leaves the previous count in place**, which is the same property
/// [`cc_config::BlobConfigStore::save`] relies on, and it is why the in-memory
/// count is not reverted to match: the two agree *eventually* anyway — the next
/// counted brew writes the then-current value, which includes this one — so
/// reverting would throw away a real shot to buy a temporary agreement. The
/// failure is logged as an `error!` at the call site instead.
pub fn save_shots_since_backflush(nvs: &mut EspNvsBlob, shots: i32) -> Result<(), StoreError> {
    nvs.set(
        SHOTS_SINCE_BACKFLUSH_KEY,
        &cc_machine::maintenance::encode_shot_count(shots),
    )
}
