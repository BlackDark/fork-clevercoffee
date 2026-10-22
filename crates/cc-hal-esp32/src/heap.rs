//! Heap gauges for the limits in ADR-0002.
//!
//! See `docs/adr/0002-wifi-logging-ota-memory-architecture.md`: stream large JSON
//! without intermediate strings, and shed telnet writes under pressure while
//! keeping the connection open. Active close would look like a network failure.

/// The free-heap floor below which the Wi-Fi log stream stops writing.
///
/// [ADR-0002](../../../docs/adr/0002-wifi-logging-ota-memory-architecture.md)
/// decision 5: 30 KB. The same number guards the large-HTTP-response path
/// ([`crate::web::HEAP_FLOOR_BYTES`]) — one threshold, several subscribers,
/// because several constants for "the machine is tight" is how they drift.
///
/// **Re-exported, not defined here.** The value moved to
/// [`cc_net::web::telnet::SHED_FLOOR_BYTES`] when the log stream's policy moved
/// there: the shed decision is host-testable and a heap
/// gauge is not, so the *policy* is portable and this module is the one place
/// that reads the gauge and hands it over.
pub use cc_net::web::telnet::SHED_FLOOR_BYTES as HEAP_SHED_BYTES;

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
        // ADR-0002 decision 5. A test because it is a
        // judgement call someone will otherwise "tidy" to 32768, or to
        // HEAP_SHED_BYTES / 2, without reading the ADR that explains it.
        // The literal is now in cc-net, which is where the policy lives; this
        // pins the re-export rather than the definition.
        assert_eq!(HEAP_SHED_BYTES, 30_000);
        assert_eq!(HEAP_SHED_BYTES, cc_net::web::telnet::SHED_FLOOR_BYTES);
    }
}
