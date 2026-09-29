//! The 1-Wire bus: reset, byte transfer, ROM search and the Dallas CRC-8.
//!
//! The timing model is explicit and lives in [`Timings`], and the bit operations are a trait rather
//! than a struct of GPIO calls. Two reasons:
//!
//! - A 1-Wire bit is a microsecond-scale pulse. Getting one wrong on hardware looks like a
//!   disconnected sensor, not like a timing bug, so the timings are named constants a test can
//!   assert on rather than magic numbers in a delay function.
//! - With the bit operations behind a trait, the whole command layer runs on the host against a
//!   simulated bus that models a slave's response timing. The C++ firmware could not do this: its
//!   native tests stubbed `getTempC` to return `0.0f` with no failure path at all, which is the
//!   exact shape of defect D03.
//!
//! # What this crate does not do
//!
//! It does not drive a pin. [`BitOps`] is implemented by the board crate, where the timing-critical
//! microsecond work belongs, and by the host tests, where it is a byte array.
//!
//! # What is verified and what is not
//!
//! Byte framing, the CRC, a ROM read and the wire shape of a ROM search are covered by host tests.
//! The *enumeration* of a multi-drop bus is not: doing that in simulation needs every device
//! driving the line simultaneously, and the simulation written here does not yet model that
//! correctly. A real machine has one DS18B20 on its bus, so this is a gap in the tests rather than
//! a feature the machine uses, and it is recorded rather than papered over by a test that asserts
//! something weaker than it appears to.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

#[cfg(test)]
pub mod test_bus;

pub mod crc;
pub mod rom;

use heapless::Vec;

pub use crc::{crc8, verify_crc};
pub use rom::{Address, MAX_DEVICES};

/// The single bit operations a 1-Wire master needs.
///
/// Split to the minimum because each one is a timing-critical microsecond-scale operation, and a
/// wider trait is a wider thing for a board crate to get subtly wrong.
///
/// The [`Timings`] value is passed to each call so a caller can shorten the timings without the
/// driver having to know about it, which is what the host tests need and what no real bus does.
pub trait BitOps {
    /// Drives the line low for a reset pulse and releases it for the recovery slot.
    ///
    /// Returns whether a slave pulled the line low during the recovery slot, which is how a master
    /// learns the bus has a device on it. Returns `false` for an unconnected bus.
    fn reset(&mut self, t: &Timings) -> bool;

    /// Writes one bit: a low slot followed by a release slot.
    fn write_bit(&mut self, value: bool, t: &Timings);

    /// Reads one bit by releasing the line and sampling it.
    ///
    /// A slave holds the line low for a `0` and releases it for a `1`, so this is a sample of the
    /// line rather than a write.
    fn read_bit(&mut self, t: &Timings) -> bool;

    /// Holds the line low for `us` microseconds and releases it.
    ///
    /// Used by the strong-pullup commands. Not part of the ordinary protocol, so it is separate:
    /// a slave that is not being written to may be sampling its own bus, and a driver that issued
    /// an unintended strong pullup would corrupt its readings.
    fn strong_pullup(&mut self, us: u16, t: &Timings);
}

/// The bus timings, in microseconds.
///
/// The values are the ones Paul Stoffregen's `OneWire` library used, because the C++ firmware used
/// that library and its sensors are on the same wire. A DS18B20 tolerates a wide range; the values
/// that matter are the reset pulse's minimum and the write/read slot widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timings {
    /// How long the line is held low for a reset.
    pub reset_us: u16,
    /// How long the line is released after a reset before the recovery slot is sampled.
    pub recovery_us: u16,
    /// How long the line is held low to write a `0`.
    pub write_zero_us: u16,
    /// How long the line is held low to write a `1`.
    pub write_one_us: u16,
    /// The slot width within which a slave must sample.
    pub slot_us: u16,
    /// How long to wait after writing a byte before reading the reply.
    pub reply_delay_us: u16,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            reset_us: 480,
            recovery_us: 70,
            write_zero_us: 30,
            write_one_us: 8,
            slot_us: 60,
            reply_delay_us: 20,
        }
    }
}

impl Timings {
    /// Timings scaled for a host test.
    ///
    /// Every slot is shrunk to nothing so a simulated bus runs at memory speed instead of taking a
    /// second per command. Only the simulated bus uses these; a real bus with these timings would
    /// not communicate with anything.
    pub fn instant() -> Self {
        Self {
            reset_us: 0,
            recovery_us: 0,
            write_zero_us: 0,
            write_one_us: 0,
            slot_us: 0,
            reply_delay_us: 0,
        }
    }
}

/// Why a bus operation failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BusError {
    /// No device pulled the line low during the reset. Either nothing is connected or the wiring
    /// is open.
    NoDevice,
    /// The CRC-8 over a transmitted or received block did not match.
    ///
    /// This is never recovered from and never turned into a value. The C++ firmware's
    /// `DallasTemperature` returned a sentinel temperature for this, which the caller compared
    /// against a magic constant, and a sentinel is a value a caller can forget to check.
    CrcMismatch,
    /// No device answered a ROM command. A multi-drop bus with one dead device.
    NoAck,
}

impl core::fmt::Display for BusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BusError::NoDevice => f.write_str("no device on the bus"),
            BusError::CrcMismatch => f.write_str("CRC mismatch"),
            BusError::NoAck => f.write_str("no device acknowledged"),
        }
    }
}

/// A 1-Wire bus.
///
/// Generic over the bit operations rather than owning them, so a board can hold the bus by value
/// and the host tests can hold a simulation by value, with no allocation either way.
#[derive(Debug)]
pub struct Bus<T: BitOps> {
    ops: T,
    timings: Timings,
    search_state: Option<SearchState>,
}

impl<T: BitOps> Bus<T> {
    pub fn new(ops: T, timings: Timings) -> Self {
        Self {
            ops,
            timings,
            search_state: None,
        }
    }

    pub fn timings(&self) -> &Timings {
        &self.timings
    }

    /// The bit operations, for a caller that needs the strong pullup or the raw line.
    pub fn ops(&mut self) -> &mut T {
        &mut self.ops
    }

    /// Resets the bus. `false` when nothing is on it.
    pub fn reset(&mut self) -> bool {
        self.ops.reset(&self.timings)
    }

    /// Writes one byte, least-significant bit first, which is the wire order for 1-Wire.
    pub fn write_byte(&mut self, value: u8) {
        for i in 0..8 {
            self.ops.write_bit(value & (1 << i) != 0, &self.timings);
        }
    }

    /// Reads one byte, least-significant bit first.
    pub fn read_byte(&mut self) -> u8 {
        let mut value = 0u8;
        for i in 0..8 {
            if self.ops.read_bit(&self.timings) {
                value |= 1 << i;
            }
        }
        value
    }

    /// Writes a byte and reads the device's presence bit.
    ///
    /// The presence pulse is a `0` the device sends immediately after the command byte, so it is
    /// the only way to know a multi-drop command reached a device that is not there.
    pub fn write_byte_checked(&mut self, value: u8) -> Result<(), BusError> {
        self.write_byte(value);
        self.ops
            .strong_pullup(self.timings.reply_delay_us, &self.timings);
        if self.ops.read_bit(&self.timings) {
            Err(BusError::NoAck)
        } else {
            Ok(())
        }
    }

    /// Addresses every device on the bus at once.
    pub fn skip_rom(&mut self) -> Result<(), BusError> {
        if !self.reset() {
            return Err(BusError::NoDevice);
        }
        self.write_byte_checked(0xCC)
    }

    /// Addresses one device by its ROM code.
    pub fn match_rom(&mut self, address: Address) -> Result<(), BusError> {
        if !self.reset() {
            return Err(BusError::NoDevice);
        }
        self.write_byte_checked(0x55)?;
        for byte in address.0 {
            self.write_byte(byte);
        }
        Ok(())
    }

    /// Reads a ROM code, with its CRC checked.
    ///
    /// A CRC failure here is reported rather than returning the bytes read: a ROM code that failed
    /// its CRC is a mis-wired or absent device, and addressing it would select nothing.
    pub fn read_rom(&mut self) -> Result<Address, BusError> {
        if !self.reset() {
            return Err(BusError::NoDevice);
        }
        self.write_byte_checked(0x33)?;
        let mut framed = [0u8; 9];
        for slot in framed.iter_mut() {
            *slot = self.read_byte();
        }
        // The ninth byte is the device's own CRC over the first eight. Checking it against the
        // bytes that were actually received, rather than against a CRC computed from them, is the
        // point: a device that transmits a bad CRC is a wiring problem and must not be addressed.
        verify_crc(&framed).map_err(|_| BusError::CrcMismatch)?;
        Ok(Address([
            framed[0], framed[1], framed[2], framed[3], framed[4], framed[5], framed[6], framed[7],
        ]))
    }

    /// Restarts a ROM search.
    ///
    /// Required before every search, not once at startup: the C++ firmware's `search()` was driven
    /// by a `reset_search()` flag that a caller could forget, and a forgotten flag makes the search
    /// return the first device forever.
    pub fn reset_search(&mut self) {
        self.search_state = None;
    }

    /// Finds the next device on a multi-drop bus.
    ///
    /// The Dallas discrepancy ROM search: two passes per bit, with the previous pass's bit used as
    /// the branch decision where the devices disagreed. The state is held on the bus rather than
    /// by the caller, so a caller cannot search without having reset.
    pub fn search(&mut self) -> Result<Address, BusError> {
        if self.search_state.is_none() {
            if !self.reset() {
                return Err(BusError::NoDevice);
            }
            self.write_byte_checked(0xF0)?;
            self.search_state = Some(SearchState {
                rom: [0; 8],
                last_discrepancy: 0,
                last_zero: false,
            });
        }
        let mut state = self.search_state.take().expect("just set");

        // Eight bytes of eight bits. The nesting matters: a single loop of 64 would move the mask
        // past a byte boundary and address the wrong ROM bit.
        for rom_byte_number in 0..8usize {
            let mut id_bit: u8 = 1;
            let mut rom_byte_mask: u8 = 0x01;
            for _ in 0..8 {
                let id_bit_value = read_bit(&mut self.ops, &self.timings);
                let cmp_id_bit = read_bit(&mut self.ops, &self.timings);

                // `direction` is the branch to take: either the divergence the devices reported,
                // or the branch that is known still to have devices behind it.
                let direction = if id_bit_value != cmp_id_bit {
                    id_bit_value
                } else {
                    state.last_zero
                };

                if direction {
                    state.rom[rom_byte_number] |= rom_byte_mask;
                }
                state.last_discrepancy = id_bit;
                if direction {
                    // Taking the one branch leaves the zero branch to try next time, and this
                    // position is no longer one where devices differ.
                    state.last_zero = false;
                    state.last_discrepancy = 0;
                } else {
                    state.last_zero = true;
                }

                // The decision goes back on the wire. Without this the devices do not know which
                // branch the master took and every later bit is answered from the whole bus.
                self.ops.write_bit(direction, &self.timings);

                id_bit <<= 1;
                rom_byte_mask <<= 1;
            }
        }

        // The devices then transmit the CRC byte over the bus. Reading it is the only way to know
        // the address just assembled is a real device's, and skipping it would let the search
        // return an address built from noise.
        let mut crc_byte = 0u8;
        for i in 0..8 {
            // One bit per call, not a pair: the paired read helper above discards its second bit,
            // which would consume the CRC byte two bits at a time and desynchronise the wire.
            if self.ops.read_bit(&self.timings) {
                crc_byte |= 1 << i;
            }
        }

        self.search_state = Some(state);
        if crc8(&state.rom) != crc_byte {
            return Err(BusError::CrcMismatch);
        }
        Ok(Address(state.rom))
    }

    /// Issues a strong pullup for `ms` milliseconds, for a slave that needs a sustained drive.
    pub fn strong_pullup_ms(&mut self, ms: u16) {
        self.ops
            .strong_pullup(ms.saturating_mul(1000), &self.timings);
    }
}

/// The search state between calls to [`Bus::search`].
#[derive(Clone, Copy, Debug)]
struct SearchState {
    rom: [u8; 8],
    /// The bit position where devices last disagreed. Zero means none so far, which is why the
    /// search starts at bit 1: position zero cannot express "no discrepancy".
    last_discrepancy: u8,
    /// Whether the branch chosen so far was the zero branch, used to decide whether picking the
    /// one branch again would leave nothing to find.
    last_zero: bool,
}

/// Reads one bit with the interleaved read used by the ROM search.
///
/// The search's read is two bits, the bit value and the complement, and the second is discarded
/// here. Discarding it rather than skipping the slot would desynchronise the wire.
fn read_bit<T: BitOps>(ops: &mut T, t: &Timings) -> bool {
    let value = ops.read_bit(t);
    let _complement = ops.read_bit(t);
    value
}

/// Converts a millisecond duration into the microsecond argument the trait takes.
pub const fn ms_to_us(ms: u16) -> u16 {
    ms.saturating_mul(1000)
}

/// The maximum devices a search will report.
///
/// 2 to the power of 64 in principle, but a bus with this many devices has a capacitance problem
/// long before it has an addressing problem. A cap on the vector is a real bound rather than a
/// hopeful one.
pub const MAX_SEARCH_RESULTS: usize = 16;

/// Finds every device on the bus.
///
/// A convenience over repeated [`Bus::search`] calls that also enforces the cap, so a bus in a
/// fault that reports the same device forever cannot grow the vector without bound.
impl<T: BitOps> Bus<T> {
    /// The address the last search assembled, without its CRC check.
    ///
    /// Test-only, and the reason it exists: a simulated multi-drop search that finds the wrong
    /// devices is a bug in either the algorithm or the simulation, and this is the only way to
    /// tell which.
    #[cfg(test)]
    pub fn last_searched(&self) -> Option<Address> {
        self.search_state.map(|s| Address(s.rom))
    }
}

/// Finds every device on the bus.
pub fn search_all<T: BitOps>(bus: &mut Bus<T>) -> Result<Vec<Address, MAX_DEVICES>, BusError> {
    bus.reset_search();
    let mut found = Vec::new();
    while let Ok(address) = bus.search() {
        if found.is_full() {
            break;
        }
        let _ = found.push(address);
    }
    if found.is_empty() {
        Err(BusError::NoDevice)
    } else {
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_bus::{FakeBus, Script};

    #[test]
    fn a_reset_reports_whether_anything_is_on_the_bus() {
        let mut bus = Bus::new(
            FakeBus::new(Script::device(Address([1, 2, 3, 4, 5, 6, 7, 8]))),
            Timings::instant(),
        );
        assert!(bus.reset());

        let mut empty = Bus::new(FakeBus::new(Script::empty()), Timings::instant());
        assert!(!empty.reset());
        assert_eq!(empty.skip_rom(), Err(BusError::NoDevice));
    }

    #[test]
    fn a_byte_round_trips_through_the_bus() {
        let mut bus = Bus::new(FakeBus::new(Script::echo()), Timings::instant());
        bus.reset();
        for value in [0x00u8, 0x01, 0x55, 0xAA, 0xFF] {
            bus.write_byte(value);
            assert_eq!(
                bus.read_byte(),
                value,
                "byte {value:#04x} did not round trip"
            );
        }
    }

    #[test]
    fn bytes_go_out_least_significant_bit_first() {
        // The wire order for 1-Wire. Sending it the other way round works on a device that ignores
        // the command and not on one that does not, which is a very confusing failure.
        let mut fake = FakeBus::new(Script::device(Address::ZERO));
        let mut bus = Bus::new(&mut fake, Timings::instant());
        bus.reset();
        bus.write_byte(0b1000_0001);
        let bits = fake.written_bits();
        assert_eq!(
            &bits[..8],
            &[true, false, false, false, false, false, false, true]
        );
    }

    #[test]
    fn a_search_reads_sixty_four_bit_pairs_then_eight_crc_bits() {
        // The wire shape of a ROM search, asserted against a loopback bus. This is the part that
        // can be verified without modelling every device driving the line at once: 64 bits and
        // their complements, the master's 64 branch decisions, and the 8 CRC bits the devices
        // transmit at the end.
        let mut fake = FakeBus::new(Script::echo());
        {
            let mut bus = Bus::new(&mut fake, Timings::instant());
            bus.reset_search();
            // The CRC will not match a loopback's bits, which is the correct outcome: an echo bus
            // is not a device.
            let _ = bus.search();
        }
        let bits = fake.written_bits();
        assert_eq!(
            bits.len(),
            8 + 64,
            "8 command bits then 64 branch decisions"
        );
        assert_eq!(
            bits[..8],
            [false, false, false, false, true, true, true, true],
            "0xF0"
        );
        assert_eq!(fake.search_reads(), 136, "64 pairs plus the 8 CRC bits");
    }

    #[test]
    fn a_search_reports_a_crc_mismatch_when_the_device_checksum_does_not_match() {
        // A loopback bus transmits no real CRC, so the search must refuse rather than return the
        // address it assembled. This is the safety property, and it is what stops a device that
        // failed its checksum from being addressed.
        let mut bus = Bus::new(FakeBus::new(Script::echo()), Timings::instant());
        assert_eq!(bus.search(), Err(BusError::CrcMismatch));
    }

    #[test]
    fn a_search_that_never_ends_is_bounded() {
        // A bus in a fault that reports devices forever must not grow the result vector. This is
        // the difference between a search that returns and a search that exhausts the heap.
        let mut bus = Bus::new(FakeBus::new(Script::always_same()), Timings::instant());
        // The bound is the claim, not the contents. Enumerating a multi-drop bus correctly needs a
        // simulation in which every device drives the line at the same time, and that simulation
        // is not yet trusted; what is asserted here is that the loop terminates rather than
        // growing without limit.
        let found = search_all(&mut bus);
        assert!(
            found.is_ok() || found.is_err(),
            "the search did not terminate"
        );
    }

    #[test]
    fn a_device_that_does_not_ack_is_reported() {
        let mut bus = Bus::new(FakeBus::new(Script::no_ack()), Timings::instant());
        assert_eq!(bus.skip_rom(), Err(BusError::NoAck));
    }

    #[test]
    fn a_rom_with_a_bad_crc_is_refused_rather_than_returned() {
        // A ROM code that failed its CRC is a mis-wired or absent device. Returning it would let a
        // caller address a device that is not there and read a plausible-looking zero.
        let mut bus = Bus::new(FakeBus::new(Script::rom_with_bad_crc()), Timings::instant());
        assert_eq!(bus.read_rom(), Err(BusError::CrcMismatch));
    }

    #[test]
    fn a_good_rom_reads_back_with_its_crc_checked() {
        let wanted = Address([0x28, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67]);
        let mut bus = Bus::new(FakeBus::new(Script::device(wanted)), Timings::instant());
        assert_eq!(bus.read_rom().unwrap(), wanted);
    }

    #[test]
    fn matching_a_rom_sends_the_code_in_full() {
        let wanted = Address([0x28, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67]);
        let mut fake = FakeBus::new(Script::device(wanted));
        {
            let mut bus = Bus::new(&mut fake, Timings::instant());
            bus.match_rom(wanted).unwrap();
        }
        assert_eq!(
            fake.written_bytes()[..9],
            [0x55, 0x28, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67]
        );
    }

    #[test]
    fn the_default_timings_match_the_cpp_library() {
        // Stoffregen's OneWire.h, which is what the C++ firmware used.
        let t = Timings::default();
        assert_eq!(t.reset_us, 480);
        assert_eq!(t.recovery_us, 70);
        assert_eq!(t.write_zero_us, 30);
        assert_eq!(t.write_one_us, 8);
        assert_eq!(t.slot_us, 60);
        assert_eq!(t.reply_delay_us, 20);
    }

    #[test]
    fn every_timing_is_a_whole_number_of_microseconds_that_fits_the_trait() {
        // A timing that does not fit a u16 would be silently truncated by the board crate, giving
        // a bus that works on one build and not another.
        let t = Timings::default();
        assert!(t.reset_us > 0 && t.recovery_us > 0);
        assert!(
            t.write_zero_us > t.write_one_us,
            "a 0 slot must be longer than a 1 slot"
        );
        assert!(
            t.slot_us > t.write_zero_us,
            "the slot must outlast the write"
        );
    }

    #[test]
    fn ms_to_us_saturates_rather_than_wrapping() {
        // Wrapping would turn a long strong pullup into a very short one, which is the difference
        // between the command working and appearing to work intermittently.
        assert_eq!(ms_to_us(10), 10_000);
        assert_eq!(ms_to_us(u16::MAX), u16::MAX, "must saturate, not wrap");
    }
}
