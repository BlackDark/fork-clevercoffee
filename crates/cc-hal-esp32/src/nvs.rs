//! The NVS backend: [`EspNvsBlob`].
//!
//! Owner: **R3-08** (task A).
//!
//! # What this file is
//!
//! The entire device half of the configuration store, and it is about thirty
//! lines because everything worth testing — the namespace, the key, the JSON
//! encoding, the schema version, the C++-written-NVS behaviour — lives in
//! `cc_config::BlobConfigStore` and is a host test. This is the same split
//! `heater.rs` uses: the decision is portable, the peripheral is not.
//!
//! # The API, verified against the installed crate
//!
//! From `esp-idf-svc` 0.53.0 `src/nvs.rs`:
//!
//! * `EspDefaultNvsPartition::take()` → `Result<Self, EspError>`, and
//!   `take_with(reinit: bool)` underneath it. `take()` passes `reinit = true`,
//!   which means a partition that is full (`ESP_ERR_NVS_NO_FREE_PAGES`) or from
//!   a newer NVS version (`ESP_ERR_NVS_NEW_VERSION_FOUND`) is **erased and
//!   re-initialised** rather than reported (`nvs.rs:76-96`). That is the right
//!   default here: a 20 KB partition holding one 2 KB blob cannot legitimately
//!   be full, and a partition from a newer NVS version is one this firmware
//!   cannot read anyway. The boot log says so when it happens, because
//!   `esp-idf-svc` logs a `warn!` at `nvs.rs:86-88`.
//! * `EspNvs::new(partition, namespace, read_write)` → opens the namespace.
//! * `EspNvs::blob_len(name)` → `Ok(None)` if the key is absent,
//!   `ESP_ERR_NVS_NOT_FOUND` is mapped to `None` inside (`nvs.rs:349-365`).
//! * `EspNvs::get_blob(name, buf)` → `Ok(None)` if absent.
//! * `EspNvs::set_blob(name, buf)` → erases the key, sets it, commits.
//! * `EspNvs::erase_all()` → erases and commits.
//!
//! Two of those are worth a second look:
//!
//! * **`set_blob` erases first** (`nvs.rs:398-410`). `nvs_erase_key` followed by
//!   `nvs_set_blob` followed by one `nvs_commit` is still atomic from a reader's
//!   point of view, because NVS is copy-on-write at page granularity and the
//!   commit is the only thing that publishes the new page. A power cut before
//!   the commit leaves the previous page intact. This is the property
//!   `ConfigStore::save` promises and `a_failed_save_leaves_the_previous_
//!   configuration_in_place` pins.
//! * **`get_blob` does not shrink its buffer.** It writes `len` into the `inout`
//!   length parameter and returns `&buf[..len]`, so the buffer must be at least
//!   as large as the stored value. `blob_len` first is therefore not an
//!   optimisation, it is the only way to size the read buffer without a
//!   `MAX_BLOB_BYTES` heap allocation on every boot.
//!
//! # `EspNvs` is `Send` but not `Sync`
//!
//! `unsafe impl<T: NvsPartitionId> Send for EspNvs<T>` exists (`nvs.rs:576`)
//! and there is no `Sync`. So the store can be *moved* to the diagnostics task
//! but not *shared*. It is therefore owned by one task — the firmware's network
//! task — and the web server reaches it through the same bounded command queue
//! everything else uses (04 §3.2), not by borrowing it.

use alloc::vec;
use alloc::vec::Vec;

use cc_config::blob_store::BlobBackend;
use cc_config::store::StoreError;
use cc_domain::sensor::hx711::{decode_tare, encode_tare, TareRecord};
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition, EspNvs};

/// An NVS namespace opened read-write, behind [`BlobBackend`].
///
/// A newtype rather than a type alias, and the reason is the orphan rule:
/// `impl BlobBackend for EspNvs<NvsDefault>` is not allowed from this crate,
/// because neither the trait nor the type is local. One local type with one impl
/// is the whole of the fix, and it has the side benefit of naming what the
/// firmware actually holds.
///
/// It also means `ConfigStore::erase_all` and the `raw()` diagnostics are
/// reachable without a downcast, and that the store handed to the HTTP server
/// has one concrete type.
pub struct EspNvsBlob(EspDefaultNvs);

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
    EspNvs::new(partition, namespace, true)
        .map(EspNvsBlob)
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
        let Some(len) = self.0.blob_len(key).map_err(|err| {
            log::error!("nvs: blob_len({key:?}) failed: {err:?}");
            StoreError::Unavailable
        })?
        else {
            return Ok(None);
        };
        let mut buf = vec![0u8; len];
        match self.0.get_blob(key, &mut buf) {
            Ok(Some(bytes)) => Ok(Some(Vec::from(bytes))),
            Ok(None) => Ok(None),
            Err(err) => {
                log::error!("nvs: get_blob({key:?}) failed: {err:?}");
                Err(StoreError::Unavailable)
            }
        }
    }

    fn set(&mut self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        self.0.set_blob(key, value).map_err(|err| {
            log::error!("nvs: set_blob({key:?}) failed: {err:?}");
            StoreError::WriteFailed
        })
    }

    fn erase_all(&mut self) -> Result<(), StoreError> {
        self.0.erase_all().map_err(|err| {
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
/// not a parameter: it is not in the C++'s 98 `ParamDef`s, it has no range, no
/// default and no UI, and `GET /api/parameters` would gain an entry that the
/// web UI would then have to render. The C++ keeps the tare in a `long` member
/// (`HX711_ADC.h:66`) and loses it on every reset; this is where it goes
/// instead.
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
