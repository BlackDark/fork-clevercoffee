//! A bit-level `ZACwire` waveform synthesiser, for the host tests only.
//!
//! Owner: **R3-07**.
//!
//! # 🔴 What this is, and what a passing test of it is worth
//!
//! This module **generates** a GPIO waveform; [`decode`](super::decode) **reads**
//! one. There is no real TSIC-306 on the machine this was written on, so this is
//! the only way the decoder can be exercised at all — and that is exactly why the
//! weakness of the approach has to be said out loud rather than left for someone
//! to discover when they read a green test run as evidence.
//!
//! **A passing simulator test is not evidence that a TSIC-306 works.** It is
//! evidence that the decode arithmetic is self-consistent and that damaged frames
//! are rejected. It is *not* evidence about any of the following, none of which a
//! synthesiser can produce:
//!
//! * the real sensor's clock tolerance, and whether 62 µs measured against a
//!   62.5 µs half-period survives it plus a 1 µs timestamp quantisation;
//! * real 31.25 µs and 93.75 µs pulses through a pull-up, a cable and an input
//!   with a finite rise time — 31 µs is three time constants of a lot of RC;
//! * EMI, and the app note's own reason for parity existing (> 2 m of cable);
//! * what a real TSIC-306 does when it is brownout, at a supply edge, or
//!   unpowered, which is the case that actually matters for a heater.
//!
//! # How the independence is engineered, and where it stops
//!
//! The obvious version of this test is worthless: an encoder that calls the
//! decoder, or an encoder and a decoder that share a threshold, proves only that
//! the two agree. Three things push back on that:
//!
//! 1. **The encoder works in duty-cycle percentages of a bit window** — it knows
//!    "a `1` is 75 %" and nothing about how the decoder decides what a `1` is.
//!    It never computes a strobe and never compares against one.
//! 2. **The decoder derives its decision boundary from the waveform it was
//!    given**, by measuring the start bit. It does not use
//!    [`protocol::STROBE_US`]. Change the synthesiser's bit window to 120 µs and
//!    the decoder follows; there is no constant in it that has to change.
//! 3. **Edges are found by scanning the level function**, not by construction.
//!    [`Waveform::edges`] walks [`Waveform::level_at`] in 1 µs steps and reports
//!    where the level changes, so a rounding error in a pulse width becomes a
//!    *shifted edge* that the decoder has to cope with — which is what a real
//!    clock offset does.
//!
//! What they *do* share is [`protocol::BIT_WINDOW_US`] and
//! [`protocol::duty`], because those are the spec. Sharing the specification is
//! the point; sharing the decoder would defeat the exercise. The tests below
//! include one that shifts the bit window away from the nominal to prove point 2.
//!
//! # The waveform, from the app note
//!
//! §1.1: "The signal is normally high." §1.2: "The bit format is duty cycle
//! encoded: Start bit => 50 % duty cycle used to set up strobe time, Logic 1 =>
//! 75 % duty cycle, Logic 0 => 25 % duty cycle." §1.1: the packet is
//! "a start bit, 8 data bits, and a parity bit", MSB first, and "There is a
//! single bit window of high signal (stop bit) between the end of the first
//! transmission and the start of the second transmission."

use alloc::vec::Vec;

use super::decode::{Frame, FrameError};
use super::protocol::{self, duty};
use super::ring::Edge;

/// [`protocol::PACKET_BITS`] as a `usize`, for indexing a bit position.
///
/// `usize::from` is not `const` on this toolchain, so the value is spelled out
/// and asserted against the constant rather than written at each use.
const PACKET_BITS_INDEX: usize = 10;
const _: () = assert!(PACKET_BITS_INDEX == protocol::PACKET_BITS as usize);

/// How the synthesised waveform is broken, on purpose.
///
/// Nine flags, and that is the point: every one is a *specific* way a real
/// waveform goes wrong, and each has a test named after it. A single "corrupt"
/// flag would be easier to write and would prove nothing about which check
/// catches what.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "every field is an independent damage mode; see this type's docs"
)]
pub struct Damage {
    /// Flip one of the **11 raw data bits** — index 10 is the most significant,
    /// index 0 the least — and leave the synthesised parity bits as they would
    /// have been.
    ///
    /// That is the point of the damage mode: the wire then carries a data bit that
    /// does not match its parity bit, which is the only error parity can see, and
    /// the frame is catchable *only* by [`FrameError::BadParity`]. The reserved
    /// zeros of packet 1 cannot be addressed this way, which is deliberate —
    /// corrupting one of those is a *framing* error, and there is a separate
    /// check for it.
    pub flip_data_bit: Option<usize>,
    /// Give the start bit a different duty cycle, so it is not a 50 % pulse and
    /// the frame must be refused at acquisition.
    pub start_duty_pct: Option<u32>,
    /// Shorten the stop bit so the gap between the two packets is one bit window
    /// instead of two.
    pub missing_stop_bit: bool,
    /// Lengthen the stop bit so the gap is three windows.
    pub doubled_stop_bit: bool,
    /// Emit only the first packet, so the capture is one packet short.
    pub truncate_after_first_packet: bool,
    /// Emit no low pulses at all: a line with nothing on it.
    pub no_signal: bool,
    /// Drop one falling edge, so the two bits around it merge into one 250 µs
    /// gap. This is what a lost interrupt looks like.
    pub drop_falling_edge_at: Option<usize>,
    /// The bit window, so a test can present a sensor running off nominal.
    pub bit_window_us: Option<u32>,
}

impl Damage {
    /// An undamaged waveform.
    ///
    /// Every field is named, so a new damage mode is a compile error here rather
    /// than a silently-ignored field at a call site that thinks it is testing
    /// something.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            flip_data_bit: None,
            start_duty_pct: None,
            missing_stop_bit: false,
            doubled_stop_bit: false,
            truncate_after_first_packet: false,
            no_signal: false,
            drop_falling_edge_at: None,
            bit_window_us: None,
        }
    }
}

/// A synthesised `ZACwire` transmission.
#[derive(Clone, Copy, Debug)]
pub struct Waveform {
    /// The 11-bit digital output value the waveform encodes.
    pub raw: u16,
    /// The nominal bit window for this waveform. 125 µs unless damaged.
    pub bit_window_us: u32,
    damage: Damage,
    no_signal: bool,
}

impl Waveform {
    /// An undamaged waveform for `raw`, at the nominal 8 kHz.
    #[must_use]
    pub const fn for_raw(raw: u16) -> Self {
        Self {
            raw,
            bit_window_us: protocol::BIT_WINDOW_US,
            damage: Damage {
                flip_data_bit: None,
                start_duty_pct: None,
                missing_stop_bit: false,
                doubled_stop_bit: false,
                truncate_after_first_packet: false,
                no_signal: false,
                drop_falling_edge_at: None,
                bit_window_us: None,
            },
            no_signal: false,
        }
    }

    /// The same waveform, damaged.
    #[must_use]
    pub fn damaged(mut self, damage: Damage) -> Self {
        self.no_signal = damage.no_signal;
        if let Some(window) = damage.bit_window_us {
            self.bit_window_us = window;
        }
        self.damage = damage;
        self
    }

    /// The duty cycle of the bit at `index` in falling-edge order, in per cent.
    fn duty_at(&self, index: usize) -> u32 {
        // Only the *first* start bit can be damaged. A 25 % "start" in packet 2
        // would be rejected by the period check anyway, and the decoder
        // deliberately does not re-measure the strobe for the second packet -- so
        // damaging it here would test a code path that does not exist.
        if index == 0 {
            if let Some(pct) = self.damage.start_duty_pct {
                return pct;
            }
        }
        if index == 0 || index == protocol::PACKET_BITS as usize {
            duty::START_PCT
        } else if self.bit_at(index) {
            duty::ONE_PCT
        } else {
            duty::ZERO_PCT
        }
    }

    /// The position of a data bit within its packet, in **transmission order**.
    ///
    /// `None` for the two start bits and the two parity bits.
    fn data_position(index: usize) -> Option<(u8, bool)> {
        match index {
            1..=8 => Some(
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "the range bounds the value at 7"
                )]
                ((index - 1) as u8, true),
            ),
            11..=18 => Some(
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "the range bounds the value at 7"
                )]
                ((index - 11) as u8, false),
            ),
            _ => None,
        }
    }

    /// The raw bit a data position carries, or `None` for a reserved zero.
    ///
    /// # The layout, and the part that is easy to get wrong
    ///
    /// Falling-edge order, 20 bits:
    ///
    /// ```text
    ///   0        1 2 3 4 5 6 7 8   9      10      11..=18        19
    ///   start1   p p p p p s s s  parity1 start2  d d d d d d d d  parity2
    ///            \_____/ ^^^^^^                   \_________/
    ///            reserved  raw                    raw
    ///            zeros    bits 10,9,8            bits 7..0
    /// ```
    ///
    /// **The significant bits of packet 1 are at positions 5, 6 and 7, not 0, 1
    /// and 2.** The app note says the data byte is `0b00000_011` and that the
    /// bits go out "MSB first, LSB last", so the first bit transmitted is the
    /// byte's *bit 7* — and the three significant bits live in the byte's bits
    /// 2, 1 and 0. Positions are counted from the first bit sent, so byte bit
    /// `b` is at position `7 - b`: bits 2/1/0 are positions 5/6/7, and
    /// positions 0..4 are the reserved zeros.
    ///
    /// Getting this backwards is silent. Both orderings produce an 11-bit
    /// number and both produce valid parity, so the only symptom is a
    /// temperature that is wrong by a factor of 32 in the top three bits — and
    /// `a_whole_eleven_bit_range_decodes_to_itself` is the test that catches it,
    /// which is the reason that test walks all 2048 codes rather than a handful.
    #[allow(
        clippy::unused_self,
        reason = "kept as a method so it reads alongside the other bit helpers"
    )]
    fn raw_bit_at(&self, index: usize) -> Option<u16> {
        let (position, high_packet) = Self::data_position(index)?;
        if high_packet {
            // Positions 5..7 carry raw bits 10..8; 0..4 are the reserved zeros.
            if position >= 5 {
                Some(10 - u16::from(position - 5))
            } else {
                None
            }
        } else {
            Some(7 - u16::from(position))
        }
    }

    /// The value of the bit at `index` in falling-edge order.
    ///
    /// Includes the parity bits, which are **computed**, not assumed zero — the
    /// app note's example happens to have an even-parity byte of `0x96` and so a
    /// zero parity bit, and a synthesiser that always emitted zero would never
    /// produce an odd-parity packet, which is the case a decoder most needs to
    /// see.
    fn bit_at(&self, index: usize) -> bool {
        match index {
            9 => return self.parity_at(1),
            19 => return self.parity_at(2),
            _ => {}
        }
        let Some(raw_bit) = self.raw_bit_at(index) else {
            // A start bit, or one of the reserved zeros.
            return false;
        };
        let value = (self.raw >> raw_bit) & 1 == 1;
        // `flip_data_bit` indexes the **11 raw bits** (10 = the most
        // significant), so "flip the bit carrying DS bit 4" is
        // `Some(4)` regardless of which packet it lands in. The synthesised
        // parity is then left as it would have been, so the frame is catchable
        // *only* by the parity check — which is the point of the damage mode.
        value
            != self
                .damage
                .flip_data_bit
                .is_some_and(|flip| flip == usize::from(raw_bit))
    }

    /// The even parity bit of one packet.
    ///
    /// "The packet ends with an even parity bit" (app note §1.1): the parity bit
    /// is 1 when the eight data bits hold an odd number of ones, so the packet as
    /// a whole holds an even number.
    #[allow(
        clippy::unused_self,
        reason = "kept as a method so it reads alongside the other bit helpers"
    )]
    fn parity_at(&self, packet: u8) -> bool {
        let base = usize::from(packet) - 1;
        let first = base * protocol::PACKET_BITS as usize + 1;
        let mut ones = 0u32;
        let mut offset = 0;
        while offset < protocol::DATA_BITS as usize {
            // `bit_at_ideal`, not `bit_at`: the parity bit the sensor would have
            // sent is the one for the *undamaged* data. Using `bit_at` here would
            // make the damage mode a no-op, because the synthesiser would keep
            // the frame self-consistent and every "corrupted" test would be
            // testing a valid frame.
            if self.bit_at_ideal(first + offset) {
                ones += 1;
            }
            offset += 1;
        }
        ones % 2 == 1
    }

    /// The data bit as the sensor intended it, with no damage applied.
    #[allow(
        clippy::unused_self,
        reason = "kept as a method so it reads alongside the other bit helpers"
    )]
    fn bit_at_ideal(&self, index: usize) -> bool {
        let Some(raw_bit) = self.raw_bit_at(index) else {
            return false;
        };
        (self.raw >> raw_bit) & 1 == 1
    }

    /// The offset, in bit windows, at which the bit at `index` starts.
    ///
    /// Bits 0..=9 sit back to back. Bit 10 is packet 2's start bit and the stop
    /// bit occupies the window before it, so from index 10 on everything shifts
    /// by one window.
    fn start_of(&self, index: usize) -> u32 {
        // Bit index 10 is packet 2's start bit, and the stop bit occupies the
        // whole window in front of it — so from index 10 on, everything is one
        // window later than the bit index alone suggests. Index 9 is packet 1's
        // parity bit and sits at 9 windows; index 10 must sit at 11, not 10.
        //
        // A *doubled* stop bit is one more window than that, and a *missing* one
        // is one fewer, so both damage modes are a single arithmetic change here
        // rather than a special case in the waveform.
        let stop_windows: u32 = if self.damage.doubled_stop_bit {
            2
        } else {
            u32::from(!self.damage.missing_stop_bit)
        };
        let before_second_packet = index >= PACKET_BITS_INDEX;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "index < TRANSMISSION_BITS, which is 20"
        )]
        let windows = index as u32 + u32::from(before_second_packet) * stop_windows;
        windows * self.bit_window_us
    }

    /// The level of the line `at_us` microseconds after the transmission begins.
    ///
    /// High is the idle level (app note §1.1); a bit is low for the first
    /// `duty %` of its window and high for the rest. This is the *only* place the
    /// waveform exists — [`Self::edges`] discovers the transitions by scanning
    /// it, so nothing downstream knows how the pulses were built.
    #[must_use]
    pub fn level_at(&self, at_us: u32) -> bool {
        if self.no_signal {
            return true;
        }
        let windows = self.transmission_bits();
        for index in 0..windows as usize {
            if self.damage.drop_falling_edge_at == Some(index) {
                continue;
            }
            let start = self.start_of(index);
            let end = start + self.bit_window_us;
            if at_us >= start && at_us < end {
                // Low for the first `pct` per cent of the window, then high.
                // Truncated, like `protocol::low_us`, so a synthesised pulse is
                // 31/62/93 rather than 31/63/94. The decoder must not care, and
                // that it does not is a test.
                let inside_pulse = at_us - start < (self.bit_window_us * self.duty_at(index)) / 100;
                return !inside_pulse;
            }
        }
        // Between the two packets the stop bit is a window of pure high, and
        // after the last bit the line idles high. Both fall through to `true`.
        true
    }

    /// How many bits this waveform emits, before the damage.
    fn transmission_bits(&self) -> u32 {
        if self.damage.truncate_after_first_packet {
            u32::from(protocol::PACKET_BITS)
        } else {
            u32::from(protocol::TRANSMISSION_BITS)
        }
    }

    /// How long the waveform is, in microseconds, including the trailing idle.
    #[must_use]
    pub fn duration_us(&self) -> u32 {
        self.transmission_bits() * self.bit_window_us + 2 * self.bit_window_us
    }

    /// Every edge in the waveform, in time order, found by **scanning**
    /// [`Self::level_at`] at 1 µs.
    ///
    /// The scan is what keeps the test honest: an edge here is a *discovered*
    /// transition, so a pulse width that does not land on a whole microsecond,
    /// or a start bit that is 75 % instead of 50 %, shows up as a genuinely
    /// different edge list rather than as a flag the decoder could see.
    #[must_use]
    pub fn edges(&self) -> Vec<Edge> {
        let mut edges = Vec::new();
        let mut previous = true; // the line idles high
        #[allow(
            clippy::cast_possible_truncation,
            reason = "duration_us is 22 bit windows, under 8 k"
        )]
        for at_us in 0..=self.duration_us() {
            let level = self.level_at(at_us);
            if level != previous {
                edges.push(Edge { at_us, high: level });
                previous = level;
            }
        }
        edges
    }

    /// Decode this waveform with the production decoder.
    ///
    /// # Errors
    ///
    /// Whatever [`decode_frame`](super::decode::decode_frame) says, unmodified.
    /// There is no "expected" helper here on purpose: the tests must not be able
    /// to assert agreement with a second, more forgiving implementation.
    ///
    /// [`FrameError`]: super::decode::FrameError
    pub fn decode(&self) -> Result<Frame, FrameError> {
        super::decode::decode_frame(&self.edges())
    }
}

#[cfg(test)]
#[allow(
    clippy::assertions_on_constants,
    reason = "these tests assert protocol and transport constants against the \
              datasheet/app note numbers on purpose: that is the claim, and the \
              compiler is right that a run-time comparison of two constants is \
              not a test"
)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "test-side narrowing of values the protocol bounds to 0..=2047"
)]
mod tests {
    use super::*;

    #[test]
    fn an_undamaged_waveform_emits_two_edges_per_bit() {
        // 20 bits, each contributing a falling and a rising edge, and the stop
        // bit contributing none: 40 edges. The app note's "2.7 ms" of
        // transmission is 21 windows x 125 us = 2625 us, which is the 20 bits
        // plus the stop bit.
        let wire = Waveform::for_raw(0);
        let edges = wire.edges();
        assert_eq!(edges.len(), 40);
        assert_eq!(edges.iter().filter(|edge| !edge.high).count(), 20);
        // The app note's "2.7 ms": 21 windows, being 20 bits plus the stop bit.
        let last_fall = wire
            .edges()
            .iter()
            .rfind(|edge| !edge.high)
            .map_or(0, |edge| edge.at_us);
        assert_eq!(last_fall, 20 * protocol::BIT_WINDOW_US, "2500 us in");
        assert_eq!(last_fall + protocol::BIT_WINDOW_US, 2_625);
        // And two windows of idle on the end, so the scan has a high tail to
        // find the last rising edge in.
        assert_eq!(wire.duration_us(), 22 * protocol::BIT_WINDOW_US);
    }

    #[test]
    fn the_parity_bits_are_computed_and_not_assumed_zero() {
        // A synthesiser that always emitted a zero parity bit could never
        // produce an odd-parity packet, and an odd-parity packet is the case a
        // decoder most needs to see exercised. `raw = 0` has even parity in both
        // packets (zero ones) and `raw = 1` has odd parity in the low packet.
        assert!(
            !Waveform::for_raw(0).bit_at(9),
            "raw 0: packet 1 parity is 0"
        );
        assert!(
            !Waveform::for_raw(0).bit_at(19),
            "raw 0: packet 2 parity is 0"
        );
        assert!(
            Waveform::for_raw(1).bit_at(19),
            "raw 1: packet 2 has one one, so its parity bit must be 1"
        );
        assert!(
            !Waveform::for_raw(1).bit_at(9),
            "raw 1: packet 1 is all zeros"
        );
        // And three ones in the low packet also gives parity 1.
        assert!(Waveform::for_raw(0b111).bit_at(19));
    }

    #[test]
    fn the_reserved_bits_of_packet_one_are_synthesised_as_zero() {
        // Data positions 3..7 of packet 1 are the sensor's reserved zeros. If the
        // synthesiser put anything there, every "good" frame would be refused by
        // the decoder's reserved-bit check and the round-trip test would pass for
        // entirely the wrong reason.
        let wire = Waveform::for_raw(0b111_1111_1111);
        // Positions 0..4 are the reserved zeros, positions 5..7 carry raw 10..8.
        for index in 1..=5usize {
            assert!(!wire.bit_at(index), "packet 1 position {index} must be 0");
        }
        for index in 6..=8usize {
            assert!(wire.bit_at(index), "packet 1 position {index} carries DS");
        }
        // And packet 2's eight positions carry raw 7..0 in order, so position 0 is
        // raw bit 7 -- the *most* significant of the low byte.
        for index in 11..=18usize {
            let expected = 0b1111_1111u16 >> (18 - index);
            assert_eq!(
                wire.bit_at(index),
                expected & 1 == 1,
                "packet 2 position {}",
                index - 11
            );
        }
    }

    #[test]
    fn the_first_pulse_is_half_a_bit_window() {
        let wire = Waveform::for_raw(0);
        let edges = wire.edges();
        assert!(!edges[0].high, "the line falls first");
        let width = edges[1].at_us - edges[0].at_us;
        assert_eq!(width, 62, "50 % of 125 is 62.5, truncated to 62");
        assert!(edges[0].at_us == 0, "and the transmission starts at zero");
        assert!(width * 2 <= protocol::BIT_WINDOW_US);
    }

    #[test]
    fn a_missing_and_a_doubled_stop_bit_move_the_second_packet_by_one_window() {
        // Both damage modes are one arithmetic change in `start_of`, and both
        // must be visible as a gap the decoder's stop check will reject.
        let good = Waveform::for_raw(0).edges();
        let missing = Waveform::for_raw(0)
            .damaged(Damage {
                missing_stop_bit: true,
                ..Damage::none()
            })
            .edges();
        let doubled = Waveform::for_raw(0)
            .damaged(Damage {
                doubled_stop_bit: true,
                ..Damage::none()
            })
            .edges();
        // The first ten falling edges are packet 1 and are unmoved in all three.
        for edges in [&good, &missing, &doubled] {
            let first_ten: alloc::vec::Vec<u32> = edges
                .iter()
                .filter(|edge| !edge.high)
                .take(10)
                .map(|edge| edge.at_us)
                .collect();
            assert_eq!(first_ten[9], 9 * protocol::BIT_WINDOW_US);
        }
        // And the eleventh moves by exactly one window each way.
        let eleventh = |edges: &[Edge]| {
            edges
                .iter()
                .filter(|edge| !edge.high)
                .nth(10)
                .map_or(0, |edge| edge.at_us)
        };
        assert_eq!(eleventh(&good), 11 * protocol::BIT_WINDOW_US);
        assert_eq!(
            eleventh(&missing),
            10 * protocol::BIT_WINDOW_US,
            "a missing stop bit pulls the second packet one window earlier"
        );
        assert_eq!(
            eleventh(&doubled),
            12 * protocol::BIT_WINDOW_US,
            "a doubled stop bit pushes it one window later"
        );
    }

    #[test]
    fn the_stop_bit_is_a_window_of_pure_high_between_the_packets() {
        // Falling edges 9 and 10 straddle it, and nothing happens in between.
        let wire = Waveform::for_raw(0);
        let falls: Vec<u32> = wire
            .edges()
            .iter()
            .filter(|edge| !edge.high)
            .map(|edge| edge.at_us)
            .collect();
        assert_eq!(falls.len(), 20);
        for index in 1..falls.len() {
            if index == 10 {
                continue;
            }
            assert_eq!(
                falls[index] - falls[index - 1],
                protocol::BIT_WINDOW_US,
                "gap before bit {index}"
            );
        }
        // Two windows, not one: this is the whole of the stop-bit check.
        assert_eq!(falls[10] - falls[9], 2 * protocol::BIT_WINDOW_US);
        assert_eq!(falls[9], 9 * protocol::BIT_WINDOW_US);
        assert_eq!(falls[10], 11 * protocol::BIT_WINDOW_US);
    }

    #[test]
    fn the_waveform_tracks_a_shifted_bit_window() {
        // The independence argument, as an assertion: the synthesiser's timings
        // and the decoder's decision boundary are separate, so a sensor running
        // 4 % slow decodes without either side being told.
        let wire = Waveform::for_raw(0).damaged(Damage {
            bit_window_us: Some(120),
            ..Damage::none()
        });
        let decoded = wire.decode().map(|frame| frame.strobe_us);
        // 120 * 0.5 = 60 us, measured by the decoder from the waveform.
        assert_eq!(decoded, Ok(60));
    }
}
