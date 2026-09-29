//! A simulated 1-Wire bus, for host tests.
//!
//! It models a slave's *responses*, not the electrical timing: what a real device does with the
//! bits it is sent, including the presence pulse, the ROM search's paired bits and a corrupted
//! byte. Timing itself is a board concern and cannot be simulated meaningfully; what this catches
//! is the protocol, which is where the C++ firmware's bugs were.
//!
//! The timings are set to [`crate::Timings::instant`] by the tests, so a simulated bus runs at
//! memory speed. A real bus with those timings would not talk to anything, which is why this type
//! is only ever constructed in a test.

use heapless::Vec;

use crate::rom::Address;
use crate::{BitOps, Timings};

/// What the simulated devices are set up to do.
#[derive(Debug)]
pub enum Script {
    /// One device, answering reads with its own ROM.
    Device(Address),
    /// Several devices, answering a ROM search.
    Multi(Vec<Address, 16>),
    /// A bus carrying many copies of the same ROM, which is what a bus in a fault looks like:
    /// every branch is populated, so a search never terminates on its own. Used to prove a search
    /// is bounded rather than assuming a caller stops counting.
    AlwaysSame,
    /// A bus with devices that never pull the line low for a presence pulse.
    NoAck,
    /// A device that transmits a ROM with a bad CRC.
    RomWithBadCrc,
    /// No devices at all.
    Empty,
    /// Every bit written is read back unchanged, for testing byte framing rather than a protocol.
    Echo,
    /// One device that answers a scratchpad read with a fixed nine bytes, for testing a device
    /// layer's decoding rather than the bus.
    Scratchpad([u8; 9]),
}

impl Script {
    pub fn device(address: Address) -> Self {
        Script::Device(address)
    }

    pub fn multi(addresses: &[Address]) -> Self {
        let mut v = Vec::new();
        for a in addresses {
            let _ = v.push(*a);
        }
        Script::Multi(v)
    }

    pub fn empty() -> Self {
        Script::Empty
    }

    pub fn no_ack() -> Self {
        Script::NoAck
    }

    pub fn rom_with_bad_crc() -> Self {
        Script::RomWithBadCrc
    }

    pub fn always_same() -> Self {
        Script::AlwaysSame
    }

    pub fn echo() -> Self {
        Script::Echo
    }

    /// A device that returns `scratchpad` when asked to read one.
    pub fn scratchpad(scratchpad: &[u8; 9]) -> Self {
        Script::Scratchpad(*scratchpad)
    }
}

/// A simulated bus.
///
/// Deliberately not thread-safe and not `Sync`: it belongs to one test thread, and pretending
/// otherwise would let a test appear to pass while two threads shared its state.
#[derive(Debug)]
pub struct FakeBus {
    script: Script,
    /// The bits the master wrote, in order.
    /// A test's bus sees a few hundred bits per command and a few thousand across a search, so a
    /// bound of 2048 is generous and keeps the fake off the heap in spirit as well as in size.
    written: Vec<bool, 2048>,
    /// A pending read: the bits a device will return, consumed from the front.
    to_read: Vec<bool, 2048>,
    /// Whether a reset should see a device.
    device_present: bool,
    /// Whether the next presence pulse should be a `0` (a device answered) or a `1` (none did).
    ack_next: bool,
    /// Whether a presence pulse is owed. Kept separate from the data queue so reading the pulse
    /// does not consume a byte the device is about to transmit, which is the difference between a
    /// working ROM read and one that is off by a byte.
    presence_pending: bool,
    /// Set while a ROM search is in progress: the branch the master has taken at each of the 64 bit
    /// positions. A real multi-drop bus answers each bit pair from every device at once, so a
    /// faithful simulation has to know the master's path to answer correctly.
    search_path: Option<Vec<bool, 64>>,
    /// How many bits the master has read so far. A search reads two bits per ROM bit, so the ROM
    /// bit position is this divided by two.
    search_reads: usize,
    /// How far into [`Self::written`] a loopback read has got.
    echo_reads: usize,
}

impl FakeBus {
    /// How many bits the master has read during a search, pairs and CRC bits together.
    pub fn search_reads(&self) -> usize {
        self.search_reads
    }
}

impl FakeBus {
    pub fn new(script: Script) -> Self {
        // `NoAck` still has a device on the bus: it answers the reset and then never answers a
        // ROM command, which is a different fault from a bus with nothing on it.
        let device_present = !matches!(script, Script::Empty);
        Self {
            script,
            written: Vec::new(),
            to_read: Vec::new(),
            device_present,
            ack_next: true,
            presence_pending: false,
            search_path: None,
            search_reads: 0,
            echo_reads: 0,
        }
    }

    /// The bits the master wrote, oldest first.
    pub fn written_bits(&self) -> &[bool] {
        &self.written
    }

    /// The bits the master wrote, grouped into bytes.
    pub fn written_bytes(&self) -> Vec<u8, 512> {
        let mut out = Vec::new();
        for b in self.written.chunks(8).map(|bits| {
            bits.iter()
                .enumerate()
                .filter(|(_, b)| **b)
                .fold(0u8, |acc, (i, _)| acc | (1 << i))
        }) {
            let _ = out.push(b);
        }
        out
    }

    /// Queues the bits a device will return, least-significant bit first per byte.
    fn queue(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            for i in 0..8 {
                let _ = self.to_read.push(byte & (1 << i) != 0);
            }
        }
    }

    /// The ROM the current script's device answers with.
    fn rom(&self) -> Address {
        match &self.script {
            Script::Device(a) => *a,
            Script::Multi(v) => v.first().copied().unwrap_or(Address::ZERO),
            Script::AlwaysSame => Address([0x28, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]),
            Script::Echo | Script::Scratchpad(_) => Address::ZERO,
            Script::RomWithBadCrc => Address([0x28, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77]),
            Script::Empty | Script::NoAck => Address::ZERO,
        }
    }
}

impl FakeBus {
    /// The devices a search walks, as a plain vector.
    ///
    /// `AlwaysSame` expands to many copies of one ROM so every branch of the search tree is
    /// populated, which is what a search that cannot terminate looks like.
    fn search_devices(&self) -> Vec<Address, 32> {
        let mut out = Vec::new();
        match &self.script {
            Script::Multi(d) => {
                for a in d.iter() {
                    let _ = out.push(*a);
                }
            }
            Script::AlwaysSame => {
                for _ in 0..32 {
                    let _ = out.push(Address([0x28, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]));
                }
            }
            _ => {}
        }
        out
    }

    /// Answers one bit pair of a ROM search, the way a real multi-drop bus does.
    ///
    /// Every device whose ROM agrees with the path the master has walked so far contributes its
    /// bit at this position; the answer is the AND over those devices, and the complement is the
    /// AND of the inverted bits. A branch with no devices behind it answers `1`, which is how a
    /// master learns it has found the last device on the bus.
    fn search_pair(&mut self, position: usize, is_value: bool) -> bool {
        let devices = self.search_devices();
        let path = self.search_path.clone().unwrap_or_default();

        let mut bit = true;
        let mut complement = true;
        for device in devices.iter() {
            // Only devices consistent with the branch decisions so far are still on the bus.
            let consistent = path
                .iter()
                .enumerate()
                .take(position)
                .all(|(i, chosen)| ((device.0[i / 8] >> (i % 8)) & 1 == 1) == *chosen);
            if !consistent {
                continue;
            }
            let rom_bit = (device.0[position / 8] >> (position % 8)) & 1 == 1;
            bit = bit && rom_bit;
            complement = complement && !rom_bit;
        }
        // A branch with nothing behind it reads as all ones, which is how a master learns it has
        // found the last device on the bus.
        if is_value {
            bit
        } else {
            complement
        }
    }

    /// The CRC byte the devices transmit after the 64 ROM bits.
    ///
    /// The last device on the path computed the whole way down the tree transmits it, because by
    /// the time the master finishes the 64 bits only that one is still driving the bus.
    fn search_crc(&self) -> u8 {
        let devices = self.search_devices();
        if devices.is_empty() {
            return 0;
        }
        let Some(path) = self.search_path.as_ref() else {
            return 0;
        };
        let path = path.clone();
        let last = devices.iter().rev().find(|device| {
            path.iter()
                .enumerate()
                .all(|(i, chosen)| ((device.0[i / 8] >> (i % 8)) & 1 == 1) == *chosen)
        });
        match last {
            Some(device) => crate::crc::crc8(&device.0),
            None => 0,
        }
    }
}

/// Lets a caller hand the bus's bit operations out as a borrow, so a test can build a `Bus` over a
/// `&mut FakeBus` and then inspect what was written. The board crate has the same need: it owns the
/// pins somewhere else and drives them through a reference.
impl<T: BitOps + ?Sized> BitOps for &mut T {
    fn reset(&mut self, t: &Timings) -> bool {
        (**self).reset(t)
    }
    fn write_bit(&mut self, value: bool, t: &Timings) {
        (**self).write_bit(value, t)
    }
    fn read_bit(&mut self, t: &Timings) -> bool {
        (**self).read_bit(t)
    }
    fn strong_pullup(&mut self, us: u16, t: &Timings) {
        (**self).strong_pullup(us, t)
    }
}

impl BitOps for FakeBus {
    fn reset(&mut self, _t: &Timings) -> bool {
        self.written.clear();
        self.search_path = None;
        self.search_reads = 0;
        self.echo_reads = 0;
        self.device_present
    }

    fn write_bit(&mut self, value: bool, _t: &Timings) {
        // During a search, each write after the command byte is the master's decision for one ROM
        // bit position, taken after it has read that position's pair. Recording it here is what
        // lets `search_pair` know which devices are still listening.
        if let Some(path) = self.search_path.as_mut() {
            if self.written.len() >= 8 {
                let _ = path.push(value);
            }
        }
        let _ = self.written.push(value);
    }

    fn read_bit(&mut self, _t: &Timings) -> bool {
        if let Script::Scratchpad(_) = self.script {
            if !self.presence_pending && self.to_read.is_empty() {
                return true;
            }
        }
        // The presence pulse comes first and is not part of the device's data.
        if self.presence_pending {
            self.presence_pending = false;
            return !self.ack_next;
        }
        // In a ROM search the master reads a bit and its complement for each of the 64 ROM bits,
        // then reads the eight CRC bits the devices transmit.
        if self.search_path.is_some() {
            if self.search_reads < 128 {
                let read = self.search_reads;
                let bit = self.search_pair(read / 2, read.is_multiple_of(2));
                self.search_reads += 1;
                return bit;
            }
            if self.search_reads < 136 {
                let position = self.search_reads - 128;
                let crc = self.search_crc();
                self.search_reads += 1;
                return (crc >> position) & 1 == 1;
            }
        }
        if let Some(bit) = pop_front(&mut self.to_read) {
            return bit;
        }
        if let Script::Echo = self.script {
            // The bits that were written, oldest first: what a loopback looks like. Used to test
            // byte framing, where the point is the bit order rather than the protocol.
            let bit = self.written.get(self.echo_reads).copied().unwrap_or(false);
            self.echo_reads += 1;
            return bit;
        }
        // Nothing queued: a device that answers a read it was not asked for would make a driver's
        // protocol errors invisible, so the bus reads high, which is a `1` and a failure for most
        // commands.
        true
    }

    fn strong_pullup(&mut self, _us: u16, _t: &Timings) {
        self.ack_next = !matches!(self.script, Script::NoAck);
        self.presence_pending = true;
        if !self.ack_next {
            // A device that does not answer has nothing more to say.
            return;
        }
        // A device answers a ROM command by transmitting. Queueing it here is what lets
        // `read_rom` and `search` see real bytes.
        match &self.script {
            Script::RomWithBadCrc => {
                let mut rom = self.rom().0;
                let good = crate::crc::crc8(&rom);
                rom[7] = good.wrapping_add(1);
                self.queue(&rom);
                self.queue(&[good]);
            }
            Script::Multi(_) | Script::AlwaysSame | Script::Echo => {
                // A ROM search is answered bit by bit, not device by device: every device drives
                // its own bit and its complement simultaneously, and the master resolves the pair.
                // The branch bits are recorded in `write_bit` and answered in `read_bit`.
                self.search_path = Some(Vec::new());
                self.search_reads = 0;
            }
            Script::Scratchpad(sp) => {
                // A device layer asks for a scratchpad read and gets these nine bytes.
                let bytes = *sp;
                self.queue(&bytes);
            }
            Script::Device(a) => {
                let rom = a.0;
                let crc = crate::crc::crc8(&rom);
                self.queue(&rom);
                self.queue(&[crc]);
            }
            Script::Empty => {
                let rom = self.rom().0;
                let crc = crate::crc::crc8(&rom);
                self.queue(&rom);
                self.queue(&[crc]);
            }
            Script::NoAck => {}
        }
    }
}

/// Removes the oldest bit.
///
/// `heapless::Vec` has no `pop_front`, and a ring buffer here would need its own tests. The queue
/// is at most a few hundred bits in any test, so a linear remove is cheap and obviously correct.
fn pop_front(v: &mut Vec<bool, 2048>) -> Option<bool> {
    if v.is_empty() {
        None
    } else {
        Some(v.remove(0))
    }
}
