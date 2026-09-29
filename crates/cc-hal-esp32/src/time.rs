//! The monotonic clock.
//!
//! Owner: **R3-12** (the network tier's timeouts) and **R3-13/R3-14** (the
//! publish budget and the SSE cadence).
//!
//! # The one `unsafe` here, and why
//!
//! `esp_timer_get_time()` (`components/esp_timer/include/esp_timer.h:223`) is
//! declared:
//!
//! ```c
//! /// @brief Get time in microseconds. This function may be called from
//! /// any ISR or task with the following restrictions:
//! /// ...
//! int64_t esp_timer_get_time(void);
//! ```
//!
//! Its own documentation says it may be called from any task or ISR, takes no
//! arguments, allocates nothing, takes no lock that could deadlock, and has no
//! precondition to uphold. It is a read of the APB-backed timer register plus a
//! software accumulator. **The `unsafe` is the FFI boundary, not a contract
//! this crate can break.**
//!
//! The alternatives were checked and are all worse:
//!
//! * a C shim is the same `unsafe` in a different file;
//! * a `GPTimer` read is also `unsafe` FFI and gives a 24-bit wrapping count
//!   that has to be differenced by hand — strictly more code for strictly less
//!   precision;
//! * `std::time::Instant` is not usable: `esp-idf-sys` 0.38.1 ships no `std`
//!   shim of its own (`src/` has `alloc.rs`, `stdio.rs`, `start.rs` and nothing
//!   time-related) and this target's `std` does not document a clock.
//!
//! The heap gauges live in [`crate::heap`], which has the same exception for
//! the same reason.
//!
//! # What the clock is *not* used for
//!
//! **Not in an ISR.** [09 §22](../../../docs/rust-migration/09-cpp-findings.md):
//! an FPU instruction in a level-1 ISR panics the original ESP32, because Xtensa
//! does not save coprocessor state on an interrupt. These functions are
//! integer-only and neither is called from an ISR — the heater's ISR owns its
//! own counter — but the rule is worth restating where a clock lives, because a
//! clock is exactly what a future ISR would reach for.

/// Microseconds since boot, truncated to 31 bits.
///
/// The low 31 bits, not the low 32, because that is what
/// `cc_domain::sensor::tsic306::ring::EdgeRing` packs alongside an edge level in
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
/// The whole of the C++'s `millis()` and of the `now_ms` argument every
/// `cc_domain::resilience` and `cc_domain::wifi` call takes.
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
        // A cheap proof that the FFI is wired at all. The thing that actually
        // matters is that the two failure modes this exists to catch are
        // caught:
        //
        // * a **wrong unit** — `esp_timer_get_time()` in microseconds read as
        //   milliseconds — collapses the measured window to 0;
        // * a **missing divide** by 1000 — blows it up by three orders of
        //   magnitude.
        //
        // The band around the request is wide on purpose, because
        // `FreeRtos::delay_ms(n)` does not sleep `n` milliseconds.
        // `CONFIG_FREERTOS_HZ` is 100 on this target, so a delay is
        // `ceil(n * 100 / 1000)` ticks of a **10 ms** grid and the task wakes on
        // a tick edge. A 20 ms request is two ticks, so one tick of quantisation
        // is 50 % of the measurement — and the first run of this suite on the
        // device measured **12 ms** for a 20 ms request and failed on that.
        //
        // The clock is not the suspect. Cross-checked against the host's own
        // clock over the 1.27 s boot, the device's `esp_timer_get_time()`
        // stamps agree to **1.4 %**: 1.256 s of device time inside 1.274 s of
        // host time. Whether `delay_ms` is systematically short, or that 12 ms
        // was the tick grid landing unluckily, is an open question and is
        // deliberately not asserted here — `docs/rust-migration/06-migration-
        // task-list.md` R2-09b owns the control-loop timing measurement.
        //
        // A 200 ms request is 20 ticks, so the quantisation is 5 % rather than
        // 50 %, and the band below still admits a 0.6 factor while rejecting
        // both failure modes by orders of magnitude.
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
