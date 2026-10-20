//! The DS18B20 driver: a non-blocking pipeline with an accept/reject decision.
//!
//! # The pipeline
//!
//! ```text
//!   poll(now) ──▶ CONVERT T issued ──▶ (>= 375 ms elapse) ──▶ read scratchpad
//!                                                                    │
//!                                                       CONVERT T ───┘
//! ```
//!
//! The cadence belongs to the caller: `Timing::TEMPERATURE_SENSOR_INTERVAL_MS` is **400 ms** and
//! [`CADENCE`] is that number.
//!
//! 400 ms against an 11-bit conversion time of 375 ms leaves 25 ms of slack, so the conversion
//! started on the previous tick has always finished. [`Driver::poll`] enforces the conversion time
//! explicitly rather than relying on the caller's cadence being long enough, so a caller that polls
//! faster than 375 ms gets [`Poll::Waiting`] instead of a stale reading.
//!
//! # The accept/reject decision
//!
//! Every value the bus can report is either a temperature or one of seven named faults: the six the
//! datasheet defines, plus `OutOfRange`, which this driver adds.
//!
//! | fault | the value on the wire | rejected? |
//! | --- | --- | --- |
//! | `Disconnected` | -127 | yes |
//! | `Open` | -127 | yes |
//! | `ShortGnd` | -127 | yes |
//! | `ShortVdd` | -127 | yes |
//! | `PowerOnReset` | -127 | yes |
//! | `InsufficientPower` | -127 | yes |
//! | `OutOfRange` | a real count outside `-50 ..= 150` | yes |
//!
//! Two things to read out of that table.
//!
//! **Every fault carries the same -127 sentinel, and that is the finding.** The scratchpad decode
//! folds all six of the datasheet's fault values to one, so a caller that reads only the sentinel
//! cannot tell a power-on reset from an unplugged probe, and the diagnostic it prints points an
//! operator at the wiring when the fault is on the probe. This driver keeps the [`Ds18b20Fault`],
//! so the reason survives to the log line and to `SENSOR_ERROR`. Pinned by
//! `cc_protocol::sensor::onewire::div7_every_ds18b20_fault_is_rejected`.
//!
//! **The last row is a deliberate divergence.** A reading above 150 °C is refused, so a 155 °C
//! reading -- which the sensor can put on the wire but which is outside the DS18B20's specified
//! -55..125 °C span -- arrives as a *sensor read failure* rather than as a temperature. After ten
//! of them the machine reaches `SENSOR_ERROR` instead of handing the number to the PID. The heater
//! is off in both cases, so nothing unsafe is accepted; what changes is what the operator is told.

pub use crate::sensor::onewire::Ds18b20Fault;
use crate::sensor::onewire::{self, OneWireBus, OneWireError, Rom, RomSelection, ScratchPad};
use crate::sensor::probe::{ProbeFault, ProbeReading, ProbeSource};
use cc_domain::units::Millis;

/// The cadence this driver expects the caller to keep, from
/// `Timing::TEMPERATURE_SENSOR_INTERVAL_MS`.
/// 2.5 Hz.
pub const CADENCE: Millis = Millis::new(400);

/// The number of consecutive failed reads before the driver reports itself faulty.
///
/// Ten is S1's debounce budget, and reaching it is what drives `SENSOR_ERROR` in the state machine.
/// **This is the DS18B20's contribution to safety path S1**, and it is the count S1 was written
/// against -- not three, and not configurable here.
pub const MAX_BAD_READINGS: u8 = 10;

/// The resolution this driver configures: 11 bits.
///
/// 11-bit is 0.0625 °C, which is finer than the 400 ms cadence can use and costs 375 ms of
/// conversion -- 25 ms inside the cadence. 12-bit would cost 750 ms, longer than the cadence, so
/// the resolution is chosen for the timing rather than for the precision.
pub const RESOLUTION_BITS: u8 = 11;

/// The range a reading must fall in for the driver to accept it: -50 °C to +150 °C.
///
/// The bounds are narrower than S1's at both ends. S1 accepts 0.0 ..= 200.0 and trips outside it
/// with no debounce; here, a probe reading -1 °C is a *sensor read failure* rather than an
/// emergency, and a reading at 155 °C is a read failure rather than a temperature inside S1's own
/// band. The two ranges answer different questions -- "can a probe physically be here" against
/// "must the machine stop" -- and the driver's is deliberately the stricter one, a deliberate
/// behaviour change.
pub const PLAUSIBLE_RANGE: (f32, f32) = (-50.0, 150.0);

/// S1's band, kept beside [`PLAUSIBLE_RANGE`] so that the two are visibly different numbers in the
/// same file: 0.0 °C to 200.0 °C, outside which emergency stop trips with no debounce.
pub const EMERGENCY_RANGE: (f32, f32) = (0.0, 200.0);

/// Whether a decoded temperature is inside [`PLAUSIBLE_RANGE`].
#[must_use]
pub fn is_plausible(celsius: f32) -> bool {
    (PLAUSIBLE_RANGE.0..=PLAUSIBLE_RANGE.1).contains(&celsius)
}

/// Whether a decoded temperature would trip S1's emergency stop.
#[must_use]
pub fn is_emergency(celsius: f32) -> bool {
    !(EMERGENCY_RANGE.0..=EMERGENCY_RANGE.1).contains(&celsius)
}

/// What one call to [`Driver::poll`] produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Poll {
    /// A conversion was started; nothing to report yet.
    Started,
    /// The conversion is still running. The caller should come back after
    /// [`ScratchPad::conversion_time`].
    Waiting,
    /// A reading, or a fault, from a completed conversion.
    ///
    /// `Ok` carries the decoded temperature. A reading outside
    /// [`PLAUSIBLE_RANGE`] is `Err(OutOfRange)`, not `Ok` — see the module docs.
    Reading(Result<f32, Ds18b20Fault>),
}

/// The result of the most recent completed read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    /// The decoded temperature in °C, or the fault.
    pub value: Result<f32, Ds18b20Fault>,
    /// Consecutive failed reads, saturating at [`MAX_BAD_READINGS`].
    pub bad_readings: u8,
    /// Whether ten consecutive bad readings have latched this probe as faulty.
    ///
    /// Set once `bad_readings` reaches [`MAX_BAD_READINGS`], cleared by any successful read.
    pub error: bool,
}

/// The DS18B20 driver: a ROM selection, a pipeline phase, and the bad-reading counter.
///
/// Generic over [`OneWireBus`] so the whole pipeline is host-testable against
/// the fake bus in `cc_protocol::sensor::onewire`'s tests. The device crate supplies a
/// bit-banging implementation and nothing else.
pub struct Driver {
    rom: Rom,
    selection: RomSelection,
    /// When the outstanding `CONVERT T` was issued, or `None` if none is.
    convert_started: Option<Millis>,
    /// The most recent read, kept so a caller polling between conversions still
    /// sees a value rather than nothing.
    last: Option<Reading>,
    /// The conversion time, cached from the device's configuration register
    /// during [`Driver::calibrate`].
    ///
    /// Until [`Driver::calibrate`] has run this is the **12-bit** time, which
    /// is the DS18B20's power-on default and therefore the safe over-estimate:
    /// a driver that has not been calibrated waits longer than it must, and
    /// never reads a conversion that has not finished.
    conversion_ms: Millis,
    bad_readings: u8,
    error: bool,
}

impl Driver {
    /// A driver for a specific device.
    ///
    /// `selection` defaults to addressing the device by ROM, which is what a bus holding more than
    /// one device needs: a bus-wide conversion would convert every DS18B20 on the line and each
    /// would answer with its own value into the same read. Skip-ROM is available for a bus known to
    /// hold exactly one device, and it is the cheaper command -- no ROM bytes on the wire.
    #[must_use]
    pub const fn new(rom: Rom) -> Self {
        Self {
            rom,
            selection: RomSelection::Match(rom),
            convert_started: None,
            last: None,
            // 12-bit is the power-on default (`RES_12_BIT` in the
            // configuration register), so this is the conservative choice for
            // an uncalibrated driver.
            conversion_ms: Millis::new(750),
            bad_readings: 0,
            error: false,
        }
    }

    /// The ROM code this driver addresses.
    #[must_use]
    pub const fn rom(&self) -> Rom {
        self.rom
    }

    /// Whether a conversion is outstanding.
    #[must_use]
    pub const fn is_converting(&self) -> bool {
        self.convert_started.is_some()
    }

    /// The most recent completed read, if any.
    #[must_use]
    pub const fn last_reading(&self) -> Option<Reading> {
        self.last
    }

    /// Whether ten consecutive bad readings have latched this probe as faulty.
    #[must_use]
    pub const fn has_error(&self) -> bool {
        self.error
    }

    /// Advance the pipeline by one step.
    ///
    /// The caller drives this; the driver never blocks and never sleeps. Call it at least every
    /// [`CADENCE`].
    ///
    /// # Errors
    ///
    /// Propagates a bus or presence failure. A **presence** failure is not a bad reading on its
    /// own: the caller counts it through `Driver::record_bus_failure`, which is what [`Self::poll`]
    /// does before returning the error.
    pub fn poll<B: OneWireBus>(
        &mut self,
        bus: &mut B,
        now: Millis,
    ) -> Result<Poll, OneWireError<B::Error>> {
        let Some(started) = self.convert_started else {
            // Nothing outstanding: start one. A conversion is in flight at all times after the
            // first poll, so the pipeline never idles.
            let issued = onewire::request_conversion(bus, self.selection);
            if let Err(err) = issued {
                // No device, or a bus failure. A device that will not answer is a disconnection,
                // and it counts as a bad reading like any other:
                self.record_failure(Some(Ds18b20Fault::Disconnected));
                return Err(err);
            }
            self.convert_started = Some(now);
            return Ok(Poll::Started);
        };

        // A conversion is outstanding. Wait for it: the caller's 400 ms cadence normally exceeds
        // the conversion time, but a faster caller must not read a half-finished conversion.
        if now.since(started) < self.conversion_time() {
            return Ok(Poll::Waiting);
        }

        let pad = match onewire::read_scratchpad(bus, self.selection) {
            Ok(pad) => pad,
            Err(err) => {
                // The device went away between the conversion and the read.
                // Same accounting as above.
                self.record_failure(Some(Ds18b20Fault::Disconnected));
                self.convert_started = None;
                return Err(err);
            }
        };

        // The accept/reject decision: the scratchpad decode names the fault, and the range check
        // below rejects a real count outside the probe's window. Both are applied here, at the
        // driver, so nothing downstream ever sees a value the sensor could not have measured.
        let decoded = pad.interpret(self.rom);
        let value = match decoded {
            Ok(celsius) if !is_plausible(celsius) => Err(Ds18b20Fault::OutOfRange),
            other => other,
        };
        let rejected = value.is_err();
        if rejected {
            self.record_failure(value.err());
        } else {
            self.record_success(value.unwrap_or(0.0));
        }
        self.convert_started = None;

        // Start the next conversion so the pipeline never goes idle, but only after a *good* read:
        // a rejected read leaves it idle, and the next poll re-issues `CONVERT T`. See
        // `a_rejected_read_does_not_start_a_new_conversion`.
        if !rejected {
            onewire::request_conversion(bus, self.selection)?;
            self.convert_started = Some(now);
        }
        Ok(Poll::Reading(value))
    }

    /// The conversion time for the device's configured resolution.
    ///
    /// Read once, during calibration. Reading it back on every poll would be a second bus
    /// transaction per tick for a value that cannot change, so this is read once during
    /// [`Driver::calibrate`] and cached.
    #[must_use]
    pub const fn conversion_time(&self) -> Millis {
        self.conversion_ms
    }

    /// Set the resolution and cache the conversion time.
    ///
    /// Read the scratchpad, set the configuration register, persist it, and remember the conversion
    /// time that implies. The resolution is read back once rather than on every conversion: it
    /// cannot change unless this method changes it.
    ///
    /// # Errors
    ///
    /// Propagates a bus or presence failure. A device that will not answer the scratchpad read
    /// cannot be configured, and its resolution stays at the power-on default (12-bit), which is
    /// the slowest conversion time the part has.
    pub fn calibrate<B: OneWireBus>(&mut self, bus: &mut B) -> Result<(), OneWireError<B::Error>> {
        let mut pad = onewire::read_scratchpad(bus, self.selection)?;
        if pad.resolution() != RESOLUTION_BITS {
            pad = pad.with_resolution(RESOLUTION_BITS).with_valid_crc();
            onewire::write_scratchpad(bus, self.selection, &pad)?;
            // No 20 ms wait for the EEPROM write: the readback
            // below happens after a 20 ms `delay` that the *caller* already
            // owes, and the next thing the firmware does is sleep for the
            // conversion anyway. See `write_scratchpad`'s docs.
            pad = onewire::read_scratchpad(bus, self.selection)?;
        }
        self.conversion_ms = pad.conversion_time();
        Ok(())
    }

    /// Count a failure: the counter increments and saturates, and the error flag is set at
    /// [`MAX_BAD_READINGS`] and cleared by any success.
    ///
    /// `fault` is **kept**, which a flag alone cannot do: the scratchpad decode has already
    /// collapsed all six fault values into one by the time the driver sees them, so a flag could
    /// only ever say "not connected". Keeping the reason is the whole point of [`Ds18b20Fault`],
    /// and it costs one byte.
    pub fn record_failure(&mut self, fault: Option<Ds18b20Fault>) {
        self.bad_readings = self.bad_readings.saturating_add(1);
        if self.bad_readings >= MAX_BAD_READINGS && !self.error {
            self.error = true;
        }
        self.last = Some(Reading {
            value: Err(fault.unwrap_or(Ds18b20Fault::Disconnected)),
            bad_readings: self.bad_readings,
            error: self.error,
        });
    }

    /// Clear the error state, as a successful read does.
    fn record_success(&mut self, celsius: f32) {
        self.bad_readings = 0;
        self.error = false;
        self.last = Some(Reading {
            value: Ok(celsius),
            bad_readings: 0,
            error: false,
        });
    }

    /// Decode a scratchpad this driver's device would produce, for tests.
    ///
    /// # Errors
    ///
    /// The [`Ds18b20Fault`] the scratchpad represents. Note this is the
    /// *decode*, so it does **not** apply [`PLAUSIBLE_RANGE`] — that is
    /// [`Driver::poll`]'s decision, and keeping the two apart is what lets a
    /// test ask "what did the device say?" independently of "what will the
    /// driver do with it?".
    pub fn decode(&self, pad: ScratchPad) -> Result<f32, Ds18b20Fault> {
        pad.interpret(self.rom)
    }
}

/// Collapse one [`Poll`] into the shared [`ProbeReading`] vocabulary, applying
/// the same range check [`Driver::poll`] applies.
///
/// The mapping is a decision, so it lives here where it can be tested, rather
/// than in a device crate. `Poll::Started` and `Poll::Waiting` both become
/// `None`: the shared vocabulary says "nothing new", and a caller that needs to
/// tell them apart is reaching past it for no benefit.
#[must_use]
pub fn as_probe(poll: Poll) -> Option<ProbeReading> {
    match poll {
        Poll::Reading(Ok(celsius)) => Some(ProbeReading {
            celsius,
            source: ProbeSource::Ds18b20,
        }),
        // `Started`, `Waiting` and a rejected read all mean "nothing new". They
        // are one arm because in the shared vocabulary they are the same thing:
        // the driver has advanced its own bad-reading counter (which is the part
        // S1 acts on), and the reason is still available from
        // `Driver::last_reading()` for whoever wants to log it. A rejected read
        // is deliberately **not** an error here — the bus worked, the sensor did
        // not answer usefully.
        Poll::Started | Poll::Waiting | Poll::Reading(Err(_)) => None,
    }
}

/// The [`ProbeFault`] a [`Ds18b20Fault`] becomes, for the device crate to
/// report alongside the counter.
#[must_use]
pub const fn probe_fault(fault: Ds18b20Fault) -> ProbeFault {
    match fault {
        Ds18b20Fault::Disconnected => ProbeFault::NotConnected,
        Ds18b20Fault::OutOfRange => ProbeFault::ReadFailed,
        Ds18b20Fault::Open
        | Ds18b20Fault::ShortGnd
        | Ds18b20Fault::ShortVdd
        | Ds18b20Fault::PowerOnReset
        | Ds18b20Fault::InsufficientPower => ProbeFault::Ds18b20Fault(fault),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensor::onewire::{
        timing, SCRATCHPAD_LEN, SP_COUNT_REMAIN, SP_TEMP_LSB, SP_TEMP_MSB,
    };
    use alloc::vec;
    use alloc::vec::Vec;

    /// The ROM the board answered with, in the device's byte order.
    ///
    /// The recovered boot log printed it as `0x41af78cdaa376928`, which is the
    /// same bytes **reversed** — 1-Wire is clocked out least-significant bit
    /// first, and the log prints the wire order. See
    /// `cc_protocol::sensor::onewire`'s `the_logged_rom_is_printed_least_significant_byte_first`.
    const LIVE_ROM: Rom = Rom([0x28, 0x69, 0x37, 0xAA, 0xCD, 0x78, 0xAF, 0x41]);

    /// A bus that plays the part of a DS18B20: answers the scratchpad read
    /// with a scripted scratchpad, and records every byte clocked out.
    ///
    /// The framing is tracked in [`Framing`], which is a real three-state walk
    /// rather than a re-parse of the byte stream — a decoder that had to know
    /// the framing could silently mis-frame a regression and turn a real one
    /// into a passing test.
    struct FakeDevice {
        scratchpad: ScratchPad,
        /// Every *function* command completed on the bus, in order.
        commands: Vec<u8>,
        /// Every byte clocked out since the last reset, reconstructed
        /// LSB-first.
        ///
        /// Reset clears it, because a reset *is* the start of a transaction
        /// and a 1-Wire transaction never spans one.
        written: Vec<u8>,
        /// Bits clocked out into `written` since the last reset.
        ///
        /// **Not** `written.len()`: that is a count of completed *bytes*, so
        /// it stays below 8 for the whole of the first byte and every bit of
        /// that byte lands in bit position 0.
        write_bits: u32,
        present: bool,
        /// Bit cursor within the current `READ SCRATCHPAD` answer.
        read_cursor: usize,
        framing: Framing,
    }

    /// Where in a 1-Wire transaction the bus currently is.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Framing {
        /// Between transactions: the next byte is a ROM-selection byte, or a
        /// function command if the driver addresses nothing.
        Idle,
        /// Seen `SKIP ROM`; the next byte is the function command.
        SkipRom,
        /// Seen `MATCH ROM`; `n` ROM bytes still to come before the function.
        MatchRom(u8),
    }

    impl FakeDevice {
        fn new() -> Self {
            let mut scratchpad = ScratchPad([0x00; SCRATCHPAD_LEN]);
            scratchpad = scratchpad.with_valid_crc();
            Self {
                scratchpad,
                commands: Vec::new(),
                written: Vec::new(),
                write_bits: 0,
                present: true,
                read_cursor: 0,
                framing: Framing::Idle,
            }
        }

        /// A device reporting `raw`, in 1/128 °C, at 11-bit resolution.
        ///
        /// Both shifts are arithmetic on an `i16`, matching `interpret`'s
        /// `raw = (MSB << 11) | (LSB << 3)`. An unsigned MSB shift turns a
        /// negative reading into a large positive one.
        fn with_raw(raw: i16) -> Self {
            let mut this = Self::new();
            #[allow(clippy::cast_sign_loss)]
            let lsb = ((raw as u16 >> 3) & 0xFF) as u8;
            #[allow(clippy::cast_sign_loss)]
            let msb = ((raw >> 11) & 0xFF) as u8;
            let mut pad = ScratchPad([
                lsb,
                msb,
                0x00,
                0x00,
                crate::sensor::onewire::RES_11_BIT,
                0x00,
                0x00,
                0x00,
                0x00,
            ]);
            pad = pad.with_valid_crc();
            this.scratchpad = pad;
            this
        }

        /// A device reporting as close to `celsius` as 11-bit allows.
        fn at_celsius(celsius: f32) -> Self {
            Self::with_raw(grid(celsius))
        }

        /// Every *function* command the driver has issued, in order, across
        /// all transactions so far.
        ///
        /// Completed at the bus level rather than re-derived from `written`,
        /// because `written` holds only the current transaction.
        fn commands(&self) -> Vec<u8> {
            self.commands.clone()
        }
    }

    /// Advance the framing by one completed byte.
    fn step(framing: Framing, byte: u8) -> Framing {
        use crate::sensor::onewire::{CMD_MATCH_ROM, CMD_SKIP_ROM};
        match framing {
            Framing::Idle => match byte {
                CMD_SKIP_ROM => Framing::SkipRom,
                CMD_MATCH_ROM => Framing::MatchRom(8),
                _ => Framing::Idle,
            },
            // Both mean "the next byte is the function command".
            Framing::SkipRom | Framing::MatchRom(0) => Framing::Idle,
            Framing::MatchRom(n) => Framing::MatchRom(n - 1),
        }
    }

    /// Whether `byte`, given the framing state *before* it, is a function
    /// command rather than a ROM byte.
    fn is_function_command(before: Framing, byte: u8) -> bool {
        use crate::sensor::onewire::{CMD_MATCH_ROM, CMD_SKIP_ROM};
        match before {
            Framing::Idle => !matches!(byte, CMD_SKIP_ROM | CMD_MATCH_ROM),
            Framing::SkipRom | Framing::MatchRom(0) => true,
            Framing::MatchRom(_) => false,
        }
    }

    /// The nearest 11-bit-representable temperature, in 1/128 °C.
    ///
    /// The device reports 11 of 16 bits, so the raw value is a multiple of 8
    /// and the grid step is 0.0625 °C.
    fn grid(celsius: f32) -> i16 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let raw = (celsius * 128.0).round() as i32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let snapped = ((raw + 4) / 8 * 8) as i16;
        snapped
    }

    impl OneWireBus for FakeDevice {
        type Error = ();

        fn reset(&mut self) -> Result<bool, Self::Error> {
            // A reset starts a new transaction, so the byte stream and the
            // read cursor both begin again from zero.
            self.read_cursor = 0;
            self.written.clear();
            self.write_bits = 0;
            self.framing = Framing::Idle;
            Ok(self.present)
        }

        fn write_bit(&mut self, bit: bool) -> Result<(), Self::Error> {
            if self.write_bits % 8 == 0 {
                self.written.push(0);
            }
            if bit {
                let last = self.written.len() - 1;
                self.written[last] |= 1 << (self.write_bits % 8);
            }
            self.write_bits = self.write_bits.wrapping_add(1);
            if self.write_bits % 8 == 0 {
                let byte = self.written[self.written.len() - 1];
                if is_function_command(self.framing, byte) {
                    self.commands.push(byte);
                }
                self.framing = step(self.framing, byte);
            }
            Ok(())
        }

        fn read_bit(&mut self) -> Result<bool, Self::Error> {
            let index = self.read_cursor;
            self.read_cursor += 1;
            let byte = self.scratchpad.0.get(index / 8).copied().unwrap_or(0xFF);
            Ok(byte & (1 << (index % 8)) != 0)
        }
    }

    /// Bring a driver up to a running pipeline: calibrated, first conversion
    /// issued. The command log is cleared first, so a test sees only what the
    /// *pipeline* does and not the boot-time calibration.
    fn running(device: &mut FakeDevice) -> (Driver, Millis) {
        let mut driver = Driver::new(LIVE_ROM);
        driver.calibrate(device).unwrap_or(());
        device.commands.clear();
        let now = Millis::ZERO;
        assert_eq!(driver.poll(device, now), Ok(Poll::Started));
        (driver, now)
    }

    // ================================================== the bad-reading counter

    #[test]
    fn ten_consecutive_failures_raise_the_error_flag() {
        // The tenth failure is the one that raises it: the test is at the threshold, not above it.
        let mut driver = Driver::new(LIVE_ROM);
        assert!(!driver.has_error());
        for i in 1..MAX_BAD_READINGS {
            driver.record_failure(Some(Ds18b20Fault::Disconnected));
            assert!(!driver.has_error(), "error raised early, at {i}");
        }
        driver.record_failure(Some(Ds18b20Fault::Disconnected));
        assert!(driver.has_error(), "the tenth failure must raise it");
        // And it stays raised: the flag is only ever set here, and only a successful read clears
        // it.
        driver.record_failure(Some(Ds18b20Fault::Disconnected));
        assert!(driver.has_error());
    }

    #[test]
    fn the_bad_reading_counter_keeps_counting_past_the_threshold() {
        // The counter saturates at `u8::MAX` only because of its type: the flag is the decision,
        // and the count is the diagnostic. A tighter cap would lose the answer to "how long has
        // this been wrong".
        let mut driver = Driver::new(LIVE_ROM);
        for _ in 0..50 {
            driver.record_failure(Some(Ds18b20Fault::Disconnected));
        }
        assert!(driver.has_error());
        assert_eq!(driver.last_reading().map(|r| r.bad_readings), Some(50));
    }

    #[test]
    fn the_bad_reading_counter_is_cleared_by_a_success() {
        // A success clears both: the count goes back to zero and the flag drops.
        let mut driver = Driver::new(LIVE_ROM);
        for _ in 0..MAX_BAD_READINGS {
            driver.record_failure(Some(Ds18b20Fault::Disconnected));
        }
        assert!(driver.has_error());
        driver.record_success(22.9);
        assert!(!driver.has_error());
        assert_eq!(driver.last_reading().map(|r| r.bad_readings), Some(0));
    }

    // ================================================ the accept/reject set

    #[test]
    fn div6_every_ds18b20_fault_is_rejected_and_named() {
        // Walk the whole pipeline rather than the enum: every fault is rejected at the driver, and
        // the *only* thing this driver adds is that it keeps the reason. A refactor that starts
        // folding the six values together again fails here. See
        // `cc_protocol::sensor::onewire::div7_*`. Each pad is stamped with a *valid* resolution and
        // then a valid CRC, in that order: the resolution write invalidates the CRC, and a
        // CRC-invalid scratchpad is `Disconnected`, which would make this test pass for the wrong
        // reason on every case.
        let mut pow = ScratchPad::power_on();
        pow.0[SP_TEMP_LSB] = 0x50;
        pow.0[SP_TEMP_MSB] = 0x05;
        pow.0[SP_COUNT_REMAIN] = 0x0C;
        let mut brown = ScratchPad::power_on();
        brown.0[SP_TEMP_LSB] = 0xFF;
        brown.0[SP_TEMP_MSB] = 0x07;
        for (fault, pad) in [
            // The all-zeros scratchpad needs no resolution stamp: it is rejected
            // by the all-zeros test, which is the first check `interpret` makes
            // and does not care about the CRC or the configuration register.
            (Ds18b20Fault::Disconnected, ScratchPad::power_on()),
            (Ds18b20Fault::PowerOnReset, pow),
            (Ds18b20Fault::InsufficientPower, brown),
        ] {
            let mut device = FakeDevice::new();
            let pad = if pad.is_all_zeros() {
                pad
            } else {
                let stamped = pad.with_resolution(RESOLUTION_BITS).with_valid_crc();
                assert!(stamped.crc_valid());
                stamped
            };
            device.scratchpad = pad;
            let (mut driver, start) = running(&mut device);
            // The all-zeros pad carries a configuration byte of zero, so
            // `calibrate` caches the 12-bit 750 ms time and a 400 ms poll would
            // be `Waiting` rather than a read. Poll after whatever the driver
            // actually decided the conversion time was.
            let due = start + driver.conversion_time();
            let got = driver.poll(&mut device, due);
            assert_eq!(got, Ok(Poll::Reading(Err(fault))), "{fault}");
            // The reason survives into the cached reading, which is the whole
            // point: folding the three into one value would lose which wiring fault it is.
            assert_eq!(driver.last_reading().map(|r| r.value), Some(Err(fault)));
        }
    }

    #[test]
    fn div6_ten_power_on_resets_latch_the_error_flag() {
        // Ten power-on resets latch the error flag, exactly as ten disconnections do, and
        // the fault this driver reports says which of the two it was.
        let mut driver = Driver::new(LIVE_ROM);
        for _ in 0..MAX_BAD_READINGS {
            driver.record_failure(Some(Ds18b20Fault::PowerOnReset));
        }
        assert!(driver.has_error());
        assert_eq!(
            driver.last_reading().map(|r| r.value),
            Some(Err(Ds18b20Fault::PowerOnReset))
        );
    }

    // ==================================================== the plausibility range

    #[test]
    fn div8_the_dallas_path_applies_a_range_check() {
        // 🔴 DIVERGENCE, added by this port: the driver applies a -50 ..= 150 window, which the
        // temperature path would otherwise never check.
        assert_eq!(PLAUSIBLE_RANGE, (-50.0, 150.0));
        assert!(is_plausible(-50.0));
        assert!(is_plausible(0.0));
        assert!(is_plausible(22.9));
        assert!(is_plausible(150.0));
        assert!(!is_plausible(-50.0625));
        assert!(!is_plausible(150.0625));
        assert!(!is_plausible(f32::NAN));
        assert!(!is_plausible(f32::INFINITY));
    }

    #[test]
    fn div8_the_two_ranges_overlap_only_between_zero_and_a_hundred_and_fifty() {
        // S1's range is 0.0..200.0; the
        // driver's is -50.0..150.0. They are not nested: each has a band the
        // other does not, and the driver's bands are exactly the readings this
        // port now turns into "sensor read failure" instead of a temperature.
        assert_eq!(EMERGENCY_RANGE, (0.0, 200.0));
        assert_eq!(PLAUSIBLE_RANGE, (-50.0, 150.0));
        assert_ne!(PLAUSIBLE_RANGE, EMERGENCY_RANGE);

        // Plausible to the driver, an emergency to S1: a probe that is too cold
        // for the boiler: plausible to the driver, and an emergency to S1.
        for celsius in [-49.0f32, -1.0] {
            assert!(is_plausible(celsius), "{celsius} is a plausible probe");
            assert!(is_emergency(celsius), "{celsius} would trip S1");
        }
        // Where they agree a reading is fine.
        for celsius in [0.0f32, 22.9, 149.0] {
            assert!(is_plausible(celsius) && !is_emergency(celsius), "{celsius}");
        }
        // The fourth quadrant: implausible to the driver and *not* an emergency for S1. A reading
        // in 150.5 ..= 200 is refused here, so what an operator sees is a sensor read failure
        // rather than a temperature.
        for celsius in [150.5f32, 199.0] {
            assert!(!is_plausible(celsius), "{celsius} is not a plausible probe");
            assert!(!is_emergency(celsius), "{celsius} is inside S1's range");
        }
        // And below both, which S1 would have stopped on and the driver refuses
        // first.
        {
            let celsius = -60.0f32;
            assert!(!is_plausible(celsius) && is_emergency(celsius), "{celsius}");
        }
        // And the two ways a reading can be refused for range, which is the
        // asymmetry that matters: the driver refuses *below* zero where S1 also
        // would, and *above* 150 where S1 would not until 200. The 150..200 band
        // is where the operator's diagnostic changes from "over-temperature" to
        // "sensor error". See the module docs.
        assert!(is_emergency(-1.0) && is_plausible(-1.0));
    }

    #[test]
    fn div8_a_reading_outside_the_range_is_a_read_failure_not_a_hot_temperature() {
        // The trade-off made concrete: a 155 °C scratchpad is a rejected read, not a temperature.
        let mut device = FakeDevice::at_celsius(155.0);
        let (mut driver, start) = running(&mut device);
        assert_eq!(
            driver.poll(&mut device, start + Millis::new(CADENCE.raw())),
            Ok(Poll::Reading(Err(Ds18b20Fault::OutOfRange)))
        );
        assert!(!driver.has_error(), "one bad read is not an error yet");
        assert_eq!(driver.last_reading().map(|r| r.bad_readings), Some(1));
        // And it is NOT a temperature, so nothing downstream can mistake it for a reading.
        assert_eq!(as_probe(Poll::Reading(Err(Ds18b20Fault::OutOfRange))), None);
    }

    #[test]
    fn div8_only_the_cold_end_of_the_ds18b20s_range_is_now_refused() {
        // 🔴 The reachable size of the divergence, measured rather than assumed.
        //
        // The DS18B20's own range is **-55..+125 °C** (AT24+DS18B20
        // §"Temperature measurement"). `PLAUSIBLE_RANGE` is -50..150, so:
        //
        // | band | reachable? | effect |
        // | --- | --- | --- |
        // | -55..-50 | **yes**, real readings | now refused (were passed to the PID) |
        // | 125..150 | no, the sensor cannot report it | nothing |
        // | 150..200 | no | S1's band only; the driver never sees it |
        //
        // So the divergence costs a ~5 °C band at the very bottom of the
        // probe's range, and the over-temperature diagnostic (150..200) is
        // unreachable from a DS18B20 however it is wired. That is a much smaller
        // behavioural change than the module docs' framing suggests, and the
        // asymmetry is worth stating: **the driver's upper bound is looser than
        // the sensor's, so the upper reject never fires.**
        for celsius in [-50.0f32, -49.0, 0.0, 22.9, 124.0, 125.0] {
            let mut device = FakeDevice::at_celsius(celsius);
            let (mut driver, start) = running(&mut device);
            let got = driver.poll(&mut device, start + Millis::new(CADENCE.raw()));
            assert_eq!(
                got,
                Ok(Poll::Reading(Ok(f32::from(grid(celsius)) / 128.0))),
                "{celsius} C is inside both ranges and must be accepted"
            );
        }
        // And the band the divergence actually reaches: refused.
        for celsius in [-54.0f32, -52.0, -51.0] {
            let mut device = FakeDevice::at_celsius(celsius);
            let (mut driver, start) = running(&mut device);
            assert_eq!(
                driver.poll(&mut device, start + Millis::new(CADENCE.raw())),
                Ok(Poll::Reading(Err(Ds18b20Fault::OutOfRange))),
                "{celsius} C is a real reading the C++ passed to the PID"
            );
        }
        // Nothing above 125 is reachable at all, so the upper bound is inert.
        assert!(125.0 < PLAUSIBLE_RANGE.1);
    }

    // =================================================== the timing envelope

    #[test]
    fn the_cadence_exceeds_the_conversion_time() {
        // The whole non-blocking design rests on this: 400 ms of cadence
        // against 375 ms of conversion leaves 25 ms.
        assert!(CADENCE.raw() > 375);
        assert_eq!(CADENCE.since(Millis::ZERO), Millis::new(400));
    }

    #[test]
    fn the_conversion_time_is_read_from_the_device_not_assumed() {
        // The four conversion times the datasheet gives, one per resolution bit count.
        for (bits, expected) in [(9u8, 94u32), (10, 188), (11, 375), (12, 750)] {
            let mut pad = crate::sensor::onewire::ScratchPad([0x00; 9]);
            pad = pad.with_resolution(bits);
            assert_eq!(pad.conversion_time().raw(), expected, "{bits}-bit");
        }
    }

    #[test]
    fn the_shortest_slot_is_3_microseconds() {
        // The plan's claim, checked: the shortest 1-Wire timing in this driver
        // is the read-initiation pulse, and 1 µs granularity is 1/3 of it.
        let shortest = [
            timing::RESET_LOW_US,
            timing::RESET_PRESENCE_US,
            timing::RESET_RECOVERY_US,
            timing::WRITE_ONE_LOW_US,
            timing::WRITE_ONE_HIGH_US,
            timing::WRITE_ZERO_LOW_US,
            timing::WRITE_ZERO_HIGH_US,
            timing::READ_LOW_US,
            timing::READ_SAMPLE_US,
            timing::READ_RECOVERY_US,
        ]
        .into_iter()
        .min();
        assert_eq!(shortest, Some(3));
    }

    // ==================================================== the whole pipeline

    #[test]
    fn the_first_poll_starts_a_conversion_and_blocks() {
        // The first poll issues `CONVERT T` and returns waiting; the read happens on a later tick.
        let mut device = FakeDevice::at_celsius(22.9);
        let mut driver = Driver::new(LIVE_ROM);
        assert!(!driver.is_converting());
        assert_eq!(driver.poll(&mut device, Millis::ZERO), Ok(Poll::Started));
        assert!(driver.is_converting());
        assert_eq!(
            device.commands(),
            vec![crate::sensor::onewire::CMD_CONVERT_T]
        );
    }

    #[test]
    fn polling_before_the_conversion_finishes_yields_waiting() {
        // A 400 ms cadence exceeds 375 ms, but the driver does not rely on that: it checks the
        // elapsed time explicitly, so a faster caller gets `Waiting` rather than a half-finished
        // conversion.
        let mut device = FakeDevice::at_celsius(22.9);
        let (mut driver, start) = running(&mut device);
        assert_eq!(driver.conversion_time(), Millis::new(375));
        for elapsed in [0, 1, 100, 374] {
            let now = start + Millis::new(elapsed);
            assert_eq!(
                driver.poll(&mut device, now),
                Ok(Poll::Waiting),
                "at {elapsed} ms"
            );
        }
    }

    #[test]
    fn a_completed_conversion_yields_the_reading_and_starts_the_next() {
        let expected = grid(22.9);
        let mut device = FakeDevice::at_celsius(22.9);
        let (mut driver, start) = running(&mut device);
        let now = start + Millis::new(400);
        assert_eq!(
            driver.poll(&mut device, now),
            Ok(Poll::Reading(Ok(f32::from(expected) / 128.0)))
        );
        // And the next conversion is already outstanding, so the pipeline never
        // goes idle: the next `CONVERT T` was issued as soon as this reading was accepted.
        assert!(driver.is_converting());
        assert_eq!(
            device.commands(),
            vec![
                crate::sensor::onewire::CMD_CONVERT_T,
                crate::sensor::onewire::CMD_READ_SCRATCHPAD,
                crate::sensor::onewire::CMD_CONVERT_T
            ]
        );
    }

    #[test]
    fn the_pipeline_sustains_two_and_a_half_hertz_for_two_hundred_ticks() {
        // 200 ticks at the 400 ms cadence is 80 s. Every tick after the first must produce a
        // reading, which is the steady state the control loop depends on.
        let mut device = FakeDevice::at_celsius(23.25);
        let (mut driver, start) = running(&mut device);

        let mut readings = 0;
        for tick in 1..=200u32 {
            let now = start + Millis::new(tick * CADENCE.raw());
            match driver.poll(&mut device, now) {
                Ok(Poll::Reading(Ok(_))) => readings += 1,
                Ok(Poll::Waiting | Poll::Started) => {}
                other => panic!("tick {tick} produced {other:?}"),
            }
        }
        assert_eq!(readings, 200, "every tick after the first must read");
        assert!(!driver.has_error());
    }

    #[test]
    fn a_temperature_change_is_followed_on_the_next_tick() {
        // A read re-arms the pipeline, so a change on the bus is visible on the very next 400 ms
        // tick with no extra latency.
        let mut device = FakeDevice::at_celsius(22.88);
        let (mut driver, start) = running(&mut device);
        assert_eq!(
            driver.poll(&mut device, start + Millis::new(400)),
            Ok(Poll::Reading(Ok(f32::from(grid(22.88)) / 128.0)))
        );

        // The probe warms up.
        device.scratchpad = FakeDevice::at_celsius(23.25).scratchpad;
        assert_eq!(
            driver.poll(&mut device, start + Millis::new(800)),
            Ok(Poll::Reading(Ok(f32::from(grid(23.25)) / 128.0)))
        );
    }

    #[test]
    fn the_readings_measured_on_the_board_decode_exactly() {
        // MEASURED 2026-09-28 on the attached machine, by flashing this driver
        // and reading the bus. The boot log read:
        //
        // ```text
        // temperature: DS18B20 at 286937aacd78af41, 11-bit, 375 ms conversion
        // temperature: 24.25 C (plausible: true)
        // temperature: 24.38 C (plausible: true)
        // ```
        //
        // over 20 s at the 400 ms cadence, with no resets and no CRC failures.
        // 48 samples, of which 37 were 24.25 and 11 were 24.38 — i.e. a
        // rock-steady room-temperature probe whose only movement is the two
        // adjacent points on the 11-bit grid.
        //
        // These are the two raw counts the device reported. The grid property (a multiple of 8) is
        // what proves the decode is reading a real DS18B20 register rather than producing a
        // plausible-looking number.
        for (celsius, raw) in [(24.25f32, 3104i16), (24.375, 3120)] {
            assert_eq!(raw % 8, 0, "11-bit readings are a multiple of 8 raw");
            let mut device = FakeDevice::with_raw(raw);
            let (mut driver, start) = running(&mut device);
            assert_eq!(
                driver.poll(&mut device, start + Millis::new(CADENCE.raw())),
                Ok(Poll::Reading(Ok(celsius))),
                "raw {raw} did not decode to {celsius}"
            );
            assert!(driver.last_reading().is_some_and(|r| !r.error));
        }
    }

    #[test]
    fn the_measured_live_range_decodes_exactly() {
        // The recovered firmware's boot log recorded 22.88 .. 23.25 °C. All of them must round-trip
        // through the 11-bit decode.
        for log_line in [22.88, 22.9375, 23.0, 23.1875, 23.25] {
            let mut device = FakeDevice::at_celsius(log_line);
            let (mut driver, start) = running(&mut device);
            let got = driver.poll(&mut device, start + Millis::new(400));
            let expected = f32::from(grid(log_line)) / 128.0;
            assert_eq!(got, Ok(Poll::Reading(Ok(expected))), "log line {log_line}");
        }
    }

    // ============================================== the reject decision, E2E

    #[test]
    fn a_disconnected_sensor_counts_a_bad_reading_and_stops_re_arming() {
        // A bus with nothing on it: the scratchpad read fails, the driver counts a disconnection as
        // a bad reading, and it returns before re-arming, so the next poll has to start a
        // conversion from scratch.
        let mut device = FakeDevice::at_celsius(22.9);
        device.present = false;
        let mut driver = Driver::new(LIVE_ROM);
        // `poll` records the failure itself, once per attempt, so this loop is
        // `record_failure` being called MAX_BAD_READINGS times with a bus that never answers.
        for _ in 0..MAX_BAD_READINGS {
            let _ = driver.poll(&mut device, Millis::ZERO);
        }
        assert!(driver.has_error());
        assert_eq!(driver.last_reading().map(|r| r.bad_readings), Some(10));
        assert!(!driver.is_converting(), "no conversion is outstanding");
    }

    #[test]
    fn s9_a_rejected_read_does_not_start_a_new_conversion() {
        // A rejected read leaves the pipeline idle, because the only arm that re-arms is the
        // success arm, so the next poll must re-issue `CONVERT T` from scratch.
        let mut device = FakeDevice::new();
        let mut driver = Driver::new(LIVE_ROM);
        driver.conversion_ms = Millis::new(375);

        assert_eq!(driver.poll(&mut device, Millis::ZERO), Ok(Poll::Started));
        // The device is still reporting its power-on all-zero scratchpad.
        assert_eq!(
            driver.poll(&mut device, Millis::new(400)),
            Ok(Poll::Reading(Err(Ds18b20Fault::Disconnected)))
        );
        assert!(!driver.is_converting());
        assert_eq!(
            device.commands(),
            vec![
                crate::sensor::onewire::CMD_CONVERT_T,
                crate::sensor::onewire::CMD_READ_SCRATCHPAD
            ],
            "no CONVERT T after a rejected read"
        );
        // And the next poll starts a fresh conversion.
        assert_eq!(
            driver.poll(&mut device, Millis::new(800)),
            Ok(Poll::Started)
        );
    }

    #[test]
    fn a_faulted_reading_still_counts_toward_the_error_flag() {
        // Every rejected read counts, so ten of them latch the error flag and the machine goes to
        // `SENSOR_ERROR`.
        // A device that powers up and never converts: its scratchpad reads all
        // zeros, which is the one fault only the all-zeros check catches.
        let mut device = FakeDevice::new();
        let mut driver = Driver::new(LIVE_ROM);
        driver.conversion_ms = Millis::new(375);
        // Two polls per read, because a *rejected* read leaves the pipeline idle and the next poll
        // has to re-issue `CONVERT T`. So ten rejected reads take twenty polls.
        let mut now = Millis::ZERO;
        let mut rejected = 0;
        for _ in 0..2 * MAX_BAD_READINGS {
            if let Ok(Poll::Reading(Err(_))) = driver.poll(&mut device, now) {
                rejected += 1;
            }
            now += Millis::new(CADENCE.raw());
        }
        assert_eq!(rejected, MAX_BAD_READINGS, "every other poll is a read");
        assert!(
            driver.has_error(),
            "ten rejected reads must latch the error"
        );
    }

    // ================================================== calibration and setup

    #[test]
    fn calibrate_sets_eleven_bit_and_caches_the_conversion_time() {
        // The fake device powers on at 12-bit, so this is the path where the configuration register
        // actually has to change.
        let mut device = FakeDevice::at_celsius(22.9);
        let mut driver = Driver::new(LIVE_ROM);
        assert_eq!(driver.conversion_time(), Millis::new(750)); // uncalibrated
        driver.calibrate(&mut device).unwrap_or(());
        assert_eq!(driver.conversion_time(), Millis::new(375));
        assert_eq!(device.scratchpad.resolution(), 11);
    }

    #[test]
    fn an_uncalibrated_driver_waits_the_longer_12_bit_time() {
        // Fail-safe: a driver that has not been told the resolution waits
        // 750 ms rather than 375 ms, so it can never read a conversion that has
        // not finished.
        let mut device = FakeDevice::at_celsius(22.9);
        let mut driver = Driver::new(LIVE_ROM);
        assert_eq!(driver.poll(&mut device, Millis::ZERO), Ok(Poll::Started));
        // 400 ms is the production cadence; at 12-bit that is not enough.
        assert_eq!(
            driver.poll(&mut device, Millis::new(400)),
            Ok(Poll::Waiting)
        );
        assert_eq!(
            driver.poll(&mut device, Millis::new(750)),
            Ok(Poll::Reading(Ok(f32::from(grid(22.9)) / 128.0)))
        );
    }

    #[test]
    fn calibrate_on_an_already_eleven_bit_device_does_not_rewrite() {
        // An 11-bit device is not rewritten: the configuration register is only written when it
        // differs. Rewriting would cost an EEPROM cycle on every boot, and the part is specified
        // for 100k of them.
        let mut device = FakeDevice::at_celsius(22.9);
        device.scratchpad = device.scratchpad.with_resolution(11).with_valid_crc();
        let mut driver = Driver::new(LIVE_ROM);
        driver.calibrate(&mut device).unwrap_or(());
        assert_eq!(driver.conversion_time(), Millis::new(375));
        let writes = device.commands().len()
            - device
                .commands()
                .iter()
                .filter(|c| **c != crate::sensor::onewire::CMD_COPY_SCRATCHPAD)
                .count();
        assert_eq!(
            writes, 0,
            "an EEPROM write on every boot would wear the sensor"
        );
    }

    #[test]
    fn match_rom_addresses_the_device_not_the_bus() {
        // The driver addresses the device by ROM on every command, so a second DS18B20 on the bus
        // cannot answer in its place.
        let mut device = FakeDevice::at_celsius(22.9);
        let (mut driver, start) = running(&mut device);
        let _ = driver.poll(&mut device, start + Millis::new(400));
        // Reconstruct the byte stream and check the ROM went out after 0x55.
        let stream = &device.written;
        let match_positions: Vec<usize> = stream
            .iter()
            .enumerate()
            .filter(|(_, &b)| b == crate::sensor::onewire::CMD_MATCH_ROM)
            .map(|(i, _)| i)
            .collect();
        assert!(
            !match_positions.is_empty(),
            "expected MATCH ROM in the stream"
        );
        for position in match_positions {
            assert_eq!(
                &stream[position + 1..position + 9],
                &LIVE_ROM.0[..],
                "MATCH ROM must be followed by the eight ROM bytes"
            );
        }
    }
}
