//! Bit-banged 1-Wire on an open-drain pin, and the DS18B20 temperature sensor.
//!
//! Every time slot runs inside [`Slot::with`] (a critical section on the device):
//! slots are at most ~70 µs, so interrupts are only held off briefly, unlike locking a
//! whole multi-millisecond transaction.

#![no_std]

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};

pub trait Slot {
    fn with<R>(&mut self, f: impl FnOnce() -> R) -> R;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Unprotected;

impl Slot for Unprotected {
    fn with<R>(&mut self, f: impl FnOnce() -> R) -> R {
        f()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Pin,
    /// No device answered the reset.
    NoPresence,
    /// The bus stays low (short to ground).
    BusLow,
    Crc,
}

/// The bus. `P` is an open-drain pin: `set_low` pulls, `set_high` releases.
#[derive(Debug)]
pub struct OneWire<P> {
    pin: P,
}

impl<P: OutputPin + InputPin> OneWire<P> {
    pub fn new(mut pin: P) -> Result<Self, Error> {
        pin.set_high().map_err(|_| Error::Pin)?;
        Ok(Self { pin })
    }

    /// Reset pulse; `Ok` if a device answered with a presence pulse.
    pub fn reset(&mut self, d: &mut impl DelayNs, s: &mut impl Slot) -> Result<(), Error> {
        if self.pin.is_low().map_err(|_| Error::Pin)? {
            return Err(Error::BusLow);
        }
        self.pin.set_low().map_err(|_| Error::Pin)?;
        d.delay_us(480);
        let present = s.with(|| -> Result<bool, Error> {
            self.pin.set_high().map_err(|_| Error::Pin)?;
            d.delay_us(70);
            self.pin.is_low().map_err(|_| Error::Pin)
        })?;
        d.delay_us(410);
        if present { Ok(()) } else { Err(Error::NoPresence) }
    }

    fn write_bit(&mut self, bit: bool, d: &mut impl DelayNs, s: &mut impl Slot) -> Result<(), Error> {
        s.with(|| -> Result<(), Error> {
            self.pin.set_low().map_err(|_| Error::Pin)?;
            d.delay_us(if bit { 6 } else { 60 });
            self.pin.set_high().map_err(|_| Error::Pin)
        })?;
        d.delay_us(if bit { 64 } else { 10 });
        Ok(())
    }

    fn read_bit(&mut self, d: &mut impl DelayNs, s: &mut impl Slot) -> Result<bool, Error> {
        let bit = s.with(|| -> Result<bool, Error> {
            self.pin.set_low().map_err(|_| Error::Pin)?;
            d.delay_us(6);
            self.pin.set_high().map_err(|_| Error::Pin)?;
            d.delay_us(9);
            self.pin.is_high().map_err(|_| Error::Pin)
        })?;
        d.delay_us(55);
        Ok(bit)
    }

    pub fn write_byte(&mut self, b: u8, d: &mut impl DelayNs, s: &mut impl Slot) -> Result<(), Error> {
        for i in 0..8 {
            self.write_bit(b >> i & 1 == 1, d, s)?;
        }
        Ok(())
    }

    pub fn read_byte(&mut self, d: &mut impl DelayNs, s: &mut impl Slot) -> Result<u8, Error> {
        let mut b = 0;
        for i in 0..8 {
            b |= u8::from(self.read_bit(d, s)?) << i;
        }
        Ok(b)
    }
}

/// Dallas/Maxim CRC-8 (polynomial x^8 + x^5 + x^4 + 1).
pub fn crc8(data: &[u8]) -> u8 {
    data.iter().fold(0, |mut crc, &byte| {
        let mut b = byte;
        for _ in 0..8 {
            let mix = (crc ^ b) & 1;
            crc >>= 1;
            if mix != 0 {
                crc ^= 0x8c;
            }
            b >>= 1;
        }
        crc
    })
}

const SKIP_ROM: u8 = 0xcc;
const CONVERT_T: u8 = 0x44;
const READ_SCRATCHPAD: u8 = 0xbe;
const WRITE_SCRATCHPAD: u8 = 0x4e;

/// Conversion time at 11-bit resolution (the C++ setting).
pub const CONVERSION_MS_11BIT: u32 = 375;

/// A single DS18B20 on the bus (skip-ROM addressing).
#[derive(Debug)]
pub struct Ds18b20<P> {
    bus: OneWire<P>,
}

impl<P: OutputPin + InputPin> Ds18b20<P> {
    pub fn new(bus: OneWire<P>) -> Self {
        Self { bus }
    }

    /// Sets 11-bit resolution (0.125 °C, 375 ms).
    pub fn configure(&mut self, d: &mut impl DelayNs, s: &mut impl Slot) -> Result<(), Error> {
        self.bus.reset(d, s)?;
        for b in [SKIP_ROM, WRITE_SCRATCHPAD, 0x4b, 0x46, 0x5f] {
            self.bus.write_byte(b, d, s)?;
        }
        Ok(())
    }

    /// Starts a conversion; read it after [`CONVERSION_MS_11BIT`].
    pub fn start(&mut self, d: &mut impl DelayNs, s: &mut impl Slot) -> Result<(), Error> {
        self.bus.reset(d, s)?;
        self.bus.write_byte(SKIP_ROM, d, s)?;
        self.bus.write_byte(CONVERT_T, d, s)
    }

    pub fn read_celsius(&mut self, d: &mut impl DelayNs, s: &mut impl Slot) -> Result<f32, Error> {
        self.bus.reset(d, s)?;
        self.bus.write_byte(SKIP_ROM, d, s)?;
        self.bus.write_byte(READ_SCRATCHPAD, d, s)?;
        let mut sp = [0u8; 9];
        for b in &mut sp {
            *b = self.bus.read_byte(d, s)?;
        }
        decode_scratchpad(&sp)
    }
}

/// Temperature from a scratchpad, CRC checked. All-ones (no device pulling) fails
/// the CRC.
pub fn decode_scratchpad(sp: &[u8; 9]) -> Result<f32, Error> {
    if crc8(&sp[..8]) != sp[8] || sp.iter().all(|&b| b == 0xff) {
        return Err(Error::Crc);
    }
    Ok(f32::from(i16::from_le_bytes([sp[0], sp[1]])) / 16.0)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use core::cell::RefCell;
    use core::convert::Infallible;
    use embedded_hal::digital::ErrorType;
    use std::rc::Rc;
    use std::vec::Vec;

    /// A DS18B20 model driven by pin edges and a shared clock.
    struct Model {
        now: u64,
        master_low_since: Option<u64>,
        presence_until: Option<(u64, u64)>,
        rx_bits: Vec<bool>,
        tx: Vec<bool>,
        tx_low_until: Option<u64>,
        scratchpad: [u8; 9],
        present: bool,
    }

    impl Model {
        fn released(&mut self) {
            let Some(since) = self.master_low_since.take() else {
                return;
            };
            let low = self.now - since;
            if low >= 480 {
                if self.present {
                    self.presence_until = Some((self.now + 15, self.now + 120));
                }
                self.rx_bits.clear();
                self.tx.clear();
            } else if !self.tx.is_empty() && low < 15 {
                // read slot: the bit was decided at the falling edge
            } else {
                self.rx_bits.push(low < 15);
                if self.rx_bits.len().is_multiple_of(8) {
                    let byte = self.rx_bits[self.rx_bits.len() - 8..]
                        .iter()
                        .enumerate()
                        .fold(0u8, |a, (i, &b)| a | (u8::from(b) << i));
                    if byte == READ_SCRATCHPAD {
                        for b in self.scratchpad {
                            for i in 0..8 {
                                self.tx.push(b >> i & 1 == 1);
                            }
                        }
                        self.tx.reverse();
                    }
                }
            }
        }
        fn pulled_low(&mut self) {
            self.master_low_since = Some(self.now);
            if let Some(bit) = self.tx.pop()
                && !bit
            {
                self.tx_low_until = Some(self.now + 30);
            }
        }
        fn line_low(&self) -> bool {
            self.master_low_since.is_some()
                || self
                    .presence_until
                    .is_some_and(|(a, b)| (a..b).contains(&self.now))
                || self.tx_low_until.is_some_and(|t| self.now < t)
        }
    }

    #[derive(Clone)]
    struct Sim(Rc<RefCell<Model>>);
    impl ErrorType for Sim {
        type Error = Infallible;
    }
    impl OutputPin for Sim {
        fn set_low(&mut self) -> Result<(), Infallible> {
            self.0.borrow_mut().pulled_low();
            Ok(())
        }
        fn set_high(&mut self) -> Result<(), Infallible> {
            self.0.borrow_mut().released();
            Ok(())
        }
    }
    impl InputPin for Sim {
        fn is_high(&mut self) -> Result<bool, Infallible> {
            Ok(!self.0.borrow().line_low())
        }
        fn is_low(&mut self) -> Result<bool, Infallible> {
            Ok(self.0.borrow().line_low())
        }
    }
    struct Clock(Sim);
    impl DelayNs for Clock {
        fn delay_ns(&mut self, ns: u32) {
            self.0.0.borrow_mut().now += u64::from(ns.div_ceil(1000));
        }
    }

    fn scratchpad(raw: i16) -> [u8; 9] {
        let [l, h] = raw.to_le_bytes();
        let mut sp = [l, h, 0x4b, 0x46, 0x5f, 0xff, 0x0c, 0x10, 0];
        sp[8] = crc8(&sp[..8]);
        sp
    }

    fn sim(present: bool, raw: i16) -> Sim {
        Sim(Rc::new(RefCell::new(Model {
            now: 0,
            master_low_since: None,
            presence_until: None,
            rx_bits: Vec::new(),
            tx: Vec::new(),
            tx_low_until: None,
            scratchpad: scratchpad(raw),
            present,
        })))
    }

    #[test]
    fn reads_a_temperature_over_the_simulated_bus() {
        let s = sim(true, 1500); // 93.75 °C
        let mut clock = Clock(s.clone());
        let mut dev = Ds18b20::new(OneWire::new(s).unwrap());
        dev.start(&mut clock, &mut Unprotected).unwrap();
        assert_eq!(dev.read_celsius(&mut clock, &mut Unprotected), Ok(93.75));
    }

    #[test]
    fn missing_sensor_is_reported() {
        let s = sim(false, 0);
        let mut clock = Clock(s.clone());
        let mut dev = Ds18b20::new(OneWire::new(s).unwrap());
        assert_eq!(dev.start(&mut clock, &mut Unprotected), Err(Error::NoPresence));
    }

    #[test]
    fn crc_and_decoding() {
        assert_eq!(decode_scratchpad(&scratchpad(-160)), Ok(-10.0));
        let mut bad = scratchpad(400);
        bad[0] ^= 1;
        assert_eq!(decode_scratchpad(&bad), Err(Error::Crc));
        assert_eq!(decode_scratchpad(&[0xff; 9]), Err(Error::Crc));
        // Maxim application note example ROM: 02 1C B8 01 00 00 00 -> CRC A2
        assert_eq!(crc8(&[0x02, 0x1c, 0xb8, 0x01, 0, 0, 0]), 0xa2);
    }
}
