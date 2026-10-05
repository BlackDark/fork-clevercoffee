//! A fixed-size, lock-free, single-producer/single-consumer edge ring.
//!
//! Owner: **R3-07**.
//!
//! # This is the whole of what the interrupt does
//!
//! The app note recommends a falling-edge ISR that measures `Tstrobe` and then
//! *waits* for the next nine edges, sampling after each (app note §1.4). That is
//! a bit-sampling ISR, and [04 §3.1](../../../../../docs/history/target-architecture.md)
//! constrains this firmware's ISRs to "nothing beyond one GPIO write". The two
//! cannot both be satisfied, and the constraint wins, so the split is:
//!
//! | | where | what it does |
//! | --- | --- | --- |
//! | interrupt | ISR | read the pin, read the clock, store one `u64` in this ring, bump one index |
//! | decode | a task | drain the ring, measure `Tstrobe`, assemble the frame, check parity, convert |
//!
//! The decode is a pure function of a list of edge timestamps, which is the
//! property that makes the whole protocol host-testable: [`EdgeRing`] is fed
//! from a test array in `cc-domain` and from a GPIO interrupt on the device,
//! with nothing in between.
//!
//! # Safety and memory ordering, without `unsafe`
//!
//! The workspace denies `unsafe_code`, so this is built from atomics rather than
//! from `UnsafeCell` + `static mut`:
//!
//! * `write` is written **only** by the producer (the sampler) and `read` **only**
//!   by the consumer (the decode task). One writer each, so each index needs no
//!   compare-and-swap — only a release store to publish.
//! * A slot is written with a [`Ordering::Release`] store and read with an
//!   [`Ordering::Acquire`] load, so a consumer that has seen index `w` is
//!   guaranteed to see the slot's contents.
//! * The indices are monotonic and never shrink, so a full ring is
//!   `write.wrapping_sub(read) == CAP` and does not need a separate count. The
//!   index arithmetic is `usize`, which is 32-bit on the target — and the wrap is
//!   harmless for the reason in [`Edge::at_us`].
//!
//! # 🔴 One `AtomicU32` per slot, not an `AtomicU64`, and why that is forced
//!
//! An edge is a 32-bit timestamp and a level, so a slot could be an
//! `AtomicU64` — 768 bytes for the ring, one 64-bit store per edge, and no
//! possibility of a torn read. **The original ESP32 does not have
//! `core::sync::atomic::AtomicU64`** (Xtensa is 32-bit and the type is not
//! offered), so this uses [`AtomicU32`] with the level in bit 31 and the
//! timestamp in bits 0..30. A test in this module pins that the original ESP32
//! does not have it, so the packing cannot be "optimised" back to a `u64` by
//! someone building for a 64-bit host.
//!
//! The cost is a 31-bit timestamp: **35.8 minutes** instead of 71.6, and a wrap
//! every 35.8 minutes. That is harmless here and the argument is worth stating,
//! because it is not obviously harmless:
//!
//! * every consumer of a timestamp is a `saturating_sub` between two edges of the
//!   *same* transmission, which is at most 2.75 ms long;
//! * a wrap inside a burst would therefore produce an interval of ~2100 seconds,
//!   which is outside every tolerance window in [`crate::sensor::tsic306::protocol`]
//!   and is **rejected**;
//! * so a wrap costs at most one dropped frame every 35.8 minutes, and can never
//!   produce a wrong temperature. A 32-bit unsigned microsecond counter that
//!   wraps is a monotonic reset, and every check in the decoder is a bound.
//!
//! # Overflow behaviour, stated once and pinned by a test
//!
//! **Overflow discards the in-flight frame; it never wraps.**
//!
//! When the producer finds the ring full it does **not** overwrite a slot the
//! consumer has not read. It increments the dropped-edge count and sets the sticky
//! overrun flag, and the frame that was in progress is lost. The
//! consumer sees the flag, resets its index to the producer's — which drops the
//! partial frame — clears the flag, and reports
//! `FrameError::RingOverrun` for that transmission.
//!
//! The alternative — overwriting the oldest edge — was rejected because the
//! result would be a *plausible* temperature assembled from a waveform that never
//! existed. On a temperature input that feeds emergency stop, "no reading" is a
//! strictly better answer than "a reading that is wrong in an unpredictable
//! direction".
//!
//! The capacity is chosen so overflow cannot happen in normal operation. A
//! transmission is [`TRANSMISSION_BITS`](super::protocol::TRANSMISSION_BITS) (20)
//! falling edges plus 20 rising edges = **40** edges, in 2.75 ms
//! (21 windows x 125 µs, the stop bit included). [`CAPACITY`] is **96**, which is
//! 2.4 transmissions: enough that the consumer has two whole transmission
//! periods of slack, and small enough that the whole ring is 768 bytes.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// The bit of a packed slot that holds the level.
const LEVEL_BIT: u32 = 1 << 31;

/// The bits of a packed slot that hold the timestamp: 31 of them, i.e. 35.8
/// minutes of microseconds.
const TIMESTAMP_MASK: u32 = LEVEL_BIT - 1;

/// The number of edges the ring holds.
///
/// 96 x 8 bytes = 768 bytes of `.bss`. See the module docs for the derivation.
pub const CAPACITY: usize = 96;

/// One transition on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    /// Microseconds since the capture started, from `esp_timer_get_time()`,
    /// **truncated to 31 bits** so it fits alongside the level in one
    /// [`AtomicU32`].
    ///
    /// Microseconds, not nanoseconds, because that is the clock's resolution
    /// and a nanosecond field would imply a precision that does not exist. The
    /// app note asks for 7.8 µs and this is 1 µs; see
    /// [`protocol::STROBE_SAMPLE_RATE_HZ`](super::protocol::STROBE_SAMPLE_RATE_HZ).
    ///
    /// 31 bits is 35.8 minutes, and a wrap is a loss of at most one frame and
    /// never a wrong reading — see the module docs.
    pub at_us: u32,
    /// The level **after** the transition: `true` is the line's idle high.
    pub high: bool,
}

/// A fixed-capacity SPSC ring of [`Edge`]s, safe to share between an ISR and a
/// task.
///
/// `const N` so a caller can size it for its own duty cycle; the tests use small
/// ones to reach the overflow path without pushing 100 entries by hand.
pub struct EdgeRing<const N: usize> {
    slots: [AtomicU32; N],
    /// Next slot the producer will write. Producer-only.
    write: AtomicUsize,
    /// Next slot the consumer will read. Consumer-only.
    read: AtomicUsize,
    /// How many edges were refused because the ring was full. Diagnostic.
    dropped: AtomicUsize,
    /// Sticky: the consumer clears it after resynchronising.
    overrun: AtomicU32,
}

impl<const N: usize> EdgeRing<N> {
    /// An empty ring, ready to be a `static`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [const { AtomicU32::new(0) }; N],
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
            overrun: AtomicU32::new(0),
        }
    }

    /// Store one edge. **The only thing the interrupt calls.**
    ///
    /// # Errors
    ///
    /// [`RingFull`] if the ring is full. The edge is **not** stored and the
    /// overrun flag is set; see the module docs for why it is dropped rather than
    /// overwriting.
    #[inline]
    pub fn push(&self, at_us: u32, high: bool) -> Result<(), RingFull> {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        if write.wrapping_sub(read) >= N {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            self.overrun.store(1, Ordering::Release);
            return Err(RingFull);
        }
        // Level in bit 31, timestamp in bits 0..30. One `u32` store, so the
        // consumer cannot observe the two fields torn against each other -- which
        // is the whole reason they are packed and not two atomics.
        let packed = if high {
            LEVEL_BIT | (at_us & TIMESTAMP_MASK)
        } else {
            at_us & TIMESTAMP_MASK
        };
        self.slots[write % N].store(packed, Ordering::Release);
        self.write.store(write.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Take the next edge. **Consumer only.**
    ///
    /// Returns `None` when the ring is empty, or when the overrun flag is set —
    /// in which case the ring is resynchronised to the producer's index and
    /// [`Self::take_overrun`] reports it once.
    pub fn pop(&self) -> Option<Edge> {
        if self.overrun.load(Ordering::Acquire) != 0 {
            // Drop the partial frame rather than decode a waveform that has a
            // hole in it. Advancing `read` to `write` empties the ring; the
            // producer only ever moves `write` forward, so this cannot lose an
            // edge that arrives after it.
            let write = self.write.load(Ordering::Acquire);
            self.read.store(write, Ordering::Release);
            return None;
        }
        let read = self.read.load(Ordering::Relaxed);
        if read == self.write.load(Ordering::Acquire) {
            return None;
        }
        let packed = self.slots[read % N].load(Ordering::Acquire);
        self.read.store(read.wrapping_add(1), Ordering::Release);
        Some(Edge {
            at_us: packed & TIMESTAMP_MASK,
            high: (packed & LEVEL_BIT) != 0,
        })
    }

    /// Consume the overrun flag, reporting whether a frame was lost.
    ///
    /// The consumer calls this after [`Self::pop`] has returned `None` on a
    /// flagged ring, which is how a lost frame becomes a
    /// `FrameError::RingOverrun` rather than a
    /// silent "no data".
    pub fn take_overrun(&self) -> bool {
        self.overrun.swap(0, Ordering::AcqRel) != 0
    }

    /// How many edges have been refused because the ring was full.
    ///
    /// The C++'s equivalent is the fact that it has no equivalent: `ZACwire`
    /// never runs out of room because its buffer is two `uint16_t`s that it
    /// overwrites, so a missed edge is silently decoded as a bit. This counter is
    /// the thing that makes the alternative visible.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    /// How many edges are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.write
            .load(Ordering::Acquire)
            .wrapping_sub(self.read.load(Ordering::Acquire))
    }

    /// Whether the ring is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The number of edges a full transmission produces.
    ///
    /// Two packets of ten bits, each bit contributing a falling and a rising
    /// edge, and the stop bit contributing none. `2 * 2 * PACKET_BITS`.
    #[must_use]
    pub const fn edges_per_transmission() -> usize {
        // `PACKET_BITS` is a `u8`; this is a `const fn` and `usize::from` is not
        // const on this toolchain, so the value is spelled out and asserted
        // against the constant by the test below.
        2 * 2 * 10
    }
}

impl<const N: usize> Default for EdgeRing<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// The ring was full. See the module docs for the policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingFull;

/// A caller-owned, fixed-capacity buffer of drained edges.
///
/// # Why this exists instead of returning a `Vec`
///
/// `cc-domain` is `no_std` **and** `no_alloc` in library code (04 §1), and the
/// reason is the target: the ESP32 has roughly 320 KB of RAM in total and
/// AGENTS.md's heap rules are about not spending it. A `ZACwire` transmission is
/// 40 edges, so a `[Edge; 96]` is 768 bytes of stack — the decode task's 8 KB
/// stack has it — against a heap allocation on every 100 ms tick forever, which
/// fragments and is never the thing you want to have found at 3 a.m.
///
/// The buffer is *not* zeroed on construction beyond what the array
/// initialisation does, because every element is written before it is read:
/// [`Self::len`] is the only authority.
#[derive(Clone, Debug)]
pub struct EdgeBuffer {
    edges: [Edge; CAPACITY],
    len: usize,
}

impl EdgeBuffer {
    /// An empty buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            edges: [const {
                Edge {
                    at_us: 0,
                    high: true,
                }
            }; CAPACITY],
            len: 0,
        }
    }

    /// Forget every edge, keeping the allocation.
    pub const fn clear(&mut self) {
        self.len = 0;
    }

    /// Append one edge, saturating rather than growing.
    ///
    /// Saturating is the right failure here: a buffer that is already full means
    /// the ring is not being drained, which is a bug in the caller's cadence, and
    /// silently reallocating would hide it. The edge that does not fit is
    /// dropped, and the loss is visible because the decode then fails.
    pub fn push(&mut self, edge: Edge) {
        if self.len < CAPACITY {
            self.edges[self.len] = edge;
            self.len += 1;
        }
    }

    /// The edges, oldest first.
    #[must_use]
    pub fn as_slice(&self) -> &[Edge] {
        &self.edges[..self.len]
    }

    /// How many edges are held.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the buffer is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for EdgeBuffer {
    fn default() -> Self {
        Self::new()
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
    use crate::sensor::tsic306::protocol;

    #[test]
    fn a_new_ring_is_empty() {
        let ring = EdgeRing::<8>::new();
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
        assert_eq!(ring.pop(), None);
        assert_eq!(ring.dropped(), 0);
    }

    #[test]
    fn edges_come_back_in_the_order_they_went_in() {
        let ring = EdgeRing::<8>::new();
        for at_us in 0..8u32 {
            assert_eq!(ring.push(at_us, at_us % 2 == 0), Ok(()));
        }
        for at_us in 0..8u32 {
            let edge = ring.pop().expect("an edge");
            assert_eq!(edge.at_us, at_us);
            assert_eq!(edge.high, at_us % 2 == 0);
        }
        assert_eq!(ring.pop(), None);
    }

    #[test]
    fn a_full_ring_refuses_rather_than_overwrites() {
        // The safety property: a refused edge is a *lost frame*, not a wrong
        // reading. If this ever overwrote, the oldest edge would be lost
        // silently and the frame would decode to a plausible temperature.
        let ring = EdgeRing::<4>::new();
        for at_us in 0..4u32 {
            assert_eq!(ring.push(at_us, true), Ok(()));
        }
        assert_eq!(ring.len(), 4);
        assert_eq!(ring.push(4, true), Err(RingFull));
        assert_eq!(ring.dropped(), 1, "the refusal is counted");
        assert_eq!(ring.len(), 4, "nothing was stored");
        // `pop` refuses to hand out edges from a ring that has overrun, and says
        // so once. That is the mechanism by which a lost frame becomes "no
        // reading" rather than a reading assembled from a hole.
        assert_eq!(ring.pop(), None);
        assert!(ring.take_overrun());
        // The four that went in *before* the overflow are still intact in order;
        // it is the in-flight frame that is lost, not the history.
        assert!(ring.is_empty(), "the drain resynchronised to the producer");
        for at_us in 10..14u32 {
            assert_eq!(ring.push(at_us, false), Ok(()));
        }
        for at_us in 10..14u32 {
            assert_eq!(ring.pop().map(|e| e.at_us), Some(at_us));
        }
    }

    #[test]
    fn an_overrun_resynchronises_and_is_reported_once() {
        let ring = EdgeRing::<4>::new();
        for at_us in 0..4u32 {
            assert_eq!(ring.push(at_us, true), Ok(()));
        }
        assert_eq!(ring.push(99, true), Err(RingFull));
        // The consumer sees the flag, drops the partial frame...
        assert_eq!(ring.pop(), None, "no edge is handed out mid-overrun");
        assert!(ring.take_overrun(), "and is told why");
        assert!(!ring.take_overrun(), "exactly once");
        // ...and the ring is usable again, with the *next* frame intact.
        assert!(ring.is_empty());
        for at_us in 10..14u32 {
            assert_eq!(ring.push(at_us, false), Ok(()));
        }
        for at_us in 10..14u32 {
            assert_eq!(ring.pop().map(|e| e.at_us), Some(at_us));
        }
    }

    #[test]
    fn a_buffer_hands_back_what_was_pushed_in_order() {
        let mut buffer = EdgeBuffer::new();
        assert!(buffer.is_empty());
        let pushed = [
            Edge {
                at_us: 5,
                high: true,
            },
            Edge {
                at_us: 60,
                high: false,
            },
            Edge {
                at_us: 190,
                high: true,
            },
            Edge {
                at_us: 300,
                high: false,
            },
        ];
        for edge in pushed {
            buffer.push(edge);
        }
        assert_eq!(buffer.len(), 4);
        assert_eq!(buffer.as_slice(), pushed);
    }

    #[test]
    fn a_full_buffer_saturates_rather_than_growing() {
        // 40 edges is a transmission; CAPACITY is 96. A buffer that has already
        // overflowed means the ring is not being drained, and the failure has to
        // be visible in the decode rather than papered over with a realloc.
        let mut buffer = EdgeBuffer::new();
        for at_us in 0..(CAPACITY as u32 + 10) {
            buffer.push(Edge { at_us, high: true });
        }
        assert_eq!(buffer.len(), CAPACITY);
        assert_eq!(
            buffer.as_slice().last().map(|e| e.at_us),
            Some(CAPACITY as u32 - 1)
        );
    }

    #[test]
    fn clearing_a_buffer_keeps_its_capacity() {
        let mut buffer = EdgeBuffer::new();
        for at_us in 0..10u32 {
            buffer.push(Edge { at_us, high: false });
        }
        buffer.clear();
        assert!(buffer.is_empty());
        buffer.push(Edge {
            at_us: 42,
            high: true,
        });
        assert_eq!(
            buffer.as_slice(),
            [Edge {
                at_us: 42,
                high: true
            }]
        );
    }

    #[test]
    fn the_drop_counter_accumulates_across_frames() {
        // A persistent overrun must be visible as a growing number, not as a
        // single sticky bit somebody clears once.
        let ring = EdgeRing::<2>::new();
        for round in 0..5u32 {
            for offset in 0..2u32 {
                assert_eq!(ring.push(round * 10 + offset, true), Ok(()));
            }
            assert_eq!(ring.push(999, true), Err(RingFull));
            assert_eq!(ring.pop(), None, "no edges from an overrun ring");
            assert!(ring.take_overrun());
        }
        assert_eq!(ring.dropped(), 5);
    }

    #[test]
    fn the_capacity_holds_two_and_a_bit_transmissions() {
        // The derivation in the module docs, as an assertion: if a transmission
        // ever needed more than CAPACITY/2 edges, "two and a half transmissions of
        // slack" would be a lie.
        let per_transmission = EdgeRing::<1>::edges_per_transmission();
        assert_eq!(protocol::PACKET_BITS, 10);
        assert_eq!(per_transmission, 2 * 2 * usize::from(protocol::PACKET_BITS));
        assert_eq!(per_transmission, 40);
        assert!(
            CAPACITY >= 2 * per_transmission,
            "capacity {CAPACITY} must hold at least two transmissions of {per_transmission}"
        );
        assert!(
            CAPACITY < 3 * per_transmission,
            "and not so much that a stuck consumer is never noticed"
        );
    }

    #[test]
    fn a_whole_transmission_fits_without_a_single_refusal() {
        // 40 edges, the real count, into the real capacity.
        let ring = EdgeRing::<CAPACITY>::new();
        #[allow(clippy::cast_possible_truncation, reason = "TRANSMISSION_BITS is 20")]
        let count = 2 * u32::from(protocol::TRANSMISSION_BITS);
        for at_us in 0..count {
            assert_eq!(ring.push(at_us, at_us % 2 == 1), Ok(()));
        }
        assert_eq!(ring.dropped(), 0, "no edge of a transmission is ever lost");
        for at_us in 0..count {
            assert_eq!(ring.pop().map(|e| e.at_us), Some(at_us));
        }
    }

    #[test]
    fn the_level_is_packed_without_truncating_the_timestamp() {
        // The two fields share one atomic store, so a torn read would be
        // catastrophic rather than merely wrong. Round-trip both levels against
        // timestamps either side of a power-of-two boundary, which is where a
        // packing bug would show.
        let ring = EdgeRing::<4>::new();
        for (at_us, high) in [
            (0u32, true),
            (1, false),
            (TIMESTAMP_MASK, true),
            (0x4000_0000, false),
            (TIMESTAMP_MASK, false),
        ] {
            assert_eq!(ring.push(at_us, high), Ok(()));
            let edge = ring.pop().expect("an edge");
            assert_eq!(edge.at_us, at_us, "timestamp round trip");
            assert_eq!(edge.high, high, "level round trip");
        }
    }

    #[test]
    fn the_timestamp_is_thirty_one_bits_because_the_target_has_no_atomic_u64() {
        // `core::sync::atomic::AtomicU64` does not exist on the original ESP32:
        // Xtensa is 32-bit and the type is not offered. This is a `compile_fail`
        // test in spirit -- the packing exists because of it -- and the constant
        // is asserted so the reason cannot be quietly forgotten.
        const _: () = assert!(TIMESTAMP_MASK == 0x7FFF_FFFF);
        const _: () = assert!(LEVEL_BIT == 0x8000_0000);
        // 31 bits of microseconds.
        assert_eq!(u64::from(TIMESTAMP_MASK) + 1, 2_147_483_648);
        assert_eq!(2_147_483_648_u64 / 1_000_000, 2_147, "seconds before wrap");
    }

    #[test]
    fn a_timestamp_wrap_costs_a_frame_and_never_a_reading() {
        // The claim the 31-bit packing rests on, as a test: a wrapped timestamp
        // produces an interval far outside every tolerance window, so the
        // decoder refuses the frame. Pinned here on the *arithmetic*, because the
        // decoder's own reaction is covered in `decode`'s tests.
        let before = TIMESTAMP_MASK;
        let after = 5u32; // just past the wrap
        let interval = after.wrapping_sub(before);
        assert!(
            interval > 1_000_000,
            "a wrap must not look like a short interval"
        );
        // And the decoder's widest window is 312 us, so this is 3000x outside it.
        assert!(interval > 312);
    }
}
