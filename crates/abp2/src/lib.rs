//! Honeywell ABP2 digital pressure sensor (I²C, 24-bit). The 0–10 bar part CleverCoffee
//! uses by default. Measurement is split into [`Abp2::start`] and [`Abp2::read`] so the
//! caller can wait without blocking.

#![no_std]

use embedded_hal::i2c::I2c;

pub const DEFAULT_ADDRESS: u8 = 0x28;
/// Conversion time; the C++ waited 10 ms.
pub const CONVERSION_MS: u32 = 10;

const OUTPUT_MIN: f32 = 1_677_722.0; // 10 % of 2^24
const OUTPUT_MAX: f32 = 15_099_494.0; // 90 % of 2^24
const STATUS_BUSY: u8 = 0x20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub bar: f32,
    pub celsius: f32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error<E> {
    I2c(E),
    /// The conversion was not finished yet.
    Busy,
}

#[derive(Debug)]
pub struct Abp2<I> {
    i2c: I,
    address: u8,
    p_min: f32,
    p_max: f32,
}

impl<I: I2c> Abp2<I> {
    /// A sensor with the given range in bar.
    pub fn new(i2c: I, address: u8, p_min: f32, p_max: f32) -> Self {
        Self {
            i2c,
            address,
            p_min,
            p_max,
        }
    }

    /// Triggers a conversion. Read it after [`CONVERSION_MS`].
    pub fn start(&mut self) -> Result<(), Error<I::Error>> {
        self.i2c
            .write(self.address, &[0xaa, 0x00, 0x00])
            .map_err(Error::I2c)
    }

    pub fn read(&mut self) -> Result<Reading, Error<I::Error>> {
        let mut b = [0u8; 7];
        self.i2c.read(self.address, &mut b).map_err(Error::I2c)?;
        if b[0] & STATUS_BUSY != 0 {
            return Err(Error::Busy);
        }
        Ok(decode(&b, self.p_min, self.p_max))
    }

    pub fn release(self) -> I {
        self.i2c
    }
}

/// Decodes the 7-byte answer (status, pressure ×3, temperature ×3).
pub fn decode(b: &[u8; 7], p_min: f32, p_max: f32) -> Reading {
    let counts = |h: u8, m: u8, l: u8| ((u32::from(h) << 16) | (u32::from(m) << 8) | u32::from(l)) as f32;
    let p = counts(b[1], b[2], b[3]);
    let t = counts(b[4], b[5], b[6]);
    Reading {
        bar: (p - OUTPUT_MIN) * (p_max - p_min) / (OUTPUT_MAX - OUTPUT_MIN) + p_min,
        celsius: t * 270.0 / 16_777_215.0 - 40.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};

    struct Fake {
        answer: [u8; 7],
        wrote: [u8; 3],
    }

    impl ErrorType for Fake {
        type Error = ErrorKind;
    }

    impl I2c for Fake {
        fn transaction(&mut self, address: u8, ops: &mut [Operation<'_>]) -> Result<(), ErrorKind> {
            assert_eq!(address, DEFAULT_ADDRESS);
            for op in ops {
                match op {
                    Operation::Write(w) => self.wrote.copy_from_slice(w),
                    Operation::Read(r) => r.copy_from_slice(&self.answer),
                }
            }
            Ok(())
        }
    }

    #[test]
    fn decodes_range_ends() {
        let zero = [0x40, 0x19, 0x99, 0x9a, 0, 0, 0];
        let full = [0x40, 0xe6, 0x66, 0x66, 0, 0, 0];
        assert!((decode(&zero, 0.0, 10.0).bar).abs() < 1e-3);
        assert!((decode(&full, 0.0, 10.0).bar - 10.0).abs() < 1e-3);
    }

    #[test]
    fn talks_the_protocol() {
        let mut s = Abp2::new(
            Fake {
                answer: [0x40, 0x80, 0, 0, 0x80, 0, 0],
                wrote: [0; 3],
            },
            DEFAULT_ADDRESS,
            0.0,
            10.0,
        );
        s.start().unwrap();
        let r = s.read().unwrap();
        assert!((r.bar - 5.0).abs() < 0.01);
        assert!((r.celsius - 95.0).abs() < 0.01);
        assert_eq!(s.release().wrote, [0xaa, 0, 0]);
    }

    #[test]
    fn busy_is_reported() {
        let mut s = Abp2::new(
            Fake {
                answer: [0x60, 0, 0, 0, 0, 0, 0],
                wrote: [0; 3],
            },
            DEFAULT_ADDRESS,
            0.0,
            10.0,
        );
        assert_eq!(s.read(), Err(Error::Busy));
    }
}
