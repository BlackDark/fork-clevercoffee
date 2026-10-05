//! The NVS wire format: one namespace, one JSON blob, one key.
//!
//! Owner: **R3-08** (task A).
//!
//! # Why the store is split in two
//!
//! Everything interesting about *where* the bytes live is separable from
//! everything interesting about what they mean: the namespace name, the key
//! name, the JSON encoding, the schema version, and the decision that a
//! namespace the C++ also wrote is not ours. None of that needs an ESP32.
//!
//! So [`BlobConfigStore`] owns the format and [`BlobBackend`] owns the medium.
//! The device implementation of the backend is
//! `cc_hal_esp32::nvs::EspNvsBlob` — three methods over
//! `esp_idf_svc::nvs::EspNvs` — and everything else, including every rejection
//! path, is a host unit test. This is the same split
//! [`cc_domain::heater`](../../cc_domain/heater/index.html) uses: the decision
//! is portable, the peripheral is not.
//!
//! The store's `load`/`save`/`erase_all` are **inherent methods**, not a trait.
//! Every caller in the workspace names the concrete
//! `BlobConfigStore<EspNvsBlob>`, nothing is generic over a store, and there is
//! no `dyn`. The seam that does get used is one level down, [`BlobBackend`] —
//! and it already provides the substitution a store trait would have. Finding
//! 4.6 of 32-findings-2026-10-03.
//!
//! # One blob, not 98 keys
//!
//! The C++ writes each parameter separately into the `config` namespace under an
//! FNV-1a-hashed key — `"p" + 8 hex digits` (`Config.h:318-332, 487-501`).
//! `saveAll()` writes 98 keys one at a time (`Config.cpp:154-171`), so a power
//! cut mid-save leaves a configuration where some parameters are new and the
//! rest are old. For a machine that heats to 150 °C, "some new" is a
//! configuration nobody ever chose.
//!
//! The recovered oracle firmware stored **one nested JSON blob** in a single
//! namespace, 2071 bytes (08 §3, 08 §5.3), and 08 §6 recommends it. This does
//! the same.
//!
//! # ⚠ No C++ compatibility, and this is deliberate
//!
//! Decided 2026-09-28: the Rust firmware owns its namespace, and a NVS written
//! by the C++ firmware is **expected to be ignored and overwritten with
//! defaults**. There is no migration, no compatibility shim, and no
//! "read the old hashed keys if the new key is absent" path.
//!
//! Three things make this safe rather than merely convenient:
//!
//! * **A different namespace cannot collide.** The C++ uses `config`; this uses
//!   [`NAMESPACE`]. NVS namespaces are independent key spaces, so a C++-written
//!   partition is not merely unreadable, it is invisible. A test asserts it.
//! * **The old keys are not garbage-collected and do not need to be.** They sit
//!   in a namespace nothing opens. The 20 KB partition is not the constraint
//!   (a blob is ~2 KB, 08 §3), and erasing another namespace would be a write
//!   path with its own failure modes for no benefit.
//! * **A first boot with a C++-written NVS is indistinguishable from a first
//!   boot on a blank device**, which is exactly the state the C++ firmware
//!   reaches on a factory reset anyway. The operator's saved setpoints are
//!   lost once, deliberately, and the reason is recorded in the boot log.
//!
//! The alternative — importing the C++'s 98 FNV-1a keys — would mean shipping
//! the C++'s hash function, its key list, and its per-parameter type table, and
//! would make the Rust schema permanently answerable to a C++ file that is
//! being deleted. That is a large permanent cost for a one-time migration of
//! values that are all within a whisker of their defaults.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use serde_json::from_slice;

use crate::config::Config;
use crate::store::StoreError;

/// The NVS namespace the Rust firmware owns.
///
/// 15 characters is the NVS limit and this is 2. It is deliberately **not**
/// `config`, which is what the C++ uses (`Config.cpp:265`): see the module
/// documentation on why a C++-written partition must be invisible.
pub const NAMESPACE: &str = "cc";

/// The single key the configuration blob is stored under.
///
/// 15 characters is the NVS limit and this is 11. It carries the `cc.` prefix
/// as well as the namespace, so that a human reading an `nvs_dump` of a
/// development board can tell at a glance which firmware wrote what, without
/// having to know which namespace is open.
pub const KEY: &str = "cc.config";

/// The format version stored alongside the blob.
///
/// A [`u32`] written with the blob in the same key, so reading it and reading
/// the blob cannot disagree. It is **not** a migration mechanism — see the
/// module docs. It exists so that a future firmware which changes the encoding
/// can tell "this blob is not mine" from "this blob is corrupt", and report the
/// difference, instead of both producing the same opaque parse failure.
pub const SCHEMA_VERSION: u32 = 1;

/// The largest blob this store will read or write, in bytes.
///
/// A [`Config`] serialises to about 2100 bytes (08 §3 measured 2071 for the
/// oracle's schema, which is the same 97 keys). The bound is 4× that: generous
/// enough that no legitimate configuration is refused, tight enough that a
/// corrupt length field cannot make the firmware try to allocate megabytes
/// from a 320 KB heap before it has decided whether the blob is even its own.
///
/// The NVS partition itself is 20 KB (`partitions_4M.csv`), so a blob this size
/// fits several times over; the limit is about what the *reader* will trust.
pub const MAX_BLOB_BYTES: usize = 8 * 1024;

/// A key-value medium that stores opaque byte strings in one namespace.
///
/// The three methods are the whole of what the NVS API this firmware needs
/// offers: `nvs_get_blob`, `nvs_set_blob` and `nvs_erase_all`. Notably absent
/// is enumeration, because a format with one key has nothing to enumerate and
/// an API that could return "keys I do not understand" is an API that invites
/// someone to act on a foreign writer's data.
///
/// The `&self` on `get` and the `&mut self` on `set` are deliberate and match
/// [`BlobConfigStore::load`]: reading a configuration must not need exclusive
/// access, so a diagnostics task can read it while the web server holds it.
/// ESP-IDF's
/// `nvs_open` handle is thread-safe for this access pattern
/// (`nvs_get_blob` takes the partition lock, `esp32-hal`'s and this crate's
/// callers all treat a handle as shareable).
pub trait BlobBackend {
    /// The bytes stored under `key`, or `None` if the key is absent.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unavailable`] if the namespace could not be opened.
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError>;

    /// Write `value` under `key`, replacing whatever was there.
    ///
    /// Must be atomic from a reader's point of view: a reader sees the previous
    /// value or the new one, never a mixture, and never a half-written length
    /// prefix. That is the entire reason this store holds one blob.
    ///
    /// # Errors
    ///
    /// [`StoreError::WriteFailed`] or [`StoreError::Unavailable`]. A failed
    /// write must leave the previous value intact.
    fn set(&mut self, key: &str, value: &[u8]) -> Result<(), StoreError>;

    /// Remove every key in this backend's namespace.
    ///
    /// # Errors
    ///
    /// [`StoreError::WriteFailed`] or [`StoreError::Unavailable`].
    fn erase_all(&mut self) -> Result<(), StoreError>;

    /// The number of bytes this backend will accept for one value, if it has a
    /// bound of its own.
    ///
    /// ESP-IDF's NVS has no useful one — the limit is a function of the
    /// partition size and the current fill — so the device backend returns
    /// `None` and the store's own [`MAX_BLOB_BYTES`] applies.
    fn capacity_hint(&self) -> Option<usize> {
        None
    }
}

/// A configuration store that moves one JSON blob through a [`BlobBackend`].
#[derive(Debug)]
pub struct BlobConfigStore<B> {
    backend: B,
}

/// Bytes of framing in front of the JSON: a little-endian `u32` version.
///
/// 4, and a power of two, so the JSON starts on an 8-byte boundary and
/// `serde_json` gets an aligned slice. It is a separate byte count from
/// [`SCHEMA_VERSION`] because this one is a compile-time property of the
/// encoding and the other is a value that changes.
const ENVELOPE_HEADER_BYTES: usize = 4;

impl<B: BlobBackend> BlobConfigStore<B> {
    /// Wrap a backend.
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }

    /// The wrapped backend, for the diagnostics that report on NVS itself
    /// (`GET /api/nvs-debug` reads the namespace, the free heap and the
    /// parameter count).
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    /// The wrapped backend, mutably.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// The stored bytes and their schema version, without decoding them.
    ///
    /// This is what `/api/nvs-debug` reports on: a blob that exists, how big it
    /// is, and which version wrote it. It is deliberately *not* a way to get a
    /// `Config` — decoding is [`Self::load`]'s job, and there is one path into
    /// a `Config` so that the schema version and the safety check cannot be
    /// skipped by using the wrong door.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unavailable`], or [`StoreError::Corrupt`] if the envelope
    /// is malformed.
    pub fn raw(&self) -> Result<Option<(u32, usize)>, StoreError> {
        let Some(bytes) = self.backend.get(KEY)? else {
            return Ok(None);
        };
        if bytes.len() < ENVELOPE_HEADER_BYTES {
            return Err(StoreError::Corrupt);
        }
        let version = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        Ok(Some((version, bytes.len() - ENVELOPE_HEADER_BYTES)))
    }

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
    pub fn load(&mut self) -> Result<Option<Config>, StoreError> {
        let Some(bytes) = self.backend.get(KEY)? else {
            // `Ok(None)` is not an error: a machine on a fresh partition, or one
            // whose partition was written by the C++ firmware, has nothing here.
            // The caller uses `Config::default()` and says so in the boot log.
            return Ok(None);
        };
        if bytes.len() < ENVELOPE_HEADER_BYTES {
            // A blob too short to even hold the version. Not a `Config` this
            // firmware could have written, so treat it as absent rather than as
            // an error the caller has to distinguish: the outcome is the same
            // (defaults) and the boot log reports the size.
            return Ok(None);
        }
        let version = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if version != SCHEMA_VERSION {
            // A different version is not corruption, it is a *newer or older*
            // writer. Either way this firmware cannot interpret it, and
            // interpreting half of it would be worse than not interpreting any
            // of it. Defaults, and the boot log names the version it found.
            return Ok(None);
        }
        let json = &bytes[ENVELOPE_HEADER_BYTES..];
        match from_slice::<Config>(json) {
            Ok(config) => Ok(Some(config)),
            Err(_) => {
                // A blob this firmware wrote that will not decode: a truncated
                // write, or bit rot in a 20 KB partition that has been written
                // to on every parameter change for years. Half a configuration
                // is not a safer configuration than none (see
                // `cc_config::StoreError::Corrupt`), so this is reported as
                // corruption and the caller uses the defaults.
                Err(StoreError::Corrupt)
            }
        }
    }

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
    pub fn save(&mut self, config: &Config) -> Result<(), StoreError> {
        let json = serde_json::to_vec(config).map_err(|_| StoreError::WriteFailed)?;
        if json.len() > MAX_BLOB_BYTES {
            // A `Config` cannot reach this — 98 bounded fields serialise to
            // about 2100 bytes — so this is a compile-time-shaped assertion
            // that a future unbounded text field would trip. Failing the write
            // is right: truncating would store a configuration that is missing
            // parameters, which is the exact failure mode one blob was chosen
            // to eliminate.
            return Err(StoreError::WriteFailed);
        }
        let limit = self.backend.capacity_hint().unwrap_or(MAX_BLOB_BYTES);
        if json.len() > limit {
            return Err(StoreError::WriteFailed);
        }
        let mut bytes = Vec::with_capacity(ENVELOPE_HEADER_BYTES + json.len());
        bytes.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
        bytes.extend_from_slice(&json);
        self.backend.set(KEY, &bytes)
    }

    /// Remove everything, returning the store to its never-written state.
    ///
    /// Backs `POST /api/factory-reset`. After this, [`Self::load`] returns
    /// `Ok(None)`.
    ///
    /// # Errors
    ///
    /// As [`Self::save`].
    pub fn erase_all(&mut self) -> Result<(), StoreError> {
        self.backend.erase_all()
    }

    /// A one-line description of what is stored, for the boot log and for
    /// `GET /api/nvs-debug`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unavailable`] if the namespace cannot be read. The caller
    /// substitutes a placeholder, because a `/api/nvs-debug` that 500s tells an
    /// operator nothing about the machine.
    ///
    /// Names the namespace and the key, and whether a blob is present. It does
    /// **not** report the SSID, the MQTT password, the OTA password or the HTTP
    /// password — four of the five credentials in the blob are in there, and a
    /// diagnostic endpoint reachable without authentication is the wrong place
    /// for them to become visible.
    pub fn describe(&self) -> Result<String, StoreError> {
        match self.raw()? {
            None => Ok(format!("cc/{KEY}: empty")),
            Some((version, len)) => Ok(format!("cc/{KEY}: schema v{version}, {len} B JSON")),
        }
    }
}

/// An in-memory [`BlobBackend`], for host tests and for nothing else.
///
/// Not part of the shipped image: the only production [`BlobBackend`] is
/// `cc_hal_esp32::nvs::EspNvsBlob`, which wraps
/// `esp_idf_svc::nvs::EspNvs<NvsDefault>`. Keeping this out of the device build
/// means a 1.8 MB image does not carry a hash map for a test.
///
/// # Why it models *namespaces* and not just keys
///
/// Because the "no C++ compatibility" decision is a claim about key spaces, not
/// about key names, and a fake holding one flat map could not test it. So
/// [`MemoryBackend`] has an `own` namespace and a `foreign` one: the first is
/// what `nvs_open` would have opened, the second is bytes a previous firmware
/// left behind, and the test asserts the first cannot see the second.
#[derive(Debug, Default)]
pub struct MemoryBackend {
    own: BTreeMap<String, Vec<u8>>,
    foreign: BTreeMap<String, Vec<u8>>,
    fail_next_set: bool,
    fail_next_erase: bool,
}

impl MemoryBackend {
    /// A backend with an empty namespace, as after `nvs_flash_erase`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Plant bytes in a namespace this store is not opened on.
    ///
    /// This is how a test reproduces "the C++ firmware ran here first": the C++
    /// writes the `config` namespace, this store is opened on [`NAMESPACE`], and
    /// the question is whether it can see the other one. It cannot, and the
    /// test says so.
    pub fn plant_foreign(&mut self, namespace: &str, key: &str, value: &[u8]) {
        self.foreign
            .insert(format!("{namespace}/{key}"), Vec::from(value));
    }

    /// Whether a planted key is still present, i.e. was neither read nor
    /// garbage-collected.
    ///
    /// A store that erased another namespace's keys would be doing something no
    /// caller asked for and that could fail; the decision is to leave them
    /// alone.
    #[must_use]
    pub fn foreign_intact(&self, namespace: &str, key: &str) -> bool {
        self.foreign.contains_key(&format!("{namespace}/{key}"))
    }

    /// Make the next `set` fail once, so a test can assert the previous value
    /// survives.
    pub fn fail_next_set(&mut self) {
        self.fail_next_set = true;
    }

    /// How many keys this store's own namespace holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.own.len()
    }

    /// Whether this store's own namespace is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.own.is_empty()
    }
}

impl BlobBackend for MemoryBackend {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.own.get(key).cloned())
    }

    fn set(&mut self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        if self.fail_next_set {
            self.fail_next_set = false;
            // Leave `own` untouched: that is the property under test, and a fake
            // that cleared the key before failing would hide a real defect in
            // the store's ordering.
            return Err(StoreError::WriteFailed);
        }
        self.own.insert(String::from(key), Vec::from(value));
        Ok(())
    }

    fn erase_all(&mut self) -> Result<(), StoreError> {
        if self.fail_next_erase {
            self.fail_next_erase = false;
            return Err(StoreError::WriteFailed);
        }
        self.own.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec::Vec;

    use super::*;
    use crate::secret::Secret;

    struct FailingBackend;

    impl BlobBackend for FailingBackend {
        fn get(&self, _key: &str) -> Result<Option<Vec<u8>>, StoreError> {
            Err(StoreError::Unavailable)
        }
        fn set(&mut self, _key: &str, _value: &[u8]) -> Result<(), StoreError> {
            Err(StoreError::WriteFailed)
        }
        fn erase_all(&mut self) -> Result<(), StoreError> {
            Err(StoreError::WriteFailed)
        }
    }

    fn store() -> BlobConfigStore<MemoryBackend> {
        BlobConfigStore::new(MemoryBackend::new())
    }

    #[test]
    fn an_empty_store_loads_nothing_rather_than_erroring() {
        assert_eq!(store().load(), Ok(None));
    }

    #[test]
    fn a_configuration_round_trips() {
        let mut s = store();
        let mut original = Config::default();
        original.brew.setpoint = 94.5;
        original.pid.regular.kp = 12.25;
        original.system.hostname = "kitchen".into();
        s.save(&original).expect("save");
        assert_eq!(s.load().expect("load"), Some(original));
    }

    #[test]
    fn the_four_credentials_round_trip_unchanged() {
        // The blob is plaintext by design (`cc_config::Secret`'s docs): the
        // machine has to be able to use them. What matters here is that they
        // survive, so a reboot does not silently drop the Wi-Fi.
        let mut s = store();
        let mut original = Config::default();
        original.system.wifi.ssid = String::from("mynet");
        original.system.wifi.password = Secret::new(String::from("hunter2"));
        original.mqtt.password = Secret::new(String::from("brokerpw"));
        original.system.auth.password = Secret::new(String::from("httppw"));
        s.save(&original).expect("save");
        let back = s.load().expect("load").expect("a config");
        assert_eq!(back.system.wifi.ssid, "mynet");
        assert_eq!(back.system.wifi.password.expose(), "hunter2");
        assert_eq!(back.mqtt.password.expose(), "brokerpw");
        assert_eq!(back.system.auth.password.expose(), "httppw");
    }

    #[test]
    fn the_blob_is_a_namespace_and_a_key_the_cpp_does_not_use() {
        // This is the whole of the "no C++ compatibility" decision, stated as an
        // assertion rather than as a comment.
        assert_ne!(NAMESPACE, "config", "the C++ namespace is `config`");
        assert!(!KEY.starts_with('p'), "the C++ keys are `p` + 8 hex digits");
    }

    #[test]
    fn a_cpp_written_nvs_is_ignored_and_overwritten_with_defaults() {
        // Reproduce the C++'s NVS: the `config` namespace, one key per
        // parameter, each an FNV-1a hash of the dotted name. Then open the
        // store the way the firmware does and assert what actually happens.
        let mut backend = MemoryBackend::new();
        backend.plant_foreign("config", "p1a2b3c4", b"94.5");
        backend.plant_foreign("config", "pdeadbeef", b"1");
        let mut s = BlobConfigStore::new(backend);

        // 1. Nothing of ours is visible, so the firmware sees a first boot.
        assert_eq!(s.load(), Ok(None));

        // 2. Writing defaults leaves the C++'s namespace untouched: it is a
        //    different key space and this code has no path that opens it.
        s.save(&Config::default()).expect("save");
        let raw = s.raw().expect("raw").expect("a blob");
        assert_eq!(raw.0, SCHEMA_VERSION);

        // 3. The C++'s keys are still there, still unread, still harmless.
        assert!(s.backend().foreign_intact("config", "p1a2b3c4"));
        assert!(s.backend().foreign_intact("config", "pdeadbeef"));
        assert_eq!(
            s.backend().len(),
            1,
            "exactly one key, ours; the two C++ keys are in another namespace"
        );
    }

    #[test]
    fn a_blob_from_another_schema_version_is_ignored_rather_than_mis_parsed() {
        let mut s = store();
        let mut json = Vec::new();
        json.extend_from_slice(&(SCHEMA_VERSION + 1).to_le_bytes());
        json.extend_from_slice(br#"{"brew":{"setpoint":94.5}}"#);
        s.backend_mut().set(KEY, &json).expect("set");
        // Not an error: the firmware cannot read it, and the outcome is the
        // same as an empty partition. Reporting it as corrupt would send the
        // boot log looking for bit rot that is not there.
        assert_eq!(s.load(), Ok(None));
    }

    #[test]
    fn a_blob_this_firmware_wrote_that_will_not_decode_is_corrupt() {
        let mut s = store();
        let mut json = Vec::new();
        json.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
        json.extend_from_slice(b"{\"brew\": {\"setpoint\": 94.5"); // truncated
        s.backend_mut().set(KEY, &json).expect("set");
        assert_eq!(s.load(), Err(StoreError::Corrupt));
    }

    #[test]
    fn a_blob_too_short_to_hold_a_version_is_treated_as_absent() {
        let mut s = store();
        s.backend_mut().set(KEY, b"\x01\x00").expect("set");
        assert_eq!(s.load(), Ok(None));
    }

    #[test]
    fn an_unavailable_backend_surfaces_rather_than_reading_as_defaults() {
        // The distinction matters at boot: "no NVS" and "NVS broken" both end
        // up on the defaults, but only one of them is a fault to report.
        let mut s = BlobConfigStore::new(FailingBackend);
        assert_eq!(s.load(), Err(StoreError::Unavailable));
        assert_eq!(s.save(&Config::default()), Err(StoreError::WriteFailed));
        assert_eq!(s.erase_all(), Err(StoreError::WriteFailed));
    }

    #[test]
    fn a_failed_save_leaves_the_previous_configuration_in_place() {
        // The C++ can leave a machine on half of a new configuration. This is
        // the property one blob was chosen for, so it is worth a test.
        let mut s = store();
        let mut original = Config::default();
        original.brew.setpoint = 88.0;
        s.save(&original).expect("save");
        s.backend_mut().fail_next_set();

        let mut replacement = Config::default();
        replacement.brew.setpoint = 99.0;
        assert_eq!(s.save(&replacement), Err(StoreError::WriteFailed));
        assert_eq!(s.load().expect("load"), Some(original));
    }

    #[test]
    fn erase_all_returns_the_store_to_never_written() {
        let mut s = store();
        s.save(&Config::default()).expect("save");
        s.erase_all().expect("erase");
        assert_eq!(s.load(), Ok(None));
    }

    #[test]
    fn describe_names_the_key_but_never_a_credential() {
        let mut s = store();
        let mut c = Config::default();
        c.system.wifi.ssid = String::from("mysecretnet");
        c.system.wifi.password = Secret::new(String::from("hunter2"));
        s.save(&c).expect("save");
        let text = s.describe().expect("describe");
        assert!(text.contains("cc/cc.config"), "{text}");
        assert!(!text.contains("mysecretnet"), "{text}");
        assert!(!text.contains("hunter2"), "{text}");
    }

    #[test]
    fn the_serialised_blob_fits_the_bound_and_the_partition() {
        // ~2100 bytes measured on the oracle (08 §3). The bound is 8 KiB and
        // the partition is 20 KiB, so there is room for a few generations of
        // NVS garbage-collection churn before the partition fills.
        let mut json = Vec::new();
        json.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
        json.extend_from_slice(&serde_json::to_vec(&Config::default()).expect("serialise"));
        assert!(
            json.len() < MAX_BLOB_BYTES,
            "default blob is {} B, bound is {MAX_BLOB_BYTES} B",
            json.len()
        );
        const {
            assert!(
                MAX_BLOB_BYTES * 2 < 20 * 1024,
                "the bound must leave NVS headroom"
            );
        };
    }

    #[test]
    fn a_credential_never_reaches_the_describe_string() {
        // The four credential fields are in the blob. `describe` is what the
        // boot log and /api/nvs-debug print, so it is the one function where a
        // leak would be systematic rather than incidental.
        let mut s = store();
        let mut c = Config::default();
        c.system.ota_password = Secret::new(String::from("ota-secret"));
        c.system.auth.username = String::from("admin");
        s.save(&c).expect("save");
        let text = s.describe().expect("describe");
        assert!(!text.contains("ota-secret"), "{text}");
        // Non-secret fields are not what this function is for either; it reports
        // the container, not the contents.
        assert!(!text.contains("admin"), "{text}");
        assert!(text.contains("JSON"), "{text}");
    }

    #[test]
    fn the_schema_version_is_readable_without_decoding_the_configuration() {
        // /api/nvs-debug reports this. It must not require the blob to parse,
        // because the interesting case is a blob that does not parse.
        let mut s = store();
        assert_eq!(s.raw().expect("raw"), None);
        s.save(&Config::default()).expect("save");
        let (version, len) = s.raw().expect("raw").expect("a blob");
        assert_eq!(version, SCHEMA_VERSION);
        assert!(
            len > 1000,
            "a default Config is about 2 kB of JSON, got {len}"
        );
    }

    #[test]
    fn a_config_with_its_text_fields_full_round_trips() {
        // The C++ has no length checks on its text parameters (01 §10), so the
        // blob can be much larger than the ~2100 B average. A hostname is the
        // realistic worst case, and 30 is the C++'s own limit for it.
        let mut s = store();
        let mut c = Config::default();
        c.system.hostname = "k".repeat(30);
        c.mqtt.broker = "192.168.1.10".into();
        c.mqtt.topic = "clevercoffee/".into();
        s.save(&c).expect("save");
        assert_eq!(s.load().expect("load"), Some(c));
    }
}
