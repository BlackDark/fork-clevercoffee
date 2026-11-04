//! Decoder for the ZACwire protocol of the TSIC 306 temperature sensor.
//!
//! The sensor sends a frame about every 100 ms: two packets of a start bit, 8 data
//! bits (MSB first) and an even parity bit. Every bit starts with a falling edge; the
//! line stays low for 50 % of the bit period for the start bit, 25 % for a 1 and 75 %
//! for a 0. The start bit's low time is therefore the 1/0 threshold.
//!
//! The decoder is fed edge timestamps and has no timing of its own, so it is fully
//! testable on a PC. The firmware measures the pulses with the RMT peripheral (1 µs
//! resolution in hardware); [`decode_pulses`] turns one captured frame into a value.
//! Timestamping edges in a GPIO interrupt was tried first and lost a quarter of the
//! frames to interrupt jitter (see `docs/decisions/0004-behaviour-changes.md`).

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

/// Pulses in one captured frame: 20 falling and 20 rising edges make 40, but the
/// capture ends at the idle high after the last low, so that high is missing.
pub const FRAME_PULSES: core::ops::RangeInclusive<usize> = 39..=40;

/// Decodes one frame captured as `(level_is_high, length_µs)` pulses, starting at the
/// first falling edge. Anything but a complete frame is a framing error, so a capture
/// that began in the middle of a frame cannot pass for a value.
pub fn decode_pulses(pulses: impl IntoIterator<Item = (bool, u16)>) -> Result<u16, Error> {
    let mut decoder = Decoder::new();
    let mut t = 0u32;
    let (mut count, mut high) = (0usize, true);
    let mut result = None;
    for (level, len) in pulses {
        count += 1;
        high = level;
        result = decoder.edge(level, t).or(result);
        t = t.wrapping_add(u32::from(len));
    }
    if !high {
        // The rising edge that closes a last low pulse is not part of the capture.
        result = decoder.edge(true, t).or(result);
    }
    match result {
        Some(r) if FRAME_PULSES.contains(&count) => r,
        _ => Err(Error::Framing),
    }
}

/// A reading further than this (raw units, about 1 °C) from the median is counted as a spike.
pub const SPIKE_RAW: u16 = 10;

/// Median of the last three readings. The line has one parity bit per packet, so a frame
/// whose timing was disturbed (a delayed edge flips a bit; two flips keep the parity
/// right) decodes to a plausible but wrong value; on a real machine single frames showed
/// 50-99 °C among readings of 56 °C. A boiler cannot move that far in 100 ms, and a
/// median of three drops any single bad frame at the price of one frame of delay.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpikeFilter {
    last: [u16; 3],
    seen: u8,
}

impl SpikeFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes a decoded reading; returns the filtered one and whether this reading was
    /// a spike. `None` until three readings are in.
    pub fn push(&mut self, raw: u16) -> Option<(u16, bool)> {
        self.last = [self.last[1], self.last[2], raw];
        self.seen = (self.seen + 1).min(3);
        if self.seen < 3 {
            return None;
        }
        let mut sorted = self.last;
        sorted.sort_unstable();
        let median = sorted[1];
        Some((median, raw.abs_diff(median) > SPIKE_RAW))
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

    /// Pulses of a frame as the RMT delivers them, after a capture from the bench (bit
    /// period about 105 µs, 52 µs start bit): the gap between the packets is part of
    /// the last high of the first one, and the idle high after the frame is missing.
    fn pulses(raw: u16) -> Vec<(bool, u16)> {
        let mut v: Vec<(bool, u16)> = Vec::new();
        for byte in [(raw >> 8) as u8, raw as u8] {
            let mut bits: Vec<bool> = (0..8).rev().map(|b| byte >> b & 1 == 1).collect();
            bits.push(byte.count_ones() % 2 == 1);
            v.extend([(false, 52), (true, 53)]);
            for bit in bits {
                v.extend(if bit {
                    [(false, 26), (true, 78)]
                } else {
                    [(false, 78), (true, 26)]
                });
            }
            if v.len() == 20 {
                v.last_mut().unwrap().1 += 110; // gap between the packets
            }
        }
        v.pop();
        v
    }

    #[test]
    fn fix_tsic_decodes_an_rmt_capture() {
        let p = pulses(0x2ab);
        assert_eq!(p.len(), 39);
        assert_eq!(decode_pulses(p), Ok(0x2ab));
        // A capture that kept the closing high decodes the same.
        let mut p = pulses(0x2ab);
        p.push((true, 26));
        assert_eq!(decode_pulses(p), Ok(0x2ab));
    }

    #[test]
    fn rejects_incomplete_captures() {
        let full = pulses(0x2ab);
        // Started mid-frame, or cut short.
        assert_eq!(decode_pulses(full[3..].to_vec()), Err(Error::Framing));
        assert_eq!(decode_pulses(full[..20].to_vec()), Err(Error::Framing));
        assert_eq!(decode_pulses(Vec::new()), Err(Error::Framing));
    }

    #[test]
    fn converts_to_celsius() {
        assert!((celsius(0) + 50.0).abs() < 1e-4);
        assert!((celsius(2047) - 150.0).abs() < 1e-4);
        assert!((celsius(1488) - 95.38).abs() < 0.01);
    }

    fn filtered(inputs: &[u16]) -> Vec<(u16, bool)> {
        let mut f = SpikeFilter::new();
        inputs.iter().filter_map(|&r| f.push(r)).collect()
    }

    #[test]
    fn fix_tsic_needs_three_readings_first() {
        let mut f = SpikeFilter::new();
        assert_eq!(f.push(1000), None);
        assert_eq!(f.push(1000), None);
        assert_eq!(f.push(1000), Some((1000, false)));
    }

    #[test]
    fn fix_tsic_removes_a_single_spike_in_either_direction() {
        // 57.08, 53.86, 56.99 and 91.77, 96.07, 91.77 degrees, as on the machine.
        let raw = |c: f32| ((c + 50.0) * 2047.0 / 200.0) as u16;
        let out = filtered(&[raw(57.0), raw(57.1), raw(53.9), raw(57.0), raw(57.0)]);
        assert!(out.iter().all(|&(v, _)| (raw(57.0)..=raw(57.1)).contains(&v)));
        assert!(out.iter().any(|&(_, spike)| spike));
        let out = filtered(&[raw(91.8), raw(91.8), raw(96.1), raw(91.8), raw(91.8)]);
        assert!(out.iter().all(|&(v, _)| v == raw(91.8)));
    }

    #[test]
    fn a_fresh_filter_forgets_the_readings_from_before_an_outage() {
        let mut f = SpikeFilter::new();
        for _ in 0..3 {
            f.push(1000);
        }
        f = SpikeFilter::new();
        assert_eq!(f.push(1500), None);
        assert_eq!(f.push(1500), None);
        assert_eq!(f.push(1500), Some((1500, false)));
    }

    #[test]
    fn fix_tsic_follows_a_real_change_after_two_readings() {
        let out: Vec<u16> = filtered(&[1000, 1000, 1000, 1200, 1200, 1200])
            .into_iter()
            .map(|(v, _)| v)
            .collect();
        assert_eq!(out, std::vec![1000, 1000, 1200, 1200]);
    }

    #[test]
    fn fix_tsic_a_slow_ramp_is_not_a_spike() {
        let ramp: Vec<u16> = (0..20).map(|i| 1000 + i * 2).collect();
        assert!(filtered(&ramp).iter().all(|&(_, spike)| !spike));
    }
}
