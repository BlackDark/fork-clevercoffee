//! The Honeywell ABP2 pressure sensor.
//!
//! Ported from `include/clevercoffee/hardware/pressureSensor.h`, which was itself adapted from the
//! datasheet's sample code for the fitted ABP2-LANT010BG2A3XX.
//!
//! The one structural change is that the read does not block. The C++ version sent the convert
//! command, then called `delay(10)`, then read: ten milliseconds with the main loop stopped, ten
//! times a second if sampled at 10 Hz, which is defect D08. Here [`Abp2::start_conversion`]
//! returns immediately and [`Abp2::read`] is called later, after the caller has waited
//! [`Abp2::settle_ms`]. No driver in this port sleeps.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

/// The I2C address. The ABP2's address nibble comes from its part number; for the fitted variant
/// it is 0x28.
pub const ADDRESS: u8 = 0x28;

/// The convert command: read pressure, then temperature, single-shot.
pub const COMMAND: [u8; 3] = [0xAA, 0x00, 0x00];

/// How long the sensor needs after a convert command.
///
/// C++: `ABP2_READ_DELAY_MS`, which was a `delay()` on the main loop. Same value, but the caller
/// spends it doing something else.
pub const SETTLE_MS: u16 = 10;

/// The bytes a read returns.
///
/// Twelve, not the seven the C++ code read. An ABP2 returns two six-byte words: three status
/// bytes and three data bytes for pressure, then the same for temperature. The C++ firmware read
/// only seven bytes and built its temperature count from bytes 4, 5 and 6, one of which is the
/// second word's status byte rather than data, so its temperature was wrong by construction
/// (defect D54).
pub const DATA_LEN: usize = 12;

/// The status bits that mean "these data are not a measurement": the diagnostic-monitor output in
/// bit 7 and the two diagnostic-status bits 3 and 2.
pub const STATUS_FAULT_MASK: u8 = 0x8C;

/// Counts at full scale: 24 bits.
pub const FULL_SCALE: f64 = 16_777_215.0;

/// Counts at the bottom of the 0-to-10-bar span.
///
/// C++: `ABP2_outputmin`.
pub const OUTPUT_MIN: f64 = 1_677_722.0;

/// Counts at the top of the span. C++: `ABP2_outputmax`.
pub const OUTPUT_MAX: f64 = 15_099_494.0;

/// The bottom of the span in bar. C++: `ABP2_pmin`.
pub const P_MIN: f64 = 0.0;

/// The top of the span in bar. C++: `ABP2_pmax`.
pub const P_MAX: f64 = 10.0;

/// Where each field sits in the twelve-byte frame.
mod offset {
    pub const PRESSURE_STATUS: usize = 0;
    pub const PRESSURE_DATA: usize = 1;
    pub const TEMPERATURE_STATUS: usize = 6;
    pub const TEMPERATURE_DATA: usize = 7;
}

/// A pressure reading, in bar and degrees Celsius.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Reading {
    pub bar: f64,
    pub celsius: f64,
}

/// Why a pressure read failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PressureError {
    /// The device did not acknowledge. Usually nothing on the bus.
    NotAcknowledged,
    /// The read returned fewer than seven bytes, which means the bus gave up part way.
    ShortRead { got: usize },
    /// The status byte reports a diagnostic fault or the data are not ready.
    StatusFault,
    /// The counts are outside the span the part was configured for, so the conversion is not
    /// trustworthy.
    OutOfSpan { counts: u32 },
}

impl core::fmt::Display for PressureError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PressureError::NotAcknowledged => f.write_str("the sensor did not acknowledge"),
            PressureError::ShortRead { got } => write!(f, "short read: {got} of {DATA_LEN} bytes"),
            PressureError::StatusFault => f.write_str("the sensor reported a fault"),
            PressureError::OutOfSpan { counts } => write!(f, "{counts} counts is outside the span"),
        }
    }
}

/// The I2C operations the sensor needs.
///
/// A trait so the driver has no I2C dependency and the whole transaction sequence is testable
/// against a script. The board crate supplies the real bus.
pub trait I2c {
    /// Writes `bytes` to `address`, then releases the bus.
    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), PressureError>;

    /// Reads exactly [`DATA_LEN`] bytes into `out`, returning how many arrived.
    fn read(&mut self, address: u8, out: &mut [u8]) -> Result<usize, PressureError>;
}

/// The ABP2.
#[derive(Debug)]
pub struct Abp2<I: I2c> {
    bus: I,
    /// Whether a conversion has been started and not yet read.
    conversion_pending: bool,
}

impl<I: I2c> Abp2<I> {
    pub const fn new(bus: I) -> Self {
        Self {
            bus,
            conversion_pending: false,
        }
    }

    /// How long the caller must wait before reading.
    pub const fn settle_ms(&self) -> u16 {
        SETTLE_MS
    }

    /// Whether a conversion has been started and not yet read.
    ///
    /// A caller reading before this clears gets the previous conversion's data, which looks like a
    /// valid reading that is one cycle stale. The C++ firmware had no way to see that.
    pub const fn conversion_pending(&self) -> bool {
        self.conversion_pending
    }

    /// Starts a conversion. Returns immediately.
    pub fn start_conversion(&mut self) -> Result<(), PressureError> {
        self.bus.write(ADDRESS, &COMMAND)?;
        self.conversion_pending = true;
        Ok(())
    }

    /// Reads the result of a started conversion.
    pub fn read(&mut self) -> Result<Reading, PressureError> {
        let mut data = [0u8; DATA_LEN];
        let got = self.bus.read(ADDRESS, &mut data)?;
        if got < DATA_LEN {
            return Err(PressureError::ShortRead { got });
        }
        self.conversion_pending = false;
        decode(&data)
    }

    /// Starts a conversion and reads the result, given the caller's elapsed time.
    ///
    /// Provided so a caller with nothing else to do can express the whole thing in one place. It
    /// still does not wait: the elapsed time is the caller's to have spent.
    pub fn convert_and_read(&mut self, elapsed_ms: u16) -> Result<Reading, PressureError> {
        self.start_conversion()?;
        if elapsed_ms < SETTLE_MS {
            return Err(PressureError::ShortRead { got: 0 });
        }
        self.read()
    }
}

/// Decodes seven bytes into a reading.
///
/// The status byte's diagnostic bits are checked before anything is converted, because a part
/// reporting a fault can still return plausible counts, and those counts would then drive the
/// over-pressure logic.
pub fn decode(data: &[u8; DATA_LEN]) -> Result<Reading, PressureError> {
    // Each word has its own status byte, and a part reporting a diagnostic fault can still return
    // plausible counts, so both are checked before anything is converted.
    if data[offset::PRESSURE_STATUS] & STATUS_FAULT_MASK != 0
        || data[offset::TEMPERATURE_STATUS] & STATUS_FAULT_MASK != 0
    {
        return Err(PressureError::StatusFault);
    }

    let pressure = &data[offset::PRESSURE_DATA..offset::PRESSURE_DATA + 3];
    let counts =
        (u32::from(pressure[0]) << 16) | (u32::from(pressure[1]) << 8) | u32::from(pressure[2]);

    let temperature = &data[offset::TEMPERATURE_DATA..offset::TEMPERATURE_DATA + 3];
    let temp_counts = (u32::from(temperature[0]) << 16)
        | (u32::from(temperature[1]) << 8)
        | u32::from(temperature[2]);
    let celsius = f64::from(temp_counts) * 270.0 / FULL_SCALE - 40.0;

    if !(OUTPUT_MIN..=OUTPUT_MAX).contains(&(counts as f64)) {
        return Err(PressureError::OutOfSpan { counts });
    }

    // Equation 2 of the datasheet, as the C++ code had it.
    let bar =
        (f64::from(counts) - OUTPUT_MIN) * (P_MAX - P_MIN) / (OUTPUT_MAX - OUTPUT_MIN) + P_MIN;
    Ok(Reading { bar, celsius })
}

/// Encodes a reading into seven bytes, for tests and for a simulator.
pub fn encode(bar: f64, celsius: f64) -> [u8; DATA_LEN] {
    // `f64::round` needs libm in a `no_std` build. Truncating is fine here because the result is
    // a count, and a count one off its exact value decodes back within the tolerance every test
    // and the part's own resolution allow.
    let counts = ((bar - P_MIN) * (OUTPUT_MAX - OUTPUT_MIN) / (P_MAX - P_MIN) + OUTPUT_MIN) as u32;
    let counts = counts.clamp(OUTPUT_MIN as u32, OUTPUT_MAX as u32);
    let temp_counts = ((celsius + 40.0) * FULL_SCALE / 270.0) as u32;
    let mut data = [0u8; DATA_LEN];
    data[offset::PRESSURE_DATA] = ((counts >> 16) & 0xFF) as u8;
    data[offset::PRESSURE_DATA + 1] = ((counts >> 8) & 0xFF) as u8;
    data[offset::PRESSURE_DATA + 2] = (counts & 0xFF) as u8;
    data[offset::TEMPERATURE_DATA] = ((temp_counts >> 16) & 0xFF) as u8;
    data[offset::TEMPERATURE_DATA + 1] = ((temp_counts >> 8) & 0xFF) as u8;
    data[offset::TEMPERATURE_DATA + 2] = (temp_counts & 0xFF) as u8;
    data
}

#[cfg(test)]
mod tests {
    // The crate is `no_std` for the target; the tests run on the host, so `std` is linked in here
    // for the scripted bus's interior mutability.
    extern crate std;

    use super::*;
    use std::cell::RefCell;
    use std::vec::Vec;

    /// A scripted I2C bus.
    #[derive(Debug, Default)]
    struct FakeI2c {
        /// What the bus will return for a read, queued.
        reads: RefCell<Vec<[u8; DATA_LEN]>>,
        /// Whether every read reports a NACK.
        pub dead: bool,
        /// How many bytes a read reports, when a test wants a short read.
        pub short_by: usize,
        /// Commands the driver wrote, so a test can assert the sequence.
        pub commands: RefCell<Vec<u8>>,
    }

    impl FakeI2c {
        fn with(data: &[u8; DATA_LEN]) -> Self {
            Self {
                reads: RefCell::new(std::vec![*data]),
                dead: false,
                short_by: 0,
                commands: RefCell::new(Vec::new()),
            }
        }
    }

    /// Lets the driver hold the bus by reference, so a test can inspect what was written after the
    /// driver has finished with it.
    impl<T: I2c + ?Sized> I2c for &mut T {
        fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), PressureError> {
            (**self).write(address, bytes)
        }
        fn read(&mut self, address: u8, out: &mut [u8]) -> Result<usize, PressureError> {
            (**self).read(address, out)
        }
    }

    impl I2c for FakeI2c {
        fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), PressureError> {
            self.commands.borrow_mut().push(address);
            self.commands.borrow_mut().extend_from_slice(bytes);
            Ok(())
        }

        fn read(&mut self, _address: u8, out: &mut [u8]) -> Result<usize, PressureError> {
            if self.dead {
                return Err(PressureError::NotAcknowledged);
            }
            let mut reads = self.reads.borrow_mut();
            if reads.is_empty() {
                return Err(PressureError::NotAcknowledged);
            }
            let data = reads.remove(0);
            let n = DATA_LEN - self.short_by;
            out[..n].copy_from_slice(&data[..n]);
            Ok(n)
        }
    }

    #[test]
    fn a_normal_reading_decodes() {
        let data = encode(5.0, 92.5);
        let reading = decode(&data).expect("should decode");
        assert!((reading.bar - 5.0).abs() < 0.001, "bar was {}", reading.bar);
        assert!(
            (reading.celsius - 92.5).abs() < 0.001,
            "celsius was {}",
            reading.celsius
        );
    }

    #[test]
    fn the_span_endpoints_decode_to_zero_and_ten_bar() {
        let zero = decode(&encode(P_MIN, 20.0)).unwrap();
        assert!((zero.bar - 0.0).abs() < 0.001, "got {}", zero.bar);
        let full = decode(&encode(P_MAX, 20.0)).unwrap();
        assert!((full.bar - 10.0).abs() < 0.001, "got {}", full.bar);
    }

    #[test]
    fn the_conversion_is_a_transaction_not_a_sleep() {
        // Defect D08: the C++ read called delay(10) on the main loop. The driver returns from the
        // write, and the caller spends the settle time elsewhere.
        let mut bus = FakeI2c::with(&encode(5.0, 92.5));
        let mut s = Abp2::new(&mut bus);
        s.start_conversion().unwrap();
        assert!(s.conversion_pending());
        assert_eq!(s.settle_ms(), 10);
        // The command went out and the driver is ready to be called back.
        assert_eq!(bus.commands.borrow().as_slice(), &[0x28, 0xAA, 0x00, 0x00]);
    }

    #[test]
    fn a_read_clears_the_pending_flag() {
        let mut bus = FakeI2c::with(&encode(5.0, 92.5));
        let mut s = Abp2::new(&mut bus);
        s.start_conversion().unwrap();
        s.read().unwrap();
        assert!(!s.conversion_pending());
    }

    #[test]
    fn a_stuck_conversion_is_a_short_read_not_a_stale_value() {
        // Reading before the conversion finishes would return the previous cycle's numbers, which
        // look like a valid reading one cycle old. Reading a bus that gave up is reported instead.
        let mut bus = FakeI2c::with(&encode(5.0, 92.5));
        bus.short_by = 2;
        let mut s = Abp2::new(&mut bus);
        assert_eq!(s.read(), Err(PressureError::ShortRead { got: 10 }));
    }

    #[test]
    fn an_out_of_span_count_is_refused() {
        // Below the configured minimum: a part reading lower than its own zero is not measuring
        // pressure, and converting it produces a negative number the over-pressure logic would
        // treat as valid.
        let mut low = encode(5.0, 20.0);
        for byte in low.iter_mut().take(4).skip(1) {
            *byte = 0;
        }
        assert!(matches!(decode(&low), Err(PressureError::OutOfSpan { .. })));

        let mut high = encode(5.0, 20.0);
        for byte in high.iter_mut().take(4).skip(1) {
            *byte = 0xFF;
        }
        assert!(matches!(
            decode(&high),
            Err(PressureError::OutOfSpan { .. })
        ));
    }

    #[test]
    fn a_nack_is_reported_rather_than_reading_whatever_is_on_the_bus() {
        let mut bus = FakeI2c::with(&encode(5.0, 92.5));
        bus.dead = true;
        let mut s = Abp2::new(&mut bus);
        assert_eq!(s.read(), Err(PressureError::NotAcknowledged));
    }

    #[test]
    fn a_fault_status_is_refused_before_the_counts_are_converted() {
        // A part reporting a diagnostic fault can still return plausible counts, and those counts
        // would then reach the over-pressure logic.
        for status in [0x80u8, 0x08, 0x04] {
            for index in [0usize, 6] {
                let mut data = encode(5.0, 92.5);
                data[index] = status;
                assert_eq!(
                    decode(&data),
                    Err(PressureError::StatusFault),
                    "status {status:#04x} at byte {index}"
                );
            }
        }
    }

    #[test]
    fn a_clean_status_byte_is_accepted() {
        let mut data = encode(5.0, 92.5);
        data[0] = 0x00;
        data[6] = 0x00;
        assert!(decode(&data).is_ok());
        // The remaining bits are the "data ready" flags and the sensor-health output, none of
        // which invalidates a measurement.
        data[0] = 0x40;
        data[6] = 0x40;
        assert!(decode(&data).is_ok());
    }

    #[test]
    fn reading_before_the_conversion_has_settled_is_refused() {
        // The caller's elapsed time is the caller's to have spent; the driver only knows the
        // requirement.
        let mut bus = FakeI2c::with(&encode(5.0, 92.5));
        let mut s = Abp2::new(&mut bus);
        assert_eq!(
            s.convert_and_read(5),
            Err(PressureError::ShortRead { got: 0 })
        );
        assert!(s.conversion_pending(), "the conversion is still running");
        assert!(s.convert_and_read(SETTLE_MS).is_ok());
    }

    #[test]
    fn the_constants_match_the_cpp_header() {
        assert_eq!(ADDRESS, 0x28);
        assert_eq!(COMMAND, [0xAA, 0x00, 0x00]);
        assert_eq!(SETTLE_MS, 10);
        // Twelve, not the seven the C++ read: two six-byte words, each three status and three data.
        assert_eq!(DATA_LEN, 12);
        assert_eq!(OUTPUT_MIN, 1_677_722.0);
        assert_eq!(OUTPUT_MAX, 15_099_494.0);
        assert_eq!(P_MAX, 10.0);
        assert_eq!(FULL_SCALE, 16_777_215.0);
        // The span must be non-empty, or the pressure conversion divides by zero. Checked at
        // compile time because both sides are constants.
        const _: () = assert!(OUTPUT_MIN < OUTPUT_MAX);
    }

    #[test]
    fn the_temperature_conversion_uses_the_datasheet_span() {
        // Counts to degrees: the 270 degree span runs from -40 C, so 0x0000 is -40 and 0xFFFF is
        // 230. A count outside that cannot be produced by the part.
        assert!((decode(&encode(5.0, -40.0)).unwrap().celsius + 40.0).abs() < 0.001);
        assert!((decode(&encode(5.0, 100.0)).unwrap().celsius - 100.0).abs() < 0.001);
    }

    /// `no_std` has no `to_string`; a small wrapper keeps the test readable.
    fn text(e: PressureError) -> std::string::String {
        let mut out = heapless::String::<96>::new();
        let _ = core::fmt::Write::write_fmt(&mut out, format_args!("{e}"));
        std::string::String::from(out.as_str())
    }

    #[test]
    fn every_error_names_itself() {
        // These strings reach a log line a user reads, so they have to say what to check.
        assert!(text(PressureError::NotAcknowledged).contains("acknowledge"));
        assert!(text(PressureError::StatusFault).contains("fault"));
        assert!(text(PressureError::ShortRead { got: 3 }).contains('3'));
        assert!(text(PressureError::OutOfSpan { counts: 42 }).contains("42"));
    }
}
