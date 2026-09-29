//! The heap gauges, and the free-heap floor ADR-0002 set.
//!
//! Owner: **R3-14** (task E).
//!
//! # ADR-0002, and what it actually decided
//!
//! `docs/adr/0002-wifi-logging-ota-memory-architecture.md` records a reproducible
//! `abort()`: a user opened the web UI, the browser fired 6–10 parallel API
//! requests within two seconds of boot, and `/api/parameters?filter=all` — 19 KB
//! of JSON — was built three times over. Seven decisions followed; the two this
//! crate implements are:
//!
//! * **Decision 2:** stream large JSON, never build a `String` intermediate.
//!   That is [`crate::web`]'s [`respond_large`](crate::web).
//! * **Decision 5:** *"When `ESP.getFreeHeap() < 30 KB`, `writeToOutputs()`
//!   skips `WiFi` writes (Serial still works). This is a **soft shed** — the
//!   telnet connection stays open and resumes when heap recovers. **No active
//!   `client_.stop()`** to avoid the 'Connection reset by peer' problem."*
//!
//! The brief for R3-14 said the log client must be *disconnected* under heap
//! pressure. The ADR says the opposite and explains why: an active close
//! surfaces in the operator's terminal as "Connection reset by peer", which
//! looks like a network fault rather than the memory fault it is, and the
//! symptom is what gets debugged. The soft shed is what the C++ does
//! (`Logger.cpp:13` `MIN_HEAP_FOR_WIFI_LOG = 30000`, checked at `:64`), and it
//! is what is implemented here. What *is* honoured from the brief is the part
//! that matters: **under heap pressure the machine sheds, and it does not crash.**

/// The free-heap floor below which the Wi-Fi log stream stops writing.
///
/// ADR-0002 decision 5 and `Logger.cpp:13` `MIN_HEAP_FOR_WIFI_LOG = 30000`.
/// The same number guards the large-HTTP-response path
/// ([`crate::web::HEAP_FLOOR_BYTES`]) — one threshold, two subscribers, because
/// two different constants for "the machine is tight" is how they drift.
pub const HEAP_SHED_BYTES: u32 = 30_000;

/// Free bytes on the heap right now.
///
/// The ADR-0002 number. `/api/nvs-debug` reports it, and
/// [`crate::telnet::HeapShed`] compares it against [`HEAP_SHED_BYTES`].
#[allow(
    unsafe_code,
    reason = "esp_get_free_heap_size() is a documented, precondition-free read \
              of FreeRTOS heap bookkeeping and esp-idf-hal 0.47 exposes no safe \
              wrapper; the reasoning is written out here once"
)]
#[must_use]
pub fn free_heap() -> u32 {
    // Safety: `uint32_t esp_get_free_heap_size(void)` — no pointer, no
    // allocation, no lock, callable from any task.
    unsafe { esp_idf_svc::sys::esp_get_free_heap_size() }
}

/// The smallest [`free_heap`] seen since boot.
///
/// The reading that answers "was the machine ever tight", which `free_heap`
/// cannot, because it recovers.
#[allow(
    unsafe_code,
    reason = "esp_get_minimum_free_heap_size(); same reasoning as `free_heap`"
)]
#[must_use]
pub fn min_free_heap() -> u32 {
    // Safety: `uint32_t esp_get_minimum_free_heap_size(void)`.
    unsafe { esp_idf_svc::sys::esp_get_minimum_free_heap_size() }
}

/// Whether the heap is above [`HEAP_SHED_BYTES`].
#[must_use]
pub fn has_room() -> bool {
    free_heap() >= HEAP_SHED_BYTES
}

#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests {
    // A unit-test module globs its parent on purpose: the cases are exercising
    // the parent's private helpers, which is the point of keeping them in the
    // same file. `clippy::wildcard_imports` normally makes an exception for
    // `use super::*` inside a `#[cfg(test)]` module, and this module is
    // `#[cfg(any(test, feature = "device-tests"))]` -- the on-target runner
    // compiles it outside a test build -- so the exception no longer applies and
    // the allowance is made explicitly here instead of in eight import lists
    // that would rot.
    #![allow(clippy::wildcard_imports)]

    use super::*;

    #[cfg_attr(test, test)]
    pub fn the_shed_floor_is_the_adrs_thirty_kilobytes() {
        // ADR-0002 decision 5 and Logger.cpp:13. A test because it is a
        // judgement call someone will otherwise "tidy" to 32768, or to
        // HEAP_SHED_BYTES / 2, without reading the ADR that explains it.
        assert_eq!(HEAP_SHED_BYTES, 30_000);
    }
}
