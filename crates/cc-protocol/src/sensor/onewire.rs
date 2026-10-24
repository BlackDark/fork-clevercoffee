//! 1-Wire CRC, ROM search, commands, scratchpads, and slot timings.
//!
//! [`OneWireBus`] supplies GPIO operations; the HAL alternative's limitations are
//! documented on that trait. Every byte is transmitted and assembled LSB first.

use cc_domain::units::Millis;

/// The DS18B20 family code, and the only family this driver accepts.
///
/// It is also the family of the probe **measured on the board**: the recovered firmware's boot log
/// reported `sensor: DS18B20 at 0x41af78cdaa376928 (family 0x28)`.
pub const FAMILY_DS18B20: u8 = 0x28;

/// DS18S20 / DS1820.
const FAMILY_DS18S20: u8 = 0x10;
/// DS1822.
const FAMILY_DS1822: u8 = 0x22;
/// MAX31850. Carries the open/short/undervoltage fault bits.
const FAMILY_MAX31850: u8 = 0x3B;
/// DS28EA00. 12-bit, same register layout as the DS18B20.
const FAMILY_DS28EA00: u8 = 0x42;

/// The number of bytes in a 1-Wire ROM code.
pub const ROM_LEN: usize = 8;

/// A 1-Wire ROM code: 7 serial bytes, then the family byte, then the CRC byte.
///
/// Byte 0 is the **family code** and byte 7 is the CRC of bytes 0..6, most significant byte first,
/// which is also the order the search algorithm returns them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rom(pub [u8; ROM_LEN]);

impl Rom {
    /// The family code, i.e. byte 0.
    #[must_use]
    pub const fn family(self) -> u8 {
        self.0[0]
    }

    /// Whether the ROM's CRC byte matches its first seven bytes.
    ///
    /// Whether byte 7 is the CRC of bytes 0..6. A ROM that fails this is dropped rather than
    /// searched again: a corrupted search result is a device that cannot answer.
    #[must_use]
    pub fn crc_valid(self) -> bool {
        crc8(&self.0[..ROM_LEN - 1]) == self.0[ROM_LEN - 1]
    }

    /// Whether this ROM is a DS18B20.
    #[must_use]
    pub const fn is_ds18b20(self) -> bool {
        self.family() == FAMILY_DS18B20
    }

    /// Whether this is a family [`ScratchPad::interpret`] can decode.
    ///
    /// Whether the family code is one of the five DS18B20-compatible families: `0x10` (DS18S20),
    /// `0x28` (DS18B20), `0x22` (DS1822), `0x3B` (MAX31850) and `0x42` (DS28EA00).
    #[must_use]
    pub const fn is_supported_family(self) -> bool {
        matches!(
            self.family(),
            FAMILY_DS18S20 | FAMILY_DS18B20 | FAMILY_DS1822 | FAMILY_MAX31850 | FAMILY_DS28EA00
        )
    }
}

// ===================================================================== CRC8

/// The 1-Wire / Dallas 8-bit CRC: polynomial `x^8 + x^5 + x^4 + 1`.
///
/// Computed bitwise rather than from a table: the domain is one byte at a time, so the bitwise form
/// needs no table at all. [`CRC8_STEP_TABLE`] holds the equivalent nibble table, and
/// `crc8_matches_the_table_driven_crc` proves the two agree rather than asserting it.
#[must_use]
pub fn crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &input in bytes {
        let mut byte = input;
        for _ in 0..8 {
            let mix = (crc ^ byte) & 0x01;
            crc >>= 1;
            if mix != 0 {
                crc ^= 0x8C;
            }
            byte >>= 1;
        }
    }
    crc
}

/// The nibble table for the same CRC, used only to test [`crc8`] against a second formulation.
///
/// Not `const`-folded into the table form: the domain this firmware CRCs is a ROM and a scratchpad,
/// nine bytes at most, so the bitwise loop is the implementation and this is the oracle.
pub const CRC8_STEP_TABLE: [u8; 32] = [
    0x00, 0x5E, 0xBC, 0xE2, 0x61, 0x3F, 0xDD, 0x83, 0xC2, 0x9C, 0x7E, 0x20, 0xA3, 0xFD, 0x1F, 0x41,
    0x00, 0x9D, 0x23, 0xBE, 0x46, 0xDB, 0x65, 0xF8, 0x8C, 0x11, 0xAF, 0x32, 0xCA, 0x57, 0xE9, 0x74,
];

/// One step of the table-driven CRC: `table[low nibble] ^ table[16 + high nibble]` after the byte
/// is xor-ed in.
///
/// Not `const`: `usize::from(u8)` is not a const conversion on this toolchain, and this exists only
/// so the tests can walk the table formulation.
#[must_use]
pub fn crc8_step_table(crc: u8, byte: u8) -> u8 {
    let xored = byte ^ crc;
    CRC8_STEP_TABLE[usize::from(xored & 0x0f)] ^ CRC8_STEP_TABLE[16 + usize::from(xored >> 4)]
}

// ==================================================================== timing

/// The 1-Wire bit-slot and reset timings, in microseconds.
///
/// The numbers are the datasheet's. They are **constants in the portable crate** rather than magic
/// numbers in the device code for two reasons:
///
/// * the protocol's correctness is a property of these numbers, and it must be
///   checkable on the host against the DS18B20 datasheet — see the
///   `the_slot_timings_sit_inside_the_ds18b20_windows` test;
/// * the `delay_ns` rounding question is a property of these numbers too, and
///   [`delay_ns_to_us_ceil`] exists to make it a test rather than a claim.
///
/// The datasheet windows (`AT24+DS18B20`, §"1-Wire Protocol"), for reference:
///
/// | quantity | datasheet | here | margin |
/// | --- | --- | --- | --- |
/// | `t_SLOT` | 60–120 µs | 65 (write 0) / 66 (write 1) / 66 (read) | ≥ 5 µs |
/// | write-1 `t_LOW` | 1–15 µs | 10 | 5 µs of headroom either way |
/// | write-0 `t_LOW` | 60–120 µs | 65 | 5 µs of headroom either way |
/// | read `t_LOW` | 1–15 µs | 3 | 12 µs of headroom |
/// | `t_SAMPLE` after the falling edge | 5–15 µs | 13 | 2 µs — **the tightest number here** |
/// | `t_RST` | ≥ 480 µs | 480 | 0 µs — at the limit, deliberately |
pub mod timing {
    /// Reset: master holds the bus low this long. Datasheet `t_RST` ≥ 480 µs.
    pub const RESET_LOW_US: u32 = 480;
    /// Reset: after releasing, wait this long before sampling for presence.
    ///
    /// The datasheet puts the presence pulse 15-60 µs after the release, so sampling at 70 µs is
    /// inside the window with margin.
    pub const RESET_PRESENCE_US: u32 = 70;
    /// Reset: after sampling, wait out the rest of `t_RSTH` (480 µs total).
    pub const RESET_RECOVERY_US: u32 = 410;

    /// Write a `1`: low this long, then release. Datasheet 1–15 µs.
    pub const WRITE_ONE_LOW_US: u32 = 10;
    /// Write a `1`: released this long, completing the slot.
    pub const WRITE_ONE_HIGH_US: u32 = 55;
    /// Write a `0`: low this long. Datasheet 60–120 µs.
    pub const WRITE_ZERO_LOW_US: u32 = 65;
    /// Write a `0`: released this long, completing the slot.
    pub const WRITE_ZERO_HIGH_US: u32 = 5;

    /// Read: hold the bus low this long to request a bit. Datasheet 1–15 µs.
    pub const READ_LOW_US: u32 = 3;
    /// Read: after releasing, wait this long before sampling the line.
    ///
    /// **The tightest number in the protocol.** 3 + 10 = 13 µs after the
    /// falling edge, against a datasheet window of 5–15 µs. See
    /// `read_the_sample_point_has_two_microseconds_of_headroom`.
    pub const READ_SAMPLE_US: u32 = 10;
    /// Read: after sampling, wait out the rest of the slot.
    pub const READ_RECOVERY_US: u32 = 53;
}

/// The microsecond value `esp_idf_hal::delay::Ets::delay_ns` would produce.
///
/// `Ets`'s `DelayNs` impl is
/// `Ets::delay_us(ns.saturating_add(NS_PER_US - 1) / NS_PER_US)`
/// (`esp-idf-hal-0.47.0/src/delay.rs:250-252`) with `NS_PER_US == 1000`
/// (`:65`) — i.e. **round up to the next whole microsecond**, and `0 ns` stays
/// `0`.
///
/// This is reproduced here so the rounding can be *tested* rather than assumed.
/// The test `delay_ns_rounds_up_to_the_next_microsecond` pins the formula
/// against the installed source, and
/// `the_slot_timings_survive_delay_ns_rounding` pins that none of
/// [`timing`]'s values is actually affected.
#[must_use]
pub const fn delay_ns_to_us_ceil(ns: u32) -> u32 {
    ns.saturating_add(999) / 1000
}

// ======================================================================= bus

/// The electrical half of the 1-Wire bus: three operations, and nothing else.
///
/// This trait is the whole seam. Everything the DS18B20 *means* — which bits go
/// out in which order, what the CRC is, when a reading is a fault — is decided
/// in this crate and tested on the host against a fake bus that records the
/// exact bit stream. The device implementation only has to move a pin.
///
/// # Why not `esp_idf_hal::onewire`
///
/// Read from the installed source rather than assumed
/// (`esp-idf-hal-0.47.0/src/onewire.rs`):
///
/// * its own module doc lists `crc checking on messages` and
///   `helper methods on the driver for executing commands` under `todo:`
///   (`:15-17`) — the two things a DS18B20 driver is *made of* are missing;
/// * `OWDriver` exposes only `read(&[u8])` / `write(&[u8])` / `reset()` /
///   `search()`, so a caller must bit-bang the scratchpad protocol
///   itself over a byte interface, which is where the CRC would have to live
///   anyway;
/// * it is RMT-backed, which allocates an RMT channel and cannot be driven
///   from a bit-bang timing model, and the DS18B20's `t_SAMPLE` window is 10 µs
///   wide.
///
/// So the bus is bit-banged here. Recheck 2027-01 before moving the probe onto `OWDriver`: that is
/// the path which does not call `interrupt::free`.
pub trait OneWireBus {
    /// What the transport reports when the pin itself fails.
    type Error;

    /// Drive a reset pulse and report whether a device answered with a
    /// presence pulse.
    ///
    /// Implements `OneWire::reset`.
    ///
    /// # Errors
    ///
    /// Whatever the transport reports if the pin itself fails. **A `false`
    /// result is not an error** — it is a well-formed "nothing on the bus".
    fn reset(&mut self) -> Result<bool, Self::Error>;

    /// Clock out one bit: pull low for [`timing::WRITE_ONE_LOW_US`] or
    /// [`timing::WRITE_ZERO_LOW_US`], then release.
    ///
    /// # Errors
    ///
    /// Whatever the transport reports if the pin itself fails.
    fn write_bit(&mut self, bit: bool) -> Result<(), Self::Error>;

    /// Clock in one bit: pull low for [`timing::READ_LOW_US`], release, wait
    /// [`timing::READ_SAMPLE_US`], sample the line, then wait out the slot.
    ///
    /// # Errors
    ///
    /// Whatever the transport reports if the pin itself fails.
    fn read_bit(&mut self) -> Result<bool, Self::Error>;
}

/// A failure of the 1-Wire protocol, as distinct from a failure of the bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OneWireError<E> {
    /// The transport failed.
    Bus(E),
    /// The reset produced no presence pulse: nothing on the bus.
    ///
    /// No device pulled the bus low in response to a reset, so nothing is on the wire. That is a
    /// normal state, not a transport failure.
    NoPresence,
}

// ================================================================ byte layer

/// Clock out `byte`, least-significant bit first.
///
/// The low bit first, then up: 1-Wire clocks the least-significant bit of a byte out first.
///
/// # Errors
///
/// [`OneWireError::Bus`] if the transport fails.
pub fn write_byte<B: OneWireBus>(bus: &mut B, byte: u8) -> Result<(), OneWireError<B::Error>> {
    for mask in (0u8..8).map(|i| 1u8 << i) {
        bus.write_bit(byte & mask != 0).map_err(OneWireError::Bus)?;
    }
    Ok(())
}

/// Clock in one byte, least-significant bit first.
///
/// `OneWire::read` accumulates with the same ascending
/// mask, so the first bit read is the low bit.
///
/// # Errors
///
/// [`OneWireError::Bus`] if the transport fails.
pub fn read_byte<B: OneWireBus>(bus: &mut B) -> Result<u8, OneWireError<B::Error>> {
    let mut byte = 0u8;
    for mask in (0u8..8).map(|i| 1u8 << i) {
        if bus.read_bit().map_err(OneWireError::Bus)? {
            byte |= mask;
        }
    }
    Ok(byte)
}

// ================================================================= commands

/// `SKIP ROM` — address "the only device on the bus".
pub const CMD_SKIP_ROM: u8 = 0xCC;
/// `MATCH ROM` — address one specific device by its ROM code.
pub const CMD_MATCH_ROM: u8 = 0x55;
/// `SEARCH ROM` — the enumeration command the ROM search drives.
pub const CMD_SEARCH_ROM: u8 = 0xF0;
/// `CONVERT T` — start a temperature conversion.
pub const CMD_CONVERT_T: u8 = 0x44;
/// `WRITE SCRATCHPAD`.
pub const CMD_WRITE_SCRATCHPAD: u8 = 0x4E;
/// `READ SCRATCHPAD`.
pub const CMD_READ_SCRATCHPAD: u8 = 0xBE;
/// `COPY SCRATCHPAD` — persist the scratchpad to EEPROM.
pub const CMD_COPY_SCRATCHPAD: u8 = 0x48;

/// How to address the device before a function command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RomSelection {
    /// `0xCC`: select every device on the bus. Only safe with exactly one device, because the
    /// command reaches all of them.
    Skip,
    /// `0x55` followed by the eight ROM bytes: select the one device with that ROM.
    Match(Rom),
}

/// Reset, check for presence, and address the device.
///
/// Reset, then the ROM command. Every command sequence on the bus starts this way, and a reset that
/// gets no answer means no device is listening.
///
/// # Errors
///
/// [`OneWireError::NoPresence`] if the reset produced no presence pulse, or
/// [`OneWireError::Bus`] if the transport fails.
pub fn address<B: OneWireBus>(
    bus: &mut B,
    selection: RomSelection,
) -> Result<(), OneWireError<B::Error>> {
    if !bus.reset().map_err(OneWireError::Bus)? {
        return Err(OneWireError::NoPresence);
    }
    match selection {
        RomSelection::Skip => write_byte(bus, CMD_SKIP_ROM),
        RomSelection::Match(rom) => {
            write_byte(bus, CMD_MATCH_ROM)?;
            for byte in rom.0 {
                write_byte(bus, byte)?;
            }
            Ok(())
        }
    }
}

/// `CONVERT T` — start a conversion and return immediately.
///
/// Non-blocking by construction: the device converts in the background and
/// [`ScratchPad::conversion_time`] is how long the caller waits before reading: this issues the
/// command and returns, and the conversion runs while the caller does other work.
///
/// # Errors
///
/// [`OneWireError::NoPresence`] or [`OneWireError::Bus`], as
/// [`address`].
pub fn request_conversion<B: OneWireBus>(
    bus: &mut B,
    selection: RomSelection,
) -> Result<(), OneWireError<B::Error>> {
    address(bus, selection)?;
    write_byte(bus, CMD_CONVERT_T)
}

/// `READ SCRATCHPAD` — reset, address, command, nine bytes, reset.
///
/// Reads the nine scratchpad bytes and then resets the bus, which releases the device if a read was
/// cut short.
///
/// # Errors
///
/// [`OneWireError::NoPresence`] or [`OneWireError::Bus`], as
/// [`address`].
pub fn read_scratchpad<B: OneWireBus>(
    bus: &mut B,
    selection: RomSelection,
) -> Result<ScratchPad, OneWireError<B::Error>> {
    address(bus, selection)?;
    write_byte(bus, CMD_READ_SCRATCHPAD)?;
    let mut bytes = [0u8; SCRATCHPAD_LEN];
    for slot in &mut bytes {
        *slot = read_byte(bus)?;
    }
    if !bus.reset().map_err(OneWireError::Bus)? {
        return Err(OneWireError::NoPresence);
    }
    Ok(ScratchPad(bytes))
}

/// `WRITE SCRATCHPAD` + `COPY SCRATCHPAD` — persist a scratchpad to EEPROM.
///
/// Writes the alarm registers and the configuration, then the copy-to-EEPROM command. Used only by
/// [`ScratchPad::with_resolution`], i.e. once at boot.
///
/// **The 20 ms is the device's, not ours.** The datasheet's NVRAM write cycle is typically 2 ms and
/// 10 ms maximum, with 20 ms the figure to allow for sensors that take longer in practice. Nothing
/// blocks here: the command returns immediately, and the caller must not read the scratchpad back
/// before 20 ms have passed. Since
/// the only caller is the boot-time resolution write and nothing reads the
/// scratchpad for the following 400 ms, the wait is unobservable.
///
/// # Errors
///
/// [`OneWireError::NoPresence`] or [`OneWireError::Bus`], as
/// [`address`].
pub fn write_scratchpad<B: OneWireBus>(
    bus: &mut B,
    selection: RomSelection,
    scratchpad: &ScratchPad,
) -> Result<(), OneWireError<B::Error>> {
    address(bus, selection)?;
    write_byte(bus, CMD_WRITE_SCRATCHPAD)?;
    for index in [2, 3, 4] {
        write_byte(bus, scratchpad.0[index])?;
    }
    address(bus, selection)?;
    write_byte(bus, CMD_COPY_SCRATCHPAD)
}

// =============================================================== scratchpad

/// The number of bytes in a scratchpad: eight data plus the CRC.
pub const SCRATCHPAD_LEN: usize = 9;

/// Scratchpad index of the temperature LSB register.
pub(crate) const SP_TEMP_LSB: usize = 0;
/// Scratchpad index of the temperature MSB register.
pub(crate) const SP_TEMP_MSB: usize = 1;
/// Scratchpad index of the high alarm register.
const SP_HIGH_ALARM: usize = 2;
/// Scratchpad index of the low alarm register.
#[allow(
    dead_code,
    reason = "names the scratchpad layout; only the tests read it, so `expect` would fail one build"
)]
const SP_LOW_ALARM: usize = 3;
/// Scratchpad index of the configuration register (resolution).
const SP_CONFIGURATION: usize = 4;
/// Scratchpad index of `COUNT REMAIN`.
pub(crate) const SP_COUNT_REMAIN: usize = 6;
/// Scratchpad index of `COUNT PER °C`.
const SP_COUNT_PER_C: usize = 7;
/// Scratchpad index of the CRC.
const SP_CRC: usize = 8;

/// The sign-extension mask for a negative reading: bits 31..19 set, i.e. -524288 as an i32.
///
/// Stated as the negative i32 it is, because writing it as a `u32` and casting would be a trap: the
/// obvious spelling, `i32::MIN | 0x0007_FFFF`, is `0x807F_FFFF` and decodes every negative
/// temperature wrongly.
const SIGN_EXTENSION: i32 = -0x0008_0000;

/// The configuration-register value for 9-bit resolution.
const RES_9_BIT: u8 = 0x1F;
/// The configuration-register value for 10-bit resolution.
const RES_10_BIT: u8 = 0x3F;
/// The configuration-register value for 11-bit resolution.
pub const RES_11_BIT: u8 = 0x5F;
/// The configuration-register value for 12-bit resolution.
const RES_12_BIT: u8 = 0x7F;

/// A device's nine-byte scratchpad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScratchPad(pub [u8; SCRATCHPAD_LEN]);

impl ScratchPad {
    /// Whether the CRC byte matches the other eight.
    ///
    /// The nine bytes must satisfy the 1-Wire CRC. Nothing else is decoded until they do.
    #[must_use]
    pub fn crc_valid(self) -> bool {
        crc8(&self.0[..SP_CRC]) == self.0[SP_CRC]
    }

    /// Whether every byte is zero.
    ///
    /// A device that has powered up but never converted reads all zeros,
    /// and that state passes the CRC -- the CRC of nothing is zero -- so this is the check that
    /// catches it.
    #[must_use]
    pub fn is_all_zeros(self) -> bool {
        self.0.iter().all(|&byte| byte == 0)
    }

    /// The resolution the device is configured for, from the configuration
    /// register.
    ///
    /// 0 if the configuration register holds an unrecognised value, which means the device is not
    /// answering with a scratchpad this firmware can read.
    #[must_use]
    pub fn resolution(self) -> u8 {
        match self.0[SP_CONFIGURATION] {
            RES_9_BIT => 9,
            RES_10_BIT => 10,
            RES_11_BIT => 11,
            RES_12_BIT => 12,
            _ => 0,
        }
    }

    /// The maximum conversion time for the configured resolution, in
    /// milliseconds.
    ///
    /// 94 / 188 / 375 / 750 ms, which is the datasheet's §"Temperature measurement" maximum per
    /// resolution.
    #[must_use]
    pub const fn conversion_time(self) -> Millis {
        match self.0[SP_CONFIGURATION] {
            RES_9_BIT => Millis::new(94),
            RES_10_BIT => Millis::new(188),
            RES_11_BIT => Millis::new(375),
            _ => Millis::new(750),
        }
    }

    /// A copy of this scratchpad with the resolution changed.
    ///
    /// Only `SP_CONFIGURATION` changes and every other byte is left alone, including the CRC, which
    /// is recomputed on the way out. This returns the
    /// bytes; [`Self::with_valid_crc`] stamps the CRC.
    #[must_use]
    pub fn with_resolution(self, bits: u8) -> Self {
        let mut next = self;
        next.0[SP_CONFIGURATION] = match bits {
            9 => RES_9_BIT,
            10 => RES_10_BIT,
            11 => RES_11_BIT,
            _ => RES_12_BIT,
        };
        next
    }

    /// This scratchpad with byte 8 recomputed as the CRC of bytes 0..7.
    #[must_use]
    pub fn with_valid_crc(self) -> Self {
        let mut next = self;
        next.0[SP_CRC] = crc8(&next.0[..SP_CRC]);
        next
    }

    /// The scratchpad a freshly reset device presents: all zeros.
    #[must_use]
    pub const fn power_on() -> Self {
        Self([0u8; SCRATCHPAD_LEN])
    }

    /// Decode the scratchpad into a temperature, or a fault.
    ///
    /// The whole decision in one place: readable, not all zeros, CRC good, then the family-specific
    /// fault registers, then the raw-to-celsius mapping. The order matters -- presence before the
    /// faults, and the fault registers before any mapping, because the fault values are raw
    /// sentinels that the mapping would otherwise fold into a temperature.
    ///
    /// # Errors
    ///
    /// The [`Ds18b20Fault`] the scratchpad represents. All six are rejected; see
    /// [`Ds18b20Fault::is_rejected`].
    pub fn interpret(self, rom: Rom) -> Result<f32, Ds18b20Fault> {
        // Readable, not all zeros, CRC good. A failure of any of the three is a disconnection:
        // there is no reading to decode.
        if self.is_all_zeros() || !self.crc_valid() {
            return Err(Ds18b20Fault::Disconnected);
        }

        // The MAX31850 fault register, read only for family 0x3B with configuration bit 7 set. Its
        // fault values are raw sentinels, not temperatures, and they are kept as named faults here
        // rather than folded to one value. See `div7_every_ds18b20_fault_is_rejected` and
        // `the_dallas_wiring_sentinels_are_rejected`.
        if rom.family() == FAMILY_MAX31850
            && self.0[SP_CONFIGURATION] & 0x80 != 0
            && self.0[SP_TEMP_LSB] & 1 != 0
        {
            return Err(if self.0[SP_HIGH_ALARM] & 1 != 0 {
                Ds18b20Fault::Open
            } else if self.0[SP_HIGH_ALARM] >> 1 & 1 != 0 {
                Ds18b20Fault::ShortGnd
            } else if self.0[SP_HIGH_ALARM] >> 2 & 1 != 0 {
                Ds18b20Fault::ShortVdd
            } else {
                Ds18b20Fault::Disconnected
            });
        }

        // Sign extension, then the two's-complement assembly. The mask is OR-ed in when bit 7 of
        // the MSB register is set, to propagate the sign across the 16 raw bits shifted left by
        // three.
        //
        // [`SIGN_EXTENSION`] is a *negative* i32, so OR-ing in the low 19 bits and leaving bit 31
        // clear would be a different constant and would decode every negative temperature wrongly,
        // so this is written as the value it is rather than as a `u32` cast.
        let neg: i32 = if self.0[SP_TEMP_MSB] & 0x80 != 0 {
            SIGN_EXTENSION
        } else {
            0
        };
        // Each register is cast to i16 and *that* is shifted, so the sign of the LSB register
        // propagates. An i32 cast before the shift would not: `(0xFF_i32) << 3` is 0x7F8, not -8.
        let mut raw = (i32::from(i16::from(self.0[SP_TEMP_MSB])) << 11)
            | (i32::from(i16::from(self.0[SP_TEMP_LSB])) << 3)
            | neg;

        // The DS18B20-only power-on-reset and brownout patterns, checked
        // against the *raw* register bytes, before the DS18S20 correction.
        if rom.family() == FAMILY_DS18B20 {
            if self.0[SP_TEMP_LSB] == 0x50
                && self.0[SP_TEMP_MSB] == 0x05
                && self.0[SP_COUNT_REMAIN] == 0x0C
            {
                return Err(Ds18b20Fault::PowerOnReset);
            }
            if self.0[SP_TEMP_LSB] == 0xFF && self.0[SP_TEMP_MSB] == 0x07 {
                return Err(Ds18b20Fault::InsufficientPower);
            }
        }

        // The DS18S20/DS1820 extended-resolution correction: below 0.5 °C the fraction comes from
        // the count registers rather than from the LSB register.
        if rom.family() == FAMILY_DS18S20 && self.0[SP_COUNT_PER_C] != 0 {
            let correction = (i32::from(self.0[SP_COUNT_PER_C] - self.0[SP_COUNT_REMAIN]) << 7)
                / i32::from(self.0[SP_COUNT_PER_C]);
            raw = (((raw & 0xfff0) << 3) - 32 + correction) | neg;
        }

        Ok(raw_to_celsius(raw))
    }
}

/// The raw count scaled to °C: `raw * 0.0078125` (1/128), unless the raw value is at or below
/// [`DISCONNECTED_RAW`], which maps to [`DISCONNECTED_C`].
///
/// **The comparison is a range check, not a sentinel check**, and that has a consequence worth
/// stating: `DISCONNECTED_RAW` is -7040, which is exactly -55 °C in 1/128 °C units, the bottom of
/// the DS18B20's range. So a *legitimate* reading of -55.0 °C comes back as "disconnected". The
/// boundary is kept as it is, pinned by `minus_55_c_is_reported_as_disconnected`.
#[must_use]
pub fn raw_to_celsius(raw: i32) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a sensor count is 16 bits, exact in f32"
    )]
    {
        if raw <= DISCONNECTED_RAW {
            DISCONNECTED_C
        } else {
            raw as f32 * 0.007_812_5
        }
    }
}

/// The raw count at or below which a reading is treated as "not there": -7040, which is exactly
/// -55 °C in 1/128 °C units.
pub const DISCONNECTED_RAW: i32 = -7040;
/// The Celsius value every 1-Wire fault sentinel folds to, and the value a disconnection is
/// reported as.
pub const DISCONNECTED_C: f32 = -127.0;

/// A fault the DS18B20 driver can report.
///
/// The discriminants are the Celsius sentinels the 1-Wire convention uses for each fault, so a log
/// line that prints the number and a log line that prints the name say the same thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ds18b20Fault {
    /// -127. No presence pulse, or the scratchpad read back as all zeros, or the CRC failed, or the
    /// raw value landed at or below [`DISCONNECTED_RAW`].
    Disconnected,
    /// -254. The MAX31850's alarm register reports the probe wiring as open.
    Open,
    /// -253. The MAX31850's alarm register reports the probe shorted to ground.
    ShortGnd,
    /// -252. The MAX31850's alarm register reports the probe shorted to its supply.
    ShortVdd,
    /// -251. The device reported a power-on reset. Rejected like every other fault, but kept as a
    /// name so the log says what happened rather than "not connected".
    PowerOnReset,
    /// -250. The device reported insufficient supply voltage, which is what a long or thin cable
    /// produces under load.
    InsufficientPower,
    /// 🔴 **Added by this port.** The decoded temperature is outside the -50 ..= 150 window
    /// [`crate::sensor::ds18b20`] applies.
    ///
    /// There is no sentinel for this fault, because there is no number the sensor would have
    /// reported for it: the reading is arithmetically valid and physically implausible.
    OutOfRange,
}

impl Ds18b20Fault {
    /// The Celsius sentinel that stands for this fault in the 1-Wire convention, or `None` when the
    /// convention has no number for it.
    ///
    /// # Every one of the six is -127, and that is the whole finding
    ///
    /// The convention has one value for "not there". Every raw sentinel the device can report is at
    /// or below [`DISCONNECTED_RAW`], so every one of them maps to [`DISCONNECTED_C`]:
    ///
    /// | fault | its Celsius sentinel | its raw sentinel | maps to |
    /// | --- | --- | --- | --- |
    /// | `Disconnected` | -127 | -7040 | **-127** |
    /// | `Open` | -254 | -32512 | **-127** |
    /// | `ShortGnd` | -253 | -32384 | **-127** |
    /// | `ShortVdd` | -252 | -32256 | **-127** |
    /// | `PowerOnReset` | -251 | -32128 | **-127** |
    /// | `InsufficientPower` | -250 | -32000 | **-127** |
    ///
    /// So the six distinct values are unreachable as *returned* values: a caller that reads only
    /// the sentinel cannot tell an unplugged probe from a power-on reset, and the message it prints
    /// names the wrong cause. [`OutOfRange`](Self::OutOfRange) returns `None` because it is a range
    /// decision, not something the device said.
    #[must_use]
    pub const fn sentinel(self) -> Option<f32> {
        match self {
            Self::Disconnected
            | Self::Open
            | Self::ShortGnd
            | Self::ShortVdd
            | Self::PowerOnReset
            | Self::InsufficientPower => Some(-127.0),
            Self::OutOfRange => None,
        }
    }

    /// Whether the fault stops the reading from being used as a temperature.
    ///
    /// **All six, and that is the point of the table on [`sentinel`](Self::sentinel).** Every
    /// sensor fault arrives as the same -127, so a DS18B20 reporting a power-on reset is rejected
    /// -- as "not connected".
    ///
    /// An earlier revision of this repository recorded `PowerOnReset` and `InsufficientPower` as
    /// **not** rejected, on the reading that -251 and -250 reach the control loop as temperatures.
    /// They do not: the conversion folds the raw sentinel to -127 before any caller sees it. The
    /// test
    /// `div7_every_ds18b20_fault_is_rejected` pins the truth so it cannot be re-instated.
    #[must_use]
    pub const fn is_rejected(self) -> bool {
        !matches!(self, Self::OutOfRange)
    }
}

impl core::fmt::Display for Ds18b20Fault {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let text = match self {
            Self::Disconnected => "not connected",
            Self::Open => "open circuit",
            Self::ShortGnd => "short to ground",
            Self::ShortVdd => "short to VDD",
            Self::PowerOnReset => "power-on reset",
            Self::InsufficientPower => "insufficient power",
            Self::OutOfRange => "outside -50..150 C",
        };
        f.write_str(text)
    }
}

#[cfg(test)]
// `clippy::assert_is_empty` is new in clippy 1.99, the channel the CI host gate
// runs on (`CC_RUST_TOOLCHAIN=stable`). The emptiness assertions in this module
// are on COLLECTIONS, so the suggested `assert_eq!(x, "")` does not typecheck,
// and `assert_eq!(x.len(), 0)` would print a count instead of the contents.
// Everywhere else in this crate a `String` emptiness assertion is written
// `assert_ne!(x, "")` and is still linted.
#[expect(
    clippy::assert_is_empty,
    reason = "the assertions are on collections, so assert_eq!(x, \"\") does not typecheck"
)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    /// The ROM the board actually answered with, from the recovered firmware's boot log:
    /// `0x41af78cdaa376928`.
    ///
    /// **The log prints the ROM in wire order, least-significant byte first.**
    /// In the device's own layout byte 0 is the family code and byte 7 is the
    /// CRC, so read in *that* order the printed string is not CRC-valid; read
    /// reversed it is, and the family code lands on byte 0 where the DS18B20
    /// puts it. Both forms are in the tests, because getting this wrong is
    /// silent: a byte-reversed ROM still parses and still shows a plausible
    /// `0x28` at one end, and fails only at the CRC check. See
    /// `the_logged_rom_is_printed_least_significant_byte_first`.
    const LIVE_ROM: Rom = Rom([0x28, 0x69, 0x37, 0xAA, 0xCD, 0x78, 0xAF, 0x41]);

    /// The same ROM as the boot log printed it, i.e. reversed.
    const LIVE_ROM_AS_LOGGED: [u8; ROM_LEN] = [0x41, 0xAF, 0x78, 0xCD, 0xAA, 0x37, 0x69, 0x28];

    /// Build a scratchpad from a raw 1/128 °C reading at 11-bit resolution.
    ///
    /// The inverse of `interpret`'s assembly, which is
    /// `raw = (MSB << 11) | (LSB << 3)`.
    ///
    /// Both shifts are **arithmetic on an `i16`**, which is what makes the
    /// encoding work for negative temperatures: -1280 (`0xFB00`) has
    /// `MSB = 0xFF`, not `0x1F`. An unsigned shift here produces a MSB of
    /// `0x1F` and decodes to +502 °C.
    ///
    /// `extra` supplies the five registers a temperature reading does not
    /// determine: high alarm, low alarm, byte 5, `COUNT REMAIN` and
    /// `COUNT PER °C`.
    fn scratchpad_from_raw(raw: i16, extra: [u8; 5]) -> ScratchPad {
        #[expect(
            clippy::cast_sign_loss,
            reason = "reinterpreting the two's-complement bits is the point"
        )]
        let lsb = ((raw as u16 >> 3) & 0xFF) as u8;
        #[expect(
            clippy::cast_sign_loss,
            reason = "reinterpreting the two's-complement bits is the point"
        )]
        let msb = ((raw >> 11) & 0xFF) as u8;
        let mut pad = ScratchPad([
            lsb, msb, extra[0], extra[1], RES_11_BIT, extra[2], extra[3], extra[4], 0,
        ]);
        pad = pad.with_valid_crc();
        pad
    }

    const UNUSED: [u8; 5] = [0x00; 5];

    // ================================================================= CRC

    #[test]
    fn crc8_of_the_empty_slice_is_zero() {
        // `OneWire::crc8` starts from `crc = 0` and does nothing for len 0.
        assert_eq!(crc8(&[]), 0);
    }

    #[test]
    fn crc8_matches_the_table_driven_crc() {
        // The table form is `crc = table[crc & 0x0f] ^ table[16 + crc >> 4]` once per input byte,
        // after `crc = byte ^ crc`.
        // Exhaustive over every (crc, byte) pair reachable in one step: 256
        // values of the pre-XOR accumulator are not all reachable, so instead
        // walk real buffers and compare the two formulations step for step.
        for len in 0..=9usize {
            let buf: Vec<u8> = (0..len)
                .map(|i| {
                    #[expect(clippy::cast_possible_truncation, reason = "`i` is at most 9")]
                    let n = i as u8;
                    n.wrapping_mul(37).wrapping_add(11)
                })
                .collect();
            let mut table_crc = 0u8;
            for &byte in &buf {
                table_crc = crc8_step_table(table_crc, byte);
            }
            assert_eq!(
                crc8(&buf),
                table_crc,
                "crc8 disagrees with the C++ table form for len {len}"
            );
        }
    }

    #[test]
    fn crc8_of_the_live_rom_is_its_last_byte() {
        // The ROM recovered from the board must be self-consistent, or
        // `Rom::crc_valid` would reject the very device the driver is for.
        assert!(
            LIVE_ROM.crc_valid(),
            "the recorded ROM 0x41af78cdaa376928 must CRC to 0x28"
        );
    }

    #[test]
    fn a_corrupted_rom_fails_its_crc() {
        let mut broken = LIVE_ROM;
        broken.0[3] ^= 0x01;
        assert!(!broken.crc_valid());
    }

    #[test]
    fn the_logged_rom_is_printed_least_significant_byte_first() {
        // The recovered firmware logged `0x41af78cdaa376928`. Read in the
        // device's own order that string is *not* CRC-valid; read reversed it
        // is, and the family code lands on byte 0 where the DS18B20 puts it.
        // So the log is printing the ROM as it went over the wire.
        let as_printed = Rom(LIVE_ROM_AS_LOGGED);
        assert!(
            !as_printed.crc_valid(),
            "the printed order must not validate"
        );
        // In the device's layout byte 0 is the family, so the printed string's
        // *first* byte 0x41 is where the family would be read from -- and it is
        // the CRC. The 0x28 the operator reads as "family" is the last byte.
        assert_eq!(as_printed.family(), 0x41, "byte 0 of the printed order");
        assert_eq!(
            as_printed.0[ROM_LEN - 1],
            0x28,
            "byte 7 of the printed order"
        );

        let mut reversed = as_printed;
        reversed.0.reverse();
        assert_eq!(reversed, LIVE_ROM);
        assert!(reversed.crc_valid());
        assert_eq!(reversed.family(), 0x28);
    }

    #[test]
    fn the_live_rom_is_a_ds18b20() {
        assert!(LIVE_ROM.is_ds18b20());
        assert_eq!(LIVE_ROM.family(), 0x28);
        assert!(LIVE_ROM.is_supported_family());
    }

    // ============================================================== timing

    #[test]
    fn delay_ns_rounds_up_to_the_next_microsecond() {
        // `esp-idf-hal-0.47.0/src/delay.rs:250-252`:
        // Ets::delay_ns(ns) == Ets::delay_us((ns + 999) / 1000)
        // with NS_PER_US == 1000 (`delay.rs:65`).
        assert_eq!(delay_ns_to_us_ceil(0), 0);
        assert_eq!(delay_ns_to_us_ceil(1), 1);
        assert_eq!(delay_ns_to_us_ceil(999), 1);
        assert_eq!(delay_ns_to_us_ceil(1000), 1);
        assert_eq!(delay_ns_to_us_ceil(1001), 2);
        assert_eq!(delay_ns_to_us_ceil(1999), 2);
        assert_eq!(delay_ns_to_us_ceil(2000), 2);
        assert_eq!(delay_ns_to_us_ceil(2001), 3);
        // Saturating, like the source: `saturating_add(999)` clamps at
        // u32::MAX *before* the division, so the result is 4294967, not
        // 4294968. Pinned because the difference is a whole microsecond and
        // the source's order of operations is the only thing that decides it.
        assert_eq!(delay_ns_to_us_ceil(u32::MAX), 4_294_967);
    }

    #[test]
    fn the_slot_timings_survive_delay_ns_rounding() {
        // Every value in `timing` is a whole number of microseconds, so
        // `delay_ns` would hand `ets_delay_us` exactly the same integer. The
        // 1 µs rounding therefore costs nothing here — which is the claim the
        // plan makes, tested rather than asserted.
        for us in [
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
        ] {
            assert_eq!(delay_ns_to_us_ceil(us.saturating_mul(1000)), us);
        }
    }

    #[test]
    fn the_slot_timings_sit_inside_the_ds18b20_windows() {
        // The DS18B20 datasheet's §"1-Wire Protocol" and §"Timing diagram".
        let slot_write_one = timing::WRITE_ONE_LOW_US + timing::WRITE_ONE_HIGH_US;
        let slot_write_zero = timing::WRITE_ZERO_LOW_US + timing::WRITE_ZERO_HIGH_US;
        let slot_read = timing::READ_LOW_US + timing::READ_SAMPLE_US + timing::READ_RECOVERY_US;

        // t_SLOT: 60-120 µs.
        for slot in [slot_write_one, slot_write_zero, slot_read] {
            assert!((60..=120).contains(&slot), "t_SLOT {slot} µs out of range");
        }
        // Write-1 t_LOW: 1-15 µs.
        const { assert!(timing::WRITE_ONE_LOW_US >= 1 && timing::WRITE_ONE_LOW_US <= 15) };
        // Write-0 t_LOW: 60-120 µs.
        const { assert!(timing::WRITE_ZERO_LOW_US >= 60 && timing::WRITE_ZERO_LOW_US <= 120) };
        // Read t_LOW: 1-15 µs.
        const { assert!(timing::READ_LOW_US >= 1 && timing::READ_LOW_US <= 15) };
        // t_RST: >= 480 µs. A `const` block, because the comparison is
        // between constants and clippy is right that an `assert!` at run time
        // would only ever be checking what the compiler already knows.
        const { assert!(timing::RESET_LOW_US >= 480) };
    }

    #[test]
    fn read_the_sample_point_has_two_microseconds_of_headroom() {
        // The tightest number in the whole protocol, stated so a future edit
        // to READ_SAMPLE_US has to update this test.
        let sample_point = timing::READ_LOW_US + timing::READ_SAMPLE_US;
        assert_eq!(sample_point, 13);
        assert!((5..=15).contains(&sample_point), "t_SAMPLE out of range");
        // And the window is 10 µs wide, so there is 2 µs of slack before the
        // sample point could be pushed past the datasheet maximum.
        assert_eq!(15 - sample_point, 2);
    }

    // ========================================================== scratchpad

    #[test]
    fn the_power_on_scratchpad_is_all_zeros_and_fails_crc_validity() {
        // Not both: all zeros IS CRC-valid (the CRC of nothing is zero), so the all-zeros check is
        // what catches a device that has powered up and never converted.
        let pad = ScratchPad::power_on();
        assert!(pad.is_all_zeros());
        assert!(pad.crc_valid());
        assert!(pad.interpret(LIVE_ROM).is_err());
    }

    #[test]
    fn a_powered_up_but_unconverted_device_is_disconnected() {
        // A device that has powered up but never converted reads all zeros, and `is_all_zeros` is
        // the only check that catches it -- the CRC of nothing is zero, so the CRC test passes.
        let pad = ScratchPad::power_on();
        assert!(pad.is_all_zeros());
        assert!(pad.crc_valid(), "the CRC cannot catch an all-zero read");
        assert_eq!(pad.interpret(LIVE_ROM), Err(Ds18b20Fault::Disconnected));

        // And with a *valid* CRC but a non-zero LSB register it is a real
        // reading of 0.0625 °C, not a fault: raw = 0x01 << 3 = 8.
        let mut pad = ScratchPad::power_on();
        pad.0[0] = 0x01;
        let pad = pad.with_valid_crc();
        assert!(!pad.is_all_zeros());
        assert_eq!(
            (pad.interpret(LIVE_ROM).unwrap_or_default() - 0.0625).to_bits(),
            0
        );
    }

    #[test]
    fn a_bad_crc_is_disconnected() {
        // 22.9 °C is not on the 11-bit grid: 22.9 * 128 = 2929.2, and the
        // device only reports multiples of 8 raw counts, so the nearest it can
        // say is 2928 -> 22.875 °C. This is why the recovered boot log reads
        // 22.88 and 23.25 rather than 22.9: it is printing two decimals of a
        // value whose true grid is 0.0625 °C.
        let pad = scratchpad_from_raw(2928, UNUSED);
        // Exact: 2928 / 128 is 22.875 and the division is exact.
        assert_eq!(
            (pad.interpret(LIVE_ROM).unwrap_or_default() - 22.875).to_bits(),
            0
        );
        let mut broken = pad;
        broken.0[SCRATCHPAD_LEN - 1] ^= 0xFF;
        assert_eq!(broken.interpret(LIVE_ROM), Err(Ds18b20Fault::Disconnected));
    }

    #[test]
    fn eleven_bit_readings_land_on_the_lsb_grid() {
        // 11 of 16 bits: the raw value is a multiple of 8 in 1/128 °C, i.e.
        // 0.0625 °C. The live log said 22.88 .. 23.25, both on that grid.
        // Every value must be a multiple of 0.0625 °C, and must round-trip.
        for eighth_quarters in [2928i16, 2944, 2960, 2976] {
            let expected = f32::from(eighth_quarters) / 128.0;
            let pad = scratchpad_from_raw(eighth_quarters, UNUSED);
            let got = pad.interpret(LIVE_ROM).unwrap_or_default();
            // Exact, not approximate: every value here is `raw / 128` and 128
            // is a power of two, so the division is exact in f32 and any
            // difference at all would be a real one.
            assert_eq!(
                (got - expected).to_bits(),
                0,
                "raw {eighth_quarters} decoded as {got}, expected {expected}"
            );
            // And the raw really is on the grid: the low three bits are zero.
            assert_eq!(eighth_quarters % 8, 0);
        }
    }

    #[test]
    fn negative_temperatures_sign_extend() {
        // -10 °C is -1280 in 1/128 units. The `neg` mask is what makes the
        // OR-assembled i32 come out negative.
        let pad = scratchpad_from_raw(-1280, UNUSED);
        assert_eq!(
            (pad.interpret(LIVE_ROM).unwrap_or_default() + 10.0).to_bits(),
            0
        );
    }

    #[test]
    fn minus_55_c_is_reported_as_disconnected() {
        // The raw comparison happens before scaling, and -7040 is exactly -55 °C, the bottom of the
        // DS18B20's range. So the lowest temperature the device can report comes back as
        // "disconnected". Moving the boundary would change what -55 °C means, so it is pinned.
        // -7040 raw is exactly -55.0 °C: both are exactly representable, and
        // 128 is a power of two, so the comparison is exact rather than
        // approximate.
        assert_eq!((-55.0f32 * 128.0).to_bits(), (-7040.0f32).to_bits());
        assert_eq!(DISCONNECTED_RAW, -7040);
        assert_eq!(
            raw_to_celsius(DISCONNECTED_RAW).to_bits(),
            DISCONNECTED_C.to_bits()
        );
        // One 1/128 °C step up is fine.
        let just_above = raw_to_celsius(DISCONNECTED_RAW + 1);
        assert!(just_above > -55.0);
    }

    #[test]
    fn the_power_on_reset_pattern_is_rejected_as_disconnected() {
        // The 0x50/0x05/0x0C pattern is a power-on reset, and its raw sentinel folds to -127, so it
        // is rejected as a disconnection. What is kept here is the *reason*: the -251 sentinel
        // exists but no caller compares against it, so the name is the only thing that
        // distinguishes a power-on reset from an unplugged probe.
        let mut pad = ScratchPad::power_on();
        pad.0[SP_TEMP_LSB] = 0x50;
        pad.0[SP_TEMP_MSB] = 0x05;
        pad.0[SP_COUNT_REMAIN] = 0x0C;
        let pad = pad.with_valid_crc();
        assert_eq!(pad.interpret(LIVE_ROM), Err(Ds18b20Fault::PowerOnReset));
    }

    #[test]
    fn the_ds18b20_fault_registers_cannot_be_reached() {
        // The three wiring faults are only ever returned for family 0x3B, the MAX31850, so on a
        // DS18B20 they are unreachable -- and even there their sentinels are at or below
        // [`DISCONNECTED_RAW`], so nothing that reads a temperature could tell them apart.
        let ds18b20 = scratchpad_from_raw(0x0B70, UNUSED);
        for extra in 0..=0xFFu8 {
            let mut pad = ds18b20;
            pad.0[SP_TEMP_LSB] = extra; // force the fault bit into every position
            pad.0[SP_HIGH_ALARM] = 0xFF;
            pad.0[SP_CONFIGURATION] = 0xFF;
            let pad = pad.with_valid_crc();
            let verdict = pad.interpret(LIVE_ROM);
            assert!(
                !matches!(
                    verdict,
                    Err(Ds18b20Fault::Open | Ds18b20Fault::ShortGnd | Ds18b20Fault::ShortVdd)
                ),
                "a DS18B20 scratchpad produced {verdict:?}"
            );
        }
    }

    #[test]
    fn the_dallas_wiring_sentinels_are_rejected() {
        // The MAX31850 *can* reach the three wiring faults, and they are rejected -- as -127, by
        // the fold documented on [`Ds18b20Fault::sentinel`], not as the -254/-253/-252 the enum
        // names. The three rejections are kept and the *reason* is reported, so a machine whose
        // probe is swapped for a MAX31850 fails closed with a diagnostic that points at the wiring.
        let max31850 = Rom([FAMILY_MAX31850, 0x28, 0x69, 0x37, 0xAA, 0xCD, 0x78, 0xAF]);
        for (alarm_bits, expected) in [
            (0b001u8, Ds18b20Fault::Open),
            (0b010, Ds18b20Fault::ShortGnd),
            (0b100, Ds18b20Fault::ShortVdd),
        ] {
            // Note the LSB register is rebuilt *after* the fault bit is set:
            // `scratchpad_from_raw` derives it from the raw value, so setting
            // it first and then calling the helper would lose the fault.
            let mut pad = scratchpad_from_raw(2928, UNUSED);
            pad.0[SP_TEMP_LSB] = (pad.0[SP_TEMP_LSB] & !0x01) | 0x01; // "fault detected"
            pad.0[SP_CONFIGURATION] |= 0x80; // MAX31850 mode
            pad.0[SP_HIGH_ALARM] = alarm_bits;
            let pad = pad.with_valid_crc();
            let fault = pad.interpret(max31850).unwrap_err();
            assert_eq!(fault, expected);
            assert!(fault.is_rejected(), "{fault} must be rejected");
        }
    }

    #[test]
    fn div7_every_ds18b20_fault_is_rejected() {
        // 🔴 All six faults are rejected, which corrects an earlier claim in this repository.
        //
        // The fault sentinels the device can report are raw values at or below
        // [`DISCONNECTED_RAW`], so the conversion folds every one of them to [`DISCONNECTED_C`]
        // before any caller sees them. The -254/-253/-252 values exist but nothing compares against
        // them, and neither does anything compare against -251 or -250.
        //
        // So a power-on reset is rejected -- as "not connected". What is wrong is the *message*,
        // not the rejection: see `div6_*` in `sensor::ds18b20`, which keeps the reason.
        for (fault, raw) in [
            (Ds18b20Fault::Disconnected, DISCONNECTED_RAW),
            (Ds18b20Fault::Open, -32512),
            (Ds18b20Fault::ShortGnd, -32384),
            (Ds18b20Fault::ShortVdd, -32256),
            (Ds18b20Fault::PowerOnReset, -32128),
            (Ds18b20Fault::InsufficientPower, -32000),
        ] {
            assert!(raw <= DISCONNECTED_RAW, "{fault}: raw {raw} must fold");
            assert_eq!(fault.sentinel(), Some(-127.0), "{fault}");
            assert!(fault.is_rejected(), "{fault} must be rejected");
            // And the fold is not a claim about the enum: it is the conversion's own arithmetic.
            assert_eq!(raw_to_celsius(raw), -127.0, "{fault}");
        }
    }

    #[test]
    fn the_out_of_range_fault_is_the_one_with_no_sentinel() {
        // `OutOfRange` is a range decision, not something the device reported, so the convention
        // has no sentinel for it.
        assert!(!Ds18b20Fault::OutOfRange.is_rejected());
        assert_eq!(Ds18b20Fault::OutOfRange.sentinel(), None);
    }

    #[test]
    fn the_insufficient_power_pattern_is_reported() {
        let mut pad = ScratchPad::power_on();
        pad.0[SP_TEMP_LSB] = 0xFF;
        pad.0[SP_TEMP_MSB] = 0x07;
        let pad = pad.with_valid_crc();
        assert_eq!(
            pad.interpret(LIVE_ROM),
            Err(Ds18b20Fault::InsufficientPower)
        );
    }

    #[test]
    fn resolution_and_conversion_time_come_from_the_configuration_register() {
        // The wait per resolution, read from the configuration register.
        for (bits, expected_crc, expected_ms) in [
            (9u8, RES_9_BIT, 94u32),
            (10, RES_10_BIT, 188),
            (11, RES_11_BIT, 375),
            (12, RES_12_BIT, 750),
        ] {
            let mut pad = ScratchPad::power_on();
            pad.0[SP_TEMP_LSB] = 0x00;
            pad.0[SP_TEMP_MSB] = 0x05;
            pad = pad.with_resolution(bits).with_valid_crc();
            assert_eq!(pad.0[SP_CONFIGURATION], expected_crc);
            assert_eq!(pad.resolution(), bits);
            assert_eq!(pad.conversion_time(), Millis::new(expected_ms));
        }
    }

    #[test]
    fn eleven_bit_is_what_the_firmware_configures() {
        // 11-bit is the resolution the firmware configures at boot, and 375 ms is its conversion
        // maximum from the configuration register.
        let mut pad = ScratchPad::power_on();
        pad.0[SP_TEMP_LSB] = 0x00;
        pad.0[SP_TEMP_MSB] = 0x05;
        let pad = pad.with_resolution(11).with_valid_crc();
        assert_eq!(pad.resolution(), 11);
        assert_eq!(pad.conversion_time(), Millis::new(375));
    }

    #[test]
    fn set_resolution_only_touches_the_configuration_register() {
        let before = scratchpad_from_raw(0x0B70, [0xAA, 0x55, 0x11, 0x22, 0x33]);
        let after = before.with_resolution(12);
        assert_eq!(after.0[SP_CONFIGURATION], RES_12_BIT);
        for index in [
            SP_TEMP_LSB,
            SP_TEMP_MSB,
            SP_HIGH_ALARM,
            SP_LOW_ALARM,
            5,
            SP_COUNT_REMAIN,
            SP_COUNT_PER_C,
        ] {
            assert_eq!(after.0[index], before.0[index], "index {index} changed");
        }
    }

    #[test]
    fn the_ds18s20_correction_uses_the_count_registers() {
        // The DS18S20 correction: `TEMP_READ - 0.25 + (COUNT_PER_C - COUNT_REMAIN) / COUNT_PER_C`.
        // 0.5 °C raw with COUNT_PER_C == 0x10 and COUNT_REMAIN == 0x0E is
        // 0.5 - 0.25 + 2/16 = 0.375 °C.
        let ds18s20 = Rom([FAMILY_DS18S20, 0x28, 0x69, 0x37, 0xAA, 0xCD, 0x78, 0xAF]);
        // The DS18S20's raw register holds 9 bits at 0.5 °C, so raw 0x40 is
        // 0.5 °C and the sub-0.5 °C fraction comes from the count registers.
        // As an integer expression:
        // ((raw & 0xfff0) << 3) - 32 + ((COUNT_PER_C - COUNT_REMAIN) << 7) / COUNT_PER_C
        // = (0x40 << 3) - 32 + ((0x10 - 0x0E) << 7) / 0x10
        // = 512 - 32 + 16 = 496  ->  496 / 128 = 3.875 °C.
        let mut pad = scratchpad_from_raw(0x0040, [0x00, 0x00, 0x00, 0x0E, 0x10]);
        pad.0[SP_CONFIGURATION] = 0; // a DS18S20 has no resolution register
        let pad = pad.with_valid_crc();
        let got = pad.interpret(ds18s20).unwrap_or_default();
        // 496 / 128 = 3.875 exactly.
        assert_eq!((got - 3.875).to_bits(), 0, "got {got} °C");
    }

    // ======================================================== bit-level I/O

    /// A bus that records every bit written and replays a scripted read.
    struct FakeBus {
        written: Vec<bool>,
        to_read: Vec<bool>,
        present: bool,
    }

    impl FakeBus {
        fn new(present: bool) -> Self {
            Self {
                written: Vec::new(),
                to_read: Vec::new(),
                present,
            }
        }

        /// Every byte written, LSB first, as the bus saw it.
        fn written_bytes(&self) -> Vec<u8> {
            self.written
                .chunks(8)
                .map(|chunk| {
                    chunk
                        .iter()
                        .enumerate()
                        .fold(0u8, |acc, (i, &bit)| acc | u8::from(bit) << i)
                })
                .collect()
        }
    }

    impl OneWireBus for FakeBus {
        type Error = core::convert::Infallible;

        fn reset(&mut self) -> Result<bool, Self::Error> {
            Ok(self.present)
        }

        fn write_bit(&mut self, bit: bool) -> Result<(), Self::Error> {
            self.written.push(bit);
            Ok(())
        }

        fn read_bit(&mut self) -> Result<bool, Self::Error> {
            Ok(self.to_read.pop().unwrap_or(false))
        }
    }

    #[test]
    fn bytes_go_out_least_significant_bit_first() {
        // The low bit goes out first, so bit 0 is the first bit on the wire.
        let mut bus = FakeBus::new(true);
        write_byte(&mut bus, 0b1000_0001).unwrap_or(());
        assert_eq!(
            bus.written,
            vec![true, false, false, false, false, false, false, true]
        );
    }

    #[test]
    fn bytes_come_in_least_significant_bit_first() {
        // Bits come back in the same order they went out: low bit first.
        let mut bus = FakeBus::new(true);
        // `read_bit` pops from the *back*, so the first element of the script
        // is the eighth bit on the wire. Setting both ends makes the byte
        // order unambiguous: a LSB-first reader gets 0b1000_0001, a MSB-first
        // one gets 0b1000_0001 too — so the low bit is set and the high bit is
        // set, and the asymmetry is that *both* are set. Use a single low bit
        // instead: LSB-first gives 1, MSB-first gives 128.
        bus.to_read = vec![false, false, false, false, false, false, false, true];
        assert_eq!(read_byte(&mut bus).unwrap_or(0), 1);
    }

    #[test]
    fn match_rom_sends_the_command_then_the_eight_rom_bytes() {
        // `Match` writes 0x55, then the eight ROM bytes least-significant byte first.
        let mut bus = FakeBus::new(true);
        address(&mut bus, RomSelection::Match(LIVE_ROM)).unwrap_or(());
        assert_eq!(bus.written_bytes(), {
            let mut expect = vec![CMD_MATCH_ROM];
            expect.extend_from_slice(&LIVE_ROM.0);
            expect
        });
    }

    #[test]
    fn skip_rom_sends_only_the_command() {
        // `Skip` writes 0xCC and nothing else.
        let mut bus = FakeBus::new(true);
        address(&mut bus, RomSelection::Skip).unwrap_or(());
        assert_eq!(bus.written_bytes(), vec![CMD_SKIP_ROM]);
    }

    #[test]
    fn convert_t_is_the_second_byte_after_the_rom_selection() {
        let mut bus = FakeBus::new(true);
        request_conversion(&mut bus, RomSelection::Skip).unwrap_or(());
        assert_eq!(bus.written_bytes(), vec![CMD_SKIP_ROM, CMD_CONVERT_T]);
    }

    #[test]
    fn reading_the_scratchpad_reads_nine_bytes_after_read_scratchpad() {
        let mut bus = FakeBus::new(true);
        // 72 bits of scratchpad, all zero, reversed so LSB-first reads them out
        // in the documented order.
        bus.to_read = vec![false; SCRATCHPAD_LEN * 8];
        let pad = read_scratchpad(&mut bus, RomSelection::Skip);
        assert_eq!(
            pad.unwrap_or(ScratchPad([0xFF; SCRATCHPAD_LEN])).0,
            [0u8; SCRATCHPAD_LEN]
        );
        assert_eq!(bus.written_bytes(), vec![CMD_SKIP_ROM, CMD_READ_SCRATCHPAD]);
    }

    #[test]
    fn a_missing_device_is_no_presence_not_a_bus_error() {
        // No presence pulse is reported as such, not as a bus error: an empty bus is a normal state
        // rather than a transport failure.
        let mut bus = FakeBus::new(false);
        assert_eq!(
            address(&mut bus, RomSelection::Skip),
            Err(OneWireError::NoPresence)
        );
        assert_eq!(
            request_conversion(&mut bus, RomSelection::Skip),
            Err(OneWireError::NoPresence)
        );
        assert_eq!(
            read_scratchpad(&mut bus, RomSelection::Skip),
            Err(OneWireError::NoPresence)
        );
        // Nothing was clocked out: a reset is a reset.
        assert!(bus.written.is_empty());
    }

    #[test]
    fn writing_the_scratchpad_sends_three_registers_then_copies() {
        // `WriteScratchpad` sends 0x4E and the three register bytes, then `CopyScratchpad` sends
        // 0x48.
        let pad = scratchpad_from_raw(0x0B70, [0xAA, 0x55, 0x11, 0x22, 0x33]).with_resolution(11);
        let mut bus = FakeBus::new(true);
        write_scratchpad(&mut bus, RomSelection::Skip, &pad).unwrap_or(());
        assert_eq!(
            bus.written_bytes(),
            vec![
                CMD_SKIP_ROM,
                CMD_WRITE_SCRATCHPAD,
                0xAA,       // high alarm
                0x55,       // low alarm
                RES_11_BIT, // configuration
                CMD_SKIP_ROM,
                CMD_COPY_SCRATCHPAD,
            ]
        );
    }
}
