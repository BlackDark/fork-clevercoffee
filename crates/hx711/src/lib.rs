//! HX711 24-bit load-cell ADC, bit-banged. Several chips may share the clock line
//! (CleverCoffee's dual-cell scale): they are read in the same 24 clock pulses.
//!
//! The clock must not stay high for more than 60 µs or the chip powers down, so each
//! pulse runs inside [`Pulse::with`], which the firmware implements as a critical
//! section.

#![no_std]

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};

/// Wraps one clock pulse so it cannot be stretched by interrupts.
pub trait Pulse {
    fn with<R>(&mut self, f: impl FnOnce() -> R) -> R;
}

/// No protection (tests, or a context that already runs with interrupts off).
#[derive(Debug, Default, Clone, Copy)]
pub struct Unprotected;

impl Pulse for Unprotected {
    fn with<R>(&mut self, f: impl FnOnce() -> R) -> R {
        f()
    }
}

/// Gain and channel, selected by the pulses after the 24 data bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gain {
    A128 = 1,
    B32 = 2,
    A64 = 3,
}

#[derive(Debug)]
pub struct Hx711<CLK, DATA, const N: usize> {
    clk: CLK,
    data: [DATA; N],
    gain: Gain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Pin,
}

impl<CLK: OutputPin, DATA: InputPin, const N: usize> Hx711<CLK, DATA, N> {
    pub fn new(mut clk: CLK, data: [DATA; N], gain: Gain) -> Result<Self, Error> {
        clk.set_low().map_err(|_| Error::Pin)?;
        Ok(Self { clk, data, gain })
    }

    /// All chips have a conversion ready (DOUT low).
    pub fn is_ready(&mut self) -> Result<bool, Error> {
        for d in &mut self.data {
            if d.is_high().map_err(|_| Error::Pin)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Reads one conversion from every chip. Call when [`Self::is_ready`].
    pub fn read(&mut self, delay: &mut impl DelayNs, pulse: &mut impl Pulse) -> Result<[i32; N], Error> {
        let mut raw = [0u32; N];
        for _ in 0..24 {
            let bits = pulse.with(|| -> Result<[bool; N], Error> {
                self.clk.set_high().map_err(|_| Error::Pin)?;
                delay.delay_us(1);
                let mut bits = [false; N];
                for (b, d) in bits.iter_mut().zip(self.data.iter_mut()) {
                    *b = d.is_high().map_err(|_| Error::Pin)?;
                }
                self.clk.set_low().map_err(|_| Error::Pin)?;
                Ok(bits)
            })?;
            delay.delay_us(1);
            for (r, b) in raw.iter_mut().zip(bits) {
                *r = (*r << 1) | u32::from(b);
            }
        }
        for _ in 0..self.gain as u8 {
            pulse.with(|| -> Result<(), Error> {
                self.clk.set_high().map_err(|_| Error::Pin)?;
                delay.delay_us(1);
                self.clk.set_low().map_err(|_| Error::Pin)
            })?;
            delay.delay_us(1);
        }
        Ok(raw.map(sign_extend_24))
    }

    /// Clock high for > 60 µs: power down.
    pub fn power_down(&mut self, delay: &mut impl DelayNs) -> Result<(), Error> {
        self.clk.set_high().map_err(|_| Error::Pin)?;
        delay.delay_us(80);
        Ok(())
    }

    pub fn power_up(&mut self) -> Result<(), Error> {
        self.clk.set_low().map_err(|_| Error::Pin)
    }
}

pub fn sign_extend_24(v: u32) -> i32 {
    ((v << 8) as i32) >> 8
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use core::cell::RefCell;
    use core::convert::Infallible;
    use embedded_hal::digital::ErrorType;
    use std::rc::Rc;

    /// Two simulated HX711s on one clock.
    struct Chip {
        values: [u32; 2],
        clocks: u32,
    }

    #[derive(Clone)]
    struct Bus(Rc<RefCell<Chip>>);
    struct Clk(Bus);
    struct Data(Bus, usize);

    impl ErrorType for Clk {
        type Error = Infallible;
    }
    impl ErrorType for Data {
        type Error = Infallible;
    }
    impl OutputPin for Clk {
        fn set_high(&mut self) -> Result<(), Infallible> {
            self.0.0.borrow_mut().clocks += 1;
            Ok(())
        }
        fn set_low(&mut self) -> Result<(), Infallible> {
            Ok(())
        }
    }
    impl InputPin for Data {
        fn is_high(&mut self) -> Result<bool, Infallible> {
            let c = self.0.0.borrow();
            // After rising edge n (1-based), the chip presents bit 24-n.
            Ok(c.clocks >= 1 && c.clocks <= 24 && (c.values[self.1] >> (24 - c.clocks)) & 1 == 1)
        }
        fn is_low(&mut self) -> Result<bool, Infallible> {
            self.is_high().map(|h| !h)
        }
    }

    struct NoDelay;
    impl DelayNs for NoDelay {
        fn delay_ns(&mut self, _: u32) {}
    }

    #[test]
    fn reads_two_cells_on_a_shared_clock() {
        let bus = Bus(Rc::new(RefCell::new(Chip {
            values: [0x00_1234, 0xff_fffe],
            clocks: 0,
        })));
        let mut hx = Hx711::new(
            Clk(bus.clone()),
            [Data(bus.clone(), 0), Data(bus.clone(), 1)],
            Gain::A128,
        )
        .unwrap();
        assert!(hx.is_ready().unwrap());
        let v = hx.read(&mut NoDelay, &mut Unprotected).unwrap();
        assert_eq!(v, [0x1234, -2]);
        assert_eq!(bus.0.borrow().clocks, 25, "24 data pulses + 1 for gain 128");
    }

    #[test]
    fn sign_extension() {
        assert_eq!(sign_extend_24(0x80_0000), -8_388_608);
        assert_eq!(sign_extend_24(0x7f_ffff), 8_388_607);
    }
}
