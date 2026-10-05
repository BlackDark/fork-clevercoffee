//! `ZACwire` timings and frame layout, as the IST AG app note states them.
//!
//! Owner: **R3-07**.
//!
//! # Source, and what "as the app note states them" excludes
//!
//! Everything in this file is transcribed from `ATTSic_E2.3.0.pdf` §1.1–§1.4
//! (summarised in [02 §6](../../../docs/rust-migration/02-research-compatibility-matrix.md))
//! and cross-read against the two implementations in this tree: the
//! `ZACwire` 2.0.0 library `TempSensorTSIC` wraps
//! (`.pio/libdeps/esp32_usb/ZACwire for TSic/ZACwire.cpp`) and the app note's
//! own worked example, which the C++ does not implement and which is the only
//! published ground truth for the frame's *content*.
//!
//! The tolerances in [`tolerances`] are the exception and are flagged as such:
//! the app note publishes no clock-tolerance figure, so they are this port's
//! choice, derived in that module's docs.
//!
//! # The waveform, in one paragraph
//!
//! The line idles **high**. Every bit begins with a **falling edge**, and the
//! *width of the low pulse* carries the symbol as a duty cycle of the 125 µs
//! bit window: 50 % for the start bit, 75 % for a `1`, 25 % for a `0`. Because
//! every bit is re-synchronised by its own falling edge, timing error does not
//! accumulate down the packet. This is the app note's model and it is what
//! [`decode`](super::decode) implements.
//!
//! # What the C++ does instead, and why this is not a port of `ZACwire`
//!
//! `ZACwire::read` (`ZACwire.cpp:80-101`) is an interrupt on the **rising**
//! edge that measures the **interval** between consecutive rising edges and
//! compares it against a learned threshold, with a "add half a threshold if the
//! previous bit was 0" normalisation and a run of "first 4 bits are always 0"
//! skipped at the top of every frame. That is a *different decoder for the same
//! waveform*, and it works. This port implements the app note's decoder instead
//! because:
//!
//! * it is **specified**, so it can be reviewed against a document rather than
//!   against index arithmetic in a third-party library;
//! * it needs only a falling-edge reference and a measured strobe, so the
//!   decoder consumes a list of edge timestamps and nothing else — which is what
//!   makes it host-testable at all;
//! * its one hard requirement (a measured `Tstrobe`) is measurable from the
//!   waveform itself, so a wrong clock is a rejected frame rather than a
//!   plausible temperature.
//!
//! What the port keeps from `ZACwire` is its *policy*, not its arithmetic: the
//! 221/222 sentinels, the adaptive change rate, the backup-buffer retry and the
//! heartbeat timeout all live in [`super`], not here.

/// The nominal bit window, in microseconds.
///
/// "The nominal baud rate is 8 kHz (125 µsec bit window)" (app note §1.1).
/// Used here **only** for the tolerance checks and for the simulator — the
/// decoder measures the period from the waveform and never assumes 125.
pub const BIT_WINDOW_US: u32 = 125;

/// The nominal strobe, `Tstrobe`, in microseconds.
///
/// "For standard `TSic` sensors, the value of Tstrobe is known in advance and is
/// equal to 125/2 = 62.5 µs" — it is half the bit window, because the start bit
/// is 50 % duty. [`tolerances::STROBE_MIN_US`] / [`tolerances::STROBE_MAX_US`]
/// bound it, and the decoder uses the **measured** value, never this one.
pub const STROBE_US: u32 = 62;

/// The app note's minimum sampling rate for acquiring `Tstrobe`, in hertz.
///
/// "It is recommended, however, that the sampling rate of the `ZACwire` signal
/// when acquiring the start bit be at least 16x the nominal baud rate. Because
/// the nominal baud rate is 8 kHz, a 128 kHz sampling rate is recommended when
/// acquiring Tstrobe" (app note §1.3).
///
/// 16 x 8 kHz = 128 kHz, i.e. a resolution of **7.8 µs**.
///
/// The device crate meets this with `esp_timer_get_time()`, which is a 1 µs
/// register read on the APB clock: **1 MHz effective, 7.8x the required rate**.
/// That is the concrete timing constraint this protocol imposes, and it is the
/// reason the edge capture timestamps rather than counting: a counter clocked at
/// `BIT_WINDOW_US / 8` would be 15.6 µs per tick and would put the measured
/// strobe up to 8 ticks out, which is a 12.8 % error on a quantity the decision
/// boundary is compared against. See [`super`] for the arithmetic.
pub const STROBE_SAMPLE_RATE_HZ: u32 = 128_000;

/// The data bits in one packet: eight, MSB first.
///
/// "a start bit, 8 data bits, and a parity bit … followed by the data bits (MSB
/// first, LSB last)" (app note §1.1).
pub const DATA_BITS: u8 = 8;

/// The bits in one packet: start, eight data, and the parity bit.
pub const PACKET_BITS: u8 = DATA_BITS + 2;

/// The number of bits in a full temperature transmission, in falling-edge order.
///
/// ```text
///   0        1..=8        9          10       11..=18   19
///   start1   data_high    parity1   start2   data_low  parity2
/// ```
///
/// Ten bits per packet, two packets, twenty falling edges. There is **no** bit
/// for the stop; see [`STOP_GAP_WINDOWS`].
pub const TRANSMISSION_BITS: u8 = 20;

/// How many bit windows separate packet 1's parity bit from packet 2's start bit.
///
/// "There is a single bit window of high signal (stop bit) between the end of the
/// first transmission and the start of the second transmission" (app note §1.1).
///
/// The stop bit is a window of *high*, so it produces **no edges at all**. The
/// decoder therefore sees a **two-window** gap between falling edges 9 and 10
/// where every other gap is one window, and that gap *is* the stop-bit check. A
/// missing stop bit collapses it to one window and a doubled one makes it three,
/// and both are rejected — see `a_missing_stop_bit_is_rejected`.
pub const STOP_GAP_WINDOWS: u32 = 2;

/// The TSIC-306's digital-output span, in °C, and its bottom.
///
/// `T = (DS / 2047) · (HT − LT) + LT` with `HT = +150` and `LT = −50` for the
/// TSIC 30x family (app note). So `T = DS / 2047 · 200 − 50`.
pub const RANGE_SPAN_C: f32 = 200.0;
/// `LT` for the TSIC 30x family: −50 °C.
pub const RANGE_MIN_C: f32 = -50.0;
/// The denominator: `2047`, i.e. one more than the 11-bit maximum.
pub const RANGE_STEPS: f32 = 2047.0;

/// The sentinel `ZACwire` returns when the probe is not transmitting.
///
/// `ZACwire.h:30`, `errorNotConnected {221}`. `TempSensorTSIC.cpp:51-54` logs
/// `"Temperature sensor not connected"` and returns false.
pub const ERROR_NOT_CONNECTED: f32 = 221.0;

/// The sentinel `ZACwire` returns when a frame did not decode or was rate-limited.
///
/// `ZACwire.h:31`, `errorMisreading {222}`. `TempSensorTSIC.cpp:46-49` logs
/// `"Temperature reading failed"` and returns false.
pub const ERROR_MISREADING: f32 = 222.0;

/// The maximum change allowed per reading before the signal is suspected.
///
/// `INITIAL_CHANGERATE` (`TempSensorTSIC.cpp:11`), passed to
/// `ZACwire::getTemp(maxChangeRate)`.
pub const INITIAL_CHANGE_RATE_C: f32 = 200.0;

/// The change rate the driver latches onto once the signal has settled.
///
/// `RUNTIME_CHANGERATE` (`TempSensorTSIC.cpp:12`).
pub const RUNTIME_CHANGE_RATE_C: f32 = 5.0;

/// The physical range the C++ accepts, exclusive at both ends.
///
/// `temp <= 0.0 || temp >= 180.0` (`TempSensorTSIC.cpp:59-62`).
///
/// **Inclusive rejection**: a reading of exactly 0.00 °C is rejected, and so is
/// one of exactly 180.0 °C. The TSIC-306 can legitimately report both (its span
/// is −50..+150, so 0.0 is squarely inside it), so this rejects a real reading
/// at the bottom of the range. Preserved verbatim, and flagged: it is a
/// deliberate, documented trade in the C++ — see [`super`].
pub const ACCEPT_MIN_C: f32 = 0.0;
/// See [`ACCEPT_MIN_C`].
pub const ACCEPT_MAX_C: f32 = 180.0;

/// The heartbeat timeout after which the probe counts as not connected, in
/// microseconds.
///
/// # 🔴 Deliberately not the C++'s 100 ms
///
/// `ZACwire.h:29` is `const uint8_t timeout {100}`, and
/// `ZACwire::connectionCheck` (`ZACwire.cpp:104-113`) returns false — the 221
/// path — when no completed frame has arrived for longer than that.
///
/// The sensor transmits **at 10 Hz** (app note §1.4: "The update rate of the
/// `TSic` is programmed to 10Hz"). A 100 ms no-frame timeout is therefore *one
/// transmission period*, with zero margin: any scheduling jitter, any frame lost
/// to a transient, and the driver reports a probe that is present and working as
/// disconnected. It is a real fragility in the C++ and it is a false negative on
/// a safety input.
///
/// This port uses **2.5 transmission periods**. That is long enough that a single
/// missed or corrupted frame cannot be mistaken for a missing probe, and short
/// enough that a genuinely unplugged probe is still reported inside the
/// temperature path's own 400 ms cadence (`Timing::TEMPERATURE_SENSOR_INTERVAL_MS`)
/// plus S1's ten-read debounce. Recorded in `intentional-diffs.md`.
pub const NO_SIGNAL_TIMEOUT_US: u32 = 250_000;

/// The sensor's own update period, in microseconds. 10 Hz (app note §1.4).
pub const UPDATE_PERIOD_US: u32 = 100_000;

/// The tolerances, which the app note does not publish.
///
/// # Why these numbers, stated as an argument rather than as a constant
///
/// The app note gives nominal timings and a mechanism but **no** tolerance
/// figures, so the windows below are this port's choice. They are not arbitrary
/// and they are not generous:
///
/// * **The strobe window** exists to answer one question: *is this falling edge
///   the start of a packet, or is it a data bit?* A data bit's low pulse is
///   31.25 µs (`0`) or 93.75 µs (`1`) against a 62.5 µs start pulse. Any window
///   that contains 62.5 and excludes both 31.25 and 93.75 answers the question
///   correctly for an *undistorted* pulse. [`tolerances::STROBE_MIN_US`] = 45 and
///   [`tolerances::STROBE_MAX_US`] = 80 do that with 13 µs of clearance on each
///   side, and they also admit a sensor running ±28 % off nominal — far more than
///   any real TSIC, which is specified to a few per cent.
/// * **The bit-period window** answers *did an edge go missing, or arrive twice?*
///   A missing edge doubles a period (250 µs) and a spurious one halves it
///   (62 µs); both are far outside [`tolerances::BIT_PERIOD_MIN_US`] ..
///   [`tolerances::BIT_PERIOD_MAX_US`] = 94 .. 156, which is 125 µs ±25 %.
/// * **The stop gap** is checked exactly, not with a tolerance: it is
///   [`STOP_GAP_WINDOWS`] bit windows, and the only uncertainty is the same
///   per-bit clock error, so the window is the bit-period window widened by half
///   a period in each direction.
///
/// A window that is *too wide* here is the dangerous direction, because it lets a
/// mis-framed frame through as a plausible temperature. That is why every
/// rejection in [`super::decode`] is tested against a waveform that was
/// deliberately damaged rather than only against a good one.
/// The tolerance windows, which the app note does not publish.
///
/// # Why these numbers, stated as an argument rather than as a constant
///
/// The app note gives nominal timings and a mechanism but **no** tolerance
/// figures, so the windows below are this port's choice. They are not arbitrary
/// and they are not generous:
///
/// * **The strobe window** exists to answer one question: *is this falling edge
///   the start of a packet, or is it a data bit?* A data bit's low pulse is
///   31.25 µs (`0`) or 93.75 µs (`1`) against a 62.5 µs start pulse. Any window
///   that contains 62.5 and excludes both 31.25 and 93.75 answers the question
///   correctly for an *undistorted* pulse. [`tolerances::STROBE_MIN_US`] = 45 and
///   [`tolerances::STROBE_MAX_US`] = 80 do that with 13 µs of clearance on each
///   side, and they also admit a sensor running ±28 % off nominal — far more than
///   any real TSIC, which is specified to a few per cent.
/// * **The bit-period window** answers *did an edge go missing, or arrive twice?*
///   A missing edge doubles a period (250 µs) and a spurious one halves it
///   (62 µs); both are far outside [`tolerances::BIT_PERIOD_MIN_US`] ..
///   [`tolerances::BIT_PERIOD_MAX_US`] = 94 .. 156, which is 125 µs ±25 %.
/// * **The stop gap** is checked exactly, not with a tolerance: it is
///   [`STOP_GAP_WINDOWS`] bit windows, and the only uncertainty is the same
///   per-bit clock error, so the window is the bit-period window widened by half
///   a period in each direction.
///
/// A window that is *too wide* here is the dangerous direction, because it lets a
/// mis-framed frame through as a plausible temperature. That is why every
/// rejection in `decode` is tested against a waveform that was deliberately
/// damaged rather than only against a good one.
pub mod tolerances {
    /// Lowest plausible measured start-bit low-pulse width, in microseconds.
    pub const STROBE_MIN_US: u32 = 45;
    /// Highest plausible measured start-bit low-pulse width, in microseconds.
    pub const STROBE_MAX_US: u32 = 80;
    /// Shortest plausible interval between two consecutive falling edges.
    pub const BIT_PERIOD_MIN_US: u32 = 94;
    /// Longest plausible interval between two consecutive falling edges.
    pub const BIT_PERIOD_MAX_US: u32 = 156;
    /// Narrowest plausible interval for the two-window stop gap.
    ///
    /// The gap is the *sum of two bit periods* — the stop bit's window and the
    /// period that follows it — so it carries twice the per-period uncertainty:
    /// `2 * BIT_PERIOD_MIN_US` = 188 µs, which is above one window (125) and
    /// below two (250).
    pub const STOP_GAP_MIN_US: u32 = 2 * BIT_PERIOD_MIN_US;
    /// Widest plausible stop gap: 312 µs, which is below three windows (375).
    ///
    /// So a *missing* stop bit (125) and a *doubled* one (375) are both outside,
    /// and the check is not vacuous.
    pub const STOP_GAP_MAX_US: u32 = 2 * BIT_PERIOD_MAX_US;
}

/// The duty cycles, as whole per cent of [`BIT_WINDOW_US`].
///
/// "The bit format is duty cycle encoded: Start bit => 50 % duty cycle used to
/// set up strobe time, Logic 1 => 75 % duty cycle, Logic 0 => 25 % duty cycle"
/// (app note §1.2).
pub mod duty {
    /// The start bit: 50 % — this is what sets up the strobe.
    pub const START_PCT: u32 = 50;
    /// A `1`: 75 %.
    pub const ONE_PCT: u32 = 75;
    /// A `0`: 25 %.
    pub const ZERO_PCT: u32 = 25;
}

/// The low-pulse width a duty cycle implies, in microseconds, **truncated**.
///
/// `(BIT_WINDOW_US * pct) / 100`, i.e. 31 / 62 / 93 for 0 / start / 1.
///
/// Truncated, not rounded, and the direction matters: the real widths are
/// 31.25 / 62.5 / 93.75 µs and cannot be represented at 1 µs, so a synthesised
/// pulse is up to 1 µs short of nominal. A short pulse is the safe direction for
/// every check in this crate — a short start bit is still inside the strobe
/// window, a short `0` is further from the decision boundary, and a short `1` is
/// closer to it, which is the one asymmetry, and is why
/// [`tolerances::STROBE_MAX_US`] is a separate number from
/// [`tolerances::STROBE_MIN_US`].
///
/// These exist for the simulator and for the tolerance arithmetic. The decoder
/// **measures** the start bit and never uses them.
#[must_use]
pub const fn low_us(pct: u32) -> u32 {
    (BIT_WINDOW_US * pct) / 100
}

#[cfg(test)]
#[allow(
    clippy::assertions_on_constants,
    reason = "these tests assert protocol constants against the app note's printed \
              numbers on purpose: that is the entire claim, and the compiler is \
              right that it is a constant comparison"
)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "test-side narrowing of values the protocol bounds to 0..=2047"
)]
mod tests {
    use super::*;

    /// The `Tstrobe` acquisition error `esp_timer_get_time()`'s 1 µs resolution
    /// implies: at most half a microsecond, rounded up to one.
    const ESP_TIMER_ERROR_US: u32 = 1;

    /// The separation between a `0` pulse and a `1` pulse, in microseconds:
    /// 93.75 - 31.25 = 62.5, the quantity the `pulse > strobe` decision is
    /// compared against.
    ///
    /// **This is the number a `Tstrobe` error has to stay small compared to**, and
    /// it is why the app note's 128 kHz requirement is the right one to hold the
    /// implementation to: a `Tstrobe` measured to ±3.9 µs still decides every
    /// pulse correctly, because no legal pulse lies within 31 µs of the boundary.
    const SEPARATION_US: u32 = 62;

    #[test]
    fn the_nominal_widths_are_the_ones_the_app_note_prints() {
        // App note §1.2 and §1.3.
        assert_eq!(BIT_WINDOW_US, 125, "8 kHz is a 125 us window");
        assert_eq!(
            low_us(duty::ZERO_PCT),
            31,
            "25 % of 125 is 31.25, truncated"
        );
        assert_eq!(low_us(duty::START_PCT), 62, "50 % of 125 is 62.5 = Tstrobe");
        assert_eq!(low_us(duty::ONE_PCT), 93, "75 % of 125 is 93.75, truncated");
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "the 0.1 C sentinels are the claim: they must be exactly what the \
                  C++ compares against"
    )]
    fn the_strobe_is_half_the_bit_window() {
        // "the value of Tstrobe is known in advance and is equal to 125/2".
        // `STROBE_US` is the *nominal* value and the decoder does not use it; the
        // measured value is what matters, and it lands on 62 because
        // [`low_us`] truncates 62.5.
        assert_eq!(STROBE_US, 62);
        assert_eq!(low_us(duty::START_PCT), STROBE_US);
        assert!(tolerances::STROBE_MIN_US < low_us(duty::START_PCT));
        assert!(tolerances::STROBE_MAX_US > low_us(duty::START_PCT));
    }

    #[test]
    fn the_tolerance_windows_separate_a_start_bit_from_a_data_bit() {
        // The whole point of the strobe window: a start bit's pulse and a data
        // bit's pulse must not be confusable. If these assertions ever fail, the
        // window no longer answers "is this a start bit?" and every frame is
        // either rejected outright or mis-framed.
        assert!(
            tolerances::STROBE_MIN_US > low_us(duty::ZERO_PCT),
            "a '0' pulse must fall below the strobe window"
        );
        assert!(
            tolerances::STROBE_MAX_US < low_us(duty::ONE_PCT),
            "a '1' pulse must fall above the strobe window"
        );
    }

    #[test]
    fn the_tolerance_windows_reject_a_missing_or_a_doubled_edge() {
        // A missed falling edge doubles the period; a spurious one halves it.
        // The two windows must not bracket either, and the two literal sums are
        // what a doubled and a halved period are.
        const DOUBLED: u32 = 2 * BIT_WINDOW_US;
        const HALVED: u32 = BIT_WINDOW_US / 2;
        assert_eq!(DOUBLED, 250);
        assert_eq!(HALVED, 62);
        assert!(tolerances::BIT_PERIOD_MAX_US < DOUBLED);
        assert!(tolerances::BIT_PERIOD_MIN_US > HALVED);
    }

    #[test]
    fn the_stop_gap_window_brackets_exactly_two_bit_windows() {
        // One, two and three windows: 125, 250, 375. The window must contain the
        // middle one and neither of the others, or the stop-bit check is either
        // vacuous or catches a *good* frame.
        const ONE: u32 = BIT_WINDOW_US;
        const TWO: u32 = 2 * BIT_WINDOW_US;
        const THREE: u32 = 3 * BIT_WINDOW_US;
        assert!(tolerances::STOP_GAP_MIN_US < TWO);
        assert!(tolerances::STOP_GAP_MAX_US > TWO);
        assert!(
            tolerances::STOP_GAP_MIN_US > ONE,
            "a missing stop bit must fail"
        );
        assert!(
            tolerances::STOP_GAP_MAX_US < THREE,
            "a doubled stop bit must fail"
        );
    }

    #[test]
    fn the_transmission_is_twenty_falling_edges() {
        // 1 start + 8 data + 1 parity, twice.
        const _: () = assert!(PACKET_BITS == 1 + 8 + 1);
        const _: () = assert!(TRANSMISSION_BITS == 2 * PACKET_BITS);
    }

    #[test]
    fn the_app_notes_sampling_rate_is_met_with_ample_margin() {
        // The constraint stated as the thing that actually matters: the error in
        // the measured `Tstrobe` must be small against **the separation between
        // a `0`'s pulse and a `1`'s pulse**, which is 93.75 - 31.25 = 62.5 us,
        // because the decision is `pulse > strobe`.
        assert_eq!(SEPARATION_US, 62, "about half the bit window, in us");

        // The app note asks for at least 16 x 8 kHz = 128 kHz, i.e. a 7.8 us
        // period and so up to 3.9 us of quantisation error.
        let required_period_ns = 1_000_000_000 / STROBE_SAMPLE_RATE_HZ;
        assert_eq!(required_period_ns, 7_812, "128 kHz is 7812 ns");
        let app_note_error_us = required_period_ns / 2 / 1_000; // +/- half a period
        assert_eq!(app_note_error_us, 3);

        // `esp_timer_get_time()` is 1 us, so +/- 0.5 us, rounded up to 1.
        assert!(ESP_TIMER_ERROR_US < SEPARATION_US / 4);
        assert!(
            ESP_TIMER_ERROR_US < app_note_error_us,
            "and this clock beats the app note's own minimum"
        );

        // And what a *counter* at the app note's rate would give, which is the
        // reason the capture timestamps rather than counts: 7.8 us ticks make
        // Tstrobe 8 ticks +/- 1, and half a tick of that is 3.9 us -- four times
        // the error of a plain timestamp, for a mechanism that also has to be
        // read out of an ISR.
        let counter_tick_ns = required_period_ns;
        let counter_error_us = counter_tick_ns / 2 / 1_000;
        assert_eq!(counter_error_us, 3);
        assert!(counter_error_us > ESP_TIMER_ERROR_US);
    }

    #[test]
    fn the_temperature_span_matches_the_datasheet() {
        // App note: T = DS/2047 * (HT - LT) + LT, HT = +150, LT = -50. These are
        // `const` asserts rather than `assert_eq!`s on purpose: they are claims
        // about the *constants*, and the compiler is right that a run-time
        // comparison of two constants is not a test.
        const HIGH_C: f32 = 150.0;
        const LOW_C: f32 = -50.0;
        const _: () = assert!(RANGE_SPAN_C == HIGH_C - LOW_C);
        const _: () = assert!(RANGE_MIN_C == LOW_C);
        const _: () = assert!(RANGE_STEPS == 2047.0);
        // And the accept window is strictly inside the sensor's own span at the
        // bottom, which is the `ACCEPT_MIN_C` trade.
        assert!(ACCEPT_MIN_C > LOW_C);
        assert!(ACCEPT_MAX_C > HIGH_C);
    }
}
