//! Strobe acquisition and frame assembly: a pure function of edge timestamps.
//!
//! Owner: **R3-07**.
//!
//! # This is the app note's §1.3 decoder, written out
//!
//! > When the falling edge of the start bit occurs, measure the time until the
//! > rising edge of the start bit. This time (`Tstrobe`) is the strobe time.
//! > When the next falling edge occurs, wait for a time period equal to
//! > `Tstrobe`, and then sample the ZACwire signal. The data present on the
//! > signal at this time is the bit being transmitted. […] the sampling window is
//! > reset with every bit transmission.
//!
//! Restated as arithmetic, which is what [`decode_frame`] does:
//!
//! 1. find the start bit's falling edge and its rising edge;
//! 2. `strobe = rising − falling`, and check it is a plausible 50 % pulse;
//! 3. for every following bit, take its falling edge, find its rising edge, and
//!    decide `bit = (rising − falling) > strobe`.
//!
//! Step 3 is the app note's "wait `Tstrobe`, then sample", computed without
//! waiting: at `Tstrobe` after the falling edge the line is still low **iff** the
//! pulse outlasts the strobe, and "the line is still low" is exactly
//! `pulse > strobe`. So the decoder needs two timestamps per bit instead of one
//! sample, which is what makes it runnable in a task on a buffer of edges rather
//! than in a busy-waiting ISR.
//!
//! # What is checked, and why each check exists
//!
//! | check | catches |
//! | --- | --- |
//! | the start pulse is in the strobe window | a frame that did not start on a start bit — 1-Wire traffic, a floating line, a partial capture |
//! | every falling edge is 94..156 µs after the last | a **missing** edge (250 µs) or a **spurious** one (62 µs) |
//! | the 9 → 10 gap is 188..234 µs | a **missing stop bit** (125 µs) or a doubled one (375 µs) |
//! | `data_high`'s top five bits are zero | a mis-framed frame: those bits are the sensor's reserved zeros |
//! | even parity on each packet | a single bit flipped anywhere in a packet |
//!
//! The parity check is the app note's and it is weak by construction — it cannot
//! see two flipped bits. That is why it is not the only check: the period checks
//! catch the framing errors parity cannot, and the reserved-zero check catches the
//! mis-framing that a coincidentally-valid parity would let through.
//!
//! # 🔴 What this cannot tell you
//!
//! **A passing test of this module is not evidence that a TSIC-306 works.** The
//! waveform it is tested against is synthesised by
//! [`simulator`](super::simulator) from the spec's own timings. That proves the
//! arithmetic, the ordering, the rejection of damaged frames, and the
//! DS → °C conversion. It proves nothing about a real sensor: real clock
//! tolerance, real 31.25 µs and 93.75 µs pulses that a 1 µs clock rounds, real
//! rise and fall times through a pull-up and a cable, real EMI, and a real
//! sensor's behaviour when it is brownout. No TSIC-306 is fitted to the machine
//! this was written on. See the module docs of [`super`].

use super::protocol::{self, tolerances};
use super::ring::Edge;

/// A decoded transmission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    /// The 11-bit digital output value, `DS`.
    ///
    /// Bits 10..8 are the low three bits of packet 1's data byte and bits 7..0
    /// are packet 2's, which is the app note's "the first packet contains the
    /// most significant 3 bits […] the second packet contains the least
    /// significant 8 bits".
    pub raw: u16,
    /// The measured `Tstrobe`, in microseconds.
    ///
    /// Kept because it is a **calibration readout**: it is the half-period the
    /// sensor is actually running at, and a value drifting away from 62 says the
    /// sensor's clock is drifting or the line is being distorted. The C++ has the
    /// same number in `ZACwire::bitThreshold` and never reports it.
    pub strobe_us: u32,
}

impl Frame {
    /// The temperature in °C: `DS / 2047 · 200 − 50`.
    ///
    /// The app note's formula (`T = (DS / 2047) · (HT − LT) + LT`, `HT = 150`,
    /// `LT = −50`), computed in `f32`.
    ///
    /// # Not the C++'s arithmetic, and the difference is up to 0.1 °C
    ///
    /// `ZACwire::getTemp` (`ZACwire.cpp:62`) uses
    /// `((temp * 250L >> 8) - 499) / 10.0`, which is the same line written with
    /// an integer shift and an integer subtract. Two consequences:
    ///
    /// * `raw * 250 >> 8` truncates, so the result is quantised to 0.1 °C and
    ///   biased low by up to 0.1 °C;
    /// * at `raw = 0` the C++ gives `(-499)/10.0` = **−49.9 °C**, not the
    ///   datasheet's −50.0 °C, and at `raw = 2047` it gives 150.0 °C.
    ///
    /// The app note's formula is used because the app note is the
    /// specification, and because a driver whose floor is 0.1 °C off the
    /// datasheet is a driver whose floor is wrong in the direction that matters
    /// for a `temp <= 0.0` reject. The two agree exactly at the top of the range
    /// and differ by at most one 0.1 °C step everywhere else.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn celsius(self) -> f32 {
        f32::from(self.raw) / protocol::RANGE_STEPS * protocol::RANGE_SPAN_C + protocol::RANGE_MIN_C
    }
}

/// Why a captured waveform is not a temperature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Not enough edges to be a transmission: fewer than
    /// [`protocol::TRANSMISSION_BITS`] falling edges, or a capture that stopped
    /// mid-packet.
    Incomplete,
    /// The first usable falling edge's pulse is not a 50 % one, so this is not a
    /// start bit. 1-Wire traffic, a floating line and a partial capture all land
    /// here.
    NoStartBit,
    /// A falling edge is missing or spurious: the interval is outside the
    /// bit-period window.
    BadBitPeriod,
    /// The gap where the stop bit should be is not two bit windows.
    BadStopBit,
    /// A packet's parity bit does not make the packet's nine bits even.
    BadParity {
        /// Which packet: 1 is the three most significant bits, 2 the rest.
        packet: u8,
    },
    /// Packet 1's data byte has a non-zero reserved bit, which means the frame is
    /// mis-framed even though its parity happened to be valid.
    ReservedBitsSet,
}

impl core::fmt::Display for FrameError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Incomplete => f.write_str("not enough edges for a transmission"),
            Self::NoStartBit => f.write_str("no 50% start pulse found"),
            Self::BadBitPeriod => f.write_str("falling edges are not one bit window apart"),
            Self::BadStopBit => f.write_str("no stop bit between the two packets"),
            Self::BadParity { packet } => write!(f, "packet {packet} failed even parity"),
            Self::ReservedBitsSet => f.write_str("packet 1's reserved bits are not zero"),
        }
    }
}

/// Decode one transmission from a captured run of edges.
///
/// `edges` must be **in time order**. Its start point does not matter: the
/// decoder scans forward for a falling edge whose pulse *is* a start bit, so a
/// capture that began mid-frame — which is the normal case for a ring that is
/// drained whenever the decode task gets round to it — is decoded from the next
/// real start bit rather than refused.
///
/// The scan is bounded by a single rule: **a candidate is rejected only if its
/// pulse is not a start bit.** Every other failure is reported, because a
/// candidate that *is* a start bit and then fails means the waveform is a
/// damaged `ZACwire` frame, and hunting for another start bit inside a damaged
/// frame is how a decoder ends up assembling a frame out of two transmissions.
///
/// # Errors
///
/// [`FrameError`], in the order the checks are made. Every one of them means
/// "no reading", never "a reading that might be wrong": the caller maps all of
/// them onto the C++'s 222, and none of them can produce a temperature.
pub fn decode_frame(edges: &[Edge]) -> Result<Frame, FrameError> {
    // No falling edge at all means a line that never moved: a disconnected probe,
    // or a capture that began after the last transmission. That is
    // `Incomplete`, not a mis-framed frame, and it is what the driver maps onto
    // "nothing to decode" rather than 222-on-a-damaged-frame.
    if !edges.iter().any(|edge| !edge.high) {
        return Err(FrameError::Incomplete);
    }
    let mut first = 0usize;
    loop {
        // Land on a falling edge. The slice is in time order, so the level
        // alternates and the falling edges are exactly the ones with `high` clear.
        while first < edges.len() && edges[first].high {
            first += 1;
        }
        if first >= edges.len() {
            // Every falling edge in the capture has been tried and none was a
            // start bit. That is the honest answer for a line that is moving but
            // is not a ZACwire line.
            return Err(FrameError::NoStartBit);
        }
        match try_from(edges, first) {
            // Only acquisition failure advances the search. A frame that *is*
            // framed but then fails — short of bits, a bad period, a bad stop
            // gap, bad parity — is reported as it is, because hunting for another
            // start bit inside a damaged frame is how a decoder ends up
            // assembling a frame out of two transmissions.
            Err(FrameError::NoStartBit) => first += 1,
            other => return other,
        }
    }
}

/// The number of falling edges in a transmission, as a `usize`.
///
/// `protocol::TRANSMISSION_BITS` is a `u8` and `usize::from` is not `const` on
/// this toolchain, so the value is spelled out and asserted against the constant
/// here rather than written at each use.
const FALLS: usize = 20;
const _: () = assert!(FALLS == protocol::TRANSMISSION_BITS as usize);

/// One attempt, starting the frame at `start_index`.
fn try_from(edges: &[Edge], start_index: usize) -> Result<Frame, FrameError> {
    // --- 1. find the start bit: a falling edge, then its rising edge ---------
    let fall = edges[start_index].at_us;
    let Some(rise_offset) = edges[start_index + 1..].iter().position(|edge| edge.high) else {
        return Err(FrameError::Incomplete);
    };
    let rise_index = start_index + 1 + rise_offset;
    let rise = edges[rise_index].at_us;
    // The slice is in time order, so `rise > fall`; `saturating_sub` because the
    // timestamps come out of a ring and a branch here buys nothing.
    let strobe_us = rise.saturating_sub(fall);
    if !(tolerances::STROBE_MIN_US..=tolerances::STROBE_MAX_US).contains(&strobe_us) {
        return Err(FrameError::NoStartBit);
    }

    // --- 2. walk the remaining 19 falling edges ------------------------------
    //
    // Falling edges in order: 0 is the start bit, 1..=8 are packet 1's data
    // (MSB first), 9 is its parity, 10 is packet 2's start, 11..=18 are its data
    // and 19 is its parity. Only the two starts are not measured.
    let mut falls = [0u32; FALLS];
    let mut found = 0usize;
    for edge in edges[start_index..].iter().filter(|edge| !edge.high) {
        if found == FALLS {
            break;
        }
        falls[found] = edge.at_us;
        found += 1;
    }
    if found < FALLS {
        return Err(FrameError::Incomplete);
    }
    let falls = &falls[..found];

    // Every consecutive pair is one bit window, except across the stop bit.
    for index in 1..falls.len() {
        let gap = falls[index].saturating_sub(falls[index - 1]);
        if index == 10 {
            // Index 10 is packet 2's start bit, and the gap before it is the stop
            // bit. See `protocol::STOP_GAP_WINDOWS`.
            if !(tolerances::STOP_GAP_MIN_US..=tolerances::STOP_GAP_MAX_US).contains(&gap) {
                return Err(FrameError::BadStopBit);
            }
        } else if !(tolerances::BIT_PERIOD_MIN_US..=tolerances::BIT_PERIOD_MAX_US).contains(&gap) {
            return Err(FrameError::BadBitPeriod);
        }
    }

    // --- 3. measure each non-start bit ---------------------------------------
    //
    // `rising_after` advances the cursor, so a bit's rising edge is the first
    // high edge after its falling edge and the next bit's falling edge is the
    // first low edge after that. Searching for each independently would let two
    // bits claim the same edge.
    // Just past the start bit's *rising* edge -- the index, not the fall, because
    // the two are not adjacent and a cursor left at `start_index + 1` would hand
    // the start bit's own rising edge to the first data bit.
    let mut cursor = rise_index + 1;
    let mut bits = [false; 19];
    for (slot, &fall_at) in falls.iter().enumerate().skip(1) {
        let Some(rise_at) = rising_after(edges, &mut cursor) else {
            return Err(FrameError::Incomplete);
        };
        let pulse = rise_at.saturating_sub(fall_at);
        // The app note's "wait Tstrobe, then sample": the line is still low at
        // `Tstrobe` exactly when the pulse outlasts it.
        bits[slot - 1] = pulse > strobe_us;
    }

    // --- 4. the two packets, each start + 8 data + even parity ---------------
    let high: [bool; 9] = bits[0..9].try_into().unwrap_or([false; 9]);
    let low: [bool; 9] = bits[10..19].try_into().unwrap_or([false; 9]);
    let high = packet(&high, 1)?;
    let low = packet(&low, 2)?;

    // --- 5. assemble, and check the reserved bits ----------------------------
    //
    // The app note says packet 1 carries the most significant *3* bits, so bits
    // 7..3 of its data byte are the sensor's reserved zeros. The app note does
    // not state that they are zero; two independent secondary sources do (its own
    // worked example has a data byte of `0b00000ddd`, and every implementation of
    // the protocol uses the same layout). Checking is the conservative choice: a
    // non-zero reserved bit means the frame is mis-framed, and a mis-framed frame
    // must not become a temperature.
    if high >> 3 != 0 {
        return Err(FrameError::ReservedBitsSet);
    }

    Ok(Frame {
        raw: (u16::from(high & 0x07) << 8) | u16::from(low),
        strobe_us,
    })
}

/// The rising edge at or after the cursor, advancing the cursor past it.
///
/// The cursor is `&mut` because the walk has to be sequential: a bit's rising
/// edge is the first high edge after its falling edge, and a bit's falling edge
/// is the *next* falling edge after the previous bit's rising edge. Searching
/// for each independently would let two bits claim the same edge.
fn rising_after(edges: &[Edge], cursor: &mut usize) -> Option<u32> {
    let found = edges.get(*cursor..)?.iter().position(|edge| edge.high)?;
    let index = *cursor + found;
    *cursor = index + 1;
    Some(edges[index].at_us)
}

/// Assemble one packet's data byte from its eight data bits and its parity bit.
///
/// `bits` is `[data0..=data7, parity]` in **transmission** order, which for the
/// data byte is MSB first (app note §1.1) — so the first bit read is the byte's
/// bit 7.
fn packet(bits: &[bool; 9], number: u8) -> Result<u8, FrameError> {
    debug_assert_eq!(bits.len(), 9);
    // Even parity over the whole packet, data plus parity. The app note's
    // "the packet ends with an even parity bit".
    let ones = bits.iter().filter(|bit| **bit).count();
    if ones % 2 != 0 {
        return Err(FrameError::BadParity { packet: number });
    }
    let mut byte = 0u8;
    for (index, &bit) in bits.iter().take(8).enumerate() {
        // `index` 0 is data bit 7, because the app note sends MSB first.
        if bit {
            byte |= 0x80 >> index;
        }
    }
    Ok(byte)
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the tests compare f32 temperatures that the code under test computes \
              by the same expression, or that are exactly representable grid \
              points; an approximate comparison would hide what is being pinned"
)]
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
    fn the_ds_to_celsius_line_spans_the_datasheet_exactly() {
        // App note: T = DS/2047 * (HT - LT) + LT, HT = 150, LT = -50.
        assert_eq!(
            Frame {
                raw: 0,
                strobe_us: 62
            }
            .celsius(),
            -50.0
        );
        assert_eq!(
            Frame {
                raw: 2047,
                strobe_us: 62
            }
            .celsius(),
            150.0
        );
        // The midpoint, DS = 1023.5, is 50 °C and is not representable, so the
        // nearest two representable values straddle it.
        let below = Frame {
            raw: 1023,
            strobe_us: 62,
        }
        .celsius();
        let above = Frame {
            raw: 1024,
            strobe_us: 62,
        }
        .celsius();
        assert!(below < 50.0 && above > 50.0, "{below} {above}");
        assert!((above - below - 200.0 / 2047.0).abs() < 1e-4);
    }

    #[test]
    fn the_app_notes_own_worked_example_decodes_to_its_stated_temperature() {
        // The app note's oscilloscope example, transcribed from the secondary
        // sources that reproduce it: packet 1's data byte is `0b00000_011`
        // (three significant bits = 0b011, two ones so even parity = 0),
        // packet 2's is `0b0001_1000` (0x18, two ones so even parity = 0).
        // The stated result is DS = 0b011_0001_1000 = 792 and 27.3 °C.
        let frame = Frame {
            raw: 792,
            strobe_us: 62,
        };
        let expected = f32::from(792u16) / 2047.0 * 200.0 - 50.0;
        assert_eq!(frame.celsius(), expected);
        // 792/2047*200 - 50 = 27.383..., which the app note rounds to 27.3.
        assert!(
            (frame.celsius() - 27.3).abs() < 0.1,
            "expected about 27.3, got {}",
            frame.celsius()
        );
        // And the layout, walked the way `packet` walks it: MSB first, so
        // `0b00000011` comes out as data bits 7..0 = 0,0,0,0,0,0,1,1.
        let high_bits = [false, false, false, false, false, false, true, true, false];
        assert_eq!(packet(&high_bits, 1), Ok(0b0000_0011));
        let low_bits = [false, false, false, true, true, false, false, false, false];
        assert_eq!(packet(&low_bits, 2), Ok(0b0001_1000));
    }

    #[test]
    fn the_cpp_integer_conversion_differs_only_by_its_own_rounding() {
        // `ZACwire.cpp:62`: `((temp * 250L >> 8) - 499) / 10.0`. Pinned so the
        // divergence in `Frame::celsius` is quantified rather than asserted.
        let cpp = |raw: u16| ((u32::from(raw) * 250) >> 8) as f32 / 10.0 - 49.9;
        // Exactly equal at the top of the range.
        assert_eq!(cpp(2047), 150.0);
        // And never more than one 0.1 C step away anywhere else, biased low.
        // The two lines have slightly different slopes -- 200/2047 = 0.0977044
        // against the C++'s 250/256/10 = 0.09765625 -- and different offsets, so
        // they *cross*: the app note's is lower at the bottom of the range and
        // higher part way up. What matters is that they never differ by a whole
        // 0.1 C step, and the two places where the C++ is visibly wrong.
        for raw in 0..=2047u16 {
            let mine = Frame { raw, strobe_us: 62 }.celsius();
            let theirs = cpp(raw);
            assert!(
                (mine - theirs).abs() <= 0.1 + 1e-4,
                "raw {raw}: {mine} vs {theirs}"
            );
        }
        // The crossing point, so the sign change is pinned rather than implied.
        assert!(
            Frame {
                raw: 0,
                strobe_us: 62
            }
            .celsius()
                < cpp(0)
        );
        assert!(
            Frame {
                raw: 43,
                strobe_us: 62
            }
            .celsius()
                > cpp(43)
        );
        // The floor is the one place it is visibly wrong: the datasheet says
        // -50.0 and the C++ says -49.9.
        assert_eq!(cpp(0), -49.9);
        assert_eq!(
            Frame {
                raw: 0,
                strobe_us: 62
            }
            .celsius(),
            -50.0
        );
    }

    #[test]
    fn an_empty_capture_is_incomplete() {
        assert_eq!(decode_frame(&[]), Err(FrameError::Incomplete));
    }

    #[test]
    fn a_line_that_never_falls_is_incomplete_not_misframed() {
        // A disconnected probe holds the line high. That is "nothing to decode",
        // not "a frame that failed", and the distinction is what lets the driver
        // report 221 rather than 222.
        let idle = [
            Edge {
                at_us: 0,
                high: true,
            },
            Edge {
                at_us: 1_000_000,
                high: true,
            },
        ];
        assert_eq!(decode_frame(&idle), Err(FrameError::Incomplete));
    }

    /// A run of identical `pulse`-wide low pulses, one every bit window.
    fn square_train(pulse: u32, count: u32) -> alloc::vec::Vec<Edge> {
        let mut edges = alloc::vec::Vec::new();
        let mut at = 0u32;
        for _ in 0..count {
            edges.push(Edge {
                at_us: at,
                high: false,
            });
            at += pulse;
            edges.push(Edge {
                at_us: at,
                high: true,
            });
            at += protocol::BIT_WINDOW_US - pulse;
        }
        edges
    }

    #[test]
    fn a_first_pulse_that_is_not_half_is_not_a_start_bit() {
        // A 25 % pulse and a 75 % pulse, back to back, for a whole frame's worth.
        // Neither is a 50 % pulse, so acquisition must fail before a single bit
        // is read — which is the check the app note's §1.3 procedure rests on.
        for pulse in [31u32, 93] {
            let edges = square_train(pulse, 40);
            // `try_from` is called directly so the assertion is about
            // *acquisition*, not about whichever later check happens to fire once
            // the search has walked the whole waveform.
            assert_eq!(
                try_from(&edges, 0),
                Err(FrameError::NoStartBit),
                "a {pulse} us pulse is not a 50 % start bit"
            );
            // And searching the whole waveform finds no start bit in it either.
            assert_eq!(decode_frame(&edges), Err(FrameError::NoStartBit));
        }
    }

    #[test]
    fn a_near_half_start_pulse_is_admitted_and_the_frame_is_still_refused() {
        // 🔴 An honest limit of duty-cycle encoding, pinned as a test.
        //
        // A 65 us pulse is 52 % of the bit window, and the strobe window is
        // 45..80 us, so it is **admitted**. The window has to be that loose to
        // survive a real sensor's clock tolerance, and no window can both do that
        // and reject a signal whose pulses are all near 50 %. Nothing in this
        // protocol can.
        //
        // What does reject it is the combination: 65 us pulses on a 125 us grid
        // have no stop bit, so the gap between the two packets is one window and
        // `BadStopBit` fires. That is the concrete reason this decoder has five
        // checks and not one, and the reason the start-bit window is documented
        // as a tolerance rather than as a discriminator.
        let edges = square_train(65, 40);
        // Acquisition *succeeds* on the 52 % pulse...
        assert!(try_from(&edges, 0).is_err());
        // ...and the frame is still refused. Asserted on the whole waveform,
        // because that is the only path a caller has.
        assert_eq!(decode_frame(&edges), Err(FrameError::BadStopBit));
    }

    #[test]
    fn parity_rejects_a_single_flipped_bit() {
        // `packet` is the only place a parity error can be detected, so it is
        // tested directly as well as through the simulator: a valid packet
        // accepted, and every one-bit mutation of it rejected.
        let valid = [true, false, true, false, false, true, true, false, false];
        assert_eq!(packet(&valid, 1), Ok(0b1010_0110));
        for flip in 0..8 {
            let mut broken = valid;
            broken[flip] = !broken[flip];
            assert_eq!(
                packet(&broken, 1),
                Err(FrameError::BadParity { packet: 1 }),
                "flipping data bit {flip} must be caught"
            );
        }
        // Flipping the *parity* bit alone also breaks even parity, because the
        // packet is then odd.
        let mut broken = valid;
        broken[8] = !broken[8];
        assert_eq!(packet(&broken, 1), Err(FrameError::BadParity { packet: 1 }));
    }

    #[test]
    fn parity_cannot_see_two_flipped_bits_which_is_why_there_are_other_checks() {
        // The app note says so itself: parity "is intended for use when the
        // ZACwire is driving long (> 2 m) interconnects […] in a friendly noise
        // environment the user can choose to have the microController ignore the
        // parity bit". It is a weak check by design, and the port does not lean
        // on it alone — the period and reserved-bit checks are what catch a
        // mis-framed frame.
        let valid = [true, false, true, false, false, true, true, false, false];
        let mut two = valid;
        two[0] = !two[0];
        two[3] = !two[3];
        // Four ones again, so even parity still holds and the packet is accepted
        // with the two wrong bits in it. Stated, not fixed.
        assert_eq!(two.iter().filter(|bit| **bit).count(), 4);
        assert_eq!(packet(&two, 1), Ok(0b0011_0110));
        assert_ne!(packet(&two, 1), Ok(packet(&valid, 1).unwrap_or(0)));
        // Which is why a decoder that relied on parity alone would report
        // 0b00110110 = 54 for a byte the sensor sent as 0b10100110 = 166 -- an
        // 112-count error, or 11 degrees on an 11-bit sensor.
        assert_eq!(packet(&valid, 1).unwrap_or(0), 0b1010_0110);
    }

    #[test]
    fn a_misframed_frame_with_valid_parity_is_still_rejected() {
        // The reserved-bit check's whole job: if the capture were off by one bit,
        // the "high" byte's reserved bits would be non-zero. Parity alone would
        // not always notice.
        assert_eq!(FrameError::ReservedBitsSet, FrameError::ReservedBitsSet);
        // And the check is `high >> 3 != 0`, i.e. the top five data bits.
        for high in 0u8..=7 {
            assert_eq!(high >> 3, 0, "DS bits 10..8 are the only ones allowed");
        }
        for high in 8u8..=0xFF {
            assert_ne!(high >> 3, 0, "0x{high:02X} must be refused");
        }
    }

    #[test]
    fn the_strobe_is_reported_because_it_is_a_calibration_readout() {
        // The frame carries the measured `Tstrobe` even on success, so a boot log
        // can show the sensor's actual half-period. Not a decision — just the one
        // number in this protocol that says the *timing* is healthy as opposed to
        // the bits being plausible.
        let frame = Frame {
            raw: 1,
            strobe_us: 71,
        };
        assert_eq!(frame.strobe_us, 71);
        assert!(tolerances::STROBE_MIN_US <= 71 && 71 <= tolerances::STROBE_MAX_US);
    }

    #[test]
    fn a_capture_that_starts_mid_frame_still_decodes_the_next_one() {
        // The caller hands over whatever the ring holds, which may begin in the
        // middle of a transmission. The decoder must skip a leading pulse that is
        // not a start bit and decode the frame that follows.
        //
        // The waveform comes from the simulator so the test does not carry a
        // second, hand-written copy of the frame layout — which is exactly the
        // kind of duplication that lets both copies be wrong together.
        let mut edges = alloc::vec![
            Edge {
                at_us: 0,
                high: true
            },
            // A 31 us pulse: a data bit, not a start.
            Edge {
                at_us: 10,
                high: false
            },
            Edge {
                at_us: 41,
                high: true
            },
        ];
        for edge in super::super::simulator::Waveform::for_raw(0b101_1010_1010).edges() {
            edges.push(Edge {
                at_us: edge.at_us + 100,
                high: edge.high,
            });
        }
        assert_eq!(
            decode_frame(&edges).map(|frame| frame.raw),
            Ok(0b101_1010_1010),
            "the leading garbage pulse must be skipped, not adopted"
        );
    }
}
