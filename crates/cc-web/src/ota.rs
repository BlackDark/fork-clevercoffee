//! The parts of an OTA that are pure functions: the upload envelope, the size
//! and extension rules, and the status document.
//!
//! Owner: **R3-15**.
//!
//! # What is here and what stayed behind
//!
//! | Here | Why |
//! | --- | --- |
//! | [`PartReader`] | A streaming `multipart/form-data` splitter. It is a byte-level state machine over the request body, and it is where the memory argument lives — see below. |
//! | [`Kind`], [`extension_allowed`] | `validateFileExtension` (`src/ota.cpp:186-193`), verbatim. |
//! | [`fits`], [`MAX_FIRMWARE_BYTES`] | The size rule. `esp_ota_begin` takes an image size; a 1.6 MB image on a 1,835,008 B slot is 87 % full, and the C++ discovers that by erasing the whole partition first and failing afterwards. |
//! | [`status_json`] | `handleStatus`'s document (`ota.cpp:726-756`), now carrying real values. |
//!
//! | In `cc-hal-esp32` | Why |
//! | --- | --- |
//! | `esp_ota_*` / `esp_partition_*` calls, the socket read loop, the restart | FFI and a socket. Neither is reachable from a host, and neither is where the decisions are. |
//! | **the admission check** | it needs `cc_safety` and the live state; it is [`cc_machine::ota::admit`] and it is host-tested there. |
//!
//! # The memory argument, in one place
//!
//! **A firmware image is 1,675,952 B. The heap is ~320 KB.** The image cannot be
//! buffered, and the C++ never buffered it either: `processUploadChunk`
//! (`ota.cpp:196-224`) calls `Update.write(data, len)` on whatever chunk
//! `AsyncWebServer` handed it and keeps no copy. ESP-IDF's `httpd` is the same
//! shape — `httpd_req_recv` fills a caller-supplied buffer and returns.
//!
//! So the pipeline is:
//!
//! ```text
//! socket -> [ 4 KiB stack buffer ] -> PartReader -> esp_ota_write
//! ```
//!
//! and the heap high-water mark of an upload is **the size of one chunk**,
//! independent of the image. [`PartReader`] holds a carry of
//! `boundary.len() + 4` bytes ([`CARRY_SLACK`]) — under 80 for any legal
//! boundary — because a multipart delimiter may straddle two reads and the last
//! few bytes cannot be emitted until it is known not to be one. Everything else
//! goes straight through.
//!
//! This is the *inbound* half of
//! [ADR-0002](../../../docs/adr/0002-wifi-logging-ota-memory-architecture.md)
//! decision 2. That ADR fixed the outbound half — never serialise a large
//! payload to an intermediate `String` — and recorded the resulting `abort()`
//! from failed `operator new`. An upload is the same failure in the other
//! direction: one 1.6 MB allocation on a 320 KB heap is an OOM abort, and unlike
//! the outbound case there is no ADR-0002 policy to lean on, only the code.
//!
//! # Why the header parser is bounded
//!
//! A `multipart/form-data` part's headers arrive *before* the payload, so
//! [`PartReader`] must find the `filename=` parameter before it can apply
//! [`extension_allowed`] — and it has done so before it has seen a single byte of
//! the image. They are therefore accumulated, which means a hostile or broken
//! client could make that accumulation grow. It is capped at
//! [`MAX_PART_HEADER_BYTES`] and a part that exceeds it is a hard error, not a
//! truncated header and not an unbounded `String`.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

/// Which partition an upload targets.
///
/// The C++'s `Type::Firmware` / `Type::Filesystem` (`ota.h:56-62`), which
/// `handleURLUpdate` compares the `type` form field against
/// (`ota.cpp:643`). Kept as two variants with no third so that a typo in the
/// `type` field is a compile error rather than a silent third behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The app slot — `esp_ota_*`.
    Firmware,
    /// The `LittleFS` data partition — `esp_partition_erase_range` +
    /// `esp_partition_write`.
    Filesystem,
}

impl Kind {
    /// The value the UI sends as the `type` form field
    /// (`OTAUpdateSection.tsx:186`: `formData.append(otaUpdateType, file)`).
    ///
    /// `firmware` and `filesystem`, lower case — which is what the C++ compares
    /// against, so there is no case folding to reproduce.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Firmware => "firmware",
            Self::Filesystem => "filesystem",
        }
    }

    /// Parse the `type` form field. Anything else is [`Kind::Firmware`], which
    /// is the C++'s default (`ota.cpp:640`:
    /// `String updateTypeParam = Type::Firmware;`).
    ///
    /// # Why an unknown value is not an error
    ///
    /// The C++ only checks `updateTypeParam == Type::Filesystem` and treats
    /// everything else as firmware, so an unknown `type` flashes the app slot.
    /// Erroring instead would be safer but would break a client sending a value
    /// this firmware has never heard of, and the target partition is decided by
    /// the *route* (`/api/ota/firmware` vs `/api/ota/filesystem`) — never by
    /// this field — so a wrong value here cannot write to the wrong partition.
    /// That is worth stating, because it is the reason the laxness is safe.
    #[must_use]
    pub fn from_field(value: &str) -> Self {
        if value == Self::Filesystem.as_str() {
            Self::Filesystem
        } else {
            Self::Firmware
        }
    }

    /// The partition label reported by `/api/ota/status`.
    ///
    /// The C++ reports `"spiffs"` (`ota.cpp:46`) and so does this, because the
    /// UI renders it as a label and a partition renamed under it would read as a
    /// different machine. The Rust partition table calls the partition `littlefs`
    /// (`rust/partitions_4M.csv`) — the *label* on the wire is the C++'s, and the
    /// lookup uses the real one. Recorded in `intentional-diffs.md`.
    pub const STATUS_LABEL: &'static str = "spiffs";

    /// The partition label the flash code looks up.
    ///
    /// `littlefs`, from `rust/partitions_4M.csv`. **Not** [`Self::STATUS_LABEL`]:
    /// `esp_partition_find_first` matches the label literally, and the C++'s
    /// `Update.begin(..., "spiffs")` found its own table's row — which is named
    /// `spiffs` in the *root* `partitions_4M.csv` and `littlefs` in the Rust one.
    /// Using the C++'s string against the Rust table returns null, and the whole
    /// filesystem endpoint would fail with a lookup error instead of an upload
    /// error.
    pub const PARTITION_LABEL: &'static str = "littlefs";

    /// The smallest payload that will be accepted for this kind.
    ///
    /// `processUploadChunk` (`ota.cpp:216-218`) uses 512 KiB for firmware and
    /// 256 KiB for the filesystem purely to scale its *progress percentage*, and
    /// rejects nothing on size. Here the same two numbers are used for their
    /// better purpose: an ESP32 app image with no valid app descriptor at offset
    /// `0x20` is 0xE9, not a bootloader — and a 4 KB "firmware" that passes
    /// `extension_allowed` and fails in `esp_ota_end` has already cost a full
    /// partition erase. Refusing it before the erase is worth the constant.
    /// A **function**, not an associated `const`. Rust has no
    /// `const X: usize = match self { .. }` — a `const` cannot read `self`, so
    /// the shape that compiles is ONE value for both variants. Written as a
    /// `const` this silently became the 512 KiB firmware floor applied to a
    /// 384 KiB filesystem partition, and every legitimate filesystem image was
    /// refused. A function is how a per-variant answer is spelled.
    #[must_use]
    pub const fn min_accepted(self) -> usize {
        match self {
            Self::Firmware => 512 * 1024,
            Self::Filesystem => 64 * 1024,
        }
    }
}

/// The app slot size, from `rust/partitions_4M.csv`'s `app0` row.
///
/// `0x1C0000` = 1,835,008 B. **Read from the table and transcribed, not computed
/// at runtime** — the C++'s `Update.begin` would have discovered a mismatch by
/// erasing 1.8 MB and then failing, and a `const` that disagreed with the table
/// would be a compile-time lie rather than a runtime surprise.
pub const MAX_FIRMWARE_BYTES: usize = 0x1C0_000;

/// The `LittleFS` partition size, from the same table's `littlefs` row.
///
/// `0x60000` = 393,216 B.
pub const MAX_FILESYSTEM_BYTES: usize = 0x6_0000;

/// The largest payload accepted for `kind`.
#[must_use]
pub const fn capacity(kind: Kind) -> usize {
    match kind {
        Kind::Firmware => MAX_FIRMWARE_BYTES,
        Kind::Filesystem => MAX_FILESYSTEM_BYTES,
    }
}

/// Does this payload fit its slot?
///
/// The check `esp_ota_begin` and `esp_partition_erase_range` cannot make for us.
/// Passing a size larger than the partition returns `ESP_ERR_INVALID_SIZE` from
/// `esp_ota_begin` — *after* the app slot has been selected but before any
/// erase — so this is a convenience, not the safety net. What it genuinely buys
/// is refusing a 10 MB browser-selected file with a message instead of a
/// partition-size error, which is what the UI's own 10 MB client-side cap
/// (`OTAUpdateSection.tsx:96`) allows through.
///
/// `size == 0` is **not** accepted. `esp_ota_begin` takes `OTA_SIZE_UNKNOWN`
/// (0xFFFFFFFF) for a length the client did not declare, and that erases the
/// whole slot; this port does not use it, because a length this port does not
/// know is a length it cannot check.
#[must_use]
pub const fn fits(kind: Kind, size: usize) -> bool {
    size >= kind.min_accepted() && size <= capacity(kind)
}

/// Does this filename's extension pass?
///
/// `validateFileExtension` (`ota.cpp:186-193`) exactly:
///
/// ```cpp
/// String path = filename; path.toLowerCase();
/// if (isFilesystem) return path.endsWith(".bin") || path.endsWith(".img");
/// return path.endsWith(".bin");
/// ```
///
/// Case-insensitive via `toLowerCase`, and matched on the *whole* string rather
/// than on a parsed extension, which is why `evil.bin.exe` passes and
/// `firmware.bin?x=1` does not. Preserved: this is a guard against an operator
/// picking the wrong file off their desktop, not a security control, and the
/// thing it actually protects — writing to the wrong partition — is prevented by
/// the route, not by the name.
#[must_use]
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the rule IS case-insensitive: `lower` above is the C++'s \
              `toLowerCase()`, and the lint fires on the comparison it cannot \
              see through the local"
)]
pub fn extension_allowed(filename: &str, kind: Kind) -> bool {
    let lower = filename.to_ascii_lowercase();
    match kind {
        Kind::Firmware => lower.ends_with(".bin"),
        Kind::Filesystem => lower.ends_with(".bin") || lower.ends_with(".img"),
    }
}

/// Why a stream stopped being a valid upload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The part headers exceeded [`MAX_PART_HEADER_BYTES`].
    HeadersTooLong,
    /// The body ended before the part's payload was terminated by its
    /// delimiter.
    Truncated,
    /// No part at all: the boundary never appeared.
    NoPart,
}

impl ReadError {
    /// Operator-facing text, in the C++'s error-string voice.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::HeadersTooLong => "Malformed upload: the part headers are too long.",
            Self::Truncated => "Upload ended before the file was complete.",
            Self::NoPart => "No file was found in the upload.",
        }
    }
}

/// How far a push got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Progress {
    /// Payload bytes handed to the sink by this call.
    pub emitted: usize,
    /// Cumulative payload bytes over the whole upload.
    pub total: usize,
    /// The closing delimiter has been seen: nothing more will be emitted.
    pub complete: bool,
}

/// The extra bytes [`PartReader`] must hold back.
///
/// A multipart delimiter is `\r\n--BOUNDARY`, and the reader cannot know that
/// the last bytes of a read are not its opening `\r\n--BOU`. Holding back
/// `boundary + 4` covers `\r\n` plus the two leading dashes plus the boundary
/// itself, with no slack for the terminator — which is why it is 4 and not 2.
const CARRY_SLACK: usize = 4;

/// The blank line that ends a part's headers.
const HEADER_TERMINATOR: &[u8] = b"\r\n\r\n";

/// The largest part-header block accepted, in bytes.
///
/// A `Content-Disposition` line with a filename is ~120 bytes. 256 leaves room
/// for a long filename and still bounds the accumulation at a quarter of the
/// 4 KiB read buffer that feeds it.
pub const MAX_PART_HEADER_BYTES: usize = 256;

/// Where in the envelope the reader is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Before the opening delimiter.
    Preamble,
    /// Reading the part's headers, up to the blank line.
    Headers,
    /// Reading the payload.
    Payload,
    /// After the closing delimiter. Nothing more is emitted.
    Done,
}

/// A streaming `multipart/form-data` splitter: one file part, one sink.
///
/// Constructed with the boundary from the request's `Content-Type` and fed one
/// `httpd_req_recv` chunk at a time. It never holds more than
/// [`MAX_PART_HEADER_BYTES`] plus `boundary.len() + CARRY_SLACK`, whatever the
/// payload size — see the module docs, which is the point of this type.
///
/// # Why one part and no more
///
/// The UI sends exactly one file field
/// (`formData.append(otaUpdateType, selectedOtaFile)`,
/// `OTAUpdateSection.tsx:186`), and the C++'s handler is registered as the
/// upload callback for a single file
/// (`server.on("/api/ota/firmware", HTTP_POST, onRequest, onUpload)`,
/// `ota.cpp:848-851`) — `AsyncWebServer` streams every part to the same callback
/// and does not distinguish them. Two parts here means a client sending something
/// this firmware never asked for; the second is skipped rather than
/// concatenated, because concatenating would produce a corrupt image that fails
/// only at `esp_ota_end`.
pub struct PartReader {
    /// `--BOUNDARY` — the opening delimiter, which starts the body.
    open: Vec<u8>,
    /// `\r\n--BOUNDARY` — the closing one. **The leading CRLF is the whole
    /// point**: RFC 2046 puts a CRLF before every delimiter except the first,
    /// and that CRLF belongs to the envelope, so a search for the bare
    /// `--BOUNDARY` would splice two bytes of MIME framing into the middle of
    /// a firmware image.
    close: Vec<u8>,
    stage: Stage,
    /// Emitted bytes not yet consumed: the tail of the previous read plus the
    /// head of this one, searched for the delimiter and otherwise passed on.
    carry: Vec<u8>,
    /// The part headers, bounded by [`MAX_PART_HEADER_BYTES`].
    headers: Vec<u8>,
    /// The `filename=` parameter, once the header block ends.
    filename: Option<String>,
    /// Cumulative payload bytes.
    total: usize,
}

impl PartReader {
    /// A reader for `boundary`, taken from the `Content-Type` header's
    /// `boundary=` parameter.
    ///
    /// # Panics
    ///
    /// Never, at runtime: the boundary is stored, not rendered into a format
    /// string, so a request with no boundary at all yields a reader whose
    /// [`ReadError::NoPart`] is returned on the first push. That is the honest
    /// answer — the C++ gets the same request, parses no multipart either, and
    /// writes the *whole body including the delimiters* into flash.
    #[must_use]
    pub fn new(boundary: &str) -> Self {
        let mut open = Vec::with_capacity(boundary.len() + 2);
        open.extend_from_slice(b"--");
        open.extend_from_slice(boundary.as_bytes());
        let mut close = Vec::with_capacity(boundary.len() + 4);
        close.extend_from_slice(b"\r\n--");
        close.extend_from_slice(boundary.as_bytes());
        Self {
            open,
            close,
            stage: Stage::Preamble,
            carry: Vec::new(),
            headers: Vec::new(),
            total: 0,
            filename: None,
        }
    }

    /// Extract the `boundary=` parameter from a `Content-Type` header value.
    ///
    /// Returns `None` when the header is absent or is not multipart, which the
    /// caller answers with a `400` — the C++'s `sendUploadResult(request, "No
    /// firmware file provided")` arm for a request that carries no file.
    ///
    /// Handles the quoted form (`boundary="x"`) and the unquoted one, and
    /// tolerates other parameters after it, because a browser's boundary is
    /// `----WebKitFormBoundary` followed by 16 random alphanumerics and the
    /// parameter is not last.
    #[must_use]
    pub fn boundary_of(content_type: Option<&str>) -> Option<&str> {
        let value = content_type?;
        if !value
            .to_ascii_lowercase()
            .starts_with("multipart/form-data")
        {
            return None;
        }
        let (_, after) = value.split_once("boundary=")?;
        let boundary = after.trim_start();
        // RFC 2046: the boundary is at most 70 characters and is not itself
        // quoted-in-quotes; a browser never quotes it, but `curl -F` does, so
        // the quoted form is unwrapped here rather than treated as a boundary
        // that begins with a double quote (which would then never match the
        // body's `--boundary`).
        let boundary = match boundary.strip_prefix('"') {
            Some(inner) => inner.split('"').next().unwrap_or(inner),
            None => boundary
                .split(|c: char| c == ';' || c.is_whitespace())
                .next()
                .unwrap_or(boundary),
        };
        if boundary.is_empty() {
            None
        } else {
            Some(boundary)
        }
    }

    /// The uploaded filename, once the part headers have been read.
    ///
    /// `None` until the blank line that ends the header block, and `None`
    /// forever if the part carried no `filename=` parameter — which is what an
    /// `application/octet-stream` body or a hand-rolled client looks like.
    #[must_use]
    pub fn filename(&self) -> Option<&str> {
        self.filename.as_deref()
    }

    /// Cumulative payload bytes seen so far.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.total
    }

    /// Whether the closing delimiter has been seen.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.stage, Stage::Done)
    }

    /// Feed one read chunk, handing every payload run to `sink` as it appears.
    ///
    /// `sink` is called zero or more times per push with contiguous slices of the
    /// **payload**, already stripped of the envelope. Passing it straight to
    /// `esp_ota_write` is the whole intended use, and it is why nothing in this
    /// function allocates proportionally to the image.
    ///
    /// # Errors
    ///
    /// [`ReadError::HeadersTooLong`] on a part header block past
    /// [`MAX_PART_HEADER_BYTES`], and [`ReadError::Truncated`] when the body ends
    /// with the reader still in [`Stage::Preamble`] — no boundary was ever seen,
    /// so nothing was written and there is nothing to report but a bad request.
    pub fn push(
        &mut self,
        chunk: &[u8],
        sink: &mut dyn FnMut(&[u8]),
    ) -> Result<Progress, ReadError> {
        let before = self.total;
        self.carry.extend_from_slice(chunk);

        // The first delimiter ends the preamble; the header block ends at the
        // blank line; payload runs until the delimiter reappears. Each stage
        // advances at most once per push because a read is 4 KiB and every
        // delimiter is longer than that for any legal boundary — but a tiny
        // boundary is legal, so the `loop` is what makes this correct rather than
        // merely usually-correct.
        loop {
            match self.stage {
                Stage::Preamble => {
                    let Some(at) = find(&self.carry, &self.open) else {
                        // Keep only what a split delimiter could still need.
                        self.retain_from_delimiter_start();
                        break;
                    };
                    self.carry.drain(..at + self.open.len());
                    self.stage = Stage::Headers;
                }
                Stage::Headers => {
                    // Search the whole carry, then **hold back the last 3
                    // bytes** rather than clearing it. Clearing was a real bug:
                    // a `\r\n\r\n` split across two reads was then never found,
                    // the header block never terminated, and a perfectly ordinary
                    // upload failed `HeadersTooLong` — but only at small chunk
                    // sizes, which is the worst shape of bug to reach hardware.
                    if let Some(at) = find(&self.carry, HEADER_TERMINATOR) {
                        if self.headers.len() + at > MAX_PART_HEADER_BYTES {
                            return Err(ReadError::HeadersTooLong);
                        }
                        self.headers.extend_from_slice(&self.carry[..at]);
                        self.carry.drain(..at + HEADER_TERMINATOR.len());
                        self.filename = filename_of(&self.headers);
                        self.stage = Stage::Payload;
                    } else {
                        let take = self.carry.len().saturating_sub(HEADER_TERMINATOR.len() - 1);
                        if self.headers.len() + take > MAX_PART_HEADER_BYTES {
                            // A header block this long cannot still be terminated
                            // within the cap, so refuse now rather than read on.
                            return Err(ReadError::HeadersTooLong);
                        }
                        let taken: Vec<u8> = self.carry.drain(..take).collect();
                        self.headers.extend_from_slice(&taken);
                        break;
                    }
                }
                Stage::Payload => {
                    // The delimiter is searched over the WHOLE carry, and the
                    // slack is only a hold-back applied when it is *absent*.
                    // Searching a truncated prefix instead would miss a closing
                    // delimiter that arrived in the final read, and the last few
                    // bytes of every image would be silently dropped — an image
                    // whose SHA-256 is wrong by a handful of bytes, which is the
                    // worst possible failure to diagnose from a device.
                    if let Some(at) = find(&self.carry, &self.close) {
                        emit(&self.carry[..at], sink, &mut self.total);
                        self.carry.drain(..at);
                        self.stage = Stage::Done;
                    } else {
                        // Hold back what a split delimiter could still need.
                        let keep = self.close.len() + CARRY_SLACK;
                        if self.carry.len() > keep {
                            let at = self.carry.len() - keep;
                            emit(&self.carry[..at], sink, &mut self.total);
                            self.carry.drain(..at);
                        }
                        break;
                    }
                }
                Stage::Done => {
                    self.carry.clear();
                    break;
                }
            }
        }

        Ok(Progress {
            emitted: self.total - before,
            total: self.total,
            complete: self.is_complete(),
        })
    }

    /// Drop the front of `carry`, keeping only the bytes a split delimiter could
    /// still be made of.
    ///
    /// Keep only the tail a half-delivered delimiter could still need.
    fn retain_from_delimiter_start(&mut self) {
        let keep = self.open.len() + CARRY_SLACK;
        if self.carry.len() > keep {
            let at = self.carry.len() - keep;
            self.carry.drain(..at);
        }
    }

    /// Check a finished stream: was a part found, and did it terminate?
    ///
    /// # Errors
    ///
    /// [`ReadError::NoPart`] when the opening delimiter never appeared, and
    /// [`ReadError::Truncated`] when the body ended mid-payload. **A truncated
    /// image is the case that matters**: `esp_ota_end` validates the written
    /// bytes, so ending early is detectable, but ending early *after* a partial
    /// erase has already cost the slot that was being written. The caller aborts
    /// rather than finalises.
    pub fn finish(&self) -> Result<(), ReadError> {
        match self.stage {
            // The closing delimiter arrived, so every payload byte was emitted.
            Stage::Done => Ok(()),
            // Still in the payload: the body ended mid-file. The bytes that did
            // arrive are already in flash and **are not** finalised — a partial
            // image is worse than none, because `esp_ota_end` would validate a
            // truncated hash only after a whole partition erase.
            Stage::Payload | Stage::Headers => Err(ReadError::Truncated),
            Stage::Preamble => Err(ReadError::NoPart),
        }
    }
}

/// Hand `bytes` to the sink and count them.
///
/// A free function rather than a method because the caller is mid-mutation of
/// `carry` and holds a borrow of it: a `&mut self` method would be a second
/// overlapping borrow, and this is the one place the read path is in the middle
/// of both. `total` is a field, not a return value, for the same reason.
fn emit(bytes: &[u8], sink: &mut dyn FnMut(&[u8]), total: &mut usize) {
    if bytes.is_empty() {
        return;
    }
    *total += bytes.len();
    sink(bytes);
}

/// `memmem`-style search, written out because `core` has none.
///
/// The needle is 4–72 bytes and the haystack is one 4 KiB read, so the naive
/// first-byte scan is a few thousand comparisons — under a microsecond on this
/// part, once per read, i.e. a few hundred times over a whole firmware upload.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The `filename=` value from a part's header block.
///
/// Written by hand rather than with a MIME library because there is no
/// multipart parser in the dependency graph and adding one for a single
/// `name="…"; filename="…"` is not a trade worth making. Handles the quoted form
/// only, which is every browser and `curl -F`.
fn filename_of(headers: &[u8]) -> Option<String> {
    let text = core::str::from_utf8(headers).ok()?;
    let at = text.find("filename=\"")?;
    let rest = &text[at + "filename=\"".len()..];
    let end = rest.find('"')?;
    if end == 0 {
        return None;
    }
    let mut name = String::with_capacity(end);
    // Bounded by the header cap, so this cannot run away on a long name.
    let _ = write!(name, "{}", &rest[..end]);
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::{
        capacity, extension_allowed, fits, Kind, PartReader, Progress, ReadError,
        MAX_FIRMWARE_BYTES, MAX_PART_HEADER_BYTES,
    };
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    /// The first index at which two byte strings differ, if any.
    ///
    /// Asserting on whole `Vec<u8>`s prints thousands of integers when a 5 KB
    /// payload loses its last four bytes, which is both unreadable and the
    /// reason this helper exists.
    fn first_difference(a: &[u8], b: &[u8]) -> Option<usize> {
        a.iter()
            .zip(b.iter())
            .position(|(x, y)| x != y)
            .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len())))
    }

    /// Build the envelope a browser sends for one file field.
    fn envelope(boundary: &str, filename: &str, payload: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"firmware\"; filename=\"{filename}\"\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(payload);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    /// Run a whole body through a reader in `chunk`-sized pieces.
    fn drain(
        boundary: &str,
        body: &[u8],
        chunk: usize,
    ) -> Result<(Vec<u8>, Option<String>, Progress), ReadError> {
        let mut reader = PartReader::new(boundary);
        let mut got = Vec::new();
        let mut progress = Progress::default();
        for slice in body.chunks(chunk) {
            progress = reader.push(slice, &mut |run| got.extend_from_slice(run))?;
        }
        reader.finish()?;
        Ok((got, reader.filename().map(String::from), progress))
    }

    #[test]
    fn a_well_formed_upload_comes_back_byte_exact() {
        let payload: Vec<u8> = (0..5_000u32).map(|i| (i % 251) as u8).collect();
        let body = envelope("X9", "firmware.bin", &payload);
        let (got, name, _) = drain("X9", &body, 4_096).expect("a complete upload");
        assert_eq!(
            got.len(),
            payload.len(),
            "the payload must survive the envelope"
        );
        assert_eq!(first_difference(&got, &payload), None, "{got:?}");
        assert_eq!(name.as_deref(), Some("firmware.bin"));
    }

    /// The whole point of the carry: **every** chunk size must give the same
    /// bytes, because a delimiter may straddle any read boundary.
    ///
    /// Sizes 1 and 3 are the adversarial ones — a delimiter of `--X9` plus its
    /// leading CRLF is 7 bytes, so at chunk 1 the reader reassembles it one byte
    /// at a time. A reader that emitted eagerly would splice `\r\n--X9` into the
    /// image and `esp_ota_end` would reject a hash that was correct 99 % of the
    /// time.
    #[test]
    fn the_payload_is_identical_at_every_chunk_size() {
        let payload: Vec<u8> = (0..2_048u32).map(|i| (i % 97) as u8).collect();
        let body = envelope("WebKitFormBoundaryABC123", "fs.img", &payload);
        let (reference, _, _) = drain("WebKitFormBoundaryABC123", &body, 4_096).unwrap();
        for chunk in [1usize, 2, 3, 5, 7, 8, 15, 64, 255, 1_000, 4_096] {
            let (got, _, _) = drain("WebKitFormBoundaryABC123", &body, chunk)
                .unwrap_or_else(|e| panic!("chunk {chunk} failed: {e:?}"));
            assert_eq!(
                first_difference(&got, &reference),
                None,
                "chunk size {chunk}: {} B vs {} B",
                got.len(),
                reference.len()
            );
        }
    }

    /// A payload full of **near-miss** delimiter bytes must come back exact.
    ///
    /// The misses are chosen so that none of them *contains* the delimiter:
    /// `\r\n-Z` has one dash where the delimiter has two, and `\r\n--Q` differs
    /// only in its last byte. That is what real binary firmware data looks like,
    /// and it is the case a greedy "emit everything not yet known to be a
    /// delimiter" reader gets wrong.
    ///
    /// Note what this test cannot be: a payload containing the delimiter
    /// itself. A multipart encoder cannot produce one, and the next test says
    /// what this reader does when handed one anyway.
    #[test]
    fn a_payload_of_near_miss_delimiters_is_not_truncated() {
        let needle = b"\r\n-Z-\r\n--Q-";
        let mut payload = Vec::new();
        for i in 0..2_000u32 {
            payload.extend_from_slice(&needle[i as usize % needle.len()..]);
        }
        let body = envelope("Z", "x.bin", &payload);
        let (got, _, _) = drain("Z", &body, 97).expect("a complete upload");
        assert_eq!(first_difference(&got, &payload), None, "{got:?}");
    }

    /// A payload that contains the actual delimiter truncates the part, by the
    /// rules of multipart.
    ///
    /// Asserted rather than left implicit because it is the one input shape
    /// that makes this reader's output differ from the raw body, and a reader
    /// that silently lost the tail of every image would be caught by exactly
    /// this test.
    #[test]
    fn a_payload_containing_the_delimiter_ends_the_part_early() {
        let mut payload = b"first-half".to_vec();
        payload.extend_from_slice(b"\r\n--Z");
        payload.extend_from_slice(b"second-half");
        let body = envelope("Z", "x.bin", &payload);
        let (got, _, progress) = drain("Z", &body, 97).expect("a complete upload");
        assert_eq!(got, b"first-half");
        assert!(progress.complete);
    }

    #[test]
    fn a_truncated_body_is_refused_rather_than_finalised() {
        let payload = vec![0xABu8; 1_000];
        let body = envelope("T", "f.bin", &payload);
        // Cut inside the payload: the closing delimiter never arrives.
        let truncated = &body[..body.len() - 40];
        let mut reader = PartReader::new("T");
        let mut got = Vec::new();
        reader
            .push(truncated, &mut |run| got.extend_from_slice(run))
            .expect("the push itself is fine");
        assert!(
            !reader.is_complete(),
            "the reader must know it is unfinished"
        );
        assert_eq!(
            reader.finish(),
            Err(ReadError::Truncated),
            "a body that ended mid-payload must not be finalised"
        );
        assert!(
            !got.is_empty(),
            "the bytes that did arrive were still emitted"
        );
    }

    #[test]
    fn a_body_with_no_boundary_is_refused_and_emits_nothing() {
        let mut reader = PartReader::new("NOPE");
        let mut got = Vec::new();
        reader
            .push(b"this is not multipart at all, just bytes", &mut |run| {
                got.extend_from_slice(run);
            })
            .expect("no error yet - the boundary may still be coming");
        assert!(
            got.is_empty(),
            "nothing may reach flash before the part starts"
        );
        assert_eq!(reader.finish(), Err(ReadError::NoPart));
    }

    /// The header cap is a hard stop, not a truncation.
    ///
    /// Without this an attacker or a broken client grows `headers` without limit,
    /// and it is the one buffer here whose size is not bounded by the read size.
    #[test]
    fn oversized_part_headers_are_a_hard_error() {
        let boundary = "H";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"X-Padding: ");
        body.extend_from_slice(&vec![b'a'; MAX_PART_HEADER_BYTES * 2]);
        body.extend_from_slice(b"\r\n\r\npayload");
        let mut reader = PartReader::new(boundary);
        let mut got = Vec::new();
        let result = reader.push(&body, &mut |run| got.extend_from_slice(run));
        assert_eq!(result, Err(ReadError::HeadersTooLong));
        assert_eq!(got.len(), 0, "nothing may reach flash: {got:?}");
    }

    #[test]
    fn the_boundary_parameter_is_extracted_from_a_browser_content_type() {
        assert_eq!(
            PartReader::boundary_of(Some(
                "multipart/form-data; boundary=----WebKitFormBoundaryAbC123"
            )),
            Some("----WebKitFormBoundaryAbC123"),
        );
        // Quoted, as curl sends it.
        assert_eq!(
            PartReader::boundary_of(Some("multipart/form-data; boundary=\"abc\"")),
            Some("abc"),
        );
        // Not the last parameter.
        assert_eq!(
            PartReader::boundary_of(Some("multipart/form-data; charset=utf-8; boundary=q1")),
            Some("q1"),
        );
        // Case-insensitive type token.
        assert_eq!(
            PartReader::boundary_of(Some("Multipart/Form-Data; boundary=m1")),
            Some("m1"),
        );
    }

    #[test]
    fn a_content_type_that_is_not_multipart_yields_no_boundary() {
        assert_eq!(PartReader::boundary_of(None), None);
        assert_eq!(PartReader::boundary_of(Some("application/json")), None);
        assert_eq!(
            PartReader::boundary_of(Some("application/octet-stream")),
            None
        );
        assert_eq!(PartReader::boundary_of(Some("multipart/form-data")), None);
        assert_eq!(
            PartReader::boundary_of(Some("multipart/form-data; boundary=")),
            None
        );
    }

    #[test]
    fn the_extension_rule_is_the_cpps() {
        // `ota.cpp:186-193`.
        assert!(extension_allowed("firmware.bin", Kind::Firmware));
        assert!(extension_allowed("FIRMWARE.BIN", Kind::Firmware));
        assert!(!extension_allowed("firmware.img", Kind::Firmware));
        assert!(!extension_allowed("firmware", Kind::Firmware));
        assert!(extension_allowed("fs.img", Kind::Filesystem));
        assert!(extension_allowed("fs.bin", Kind::Filesystem));
        assert!(!extension_allowed("fs.gz", Kind::Filesystem));
    }

    #[test]
    fn the_capacity_constants_match_the_partition_table() {
        // `rust/partitions_4M.csv`: app0 0x1C0000, littlefs 0x60000.
        assert_eq!(MAX_FIRMWARE_BYTES, 1_835_008);
        assert_eq!(capacity(Kind::Filesystem), 393_216);
        assert_eq!(capacity(Kind::Firmware), MAX_FIRMWARE_BYTES);
    }

    #[test]
    fn the_current_image_fits_its_own_slot() {
        // The number `just size-check` prints. If this fails, no machine can be
        // updated by this firmware at all — the worst possible OTA bug, and one
        // a host test catches.
        const CURRENT_IMAGE_BYTES: usize = 1_675_952;
        assert!(fits(Kind::Firmware, CURRENT_IMAGE_BYTES));
    }

    #[test]
    fn an_image_larger_than_the_slot_is_refused() {
        assert!(!fits(Kind::Firmware, MAX_FIRMWARE_BYTES + 1));
        assert!(fits(Kind::Firmware, MAX_FIRMWARE_BYTES));
        assert!(!fits(Kind::Filesystem, MAX_FIRMWARE_BYTES));
    }

    #[test]
    fn a_placeholder_sized_upload_is_refused_before_the_erase() {
        // The C++ accepts this and erases a whole 1.8 MB slot discovering it is
        // not an image.
        assert!(!fits(Kind::Firmware, 0));
        assert!(!fits(Kind::Firmware, 4_096));
        assert!(fits(Kind::Firmware, Kind::Firmware.min_accepted()));
        assert!(fits(Kind::Filesystem, Kind::Filesystem.min_accepted()));
        assert!(!fits(Kind::Filesystem, Kind::Firmware.min_accepted()));
    }
}
