//! Heap gauges for the limits in ADR-0002.
//!
//! See `docs/adr/0002-wifi-logging-ota-memory-architecture.md`: stream large JSON
//! without intermediate strings, and shed telnet writes under pressure while
//! keeping the connection open. Active close would look like a network failure.

/// Free bytes on the heap right now.
///
/// `/api/nvs-debug` reports it; the telnet log stream and the large HTTP
/// responses compare it against [`cc_net::web::HEAP_FLOOR_BYTES`].
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
