//! Polled `ZACwire` edge capture; no TSIC-306 hardware validation yet.
//!
//! GPIO16 currently carries DS18B20 traffic, so this capture is not brought up.
//! Host waveform tests do not establish that a real TSIC-306 works.
//!
//! The app note recommends at least 128 kHz strobe sampling; [`BURST_POLL_US`] is
//! 7 µs. Poll slowly while idle, then burst until [`BURST_HOLD_US`] after the last
//! edge. Nominal transmissions last 2.75 ms every 100 ms, limiting busy polling.
//!
//! The HAL has no safe timestamped GPIO callback. PCNT reports counts, not times,
//! and rearming per edge creates a loss window. Polling avoids a bit-waiting ISR;
//! microsecond timestamps still require the locally justified timer FFI.
//!
//! [`Idle`]: Sampler::Idle

use cc_protocol::sensor::tsic306::ring::{EdgeRing, CAPACITY};
use cc_protocol::sensor::tsic306::{EdgeSource, RingSource};
use esp_idf_hal::delay::Ets;
use esp_idf_hal::gpio::{Input, PinDriver};
use esp_idf_svc::sys::EspError;

/// The fast sampling period, in microseconds: **7 µs ≈ 143 kHz**.
///
/// The app note asks for at least 16x the 8 kHz nominal baud rate, i.e. 128 kHz
/// or 7.8 µs. 7 µs is just inside it. See the module docs for why this is a
/// *sampling rate* and not an interrupt rate.
pub const BURST_POLL_US: u32 = 7;

/// The slow sampling period, in microseconds: 1 ms.
///
/// Nothing is chosen here but its cost. 1 kHz is three orders of magnitude below
/// the burst rate and still catches an edge within 1 ms of it starting, which is
/// 8 bit windows — so the burst begins with at most one bit of the transmission
/// already gone, and one bit of loss at the *front* of a packet is harmless
/// because [`decode_frame`](cc_protocol::sensor::tsic306::decode_frame) scans
/// forward for a start bit.
///
/// A larger value would be cheaper and still work; a smaller one costs more and
/// buys nothing. See `the_idle_poll_rate_costs_under_one_percent_of_a_core`.
pub const IDLE_POLL_US: u32 = 1_000;

/// How long after the last edge the fast sampling continues, in microseconds.
///
/// One transmission is 21 bit windows = 2625 µs (app note §1.4's "2.7 ms"), so
/// 3000 µs covers a whole burst plus margin. Longer would keep burning CPU on
/// the high line after the sensor has finished; shorter would risk dropping the
/// last bits of a transmission that started late.
pub const BURST_HOLD_US: u32 = 3_000;

/// The microsecond clock.
///
/// Re-exported from [`crate::time`], which is where the workspace's two clock
/// and heap `unsafe` calls live and where the reasoning for them is written
/// out once. This module used to own the function; moving it means the `unsafe`
/// is in one place rather than two, and that the `ZACwire` driver and the network
/// tier's timeouts provably use the *same* clock — which matters because the
/// `ZACwire` ring packs an edge timestamp beside its level in one `AtomicU32` and
/// a second, differently-truncated clock would not compose with it.
///
/// 1 µs resolution against the app note's 7.8 µs requirement is **7.8x margin**,
/// and against the 62.5 µs decision boundary it is 1.6 %.
pub use crate::time::now_us;

/// What the sampler is currently doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sampler {
    /// Polling slowly, waiting for the line to move.
    Idle,
    /// Polling at [`BURST_POLL_US`], because an edge was seen.
    Burst,
}

/// Statistics, for the boot log. A `ZACwire` driver that is running and silent
/// should show a non-zero `edges` and a non-zero `frames`; one that shows
/// `dropped` > 0 has lost a frame and said so.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureStats {
    /// Edges the sampler has pushed into the ring.
    pub edges: u32,
    /// Edge transitions seen since the sampler was created.
    pub transitions: u32,
    /// Frames successfully decoded.
    pub frames: u32,
    /// Edges the ring refused because it was full.
    pub dropped: u32,
    /// Polls performed.
    pub polls: u32,
}

/// The capture: a pin, a ring, and the two sampling rates.
///
/// Generic-free and allocation-free. [`capture`](Self::capture) is the whole
/// public surface for the decode loop, and it is where the producer would be
/// swapped for an ISR — see the module docs.
pub struct ZacwireCapture<'d> {
    pin: PinDriver<'d, Input>,
    /// The ring and the drain bookkeeping. The capture owns it, so the pin and
    /// the buffer have one owner each and nothing is shared.
    source: RingSource,
    state: Sampler,
    last_change_us: u32,
    stats: CaptureStats,
}

/// How long [`ZacwireCapture::capture`] samples for when nothing else is asked
/// for: one whole transmission (21 bit windows = 2625 µs) plus margin.
pub const DEFAULT_CAPTURE_US: u32 = 3_000;

impl<'d> ZacwireCapture<'d> {
    /// Take a GPIO as a `ZACwire` line.
    ///
    /// The pin must be the TSIC's open-drain data line, which idles **high**.
    /// It is configured as a plain input with no internal pull: the sensor is a
    /// strong push-pull driver (app note §1.4, "The `ZACwire` line is driven by a
    /// strong CMOS push/pull driver") and an internal pull would only load it.
    ///
    /// # Errors
    ///
    /// Whatever the peripheral reports if the pin cannot be configured as an
    /// input.
    pub fn new(pin: impl esp_idf_hal::gpio::InputPin + 'd) -> Result<Self, EspError> {
        let pin = PinDriver::input(pin, esp_idf_hal::gpio::Pull::Floating)?;
        Ok(Self {
            pin,
            source: RingSource::new(),
            state: Sampler::Idle,
            last_change_us: 0,
            stats: CaptureStats::default(),
        })
    }

    /// The ring the capture fills. A decode task drains this.
    ///
    /// Exposed so that a future ISR-driven capture can publish the *same* ring
    /// to the *same* decoder, which is the entire point of the split.
    #[must_use]
    pub const fn ring(&self) -> &EdgeRing<CAPACITY> {
        self.source.ring()
    }

    /// What the sampler is currently doing.
    #[must_use]
    pub const fn state(&self) -> Sampler {
        self.state
    }

    /// Counters for the boot log.
    #[must_use]
    pub const fn stats(&self) -> CaptureStats {
        self.stats
    }

    /// Sample the line until a frame is complete, or the burst times out.
    ///
    /// Returns the number of edges pushed. Called from the decode task; the
    /// decode itself is in `cc-domain`.
    ///
    /// The loop is bounded three ways, all of them necessary:
    ///
    /// * by [`BURST_HOLD_US`] after the last edge, so a silent line cannot spin;
    /// * by [`MAX_CAPTURE_US`] in total, so a *noisy* line — one that toggles
    ///   forever, which is what an unconnected pin with a floating input does —
    ///   cannot spin either. That case is exactly the one a `ZACwire` decoder must
    ///   survive, so it must not be the one that hangs the task.
    /// * by the ring refusing edges, which sets the overrun flag and is
    ///   reported by the drain.
    pub fn capture(&mut self, max_us: u32) -> u32 {
        const MAX_CAPTURE_US: u32 = 10_000;
        let began = now_us();
        let mut pushed = 0u32;
        let mut previous = self.pin.is_high();

        loop {
            let at = now_us();
            let elapsed = at.wrapping_sub(began);
            if elapsed >= max_us || elapsed >= MAX_CAPTURE_US {
                break;
            }

            let level = self.pin.is_high();
            self.stats.polls = self.stats.polls.wrapping_add(1);

            if level != previous {
                previous = level;
                self.stats.transitions = self.stats.transitions.wrapping_add(1);
                self.last_change_us = at;
                // Entering the burst on the *first* transition is what makes the
                // idle poll rate affordable: 99.7 % of the time there is no edge
                // and the loop is polling at 1 kHz.
                if self.state == Sampler::Idle {
                    self.state = Sampler::Burst;
                }
                if self.source.ring().push(at, level).is_ok() {
                    pushed += 1;
                    self.stats.edges = self.stats.edges.wrapping_add(1);
                } else {
                    // The ring is full. The frame in progress is lost and the
                    // overrun flag says so; nothing is overwritten. See
                    // `cc_protocol::sensor::tsic306::ring`.
                    self.stats.dropped = self.stats.dropped.wrapping_add(1);
                    // A full ring means we are not keeping up at all; stop
                    // sampling rather than spin, and let the drain resynchronise.
                    break;
                }
            } else if self.state == Sampler::Burst
                && at.wrapping_sub(self.last_change_us) >= BURST_HOLD_US
            {
                // The transmission is over. Back to the cheap rate so the other
                // 97 ms of the 100 ms period costs 1 kHz and not 143 kHz.
                self.state = Sampler::Idle;
            }

            let wait = match self.state {
                Sampler::Idle => IDLE_POLL_US,
                Sampler::Burst => BURST_POLL_US,
            };
            // `Ets::delay_us`, not `delay_ns`: `Ets`'s `DelayNs` impl rounds up
            // to the next whole microsecond
            // (`esp-idf-hal-0.47.0/src/delay.rs:250-252`) and 7 812 ns would
            // become 8 us, putting the sample rate at 125 kHz — just *below* the
            // app note's minimum. An integer 7 us is above it.
            Ets::delay_us(wait);
        }

        pushed
    }

    /// How many edges the ring refused because it was full.
    ///
    /// Non-zero here means a frame was lost, and it is the one counter an
    /// operator should never see climb: it says the decode task is not keeping up
    /// with the sampler, which on this board means the control loop is spending
    /// too long elsewhere.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.source.dropped()
    }

    /// Reset the idle/burst state, e.g. after a decode that consumed a frame.
    pub const fn reset(&mut self) {
        self.state = Sampler::Idle;
    }

    /// Count one successfully decoded frame, for the log.
    pub const fn note_frame(&mut self) {
        self.stats.frames = self.stats.frames.wrapping_add(1);
    }
}

impl EdgeSource for ZacwireCapture<'_> {
    /// Sample, then drain. See [`EdgeSource::sample`] for why this is where the
    /// polling happens and not in the driver.
    fn sample(&mut self) {
        // Spelled out rather than delegating to a `pub fn sample`, so there is
        // exactly one implementation of "sample for a bounded window" and no
        // chance of the trait method and an inherent method drifting apart.
        self.capture(DEFAULT_CAPTURE_US);
    }

    fn take_edges(&mut self, into: &mut cc_protocol::sensor::tsic306::ring::EdgeBuffer) {
        self.source.take_edges(into);
    }

    fn take_overrun(&mut self) -> bool {
        self.source.take_overrun()
    }
}
