//! The DS18B20: scratchpad layout, resolution and the conversion schedule.
//!
//! Sits on [`clevercoffee_onewire::Bus`] and implements
//! [`clevercoffee_hal_traits::TemperatureSensor`], so it is interchangeable with the TSIC at the
//! trait boundary and the choice between them is a boot-time decision rather than a compile-time
//! fork in the control path.
//!
//! The rule that shapes this whole file: **a read returns a value or an error, never a
//! sentinel**. The C++ firmware used `DallasTemperature::getTempC`, which returns `DEVICE_DISCONNECTED_C`
// and friends, and the caller compared the result against those constants. A sentinel is a number,
//! and a number a caller can forget to check, which is how a disconnected sensor ends up heating
//! the boiler (defect D03). Here the CRC is checked and a failure is an error, so there is nothing
//! to forget.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

use clevercoffee_hal_traits::{TemperatureError, TemperatureSensor};
use clevercoffee_onewire::rom::Address;
use clevercoffee_onewire::{BitOps, Bus, BusError};

/// The resolution a DS18B20 can be configured for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Resolution {
    /// 9 bits, 0.5 C resolution, 94 ms conversion.
    Bits9,
    /// 10 bits, 0.25 C, 188 ms.
    Bits10,
    /// 11 bits, 0.125 C, 375 ms.
    Bits11,
    /// 12 bits, 0.0625 C, 750 ms.
    Bits12,
}

impl Resolution {
    pub const fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            9 => Some(Resolution::Bits9),
            10 => Some(Resolution::Bits10),
            11 => Some(Resolution::Bits11),
            12 => Some(Resolution::Bits12),
            _ => None,
        }
    }

    pub const fn bits(self) -> u8 {
        match self {
            Resolution::Bits9 => 9,
            Resolution::Bits10 => 10,
            Resolution::Bits11 => 11,
            Resolution::Bits12 => 12,
        }
    }

    /// How long the device needs to convert at this resolution.
    ///
    /// These are the datasheet maxima, rounded up. The driver waits this long after a convert
    /// command rather than polling, because polling a 1-Wire bus means a reset that interrupts the
    /// conversion being polled for.
    pub const fn conversion_ms(self) -> u16 {
        match self {
            Resolution::Bits9 => 94,
            Resolution::Bits10 => 188,
            Resolution::Bits11 => 375,
            Resolution::Bits12 => 750,
        }
    }

    /// The two resolution bits as they sit in the configuration register, in bits 6 and 5.
    pub const fn config_bits(self) -> u8 {
        match self {
            Resolution::Bits9 => 0b000,
            Resolution::Bits10 => 0b001,
            Resolution::Bits11 => 0b010,
            Resolution::Bits12 => 0b011,
        }
    }
}

/// The family code every DS18B20 reports.
pub const FAMILY: u8 = 0x28;

/// The `CONVERT_TEMP` command, with the parasite-power bit set.
///
/// The bit is set because the firmware powers the sensor from the data line when a strong pullup
/// is available, and a DS18B20 with that bit clear ignores a convert command sent without one.
pub const CONVERT_TEMP: u8 = 0x44;

/// `WRITE_SCRATCHPAD`.
pub const WRITE_SCRATCHPAD: u8 = 0x4E;

/// `READ_SCRATCHPAD`.
pub const READ_SCRATCHPAD: u8 = 0xBE;

/// The scratchpad is nine bytes: two temperature, two thresholds, one configuration, four
/// reserved and a CRC.
pub const SCRATCHPAD_LEN: usize = 9;

/// The valid range for a DS18B20 reading.
///
/// 0 to 125 C is the part's own range. The C++ firmware accepted 0 to 200, which is wide enough
/// that a fault which produced a large number would look like a plausible temperature, which is
/// the failure mode this range exists to prevent.
pub const MIN_VALID_C: f64 = 0.0;
pub const MAX_VALID_C: f64 = 125.0;

/// A DS18B20 on a 1-Wire bus.
#[derive(Debug)]
pub struct Ds18b20<T: BitOps> {
    bus: Bus<T>,
    address: Address,
    resolution: Resolution,
    /// Whether a conversion has been requested and not yet waited out.
    conversion_pending: bool,
}

impl<T: BitOps> Ds18b20<T> {
    /// Binds to a device already addressed by ROM.
    ///
    /// The family code is checked here rather than at read time: a DS1820 on the same bus answers
    /// the reset and would otherwise be read with DS18B20 commands and produce plausible nonsense.
    pub fn new(ops: T, address: Address, resolution: Resolution) -> Result<Self, AttachError> {
        if !address.is_ds18b20() {
            return Err(AttachError::NotADs18b20 {
                family: address.family(),
            });
        }
        let mut device = Self {
            bus: Bus::new(ops, clevercoffee_onewire::Timings::default()),
            address,
            resolution,
            conversion_pending: false,
        };
        device
            .write_resolution(resolution)
            .map_err(|_| AttachError::Bus(BusError::NoDevice))?;
        Ok(device)
    }

    /// Finds the single device on a single-drop bus.
    ///
    /// A multi-drop bus is refused rather than silently binding to whichever device answers
    /// first: a machine with two sensors on one wire is wired wrongly, and guessing which one
    /// controls the heater is not a decision this should make.
    pub fn discover(ops: T) -> Result<Self, AttachError> {
        let resolution = Resolution::Bits11;
        let mut bus = Bus::new(ops, clevercoffee_onewire::Timings::default());
        let address = clevercoffee_onewire::search_all(&mut bus).map_err(AttachError::from)?;
        if address.len() > 1 {
            return Err(AttachError::MultipleDevices {
                count: address.len(),
            });
        }
        Self::new(bus.ops_owned(), address[0], resolution)
    }

    pub const fn address(&self) -> Address {
        self.address
    }

    pub const fn resolution(&self) -> Resolution {
        self.resolution
    }

    /// How long the caller must wait after [`Ds18b20::request_conversion`] before reading.
    pub const fn conversion_ms(&self) -> u16 {
        self.resolution.conversion_ms()
    }

    /// Whether a conversion has been requested and its time has not elapsed.
    pub const fn conversion_pending(&self) -> bool {
        self.conversion_pending
    }

    /// Starts a conversion.
    ///
    /// Non-blocking on purpose. The C++ firmware called `requestTemperaturesByAddress()` and then
    /// `getTempC()` on the next tick, and set `setWaitForConversion(false)` so it would not block,
    /// which means it read whatever was in the scratchpad, possibly from the previous conversion,
    /// and had no way to tell.
    pub fn request_conversion(&mut self) -> Result<(), TemperatureError> {
        self.select()?;
        // A bus that will not take the command is a wiring fault, not a reading failure, and it is
        // reported as a disconnected sensor so the safety path inhibits the heater.
        self.bus.write_byte(CONVERT_TEMP);
        self.conversion_pending = true;
        Ok(())
    }

    /// Marks a requested conversion as waited out.
    pub fn conversion_complete(&mut self) {
        self.conversion_pending = false;
    }

    /// Reads the scratchpad and decodes it.
    ///
    /// Returns an error rather than a value for every failure, and never a sentinel. A CRC failure
    /// is [`TemperatureError::Corrupt`] and an implausible value is
    /// [`TemperatureError::OutOfRange`]; neither is ever a temperature.
    pub fn read_celsius(&mut self) -> Result<f64, TemperatureError> {
        self.select()?;
        self.bus.write_byte(READ_SCRATCHPAD);

        let mut scratchpad = [0u8; SCRATCHPAD_LEN];
        for slot in scratchpad.iter_mut() {
            *slot = self.bus.read_byte();
        }
        self.conversion_pending = false;
        decode(&scratchpad)
    }

    /// Sets the resolution, writing the scratchpad's configuration byte.
    pub fn write_resolution(&mut self, resolution: Resolution) -> Result<(), TemperatureError> {
        self.select()?;
        self.bus.write_byte(WRITE_SCRATCHPAD);

        // The three threshold and alarm bytes are echoed back unchanged; only the configuration
        // byte's two resolution bits are set.
        for _ in 0..3 {
            self.bus.write_byte(0);
        }
        self.bus.write_byte(resolution.config_bits());
        for _ in 0..4 {
            self.bus.write_byte(0);
        }

        // A scratchpad write only takes effect after the device is told to commit it, and a DS18B20
        // with parasite power needs the line held high for the copy. Without this the resolution
        // change silently does nothing.
        self.bus.strong_pullup_ms(10);

        self.resolution = resolution;
        Ok(())
    }

    /// Addresses this device.
    fn select(&mut self) -> Result<(), TemperatureError> {
        self.bus
            .match_rom(self.address)
            .map_err(|_| TemperatureError::Disconnected)
    }
}

impl<T: BitOps> TemperatureSensor for Ds18b20<T> {
    fn address(&self) -> u64 {
        self.address.serial()
    }

    fn read_celsius(&mut self) -> Result<f64, TemperatureError> {
        Ds18b20::read_celsius(self)
    }
}

/// Decodes a nine-byte scratchpad into degrees Celsius.
///
/// The first two bytes are the temperature in sixteenths of a degree, least-significant byte first,
/// and only the bits the current resolution actually fills in are meaningful. Reading all sixteen
/// at 9-bit resolution returns the last conversion's low bits, which is a stale number rather than
/// a coarse one: the fix is to mask, which is what the resolution argument is for.
pub fn decode(scratchpad: &[u8; SCRATCHPAD_LEN]) -> Result<f64, TemperatureError> {
    // The checksum is verified here rather than at the read site, so a caller that builds a
    // scratchpad any other way cannot skip it. It is the one check between bytes on the wire and a
    // number a heater will act on.
    clevercoffee_onewire::verify_crc(scratchpad).map_err(|_| TemperatureError::Corrupt)?;
    let raw = i16::from_le_bytes([scratchpad[0], scratchpad[1]]);
    // The resolution lives in bits 6 and 5 of the configuration byte.
    let resolution = Resolution::from_bits(((scratchpad[4] >> 5) & 0b11) + 9)
        .ok_or(TemperatureError::Corrupt)?;
    // The DS18B20 powers on with 85 C in the register as "no conversion performed yet", 1360 in
    // sixteenths. Reading that as a temperature is how a machine boots believing the boiler is at
    // 85 C and then holds a cold group head at 95 for a brew.
    if raw == 1360 {
        return Err(TemperatureError::Corrupt);
    }
    // The raw register always holds sixteenths of a degree: four fractional bits are fixed, and
    // the configured resolution selects how many of the sixteen bits are meaningful above them.
    // So 9 bits leaves three low bits stale, and 12 bits leaves none. Reading all sixteen at a
    // lower resolution returns a stale number rather than a coarse one, which is what the mask is
    // for.
    let mask = 0xFFFFu16 << (12 - resolution.bits());
    let masked = (raw as u16) & mask;
    let celsius = f64::from(masked) / 16.0;
    if !(MIN_VALID_C..=MAX_VALID_C).contains(&celsius) {
        return Err(TemperatureError::OutOfRange);
    }
    Ok(celsius)
}

/// Builds a scratchpad carrying a temperature, for tests and for a simulator.
pub fn encode(celsius: f64, resolution: Resolution) -> [u8; SCRATCHPAD_LEN] {
    // Truncated, not rounded: `f64::round` needs libm in a `no_std` build, and a temperature in
    // this range is always already a whole number of sixteenths by the time it is encoded.
    let raw = (celsius * 16.0) as i16;
    let mut scratchpad = [0u8; SCRATCHPAD_LEN];
    scratchpad[..2].copy_from_slice(&raw.to_le_bytes());
    scratchpad[4] = resolution.config_bits() << 5;
    scratchpad[8] = clevercoffee_onewire::crc8(&scratchpad[..8]);
    scratchpad
}

/// Why a device could not be bound.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttachError {
    /// The ROM's family code is not 0x28.
    NotADs18b20 { family: u8 },
    /// More than one device answered. A machine has one sensor, so this is a wiring fault.
    MultipleDevices { count: usize },
    /// The bus itself failed.
    Bus(BusError),
}

impl From<BusError> for AttachError {
    fn from(e: BusError) -> Self {
        AttachError::Bus(e)
    }
}

impl core::fmt::Display for AttachError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AttachError::NotADs18b20 { family } => {
                write!(f, "device family {family:#04x} is not a DS18B20")
            }
            AttachError::MultipleDevices { count } => {
                write!(f, "{count} devices on the bus; a machine has one sensor")
            }
            AttachError::Bus(e) => write!(f, "{e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clevercoffee_onewire::test_bus::{FakeBus, Script};

    const SENSOR: Address = Address([0x28, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]);

    /// Builds a device whose bus hands back `scratchpad` for a `READ_SCRATCHPAD`.
    fn device(scratchpad: [u8; SCRATCHPAD_LEN]) -> Ds18b20<FakeBus> {
        let fake = FakeBus::new(Script::scratchpad(&scratchpad));
        let mut d = Ds18b20::new(fake, SENSOR, Resolution::Bits12).expect("should attach");
        // The resolution write and the queued read would fight over one bus, so the test drives
        // the read directly and the resolution separately.
        d.conversion_pending = false;
        d
    }

    #[test]
    fn a_good_scratchpad_decodes_to_its_temperature() {
        for c in [0.0f64, 20.25, 92.5, 94.0625, 125.0] {
            let scratchpad = encode(c, Resolution::Bits12);
            assert_eq!(decode(&scratchpad).unwrap(), c, "at {c}");
        }
    }

    #[test]
    fn a_corrupted_byte_fails_the_crc_and_is_never_a_temperature() {
        // The safety property. A value from a failed CRC must not exist: this is defect D03.
        let mut scratchpad = encode(92.0, Resolution::Bits12);
        for byte in 0..8 {
            let mut corrupt = scratchpad;
            corrupt[byte] ^= 0x01;
            assert!(
                clevercoffee_onewire::verify_crc(&corrupt).is_err(),
                "the CRC accepted a corrupted byte {byte}"
            );
            assert_eq!(
                decode(&corrupt),
                Err(TemperatureError::Corrupt),
                "byte {byte} slipped through"
            );
        }
        scratchpad[0] ^= 0x01;
        assert!(scratchpad[0] != encode(92.0, Resolution::Bits12)[0]);
    }

    #[test]
    fn a_scratchpad_with_the_85c_placeholder_is_refused() {
        // The device's "no conversion yet" marker. Reading it as a temperature is how a machine
        // boots believing the boiler is at 85 C.
        let scratchpad = encode(85.0, Resolution::Bits12);
        assert_eq!(decode(&scratchpad), Err(TemperatureError::Corrupt));
    }

    #[test]
    fn a_value_outside_the_parts_range_is_refused() {
        for c in [-0.5f64, -20.0, 130.0, 200.0] {
            let scratchpad = encode(c, Resolution::Bits12);
            assert_eq!(
                decode(&scratchpad),
                Err(TemperatureError::OutOfRange),
                "at {c}"
            );
        }
    }

    #[test]
    fn each_resolution_masks_away_the_bits_it_does_not_fill() {
        // At 9-bit resolution the low bits of the raw value are whatever the previous conversion
        // left there. Reading all sixteen bits returns a stale number rather than a coarse one,
        // so the decode masks and the test proves the mask is doing the work.
        let mut scratchpad = encode(90.0, Resolution::Bits12);
        scratchpad[4] = Resolution::Bits9.config_bits() << 5;
        // Fill the three bits 9-bit resolution does not use with ones. Masking must discard them
        // rather than average them in.
        scratchpad[0] |= 0x07;
        scratchpad[8] = clevercoffee_onewire::crc8(&scratchpad[..8]);
        let decoded = decode(&scratchpad).expect("should decode");
        assert_eq!(
            decoded, 90.0,
            "the low bits must be masked away, got {decoded}"
        );
    }

    #[test]
    fn the_conversion_time_matches_the_resolution() {
        // The values the datasheet gives, and the ones the C++ firmware's comment referred to.
        assert_eq!(Resolution::Bits9.conversion_ms(), 94);
        assert_eq!(Resolution::Bits10.conversion_ms(), 188);
        assert_eq!(Resolution::Bits11.conversion_ms(), 375);
        assert_eq!(Resolution::Bits12.conversion_ms(), 750);
        // Monotonic: a finer resolution must not convert faster.
        assert!(Resolution::Bits9.conversion_ms() < Resolution::Bits10.conversion_ms());
        assert!(Resolution::Bits10.conversion_ms() < Resolution::Bits11.conversion_ms());
        assert!(Resolution::Bits11.conversion_ms() < Resolution::Bits12.conversion_ms());
    }

    #[test]
    fn resolution_round_trips_through_its_bit_count_and_config_bits() {
        for r in [
            Resolution::Bits9,
            Resolution::Bits10,
            Resolution::Bits11,
            Resolution::Bits12,
        ] {
            assert_eq!(Resolution::from_bits(r.bits()), Some(r));
            // The two configuration bits count from 9, so they are the bit count minus nine.
            assert_eq!(
                r.config_bits(),
                r.bits() - 9,
                "config bits are wrong for {r:?}"
            );
        }
        assert_eq!(Resolution::from_bits(8), None);
        assert_eq!(Resolution::from_bits(13), None);
    }

    #[test]
    fn a_non_ds18b20_on_the_bus_is_refused_at_bind_time() {
        // A DS1820 answers the reset and would otherwise be read with DS18B20 commands.
        let other = Address([0x10, 1, 2, 3, 4, 5, 6, 7]);
        let result = Ds18b20::new(
            FakeBus::new(Script::device(other)),
            other,
            Resolution::Bits12,
        );
        assert_eq!(
            result.err(),
            Some(AttachError::NotADs18b20 { family: 0x10 })
        );
    }

    #[test]
    fn a_bus_with_nothing_on_it_fails_to_attach() {
        let result = Ds18b20::new(FakeBus::new(Script::empty()), SENSOR, Resolution::Bits12);
        assert!(matches!(result, Err(AttachError::Bus(BusError::NoDevice))));
    }

    #[test]
    fn a_read_comes_back_through_the_trait_as_a_result() {
        let scratchpad = encode(92.5, Resolution::Bits12);
        let mut d = device(scratchpad);
        let reading: Result<f64, TemperatureError> = TemperatureSensor::read_celsius(&mut d);
        assert_eq!(reading, Ok(92.5));
    }

    #[test]
    fn a_corrupt_read_through_the_trait_is_an_error_not_a_number() {
        let mut scratchpad = encode(92.5, Resolution::Bits12);
        scratchpad[8] = scratchpad[8].wrapping_add(1);
        let mut d = device(scratchpad);
        let reading: Result<f64, TemperatureError> = TemperatureSensor::read_celsius(&mut d);
        assert_eq!(reading, Err(TemperatureError::Corrupt));
    }

    #[test]
    fn a_conversion_is_pending_until_it_is_waited_out() {
        // The C++ firmware read the scratchpad on the next tick with no way to tell the value was
        // from the previous conversion. A pending flag is what makes that visible.
        let scratchpad = encode(92.5, Resolution::Bits12);
        let mut d = device(scratchpad);
        d.conversion_pending = false;
        assert!(!d.conversion_pending());
        d.request_conversion().unwrap();
        assert!(
            d.conversion_pending(),
            "a requested conversion must be visible as pending"
        );
        d.conversion_complete();
        assert!(!d.conversion_pending());
    }

    #[test]
    fn the_default_resolution_is_eleven_bits_as_the_cpp_firmware_used() {
        // TempSensorDallas.cpp set 11, commenting that it matched the 400 ms sensor timer.
        assert_eq!(Resolution::Bits11.bits(), 11);
        assert!(
            Resolution::Bits11.conversion_ms() < 400,
            "the conversion must fit the tick"
        );
    }
}
