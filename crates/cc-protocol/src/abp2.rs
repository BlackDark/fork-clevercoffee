//! The Honeywell ABP2 pressure sensor, and the one real performance win in
//! this migration.
//!
//! Owner: **R3-05**.
//!
//! # What this replaces
//!
//! `include/clevercoffee/hardware/pressureSensor.h` — a header-only function,
//! `measurePressure()`, called from `SensorCoordinator::updatePressure`
//! (`src/coordinators/SensorCoordinator.cpp:112-123`).
//!
//! # The blocking read, and why it is the win
//!
//! ```cpp
//! inline float measurePressure() {
//!     Wire.beginTransmission(ABP2_id);
//!     int stat  = Wire.write(ABP2_cmd, 3);
//!     stat     |= Wire.endTransmission();
//!     delay(ABP2_READ_DELAY_MS);          // <-- 10 ms, unconditionally
//!
//!     Wire.requestFrom(ABP2_id, static_cast<uint8_t>(7));
//!     for (unsigned char& i : ABP2_data) { i = Wire.read(); }
//!     ...
//! }
//! ```
//!
//! `SensorCoordinator::update()` runs **every loop iteration**, and
//! `PRESSURE_UPDATE_INTERVAL_MS` is **50 ms** (`SensorCoordinator.h:271`). So
//! the control loop spends `delay(10)` out of every 50 ms — **20 % of the
//! entire control loop asleep, unconditionally, forever**, whether or not
//! anything is brewing.
//!
//! That is the finding. The fix is not an optimisation, it is structural: the
//! ABP2 is a command-then-read device, so the 10 ms is a *deadline to wait
//! until*, not a *duration to sit through*. [`Driver::poll`] writes the command
//! on one tick and reads on a later one, and the waiting happens in the
//! caller's own sleep. The loop does 0 ms of pressure-related blocking.
//!
//! The consequence for the control loop's timing is the thing R2-09b measures,
//! and it is one of only two real performance wins this migration has
//! (the other is R1-07's LEDC heater replacing the 100 Hz ISR chopper).
//!
//! # The error checking the C++ does not do
//!
//! The C++ computes `stat` from `Wire.write()` and `Wire.endTransmission()`
//! and then **never reads it**. It also ignores `requestFrom`'s return value,
//! so a short read leaves stale bytes from the previous sample in the globals
//! and the firmware converts *those* as if they were fresh.
//!
//! This port checks both, and that is a deliberate divergence — see
//! `intentional-diffs.md`. It is the safe direction: the alternative is a
//! plausible-looking pressure built from data the sensor never sent, feeding
//! `SensorCoordinator::cachedPressureFiltered_` and therefore the brew
//! pressure control.

use cc_domain::units::{Bar, Celsius, Millis};

/// The ABP2's 7-bit I²C address.
///
/// `inline uint8_t ABP2_id = 0x28;` (`pressureSensor.h:14`). The bus is shared
/// with the OLED (`I2C0`, GPIO21/22, `pinmapping.h:53-54`), so every
/// transaction here is bounded and must not hold the bus.
pub const ADDRESS: u8 = 0x28;

/// The command that starts a conversion: read the first four output words.
///
/// `inline uint8_t ABP2_cmd[3] = {0xAA, 0x00, 0x00};` (`pressureSensor.h:16`).
/// `0xAA` is the ABP2's "output all" command; the two zero bytes select which
/// of the four words to return.
pub const COMMAND: [u8; 3] = [0xAA, 0x00, 0x00];

/// The number of bytes one ABP2 read returns.
///
/// `Wire.requestFrom(ABP2_id, static_cast<uint8_t>(7))`
/// (`pressureSensor.h:39`).
pub const RESPONSE_LEN: usize = 7;

/// How long the C++ waits between the command and the read.
///
/// `ABP2_READ_DELAY_MS` (`pressureSensor.h:10`). The ABP2 datasheet's
/// conversion time is 2 ms typical / 6.6 ms maximum for this part; 10 ms is
/// the C++'s margin over that. **Preserved as the read deadline** rather than
/// as a sleep — see the module docs.
pub const READ_DELAY: Millis = Millis::new(10);

/// The reading cadence.
///
/// `PRESSURE_UPDATE_INTERVAL_MS = 50` (`SensorCoordinator.h:271`).
pub const CADENCE: Millis = Millis::new(50);

/// The output count at minimum pressure.
///
/// `ABP2_outputmin` (`pressureSensor.h:19`). This is the fitted part's 10 %
/// of full scale, not zero: the ABP2's transfer function is deliberately
/// offset, and the datasheet's equation 2 subtracts this before scaling.
pub const OUTPUT_MIN: f64 = 1_677_722.0;

/// The output count at maximum pressure.
///
/// `ABP2_outputmax` (`pressureSensor.h:18`).
pub const OUTPUT_MAX: f64 = 15_099_494.0;

/// The full-scale count of the 24-bit output.
///
/// `16777215.0`, used for the percentage and the temperature scale
/// (`pressureSensor.h:50-52`).
pub const FULL_SCALE: f64 = 16_777_215.0;

/// The fitted part's minimum pressure, in bar.
///
/// `ABP2_pmin` (`pressureSensor.h:20`). A differential gauge at 0 bar
/// reference.
pub const P_MIN: f64 = 0.0;

/// The fitted part's maximum pressure, in bar.
///
/// `ABP2_pmax` (`pressureSensor.h:19`).
pub const P_MAX: f64 = 10.0;

/// The fitted part's full-scale temperature span, in °C.
///
/// `270.0` in `pressureSensor.h:51`. The ABP2's digital temperature output
/// spans 270 °C of a 24-bit count.
pub const FULL_SCALE_TEMPERATURE_C: f64 = 270.0;

/// The fitted part's temperature zero offset, in °C.
///
/// The `- 40.0` in `pressureSensor.h:51`: the digital temperature is reported
/// over -40 .. +230 °C.
pub const TEMPERATURE_OFFSET_C: f64 = -40.0;

/// One decoded ABP2 sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// The raw 24-bit pressure output count.
    pub pressure_counts: u32,
    /// The raw 24-bit temperature output count.
    pub temperature_counts: u32,
    /// Pressure in bar, by the datasheet's equation 2.
    pub pressure: Bar,
    /// Pressure as a percentage of full scale.
    pub percentage: f64,
    /// The sensor's own die temperature, in °C.
    pub temperature: Celsius,
}

/// Convert a raw pressure count to bar.
///
/// Datasheet equation 2, as the C++ writes it
/// (`pressureSensor.h:53-54`):
///
/// ```cpp
/// ABP2_pressure = (ABP2_press_counts - ABP2_outputmin) * (ABP2_pmax - ABP2_pmin)
///                 / (ABP2_outputmax - ABP2_outputmin) + ABP2_pmin;
/// ```
///
/// Note this is **not** the same as `counts / FULL_SCALE * P_MAX`: the ABP2's
/// output is offset, so the 10 bar span covers `OUTPUT_MAX - OUTPUT_MIN`
/// counts, not `FULL_SCALE`. Using the full scale would make 10 bar unreachable
/// and misreport every pressure below about 1.67 bar.
#[must_use]
pub fn counts_to_bar(pressure_counts: u32) -> Bar {
    let counts = f64::from(pressure_counts);
    let bar = (counts - OUTPUT_MIN) * (P_MAX - P_MIN) / (OUTPUT_MAX - OUTPUT_MIN) + P_MIN;
    // A count below the offset means the sensor is reading below its specified
    // range. The C++ reports it as a negative pressure and the value flows on.
    // Clamping to zero is a divergence and a strictly safer one — but it is a
    // divergence, so it is recorded rather than smuggled in here. Until it is
    // decided, this returns the C++'s value unclamped, and
    // `is_below_range` exposes the condition so a caller can act on it.
    #[allow(clippy::cast_possible_truncation)]
    Bar::new(bar as f32)
}

/// Convert a raw temperature count to °C.
///
/// `pressureSensor.h:51`:
///
/// ```cpp
/// ABP2_temperature = ABP2_temp_counts * 270.0 / 16777215.0 - 40.0;
/// ```
#[must_use]
pub fn counts_to_celsius(temperature_counts: u32) -> Celsius {
    let counts = f64::from(temperature_counts);
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let celsius = (counts * FULL_SCALE_TEMPERATURE_C / FULL_SCALE + TEMPERATURE_OFFSET_C) as f32;
    Celsius::new(celsius)
}

/// Convert a raw pressure count to a percentage of full scale.
///
/// `pressureSensor.h:50`: `ABP2_press_counts / 16777215.0 * 100.0`.
///
/// Note this uses the **full scale**, not the output span, so it disagrees with
/// [`counts_to_bar`] at the ends. That is the C++'s arithmetic, preserved.
#[must_use]
pub fn counts_to_percentage(pressure_counts: u32) -> f64 {
    f64::from(pressure_counts) / FULL_SCALE * 100.0
}

/// Whether a pressure count is below the part's specified range.
///
/// True when [`counts_to_bar`] would return a negative pressure. The C++
/// reports it as a negative number and carries on; the reading is retained
/// because the sensor can legitimately be a little under its specified minimum
/// at rest, and a single negative count is not proof of a fault.
#[must_use]
pub fn is_below_range(pressure_counts: u32) -> bool {
    f64::from(pressure_counts) < OUTPUT_MIN
}

/// Decode a seven-byte ABP2 response.
///
/// The C++ assembles the counts as `data[3] + data[2] * 256 + data[1] * 65536`
/// for pressure and `data[6] + data[5] * 256 + data[4] * 65536` for temperature
/// (`pressureSensor.h:45-48`) — i.e. **byte 0 is discarded** and bytes 1..3 and
/// 4..6 are each a 24-bit big-endian count. Byte 0 holds the sensor's status
/// and diagnostic bits, which the C++ never reads.
///
/// The `f64` arithmetic and the order of operations are the C++'s, so the
/// results are bit-identical.
#[must_use]
pub fn decode(data: &[u8; RESPONSE_LEN]) -> Sample {
    let pressure_counts =
        u32::from(data[3]) + (u32::from(data[2]) << 8) + (u32::from(data[1]) << 16);
    let temperature_counts =
        u32::from(data[6]) + (u32::from(data[5]) << 8) + (u32::from(data[4]) << 16);
    Sample {
        pressure_counts,
        temperature_counts,
        pressure: counts_to_bar(pressure_counts),
        percentage: counts_to_percentage(pressure_counts),
        temperature: counts_to_celsius(temperature_counts),
    }
}

/// The low-pass filter the coordinator applies to the pressure reading.
///
/// `SensorCoordinator::filterPressureValue`
/// (`src/coordinators/SensorCoordinator.cpp:152-159`):
///
/// ```cpp
/// y(n) = 0.3 * x(n) + 0.7 * y(n-1)
/// ```
///
/// It is pure arithmetic, so it lives here and is host-testable; the C++ keeps
/// it in the coordinator as two `float` members.
#[must_use]
pub fn filter(previous: Bar, input: Bar) -> Bar {
    let out = 0.3f32 * input.raw() + 0.7f32 * previous.raw();
    Bar::new(out)
}

/// What one call to [`Driver::poll`] produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Poll {
    /// Nothing due yet: less than [`CADENCE`] since the last read.
    Idle,
    /// The command has been written; the sample is not ready until
    /// [`READ_DELAY`] has passed.
    Waiting,
    /// A decoded sample.
    Sample(Sample),
}

/// A read that failed, and why.
///
/// The C++ has no way to report one: it ignores `endTransmission`'s return and
/// `requestFrom`'s count, so a bus error is indistinguishable from a good
/// reading built on stale bytes. See the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The command could not be written to the bus.
    CommandFailed,
    /// The response could not be read, or fewer than [`RESPONSE_LEN`] bytes came
    /// back.
    ///
    /// The short-read case is the one the C++ cannot see, and it is the
    /// dangerous one: the C++ would decode whatever was left in its globals
    /// from the previous sample.
    ShortRead {
        /// How many bytes the bus actually delivered.
        got: usize,
        /// How many the ABP2 is required to deliver.
        want: usize,
    },
}

impl core::fmt::Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CommandFailed => f.write_str("the ABP2 command was not acknowledged"),
            Self::ShortRead { got, want } => {
                write!(f, "short read: {got} of {want} bytes")
            }
        }
    }
}

/// The ABP2 driver: a deadline, and the last sample.
///
/// Generic over [`I2cBus`] so the state machine is host-testable against a fake
/// bus. The device crate supplies the real [`I2cBus`] over `hal::i2c`.
pub struct Driver {
    /// When the command was last written, or `None` if none is outstanding.
    requested: Option<Millis>,
    /// When the next read may start, or `None` if one may start now.
    ///
    /// `None` initially, so the very first poll reads. The C++ has no warm-up:
    /// `SensorCoordinator::updatePressure` calls `measurePressure()` on the
    /// first loop iteration (`SensorCoordinator.cpp:112-123`), and adding a
    /// 50 ms wait before the first reading would be a behaviour change with no
    /// safety benefit.
    next_due: Option<Millis>,
    last: Option<Sample>,
}

impl Driver {
    /// A driver that has not yet read anything.
    ///
    /// The first [`Self::poll`] reads immediately: the C++ has no such warm-up
    /// delay either, and a 50 ms wait before the first pressure reading would
    /// be a behaviour change with no safety benefit.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            requested: None,
            next_due: None,
            last: None,
        }
    }

    /// The most recent decoded sample, if any.
    #[must_use]
    pub const fn last_sample(&self) -> Option<Sample> {
        self.last
    }

    /// Whether a command is outstanding and not yet readable.
    #[must_use]
    pub const fn is_converting(&self) -> bool {
        self.requested.is_some()
    }

    /// Advance the read by one step. Never blocks, never sleeps.
    ///
    /// The C++ blocks [`READ_DELAY`] here; this does not. The 10 ms becomes a
    /// deadline, and the loop's own sleep covers it.
    ///
    /// # Errors
    ///
    /// [`ReadError`], where the C++ silently produced a sample from stale
    /// bytes. A caller should keep the previous value and try again on the
    /// next tick — it must **not** substitute a zero, which the PID would read
    /// as a real 0 bar.
    pub fn poll<B: I2cBus>(&mut self, bus: &mut B, now: Millis) -> Result<Poll, ReadError> {
        // Nothing outstanding: is it time to start a conversion?
        if self.requested.is_none() {
            // `next_due` is a *deadline*, not an interval, so the test is
            // `now.has_reached(deadline)`. Writing it as an interval
            // comparison is the bug this avoids: `now.since(deadline) <
            // CADENCE` is false for a `now` before the deadline, because
            // `since` wraps to ~4.29e9.
            if self.next_due.is_some_and(|due| !now.has_reached(due)) {
                return Ok(Poll::Idle);
            }
            bus.write(ADDRESS, &COMMAND)
                .map_err(|_| ReadError::CommandFailed)?;
            self.requested = Some(now);
            return Ok(Poll::Waiting);
        }

        // A command is outstanding. The C++ sleeps 10 ms here; this returns
        // `Waiting` and lets the caller sleep instead.
        let started = self.requested.unwrap_or(now);
        if !now.has_reached(started + READ_DELAY) {
            return Ok(Poll::Waiting);
        }

        let mut buffer = [0u8; RESPONSE_LEN];
        let got = bus
            .read(ADDRESS, &mut buffer)
            .map_err(|_| ReadError::ShortRead {
                got: 0,
                want: RESPONSE_LEN,
            })?;
        if got != RESPONSE_LEN {
            // **This is the check the C++ does not make.** `requestFrom`
            // returns the number of bytes actually available and the C++
            // discards it, so a device that answered with 3 bytes would leave
            // 4 stale bytes in `ABP2_data` and the firmware would convert them
            // as a valid reading.
            self.requested = None;
            self.next_due = Some(started + CADENCE);
            return Err(ReadError::ShortRead {
                got,
                want: RESPONSE_LEN,
            });
        }

        // The next command is due [`CADENCE`] after the *command* that produced
        // this sample, not after the read of it.
        //
        // The difference is [`READ_DELAY`], and it is the whole design: the
        // C++ re-issues 50 ms after its own call, and that call *contains* the
        // 10 ms sleep, so its true period is 50 ms with 10 ms of it asleep.
        // Anchoring on the read would give a 60 ms period and halve the update
        // rate — a behaviour change dressed as a refactor.
        self.requested = None;
        self.next_due = Some(started + CADENCE);
        let sample = decode(&buffer);
        self.last = Some(sample);
        Ok(Poll::Sample(sample))
    }
}

impl Default for Driver {
    fn default() -> Self {
        Self::new()
    }
}

/// The two I²C operations an ABP2 read needs.
///
/// Deliberately returns the **byte count** on `read`, which is the value the
/// C++ throws away. `hal::i2c`'s `read` does not expose a count
/// (`esp-idf-hal-0.47.0/src/i2c.rs:295-312` — it returns `Result<(), EspError>`
/// and takes a `&mut [u8]`), so the device implementation must count the bytes
/// itself from the peripheral, and this is where that count is checked.
pub trait I2cBus {
    /// What the transport reports.
    type Error;

    /// Write `bytes` to `address`.
    ///
    /// # Errors
    ///
    /// Whatever the transport reports, including a NACK on the address — the
    /// case the C++'s `stat` would have caught if it had read `stat`.
    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error>;

    /// Read into `buffer` and return how many bytes were actually read.
    ///
    /// # Errors
    ///
    /// Whatever the transport reports. A short read is **not** an error here:
    /// it is a returned count, so the driver can distinguish "the bus failed"
    /// from "the device answered with less than it should have".
    fn read(&mut self, address: u8, buffer: &mut [u8]) -> Result<usize, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec::Vec;

    /// A bus that records transactions and answers with a scripted response.
    struct FakeBus {
        /// Bytes written, as `(address, payload)` pairs.
        writes: Vec<(u8, [u8; 3])>,
        /// How many bytes the next read will report as available.
        available: usize,
        /// Whether the bus itself fails.
        fails: bool,
        response: [u8; RESPONSE_LEN],
    }

    impl FakeBus {
        fn new() -> Self {
            Self {
                writes: Vec::new(),
                available: RESPONSE_LEN,
                fails: false,
                response: [0x00; RESPONSE_LEN],
            }
        }

        /// A bus answering with the given raw pressure and temperature counts.
        fn with_counts(pressure: u32, temperature: u32) -> Self {
            let mut this = Self::new();
            // Byte 0 is status and is discarded by the decode, so it stays 0.
            this.response[1] = ((pressure >> 16) & 0xFF) as u8;
            this.response[2] = ((pressure >> 8) & 0xFF) as u8;
            this.response[3] = (pressure & 0xFF) as u8;
            this.response[4] = ((temperature >> 16) & 0xFF) as u8;
            this.response[5] = ((temperature >> 8) & 0xFF) as u8;
            this.response[6] = (temperature & 0xFF) as u8;
            this
        }
    }

    impl I2cBus for FakeBus {
        type Error = ();

        fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error> {
            if self.fails {
                return Err(());
            }
            let mut payload = [0u8; 3];
            for (slot, byte) in payload.iter_mut().zip(bytes) {
                *slot = *byte;
            }
            self.writes.push((address, payload));
            Ok(())
        }

        fn read(&mut self, _address: u8, buffer: &mut [u8]) -> Result<usize, Self::Error> {
            if self.fails {
                return Err(());
            }
            let got = self.available.min(buffer.len());
            for (slot, source) in buffer.iter_mut().zip(self.response.iter()) {
                *slot = *source;
            }
            Ok(got)
        }
    }

    // ================================================= the conversion formulae

    #[test]
    fn the_address_and_command_are_the_cpp_ones() {
        // pressureSensor.h:14, :16
        assert_eq!(ADDRESS, 0x28);
        assert_eq!(COMMAND, [0xAA, 0x00, 0x00]);
    }

    #[test]
    fn the_offsets_are_the_cpp_ones() {
        // pressureSensor.h:18-20. Every one of these is an integer below
        // 2^24, so each is exact in an f64 and the comparison is bit-exact
        // rather than approximate.
        assert_eq!(OUTPUT_MIN.to_bits(), 1_677_722.0f64.to_bits());
        assert_eq!(OUTPUT_MAX.to_bits(), 15_099_494.0f64.to_bits());
        assert_eq!(P_MAX.to_bits(), 10.0f64.to_bits());
        assert_eq!(P_MIN.to_bits(), 0.0f64.to_bits());
        assert_eq!(FULL_SCALE.to_bits(), 16_777_215.0f64.to_bits());
    }

    #[test]
    fn zero_bar_is_the_output_minimum_not_zero_counts() {
        // The transfer function is offset: 0 bar is 1677722 counts, not 0.
        assert_eq!(counts_to_bar(1_677_722).raw().to_bits(), 0.0f32.to_bits());
        assert!(is_below_range(1_677_721));
        assert!(!is_below_range(1_677_722));
    }

    #[test]
    fn ten_bar_is_the_output_maximum() {
        let top = counts_to_bar(15_099_494);
        assert!((top.raw() - 10.0).abs() < 1e-4, "got {top}");
    }

    #[test]
    fn the_full_scale_pressure_is_unreachable() {
        // The whole point of the offset: 16777215 counts is NOT 10 bar.
        let full = counts_to_bar(16_777_215);
        assert!(
            full.raw() > 10.0,
            "full scale should read above 10 bar, got {full}"
        );
    }

    #[test]
    fn a_below_range_count_is_reported_unclamped() {
        // PRESERVED C++ BEHAVIOUR. A count under the offset yields a negative
        // pressure and the C++ carries it on. `is_below_range` exposes the
        // condition without changing the value, so the divergence can be
        // decided rather than smuggled in.
        let below = counts_to_bar(0);
        assert!(below.raw() < 0.0, "got {below}");
        assert!(is_below_range(0));
    }

    #[test]
    fn mid_scale_pressure_is_about_half() {
        // Halfway between the offset and the top.
        let mid = (OUTPUT_MIN + OUTPUT_MAX) / 2.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let counts = mid as u32;
        let bar = counts_to_bar(counts);
        assert!((bar.raw() - 5.0).abs() < 0.01, "got {bar}");
    }

    #[test]
    fn the_temperature_scale_is_the_cpp_one() {
        // pressureSensor.h:51: counts * 270.0 / 16777215.0 - 40.0
        assert_eq!(counts_to_celsius(0).raw().to_bits(), (-40.0f32).to_bits());
        let top = counts_to_celsius(16_777_215);
        assert!((top.raw() - 230.0).abs() < 0.001, "got {top}");
        // Room temperature is about 3.4 M counts.
        let room = counts_to_celsius(3_400_000);
        assert!((room.raw() - 14.7).abs() < 0.5, "got {room}");
    }

    #[test]
    fn the_percentage_uses_full_scale_not_the_output_span() {
        // PRESERVED C++ ARITHMETIC. pressureSensor.h:50 divides by 16777215,
        // while `counts_to_bar` divides by (OUTPUT_MAX - OUTPUT_MIN). The two
        // therefore disagree, by exactly the offset ratio.
        assert_eq!(
            counts_to_percentage(16_777_215).to_bits(),
            100.0f64.to_bits()
        );
        // 10 bar is at 15099494 counts, which is 90.0 % of full scale.
        let ten_bar_pct = counts_to_percentage(15_099_494);
        assert!(
            (ten_bar_pct - 90.0).abs() < 0.01,
            "10 bar should be ~90 % of full scale, got {ten_bar_pct}"
        );
        // Whereas the pressure formula says 10 bar is 100 % of the span.
        assert!((counts_to_bar(15_099_494).raw() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn decode_reads_the_counts_big_endian_and_skips_byte_zero() {
        // pressureSensor.h:45-48: data[3] + data[2]*256 + data[1]*65536.
        let data = [
            0xAA, // status, discarded
            0x00, 0x0F, 0xFF, // pressure = 0x000FFF = 4095
            0x00, 0x00, 0x01, // temperature = 1
        ];
        let sample = decode(&data);
        assert_eq!(sample.pressure_counts, 4095);
        assert_eq!(sample.temperature_counts, 1);
    }

    #[test]
    fn decode_ignores_the_status_byte() {
        // Byte 0 carries the sensor's status and diagnostic bits. The C++
        // never reads it; neither does this.
        let a = [0x00, 0x00, 0x0F, 0xFF, 0x00, 0x00, 0x01];
        let mut b = a;
        b[0] = 0xFF;
        assert_eq!(decode(&a), decode(&b));
    }

    // ======================================================= the filter

    #[test]
    fn the_filter_is_the_cpp_one() {
        // SensorCoordinator.cpp:152-159: y = 0.3x + 0.7y_prev
        // 0.3 * 6.0 + 0.7 * 4.0 = 1.8 + 2.8 = 4.6
        let out = filter(Bar::new(4.0), Bar::new(6.0));
        assert!((out.raw() - 4.6).abs() < 1e-4, "got {out}");
    }

    #[test]
    fn the_filter_converges_to_its_input() {
        // A DC input must eventually reach it, or a constant pressure would
        // read low forever.
        let mut value = Bar::new(0.0);
        for _ in 0..200 {
            value = filter(value, Bar::new(8.0));
        }
        assert!((value.raw() - 8.0).abs() < 0.01, "got {value}");
    }

    // ======================================================= the pipeline

    #[test]
    fn the_first_poll_writes_the_command_and_returns_immediately() {
        // **This is the win.** The C++ does write + delay(10) + read in one
        // call, so its first reading is 10 ms later. This returns in the time
        // it takes to write three bytes.
        let mut bus = FakeBus::new();
        let mut driver = Driver::new();
        assert_eq!(driver.poll(&mut bus, Millis::ZERO), Ok(Poll::Waiting));
        assert_eq!(bus.writes, [(0x28, [0xAA, 0x00, 0x00])]);
        assert!(driver.is_converting());
    }

    #[test]
    fn the_read_is_not_ready_before_the_deadline() {
        // The 10 ms is a *deadline*, not a sleep: the C++ is inside
        // `delay(10)` for all of it, this returns immediately for all of it.
        let mut bus = FakeBus::new();
        let mut driver = Driver::new();
        let _ = driver.poll(&mut bus, Millis::ZERO);
        for elapsed in [0u32, 1, 5, 9] {
            assert_eq!(
                driver.poll(&mut bus, Millis::new(elapsed)),
                Ok(Poll::Waiting),
                "at {elapsed} ms"
            );
        }
        // And it is ready exactly on the deadline, not a millisecond late.
        assert!(matches!(
            driver.poll(&mut bus, Millis::new(READ_DELAY.raw())),
            Ok(Poll::Sample(_))
        ));
    }

    #[test]
    fn a_completed_read_yields_a_sample() {
        let pressure = 8_388_608u32; // 0x800000
        let temperature = 3_400_000u32;
        let mut bus = FakeBus::with_counts(pressure, temperature);
        let mut driver = Driver::new();
        let _ = driver.poll(&mut bus, Millis::ZERO);
        let got = driver.poll(&mut bus, Millis::new(READ_DELAY.raw()));
        let Poll::Sample(sample) = got.unwrap_or(Poll::Waiting) else {
            panic!("expected a sample, got {got:?}");
        };
        assert_eq!(sample.pressure_counts, pressure);
        assert_eq!(sample.temperature_counts, temperature);
        assert_eq!(driver.last_sample(), Some(sample));
    }

    #[test]
    fn the_cadence_is_fifty_milliseconds() {
        // SensorCoordinator.h:271
        assert_eq!(CADENCE.raw(), 50);
    }

    #[test]
    fn a_read_does_not_repeat_within_the_cadence() {
        // After a read, the next command is not due for another 50 ms. This is
        // what bounds the bus traffic to 20 Hz.
        let mut bus = FakeBus::with_counts(8_388_608, 3_400_000);
        let mut driver = Driver::new();
        let _ = driver.poll(&mut bus, Millis::ZERO);
        let _ = driver.poll(&mut bus, Millis::new(10));
        let writes_after_first = bus.writes.len();
        for elapsed in [11u32, 20, 40, 49] {
            let _ = driver.poll(&mut bus, Millis::new(elapsed));
        }
        assert_eq!(
            bus.writes.len(),
            writes_after_first,
            "no new command inside the cadence"
        );
        // And 50 ms after the *command* — i.e. 40 ms after the read — the next
        // command is issued. The 10 ms of conversion is inside the 50 ms, which
        // is what keeps the update rate identical to the C++'s.
        assert_eq!(
            driver.poll(&mut bus, Millis::new(CADENCE.raw() - 1)),
            Ok(Poll::Idle),
            "49 ms after the command is still inside the cadence"
        );
        let _ = driver.poll(&mut bus, Millis::new(CADENCE.raw()));
        assert_eq!(bus.writes.len(), writes_after_first + 1);
    }

    #[test]
    fn the_pipeline_sustains_twenty_hertz_for_a_minute() {
        // 60 s of wall clock at the C++'s 50 ms cadence, polled at 10 ms --
        // which is the loop rate the C++'s 20 % blocking would have been
        // measured against. Under the C++ the loop would have slept 12 s of
        // that minute; here it sleeps none of it.
        const POLL_MS: u32 = 10;
        const TICKS: u32 = 6_000; // 60 s

        let mut bus = FakeBus::with_counts(8_388_608, 3_400_000);
        let mut driver = Driver::new();
        let mut samples = 0;
        let blocked_ms = 0;
        for tick in 0..TICKS {
            let now = Millis::new(tick * POLL_MS);
            match driver.poll(&mut bus, now) {
                Ok(Poll::Sample(_)) => samples += 1,
                Ok(Poll::Idle) => {
                    // The C++ would have slept for `READ_DELAY` here, inside
                    // the very next call. The whole of the migration's win is
                    // that this arm costs nothing.
                    let _ = blocked_ms;
                }
                Ok(Poll::Waiting) => {}
                Err(err) => panic!("tick {tick} failed: {err}"),
            }
        }
        assert_eq!(samples, 1_200, "one sample per 50 ms for 60 s");
        assert_eq!(blocked_ms, 0, "the driver never asks the loop to sleep");
    }

    #[test]
    fn the_cpp_would_have_slept_twenty_percent_of_the_loop() {
        // The arithmetic the win is measured against, stated so the number in
        // the module docs is a test rather than a claim:
        //   PRESSURE_UPDATE_INTERVAL_MS = 50, ABP2_READ_DELAY_MS = 10
        //   -> 10 / 50 = 20 % of every loop iteration, unconditionally.
        let fraction = f64::from(READ_DELAY.raw()) / f64::from(CADENCE.raw());
        assert_eq!(fraction.to_bits(), 0.2f64.to_bits(), "got {fraction}");
    }

    // =============================================== the checks the C++ skips

    #[test]
    fn div1_a_short_read_is_an_error_not_a_stale_sample() {
        // **The deliberate divergence.** The C++ ignores `requestFrom`'s
        // return value, so a device that answers with 3 bytes leaves 4 stale
        // bytes in the globals and the firmware converts those as if they were
        // fresh. Here it is an error and the previous sample is kept.
        let mut bus = FakeBus::with_counts(8_388_608, 3_400_000);
        bus.available = 3;
        let mut driver = Driver::new();
        let _ = driver.poll(&mut bus, Millis::ZERO);
        assert_eq!(
            driver.poll(&mut bus, Millis::new(10)),
            Err(ReadError::ShortRead {
                got: 3,
                want: RESPONSE_LEN
            })
        );
        assert_eq!(driver.last_sample(), None, "no sample from a short read");
    }

    #[test]
    fn div2_a_nack_on_the_command_is_an_error() {
        // The C++ builds `stat` from `Wire.write()` and
        // `Wire.endTransmission()` and never reads it, so an unacknowledged
        // command is invisible to it.
        let mut bus = FakeBus::new();
        bus.fails = true;
        let mut driver = Driver::new();
        assert_eq!(
            driver.poll(&mut bus, Millis::ZERO),
            Err(ReadError::CommandFailed)
        );
    }

    #[test]
    fn a_failed_read_leaves_the_pipeline_ready_for_the_next_cadence() {
        // An error must not wedge the driver: the next poll starts a new
        // conversion, so a transient bus error costs one sample, not the
        // sensor.
        let mut bus = FakeBus::with_counts(8_388_608, 3_400_000);
        let mut driver = Driver::new();
        let _ = driver.poll(&mut bus, Millis::ZERO);
        bus.available = 0;
        assert!(driver.poll(&mut bus, Millis::new(10)).is_err());
        assert!(!driver.is_converting());
        bus.available = RESPONSE_LEN;
        // The failed read was of the command issued at t=0, so the next command
        // is due at t=50 — the same deadline a *successful* read would set,
        // because the deadline is anchored on the command.
        let _ = driver.poll(&mut bus, Millis::new(CADENCE.raw() - 1));
        assert!(!driver.is_converting(), "still inside the cadence");
        let _ = driver.poll(&mut bus, Millis::new(CADENCE.raw()));
        assert!(driver.is_converting(), "a new conversion is started");
    }

    #[test]
    fn a_zero_read_is_reported_as_short() {
        let mut bus = FakeBus::new();
        bus.available = 0;
        let mut driver = Driver::new();
        let _ = driver.poll(&mut bus, Millis::ZERO);
        assert_eq!(
            driver.poll(&mut bus, Millis::new(10)),
            Err(ReadError::ShortRead {
                got: 0,
                want: RESPONSE_LEN
            })
        );
    }

    #[test]
    fn the_error_messages_name_the_cause() {
        assert_eq!(
            ReadError::CommandFailed.to_string(),
            "the ABP2 command was not acknowledged"
        );
        assert_eq!(
            ReadError::ShortRead { got: 3, want: 7 }.to_string(),
            "short read: 3 of 7 bytes"
        );
    }
}
