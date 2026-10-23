//! Decoder for the ZACwire protocol of the TSIC 306 temperature sensor.
//!
//! The sensor sends a frame about every 100 ms: two packets of a start bit, 8 data
//! bits (MSB first) and an even parity bit. Every bit starts with a falling edge; the
//! line stays low for 50 % of the bit period for the start bit, 25 % for a 1 and 75 %
//! for a 0. The start bit's low time is therefore the 1/0 threshold.
//!
//! The decoder is fed edge timestamps (from a GPIO interrupt) and has no timing of
//! its own, so it is fully testable on a PC.

#![no_std]

/// Line idle (high) at least this long before a falling edge starts a frame.
pub const FRAME_GAP_US: u32 = 1_000;
/// Plausible start-bit low time (nominal 62.5 µs at the 8 kHz strobe).
pub const START_BIT_US: core::ops::RangeInclusive<u32> = 20..=200;

/// TSIC 306: 11-bit value over -50..150 °C.
pub fn celsius(raw: u16) -> f32 {
    f32::from(raw) * 200.0 / 2047.0 - 50.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A packet's parity did not match.
    Parity,
    /// Bits that must be zero were set, or the start bit was implausible.
    Framing,
}

#[derive(Debug, Clone, Default)]
pub struct Decoder {
    last_fall_us: Option<u32>,
    last_rise_us: Option<u32>,
    /// Pulse index within the frame: 0..=19, `None` while waiting for a frame.
    index: Option<u8>,
    strobe_us: u32,
    packet: u16,
    high: u16,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one edge. `rising` is the level change, `t_us` a monotonic microsecond
    /// timestamp (wrapping is fine). Returns the raw 11-bit value when a frame ends.
    pub fn edge(&mut self, rising: bool, t_us: u32) -> Option<Result<u16, Error>> {
        if !rising {
            let idle = self.last_rise_us.map_or(u32::MAX, |r| t_us.wrapping_sub(r));
            if idle >= FRAME_GAP_US {
                self.index = Some(0);
            }
            self.last_fall_us = Some(t_us);
            return None;
        }
        self.last_rise_us = Some(t_us);
        let (Some(fall), Some(i)) = (self.last_fall_us, self.index) else {
            return None;
        };
        let low = t_us.wrapping_sub(fall);
        let bit_in_packet = i % 10;
        if bit_in_packet == 0 {
            if !START_BIT_US.contains(&low) {
                self.index = None;
                return Some(Err(Error::Framing));
            }
            self.strobe_us = low;
            self.packet = 0;
        } else {
            self.packet = (self.packet << 1) | u16::from(low < self.strobe_us);
        }
        if bit_in_packet == 9 {
            let data = self.packet >> 1;
            let ones = self.packet.count_ones();
            if !ones.is_multiple_of(2) {
                self.index = None;
                return Some(Err(Error::Parity));
            }
            if i == 9 {
                self.high = data;
            } else {
                self.index = None;
                if self.high & !0x07 != 0 {
                    return Some(Err(Error::Framing));
                }
                return Some(Ok((self.high << 8) | data));
            }
        }
        self.index = Some(i + 1);
        None
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    const T: u32 = 125; // bit period, µs

    /// Edges (rising, t) for a frame encoding `raw`, starting at `t0`.
    fn frame(raw: u16, t0: u32, corrupt_parity: bool) -> Vec<(bool, u32)> {
        let mut edges = Vec::new();
        let mut t = t0;
        for (n, byte) in [(raw >> 8) as u8, raw as u8].into_iter().enumerate() {
            let mut bits = Vec::new();
            for b in (0..8).rev() {
                bits.push(byte >> b & 1 == 1);
            }
            let parity = (byte.count_ones() % 2 == 1) ^ (corrupt_parity && n == 1);
            bits.push(parity);
            // start bit
            edges.push((false, t));
            edges.push((true, t + T / 2));
            t += T;
            for bit in bits {
                edges.push((false, t));
                edges.push((true, t + if bit { T / 4 } else { 3 * T / 4 }));
                t += T;
            }
            t += T; // stop gap between packets
        }
        edges
    }

    fn decode(d: &mut Decoder, edges: &[(bool, u32)]) -> Vec<Result<u16, Error>> {
        edges.iter().filter_map(|&(r, t)| d.edge(r, t)).collect()
    }

    #[test]
    fn decodes_frames() {
        let mut d = Decoder::new();
        for (i, raw) in [0u16, 0x7ff, 0x4c3, 1234].into_iter().enumerate() {
            let out = decode(&mut d, &frame(raw, 10_000 + i as u32 * 100_000, false));
            assert_eq!(out, std::vec![Ok(raw)]);
        }
    }

    #[test]
    fn detects_parity_errors() {
        let mut d = Decoder::new();
        assert_eq!(
            decode(&mut d, &frame(1000, 5_000, true)),
            std::vec![Err(Error::Parity)]
        );
        assert_eq!(decode(&mut d, &frame(1000, 105_000, false)), std::vec![Ok(1000)]);
    }

    #[test]
    fn survives_timestamp_wrap() {
        let mut d = Decoder::new();
        let start = u32::MAX - 1_000;
        let edges: Vec<_> = frame(777, 0, false)
            .into_iter()
            .map(|(r, t)| (r, t.wrapping_add(start)))
            .collect();
        assert_eq!(decode(&mut d, &edges), std::vec![Ok(777)]);
    }

    #[test]
    fn tolerates_strobe_drift() {
        // Bit period 10 % slow; the start bit recalibrates.
        let mut d = Decoder::new();
        let edges: Vec<_> = frame(1500, 0, false)
            .into_iter()
            .map(|(r, t)| (r, t * 11 / 10))
            .collect();
        assert_eq!(decode(&mut d, &edges), std::vec![Ok(1500)]);
    }

    #[test]
    fn converts_to_celsius() {
        assert!((celsius(0) + 50.0).abs() < 1e-4);
        assert!((celsius(2047) - 150.0).abs() < 1e-4);
        assert!((celsius(1488) - 95.38).abs() < 0.01);
    }
}
