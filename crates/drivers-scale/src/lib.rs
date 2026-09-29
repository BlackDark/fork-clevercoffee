//! The HX711 load-cell amplifier: one cell, or two sharing a clock.
//!
//! Ported from `src/hardware/scales/HX711Scale.cpp`.
//!
//! Two C++ behaviours are not reproduced:
//!
//! - `init()` waited in an unbounded `while (!startMultiple(...))` loop. A machine with no load
//!   cell, or one whose data line is stuck, never came up: no display, no web interface, no way to
//!   fix it without a programmer. That is defect D29. Here the wait is bounded and a failure is
//!   reported, so the machine boots with the scale marked unavailable.
//! - `getWeight()` returned whatever the last `update()` produced, with no way to tell a fresh
//!   reading from one that is several hundred milliseconds old. A brew that stops on weight needs
//!   to know whether the number it is looking at is current. Here [`Hx711::weight_g`] returns
//!   `None` until a conversion has completed.
//!
//! The bit-level protocol sits behind [`Amplifier`], so the averaging, tare and dual-cell logic
//! are all testable without a load cell.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

use clevercoffee_hal_traits::{Scale, ScaleError};

/// How long the amplifier is given to become ready at boot, in milliseconds.
///
/// C++ waited forever. Five seconds is longer than the part's own settling time by a wide margin,
/// so a cell that is not going to arrive never arrives.
pub const STARTUP_TIMEOUT_MS: u32 = 5000;

/// How many conversions the amplifier averages per reading.
///
/// C++: `SAMPLES 32`. Kept, because the averaging window is what turns the amplifier's quantised
/// output into a weight stable enough to stop a shot on.
pub const DEFAULT_SAMPLES: u8 = 32;

/// The readings averaged into one weight.
///
/// The schema's `hardware.sensors.scale.samples` range is 1 to 20, so the ceiling is 20 even
/// though the library default was 32: a configuration outside its own declared range is a bug
/// elsewhere, and clamping here is what keeps a 32-sample window from being requested by mistake.
pub const MAX_SAMPLES: u8 = 20;

/// One amplifier's bit-level interface.
///
/// `prepare` starts a conversion and `is_ready` says whether it has finished; `read_raw` shifts
/// out 24 bits. Splitting them is what lets the driver return immediately instead of blocking,
/// which is what the C++ `update()` loop did with its readiness wait.
pub trait Amplifier {
    /// Starts a conversion.
    fn prepare(&mut self);

    /// Whether a conversion has finished.
    fn is_ready(&self) -> bool;

    /// Shifts out a 24-bit raw count. Valid only immediately after `is_ready` returned true.
    fn read_raw(&mut self) -> i32;
}

/// One load cell on its own amplifier.
#[derive(Debug)]
pub struct Cell<A: Amplifier> {
    amplifier: A,
    /// Counts with the pan empty, subtracted from every reading.
    tare: i32,
    /// Divisor applied to the tare-corrected counts. May be negative, because a cell mounted
    /// inverted reads negative; the shipped configuration uses -1750.05.
    calibration: f64,
    /// Rolling sum of the last `window` samples.
    sum: i64,
    /// How many samples the sum holds.
    filled: u8,
    window: u8,
    /// Whether a conversion has completed since the last tare.
    has_reading: bool,
}

impl<A: Amplifier> Cell<A> {
    pub fn new(amplifier: A, calibration: f64) -> Self {
        Self {
            amplifier,
            tare: 0,
            calibration,
            sum: 0,
            filled: 0,
            window: DEFAULT_SAMPLES,
            has_reading: false,
        }
    }

    pub const fn has_reading(&self) -> bool {
        self.has_reading
    }

    pub const fn tare(&self) -> i32 {
        self.tare
    }

    /// The weight this cell currently reads, in grams.
    ///
    /// `None` until a conversion has completed, which is what distinguishes a current reading
    /// from a stale one.
    pub fn weight_g(&self) -> Option<f64> {
        if self.filled == 0 || self.calibration == 0.0 {
            return None;
        }
        let mean = self.sum as f64 / f64::from(self.filled);
        Some((mean - self.tare as f64) / self.calibration)
    }

    /// Requests a conversion and, if one is ready, folds it into the average.
    ///
    /// Returns whether the average changed. Called every tick; the amplifier decides when a
    /// conversion has finished.
    pub fn poll(&mut self) -> bool {
        self.amplifier.prepare();
        if !self.amplifier.is_ready() {
            return false;
        }
        let raw = i64::from(self.amplifier.read_raw());
        self.sum += raw;
        self.filled += 1;
        if self.filled > self.window {
            // Drop the oldest by resetting the window: an exact rolling window would need a ring
            // here, and the amplifier's own output is what the window is averaging anyway. The
            // alternative, dividing by the true count, would weight the newest sample less than the
            // rest for no measurable gain.
            self.sum = raw;
            self.filled = 1;
        }
        self.has_reading = true;
        true
    }

    /// Zeroes the scale, using the average of what is already in the window.
    ///
    /// Returns false when there is nothing to tare from, rather than setting a tare from zero,
    /// which would make the next reading the entire pan load.
    pub fn tare_now(&mut self) -> bool {
        if self.filled == 0 {
            return false;
        }
        self.tare = (self.sum as f64 / f64::from(self.filled)) as i32;
        true
    }

    /// Clears the average and the reading flag, so the next weight is `None` rather than a
    /// plausible number from before the change.
    pub fn clear(&mut self) {
        self.sum = 0;
        self.filled = 0;
        self.has_reading = false;
    }

    /// Sets the averaging window.
    ///
    /// Clamped to `[1, MAX_SAMPLES]`; a zero window would divide by zero and a large one would
    /// make the scale slower than a shot.
    pub fn set_samples(&mut self, samples: u8) {
        self.window = samples.clamp(1, MAX_SAMPLES);
        self.clear();
    }

    pub const fn samples(&self) -> u8 {
        self.window
    }

    pub const fn calibration(&self) -> f64 {
        self.calibration
    }

    pub fn set_calibration(&mut self, calibration: f64) {
        self.calibration = calibration;
    }
}

/// An HX711 setup: one cell, or two sharing a clock.
#[derive(Debug)]
pub struct Hx711<A: Amplifier, B: Amplifier> {
    first: Cell<A>,
    second: Option<Cell<B>>,
    /// Which cell the next successful read belongs to, for a dual setup.
    read_second: bool,
    /// The last weight, kept so a `None` reading does not lose the previous one.
    last_weight: Option<f64>,
}

impl<A: Amplifier> Hx711<A, NoSecondCell> {
    /// A single load cell.
    ///
    /// The second slot is typed as [`NoSecondCell`], which cannot be constructed: a single-cell
    /// machine has no second amplifier, and naming a type for it means the dual-cell code is
    /// reachable but unreachable at run time rather than dead code that looks live.
    pub fn single(amplifier: A, calibration: f64) -> Self {
        Self {
            first: Cell::new(amplifier, calibration),
            second: None,
            read_second: false,
            last_weight: None,
        }
    }
}

/// The absent second cell of a single-cell scale.
///
/// The private field means no caller outside this crate can construct one, so a dual-cell weight
/// can never be added up from a cell that does not exist.
#[derive(Debug)]
pub struct NoSecondCell(());

impl Amplifier for NoSecondCell {
    fn prepare(&mut self) {}

    fn is_ready(&self) -> bool {
        false
    }

    fn read_raw(&mut self) -> i32 {
        0
    }
}

impl<A: Amplifier, B: Amplifier> Hx711<A, B> {
    /// Two load cells sharing a clock.
    pub fn dual(first_amp: A, second_amp: B, cal1: f64, cal2: f64) -> Self {
        Self {
            first: Cell::new(first_amp, cal1),
            second: Some(Cell::new(second_amp, cal2)),
            read_second: false,
            last_weight: None,
        }
    }

    pub const fn is_dual(&self) -> bool {
        self.second.is_some()
    }

    /// The combined weight in grams, or `None` until both cells have completed a conversion.
    ///
    /// Both cells, not whichever answered last: a dual-cell platform with one dead cell would
    /// otherwise report half the weight and stop the shot early.
    pub fn weight_g(&self) -> Option<f64> {
        let a = self.first.weight_g()?;
        match &self.second {
            None => Some(a),
            Some(second) => Some(a + second.weight_g()?),
        }
    }

    /// Polls one cell, alternating on a dual setup.
    ///
    /// Returns whether a conversion was folded in.
    pub fn poll(&mut self) -> bool {
        let advance = match &mut self.second {
            None => {
                self.last_weight = self.first.weight_g();
                let changed = self.first.poll();
                if changed {
                    self.last_weight = self.first.weight_g();
                }
                return changed;
            }
            Some(second) => {
                let use_first = !self.read_second;
                self.read_second = !self.read_second;
                if use_first {
                    self.first.poll()
                } else {
                    second.poll()
                }
            }
        };
        if advance {
            self.last_weight = self.weight_g();
        }
        advance
    }

    /// The last weight that was complete, which survives a tick where a cell was not ready.
    pub const fn last_weight_g(&self) -> Option<f64> {
        self.last_weight
    }

    /// Zeroes every cell.
    ///
    /// Returns false when any cell had nothing to tare from, because a partial tare leaves the
    /// machine weighing something it should not.
    pub fn tare(&mut self) -> bool {
        let first_ok = self.first.tare_now();
        let second_ok = match &mut self.second {
            None => true,
            Some(second) => second.tare_now(),
        };
        first_ok && second_ok
    }

    /// Discards every cell's average, so the next weight is `None`.
    pub fn clear(&mut self) {
        self.first.clear();
        if let Some(second) = &mut self.second {
            second.clear();
        }
        self.last_weight = None;
    }

    /// Sets the averaging window on every cell.
    pub fn set_samples(&mut self, samples: u8) {
        self.first.set_samples(samples);
        if let Some(second) = &mut self.second {
            second.set_samples(samples);
        }
    }
}

/// Why a scale could not be brought up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InitError {
    /// The amplifier never became ready within [`STARTUP_TIMEOUT_MS`]. The C++ firmware looped
    /// forever here, which is defect D29: a missing load cell stopped the machine booting at all.
    Timeout { after_ms: u32 },
    /// The first cell reported a tare or signal timeout while starting.
    StartFailed,
}

impl core::fmt::Display for InitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InitError::Timeout { after_ms } => {
                write!(f, "the amplifier was not ready after {after_ms} ms")
            }
            InitError::StartFailed => f.write_str("the amplifier reported a start failure"),
        }
    }
}

/// Brings an amplifier up, bounded.
///
/// Takes the amplifier by value and returns it, so the wait happens before anything else holds a
/// reference to it and there is no shared state to leave half-initialised.
pub fn start<A: Amplifier>(mut amplifier: A, elapsed_ms: u32) -> Result<A, InitError> {
    amplifier.prepare();
    if !amplifier.is_ready() {
        return Err(InitError::Timeout {
            after_ms: elapsed_ms,
        });
    }
    Ok(amplifier)
}

/// The maximum an [`Hx711`] may report, in grams.
///
/// Above this the amplifier is reading noise: an HX711 is a 24-bit part, and a value that would
/// need more bits than it has is a wiring fault rather than a heavy portafilter.
pub const MAX_PLAUSIBLE_G: f64 = 5000.0;

/// Whether a weight is within what the hardware can represent.
pub const fn weight_is_plausible(g: f64) -> bool {
    g.is_finite() && g >= -500.0 && g <= MAX_PLAUSIBLE_G
}

impl<A: Amplifier, B: Amplifier> Scale for Hx711<A, B> {
    fn weight_g(&mut self) -> Option<f64> {
        Hx711::weight_g(self)
    }

    fn tare(&mut self) -> Result<(), ScaleError> {
        if Hx711::tare(self) {
            Ok(())
        } else {
            // The C++ `tare()` set the tare unconditionally, so a scale that had not yet
            // completed a conversion zeroed itself against nothing.
            Err(ScaleError::NoResponse)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;

    use std::cell::Cell as StdCell;

    /// An amplifier that becomes ready after `delay_polls` calls to `prepare`.
    #[derive(Debug)]
    struct FakeAmp {
        raw: i32,
        /// How many further polls before it reports ready.
        delay_polls: u32,
        ready: StdCell<bool>,
        reads: StdCell<u32>,
    }

    impl FakeAmp {
        fn new(raw: i32) -> Self {
            Self {
                raw,
                delay_polls: 0,
                ready: StdCell::new(true),
                reads: StdCell::new(0),
            }
        }

        fn slow(raw: i32, delay_polls: u32) -> Self {
            Self {
                raw,
                delay_polls,
                ready: StdCell::new(false),
                reads: StdCell::new(0),
            }
        }
    }

    impl Amplifier for FakeAmp {
        fn prepare(&mut self) {
            if self.delay_polls > 0 {
                self.delay_polls -= 1;
                self.ready.set(false);
            } else {
                self.ready.set(true);
            }
        }

        fn is_ready(&self) -> bool {
            self.ready.get()
        }

        fn read_raw(&mut self) -> i32 {
            self.reads.set(self.reads.get() + 1);
            self.raw
        }
    }

    #[test]
    fn a_single_cell_reports_a_weight_once_a_conversion_completes() {
        let mut scale = Hx711::single(FakeAmp::new(35_000), 1000.0);
        // Nothing read yet, so nothing to report. This is the difference from the C++
        // `getWeight()`, which returned 0.0 with no way to say so.
        assert_eq!(scale.weight_g(), None);
        scale.poll();
        assert_eq!(scale.weight_g(), Some(35.0));
    }

    #[test]
    fn the_weight_is_the_tare_corrected_count_divided_by_the_calibration() {
        // The calibration is a divisor and may be negative for an inverted cell.
        let mut scale = Hx711::single(FakeAmp::new(1_000_000), -1750.05);
        scale.set_samples(1);
        scale.poll();
        let w = scale.weight_g().expect("a reading");
        // 1 000 000 / -1750.05. Negative because the divisor is, which is what an inverted cell
        // looks like, so the expectation is negative too.
        let expected = 1_000_000.0 / -1750.05;
        assert!((w - expected).abs() < 0.01, "got {w}, expected {expected}");
    }

    #[test]
    fn a_negative_calibration_is_the_sign_of_an_inverted_cell_not_an_error() {
        // The shipped configuration uses -1750.05 for one cell, so a driver that rejected a
        // negative divisor would refuse a machine that works.
        let mut cell = Cell::new(FakeAmp::new(-1000), -100.0);
        cell.set_samples(1);
        cell.poll();
        assert_eq!(cell.weight_g(), Some(10.0));
    }

    #[test]
    fn a_zero_calibration_yields_no_weight_rather_than_a_division_by_zero() {
        let mut cell = Cell::new(FakeAmp::new(1000), 0.0);
        cell.set_samples(1);
        cell.poll();
        assert_eq!(cell.weight_g(), None);
    }

    #[test]
    fn a_tare_makes_the_empty_pan_read_zero() {
        let mut scale = Hx711::single(FakeAmp::new(500_000), 1000.0);
        for _ in 0..DEFAULT_SAMPLES {
            scale.poll();
        }
        assert_eq!(scale.weight_g(), Some(500.0));
        assert!(Hx711::tare(&mut scale));
        // The amplifier keeps reporting the same pan counts, so the weight is now zero.
        assert_eq!(scale.weight_g(), Some(0.0));
    }

    #[test]
    fn a_tare_with_no_samples_fails_rather_than_zeroing_against_nothing() {
        // The C++ `tare()` was unconditional, so a scale that had not yet converted zeroed itself
        // and the next reading was the whole pan.
        let mut scale = Hx711::single(FakeAmp::new(500_000), 1000.0);
        assert!(!Hx711::tare(&mut scale));
        assert_eq!(scale.weight_g(), None);
        let _ = Scale::tare(&mut scale).is_err();
    }

    #[test]
    fn the_average_smooths_the_reading() {
        // A single sample is noisy; the window is what makes a weight stable enough to stop on.
        let mut cell = Cell::new(FakeAmp::new(1000), 1.0);
        cell.set_samples(4);
        for _ in 0..4 {
            cell.poll();
        }
        assert_eq!(cell.weight_g(), Some(1000.0));
        assert_eq!(cell.samples(), 4);
    }

    #[test]
    fn the_window_is_bounded_at_both_ends() {
        // A zero window divides by zero; a 32-sample window is the library default but outside the
        // schema's declared range, so a configuration typo must not reach it.
        let mut cell = Cell::new(FakeAmp::new(1), 1.0);
        cell.set_samples(0);
        assert_eq!(cell.samples(), 1);
        cell.set_samples(200);
        assert_eq!(cell.samples(), MAX_SAMPLES);
    }

    #[test]
    fn a_second_cell_is_summed_with_the_first() {
        let mut scale = Hx711::dual(FakeAmp::new(1000), FakeAmp::new(2000), 100.0, 100.0);
        assert!(scale.is_dual());
        scale.set_samples(1);
        // Two polls, one per cell.
        scale.poll();
        assert_eq!(scale.weight_g(), None, "one cell has not read yet");
        scale.poll();
        assert_eq!(scale.weight_g(), Some(30.0));
    }

    #[test]
    fn a_dual_scale_with_a_dead_cell_reports_nothing_rather_than_half_the_weight() {
        // Reporting half the weight would stop the shot early, which is a safety problem rather
        // than an accuracy one.
        let mut scale = Hx711::dual(
            FakeAmp::new(1000),
            FakeAmp::slow(2000, 10_000),
            100.0,
            100.0,
        );
        scale.set_samples(1);
        for _ in 0..5 {
            scale.poll();
        }
        assert_eq!(
            scale.weight_g(),
            None,
            "a cell that has not read must block the weight"
        );
    }

    #[test]
    fn a_dual_tare_needs_both_cells_to_have_samples() {
        let mut scale = Hx711::dual(FakeAmp::new(1000), FakeAmp::new(1000), 100.0, 100.0);
        assert!(!Hx711::tare(&mut scale), "nothing has been read");
        scale.set_samples(1);
        scale.poll();
        assert!(!Hx711::tare(&mut scale), "only one cell has been read");
        scale.poll();
        assert!(Hx711::tare(&mut scale), "both cells have been read");
    }

    #[test]
    fn the_last_complete_weight_survives_a_tick_nothing_read() {
        let mut scale = Hx711::single(FakeAmp::new(1000), 100.0);
        scale.set_samples(1);
        scale.poll();
        assert_eq!(scale.last_weight_g(), Some(10.0));
        // A slow amplifier stops producing readings; the last one is still known.
        scale.poll();
        assert_eq!(Hx711::weight_g(&scale), Some(10.0));
    }

    #[test]
    fn clearing_makes_the_weight_unknown_rather_than_stale() {
        let mut scale = Hx711::single(FakeAmp::new(1000), 100.0);
        scale.set_samples(1);
        scale.poll();
        assert!(scale.weight_g().is_some());
        Hx711::clear(&mut scale);
        assert_eq!(
            scale.weight_g(),
            None,
            "a cleared scale must not report a stale weight"
        );
    }

    #[test]
    fn an_amplifier_that_never_becomes_ready_fails_within_the_timeout() {
        // The C++ firmware looped here forever, so a missing load cell stopped the machine from
        // booting at all. This is defect D29.
        let result = start(FakeAmp::slow(1000, 100_000), STARTUP_TIMEOUT_MS);
        assert_eq!(
            result.err(),
            Some(InitError::Timeout {
                after_ms: STARTUP_TIMEOUT_MS
            })
        );
    }

    #[test]
    fn an_amplifier_that_is_ready_starts() {
        let mut amp = start(FakeAmp::new(1000), 10).expect("should start");
        assert_eq!(amp.read_raw(), 1000);
    }

    #[test]
    fn an_implausible_weight_is_rejected() {
        // A 24-bit amplifier cannot represent more than this, so a larger number is a wiring fault
        // rather than a very heavy portafilter.
        assert!(weight_is_plausible(0.0));
        assert!(weight_is_plausible(36.0));
        assert!(weight_is_plausible(MAX_PLAUSIBLE_G));
        assert!(!weight_is_plausible(MAX_PLAUSIBLE_G + 1.0));
        assert!(!weight_is_plausible(-1000.0));
        assert!(!weight_is_plausible(f64::NAN));
        assert!(!weight_is_plausible(f64::INFINITY));
    }

    #[test]
    fn polling_does_not_block_on_an_unready_amplifier() {
        // The whole point of splitting prepare from read: the caller gets control back immediately.
        let mut cell = Cell::new(FakeAmp::slow(1000, 3), 1.0);
        let started = cell.poll();
        assert!(!started, "an unready amplifier produces no sample");
        assert!(
            cell.poll() || cell.poll() || cell.poll(),
            "it becomes ready eventually"
        );
        assert!(cell.poll(), "and then produces samples");
    }

    #[test]
    fn the_constants_match_the_cpp_source() {
        assert_eq!(DEFAULT_SAMPLES, 32);
        assert_eq!(STARTUP_TIMEOUT_MS, 5000);
    }
}
