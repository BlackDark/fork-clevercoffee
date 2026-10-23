//! SSD1306 and SH1106 128×64 monochrome OLEDs over I²C. The whole frame is sent each
//! refresh from a 1 KiB page-ordered buffer (`buf[page * 128 + x]`, bit 0 = top row of
//! the page), which is the layout `cc-display` renders into. Rotation is done by the
//! renderer, so both controllers are driven identically.

#![no_std]

use embedded_hal::i2c::I2c;

pub const WIDTH: usize = 128;
pub const HEIGHT: usize = 64;
pub const PAGES: usize = HEIGHT / 8;
pub const FRAME_BYTES: usize = WIDTH * PAGES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Controller {
    Ssd1306,
    /// 132-column RAM, the visible 128 start at column 2; page addressing only.
    Sh1106,
}

#[derive(Debug)]
pub struct Oled<I> {
    i2c: I,
    address: u8,
    controller: Controller,
}

const CMD: u8 = 0x00;
const DATA: u8 = 0x40;

impl<I: I2c> Oled<I> {
    pub fn new(i2c: I, address: u8, controller: Controller) -> Self {
        Self {
            i2c,
            address,
            controller,
        }
    }

    fn cmds(&mut self, c: &[u8]) -> Result<(), I::Error> {
        let mut buf = [0u8; 32];
        buf[0] = CMD;
        buf[1..=c.len()].copy_from_slice(c);
        self.i2c.write(self.address, &buf[..=c.len()])
    }

    /// Powers up and configures the panel (display stays dark until the first frame).
    pub fn init(&mut self) -> Result<(), I::Error> {
        self.cmds(&[0xae, 0xd5, 0x80, 0xa8, 0x3f, 0xd3, 0x00, 0x40])?;
        match self.controller {
            // Charge pump on, horizontal addressing.
            Controller::Ssd1306 => self.cmds(&[0x8d, 0x14, 0x20, 0x00])?,
            // DC-DC on.
            Controller::Sh1106 => self.cmds(&[0xad, 0x8b])?,
        }
        // Segment remap + COM scan reversed: the usual module orientation.
        self.cmds(&[
            0xa1, 0xc8, 0xda, 0x12, 0x81, 0xcf, 0xd9, 0xf1, 0xdb, 0x40, 0xa4, 0xa6,
        ])?;
        self.flush(&[0; FRAME_BYTES])?;
        self.cmds(&[0xaf])
    }

    pub fn set_on(&mut self, on: bool) -> Result<(), I::Error> {
        self.cmds(&[if on { 0xaf } else { 0xae }])
    }

    /// Sends a frame.
    pub fn flush(&mut self, frame: &[u8; FRAME_BYTES]) -> Result<(), I::Error> {
        let mut chunk = [0u8; 1 + WIDTH];
        chunk[0] = DATA;
        match self.controller {
            Controller::Ssd1306 => {
                self.cmds(&[0x21, 0, 127, 0x22, 0, 7])?;
                for page in frame.chunks(WIDTH) {
                    chunk[1..].copy_from_slice(page);
                    self.i2c.write(self.address, &chunk)?;
                }
            }
            Controller::Sh1106 => {
                for (p, page) in frame.chunks(WIDTH).enumerate() {
                    // Page p, column 2 (low nibble, high nibble).
                    self.cmds(&[0xb0 | p as u8, 0x02, 0x10])?;
                    chunk[1..].copy_from_slice(page);
                    self.i2c.write(self.address, &chunk)?;
                }
            }
        }
        Ok(())
    }

    pub fn release(self) -> I {
        self.i2c
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};
    use std::vec::Vec;

    #[derive(Default)]
    struct Log(Vec<Vec<u8>>);
    impl ErrorType for Log {
        type Error = ErrorKind;
    }
    impl I2c for Log {
        fn transaction(&mut self, _: u8, ops: &mut [Operation<'_>]) -> Result<(), ErrorKind> {
            for op in ops {
                if let Operation::Write(w) = op {
                    self.0.push(w.to_vec());
                }
            }
            Ok(())
        }
    }

    #[test]
    fn ssd1306_sends_eight_pages_after_setting_the_window() {
        let mut o = Oled::new(Log::default(), 0x3c, Controller::Ssd1306);
        let mut f = [0u8; FRAME_BYTES];
        f[0] = 0xaa;
        o.flush(&f).unwrap();
        let log = o.release().0;
        assert_eq!(log[0], [CMD, 0x21, 0, 127, 0x22, 0, 7]);
        let data: Vec<_> = log.iter().filter(|w| w[0] == DATA).collect();
        assert_eq!(data.len(), 8);
        assert_eq!(data[0][1], 0xaa);
        assert!(data.iter().all(|w| w.len() == 129));
    }

    #[test]
    fn sh1106_addresses_each_page_with_column_offset() {
        let mut o = Oled::new(Log::default(), 0x3c, Controller::Sh1106);
        o.flush(&[0; FRAME_BYTES]).unwrap();
        let log = o.release().0;
        assert_eq!(log[0], [CMD, 0xb0, 0x02, 0x10]);
        assert_eq!(log[14], [CMD, 0xb7, 0x02, 0x10]);
    }
}
