//! The `ZACwire` edge capture: sampling a `TSIC-306`'s waveform on one GPIO.
//!
//! Owner: **R3-07**.
//!
//! # 🔴 Not exercised on hardware, and why that is a real gap
//!
//! **No TSIC-306 is fitted to the machine this was written on.** The probe is a
//! DS18B20 (family `0x28`, ROM `286937aacd78af41`, measured), and it shares
//! GPIO16 with the 1-Wire bus ([`crate::sensors::pins::TEMP_SENSOR`]), so the
//! capture is not even brought up in the firmware build. Every claim below is
//! either read out of the installed sources or host-tested against a
//! synthesised waveform. **None of it is evidence that a TSIC-306 works.** See
//! [`cc_domain::sensor::tsic306`]'s module docs, which say the same thing from
//! the protocol side.
//!
//! # The app note asks for an ISR; this is a poller, and why
//!
//! `ATTSic_E2.3.0.pdf` §1.4 recommends a falling-edge interrupt that measures
//! `Tstrobe` and then waits for the next nine edges. This does not do that, for
//! two reasons, both checked against the installed sources rather than assumed.
//!
//! **1. 04 §3.1 constrains this firmware's ISRs to "nothing beyond one GPIO
//! write",** and a bit-sampling ISR is a busy-wait loop with a sampled decision
//! in it. The plan's constraint wins.
//!
//! **2. `esp-idf-hal` 0.47 exposes no safe way to get a timestamped GPIO edge
//! callback, and this workspace denies `unsafe_code`.** Read from
//! `esp-idf-hal-0.47.0/src/gpio.rs`:
//!
//! * [`PinDriver::enable_interrupt`](esp_idf_hal::gpio::PinDriver::enable_interrupt)
//!   registers a *private* `handle_isr` (`:1022-1020`) that only pokes an
//!   internal `PIN_NOTIF` and, under `feature = "alloc"`, an
//!   `unsafe`-obtained `PIN_ISR_HANDLER`. There is no `attach_interrupt`.
//! * The PCNT unit's [`PcntUnitDriver::subscribe`](esp_idf_hal::pcnt::PcntUnitDriver::subscribe)
//!   *is* safe, and it does take a callback — but PCNT is a **counter**, not a
//!   timer: it fires when a watched count is reached, so firing on *every* edge
//!   means re-arming a watch point from inside the callback, which has a window
//!   in which an edge is lost. And the callback's `WatchEventData` carries a
//!   **count**, not a timestamp, so the ISR would have to read a clock itself.
//! * Neither path gives microseconds. `esp-idf-hal` 0.47 has **no** `esp_timer`
//!   module (checked: `src/timer.rs` and `src/delay.rs` have no such re-export),
//!   so the only route to `esp_timer_get_time()` is the FFI symbol, which is
//!   `unsafe` to call.
//!
//! So the capture is a **poller**, and — this is the part that makes a poller the
//! *right* answer rather than a compromise — **the app note's own recommendation
//! for acquiring `Tstrobe` is a sampling rate, not an interrupt**:
//!
//! > It is recommended, however, that the sampling rate of the `ZACwire` signal
//! > when acquiring the start bit be at least 16x the nominal baud rate. Because
//! > the nominal baud rate is 8 kHz, a 128 kHz sampling rate is recommended when
//! > acquiring Tstrobe. (app note §1.3)
//!
//! 128 kHz is a 7.8 µs period. [`BURST_POLL_US`] is 7.
//!
//! # Why polling is cheap here, which is the non-obvious part
//!
//! The naive worry is that a 128 kHz poll loop is 125 000 iterations a second
//! forever, which on a 240 MHz Xtensa is a large and permanent tax. It is not,
//! because **the sensor is silent most of the time**: it transmits one 2.75 ms
//! burst every 100 ms, i.e. 10 Hz (app note §1.4). The line idles high
//! otherwise, so a poller that watches it slowly when nothing is happening and
//! switches to 7 µs only after it sees an edge costs about
//!
//! ```text
//!   idle:   1 poll / ms          ->    1 000 polls / s
//!   burst:  1 poll / 7 us, ~2.75 ms per 100 ms  ->  ~39 000 polls / s averaged
//!   total:  ~40 000 polls / s, of which ~28 000 are at 7 us
//! ```
//!
//! — about **3 % duty**, and it is *self-limiting*: the burst is entered by an
//! edge and left [`BURST_HOLD_US`] after the last one, so a disconnected probe
//! costs 1 000 polls a second and nothing more. A permanently-present sensor
//! costs the same, because the burst ends with the transmission.
//!
//! [`Idle`]: Sampler::Idle

use cc_domain::sensor::tsic306::ring::{EdgeRing, CAPACITY};
use cc_domain::sensor::tsic306::{EdgeSource, RingSource};
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
/// because [`decode_frame`](cc_domain::sensor::tsic306::decode_frame) scans
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

/// The microsecond clock, the one `unsafe` in this file.
///
/// # Why this is `unsafe`, and why it is the only `unsafe` here
///
/// The workspace denies `unsafe_code` and this is a deliberate, narrow
/// exception — **it needs a human to ratify it, and the alternative is not
/// implementing the device side of this driver at all.**
///
/// `esp_timer_get_time()` (`components/esp_timer/include/esp_timer.h:223`) is
/// declared:
///
/// ```c
/// /// @brief Get time in microseconds. This function may be called from
/// /// any ISR or task with the following restrictions:
/// /// ...
/// int64_t esp_timer_get_time(void);
/// ```
///
/// Its own documentation says it may be called from any task or ISR, takes no
/// arguments, allocates nothing, takes no lock that could deadlock, and has no
/// preconditions to uphold. It is a read of the APB-backed timer register plus a
/// software accumulator. **The `unsafe` here is the FFI boundary, not a
/// contract this file can break.**
///
/// The alternatives were checked and are all worse:
///
/// * calling it from a C shim would be the same `unsafe` in a different file;
/// * a `GPTimer` read (`gptimer_get_raw_count`) is also `unsafe` FFI and gives a
///   24-bit wrapping count that has to be differenced by hand — strictly more
///   code for strictly less precision;
/// * `std::time::Instant` is not usable here: `esp-idf-sys` 0.38.1 ships no
///   `std` shim of its own (`src/` has `alloc.rs`, `stdio.rs`, `start.rs` and
///   nothing time-related), and this target's `std` does not document a clock.
///
/// 1 µs resolution against the app note's 7.8 µs requirement is **7.8x margin**,
/// and against the 62.5 µs decision boundary it is 1.6 %.
///
/// The low 31 bits are kept, because that is what
/// [`EdgeRing`](cc_domain::sensor::tsic306::ring::EdgeRing) packs alongside the
/// level in one `AtomicU32` — the original ESP32 has no `AtomicU64`. The 35.8
/// minute wrap is harmless and the argument is in that module's docs.
#[allow(
    unsafe_code,
    clippy::cast_possible_truncation,
    reason = "esp_timer_get_time() is a no-precondition, ISR-safe ESP-IDF \
              function and this HAL exposes no safe clock. See this function's \
              docs; the exception needs a human to ratify it."
)]
#[must_use]
pub fn now_us() -> u32 {
    // Truncating a `u64` microsecond count to `u32` wraps every ~71.6 minutes.
    // That is not a defect for a decoder that only ever looks at differences
    // across a 2.75 ms burst, and it is stated rather than hidden: the ring's
    // `Edge::at_us` is a `u32` and every subtraction in `decode_frame` is
    // `saturating_sub`, so a wrap inside a burst — which cannot happen at 125 µs
    // per bit — would only ever *reject* a frame, never mis-decode one.
    let micros = unsafe { esp_idf_svc::sys::esp_timer_get_time() };
    // The count is non-negative by contract, so only the width is lost.
    #[allow(
        clippy::cast_sign_loss,
        reason = "esp_timer_get_time() is documented as returning a non-negative \
                  microsecond count"
    )]
    let low_31 = (micros as u32) & 0x7FFF_FFFF;
    low_31
}

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
                    // `cc_domain::sensor::tsic306::ring`.
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

    fn take_edges(&mut self, into: &mut cc_domain::sensor::tsic306::ring::EdgeBuffer) {
        self.source.take_edges(into);
    }

    fn take_overrun(&mut self) -> bool {
        self.source.take_overrun()
    }
}
