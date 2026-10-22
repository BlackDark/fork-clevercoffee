//! Monotonic ESP-IDF clock through `esp_timer_get_time()`.
//!
//! The FFI has no caller preconditions and is documented for any task or ISR;
//! local safety notes justify the call. A `GPTimer` alternative adds a wrapping
//! counter, and this target's `Instant` has no documented clock.
//!
//! These integer-only helpers are not called from ISRs. Keep floating-point code
//! out of level-1 ISRs: Xtensa does not save coprocessor state there.

/// Microseconds since boot, truncated to 31 bits.
///
/// The low 31 bits, not the low 32, because that is what
/// `cc_protocol::sensor::tsic306::ring::EdgeRing` packs alongside an edge level in
/// one `AtomicU32` — the original ESP32 has no `AtomicU64`. The 35.8-minute
/// wrap is harmless: every subtraction in the `ZACwire` decoder is a
/// `saturating_sub` over a 2.75 ms burst, so a wrap inside a burst can only
/// *reject* a frame, never mis-decode one.
#[allow(
    unsafe_code,
    clippy::cast_possible_truncation,
    reason = "esp_timer_get_time() is a no-precondition, ISR-safe ESP-IDF \
              function and this HAL exposes no safe clock. See this module's \
              docs; the exception needs a human to ratify it."
)]
#[must_use]
pub fn now_us() -> u32 {
    let micros = unsafe { esp_idf_svc::sys::esp_timer_get_time() };
    // The count is non-negative by contract, so only the width is lost.
    #[allow(
        clippy::cast_sign_loss,
        reason = "esp_timer_get_time() is documented as returning a \
                  non-negative microsecond count"
    )]
    ((micros as u32) & 0x7FFF_FFFF)
}

/// Milliseconds since boot.
///
/// Every `cc_net::resilience` and `cc_net::wifi` entry point takes
/// one of these as its `now_ms`.
///
/// A full 32 bits wrap every 49.7 days. Every deadline in this firmware is
/// expressed with `wrapping_sub`, so a wrap is a subtraction that goes the right
/// way round rather than a panic or a 49-day stall — `cc_domain::units` pins that
/// with `millis_since_wraps_like_unsigned_long`.
#[allow(
    unsafe_code,
    reason = "the same esp_timer_get_time() call as `now_us`; see this module's \
              docs"
)]
#[must_use]
pub fn now_ms() -> u32 {
    // The 31-bit truncation of `now_us` is deliberately NOT reused: that would
    // wrap at 35.8 minutes, and a 49.7-day wrap is a property this firmware can
    // live with while a 36-minute one is not. So the full width is read again
    // rather than derived, at the cost of one redundant FFI read per call.
    let micros = unsafe { esp_idf_svc::sys::esp_timer_get_time() };
    #[allow(
        clippy::cast_sign_loss,
        reason = "esp_timer_get_time() is documented as returning a \
                  non-negative microsecond count"
    )]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the truncation is the point: a u32 millisecond count wraps \
                  every 49.7 days, which is what millis() does"
    )]
    ((micros / 1_000) as u32)
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
    pub fn the_clock_advances_and_never_goes_backwards() {
        // Reject unit errors and a missing /1000 conversion. Use a long delay and a
        // wide band: the 100 Hz scheduler quantizes sleeps to 10 ms ticks. A 20 ms
        // request has measured 12 ms; this test does not establish delay accuracy.
        const REQUEST_MS: u32 = 200;
        const LOW_MS: u32 = 100;
        const HIGH_MS: u32 = 600;

        let a = now_ms();
        esp_idf_hal::delay::FreeRtos::delay_ms(REQUEST_MS);
        let b = now_ms();
        let elapsed = b.wrapping_sub(a);
        assert!(
            (LOW_MS..=HIGH_MS).contains(&elapsed),
            "{REQUEST_MS} ms of FreeRTOS sleep measured {elapsed} ms on the \
             esp_timer clock -- outside {LOW_MS}..={HIGH_MS}"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_microsecond_clock_is_finer_than_the_millisecond_one() {
        // The ZACwire decoder needs µs (app note §1.4's 7.8 µs bit period) and
        // the network tier needs ms. If `now_us` were derived from `now_ms` it
        // would be quantised to 1000 µs and the decoder would be unusable.
        assert!(
            now_us() > 0 || now_ms() < 1,
            "both clocks read zero only at boot"
        );
    }
}
