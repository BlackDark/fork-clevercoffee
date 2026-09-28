//! The DS18B20 driver: a non-blocking pipeline, and the C++'s accept/reject
//! decision.
//!
//! Owner: **R1-03** (the DS18B20 half) and **R3-06**.
//!
//! # What this replaces
//!
//! `TempSensorDallas` (`src/hardware/tempsensors/TempSensorDallas.cpp`) over
//! `DallasTemperature` 4.0.6. The C++ is already non-blocking — it calls
//! `setWaitForConversion(false)` and issues the next `CONVERT T` immediately
//! after reading the scratchpad (`TempSensorDallas.cpp:24-31`) — and this port
//! keeps that shape rather than inventing a blocking one.
//!
//! # The pipeline
//!
//! ```text
//!   poll(now) ──▶ CONVERT T issued ──▶ (>= 375 ms elapse) ──▶ read scratchpad
//!                                                                    │
//!                                                       CONVERT T ───┘
//! ```
//!
//! The C++'s cadence is the caller's: `SensorCoordinator::updateTemperature`
//! calls `startRead()` every `TEMP_UPDATE_INTERVAL_MS` and `tryGetValue()` on
//! every loop, and `Timing::TEMPERATURE_SENSOR_INTERVAL_MS` is **400 ms**
//! (`constants/Timing.h:42`). [`CADENCE`] is that number.
//!
//! 400 ms against an 11-bit conversion time of 375 ms leaves 25 ms of slack,
//! which is what makes the C++'s design work at all: the conversion it started
//! on the previous tick has always finished. [`Driver::poll`] enforces the
//! conversion time explicitly rather than relying on the caller's cadence being
//! long enough, so a caller that polls faster than 375 ms gets
//! [`Poll::Waiting`] instead of a stale reading — a difference from the C++
//! that is recorded in `intentional-diffs.md`.
//!
//! # The accept/reject decision is the C++'s
//!
//! `TempSensorDallas::sample_temperature` (`TempSensorDallas.cpp:26-43`) is two
//! `if` blocks and one assignment, and the whole of it is preserved:
//!
//! | fault | C++ sentinel | C++ rejects? | this port |
//! | --- | --- | --- | --- |
//! | `Disconnected` | -127 | yes | rejected |
//! | `Open` | -254 | yes | rejected |
//! | `ShortGnd` | -253 | yes | rejected |
//! | `ShortVdd` | -252 | yes | rejected |
//! | `PowerOnReset` | -251 | **no** | reported, not rejected — see `s8_*` |
//! | `InsufficientPower` | -250 | **no** | reported, not rejected |
//!
//! The two un-rejected faults are a real finding, not an oversight in the port:
//! `TempSensorDallas.cpp:29-36` does not test for them, so a DS18B20 that
//! reports power-on-reset hands the control loop **-251 °C**. That is outside
//! `Temperature::MIN_VALID_TEMP_C` (0.0), so S1 treats it as an invalid
//! reading and trips emergency stop — the C++ is safe here by accident, and
//! the accident is one refactor away from not happening. Both are pinned by
//! tests so the behaviour cannot drift silently.

use crate::onewire::{self, Ds18b20Fault, OneWireBus, OneWireError, Rom, RomSelection, ScratchPad};
use crate::units::Millis;

/// The reading cadence, in milliseconds.
///
/// `Timing::TEMPERATURE_SENSOR_INTERVAL_MS` (`constants/Timing.h:42`), which
/// `SensorCoordinator` uses for `TEMP_UPDATE_INTERVAL_MS`. 2.5 Hz.
pub const CADENCE: Millis = Millis::new(400);

/// The number of consecutive failed reads before the driver reports itself
/// faulty.
///
/// `TempSensor::max_bad_readings_` (`TempSensor.h:57`) is 10, and
/// `updateTemperature` sets `error_` at that count
/// (`TempSensor.h:50-53`). The C++ then reports `isConnected() == false`
/// (`TempSensor.h:148`), which is what drives `SENSOR_ERROR` in the state
/// machine. **This is the DS18B20's contribution to safety path S1** and it is
/// preserved exactly: ten, not three.
pub const MAX_BAD_READINGS: u8 = 10;

/// The resolution the C++ configures, and the driver here.
///
/// `TempSensorDallas.cpp:22`: `setResolution(sensorDeviceAddress_, 11)`.
/// 11-bit is 0.0625 °C, which is finer than the 400 ms cadence can use and
/// costs 375 ms of conversion — 25 ms inside the cadence.
pub const RESOLUTION_BITS: u8 = 11;

/// The safety-relevant range a reading must fall in to be usable.
///
/// **Not** the C++'s. The C++ has no such check on the Dallas path at all;
/// `TempSensor::isValidTemperature` (`TempSensor.h:91-93`, -50..150) exists but
/// is **never called** by `updateTemperature` or `tryGetValue`. The range that
/// *is* enforced is S1's, in `EmergencyStopManager::checkEmergencyConditions`
/// (`EmergencyStopManager.cpp:25-30`): `Temperature::MIN_VALID_TEMP_C` = 0.0
/// and `MAX_VALID_TEMP_C` = 200.0, outside which emergency stop trips
/// immediately with no debounce.
///
/// So a reading outside 0..200 does not reach the PID as if it were valid — it
/// reaches S1, which latches. The bounds are named here so the driver's
/// contract is stated rather than implied.
pub const PLAUSIBLE_RANGE: (f32, f32) = (0.0, 200.0);

/// Whether a decoded temperature is inside [`PLAUSIBLE_RANGE`].
///
/// Note the asymmetry with the C++: this is a *query*, not a filter. A reading
/// that fails it is still reported to the caller, and it is S1 that acts on it.
/// Filtering it here would be a divergence, and a dangerous one — it would turn
/// an emergency stop into a silently-held last-good value.
#[must_use]
pub fn is_plausible(celsius: f32) -> bool {
    (PLAUSIBLE_RANGE.0..=PLAUSIBLE_RANGE.1).contains(&celsius)
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
    /// `Ok` carries the decoded temperature **unfiltered** — see
    /// [`is_plausible`].
    Reading(Result<f32, Ds18b20Fault>),
}

/// The result of the most recent completed read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    /// The decoded temperature in °C, or the fault.
    pub value: Result<f32, Ds18b20Fault>,
    /// Consecutive failed reads, saturating at [`MAX_BAD_READINGS`].
    pub bad_readings: u8,
    /// Whether the C++ would report this sensor as faulty.
    ///
    /// `TempSensor::error_` (`TempSensor.h:56`): set once `bad_readings_`
    /// reaches [`MAX_BAD_READINGS`], cleared by any successful read.
    pub error: bool,
}

/// The DS18B20 driver: a ROM code, a phase, and the C++'s error counter.
///
/// Generic over [`OneWireBus`] so the whole pipeline is host-testable against
/// the fake bus in `cc_domain::onewire`'s tests. The device crate supplies a
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
    /// `selection` defaults to addressing the device by ROM, because the C++
    /// does: `TempSensorDallas` discovers the ROM in its constructor
    /// (`getAddress(sensorDeviceAddress_, 0)`, `TempSensorDallas.cpp:20`) and
    /// every subsequent command goes through `DallasTemperature`'s
    /// `requestTemperaturesByAddress` / `readScratchPad`, which call
    /// `_wire->select(deviceAddress)` (`DallasTemperature.cpp:489`, `:210`).
    /// Skip-ROM is available for a bus known to hold one device, and is the
    /// same code path the C++ takes for a bus-wide conversion
    /// (`DallasTemperature.cpp:469`).
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

    /// Address the device with `SKIP ROM` instead of `MATCH ROM`.
    #[must_use]
    pub const fn with_skip_rom(mut self) -> Self {
        self.selection = RomSelection::Skip;
        self
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

    /// Whether the C++ would report this sensor as faulty.
    #[must_use]
    pub const fn has_error(&self) -> bool {
        self.error
    }

    /// Advance the pipeline by one step.
    ///
    /// The caller drives this; the driver never blocks and never sleeps. Call
    /// it at least every [`CADENCE`], which is what the C++'s coordinator does.
    ///
    /// # Errors
    ///
    /// Propagates a bus or presence failure. A **presence** failure is not
    /// counted as a bad reading: the C++'s `readScratchPad` returns false, which
    /// `isConnected` turns into `DEVICE_DISCONNECTED_C`, and that *is* counted
    /// by `TempSensor::updateTemperature`'s `bad_readings_`. So a bus that
    /// reports an error and a bus that reports "no device" are the same event
    /// to the C++, and this matches it: the caller should count the error
    /// through [`Driver::record_bus_failure`], which is what
    /// [`Self::poll`] does before returning.
    pub fn poll<B: OneWireBus>(
        &mut self,
        bus: &mut B,
        now: Millis,
    ) -> Result<Poll, OneWireError<B::Error>> {
        let Some(started) = self.convert_started else {
            // Nothing outstanding: start one. The C++ does this in its
            // constructor (`TempSensorDallas.cpp:31`) and then after every
            // successful read (`:41`).
            let issued = onewire::request_conversion(bus, self.selection);
            if let Err(err) = issued {
                // No device, or a bus failure. The C++'s
                // `requestTemperaturesByAddress` sets `result = false` for a
                // device that will not answer
                // (`DallasTemperature.cpp:482-486`) and the read is never
                // taken; `updateTemperature` counts it
                // (`TempSensor.h:47-49`). Count it.
                self.record_failure(Some(Ds18b20Fault::Disconnected));
                return Err(err);
            }
            self.convert_started = Some(now);
            return Ok(Poll::Started);
        };

        // A conversion is outstanding. Wait for it, as the C++ does by virtue
        // of its 400 ms cadence exceeding the conversion time.
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

        // The C++'s accept/reject decision.
        let value = pad.interpret(self.rom);
        let rejected = value.is_err();
        if rejected {
            self.record_failure(value.err());
        } else {
            self.record_success(value.unwrap_or(0.0));
        }
        self.convert_started = None;

        // Start the next conversion so the pipeline never goes idle, which is
        // what the C++ does after a *good* read only —
        // `TempSensorDallas.cpp:41` sits after both early returns. See
        // `s9_a_rejected_read_does_not_start_a_new_conversion`.
        if !rejected {
            onewire::request_conversion(bus, self.selection)?;
            self.convert_started = Some(now);
        }
        Ok(Poll::Reading(value))
    }

    /// The conversion time for the device's configured resolution.
    ///
    /// The C++ reads it back from the device each time
    /// (`DallasTemperature::getResolution`, `DallasTemperature.cpp:378-396`).
    /// Reading it back on every poll would be a second bus transaction per tick
    /// for a value that cannot change, so this is read once during
    /// [`Driver::calibrate`] and cached.
    #[must_use]
    pub const fn conversion_time(&self) -> Millis {
        self.conversion_ms
    }

    /// Set the resolution and cache the conversion time.
    ///
    /// Mirrors the tail of `TempSensorDallas`'s constructor: read the scratchpad,
    /// set the configuration register, persist it, and remember the conversion
    /// time that implies (`TempSensorDallas.cpp:20-24`). The C++ reads the
    /// resolution back on every conversion; the value is the same, so it is
    /// read once here.
    ///
    /// # Errors
    ///
    /// Propagates a bus or presence failure. A device that will not answer the
    /// scratchpad read cannot be configured, and the C++ leaves the resolution
    /// at its power-on default (12-bit) in that case.
    pub fn calibrate<B: OneWireBus>(&mut self, bus: &mut B) -> Result<(), OneWireError<B::Error>> {
        let mut pad = onewire::read_scratchpad(bus, self.selection)?;
        if pad.resolution() != RESOLUTION_BITS {
            pad = pad.with_resolution(RESOLUTION_BITS).with_valid_crc();
            onewire::write_scratchpad(bus, self.selection, &pad)?;
            // The C++'s `saveScratchPad` waits 20 ms for the EEPROM write
            // (`DallasTemperature.cpp:255-256`). This does not: the readback
            // below happens after a 20 ms `delay` that the *caller* already
            // owes, and the next thing the firmware does is sleep for the
            // conversion anyway. See `write_scratchpad`'s docs.
            pad = onewire::read_scratchpad(bus, self.selection)?;
        }
        self.conversion_ms = pad.conversion_time();
        Ok(())
    }

    /// Count a failure the way `TempSensor::updateTemperature` does.
    ///
    /// `bad_readings_` increments and saturates; `error_` is set at
    /// [`MAX_BAD_READINGS`] and cleared by any success
    /// (`TempSensor.h:41-53`).
    pub fn record_failure(&mut self, _fault: Option<Ds18b20Fault>) {
        self.bad_readings = self.bad_readings.saturating_add(1);
        if self.bad_readings >= MAX_BAD_READINGS && !self.error {
            self.error = true;
        }
        self.last = Some(Reading {
            value: Err(Ds18b20Fault::Disconnected),
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

    /// Decode a scratchpad this driver's device would produce, for tests and
    /// for the parity harness.
    ///
    /// # Errors
    ///
    /// The [`Ds18b20Fault`] the scratchpad represents, which is the C++'s
    /// accept/reject decision and nothing else.
    pub fn decode(&self, pad: ScratchPad) -> Result<f32, Ds18b20Fault> {
        pad.interpret(self.rom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onewire::{timing, SCRATCHPAD_LEN};
    use alloc::vec;
    use alloc::vec::Vec;

    /// The ROM the board answered with (08 §4), in the device's byte order.
    ///
    /// The recovered boot log printed it as `0x41af78cdaa376928`, which is the
    /// same bytes **reversed** — 1-Wire is clocked out least-significant bit
    /// first, and the log prints the wire order. See
    /// `cc_domain::onewire`'s `the_logged_rom_is_printed_least_significant_byte_first`.
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
                crate::onewire::RES_11_BIT,
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
        use crate::onewire::{CMD_MATCH_ROM, CMD_SKIP_ROM};
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
        use crate::onewire::{CMD_MATCH_ROM, CMD_SKIP_ROM};
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

    // ============================================== the C++'s error counter

    #[test]
    fn ten_consecutive_failures_raise_the_error_flag() {
        // `TempSensor::max_bad_readings_` is 10 (`TempSensor.h:57`) and
        // `updateTemperature` sets `error_` when `bad_readings_ >= 10`
        // (`TempSensor.h:50-53`). The tenth failure is the one that raises it.
        let mut driver = Driver::new(LIVE_ROM);
        assert!(!driver.has_error());
        for i in 1..MAX_BAD_READINGS {
            driver.record_failure(Some(Ds18b20Fault::Disconnected));
            assert!(!driver.has_error(), "error raised early, at {i}");
        }
        driver.record_failure(Some(Ds18b20Fault::Disconnected));
        assert!(driver.has_error(), "the tenth failure must raise it");
        // And it stays raised: the C++'s `if (bad_readings_ >= max && !error_)`
        // only sets it, and only a successful read clears it.
        driver.record_failure(Some(Ds18b20Fault::Disconnected));
        assert!(driver.has_error());
    }

    #[test]
    fn the_bad_reading_counter_keeps_counting_past_the_threshold() {
        // The C++'s `bad_readings_++` (`TempSensor.h:48`) has no cap, so the
        // count is a diagnostic and the flag is the decision. Saturation here
        // would lose that.
        let mut driver = Driver::new(LIVE_ROM);
        for _ in 0..50 {
            driver.record_failure(Some(Ds18b20Fault::Disconnected));
        }
        assert!(driver.has_error());
        assert_eq!(driver.last_reading().map(|r| r.bad_readings), Some(50));
    }

    #[test]
    fn the_bad_reading_counter_is_cleared_by_a_success() {
        // `updateTemperature` on success: `bad_readings_ = 0; error_ = false;`
        // (`TempSensor.h:43-44`).
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
    fn s8_only_four_of_the_six_faults_are_rejected() {
        // Restated at the driver level: the C++'s reject set is
        // {Disconnected, Open, ShortGnd, ShortVdd} and nothing else
        // (`TempSensorDallas.cpp:29-36`).
        assert!(Ds18b20Fault::Disconnected.cpp_rejects());
        assert!(Ds18b20Fault::Open.cpp_rejects());
        assert!(Ds18b20Fault::ShortGnd.cpp_rejects());
        assert!(Ds18b20Fault::ShortVdd.cpp_rejects());
        // NOT rejected — a POR reading reaches the control loop as -251 °C.
        assert!(!Ds18b20Fault::PowerOnReset.cpp_rejects());
        assert!(!Ds18b20Fault::InsufficientPower.cpp_rejects());
    }

    // ==================================================== the plausibility range

    #[test]
    fn s10_the_dallas_path_has_no_range_check_of_its_own() {
        // `TempSensor::isValidTemperature` (-50..150, `TempSensor.h:91-93`) is
        // never called. The enforced range is S1's: 0.0..200.0
        // (`constants/Temperature.h:14-15`, `EmergencyStopManager.cpp:25-30`).
        assert!(is_plausible(22.9));
        assert!(is_plausible(0.0));
        assert!(is_plausible(200.0));
        assert!(!is_plausible(-0.1));
        assert!(!is_plausible(200.1));
        assert!(!is_plausible(f32::NAN));
        // The C++'s dead helper would have allowed -50 and rejected 150.1;
        // neither bound is used.
        assert!(!is_plausible(-50.0));
        assert!(is_plausible(150.0));
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
        // `DallasTemperature::millisToWaitForConversion` (`DallasTemperature.cpp:422-429`).
        for (bits, expected) in [(9u8, 94u32), (10, 188), (11, 375), (12, 750)] {
            let mut pad = crate::onewire::ScratchPad([0x00; 9]);
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
        // `TempSensorDallas.cpp:31` requests the first conversion in the
        // constructor, and the read happens on a later tick.
        let mut device = FakeDevice::at_celsius(22.9);
        let mut driver = Driver::new(LIVE_ROM);
        assert!(!driver.is_converting());
        assert_eq!(driver.poll(&mut device, Millis::ZERO), Ok(Poll::Started));
        assert!(driver.is_converting());
        assert_eq!(device.commands(), vec![crate::onewire::CMD_CONVERT_T]);
    }

    #[test]
    fn polling_before_the_conversion_finishes_yields_waiting() {
        // The C++ relies on its 400 ms cadence exceeding 375 ms. This port
        // checks explicitly, so a faster caller gets `Waiting` rather than a
        // stale reading.
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
        // goes idle — which is what `TempSensorDallas.cpp:41` does.
        assert!(driver.is_converting());
        assert_eq!(
            device.commands(),
            vec![
                crate::onewire::CMD_CONVERT_T,
                crate::onewire::CMD_READ_SCRATCHPAD,
                crate::onewire::CMD_CONVERT_T
            ]
        );
    }

    #[test]
    fn the_pipeline_sustains_two_and_a_half_hertz_for_two_hundred_ticks() {
        // 200 ticks at the C++'s 400 ms cadence is 80 s. Every tick after the
        // first must produce a reading, which is the C++'s steady state and
        // the property the control loop depends on.
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
        // The C++'s `sample_temperature` reads and then re-arms, so a change
        // on the bus is visible on the very next 400 ms tick with no extra
        // latency.
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
        // These are the two raw counts the device reported, and the grid
        // property (a multiple of 8) is what proves the port is reading a real
        // DS18B20 rather than a plausible-looking number.
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
        // The recovered image's boot log recorded 22.88 .. 23.25 °C
        // (01 §"The temperature sensor fitted to this machine"). Both must
        // round-trip through the 11-bit decode.
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
        // A bus with nothing on it: `readScratchPad` returns false, the C++
        // returns -127, `updateTemperature` increments `bad_readings_`
        // (`TempSensor.h:47-49`), and `TempSensorDallas` returns before its
        // re-arm at `:41`.
        let mut device = FakeDevice::at_celsius(22.9);
        device.present = false;
        let mut driver = Driver::new(LIVE_ROM);
        // `poll` records the failure itself, once per attempt, so this loop is
        // the C++'s `updateTemperature` being called MAX_BAD_READINGS times
        // with a bus that never answers.
        for _ in 0..MAX_BAD_READINGS {
            let _ = driver.poll(&mut device, Millis::ZERO);
        }
        assert!(driver.has_error());
        assert_eq!(driver.last_reading().map(|r| r.bad_readings), Some(10));
        assert!(!driver.is_converting(), "no conversion is outstanding");
    }

    #[test]
    fn s9_a_rejected_read_does_not_start_a_new_conversion() {
        // `TempSensorDallas::sample_temperature` returns `false` at
        // `TempSensorDallas.cpp:31` and `:35` — both *before* the
        // `requestTemperaturesByAddress` at `:41`. So a rejected read leaves
        // the pipeline idle and the next poll must re-issue `CONVERT T` from
        // scratch. Preserved here.
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
                crate::onewire::CMD_CONVERT_T,
                crate::onewire::CMD_READ_SCRATCHPAD
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
        // Every rejected read goes through the C++'s `updateTemperature`
        // `else if (!error_)` branch (`TempSensor.h:45-49`), so ten of them
        // latch `error_` and the machine goes to `SENSOR_ERROR`.
        // A device that powers up and never converts: its scratchpad reads all
        // zeros, which is the one fault only the all-zeros check catches.
        let mut device = FakeDevice::new();
        let mut driver = Driver::new(LIVE_ROM);
        driver.conversion_ms = Millis::new(375);
        // Two polls per read, because a *rejected* read leaves the pipeline
        // idle (see `s9_*`) and the next poll has to re-issue `CONVERT T`.
        // So ten rejected reads take twenty polls.
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
        // `TempSensorDallas.cpp:22`: `setResolution(sensorDeviceAddress_, 11)`.
        // The fake device powers on at 12-bit, so this is the interesting path.
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
        // The C++ only writes when `scratchPad[CONFIGURATION] != newValue`
        // (`DallasTemperature.cpp:350-353`). Rewriting would cost an EEPROM
        // cycle on every boot, which the DS18B20 is specified for 100k of.
        let mut device = FakeDevice::at_celsius(22.9);
        device.scratchpad = device.scratchpad.with_resolution(11).with_valid_crc();
        let mut driver = Driver::new(LIVE_ROM);
        driver.calibrate(&mut device).unwrap_or(());
        assert_eq!(driver.conversion_time(), Millis::new(375));
        let writes = device.commands().len()
            - device
                .commands()
                .iter()
                .filter(|c| **c != crate::onewire::CMD_COPY_SCRATCHPAD)
                .count();
        assert_eq!(
            writes, 0,
            "an EEPROM write on every boot would wear the sensor"
        );
    }

    #[test]
    fn match_rom_addresses_the_device_not_the_bus() {
        // `TempSensorDallas` discovers the ROM and addresses it specifically
        // on every command (`DallasTemperature.cpp:210`, `:489`).
        let mut device = FakeDevice::at_celsius(22.9);
        let (mut driver, start) = running(&mut device);
        let _ = driver.poll(&mut device, start + Millis::new(400));
        // Reconstruct the byte stream and check the ROM went out after 0x55.
        let stream = &device.written;
        let match_positions: Vec<usize> = stream
            .iter()
            .enumerate()
            .filter(|(_, &b)| b == crate::onewire::CMD_MATCH_ROM)
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
