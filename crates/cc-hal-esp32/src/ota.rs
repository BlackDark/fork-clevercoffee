//! Streaming writes to flash: the half of an OTA that needs a chip.
//!
//! Owner: **R3-15**.
//!
//! # What this module is, and what it is not
//!
//! It is four calls to `esp_ota_*` and two to `esp_partition_*`, plus the loop
//! that reads a socket and hands each run straight through. Every *decision*
//! — may we flash, is this file acceptable, is it big enough, is the upload
//! truncated — is in [`cc_web::ota`] or [`cc_machine::ota`] and is host-tested
//! there. This file decides nothing, which is why it is small and why it is the
//! only part of the OTA that is unreachable from `cargo test`.
//!
//! # The memory argument, and why it holds here
//!
//! ```text
//! socket -> [ OTA_CHUNK_BYTES stack buffer ] -> cc_web::ota::PartReader -> flash
//! ```
//!
//! The only buffer is [`OTA_CHUNK_BYTES`], and it lives on the **stack** of the
//! task running the handler. Nothing in the loop allocates: the reader's hold-back
//! is `boundary.len() + 4` bytes inside a `Vec` whose capacity is reached on the
//! first push and never grows, and `esp_ota_write` copies straight into the SPI
//! flash driver's own buffer. So the heap cost of a 1.6 MB upload is the same as
//! the heap cost of a 4 KB one, which is the whole difference between this and an
//! OOM abort on a 320 KB part.
//!
//! This is the *inbound* half of
//! [ADR-0002](../../../docs/adr/0002-wifi-logging-ota-memory-architecture.md)
//! decision 2. That ADR's outbound half stopped a 19 KB `String` copy from
//! aborting the firmware; without the inbound half, one 1.6 MB `String` would
//! undo it.
//!
//! # Why the handler runs on the httpd task and still does not block the API
//!
//! It does block it, for the duration of one upload, and that is stated rather
//! than hidden. ESP-IDF's httpd is a single task (`web_async`'s module docs),
//! so any handler that loops is the C++'s own `ESPAsyncWebServer` behaviour: an
//! upload occupies the web server. The C++ has the same property and does not
//! work around it either.
//!
//! What the C++ *does* work around is the **URL** download, which it queues for
//! the main loop (`ota.cpp:795-820`, with the reason: *"Running it here would
//! stall the `AsyncTCP` task and the response would never reach the client"*).
//! [`Kind::Filesystem`] and [`Kind::Firmware`] uploads are the same case here,
//! and the same answer is available for them — see the route's `202` arm in
//! `web.rs`. What is **not** done is the whole 500 ms queue-delay dance, because
//! with the reader streaming, the response is written after the last byte rather
//! than before the first, and a browser's `fetch` waits for it either way.
//!
//! # What happens to the watchdog
//!
//! **Nothing, and that is a decision.** The C++ suspends the task watchdog for
//! the whole OTA (`ota.cpp:99-110` → `g_watchdog->suspend()`), because its flash
//! write runs on the same loop that feeds the watchdog, and an erase of a 1.8 MB
//! partition blocks for seconds.
//!
//! This firmware does not need that, because the flash write happens on the
//! **httpd** task and the watchdog is subscribed to the **control** task
//! (`main.rs:1802`, 04 §2: "Watchdog feed — control task only"). The control
//! task keeps ticking and keeps feeding while the httpd task erases, so the
//! watchdog stays armed and a genuinely wedged flash still resets the chip. That
//! is strictly safer than the C++'s arrangement, which removes the fail-safe for
//! the exact window it most wants one. Recorded in `intentional-diffs.md`.

// The `unsafe` in this module is eight `esp_ota_*` / `esp_partition_*` calls,
// and the workspace lint is `unsafe_code = "deny"`, so the allowance is here,
// scoped to this one module, exactly as `crate::web` and `crate::web_async` do
// theirs. Every call's SAFETY comment states what the C signature requires and
// why this module satisfies it; the three facts the whole file rests on are
// that a partition-table pointer is valid for the life of the process (the
// table lives in flash), that an `esp_ota_handle_t` is consumed by `esp_ota_end`
// / `esp_ota_abort` on both paths, and that a payload slice is passed by pointer
// with its length beside it, so no FFI call can read past it.
#![allow(
    unsafe_code,
    reason = "eight `esp_ota_*`/`esp_partition_*` calls, each with its SAFETY \
              comment; see the note above"
)]

use core::num::NonZeroI32;
use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use esp_idf_svc::sys::EspError;
use esp_idf_sys::{
    esp_ota_abort, esp_ota_begin, esp_ota_end, esp_ota_get_next_update_partition, esp_ota_handle_t,
    esp_ota_set_boot_partition, esp_ota_write, esp_partition_erase_range, esp_partition_find_first,
    esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_DATA_LITTLEFS, esp_partition_t,
    esp_partition_type_t_ESP_PARTITION_TYPE_DATA, esp_partition_write,
};

pub use cc_web::ota::{Kind, ReadError};

/// The read buffer, and therefore the entire heap-independent memory an upload
/// costs.
///
/// **4096, and the number is load-bearing.** It is the payload size at which
/// `esp_ota_write`'s own internal buffering stops mattering (it writes in
/// 4 KiB sectors), and it is comfortably above a browser's typical TCP segment
/// so `httpd_req_recv` returns a useful amount each call. It is a `const` and
/// lives in a `[u8; OTA_CHUNK_BYTES]` on the task stack, so it costs **zero**
/// heap — see the module docs, which is the entire reason this is a fixed array
/// and not a `Vec`.
pub const OTA_CHUNK_BYTES: usize = 4096;

/// `OTA_SIZE_UNKNOWN`, from `esp_ota_ops.h:25`.
///
/// Passed to [`esp_ota_begin`] for a firmware upload because the exact image
/// length is not known until the multipart part terminates — and a *smaller*
/// declared size would make `esp_ota_end` validate a prefix, not the image.
///
/// The cost is that the whole app slot is erased up front
/// (`esp_ota_ops.c:189-197`: `if (image_size != OTA_SIZE_UNKNOWN) { erase }`),
/// which is seconds of blocked flash. That is the same trade the C++ makes with
/// `Update.begin(UPDATE_SIZE_UNKNOWN)` (`ota.cpp:426`), and it is why this
/// module's watchdog story is a paragraph rather than a shrug.
const OTA_SIZE_UNKNOWN: usize = 0xFFFF_FFFF;

/// `ESP_ERR_INVALID_ARG`, as the `NonZeroI32` [`EspError::from_non_zero`] wants.
///
/// Spelled once because the workspace lint forbids a bare `unwrap`/`expect`
/// outside tests and `EspError::from` on a raw code is awkward to read at a call
/// site; the named constant says which error is being manufactured, which is the
/// only thing that matters at these two sites.
const ERR_INVALID_ARG: NonZeroI32 = match NonZeroI32::new(esp_idf_sys::ESP_ERR_INVALID_ARG) {
    Some(value) => value,
    None => panic!("ESP_ERR_INVALID_ARG is non-zero by definition"),
};

/// `ESP_ERR_NOT_FOUND` — the code a `esp_partition_find_first` miss deserves.
const ERR_NOT_FOUND: NonZeroI32 = match NonZeroI32::new(esp_idf_sys::ESP_ERR_NOT_FOUND) {
    Some(value) => value,
    None => panic!("ESP_ERR_NOT_FOUND is non-zero by definition"),
};

/// `ESP_ERR_INVALID_SIZE` — a payload larger than its destination.
const ERR_INVALID_SIZE: NonZeroI32 = match NonZeroI32::new(esp_idf_sys::ESP_ERR_INVALID_SIZE) {
    Some(value) => value,
    None => panic!("ESP_ERR_INVALID_SIZE is non-zero by definition"),
};

/// A session writing one partition.
///
/// Dropped without [`Writer::end`] leaves the destination half-written and
/// **does not** change the boot partition, so the machine still boots the image
/// it booted before. That is the property the power-cut story rests on, and it
/// is a property of `esp_ota_end`, not of this code — see [`Writer::end`].
pub struct Writer {
    kind: Kind,
    /// `esp_ota_handle_t` for a firmware session; unused for a filesystem one.
    ///
    /// `esp_ota_handle_t` is `u32` (`esp_ota_ops.h:47`), so this is 4 bytes and
    /// the field costs nothing. It is `0` for a filesystem session because
    /// `esp_ota_*` is **not** used for a data partition at all — `esp_ota_begin`
    /// requires an app partition and returns `ESP_ERR_INVALID_ARG` otherwise —
    /// so a filesystem image goes through `esp_partition_erase_range` +
    /// `esp_partition_write`.
    handle: esp_ota_handle_t,
    /// The partition being written. For firmware this is the *staging* slot the
    /// `esp_ota_*` handle already owns, kept so the error arm can report which
    /// one failed.
    partition: *const esp_partition_t,
    written: usize,
}

impl Writer {
    /// Open a session against `kind`'s destination, erasing it.
    ///
    /// # Errors
    ///
    /// `esp_ota_begin`'s error for a firmware slot (including
    /// `ESP_ERR_OTA_PARTITION_CONFLICT`, which means the table has only one app
    /// partition), or `esp_partition_find_first` returning null for the
    /// filesystem — which is what a **label mismatch** looks like, and the
    /// reason the lookup uses [`Kind::PARTITION_LABEL`] (`littlefs`) rather than
    /// the C++'s `"spiffs"`.
    ///
    /// # Safety-irreducible note
    ///
    /// `esp_ota_begin` erases the destination before returning. The route waits
    /// for the control task to apply [`cc_machine::ota::begin_session`]'s
    /// shutdown — re-checking admission against the **live** machine state on
    /// the way, not against a snapshot the httpd task read earlier — before
    /// this is called, and this doc comment is the second place that fact is
    /// written down.
    pub fn begin(kind: Kind) -> Result<Self, EspError> {
        match kind {
            Kind::Firmware => {
                // SAFETY: `esp_ota_get_next_update_partition(NULL)` is the
                // documented "the next OTA slot" call (`esp_ota_ops.h:288-297`) and
                // returns null only if the table has no second app partition. It
                // is a read of a static table, and the pointer it yields stays
                // valid for the life of the process — the table is in flash.
                let partition = unsafe { esp_ota_get_next_update_partition(core::ptr::null()) };
                if partition.is_null() {
                    return Err(EspError::from_non_zero(ERR_INVALID_ARG));
                }
                let mut handle: esp_ota_handle_t = 0;
                // SAFETY: `partition` is non-null (checked) and came from the
                // partition table; `out_handle` is a live local. Both are the
                // signature's own requirements (`esp_ota_ops.h:105`). The
                // `borrow_as_ptr` allow is the C out-parameter: `esp_ota_begin`
                // takes `*mut esp_ota_handle_t` and there is no `Option<*mut T>`
                // form of the call.
                #[allow(
                    clippy::borrow_as_ptr,
                    reason = "`esp_ota_begin` is a C out-parameter; `&mut out` is \
                              the only way to spell the `*mut *mut` it takes"
                )]
                let rc = unsafe { esp_ota_begin(partition, OTA_SIZE_UNKNOWN, &mut handle) };
                match EspError::from(rc) {
                    None => Ok(Self {
                        kind,
                        handle,
                        partition,
                        written: 0,
                    }),
                    Some(err) => Err(err),
                }
            }
            Kind::Filesystem => {
                // `Kind::PARTITION_LABEL` is a `&'static str`; `esp_partition_find_first`
                // wants a NUL-terminated `c_char` pointer. The literal below must
                // agree with it, and `the_filesystem_partition_label_matches_the_lookup`
                // in `web.rs` is what keeps them in step.
                let label = c"littlefs";
                // SAFETY: `label` is a `'static` NUL-terminated C string, which is
                // the third parameter's whole contract. The returned pointer is
                // into the static partition table and outlives this call.
                let partition = unsafe {
                    esp_partition_find_first(
                        esp_partition_type_t_ESP_PARTITION_TYPE_DATA,
                        esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_DATA_LITTLEFS,
                        label.as_ptr(),
                    )
                };
                if partition.is_null() {
                    // Null means "no partition of that type/subtype/label". The
                    // one reason that can happen here is a label that does not
                    // match `rust/partitions_4M.csv`, and `ESP_ERR_NOT_FOUND` is
                    // the honest code for it.
                    return Err(EspError::from_non_zero(ERR_NOT_FOUND));
                }
                let size = capacity(kind);
                // SAFETY: `partition` is non-null (checked) and names a real data
                // partition; `offset` is 0 and `size` is that partition's own
                // length, both from `cc_web::ota::capacity` and the table, so the
                // range is inside it. This is the whole-partition erase the C++
                // gets from `Update.begin(UPDATE_SIZE_UNKNOWN, U_SPIFFS, ...)`.
                let rc = unsafe { esp_partition_erase_range(partition, 0, size) };
                match EspError::from(rc) {
                    None => Ok(Self {
                        kind,
                        handle: 0,
                        partition,
                        written: 0,
                    }),
                    Some(err) => Err(err),
                }
            }
        }
    }

    /// Which partition this session writes.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    /// How many payload bytes have been written.
    #[must_use]
    pub const fn written(&self) -> usize {
        self.written
    }

    /// Write one run of payload.
    ///
    /// # Errors
    ///
    /// The flash driver's error — `ESP_ERR_FLASH_OP_FAIL` on a write failure,
    /// which is the case the C++ turns into `"Write failed at byte N"`
    /// (`ota.cpp:208`). A `run` longer than
    /// [`cc_web::ota::capacity`] is refused **here**, before the driver refuses
    /// it, so the caller gets a message naming the partition rather than a
    /// driver code.
    pub fn write(&mut self, run: &[u8]) -> Result<(), EspError> {
        if run.is_empty() {
            return Ok(());
        }
        let next = self.written + run.len();
        if next > capacity(self.kind) {
            // Not `ESP_ERR_INVALID_SIZE`: the *caller* is at fault for feeding
            // more than the slot holds, and a size code would read as "the image
            // was the wrong size" — which is a different bug with a different fix.
            return Err(EspError::from_non_zero(ERR_INVALID_SIZE));
        }
        match self.kind {
            Kind::Firmware => {
                // SAFETY: `run` is a live slice and `size` is its length, which is
                // what `esp_ota_write` reads (`esp_ota_ops.h:177`); `handle` came
                // from a successful `esp_ota_begin` on this object and has not
                // been ended. A zero-length `run` never reaches here.
                let rc = unsafe { esp_ota_write(self.handle, run.as_ptr().cast(), run.len()) };
                // `EspError::from(ESP_OK)` is `None`, so this is the whole
                // conversion: an `Ok` becomes no error and any other code becomes
                // exactly that code. `unwrap_or` is unreachable — the `is_some`
                // arm has just proved it.
                if let Some(err) = EspError::from(rc) {
                    return Err(err);
                }
            }
            Kind::Filesystem => {
                // SAFETY: as for firmware, plus `dst_offset` is `self.written`,
                // which the `next > capacity` check above has just bounded against
                // this partition's length.
                let rc = unsafe {
                    esp_partition_write(
                        self.partition,
                        self.written,
                        run.as_ptr().cast::<core::ffi::c_void>(),
                        run.len(),
                    )
                };
                if let Some(err) = EspError::from(rc) {
                    return Err(err);
                }
            }
        }
        self.written = next;
        Ok(())
    }

    /// Finish the session and **switch the boot partition**.
    ///
    /// # What this is, precisely
    ///
    /// For a **firmware** session this is the whole power-cut story, and it is
    /// worth stating exactly what was and was not verified:
    ///
    /// * `esp_ota_end` validates the written image (magic byte, segment
    ///   headers and, when enabled, the SHA-256 of the whole image) — and
    ///   **that is all it does**. It does *not* select the new slot.
    ///   `esp_ota_end` is `ota_verify_partition` and cleanup
    ///   (`esp_ota_ops.c:477-524`); the only writer of `otadata` on the write
    ///   path is `esp_ota_set_boot_partition` (`:599`). This module never calls
    ///   it, and `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` is not set in this
    ///   build, so nothing else switches the slot either.
    ///   **Measured on a bench ESP32, 2026-10-06:** an upload answered
    ///   `200 {"success":true,...,"restart":true}`, the device rebooted, and the
    ///   bootloader logged `Loaded app from partition at offset 0x10000` —
    ///   `app0`, the slot it was already running from. The image in `app1` was
    ///   complete and validated, and was not booted. **An OTA through this route
    ///   did not take effect.**
    /// * [`Writer::end`] now calls `esp_ota_set_boot_partition` after a
    ///   successful `esp_ota_end`, so the slot is selected. **The consequence is
    ///   stated plainly because it is the reason this was not done sooner:**
    ///   there is no rollback, so a *bad* image in the selected slot is
    ///   unbootable without USB. Before this call, a bad update cost nothing and
    ///   a good one did nothing either; now a good one takes effect and a bad
    ///   one needs a cable. Recovery is `just flash <port>`, and the upload is
    ///   validated by `esp_ota_end` before the slot moves, so the window is an
    ///   image that boots and then misbehaves.
    /// * Therefore: a power cut **before** `esp_ota_end` leaves `otadata`
    ///   pointing at the slot the machine booted from, and it boots that slot
    ///   again. A power cut **during** `esp_ota_end` leaves the `app1` image
    ///   half-written and `otadata` untouched, so the device still boots the
    ///   slot it came from.
    /// * Once the slot is switched (see the first bullet — it is not switched
    ///   today), a power cut after that point means the new image is selected and
    ///   is a complete, validated image. There is no rollback: `CONFIG_
    ///   BOOTLOADER_APP_ROLLBACK_ENABLE` is **not set** in this build's
    ///   `sdkconfig` (checked: no `BOOTLOADER_APP_ROLLBACK` line at all), so
    ///   `esp_ota_mark_app_valid_cancel_rollback` is a no-op and a new image that
    ///   boots and then misbehaves stays selected. This is a **deliberate,
    ///   recorded limitation**, not an oversight — and it is the C++'s behaviour
    ///   too, since the C++ also never enables rollback.
    ///
    /// For a **filesystem** session there is no boot partition to switch and
    /// nothing to validate: the bytes are in the partition, and the C++ requires
    /// the same (`ota.cpp:474` calls `Update.end(true)`).
    ///
    /// # Errors
    ///
    /// `esp_ota_end`'s validation error — a truncated image, a bad magic byte, a
    /// SHA-256 mismatch. That is the answer to "the upload completed but the image
    /// is wrong", and it arrives here rather than at the socket.
    pub fn end(mut self) -> Result<(), EspError> {
        match self.kind {
            Kind::Firmware => {
                // SAFETY: `handle` is live (from `begin`, not yet ended) and
                // consumed by `esp_ota_end`, which frees the handle on **both**
                // paths (`esp_ota_ops.h:205-208`). `self` is taken by value so no
                // second call is possible.
                let rc = unsafe { esp_ota_end(self.handle) };
                self.handle = 0;
                // A validated image is not a *selected* one. `esp_ota_end` is
                // `ota_verify_partition` and cleanup
                // (`esp_ota_ops.c:477-524`); the only writer of `otadata` on the
                // write path is `esp_ota_set_boot_partition` (`:599`), and with
                // `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` unset nothing else
                // switches the slot either. Without this call the machine
                // restarts into the slot it came from and the update silently
                // does not take effect — measured on a bench ESP32 on 2026-10-07
                // before this line existed.
                //
                // Only on success: a rejected image must leave `otadata`
                // pointing at the slot that is known to work, which is the
                // direction `abort` also takes. `EspError::from` is `None` on
                // `ESP_OK` in this binding.
                if let Some(err) = EspError::from(rc) {
                    return Err(err);
                }
                // SAFETY: `self.partition` came from `esp_ota_get_next_update_partition`
                // in `begin`, so it is a pointer into the static partition table
                // and outlives this call; it is an OTA app partition, which is
                // what `esp_ota_set_boot_partition` requires of its argument.
                // The function reads `otadata` and writes the selector; it does
                // not take ownership of the pointer.
                let rc = unsafe { esp_ota_set_boot_partition(self.partition) };
                EspError::from(rc).map_or(Ok(()), Err)
            }
            // No finalisation: `esp_partition_write` is already durable in the
            // partition. `self` is consumed for symmetry with the firmware arm.
            Kind::Filesystem => Ok(()),
        }
    }

    /// Give up: erase the handle, do **not** switch the boot partition.
    ///
    /// Called on every error path. The point is what it does *not* do — no
    /// `esp_ota_set_boot_partition`, so the machine still boots what it booted
    /// before, which is the safe direction for a failed update.
    pub fn abort(mut self) {
        if self.kind == Kind::Firmware && self.handle != 0 {
            // SAFETY: as [`Self::end`]; `esp_ota_abort` also frees the handle
            // (`esp_ota_ops.h:224-230`).
            unsafe { esp_ota_abort(self.handle) };
            self.handle = 0;
        }
    }
}

/// The destination's capacity, from the partition table.
const fn capacity(kind: Kind) -> usize {
    match kind {
        Kind::Firmware => cc_web::ota::MAX_FIRMWARE_BYTES,
        Kind::Filesystem => cc_web::ota::MAX_FILESYSTEM_BYTES,
    }
}

// ==================================================== the session, across tasks

/// The control task's answer to `Command::OtaBegin`.
///
/// The request cannot be answered where it is made: the httpd task sees a
/// telemetry snapshot up to one control period old, and the machine stays fully
/// live until the control task's next tick — long enough for a `brew_start` off
/// MQTT, or a brew-switch press, to enter a state whose `on_entry` opens the
/// water valve. So the answer is a value the control task produces and the
/// route collects, not a verdict the route reaches on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// The live state was quiescent and the safe hardware shutdown is applied.
    ///
    /// The only value that authorises `esp_ota_begin`. Set **after** the
    /// shutdown has been applied, so the route cannot observe it early.
    Admitted,
    /// The live state had moved into one that flows water or steam.
    Refused(cc_machine::ota::FlashRefusal),
}

/// The one update in flight, shared between the httpd task and the firmware.
///
/// Behind a [`Mutex`] because a [`Status`] is four fields and this is written
/// once per 4 KiB chunk — roughly 400 times over a firmware upload, i.e. a
/// handful of `memcpy`s. The alternative, an `AtomicU64` packing progress and
/// size, would save microseconds and cost the ability to report a truncation
/// with a reason, which is the thing an operator needs.
///
/// # Poisoning
///
/// Every method returns `Status::default()` on a poisoned lock rather than
/// propagating. A panic while holding this lock can only happen inside
/// `String` allocation, and the honest response to "the progress display cannot
/// be written" is a progress display that says idle — **not** a failed update
/// that aborts a flash mid-write. The flash handle is not in here, so nothing
/// about the write itself depends on this lock being healthy.
pub struct Session {
    status: Mutex<cc_web::ota::Status>,
    /// The kind of the update in flight, set by [`Session::claim`] and read by
    /// [`Session::note_progress`].
    ///
    /// This is the session's answer to the C++'s `isFilesystem` flag
    /// (`ota.cpp:234`), and it is a field rather than an argument because the
    /// two are decided at different points: the route knows the kind when it
    /// claims, and the progress callback only knows how many bytes have
    /// arrived. A progress bar scaled against the wrong floor stalls halfway on
    /// a filesystem upload.
    kind: Mutex<Option<Kind>>,
    /// Set while a flash handle is open, so a second request is refused.
    busy: AtomicBool,
    /// The control task's answer to `Command::OtaBegin`, or `None` before it
    /// has answered.
    ///
    /// An answer rather than a request, for the reason on [`Admission`]. Cleared
    /// by [`Session::claim`] so a session can never inherit the previous one's,
    /// and read once by [`Session::take_verdict`].
    verdict: Mutex<Option<Admission>>,
    /// Set when a successful update wants the firmware to reboot.
    restart: AtomicBool,
}

impl Session {
    /// A session that has never run.
    #[must_use]
    pub fn new() -> Self {
        Self {
            status: Mutex::new(cc_web::ota::Status::default()),
            kind: Mutex::new(None),
            busy: AtomicBool::new(false),
            verdict: Mutex::new(None),
            restart: AtomicBool::new(false),
        }
    }

    /// Claim the session for `kind`, or refuse because one is already running.
    ///
    /// A compare-and-set rather than a lock, so two simultaneous uploads cannot
    /// both see "idle" and both erase a partition. The C++'s equivalent is
    /// `otaBusy()` (`ota.cpp:68-70`) and it answers the same `409` the UI
    /// already has a toast for (`OTAUpdateSection.tsx:198`).
    pub fn claim(&self, kind: Kind) -> bool {
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        // A claim starts a new session, so it starts with no answer. Without
        // this a session could inherit the previous one's `Admitted` and erase
        // the running image with the hardware still live.
        if let Ok(mut slot) = self.verdict.lock() {
            *slot = None;
        }
        if let Ok(mut slot) = self.kind.lock() {
            *slot = Some(kind);
        }
        if let Ok(mut status) = self.status.lock() {
            *status = cc_web::ota::Status {
                phase: cc_web::ota::Phase::Uploading,
                ..cc_web::ota::Status::default()
            };
        }
        true
    }

    /// Whether an update is in flight.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }

    /// Record the control task's answer to `Command::OtaBegin`.
    ///
    /// Called from the control task beside the applier pass that applies
    /// [`cc_machine::ota::begin_session`]'s effects — after it has re-read the
    /// **live** machine state, so a state that moved into a refused one between
    /// the request and the apply refuses here and never authorises a flash.
    pub fn note_verdict(&self, admission: Admission) {
        if let Ok(mut slot) = self.verdict.lock() {
            *slot = Some(admission);
        }
    }

    /// The control task's answer, and clear it.
    ///
    /// Read exactly once, by the upload route, after
    /// [`crate::web::Shared::wait_applied`] has confirmed the command was
    /// folded in. **`None` means no flash** — the command was never answered, or
    /// the lock was poisoned. Both are the same answer to the only question the
    /// route has, which is whether `esp_ota_begin` may run.
    pub fn take_verdict(&self) -> Option<Admission> {
        self.verdict.lock().ok().and_then(|mut slot| slot.take())
    }

    /// Note progress.
    pub fn note_progress(&self, phase: cc_web::ota::Phase, uploaded: usize, total: usize) {
        // Read the kind before the status lock, so the two never nest.
        let kind = self
            .kind
            .lock()
            .ok()
            .and_then(|slot| *slot)
            .unwrap_or(Kind::Firmware);
        if let Ok(mut status) = self.status.lock() {
            status.phase = phase;
            status.uploaded = uploaded;
            status.total = total;
            // The C++ scales progress against a 512 KiB / 256 KiB floor
            // (`ota.cpp:216-218,234`) rather than a declared length, because a
            // browser's `Content-Length` covers the multipart envelope. The same
            // arithmetic is reproduced here for the same reason, against **this
            // session's** kind, so a filesystem image's bar fills the way the
            // C++'s does instead of stalling at the firmware floor. It means a
            // large image's bar reaches ~90 % and then jumps to 100 on success —
            // which is what the C++ does and what the operator is used to.
            status.progress = cc_web::ota::progress_percent(kind, uploaded);
        }
    }

    /// Record a successful finish and ask for the restart.
    pub fn finish_ok(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.phase = cc_web::ota::Phase::Complete;
            status.progress = 100;
            status.error = None;
        }
        self.busy.store(false, Ordering::Release);
        self.restart.store(true, Ordering::Release);
    }

    /// Record a failure and release the session.
    pub fn finish_err(&self, message: cc_web::ota::StatusMessage) {
        if let Ok(mut status) = self.status.lock() {
            status.phase = cc_web::ota::Phase::Error;
            status.error = Some(message);
        }
        self.busy.store(false, Ordering::Release);
    }

    /// A copy of the status, for `GET /api/ota/status`.
    #[must_use]
    pub fn status(&self) -> cc_web::ota::Status {
        self.status
            .lock()
            .map_or_else(|_| cc_web::ota::Status::default(), |status| status.clone())
    }

    /// Whether a successful update asked for a reboot, and clear the request.
    ///
    /// Read by the control task between ticks, exactly like
    /// [`crate::web::Shared::take_reboot_request`] — the httpd task must not be
    /// the one calling `esp_restart`, because a handler that reset the machine
    /// would abandon the response the operator's browser is still reading. That
    /// is the same reason the reboot routes go through a `Command`.
    pub fn take_restart(&self) -> bool {
        self.restart.swap(false, Ordering::AcqRel)
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests {
    use super::{Admission, Session};
    use alloc::string::String;
    use cc_web::ota::{Kind, Phase, StatusMessage};

    #[cfg_attr(test, test)]
    pub fn a_session_refuses_a_second_claim_while_one_is_running() {
        let session = Session::new();
        assert!(session.claim(Kind::Firmware));
        assert!(session.is_busy());
        assert!(
            !session.claim(Kind::Filesystem),
            "a second upload must be refused, not queued"
        );
        session.finish_err(StatusMessage::Flash);
        assert!(!session.is_busy(), "a failure releases the session");
        assert!(
            session.claim(Kind::Firmware),
            "and the next one is admitted"
        );
    }

    /// 🔴 A fresh claim inherits **no** verdict, and an unanswered request
    /// reads as `None`.
    ///
    /// `None` is the answer that stops the flash: the route reads it after
    /// `wait_applied`, and only `Admitted` lets `esp_ota_begin` run. So two
    /// properties are load-bearing and both are asserted here — a claim clears
    /// the verdict, so a session cannot inherit the previous one's `Admitted`
    /// and erase the running image with the hardware still live, and the verdict
    /// is consumed once, so a later reader cannot resurrect it.
    #[cfg_attr(test, test)]
    pub fn a_claim_starts_with_no_verdict_and_a_verdict_is_read_once() {
        let session = Session::new();
        assert!(session.claim(Kind::Firmware));
        assert_eq!(
            session.take_verdict(),
            None,
            "no answer has been given yet, and no answer means no flash"
        );
        session.note_verdict(Admission::Admitted);
        assert_eq!(
            session.take_verdict(),
            Some(Admission::Admitted),
            "the control task's answer reaches the route"
        );
        assert_eq!(
            session.take_verdict(),
            None,
            "and it is consumed, so it cannot be read twice"
        );

        // The next session must not inherit it. `claim` refuses while a
        // session is running, so the first one has to be finished first --
        // otherwise this asserts nothing about inheritance, it asserts the
        // refusal that the case above already covers.
        session.finish_err(StatusMessage::Flash);
        assert!(session.claim(Kind::Firmware));
        assert_eq!(
            session.take_verdict(),
            None,
            "a new claim must not inherit the previous session's verdict"
        );
    }

    /// A refusal carries the operator-facing reason across the task boundary.
    ///
    /// The UI prints `result.message` verbatim, so the reason the control task
    /// refused with has to arrive intact — a `Refused` that lost its payload
    /// would reach an operator as a blank toast.
    #[cfg_attr(test, test)]
    pub fn a_refusal_keeps_its_reason_across_the_task_boundary() {
        let session = Session::new();
        assert!(session.claim(Kind::Firmware));
        session.note_verdict(Admission::Refused(
            cc_machine::ota::FlashRefusal::SteamActive,
        ));
        match session.take_verdict() {
            Some(Admission::Refused(reason)) => assert!(!reason.message().is_empty()),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[cfg_attr(test, test)]
    pub fn a_finished_update_asks_for_exactly_one_restart() {
        let session = Session::new();
        assert!(session.claim(Kind::Firmware));
        session.finish_ok();
        assert!(session.take_restart());
        assert!(
            !session.take_restart(),
            "a second take must not reboot again"
        );
    }

    #[cfg_attr(test, test)]
    pub fn a_failure_does_not_ask_for_a_restart() {
        let session = Session::new();
        assert!(session.claim(Kind::Firmware));
        session.finish_err(StatusMessage::Invalid);
        assert!(!session.take_restart(), "a failed update must not reboot");
        assert_eq!(session.status().phase, Phase::Error);
    }

    #[cfg_attr(test, test)]
    pub fn the_status_reports_progress_as_bytes_arrive() {
        let session = Session::new();
        assert!(session.claim(Kind::Firmware));
        session.note_progress(Phase::Uploading, 512 * 1024, 1_675_952);
        let status = session.status();
        assert_eq!(status.progress, 90);
        assert_eq!(status.uploaded, 512 * 1024);
        assert_eq!(status.total, 1_675_952);
        assert!(status.is_updating());
        assert!(status.is_in_progress());
        let json: String = status.status_json();
        assert!(json.contains("\"status\":\"uploading\""), "{json}");
    }

    /// 🔴 The claimed kind reaches the progress bar.
    ///
    /// The arithmetic is host-tested (`progress_is_scaled_against_each_kinds_own_
    /// floor` in `cc-web`); this is the half only this crate can see — that
    /// [`Session::claim`]'s kind is the one `note_progress` scales against. It was
    /// the defect: the kind was written and never read, so a filesystem upload
    /// was scaled against the 512 KiB firmware floor and its bar stalled at 45 %
    /// on a successful update.
    #[cfg_attr(test, test)]
    pub fn a_filesystem_upload_is_scaled_against_the_filesystem_floor() {
        let session = Session::new();
        assert!(session.claim(Kind::Filesystem));
        session.note_progress(Phase::Uploading, 256 * 1024, 393_216);
        assert_eq!(
            session.status().progress,
            90,
            "the C++ reaches 90 % at 256 KiB for a filesystem image (`ota.cpp:234`)"
        );
        // And the firmware floor would have read 45 for the same byte count, so
        // the two kinds are distinguishable through this session.
        let firmware = Session::new();
        assert!(firmware.claim(Kind::Firmware));
        firmware.note_progress(Phase::Uploading, 256 * 1024, 1_675_952);
        assert_eq!(firmware.status().progress, 45);
    }
}
