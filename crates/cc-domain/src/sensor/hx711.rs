//! The HX711 load-cell amplifier: the bit protocol, the moving average, the
//! tare, and the bounded wait that replaces the C++'s unbounded spin.
//!
//! Owner: **R3-17**.
//!
//! # What this replaces, and what there is to match
//!
//! `src/hardware/scales/HX711Scale.cpp` wraps `olkal/HX711_ADC`
//! (`platformio.ini:44`) in a `Scale` and an `ISensor`. **The C++ never
//! constructs one** — nothing calls `HardwareContext::setScale`
//! (`include/clevercoffee/context/HardwareContext.h:143`), so `scale_` is
//! always `nullptr` and the scale contributes nothing to any state. `09 §23`
//! has the full analysis. There is therefore **no C++ behaviour to match**:
//! this is new, working functionality, and every number below is a decision
//! made here rather than a number transcribed.
//!
//! What *is* transcribed is the library's arithmetic, because it is
//! arithmetic rather than behaviour: the `24 + GAIN` clock count
//! (`HX711_ADC.cpp:340`), the sign-bit flip (`:365`), the drop-highest-and-
//! lowest moving average (`:305-327`), the `>> divBit` division
//! (`setSamplesInUse`, `:469-490`), and `getData`'s
//! `(smoothed - tare) / calFactor` (`:290-299`). Those are reproduced so a
//! calibration factor derived against the C++'s averaging means the same thing
//! here.
//!
//! # Why the C++'s spin loops are not copied
//!
//! `HX711Scale.cpp:44` and `:51` are `while (!startMultiple(...))` — unbounded.
//! Two things are wrong with that, and only the first is obvious:
//!
//! 1. A machine with no scale attached spins forever at boot, inside
//!    `app_main`, and never reaches the control task.
//! 2. `startMultiple`'s own timeout is a **`static` local**
//!    (`HX711_ADC.cpp:135`): `static unsigned long timeout = millis() +
//!    tareTimeOut;`. It is initialised on the *first* call and never reset, so
//!    the deadline belongs to when the function was first reached rather than
//!    to the call. A second `startMultiple` — which is exactly what the
//!    dual-cell branch at `HX711Scale.cpp:53` and `:57` does — inherits a
//!    deadline that may already be past and fails on its first iteration.
//!
//! So there is no timeout here at all: a read is a *query*
//! ([`read_raw`]) that reports "not ready", and whether the cell is answering
//! is a separate question ([`SignalWatchdog`]) with an explicit deadline. The
//! caller decides how long to wait, and every caller has a reason to be
//! bounded.
//!
//! # Why the raw value is offset binary and not two's complement
//!
//! The C++ flips the sign bit (`HX711_ADC.cpp:365`, `data = data ^ 0x800000`)
//! and then treats the result as an unsigned 24-bit quantity
//! (`HX711_ADC.cpp:378-397`, `if (data > 0) { ... }`). That converts the
//! HX711's two's-complement output into **offset binary**, where `0x800000` is
//! the electrical zero of the amplifier. Reproduced exactly, because the
//! calibration factor is only meaningful against that zero, and re-deriving it
//! would make every stored factor wrong by a factor of two.
//!
//! # The one C++ quirk reproduced rather than fixed
//!
//! [`Cell::push`] **discards a raw sample of exactly zero**
//! (`HX711_ADC.cpp:378`). Offset-binary zero is `0x800000` on the wire, the
//! HX711's most-negative full-scale input — a real reading, and one a reversed
//! load cell produces routinely. The C++ treats it as "no data" and silently
//! keeps the previous average. It is kept because it doubles as the
//! plausibility filter for a data line held low, and because removing a filter
//! that changes every weight on a reversed cell is not a change to make while
//! bringing a driver up for the first time. [`Cell::push`] returns whether the
//! sample was accepted and [`Cell::rejected_samples`] counts the refusals,
//! which the C++ does not have — a driver that reports its own health needs to
//! distinguish "the weight has not moved" from "every sample is being
//! rejected".

use crate::units::Millis;

/// The HX711's output is 24 bits wide.
///
/// Datasheet §5.1. The 24th bit is the sign, and the clocks after it select
/// the gain and the sample rate (datasheet §5.3), which is why
/// [`Rate::clocks`] is 24 *plus* something.
pub const DATA_BITS: u8 = 24;

/// The bit flipped to turn two's complement into offset binary.
///
/// `data ^ 0x800000` (`HX711_ADC.cpp:365`).
pub const SIGN_FLIP: u32 = 0x0080_0000;

/// The largest offset-binary value, `0xFFFFFF`.
pub const MAX_RAW: u32 = 0x00FF_FFFF;

/// The most samples a cell will average, matching the C++'s `SAMPLES 32`
/// override (`HX711Scale.h:13`).
pub const MAX_SAMPLES: u8 = 32;

/// How long DOUT may stay high before the cell counts as not answering.
///
/// `SIGNAL_TIMEOUT` (`HX711_ADC.h:33`), 100 ms — exactly one 10 SPS conversion
/// with no margin, so a cell that is late by even a little is declared absent.
/// Reproduced because it is the number that makes the C++'s
/// `getSignalTimeoutFlag()` mean the same thing, and because a *larger* number
/// would only delay the fault.
pub const SIGNAL_TIMEOUT: Millis = Millis::new(100);

/// The gain and rate combination, which is also the clock count.
///
/// Datasheet §5.3. Gain and rate are not independent settings on this part:
/// the number of clocks sent after the 24 data bits selects both, so this enum
/// is that one choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rate {
    /// Gain 32, 40 SPS, 26 clocks. Amplifier channel B.
    ///
    /// `setGain(32)` → two extra clocks (`HX711_ADC.cpp:16`).
    Gain32,
    /// Gain 64, 40 SPS, 27 clocks. Amplifier channel A.
    ///
    /// `setGain(64)` → three extra clocks (`HX711_ADC.cpp:17`).
    Gain64,
    /// Gain 128, 10 SPS, 25 clocks. Amplifier channel A.
    ///
    /// `setGain(128)` → one extra clock (`HX711_ADC.cpp:18`), and the value
    /// `begin()` uses (`HX711_ADC.cpp:32`). **This is the C++'s rate**: gain
    /// 128 is the most sensitive setting and a scale wants sensitivity more
    /// than it wants sample rate.
    Gain128,
}

impl Default for Rate {
    /// [`Rate::Gain128`], the C++'s `setGain(128)` from `begin()`.
    fn default() -> Self {
        Self::Gain128
    }
}

impl Rate {
    /// The amplifier's gain setting: 32, 64 or 128.
    #[must_use]
    pub const fn gain(self) -> u8 {
        match self {
            Self::Gain32 => 32,
            Self::Gain64 => 64,
            Self::Gain128 => 128,
        }
    }

    /// The clocks sent *after* the 24 data bits, which is what selects the
    /// gain. The C++ calls this `GAIN` and adds it to 24 when counting
    /// (`HX711_ADC.cpp:340`).
    #[must_use]
    pub const fn extra_clocks(self) -> u8 {
        match self {
            Self::Gain32 => 2,
            Self::Gain64 => 3,
            Self::Gain128 => 1,
        }
    }

    /// The total clocks one 24-bit read takes, `24 + extra_clocks`.
    #[must_use]
    pub const fn clocks(self) -> u8 {
        DATA_BITS + self.extra_clocks()
    }

    /// The conversion rate, in samples per second.
    #[must_use]
    pub const fn samples_per_second(self) -> u32 {
        match self {
            Self::Gain32 | Self::Gain64 => 40,
            Self::Gain128 => 10,
        }
    }

    /// How long one conversion takes, in milliseconds.
    ///
    /// This is the floor on how often a cell can be read, and therefore the
    /// reason the sampler is a task with its own sleep rather than something
    /// the control tick calls.
    #[must_use]
    pub const fn conversion_ms(self) -> u32 {
        // 1000 / sps, rounded up. 40 SPS is 25 ms and 10 SPS is 100 ms, both
        // exact, so the rounding never fires and the arithmetic in the tests
        // needs no fudge.
        1000_u32.div_ceil(self.samples_per_second())
    }
}

/// Why a read failed outright.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The shifted-out word is not a 24-bit reading.
    ///
    /// The C++ sets a `dataOutOfRange` flag here (`HX711_ADC.cpp:371-374`) and
    /// then **never reads it** — no getter, no use anywhere in the library or
    /// in `HX711Scale`. This port surfaces it, because a cell that has stopped
    /// producing valid words otherwise reports its last good average forever.
    OutOfRange,
}

/// Everything [`read_raw`] can fail with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError<E> {
    /// The word was not a valid reading. See [`Fault`].
    Fault(Fault),
    /// The bus itself failed — a pin that could not be written.
    ///
    /// The C++ has no equivalent: `digitalWrite` returns `void`, so a pin that
    /// fails is indistinguishable from one that worked and a partial word
    /// enters the dataset as if it were a reading.
    Bus(E),
}

/// The one thing the driver needs from the pins.
///
/// The seam that makes the protocol host-testable: every decision below is
/// exercised on a host against a scripted bus, and the only thing
/// `cc-hal-esp32` supplies is two data pins, one clock pin and a delay.
pub trait Hx711Bus {
    /// What a pin operation can fail with. `EspError` on the device.
    type Error;

    /// Run `f` with interrupts disabled.
    ///
    /// The whole 24-to-27-clock shift has to be atomic, and it is [`read_raw`]
    /// that decides how many clocks, so the critical section belongs around
    /// the *loop* rather than around each edge. The datasheet says why
    /// (§5.2): an interrupt longer than 60 µs while SCK is high can drive the
    /// amplifier into power-down, and this firmware's longest interrupt is the
    /// 10 ms heater chopper.
    ///
    /// A host implementation just calls `f(self)`.
    fn in_critical_section<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R;

    /// One SCK cycle, returning the DOUT level sampled after the falling edge.
    ///
    /// The C++'s order is high, delay, low, **then** read
    /// (`HX711_ADC.cpp:341-352`); this is that order.
    ///
    /// # Errors
    ///
    /// [`Self::Error`] if the clock pin could not be written. The caller
    /// ([`read_raw`]) discards the partial word and propagates it, so a pin
    /// that fails cannot put half a reading into the dataset.
    fn shift_bit(&mut self) -> Result<bool, Self::Error>;

    /// The DOUT line's level right now. `true` means high, which means **no
    /// conversion is ready** — the HX711 pulls DOUT low to say "read me"
    /// (datasheet §4.2).
    ///
    /// This is the whole of the interface, and it is the only thing a
    /// caller needs in order to wait for a conversion: poll this, and sleep
    /// between polls. There is deliberately **no** "wait for ready" method,
    /// because a method that waited would have to take a timeout, and the whole
    /// point of R3-17 is that no layer of this driver owns a deadline it cannot
    /// account for. `read_raw` calls this once and returns; the caller decides
    /// whether that is "not yet" or "never".
    fn data_high(&mut self) -> bool;
}

/// Read one 24-bit offset-binary sample, if a conversion is ready.
///
/// `Ok(None)` is the ordinary "nothing new yet" answer, and it is **not** a
/// fault. This function never waits on the bus: the only loop is the
/// fixed-length clock count, which terminates by construction. How long a
/// caller is prepared to wait for DOUT is the caller's decision, and every
/// caller here has a deadline — that is the whole of the difference from
/// `HX711Scale.cpp:44` and `:51`.
///
/// # Errors
///
/// [`ReadError::Fault`] if the assembled word is wider than 24 bits, which
/// means the bus returned something that is not a 24-bit reading.
/// [`ReadError::Bus`] if a pin could not be driven.
pub fn read_raw<B: Hx711Bus>(bus: &mut B, rate: Rate) -> Result<Option<u32>, ReadError<B::Error>> {
    if bus.data_high() {
        return Ok(None);
    }

    let clocks = rate.clocks();
    // The closure cannot return the bus's `Result` out of the critical
    // section, so the error is carried out of it in an `Option` and the
    // partial word is dropped rather than used.
    let mut failure: Option<B::Error> = None;
    let word = bus.in_critical_section(|bus| {
        let mut word = 0u32;
        for _ in 0..clocks {
            match bus.shift_bit() {
                Ok(bit) => word = (word << 1) | u32::from(bit),
                Err(err) => {
                    failure = Some(err);
                    return 0;
                }
            }
        }
        word
    });
    if let Some(err) = failure {
        return Err(ReadError::Bus(err));
    }

    // `clocks` is at most 27, so a 32-bit accumulator cannot have overflowed,
    // and the top `extra_clocks` bits are the gain-select clocks, which the
    // datasheet defines as don't-care on read-back (§5.3). They are dropped
    // here rather than allowed to push the word past 24 bits.
    let word = word >> rate.extra_clocks();

    if word > MAX_RAW {
        return Err(ReadError::Fault(Fault::OutOfRange));
    }

    // Offset binary, as the C++ (`HX711_ADC.cpp:365`).
    Ok(Some(word ^ SIGN_FLIP))
}

/// Tracks whether the cell is answering, and declares it absent if it stops.
///
/// This is the C++'s `signalTimeoutFlag` (`HX711_ADC.cpp:245-252`,
/// `:550-553`) as a query rather than as a side effect, with **one deliberate
/// difference that is the whole acceptance criterion**: the deadline is armed
/// from the moment the driver starts, not from the first conversion.
///
/// The C++ gets this right by accident. `startMultiple` sets
/// `lastDoutLowTime = millis()` before its first `update()`
/// (`HX711_ADC.cpp:129`) and `lastDoutLowTime` is a `static`-scope initialiser
/// evaluated at that point, so the silence clock starts at boot whether or not a
/// scale is present. A machine with no scale therefore gets
/// `signalTimeoutFlag = 1` about 100 ms in, and `HX711Scale::init` returns
/// `false` (`HX711Scale.cpp:62-64`) — which is correct, and is the only reason
/// the C++'s `init` can return at all.
///
/// An earlier revision of this port armed the watchdog on the **first**
/// conversion instead, reasoning that "a cell that has never spoken has not yet
/// been late". That is wrong, and it was caught on hardware: with no scale
/// fitted, `note_ready` is never called, `is_faulted` is permanently `false`,
/// and the machine reports a healthy scale forever while measuring nothing —
/// which is the C++'s exact defect (09 §23) reproduced in new code. The deadline
/// must start at [`Self::armed_at`], and a cell that never converts faults like
/// any other silent cell.
///
/// The deadline runs from the last time DOUT was seen low, which is what the
/// C++ measures from (`HX711_ADC.cpp:243`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignalWatchdog {
    last_low: Millis,
    armed: bool,
}

impl Default for SignalWatchdog {
    /// A watchdog with no deadline: never faults, until
    /// [`SignalWatchdog::armed_at`] gives it one.
    ///
    /// A `Default` that armed itself would need a clock to arm against, and
    /// inventing one here would make the "not yet started" state
    /// indistinguishable from the "started and silent" state — which is the bug
    /// above, in a different place.
    fn default() -> Self {
        Self {
            last_low: Millis::ZERO,
            armed: false,
        }
    }
}

impl SignalWatchdog {
    /// A watchdog with no deadline. Call [`Self::armed_at`] when the driver
    /// starts.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_low: Millis::ZERO,
            armed: false,
        }
    }

    /// Start the silence clock at `now`, as the C++'s `lastDoutLowTime =
    /// millis()` does before its first read.
    ///
    /// **Call this when the driver starts, not when the first sample arrives.**
    /// A cell that has never converted is the case that most needs reporting.
    pub const fn armed_at(&mut self, now: Millis) {
        self.last_low = now;
        self.armed = true;
    }

    /// Note that DOUT was seen low at `now`, which is a conversion completing.
    pub const fn note_ready(&mut self, now: Millis) {
        self.last_low = now;
        self.armed = true;
    }

    /// Whether the deadline has passed with no conversion seen.
    ///
    /// `false` only while unarmed, i.e. before
    /// [`Self::armed_at`]. An unarmed watchdog is the "driver has not started"
    /// state and is the only state in which silence is not yet a fault.
    #[must_use]
    pub fn is_faulted(&self, now: Millis) -> bool {
        self.armed && now.since(self.last_low).raw() > SIGNAL_TIMEOUT.raw()
    }

    /// Milliseconds since the line was last seen low, or `None` if unarmed.
    #[must_use]
    pub const fn silent_for_ms(&self, now: Millis) -> Option<u32> {
        if self.armed {
            Some(now.since(self.last_low).raw())
        } else {
            None
        }
    }

    /// Whether the deadline has been started at all.
    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.armed
    }
}

/// `MAX_SAMPLES + 2`: the averaged set plus the high and low guards.
const MAX_DATASET: usize = MAX_SAMPLES as usize + 2;

/// The calibration factor this firmware ships with.
///
/// `hardware.sensors.scale.calibration` defaults to 1.0 (`cc-config`,
/// `HardwareSensorsScale::default`), which is also the C++'s `HX711_ADC`
/// default (`HX711_ADC.h:73`). At 1.0 the reported weight is the raw count,
/// which is a nonsense number in grams — so a factor has to be calibrated
/// before the scale means anything, and that is what
/// `hardware.sensors.scale.known_weight` is for.
pub const DEFAULT_CALIBRATION: f64 = 1.0;

/// One load cell: the moving-average dataset, the tare offset and the
/// calibration factor.
///
/// Reproduces `HX711_ADC`'s dataset arithmetic (`HX711_ADC.cpp:305-327`,
/// `:469-490`, `:290-299`) rather than inventing a filter.
#[derive(Clone, Debug)]
pub struct Cell {
    /// The dataset, `samples_in_use + 2` entries, written cyclically.
    ///
    /// The two extra entries are `IGN_HIGH_SAMPLE` and `IGN_LOW_SAMPLE`, both
    /// 1 (`HX711Scale.h:14-15`): the C++ keeps them in the set and discards
    /// the largest and smallest before averaging.
    dataset: [u32; MAX_DATASET],

    /// Where the next sample goes.
    read_index: usize,

    /// How many entries are averaged, the C++'s `samplesInUse`.
    ///
    /// Always a power of two, because the C++ derives both this and `divBit`
    /// from one shift (`HX711_ADC.cpp:479-483`) and the division is that
    /// shift.
    samples_in_use: u8,

    /// The tare, in raw offset-binary counts.
    tare: u32,

    /// Counts per gram.
    calibration: f64,

    /// Samples refused as implausible, since construction.
    rejected: u32,
}

impl Default for Cell {
    fn default() -> Self {
        Self::new(DEFAULT_CALIBRATION, 1)
    }
}

impl Cell {
    /// A cell with a calibration factor and a sample count.
    ///
    /// `samples` is rounded **down** to a power of two and clamped to
    /// `1..=MAX_SAMPLES`, which is what `setSamplesInUse` does
    /// (`HX711_ADC.cpp:469-490`): it shifts the request right until it reaches
    /// zero and uses the bit position both as the sample count (`1 << divBit`)
    /// and as the division. So `samples = 3` becomes 2 and `samples = 20`
    /// becomes 16.
    #[must_use]
    pub fn new(calibration: f64, samples: u8) -> Self {
        Self {
            dataset: [0; MAX_DATASET],
            read_index: 0,
            samples_in_use: normalise_samples(samples),
            tare: 0,
            // A factor of zero would make every weight infinite, and the C++
            // divides by it unchecked (`HX711_ADC.cpp:296`). The config API can
            // set it to zero, so it is refused here rather than producing a NaN
            // that would flow into the display and onto MQTT.
            calibration: if calibration.is_finite() && calibration != 0.0 {
                calibration
            } else {
                DEFAULT_CALIBRATION
            },
            rejected: 0,
        }
    }

    /// How many samples are averaged.
    #[must_use]
    pub const fn samples_in_use(&self) -> u8 {
        self.samples_in_use
    }

    /// Change the sample count, rounding down to a power of two.
    ///
    /// The count is clamped rather than rejected: the config range is
    /// `1..=20` (`cc-config`), a blob can hold anything, and refusing to
    /// sample at all would stop the scale.
    pub const fn set_samples(&mut self, samples: u8) {
        self.samples_in_use = normalise_samples(samples);
    }

    /// The tare offset, in raw counts.
    #[must_use]
    pub const fn tare(&self) -> u32 {
        self.tare
    }

    /// Set the tare offset directly, which is how a reboot restores it.
    pub const fn set_tare(&mut self, tare: u32) {
        self.tare = tare;
    }

    /// The calibration factor, in counts per gram.
    #[must_use]
    pub fn calibration(&self) -> f64 {
        self.calibration
    }

    /// Set the calibration factor, refusing one that would make the weight
    /// undefined.
    ///
    /// `getData` divides by the factor with no check (`HX711_ADC.cpp:296`), so
    /// this is the only place the check can live. Returns whether it was
    /// accepted.
    pub fn set_calibration(&mut self, calibration: f64) -> bool {
        if calibration.is_finite() && calibration != 0.0 {
            self.calibration = calibration;
            true
        } else {
            false
        }
    }

    /// How many samples have been refused as implausible.
    ///
    /// The C++ has no such count: `if (data > 0)` (`HX711_ADC.cpp:378`) drops
    /// the sample and says nothing.
    #[must_use]
    pub const fn rejected_samples(&self) -> u32 {
        self.rejected
    }

    /// Add one raw offset-binary sample, returning whether it was accepted.
    ///
    /// A sample of exactly zero is refused — see the module docs.
    pub fn push(&mut self, raw: u32) -> bool {
        if raw == 0 {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        self.dataset[self.read_index] = raw;
        self.read_index += 1;
        if self.read_index >= self.window() {
            self.read_index = 0;
        }
        true
    }

    /// How many dataset entries are in play: the averaged set plus two guards.
    const fn window(&self) -> usize {
        self.samples_in_use as usize + 2
    }

    /// The moving average: the sum of the set less its largest and smallest
    /// member, divided by the sample count.
    ///
    /// `smoothedData` (`HX711_ADC.cpp:305-327`). A set that is still filling
    /// averages the zeros in it, which is what the C++ does too, and is why
    /// [`Cell::has_data`] exists.
    #[must_use]
    pub fn smoothed(&self) -> u32 {
        let (mut sum, mut low, mut high) = (0u64, u32::MAX, 0u32);
        for entry in &self.dataset[..self.window()] {
            sum += u64::from(*entry);
            low = low.min(*entry);
            high = high.max(*entry);
        }
        // The two guards come off *before* the division, so the divisor is the
        // sample count and not the size of the set.
        let sum = sum
            .saturating_sub(u64::from(low))
            .saturating_sub(u64::from(high));
        let divisor = 1u64 << self.samples_in_use.trailing_zeros();
        #[allow(
            clippy::cast_possible_truncation,
            reason = "the sum is at most 34 values of 0xFFFFFF, so the quotient \
                      is below 0xFFFFFF and fits a u32"
        )]
        {
            (sum / divisor) as u32
        }
    }

    /// Whether the cell has any sample to average.
    ///
    /// `false` means the moving average is a division of zeroes and the
    /// reported weight would be `-tare / calibration`, a number with no
    /// physical meaning. The C++ reports it anyway.
    #[must_use]
    pub fn has_data(&self) -> bool {
        self.smoothed() != 0
    }

    /// The weight in grams: `(smoothed - tare) / calibration`.
    ///
    /// `getData` (`HX711_ADC.cpp:290-299`), same order and the same
    /// subtraction of the tare *before* the division — which is what makes a
    /// tare independent of the calibration factor.
    #[must_use]
    pub fn weight(&self) -> f64 {
        (f64::from(self.smoothed()) - f64::from(self.tare)) / self.calibration
    }

    /// Tare against the current average, as `HX711_ADC::tare` does when it
    /// completes (`HX711_ADC.cpp:390`, `tareOffset = smoothedData()`).
    ///
    /// Returns the offset stored, so a caller can persist it.
    pub fn tare_now(&mut self) -> u32 {
        self.tare = self.smoothed();
        self.tare
    }

    /// A new calibration factor for a known mass on the pan.
    ///
    /// `getNewCalibration` (`HX711_ADC.cpp:538-547`): the current weight is
    /// scaled by the current factor and divided by the known mass, which
    /// algebraically is `(smoothed - tare) / known_mass`. Returned as well as
    /// stored, because the C++'s caller persists it and so does this one.
    ///
    /// # Errors
    ///
    /// `None` for a non-positive or non-finite `known_weight`, or when there
    /// is nothing on the pan to calibrate against.
    /// `hardware.sensors.scale.known_weight` is a `1..=2000` parameter, so a
    /// zero can only arrive from a corrupt blob, and dividing by it would make
    /// the factor infinite.
    pub fn calibrate(&mut self, known_weight: f64) -> Option<f64> {
        if !known_weight.is_finite() || known_weight <= 0.0 || !self.has_data() {
            return None;
        }
        let factor = (f64::from(self.smoothed()) - f64::from(self.tare)) / known_weight;
        if factor.is_finite() && factor != 0.0 {
            self.calibration = factor;
            Some(factor)
        } else {
            None
        }
    }
}

/// Round a requested sample count down to a power of two in `1..=MAX_SAMPLES`.
///
/// `setSamplesInUse` (`HX711_ADC.cpp:469-490`) does this by shifting the
/// request right until it reaches zero and counting the shifts:
///
/// ```cpp
/// samples >>= 1;
/// for (divBit = 0; samples != 0; samples >>= 1, divBit++);
/// samplesInUse = 1 << divBit;
/// ```
///
/// which is a round *down* to the largest power of two that is `<= samples`.
/// `0` is the C++'s "restore the compiled-in default"
/// (`HX711_ADC.cpp:472-474`); this firmware has no compiled-in default worth
/// restoring, so it becomes 1.
#[must_use]
pub const fn normalise_samples(samples: u8) -> u8 {
    if samples == 0 {
        return 1;
    }
    let mut value = samples;
    if value > MAX_SAMPLES {
        value = MAX_SAMPLES;
    }
    // The largest power of two that is <= value: double while the *next* one
    // would still fit.
    let mut power = 1u8;
    while ((power as u16) * 2) <= (value as u16) {
        power <<= 1;
    }
    power
}

/// How many bytes a [`TareRecord`] encodes to.
pub const TARE_RECORD_BYTES: usize = 10;

/// A byte that says "this blob is a tare this firmware wrote".
///
/// The NVS namespace is this firmware's own (`cc-config`,
/// `blob_store::NAMESPACE`) and the config blob carries a schema version
/// (`blob_store::SCHEMA_VERSION`) for exactly this reason. The tare has no
/// version field of its own and gets a magic byte plus a cell count.
const TARE_MAGIC: u8 = 0xA5;

/// A tare that survives a reboot.
///
/// The C++ keeps the tare in a `long tareOffset` member (`HX711_ADC.h:66`),
/// so it is lost on every reset and the machine has to be re-tared by hand
/// after a power cut. This is the persisted form: the two raw offsets and how
/// many cells they describe, in ten bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TareRecord {
    /// The first cell's offset. Always written, even on a single cell.
    pub offset_1: u32,
    /// The second cell's offset, or 0 on a single-cell machine.
    pub offset_2: u32,
    /// 1 or 2. A record with the wrong count is refused rather than
    /// half-applied: applying one offset of a two-cell tare leaves the machine
    /// reading a weight that is wrong by however much the other cell
    /// contributes, and it looks calibrated.
    pub cells: u8,
}

/// Encode for storage: little-endian offsets, then the magic byte, then the
/// cell count.
///
/// The encoding lives in the portable crate so it is host-tested; the device
/// half is a `nvs_set_blob` and nothing else.
#[must_use]
pub fn encode_tare(record: TareRecord) -> [u8; TARE_RECORD_BYTES] {
    let mut out = [0u8; TARE_RECORD_BYTES];
    out[0..4].copy_from_slice(&record.offset_1.to_le_bytes());
    out[4..8].copy_from_slice(&record.offset_2.to_le_bytes());
    out[8] = TARE_MAGIC;
    out[9] = record.cells;
    out
}

/// Decode a stored tare.
///
/// `None` for a wrong length, a wrong magic byte, or a cell count that is
/// neither 1 nor 2. All three mean the same thing to the caller — this is not a
/// tare this firmware wrote — so all three are "there is no stored tare, so
/// tare at start-up".
#[must_use]
pub fn decode_tare(bytes: &[u8]) -> Option<TareRecord> {
    if bytes.len() != TARE_RECORD_BYTES || bytes[8] != TARE_MAGIC {
        return None;
    }
    let cells = match bytes[9] {
        1 | 2 => bytes[9],
        _ => return None,
    };
    Some(TareRecord {
        offset_1: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        offset_2: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        cells,
    })
}

/// The scale: one cell or two, sharing a clock.
///
/// `HX711Scale` (`HX711Scale.cpp:16-24`) holds one or two `HX711_ADC`s. The
/// dual case alternates which cell is read (`readSecondScale`,
/// `HX711Scale.cpp:89-99`) because one SCK line cannot clock two amplifiers'
/// data lines independently: the shared clock shifts out *both*, and the
/// firmware chooses which DOUT it samples. That alternation is reproduced,
/// including its cost — each cell is read at half the rate, so a dual cell's
/// reported weight updates at 5 Hz at 10 SPS.
#[derive(Clone, Debug, Default)]
pub struct Scale {
    /// The first cell, or the only cell.
    cell_1: Cell,
    /// The second cell, present only on a dual scale.
    cell_2: Option<Cell>,
    /// Which cell the next read belongs to. Dual only.
    next_is_second: bool,
}

impl Scale {
    /// A single-cell scale.
    #[must_use]
    pub fn single(calibration: f64, samples: u8) -> Self {
        Self {
            cell_1: Cell::new(calibration, samples),
            cell_2: None,
            next_is_second: false,
        }
    }

    /// A two-cell scale with a shared clock.
    #[must_use]
    pub fn dual(calibration_1: f64, calibration_2: f64, samples: u8) -> Self {
        Self {
            cell_1: Cell::new(calibration_1, samples),
            cell_2: Some(Cell::new(calibration_2, samples)),
            next_is_second: false,
        }
    }

    /// Whether this is a two-cell scale.
    #[must_use]
    pub const fn is_dual(&self) -> bool {
        self.cell_2.is_some()
    }

    /// How many cells.
    #[must_use]
    pub const fn cells(&self) -> u8 {
        if self.cell_2.is_some() {
            2
        } else {
            1
        }
    }

    /// Which cell index the next read samples.
    ///
    /// The bus needs this to know which DOUT line to watch, and exposing it is
    /// what lets the alternation live here rather than in the pin layer.
    #[must_use]
    pub const fn next_cell(&self) -> u8 {
        if self.cell_2.is_some() && self.next_is_second {
            1
        } else {
            0
        }
    }

    /// Read whichever cell is next and fold the sample in.
    ///
    /// `Ok(None)` when the cell has not finished a conversion, which is the
    /// common case and is **not** a fault.
    ///
    /// The alternation advances unconditionally, including on a not-ready read:
    /// the clock has already advanced both amplifiers whatever DOUT said, so
    /// tying the alternation to the data would leave both cells reading the
    /// same conversions forever.
    ///
    /// # Errors
    ///
    /// [`ReadError`], propagated from [`read_raw`].
    pub fn read<B: Hx711Bus>(
        &mut self,
        bus: &mut B,
        rate: Rate,
    ) -> Result<Option<u32>, ReadError<B::Error>> {
        let result = read_raw(bus, rate);
        if let Ok(Some(raw)) = result {
            match self.next_cell() {
                0 => {
                    self.cell_1.push(raw);
                }
                _ => {
                    if let Some(second) = self.cell_2.as_mut() {
                        second.push(raw);
                    }
                }
            }
        }
        // The alternation advances on **every** outcome — a sample, a
        // not-ready read, and a fault alike. A fault skips the *sample* but not
        // the *clock*: the shared SCK line has already advanced whichever cell
        // it was reading, so leaving the alternation where it was would make
        // the next read watch the same DOUT line again, and the two cells would
        // then be read on alternate clock pulses against a pipeline that is
        // running at half rate. The observed symptom is a dual cell whose
        // weight is permanently wrong by one cell's contribution, with no
        // fault anywhere.
        self.advance();
        result
    }

    /// Move to the next cell, alternating on a dual scale.
    const fn advance(&mut self) {
        if self.cell_2.is_some() {
            self.next_is_second = !self.next_is_second;
        }
    }

    /// The first cell.
    #[must_use]
    pub const fn cell_1(&self) -> &Cell {
        &self.cell_1
    }

    /// The first cell, mutably, for calibration and taring.
    pub const fn cell_1_mut(&mut self) -> &mut Cell {
        &mut self.cell_1
    }

    /// The second cell, if there is one.
    ///
    /// Not `const`: `Option::as_ref` only became usable in a `const` context in
    /// Rust 1.83 and the workspace's MSRV is 1.82.
    #[must_use]
    pub fn cell_2(&self) -> Option<&Cell> {
        self.cell_2.as_ref()
    }

    /// The second cell, mutably, if there is one.
    pub fn cell_2_mut(&mut self) -> Option<&mut Cell> {
        self.cell_2.as_mut()
    }

    /// The reported weight in grams.
    ///
    /// A dual scale **sums** the two cells (`HX711Scale.cpp:104`,
    /// `currentWeight = weight1 + weight2`), which is what two load cells under
    /// one pan mean. A single cell reports its own.
    #[must_use]
    pub fn weight(&self) -> f64 {
        match &self.cell_2 {
            Some(second) => self.cell_1.weight() + second.weight(),
            None => self.cell_1.weight(),
        }
    }

    /// The tare record for persistence.
    ///
    /// Not `const` for the same reason as [`Self::cell_2`].
    #[must_use]
    pub fn record(&self) -> TareRecord {
        TareRecord {
            offset_1: self.cell_1.tare,
            offset_2: match self.cell_2.as_ref() {
                Some(second) => second.tare,
                None => 0,
            },
            cells: self.cells(),
        }
    }

    /// Restore a persisted tare.
    ///
    /// # Errors
    ///
    /// `false` when the record does not describe this scale — a one-cell record
    /// for a two-cell scale, or the reverse. Applying half a tare is worse than
    /// applying none, because the result looks calibrated.
    pub fn restore(&mut self, record: TareRecord) -> bool {
        if record.cells != self.cells() {
            return false;
        }
        self.cell_1.set_tare(record.offset_1);
        if let Some(second) = self.cell_2.as_mut() {
            second.set_tare(record.offset_2);
        }
        true
    }

    /// Tare every cell against its own current average.
    pub fn tare_all(&mut self) -> TareRecord {
        self.cell_1.tare_now();
        if let Some(second) = self.cell_2.as_mut() {
            second.tare_now();
        }
        self.record()
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;

    /// A scripted bus: DOUT's level, the bits to shift out, and a count of how
    /// many clocks were asked for.
    struct FakeBus {
        /// DOUT high, i.e. no conversion ready.
        high: bool,
        /// The bits to shift out, most significant first.
        bits: Vec<bool>,
        /// How many clocks the bus has been asked for.
        clocks: usize,
        /// Start raising [`Hx711Bus::shift_bit`]'s error after this many clocks.
        fail_after: Option<usize>,
    }

    impl FakeBus {
        /// A bus with a conversion ready and a scripted 24-bit `word`.
        ///
        /// The bits are emitted the way the part emits them: the 24 data bits
        /// first, most significant first, then don't-care for the gain-select
        /// clocks (`HX711_ADC.cpp:340-352`, which samples DOUT for the first
        /// `24` iterations and only clocks for the rest).
        ///
        /// The don't-care clocks emit **ones** on purpose. The real part leaves
        /// them at whatever they were, and ones are the value that would
        /// corrupt the word if the driver let them in — so every read test here
        /// is a test that they do not.
        fn with_word(word: u32) -> Self {
            let mut bits: Vec<bool> = (0..DATA_BITS)
                .rev()
                .map(|shift| (word >> shift) & 1 == 1)
                .collect();
            bits.resize(32, true);
            Self {
                high: false,
                bits,
                clocks: 0,
                fail_after: None,
            }
        }

        /// A bus with DOUT high and nothing to shift.
        fn not_ready() -> Self {
            Self {
                high: true,
                bits: Vec::new(),
                clocks: 0,
                fail_after: None,
            }
        }

        /// A bus that stops working after `after` clocks.
        fn failing(after: usize) -> Self {
            Self {
                high: false,
                bits: vec![true; 32],
                clocks: 0,
                fail_after: Some(after),
            }
        }
    }

    impl Hx711Bus for FakeBus {
        type Error = ();

        fn in_critical_section<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
            f(self)
        }

        fn shift_bit(&mut self) -> Result<bool, Self::Error> {
            self.clocks += 1;
            if self.fail_after.is_some_and(|limit| self.clocks > limit) {
                return Err(());
            }
            let index = self.clocks - 1;
            Ok(self.bits.get(index).copied().unwrap_or(false))
        }

        fn data_high(&mut self) -> bool {
            self.high
        }
    }

    // ---- rate ----------------------------------------------------------

    #[test]
    fn the_rate_is_the_clock_count_and_the_conversion_time() {
        // Datasheet 5.3, and setGain/conversion24bit (HX711_ADC.cpp:14-19, :340).
        assert_eq!(Rate::Gain128.extra_clocks(), 1);
        assert_eq!(Rate::Gain128.clocks(), 25);
        assert_eq!(Rate::Gain128.gain(), 128);
        assert_eq!(Rate::Gain128.samples_per_second(), 10);
        assert_eq!(Rate::Gain128.conversion_ms(), 100);

        assert_eq!(Rate::Gain64.extra_clocks(), 3);
        assert_eq!(Rate::Gain64.clocks(), 27);
        assert_eq!(Rate::Gain64.gain(), 64);
        assert_eq!(Rate::Gain64.conversion_ms(), 25);

        assert_eq!(Rate::Gain32.extra_clocks(), 2);
        assert_eq!(Rate::Gain32.clocks(), 26);
        assert_eq!(Rate::Gain32.gain(), 32);
        assert_eq!(Rate::Gain32.conversion_ms(), 25);
    }

    #[test]
    fn the_default_rate_is_the_gain_begin_uses() {
        // HX711_ADC.cpp:32, begin() -> setGain(128).
        assert_eq!(Rate::default(), Rate::Gain128);
    }

    // ---- read_raw ------------------------------------------------------

    #[test]
    fn a_high_data_line_is_not_a_fault() {
        let mut bus = FakeBus::not_ready();
        assert_eq!(read_raw(&mut bus, Rate::Gain128), Ok(None));
        assert_eq!(bus.clocks, 0, "no clock may be sent while DOUT is high");
    }

    #[test]
    fn a_read_sends_exactly_the_clocks_the_rate_asks_for() {
        for rate in [Rate::Gain32, Rate::Gain64, Rate::Gain128] {
            let mut bus = FakeBus::with_word(0);
            let _ = read_raw(&mut bus, rate);
            assert_eq!(bus.clocks, rate.clocks() as usize, "{rate:?}");
        }
    }

    #[test]
    fn the_sign_bit_is_flipped_to_offset_binary() {
        // All ones on the wire is full scale in two's complement and
        // 0x7FFFFF in offset binary (HX711_ADC.cpp:365).
        let mut bus = FakeBus::with_word(0x00FF_FFFF);
        assert_eq!(read_raw(&mut bus, Rate::Gain128), Ok(Some(0x007F_FFFF)));
    }

    #[test]
    fn the_electrical_zero_reads_exactly_zero() {
        // 0x800000 on the wire is the amplifier's zero (datasheet 4.3), and the
        // sign-bit flip is what makes it read as 0 rather than as -8388608.
        // That is the whole point of the offset-binary conversion: a tared,
        // unloaded cell reads zero without any arithmetic beyond the flip, and
        // a cell with weight on it reads positive.
        let mut zero = FakeBus::with_word(0x0080_0000);
        assert_eq!(read_raw(&mut zero, Rate::Gain128), Ok(Some(0)));

        // A load pushes the count *up* from the zero point, which is the
        // opposite of the two's-complement reading and is what makes the C++'s
        // sign flip load-bearing rather than cosmetic.
        let mut loaded = FakeBus::with_word(0x0080_0064);
        assert_eq!(read_raw(&mut loaded, Rate::Gain128), Ok(Some(100)));

        // And a cell wired the other way round drives the count *down* from the
        // zero point, which is the value the sign flip turns into a number
        // above 0x800000 and a negative calibration factor turns into a
        // negative weight.
        let mut reversed = FakeBus::with_word(0x007F_FF9C);
        assert_eq!(
            read_raw(&mut reversed, Rate::Gain128),
            Ok(Some(0x00FF_FF9C))
        );
    }

    #[test]
    fn the_gain_clocks_are_discarded_from_the_word() {
        // The clocks after the 24 data bits select the gain and are don't-care
        // on read-back (datasheet 5.3). `HX711_ADC.cpp:344-350` clocks without
        // sampling for them, so they must not shift into the word. The fake
        // emits ones for them; if they leaked in, the result would be
        // 0x7FFFFF shifted rather than the plain reading.
        let mut bus = FakeBus::with_word(0x00FF_FFFF);
        assert_eq!(read_raw(&mut bus, Rate::Gain128), Ok(Some(0x007F_FFFF)));
        assert_eq!(
            bus.clocks, 25,
            "the gain clock is sent even though its bit is discarded"
        );
    }

    #[test]
    fn a_bus_that_cannot_be_written_produces_no_partial_sample() {
        // The C++ has no equivalent: digitalWrite returns void, so a partial
        // word is shifted into the dataset as if it were a reading.
        let mut bus = FakeBus::failing(5);
        assert_eq!(read_raw(&mut bus, Rate::Gain128), Err(ReadError::Bus(())));
    }

    // ---- watchdog ------------------------------------------------------

    #[test]
    fn an_unarmed_watchdog_never_faults() {
        // "The driver has not started yet" is the only state in which silence
        // is not a fault. A `Default` that armed itself would make this
        // indistinguishable from the case below.
        let watchdog = SignalWatchdog::new();
        assert!(!watchdog.is_armed());
        assert!(!watchdog.is_faulted(Millis::new(u32::MAX)));
        assert_eq!(watchdog.silent_for_ms(Millis::new(1_000)), None);
    }

    /// 🔴 The acceptance criterion, as a host test.
    ///
    /// An absent scale — DOUT held high forever, which is exactly what an
    /// unconnected data line does — must be reported as a fault, and must be
    /// reported *without a single conversion having arrived*. A watchdog armed
    /// only on the first reading would stay silent here forever, which is the
    /// C++'s defect reproduced in new code; this is the test that says so.
    #[test]
    fn a_cell_that_never_converts_is_faulted_from_the_moment_the_driver_starts() {
        let boot = Millis::new(500);
        let mut watchdog = SignalWatchdog::new();
        watchdog.armed_at(boot);
        assert!(watchdog.is_armed());

        assert!(
            !watchdog.is_faulted(boot),
            "not late at the instant the driver starts"
        );
        assert!(
            !watchdog.is_faulted(Millis::new(boot.raw() + SIGNAL_TIMEOUT.raw())),
            "the deadline is inclusive, as the C++'s `>` is"
        );
        assert!(
            watchdog.is_faulted(Millis::new(boot.raw() + SIGNAL_TIMEOUT.raw() + 1)),
            "an absent cell must fault one tick past the deadline"
        );
        assert_eq!(
            watchdog.silent_for_ms(Millis::new(boot.raw() + SIGNAL_TIMEOUT.raw() + 1)),
            Some(SIGNAL_TIMEOUT.raw() + 1),
            "silence is measured from the driver's start, as lastDoutLowTime is"
        );
    }

    #[test]
    fn the_watchdog_faults_one_signal_timeout_after_the_last_conversion() {
        let mut watchdog = SignalWatchdog::new();
        watchdog.armed_at(Millis::new(1_000));
        assert!(!watchdog.is_faulted(Millis::new(1_000 + SIGNAL_TIMEOUT.raw())));
        assert!(watchdog.is_faulted(Millis::new(1_000 + SIGNAL_TIMEOUT.raw() + 1)));
        watchdog.note_ready(Millis::new(1_100));
        assert!(
            !watchdog.is_faulted(Millis::new(1_100 + SIGNAL_TIMEOUT.raw())),
            "a conversion restarts the clock"
        );
        assert_eq!(watchdog.silent_for_ms(Millis::new(1_100)), Some(0));
    }

    #[test]
    fn a_conversion_clears_the_watchdog() {
        let mut watchdog = SignalWatchdog::new();
        watchdog.armed_at(Millis::new(0));
        assert!(watchdog.is_faulted(Millis::new(1_000)));
        watchdog.note_ready(Millis::new(1_000));
        assert!(!watchdog.is_faulted(Millis::new(1_100)));
    }

    #[test]
    fn the_watchdog_survives_the_millisecond_clock_wrapping() {
        // `Millis::since` wraps by design (units.rs:168-176) because the C++
        // relies on it across the 49.7-day rollover, so the deadline has to be
        // computed the same way or a machine up for 49 days reports every cell
        // as absent.
        let mut watchdog = SignalWatchdog::new();
        watchdog.armed_at(Millis::new(u32::MAX - 10));
        // 5 ms later in real terms, which is 5 - (2^32 - 10) on the counter.
        assert!(!watchdog.is_faulted(Millis::new(5)));
        assert_eq!(watchdog.silent_for_ms(Millis::new(5)), Some(16));
        // 200 ms later in real terms, past the 100 ms deadline.
        assert!(watchdog.is_faulted(Millis::new(201)));
    }

    // ---- sample normalisation -----------------------------------------

    #[test]
    fn the_sample_count_is_rounded_down_to_a_power_of_two() {
        // setSamplesInUse, HX711_ADC.cpp:479-483.
        assert_eq!(normalise_samples(1), 1);
        assert_eq!(normalise_samples(2), 2);
        assert_eq!(normalise_samples(3), 2);
        assert_eq!(normalise_samples(4), 4);
        assert_eq!(normalise_samples(5), 4);
        assert_eq!(normalise_samples(16), 16);
        assert_eq!(normalise_samples(20), 16, "the config maximum is 20");
    }

    #[test]
    fn a_sample_count_above_the_maximum_is_clamped_not_rejected() {
        // The compiled-in maximum is 32 (HX711Scale.h:13); a config blob can
        // hold anything, and refusing to sample would stop the scale.
        assert_eq!(normalise_samples(200), 32);
        assert_eq!(normalise_samples(0), 1, "the C++'s 'reset to default'");
    }

    // ---- averaging -----------------------------------------------------

    #[test]
    fn the_average_drops_the_highest_and_the_lowest() {
        // smoothedData, HX711_ADC.cpp:305-327, with IGN_HIGH_SAMPLE and
        // IGN_LOW_SAMPLE both 1 (HX711Scale.h:14-15). Two samples averaged
        // means a set of four, so a run of four 10s followed by four 20s
        // averages to 10 once the set is full of 20s.
        let mut cell = Cell::new(1.0, 2);
        for _ in 0..8 {
            cell.push(10);
        }
        // Set of 4 is now [20,20,20,20] once the second run fills it.
        for _ in 0..4 {
            cell.push(20);
        }
        // sum 80 - min 20 - max 20 = 40, / 2 = 20.
        assert_eq!(cell.smoothed(), 20);
    }

    #[test]
    fn a_constant_cell_averages_to_that_constant() {
        let mut cell = Cell::new(1.0, 4);
        for _ in 0..16 {
            cell.push(1234);
        }
        // sum 6*1234 - 1234 - 1234 = 4*1234, / 4 = 1234.
        assert_eq!(cell.smoothed(), 1234);
    }

    #[test]
    fn a_zero_sample_is_refused_and_counted() {
        // `if (data > 0)` (HX711_ADC.cpp:378), with no count in the C++.
        let mut cell = Cell::new(1.0, 1);
        // A window of three (one averaged sample plus the two guards). The
        // refused samples must not enter the set: fill it with real readings
        // and then offer zeroes, and the average must not move.
        for _ in 0..3 {
            assert!(cell.push(1000));
        }
        let before = cell.smoothed();
        assert_eq!(before, 1000);
        assert!(!cell.push(0));
        assert!(!cell.push(0));
        assert_eq!(cell.rejected_samples(), 2);
        assert_eq!(
            cell.smoothed(),
            before,
            "a refused sample must not reach the dataset"
        );
    }

    #[test]
    fn a_partly_filled_dataset_averages_the_empty_slots_in() {
        // The C++ does not track how much of `dataSampleSet` has been written
        // (`HX711_ADC.cpp:389-397`): the array is zero-initialised and the
        // first averages are taken over a set that is mostly zeroes. That is
        // reproduced, and it is why [`Cell::has_data`] exists — a caller that
        // reports a weight before the set has filled would publish a number
        // made of the zero slots.
        let cell = Cell::new(1.0, 4);
        assert!(!cell.has_data());
        assert_eq!(cell.smoothed(), 0);

        let mut filling = Cell::new(1.0, 4);
        filling.push(1000);
        assert!(
            filling.smoothed() < 1000,
            "one real sample in a window of six is diluted by the empty slots"
        );
    }

    #[test]
    fn a_cell_with_no_samples_has_no_data() {
        let cell = Cell::new(1.0, 1);
        assert!(!cell.has_data());
    }

    // ---- weight, tare, calibration ------------------------------------

    #[test]
    fn the_weight_is_the_tare_corrected_count_divided_by_the_factor() {
        // getData, HX711_ADC.cpp:290-299.
        let mut cell = Cell::new(2.0, 1);
        for _ in 0..3 {
            cell.push(1000);
        }
        cell.set_tare(400);
        assert!((cell.weight() - 300.0).abs() < 1e-9, "{}", cell.weight());
    }

    #[test]
    fn taring_makes_an_unloaded_pan_read_zero() {
        let mut cell = Cell::new(1.0, 1);
        for _ in 0..3 {
            cell.push(500_000);
        }
        assert!(cell.tare_now() > 0);
        assert!(cell.weight().abs() < 1e-9, "{}", cell.weight());
    }

    #[test]
    fn a_reversed_cell_reports_a_negative_weight() {
        // hardware.sensors.scale.calibration's range is -999999..=999999
        // (cc-config), so a negative factor is legal and means a load cell
        // wired the other way round. A driver that clamps it is wrong.
        let mut cell = Cell::new(-2.0, 1);
        for _ in 0..3 {
            cell.push(600);
        }
        assert!((cell.weight() + 300.0).abs() < 1e-9, "{}", cell.weight());
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "the factors here are 3.0 and 4.0, which an f64 holds exactly; \
                  `==` is the assertion, and a tolerance would hide a factor that \
                  came back as 3.0000000000000004"
    )]
    fn a_zero_or_non_finite_calibration_is_refused() {
        // getData divides by it unchecked (HX711_ADC.cpp:296), and the config
        // API can set it to zero, so the check has to live here.
        let mut cell = Cell::new(3.0, 1);
        assert!(!cell.set_calibration(0.0));
        assert!(!cell.set_calibration(f64::NAN));
        assert!(!cell.set_calibration(f64::INFINITY));
        assert_eq!(cell.calibration(), 3.0, "the previous factor survives");
        assert!(cell.set_calibration(4.0));
        assert_eq!(cell.calibration(), 4.0);
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "DEFAULT_CALIBRATION is 1.0, which an f64 holds exactly, and \
                  this test exists precisely to prove the stored value is \
                  *that* constant rather than something close to it"
    )]
    fn a_zero_calibration_at_construction_falls_back_to_the_default() {
        let cell = Cell::new(0.0, 1);
        assert_eq!(cell.calibration(), DEFAULT_CALIBRATION);
        let cell = Cell::new(f64::NAN, 1);
        assert_eq!(cell.calibration(), DEFAULT_CALIBRATION);
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "267000 / 100 is 2670 exactly in binary floating point, so \
                  `==` is the assertion: a factor that came back as 2669.9999 \
                  would be a defect, not a rounding artefact to tolerate"
    )]
    fn calibrating_against_a_known_mass_gives_counts_per_gram() {
        // getNewCalibration, HX711_ADC.cpp:538-547.
        let mut cell = Cell::new(1.0, 1);
        for _ in 0..3 {
            cell.push(267_000);
        }
        assert_eq!(cell.calibrate(100.0), Some(2670.0));
        assert!((cell.weight() - 100.0).abs() < 1e-9, "{}", cell.weight());
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "1.0 is held exactly by an f64 and a refused calibration must \
                  leave the factor bit-for-bit unchanged"
    )]
    fn calibrating_against_nothing_is_refused() {
        let mut cell = Cell::new(1.0, 1);
        assert_eq!(cell.calibrate(100.0), None, "an empty cell");
        for _ in 0..3 {
            cell.push(1000);
        }
        assert_eq!(cell.calibrate(0.0), None, "zero is not a mass");
        assert_eq!(cell.calibrate(-5.0), None);
        assert_eq!(cell.calibrate(f64::NAN), None);
        assert_eq!(
            cell.calibration(),
            1.0,
            "a refused calibration changes nothing"
        );
    }

    // ---- one and two cells --------------------------------------------

    #[test]
    fn a_single_cell_scale_reports_its_own_weight() {
        let mut scale = Scale::single(1.0, 1);
        assert_eq!(scale.cells(), 1);
        assert!(!scale.is_dual());
        for _ in 0..3 {
            scale.cell_1_mut().push(1000);
        }
        assert!((scale.weight() - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn a_dual_cell_scale_sums_both_cells() {
        // HX711Scale.cpp:104, `currentWeight = weight1 + weight2`.
        let mut scale = Scale::dual(1.0, 1.0, 1);
        assert_eq!(scale.cells(), 2);
        for _ in 0..3 {
            scale.cell_1_mut().push(1000);
        }
        for _ in 0..3 {
            scale.cell_2_mut().expect("a second cell").push(500);
        }
        assert!((scale.weight() - 1500.0).abs() < 1e-9, "{}", scale.weight());
    }

    #[test]
    fn a_dual_scale_alternates_which_cell_it_reads() {
        // readSecondScale, HX711Scale.cpp:89-99.
        let mut scale = Scale::dual(1.0, 1.0, 1);
        let mut bus = FakeBus::with_word(0);
        assert_eq!(scale.next_cell(), 0);
        let _ = scale.read(&mut bus, Rate::Gain128);
        assert_eq!(scale.next_cell(), 1);
        let _ = scale.read(&mut bus, Rate::Gain128);
        assert_eq!(scale.next_cell(), 0);
    }

    #[test]
    fn a_dual_scale_alternates_even_when_a_read_finds_nothing() {
        // The clock advances both amplifiers whatever DOUT said, so the
        // alternation has to follow the clock and not the data.
        let mut scale = Scale::dual(1.0, 1.0, 1);
        let mut bus = FakeBus::not_ready();
        assert_eq!(scale.read(&mut bus, Rate::Gain128), Ok(None));
        assert_eq!(scale.next_cell(), 1);
    }

    #[test]
    fn a_dual_scale_alternates_even_when_the_read_faults() {
        // Same reason, and a fault must not leave the alternation stuck on one
        // cell, which would silently halve the weight.
        let mut scale = Scale::dual(1.0, 1.0, 1);
        let mut bus = FakeBus::failing(5);
        assert!(scale.read(&mut bus, Rate::Gain128).is_err());
        assert_eq!(scale.next_cell(), 1, "a fault is not a reason to stall");
    }

    #[test]
    fn a_single_cell_scale_never_alternates() {
        let mut scale = Scale::single(1.0, 1);
        let mut bus = FakeBus::with_word(0);
        let _ = scale.read(&mut bus, Rate::Gain128);
        assert_eq!(scale.next_cell(), 0);
    }

    #[test]
    fn a_read_through_the_scale_folds_the_sample_into_the_right_cell() {
        // The alternation is only useful if the sample lands in the cell the
        // bus was watching.
        let mut scale = Scale::dual(1.0, 1.0, 1);
        let mut bus = FakeBus::with_word(0x0080_0064); // offset binary 100
        let _ = scale.read(&mut bus, Rate::Gain128);
        for _ in 0..3 {
            scale.cell_1_mut().push(1000);
        }
        assert!(
            scale.cell_1().has_data(),
            "the first read belongs to cell 1"
        );
    }

    // ---- persistence ---------------------------------------------------

    #[test]
    fn a_tare_round_trips_through_ten_bytes() {
        let record = TareRecord {
            offset_1: 0x0080_1234,
            offset_2: 0x00FF_FFFF,
            cells: 2,
        };
        let bytes = encode_tare(record);
        assert_eq!(bytes.len(), TARE_RECORD_BYTES);
        assert_eq!(bytes[0], 0x34, "little-endian, lowest byte first");
        assert_eq!(bytes[9], 2, "the cell count is stored");
        assert_eq!(decode_tare(&bytes), Some(record));
    }

    #[test]
    fn a_blob_that_is_not_a_tare_decodes_to_nothing() {
        assert_eq!(decode_tare(&[]), None);
        assert_eq!(decode_tare(&[0; 9]), None, "too short");
        assert_eq!(decode_tare(&[0; 11]), None, "too long");
        let mut wrong_magic = encode_tare(TareRecord::default());
        wrong_magic[8] = 0x00;
        assert_eq!(decode_tare(&wrong_magic), None);
        let mut wrong_cells = encode_tare(TareRecord::default());
        wrong_cells[9] = 3;
        assert_eq!(
            decode_tare(&wrong_cells),
            None,
            "three cells is not a scale"
        );
    }

    #[test]
    fn a_restored_tare_survives_a_rebuild_of_the_scale() {
        // The acceptance criterion: a machine that was tared and then power
        // cycled must come back tared. The "reboot" is a fresh Scale, which is
        // exactly what the driver gets.
        let mut original = Scale::single(1.0, 1);
        for _ in 0..8 {
            original.cell_1_mut().push(0x0080_0100);
        }
        let record = original.tare_all();
        assert!(record.offset_1 > 0, "a real tare was taken");

        let mut rebooted = Scale::single(1.0, 1);
        for _ in 0..8 {
            rebooted.cell_1_mut().push(0x0080_0100);
        }
        assert!(rebooted.restore(record), "a one-cell tare, restored");
        assert!(
            rebooted.weight().abs() < 1e-6,
            "restored tare reads zero, got {}",
            rebooted.weight()
        );
    }

    #[test]
    fn a_tare_for_the_wrong_number_of_cells_is_refused() {
        // Applying one offset of a two-cell tare leaves the machine reading a
        // weight wrong by the other cell's contribution, and it looks
        // calibrated — which is worse than not restoring at all.
        let mut dual = Scale::dual(1.0, 1.0, 1);
        assert!(!dual.restore(TareRecord {
            offset_1: 100,
            offset_2: 0,
            cells: 1,
        }));

        let mut single = Scale::single(1.0, 1);
        assert!(!single.restore(TareRecord {
            offset_1: 100,
            offset_2: 200,
            cells: 2,
        }));
        assert_eq!(single.cell_1().tare(), 0, "nothing was applied");
    }

    #[test]
    fn a_dual_tare_restores_onto_both_cells() {
        let mut scale = Scale::dual(1.0, 1.0, 1);
        for _ in 0..8 {
            scale.cell_1_mut().push(0x0080_0100);
            scale.cell_2_mut().expect("a second cell").push(0x0080_0200);
        }
        let record = scale.tare_all();
        assert!(record.offset_1 > 0 && record.offset_2 > 0);
        assert_eq!(record.cells, 2);

        let mut rebooted = Scale::dual(1.0, 1.0, 1);
        for _ in 0..8 {
            rebooted.cell_1_mut().push(0x0080_0100);
            rebooted
                .cell_2_mut()
                .expect("a second cell")
                .push(0x0080_0200);
        }
        assert!(rebooted.restore(record));
        assert!(rebooted.weight().abs() < 1e-6, "{}", rebooted.weight());
    }
}
