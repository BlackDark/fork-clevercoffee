//! The TSIC-306 / `ZACwire` driver: the C++'s safety filters over a decoded frame.
//!
//! Owner: **R3-07**.
//!
//! # 🔴 This driver has never run against a TSIC-306
//!
//! **The probe fitted to the machine this was written on is a DS18B20**
//! (family `0x28`, ROM `286937aacd78af41`, measured — see [`super::ds18b20`]).
//! There is no TSIC-306, no `ZACwire` waveform, and no second temperature probe.
//!
//! Everything in this module is therefore **host-tested against a synthesised
//! waveform** ([`simulator`], built from the app note's own timings), and that
//! proves the arithmetic, the frame ordering, the parity and the rejection of
//! damaged frames. It proves **nothing** about a real sensor: its clock
//! tolerance, its 31.25 µs pulses through a pull-up and a cable, its behaviour
//! when brownout, and the one thing that actually matters for a heater — what it
//! does when the supply dips. **A green test run here is not evidence that a
//! TSIC-306 works.** Anyone fitting one must treat the first reading as
//! unverified, and [`Device`](crate) is written so that a wrong decode produces
//! a *rejected reading* rather than a wrong temperature.
//!
//! # 🔴 The device side has also never executed
//!
//! [`GpioZacwire`](crate) — the GPIO edge capture, the `esp_timer_get_time()`
//! timestamps and the `hal::task::notification` wake-up — is implemented and is
//! **not brought up in the firmware build**, because the pin it would capture is
//! carrying 1-Wire traffic from the DS18B20 that is actually fitted. So there are
//! two separate gaps and both are open: the pure logic is tested, and the device
//! path is written but has never run. Neither is evidence about the other.
//!
//! # What this replaces
//!
//! `TempSensorTSIC` (`src/hardware/tempsensors/TempSensorTSIC.cpp`) over
//! `ZACwire` 2.0.0 (`.pio/libdeps/esp32_usb/ZACwire for TSic/ZACwire.cpp`).
//!
//! # The C++'s four safety-relevant behaviours, all ported
//!
//! `TempSensorTSIC::sample_temperature` is 40 lines and four of them matter.
//!
//! ## 1. The two sentinels
//!
//! `temp == 222` → `"Temperature reading failed"`; `temp == 221` →
//! `"Temperature sensor not connected"` (`TempSensorTSIC.cpp:46-54`). Both
//! return false, so both increment `TempSensor::bad_readings_`
//! (`TempSensor.h:43-48`) and both can reach `SENSOR_ERROR` after ten.
//! [`Outcome`] keeps them apart, because 221 means *go and look at the wiring*
//! and 222 means *this reading is not trustworthy*.
//!
//! ## 2. The hard physical range
//!
//! ```cpp
//! // Reject physically impossible readings (sensor glitches) instead of passing
//! // them through as valid. A single spurious value (e.g. -2.9°C) must not trip
//! // emergency stop; treating it as a failed read keeps the last good cached
//! // value instead.
//! if (temp <= 0.0 || temp >= 180.0) { ... return false; }
//! ```
//!
//! (`TempSensorTSIC.cpp:56-62`.) **This is the single most important line in the
//! file.** A TSIC-306 that is brownout for one 100 ms window can decode to a
//! wild value, and S1's `MIN_VALID_TEMP_C` is 0.0
//! (`constants/Temperature.h:14`) — so an unfiltered −2.9 °C glitch reaches
//! `EmergencyStopManager::checkEmergencyConditions` and latches a machine that
//! is perfectly healthy. The reject converts the glitch into a failed read, the
//! failed read keeps the last good cached value, and the PID never sees it.
//!
//! **Preserved verbatim, including the inclusive bounds**, which means a
//! legitimate 0.00 °C is rejected. [`protocol::ACCEPT_MIN_C`]'s docs say so.
//!
//! ## 3. The adaptive max change rate
//!
//! `INITIAL_CHANGERATE 200` / `RUNTIME_CHANGERATE 5`
//! (`TempSensorTSIC.cpp:11-12`), passed to `ZACwire::getTemp(maxChangeRate)`,
//! which rejects a reading whose gradient exceeds it by returning **222**
//! (`ZACwire.cpp:59`, `:66`). So a rate violation is a *sentinel*, not a
//! separate outcome — which is why [`Outcome::ReadFailed`] covers it.
//!
//! ## 4. The latch onto the tighter rate
//!
//! ```cpp
//! static bool validTemps = false;
//! ...
//! if (temp > 0.0 && temp < 180.0 && temperature > 0.0 && temperature < 180.0 &&
//!     abs(temperature - temp) < RUNTIME_CHANGERATE) { validTemps = true; }
//! ```
//!
//! (`TempSensorTSIC.cpp:29`, `:36-41`.) Two consecutive readings both inside
//! the accept window and within 5 °C of each other mean the signal has settled,
//! and from then on the 5 °C/sample limit is the one that rejects a glitch.
//!
//! # Two things about the C++ that are recorded rather than copied
//!
//! **The `static` is process-global.** `validTemps` is a function-local `static`
//! in a `const` member function, so it is shared by every `TempSensorTSIC` in
//! the process, is never reset, and latching is a once-forever event. A probe
//! that is unplugged, re-plugged, and warms up does not get to re-latch. This
//! port makes it per-instance state, so a reconnect starts in the permissive
//! state — which is the safe direction, since the permissive state is the one
//! that follows the C++ on a first-ever boot.
//!
//! **The units of `maxChangeRate` are not what the constant's name implies.**
//! `ZACwire::getTemp` computes `int16_t grad = (temp - prevTemp) / (heartbeat|1)`
//! (`ZACwire.cpp:58`) where `temp` is the **raw 11-bit count**, not degrees, and
//! its own comment says `//grad is [°C/s]`, which is wrong. So the C++'s two
//! constants are really 200 **counts**/sample (≈ 19.5 °C/sample) and 5
//! **counts**/sample (≈ 0.49 °C/sample) — while `TempSensorTSIC`'s own latch
//! condition compares the same `RUNTIME_CHANGERATE` against two **degrees**
//! (`abs(temperature - temp) < RUNTIME_CHANGERATE`, both `double`s in °C).
//!
//! The same number is used in two different units in two adjacent files. This
//! port applies both in **degrees**, because that is what
//! [`protocol::INITIAL_CHANGE_RATE_C`] and [`protocol::RUNTIME_CHANGE_RATE_C`]
//! are documented as (02 §6 calls them "200 °C/sample → 5 °C/sample") and
//! because the latch condition in the C++ is unambiguously in degrees. The
//! count-based reading is available by dividing the limits by
//! `200.0 / 2047.0`; see [`COUNT_SCALE`]. **This is an unresolved ambiguity, not
//! a decision**, and it is the single thing most likely to be wrong in this
//! module.

pub mod decode;
pub mod protocol;
pub mod ring;
#[cfg(test)]
mod simulator;

use crate::sensor::probe::{ProbeFault, ProbeReading, ProbeSource};
use crate::units::Millis;

use self::decode::FrameError;
use self::ring::{EdgeBuffer, EdgeRing, CAPACITY};

/// The scale factor between a raw count and a degree, for the count-based
/// reading of the change-rate constants. `200.0 / 2047.0`.
///
/// Exposed so that the alternative interpretation of
/// `INITIAL_CHANGERATE`/`RUNTIME_CHANGERATE` is one multiplication away and not
/// a re-derivation. See the module docs.
pub const COUNT_SCALE: f32 = protocol::RANGE_SPAN_C / protocol::RANGE_STEPS;

/// What one transmission produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// A temperature, accepted by every check.
    Reading(f32),
    /// `ZACwire`'s **222**, `errorMisreading`.
    ///
    /// `TempSensorTSIC.cpp:46-49`. Covers: a frame that failed to decode
    /// ([`FrameError`]), a reading outside the accept window, and a reading the
    /// change-rate limiter rejected. **All three are one outcome on purpose** —
    /// that is what the C++ collapses them into, and the distinction that
    /// matters (`NotConnected` vs everything else) is kept.
    ReadFailed,
    /// `ZACwire`'s **221**, `errorNotConnected`.
    ///
    /// `TempSensorTSIC.cpp:51-54`, and `ZACwire::connectionCheck`'s heartbeat
    /// timeout (`ZACwire.cpp:104-113`). A probe that is not transmitting.
    NotConnected,
}

impl Outcome {
    /// The C++'s sentinel this outcome corresponds to, or `None` for a reading.
    #[must_use]
    pub const fn cpp_sentinel(self) -> Option<f32> {
        match self {
            Self::Reading(_) => None,
            Self::ReadFailed => Some(protocol::ERROR_MISREADING),
            Self::NotConnected => Some(protocol::ERROR_NOT_CONNECTED),
        }
    }

    /// The [`ProbeFault`] this outcome becomes for the state machine.
    #[must_use]
    pub const fn probe_fault(self) -> Option<ProbeFault> {
        match self {
            Self::Reading(_) => None,
            Self::ReadFailed => Some(ProbeFault::ReadFailed),
            Self::NotConnected => Some(ProbeFault::NotConnected),
        }
    }
}

impl core::fmt::Display for Outcome {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Reading(celsius) => write!(f, "{celsius:.2} C"),
            Self::ReadFailed => f.write_str("read failed (222)"),
            Self::NotConnected => f.write_str("not connected (221)"),
        }
    }
}

/// Where a transmission's edges come from.
///
/// The seam that makes the whole protocol testable. The device crate implements
/// it over a GPIO interrupt and an [`EdgeRing`]; the tests implement it over a
/// synthesised [`Waveform`].
///
/// # Why the trait hands over a *buffer of edges* and not a decoded bit
///
/// A ring is drained into a caller-owned [`EdgeBuffer`] and the buffer is
/// decoded, so the transport is "give me the edges since last time". A trait
/// that handed over already-sampled bits would put the decision — which is the
/// whole of this protocol — inside the device crate, where it could not be
/// tested.
///
/// The buffer is a parameter rather than a return value so the driver holds no
/// allocation: this crate is `no_alloc` in library code, and a 768-byte stack
/// array in the decode task is strictly better than a heap block on the ESP32.
///
/// **Two reports, not one**, because they are different faults. A drain that
/// returned edges *and* had lost some must be refused, not decoded: the edges
/// it did return belong to a frame with a hole in it. So [`Self::take_overrun`]
/// is separate and [`Tsic306::poll`] refuses a buffer whenever it is set.
pub trait EdgeSource {
    /// Acquire whatever is available, before the drain.
    ///
    /// **This is the swap point between a poller and an interrupt.** It exists so
    /// that [`Tsic306::poll`] does not have to know which it is talking to:
    ///
    /// * the **poller** implementation samples the line for a bounded window and
    ///   then drains — see [`cc_hal_esp32::zacwire`];
    /// * an **interrupt-driven** implementation does nothing here, because the ISR
    ///   has already pushed the edges, and the drain is all that is left.
    ///
    /// The default is the no-op, so an ISR-backed source needs to implement only
    /// the two methods below and gets the right behaviour for free.
    fn sample(&mut self) {}

    /// Drain every edge captured since the last call into `into`, oldest first.
    fn take_edges(&mut self, into: &mut EdgeBuffer);

    /// Consume and report the ring's overrun flag.
    ///
    /// `true` means edges were lost since the last call, so whatever was drained
    /// is a partial frame and must be refused.
    fn take_overrun(&mut self) -> bool;
}

/// An [`EdgeSource`] over a fixed ring, for the device crate and the tests.
pub struct RingSource {
    ring: EdgeRing<CAPACITY>,
    overran: bool,
}

impl RingSource {
    /// An empty capture.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ring: EdgeRing::new(),
            overran: false,
        }
    }

    /// The ring the interrupt writes to. **The interrupt's only dependency.**
    #[must_use]
    pub const fn ring(&self) -> &EdgeRing<CAPACITY> {
        &self.ring
    }

    /// How many edges have been refused because the ring was full.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.ring.dropped()
    }
}

impl Default for RingSource {
    fn default() -> Self {
        Self::new()
    }
}

/// A blanket forward so a caller can hand a `&mut` source to [`Tsic306::new`].
///
/// Needed by the device crate's boot-time probe, which borrows its capture for
/// one decode before moving it into the long-lived driver. Without it that would
/// have to be a duplicate of the whole driver, which is the kind of duplication
/// that lets two copies disagree.
impl<S: EdgeSource + ?Sized> EdgeSource for &mut S {
    fn sample(&mut self) {
        (**self).sample();
    }

    fn take_edges(&mut self, into: &mut EdgeBuffer) {
        (**self).take_edges(into);
    }

    fn take_overrun(&mut self) -> bool {
        (**self).take_overrun()
    }
}

impl EdgeSource for RingSource {
    fn take_edges(&mut self, into: &mut EdgeBuffer) {
        into.clear();
        while let Some(edge) = self.ring.pop() {
            into.push(edge);
        }
        // `pop` resynchronises the ring when the flag is set, so the drain above
        // is already clean; the flag itself is handed on by `take_overrun`.
    }

    fn take_overrun(&mut self) -> bool {
        let overran = self.ring.take_overrun();
        // Latched locally as well, so a caller that drains twice for one frame
        // cannot lose the report.
        self.overran |= overran;
        let out = self.overran;
        self.overran = false;
        out
    }
}

/// A [`EdgeSource`] over a synthesised waveform, for the host tests only.
///
/// A *script* of transmissions rather than one, so a test can present the
/// sequences the driver actually has to survive: a warm-up, then a glitch, then
/// more warm readings.
#[cfg(test)]
struct WaveformSource {
    script: alloc::vec::Vec<simulator::Waveform>,
    overrun: bool,
}

#[cfg(test)]
impl WaveformSource {
    fn new(script: alloc::vec::Vec<simulator::Waveform>) -> Self {
        Self {
            script,
            overrun: false,
        }
    }
}

#[cfg(test)]
impl EdgeSource for WaveformSource {
    fn take_edges(&mut self, into: &mut EdgeBuffer) {
        into.clear();
        if self.overrun {
            self.overrun = false;
            return;
        }
        if self.script.is_empty() {
            return;
        }
        for edge in self.script.remove(0).edges() {
            into.push(edge);
        }
    }

    fn take_overrun(&mut self) -> bool {
        let overran = self.overrun;
        self.overrun = false;
        overran
    }
}

/// The driver: a change-rate latch, the previous good reading, and the C++'s
/// error counter.
///
/// Generic over [`EdgeSource`] so the whole of the decision path is host-tested
/// and the device crate supplies only a pin and a clock.
pub struct Tsic306<S> {
    source: S,
    /// `TempSensorTSIC`'s `temperature` out-parameter: the last **accepted**
    /// reading, seeded by the caller from `TempSensor::last_temperature_`
    /// (`TempSensor.h:127`).
    previous: Option<f32>,
    /// `TempSensorTSIC`'s `static bool validTemps` — as **per-instance** state.
    /// See the module docs for why that is a divergence.
    latched: bool,
    /// The most recent frame's measured `Tstrobe`, for the boot log.
    last_strobe_us: u32,
    bad_readings: u8,
    error: bool,
    /// Latched by [`Self::note_overrun`] so a caller that notices an overrun
    /// outside [`Self::poll`] still gets a 222 for that poll rather than a
    /// silently skipped tick.
    overran: bool,
}

impl<S: EdgeSource> Tsic306<S> {
    /// A driver over `source`, with no reading seen yet.
    #[must_use]
    pub const fn new(source: S) -> Self {
        Self {
            source,
            previous: None,
            latched: false,
            last_strobe_us: 0,
            bad_readings: 0,
            error: false,
            overran: false,
        }
    }

    /// The change rate currently in force, in °C per reading.
    #[must_use]
    pub const fn change_rate_c(&self) -> f32 {
        if self.latched {
            protocol::RUNTIME_CHANGE_RATE_C
        } else {
            protocol::INITIAL_CHANGE_RATE_C
        }
    }

    /// Whether the tighter rate has latched on.
    #[must_use]
    pub const fn is_latched(&self) -> bool {
        self.latched
    }

    /// The last accepted reading, which is the C++'s cached value.
    #[must_use]
    pub const fn previous(&self) -> Option<f32> {
        self.previous
    }

    /// The last frame's measured `Tstrobe`, in microseconds.
    #[must_use]
    pub const fn last_strobe_us(&self) -> u32 {
        self.last_strobe_us
    }

    /// Whether the C++ would report this probe as faulty.
    #[must_use]
    pub const fn has_error(&self) -> bool {
        self.error
    }

    /// Consecutive rejected readings.
    #[must_use]
    pub const fn bad_readings(&self) -> u8 {
        self.bad_readings
    }

    /// Advance by one transmission's worth of edges.
    ///
    /// `buffer` is the caller's drain target and is reused across calls, so a
    /// 10 Hz poll costs no allocation at all. See [`EdgeSource`] for why it is a
    /// parameter.
    pub fn poll(&mut self, buffer: &mut EdgeBuffer) -> Outcome {
        self.source.sample();
        self.source.take_edges(buffer);
        if self.overran || self.source.take_overrun() {
            self.overran = false;
            return self.reject(Outcome::ReadFailed);
        }
        if buffer.is_empty() {
            return self.no_signal();
        }
        match decode::decode_frame(buffer.as_slice()) {
            Ok(frame) => {
                self.last_strobe_us = frame.strobe_us;
                self.accept(frame.celsius())
            }
            Err(FrameError::Incomplete | FrameError::NoStartBit) => {
                // A capture that stopped mid-frame, or a line with no 50 % pulse
                // on it. The C++'s 221 comes from a *dead* probe and 222 from a
                // misread one; a line that is moving but is not a ZACwire line
                // is a misread. Failing toward 222 is the conservative choice,
                // because 221 tells an operator to check the wiring and this is
                // not a wiring fault.
                self.reject(Outcome::ReadFailed)
            }
            Err(
                FrameError::BadBitPeriod
                | FrameError::BadStopBit
                | FrameError::BadParity { .. }
                | FrameError::ReservedBitsSet,
            ) => self.reject(Outcome::ReadFailed),
        }
    }

    /// Report that the source lost a frame to a ring overrun.
    ///
    /// Separate from [`Self::poll`] because the ring notices it, not the decoder:
    /// the device crate's decode loop sees an empty buffer with the overrun flag
    /// set and calls this so the lost frame becomes a 222 rather than a skipped
    /// tick.
    pub fn note_overrun(&mut self) {
        self.overran = true;
    }

    /// What the 221 path means, as a separate query.
    ///
    /// The C++ gets there through `ZACwire::connectionCheck`'s heartbeat
    /// (`ZACwire.cpp:104-113`): a frame that has not completed for longer than
    /// [`protocol::NO_SIGNAL_TIMEOUT_US`] means the probe is not transmitting.
    /// A pure `poll` cannot tell "the line is quiet" from "nothing has happened
    /// yet", so the caller supplies the elapsed time.
    #[must_use]
    pub fn no_signal_outcome(&self, quiet_for: Millis) -> Outcome {
        if u64::from(quiet_for.raw()) * 1000 >= u64::from(protocol::NO_SIGNAL_TIMEOUT_US) {
            Outcome::NotConnected
        } else {
            Outcome::ReadFailed
        }
    }

    /// The C++'s `TempSensorTSIC::sample_temperature`, in order.
    ///
    /// This is the port of lines 28-67 and it is deliberately written as the C++
    /// reads, including the latch evaluation running *before* the rate check.
    fn accept(&mut self, celsius: f32) -> Outcome {
        // The latch, verbatim from `TempSensorTSIC.cpp:36-41`. Note it is
        // evaluated on the *new* reading and the previous one, both already
        // known to be inside the window because the previous one passed the
        // range check and this one has not been range-checked yet — which is
        // why the C++ repeats the window test on `temp` here. The repetition is
        // load-bearing: it is what stops a -2.9 °C glitch from latching the
        // driver into its strict 5 °C mode on the strength of one bad reading.
        if !self.latched && (protocol::ACCEPT_MIN_C..protocol::ACCEPT_MAX_C).contains(&celsius) {
            if let Some(previous) = self.previous {
                if (protocol::ACCEPT_MIN_C..protocol::ACCEPT_MAX_C).contains(&previous)
                    && (previous - celsius).abs() < protocol::RUNTIME_CHANGE_RATE_C
                {
                    self.latched = true;
                }
            }
        }

        // `ZACwire::getTemp`'s own rate check, which returns 222 when it fails
        // (`ZACwire.cpp:59`, `:66`). The C++ seeds `prevTemp` from the first
        // reading so that the very first one is never rate-limited
        // (`ZACwire.cpp:57`), which `Option` gives for free.
        if let Some(previous) = self.previous {
            if (celsius - previous).abs() >= self.change_rate_c() {
                return self.reject(Outcome::ReadFailed);
            }
        }

        // The hard physical range, `TempSensorTSIC.cpp:59-62`. Inclusive at both
        // ends, so a real 0.00 °C is refused.
        if celsius <= protocol::ACCEPT_MIN_C || celsius >= protocol::ACCEPT_MAX_C {
            return self.reject(Outcome::ReadFailed);
        }

        self.previous = Some(celsius);
        self.bad_readings = 0;
        self.error = false;
        Outcome::Reading(celsius)
    }

    /// A reading that became 222 or 221.
    ///
    /// `TempSensor::updateTemperature`'s `else if (!error_)` arm
    /// (`TempSensor.h:43-48`): count it, and raise `error_` at
    /// [`MAX_BAD_READINGS`](super::ds18b20::MAX_BAD_READINGS).
    ///
    /// **The C++'s own counter has the same threshold** — `max_bad_readings_` is
    /// a private member of the shared `TempSensor` base (`TempSensor.h:177`) —
    /// so it is the same ten for both drivers, which is why the constant lives
    /// with the DS18B20 and is referenced rather than duplicated.
    fn reject(&mut self, outcome: Outcome) -> Outcome {
        self.bad_readings = self.bad_readings.saturating_add(1);
        if self.bad_readings >= super::ds18b20::MAX_BAD_READINGS && !self.error {
            self.error = true;
        }
        outcome
    }

    /// Nothing arrived. The C++ only reaches its 221 through a heartbeat
    /// timeout, so an empty poll is *not* immediately a fault — the caller
    /// decides, with [`Self::no_signal_outcome`].
    // `&self`, not `&mut self`: an empty drain mutates nothing, and taking
    // `&mut` would make the caller hold a mutable borrow for no reason.
    #[allow(clippy::unused_self, reason = "an empty drain mutates nothing")]
    fn no_signal(&self) -> Outcome {
        Outcome::ReadFailed
    }
}

/// Collapse an [`Outcome`] into the [`TemperatureProbe`](super::probe::TemperatureProbe)
/// vocabulary.
///
/// A free function rather than an associated one: it does not touch the driver,
/// and putting it on `Tsic306<S>` would make every call site name a type
/// parameter it does not have.
#[must_use]
pub fn as_probe(outcome: Outcome) -> Option<ProbeReading> {
    match outcome {
        Outcome::Reading(celsius) => Some(ProbeReading {
            celsius,
            source: ProbeSource::Tsic306,
        }),
        Outcome::ReadFailed | Outcome::NotConnected => None,
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the tests compare f32 temperatures that the code under test computes \
              by the same expression, or that are exactly representable 11-bit grid \
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
    use crate::sensor::ds18b20;
    use alloc::vec::Vec;
    use decode::Frame;
    use ring::Edge;
    use simulator::{Damage, Waveform};

    /// A driver fed a scripted sequence of transmissions, plus the drain buffer
    /// the device crate's decode loop would own.
    ///
    /// `Rig` exists so the tests read as `d.poll()` and `d.driver.is_latched()`
    /// rather than threading a `&mut EdgeBuffer` through every assertion — and
    /// so it is obvious that the buffer is the *caller's*, which is the property
    /// that keeps a 10 Hz poll allocation-free.
    struct Rig {
        driver: Tsic306<WaveformSource>,
        buffer: EdgeBuffer,
    }

    impl Rig {
        fn new(script: Vec<Waveform>) -> Self {
            Self {
                driver: Tsic306::new(WaveformSource::new(script)),
                buffer: EdgeBuffer::new(),
            }
        }

        /// One transmission's worth of drain-and-decode.
        fn poll(&mut self) -> Outcome {
            self.driver.poll(&mut self.buffer)
        }
    }

    fn driver(script: Vec<Waveform>) -> Rig {
        Rig::new(script)
    }

    /// An undamaged waveform for a temperature, via the app note's own formula.
    fn wire_for(celsius: f32) -> Waveform {
        Waveform::for_raw(raw_for(celsius))
    }

    /// The 11-bit code nearest `celsius`.
    fn raw_for(celsius: f32) -> u16 {
        let steps = (celsius - protocol::RANGE_MIN_C) / protocol::RANGE_SPAN_C;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let raw = (steps * protocol::RANGE_STEPS).round().clamp(0.0, 2047.0) as u16;
        raw
    }

    /// The temperature `wire_for(celsius)` actually encodes.
    ///
    /// 90 °C is not on the 11-bit grid — the step is 0.0977 °C — so a test that
    /// writes `Reading(90.0)` is asserting a number the sensor cannot produce.
    /// Every expectation goes through here instead, which is also why the grid
    /// shows up in the failure messages rather than being a mystery.
    fn quantised(celsius: f32) -> f32 {
        Frame {
            raw: raw_for(celsius),
            strobe_us: 62,
        }
        .celsius()
    }

    // ============================================== the round trip, and its limits

    #[test]
    fn the_named_temperatures_round_trip_through_the_waveform() {
        // The task's list: 0 °C, 25 °C, 100 °C and the two extremes.
        for (celsius, expected_raw) in [
            (0.0f32, 512u16), // (0 + 50) / 200 * 2047 = 511.75 -> 512
            (25.0, 768),      // 75 / 200 * 2047 = 767.6 -> 768
            (100.0, 1535),    // 150 / 200 * 2047 = 1535.25 -> 1535
            (-50.0, 0),
            (150.0, 2047),
        ] {
            let wire = wire_for(celsius);
            assert_eq!(wire.raw, expected_raw, "{celsius} C -> raw");
            let frame = wire.decode().unwrap_or(Frame {
                raw: 0,
                strobe_us: 0,
            });
            assert_eq!(
                frame.raw, expected_raw,
                "{celsius} C did not survive the wire"
            );
            assert!(
                (frame.celsius() - celsius).abs() <= 200.0 / 2047.0,
                "{celsius} C decoded as {}",
                frame.celsius()
            );
        }
    }

    #[test]
    fn a_whole_eleven_bit_range_decodes_to_itself() {
        // Not just the named points: every one of the 2048 codes, through the
        // synthesised waveform and the production decoder. This is the test that
        // would catch a bit-order error, and it is the reason the packet layout
        // is not hand-transcribed anywhere.
        for raw in 0..=2047u16 {
            let wire = Waveform::for_raw(raw);
            assert_eq!(
                wire.decode().map(|frame| frame.raw),
                Ok(raw),
                "raw {raw} did not survive the wire"
            );
        }
    }

    #[test]
    fn the_driver_reports_the_measured_strobe_on_every_good_frame() {
        let mut d = driver(alloc::vec![wire_for(90.0)]);
        assert_eq!(d.poll(), Outcome::Reading(quantised(90.0)));
        assert_eq!(d.driver.last_strobe_us(), 62);
    }

    // ================================================ the corrupted-frame cases

    #[test]
    fn a_flipped_data_bit_fails_parity() {
        // `flip_data_bit` indexes the **11 raw bits**, so raw bits 10, 9 and 8 are
        // the three in packet 1 and raw bits 7..0 are the eight in packet 2.
        // Every one of the eleven is flipped in turn and the parity error names
        // the packet that carries it.
        for flip in 8..11u16 {
            let wire = Waveform::for_raw(0b011_0001_1000).damaged(Damage {
                flip_data_bit: Some(usize::from(flip)),
                ..Damage::none()
            });
            assert_eq!(
                wire.decode(),
                Err(FrameError::BadParity { packet: 1 }),
                "raw bit {flip} lives in packet 1"
            );
        }
        for flip in 0..8u16 {
            let wire = Waveform::for_raw(0b011_0001_1000).damaged(Damage {
                flip_data_bit: Some(usize::from(flip)),
                ..Damage::none()
            });
            assert_eq!(
                wire.decode(),
                Err(FrameError::BadParity { packet: 2 }),
                "raw bit {flip} lives in packet 2"
            );
        }
    }

    #[test]
    fn a_wrong_start_bit_duty_is_not_a_start_bit() {
        // The synthesiser emits a 25 % or a 75 % "start bit". The strobe window
        // (45..80 us) excludes both, so acquisition fails before any bit is read.
        for pct in [protocol::duty::ZERO_PCT, protocol::duty::ONE_PCT] {
            let wire = Waveform::for_raw(0b101_1010_1010).damaged(Damage {
                start_duty_pct: Some(pct),
                ..Damage::none()
            });
            assert!(
                wire.decode().is_err(),
                "a {pct} % start pulse must not produce a frame"
            );
        }
    }

    #[test]
    fn a_missing_stop_bit_is_rejected() {
        // The stop bit collapses from two windows to one. The period check for
        // index 10 is the stop check, so this is caught by `BadStopBit` and not
        // by a generic period error — which matters, because "no stop bit" and
        // "a lost edge" are different faults and only one of them is a wiring
        // problem.
        let wire = Waveform::for_raw(0b101_1010_1010).damaged(Damage {
            missing_stop_bit: true,
            ..Damage::none()
        });
        assert_eq!(wire.decode(), Err(FrameError::BadStopBit));
    }

    #[test]
    fn a_doubled_stop_bit_is_rejected_too() {
        let wire = Waveform::for_raw(0b101_1010_1010).damaged(Damage {
            doubled_stop_bit: true,
            ..Damage::none()
        });
        assert_eq!(wire.decode(), Err(FrameError::BadStopBit));
    }

    #[test]
    fn a_dropped_edge_is_a_bad_bit_period() {
        // This is what a missed interrupt looks like: two bits merge into one
        // 250 us gap. Parity would not reliably catch it, which is why the
        // period check exists.
        for drop in 1..20usize {
            let wire = Waveform::for_raw(0b101_1010_1010).damaged(Damage {
                drop_falling_edge_at: Some(drop),
                ..Damage::none()
            });
            let verdict = wire.decode();
            assert!(
                verdict.is_err(),
                "dropping edge {drop} produced {verdict:?}, which is not a rejection"
            );
            // And never, under any damage, a *different* raw value: a lost edge
            // must cost the reading, not change it.
            assert_ne!(
                verdict,
                Ok(super::decode::Frame {
                    raw: 0,
                    strobe_us: 62
                })
            );
        }
    }

    #[test]
    fn a_truncated_transmission_is_rejected() {
        let wire = Waveform::for_raw(0b011_0001_1000).damaged(Damage {
            truncate_after_first_packet: true,
            ..Damage::none()
        });
        assert_eq!(wire.decode(), Err(FrameError::Incomplete));
    }

    #[test]
    fn no_signal_is_incomplete_and_the_driver_calls_it_a_failed_read() {
        let wire = Waveform::for_raw(0).damaged(Damage {
            no_signal: true,
            ..Damage::none()
        });
        assert!(wire.edges().is_empty());
        assert_eq!(wire.decode(), Err(FrameError::Incomplete));
        let mut d = driver(alloc::vec![wire]);
        assert_eq!(d.poll(), Outcome::ReadFailed);
    }

    #[test]
    fn every_damaged_frame_is_a_222_and_never_a_temperature() {
        // The single property that matters: **a broken frame produces a sentinel,
        // never a number.** Walk every damage mode and assert it.
        let damages = [
            Damage {
                flip_data_bit: Some(4),
                ..Damage::none()
            },
            Damage {
                start_duty_pct: Some(25),
                ..Damage::none()
            },
            Damage {
                missing_stop_bit: true,
                ..Damage::none()
            },
            Damage {
                doubled_stop_bit: true,
                ..Damage::none()
            },
            Damage {
                truncate_after_first_packet: true,
                ..Damage::none()
            },
            Damage {
                drop_falling_edge_at: Some(7),
                ..Damage::none()
            },
            Damage {
                no_signal: true,
                ..Damage::none()
            },
        ];
        for damage in damages {
            let wire = Waveform::for_raw(0b111_1111_1111).damaged(damage);
            let mut d = driver(alloc::vec![wire]);
            let outcome = d.poll();
            assert!(
                !matches!(outcome, Outcome::Reading(_)),
                "{damage:?} produced {outcome}"
            );
            assert_eq!(outcome, Outcome::ReadFailed, "{damage:?}");
            assert_eq!(outcome.cpp_sentinel(), Some(222.0));
        }
    }

    // ================================================== the range reject (C++ 2)

    #[test]
    fn the_cold_extreme_is_rejected_and_the_hot_extreme_is_not() {
        // 🔴 A finding about the C++'s range check, measured rather than assumed.
        //
        // `temp <= 0.0 || temp >= 180.0` (`TempSensorTSIC.cpp:59`). The
        // TSIC-306's span is -50..+150 (`T = DS/2047·200 − 50`), so:
        //
        // | bound | reachable? |
        // | --- | --- |
        // | `temp <= 0.0` | **yes** — DS 0..511, i.e. -50 .. -0.024 °C |
        // | `temp >= 180.0` | **never** — the sensor cannot report above 150 |
        //
        // The upper half of the check is **dead code on this sensor**. It is
        // preserved verbatim anyway, because a TSIC-506 has a different span and
        // the constant is shared with it (`ZACwire.cpp:62-63` switches formula on
        // `_sensor < 400`), but a reader should know it cannot fire here.
        // A 150.00 °C boiler reading is *accepted* by the C++ and by this port.
        let mut cold = driver(alloc::vec![Waveform::for_raw(0)]);
        let outcome = cold.poll();
        assert_eq!(
            outcome,
            Outcome::ReadFailed,
            "-50 C is below the accept window"
        );
        assert_eq!(outcome.cpp_sentinel(), Some(222.0));

        let mut hot = driver(alloc::vec![Waveform::for_raw(2047)]);
        assert_eq!(
            hot.poll(),
            Outcome::Reading(150.0),
            "+150 C is the sensor's top and must be accepted"
        );
    }

    #[test]
    fn the_lower_bound_fires_from_raw_511_downwards() {
        // `temp <= 0.0` cannot fire at exactly 0.00 °C, because 0.00 is not on
        // the 11-bit grid: the nearest codes are -0.024 °C (DS 511) and +0.024 °C
        // (DS 512). So the bound's first victim is DS 511, and the boundary is
        // effectively "below +0.024 °C" rather than "at or below zero".
        assert!(
            Frame {
                raw: 511,
                strobe_us: 62
            }
            .celsius()
                < 0.0
        );
        assert!(
            Frame {
                raw: 512,
                strobe_us: 62
            }
            .celsius()
                > 0.0
        );
        let mut at_511 = driver(alloc::vec![Waveform::for_raw(511)]);
        assert_eq!(at_511.poll(), Outcome::ReadFailed);
        let mut at_512 = driver(alloc::vec![Waveform::for_raw(512)]);
        assert_eq!(
            at_512.poll(),
            Outcome::Reading(
                Frame {
                    raw: 512,
                    strobe_us: 62
                }
                .celsius()
            )
        );
    }

    #[test]
    fn a_single_glitch_never_reaches_the_control_loop() {
        // 🔴 The line the whole range reject exists for, from
        // `TempSensorTSIC.cpp:56-58`: "A single spurious value (e.g. -2.9°C) must
        // not trip emergency stop". -2.9 °C is DS = 493, squarely inside the
        // sensor's range, so it decodes perfectly — and it is refused here.
        // S1's `MIN_VALID_TEMP_C` is 0.0, so an unfiltered -2.9 would latch
        // emergency stop.
        let mut d = driver(alloc::vec![wire_for(90.0), wire_for(-2.9), wire_for(90.0)]);
        assert_eq!(d.poll(), Outcome::Reading(quantised(90.0)));
        assert_eq!(d.poll(), Outcome::ReadFailed, "-2.9 C must be refused");
        // The last good cached value is untouched, which is the point: the
        // control loop keeps the reading it had.
        assert_eq!(d.driver.previous(), Some(quantised(90.0)));
        assert_eq!(d.poll(), Outcome::Reading(quantised(90.0)));
    }

    #[test]
    fn a_glitch_cannot_latch_the_driver_into_its_strict_mode() {
        // The load-bearing detail in the latch condition: it re-tests the window
        // on the *new* reading, so one bad reading cannot be the second half of
        // a pair. A -2.9 °C reading arrives, is refused, and the driver is still
        // in the permissive 200 C/sample mode afterwards.
        let mut d = driver(alloc::vec![
            wire_for(90.0),
            wire_for(-2.9),
            wire_for(90.5),
            wire_for(91.0),
        ]);
        assert_eq!(d.poll(), Outcome::Reading(quantised(90.0)));
        assert!(!d.driver.is_latched());
        assert_eq!(d.poll(), Outcome::ReadFailed);
        assert!(!d.driver.is_latched(), "one glitch must not latch");
        assert_eq!(d.poll(), Outcome::Reading(quantised(90.5)));
        // 90.5 and 90.0 are both in the window and within 5 C, so *this* pair
        // latches -- and note it needed the reading *after* the glitch.
        assert!(d.driver.is_latched());
        assert_eq!(d.driver.change_rate_c(), protocol::RUNTIME_CHANGE_RATE_C);
    }

    #[test]
    fn the_bound_is_inclusive_and_0_c_is_not_representable() {
        // The C++'s test is inclusive (`temp <= 0.0`), so it *would* refuse a
        // reading of exactly 0.00 °C. On this sensor that reading cannot occur —
        // 0.00 is not on the 11-bit grid — so the inclusive-ness is unobservable
        // here and `the_lower_bound_fires_from_raw_511_downwards` pins what
        // actually happens. Preserved verbatim regardless, because the constant
        // is shared with the TSIC-506.
        assert_eq!(protocol::ACCEPT_MIN_C, 0.0);
        assert!(wire_for(0.0).decode().is_ok(), "a waveform always decodes");
        let mut d = driver(alloc::vec![wire_for(0.0)]);
        assert_eq!(
            d.poll(),
            Outcome::Reading(quantised(0.0)),
            "the nearest code to 0 C is +0.024 C, which is inside the window"
        );
    }

    // ============================================ the change rate (C++ 3 and 4)

    #[test]
    fn the_first_reading_is_never_rate_limited() {
        // `ZACwire.cpp:57`: `if (!prevTemp) prevTemp = temp;` seeds before the
        // gradient is computed, so the first reading always passes the *rate*
        // check — whatever it is. It is still subject to the accept window, so
        // this reading is inside it; a first reading of -49 °C would be refused
        // by `temp <= 0.0` and says nothing about the rate limiter.
        let mut d = driver(alloc::vec![wire_for(20.0)]);
        assert_eq!(d.driver.change_rate_c(), protocol::INITIAL_CHANGE_RATE_C);
        assert_eq!(d.poll(), Outcome::Reading(quantised(20.0)));
    }

    #[test]
    fn the_permissive_rate_allows_a_jump_but_not_an_impossible_one() {
        // 20 -> 21 -> 149: the last is 128 C in one sample, which the initial
        // 200 C/sample rate allows and the latched 5 C/sample rate would not.
        // The point of the permissive rate is exactly this: a machine that has
        // just been switched on, or has just been told a new setpoint, must be
        // able to get to temperature in a few samples.
        let mut d = driver(alloc::vec![wire_for(20.0), wire_for(21.0), wire_for(149.0)]);
        assert_eq!(d.poll(), Outcome::Reading(quantised(20.0)));
        assert!(
            !d.driver.is_latched(),
            "20 and 21 are 1 C apart -- that latches"
        );
        // Latched after one close pair, so reset for the permissive case.
        let mut wide = driver(alloc::vec![wire_for(20.0), wire_for(140.0)]);
        assert_eq!(wide.poll(), Outcome::Reading(quantised(20.0)));
        assert!(
            !wide.driver.is_latched(),
            "20 and 140 are 120 C apart, so no latch"
        );
        assert_eq!(
            wide.poll(),
            Outcome::Reading(quantised(140.0)),
            "a 120 C jump is under the initial 200 C/sample rate"
        );
    }

    #[test]
    fn the_rate_latch_needs_two_readings_inside_the_window_and_close_together() {
        let mut d = driver(alloc::vec![
            wire_for(90.0),
            wire_for(93.0),  // both in (0,180) and 3 C apart -> latches
            wire_for(200.0), // would be a 107 C jump, refused at 5 C/sample
        ]);
        assert_eq!(d.poll(), Outcome::Reading(quantised(90.0)));
        assert!(!d.driver.is_latched());
        assert_eq!(d.poll(), Outcome::Reading(quantised(93.0)));
        assert!(d.driver.is_latched());
        assert_eq!(d.poll(), Outcome::ReadFailed, "5 C/sample must bite now");
    }

    #[test]
    fn the_latch_boundary_is_strictly_less_than_five_degrees() {
        // `abs(temperature - temp) < RUNTIME_CHANGERATE` — so a gap of exactly 5
        // does not latch. 5.0 is not expressible on the grid, so this is stated on
        // the arithmetic rather than on a synthesised waveform.
        let five = protocol::RUNTIME_CHANGE_RATE_C;
        assert!((4.999f32 - five).abs() < five, "4.999 is inside the bound");
        assert_eq!(five, 5.0);
    }

    #[test]
    fn a_pair_more_than_five_degrees_apart_does_not_latch() {
        // The C++'s condition is `abs(temperature - temp) < RUNTIME_CHANGERATE`,
        // i.e. strictly less than 5.
        for gap in [6.0f32, 20.0] {
            let mut d = driver(alloc::vec![wire_for(90.0), wire_for(90.0 + gap)]);
            assert_eq!(d.poll(), Outcome::Reading(quantised(90.0)));
            assert_eq!(d.poll(), Outcome::Reading(quantised(90.0 + gap)));
            assert!(!d.driver.is_latched(), "a {gap} C gap must not latch");
        }
        // The C++'s test is `abs(temperature - temp) < RUNTIME_CHANGERATE`, i.e.
        // strictly less than 5. The 11-bit grid cannot express a gap of exactly
        // 5.000, so the smallest achievable gap above 5 is checked here and the
        // boundary itself is checked on the real numbers below.
        let just_above = quantised(90.0 + 5.1);
        assert!(
            just_above - quantised(90.0) > protocol::RUNTIME_CHANGE_RATE_C,
            "{just_above} - {} must exceed 5",
            quantised(90.0)
        );
    }

    #[test]
    fn a_reading_outside_the_window_never_latches_even_when_it_is_the_second_of_a_close_pair() {
        // 90.0 and -2.9 are 92.9 C apart, so the `abs < 5` test would fail anyway.
        // The load-bearing case is a reading *just* outside the window and just
        // under 5 C from the previous: -2.9 against 0.5 is 3.4 C, which passes
        // `abs < 5` and would latch on the C++'s *second* test alone.
        let mut d = driver(alloc::vec![wire_for(0.5), wire_for(-2.9)]);
        assert_eq!(d.poll(), Outcome::Reading(quantised(0.5)));
        assert_eq!(d.poll(), Outcome::ReadFailed);
        assert!(
            !d.driver.is_latched(),
            "the window test on the new reading is what prevents this"
        );
    }

    #[test]
    fn the_latch_is_per_instance_not_process_global() {
        // 🔴 DIVERGENCE. `validTemps` is a function-local `static` in a `const`
        // member function (`TempSensorTSIC.cpp:29`), so it is shared by every
        // instance in the process and is never reset. Two consequences the C++
        // has and this port does not: a probe that is unplugged and re-plugged
        // does not get to re-latch, and two TSIC probes would share one flag.
        let mut first = driver(alloc::vec![wire_for(90.0), wire_for(90.2)]);
        assert_eq!(first.poll(), Outcome::Reading(quantised(90.0)));
        assert_eq!(first.poll(), Outcome::Reading(quantised(90.2)));
        assert!(first.driver.is_latched());

        let mut second = driver(alloc::vec![wire_for(90.0), wire_for(90.2)]);
        assert_eq!(second.poll(), Outcome::Reading(quantised(90.0)));
        assert!(
            !second.driver.is_latched(),
            "a fresh driver must start permissive, whatever any other instance did"
        );
    }

    #[test]
    fn the_count_based_reading_of_the_rate_is_one_multiplication_away() {
        // The ambiguity, quantified. `ZACwire.cpp:58` compares the limit against
        // a gradient in **raw counts**; this port applies it in **degrees**.
        assert!((COUNT_SCALE - 200.0 / 2047.0).abs() < 1e-6);
        // So the C++'s "200" is really this many degrees per sample...
        assert!((protocol::INITIAL_CHANGE_RATE_C * COUNT_SCALE - 19.54).abs() < 0.01);
        // ...and its "5" is this many.
        assert!((protocol::RUNTIME_CHANGE_RATE_C * COUNT_SCALE - 0.4885).abs() < 0.001);
        // Neither is anywhere near the number its own name suggests, which is
        // why the module docs call this an ambiguity rather than a fix.
    }

    // ==================================================== the 221 path (C++ 1)

    #[test]
    fn ten_failed_reads_latch_the_error_exactly_like_the_dallas_path() {
        // `TempSensor::max_bad_readings_` is 10 for both drivers: it is a
        // private member of the shared base (`TempSensor.h:177`).
        let mut d = driver(alloc::vec![wire_for(-2.9)]);
        for _ in 0..ds18b20::MAX_BAD_READINGS {
            d.driver.note_overrun();
            assert_eq!(d.poll(), Outcome::ReadFailed);
        }
        assert!(d.driver.has_error());
    }

    #[test]
    fn a_ring_overrun_becomes_a_222_rather_than_a_skipped_tick() {
        let mut d = driver(alloc::vec![wire_for(90.0)]);
        d.driver.note_overrun();
        assert_eq!(d.poll(), Outcome::ReadFailed);
        assert_eq!(d.driver.bad_readings(), 1);
    }

    #[test]
    fn the_no_signal_timeout_is_longer_than_the_cpps_and_says_why() {
        // The C++'s is 100 ms (`ZACwire.h:29`) against a 10 Hz sensor, i.e. one
        // transmission period with zero margin. This port uses 2.5 periods.
        assert_eq!(protocol::UPDATE_PERIOD_US, 100_000);
        assert_eq!(protocol::NO_SIGNAL_TIMEOUT_US, 250_000);
        assert!(protocol::NO_SIGNAL_TIMEOUT_US > protocol::UPDATE_PERIOD_US);
        // So one missed frame is tolerated, and a genuinely dead probe is
        // reported inside one control cadence plus the ten-read debounce.
        assert!(protocol::NO_SIGNAL_TIMEOUT_US >= 2 * protocol::UPDATE_PERIOD_US);
        assert!(
            protocol::NO_SIGNAL_TIMEOUT_US < 2 * protocol::UPDATE_PERIOD_US + 100_000,
            "and it must not stretch to three periods or a dead probe is slow to report"
        );
    }

    #[test]
    fn only_a_quiet_line_long_enough_becomes_221() {
        let d = driver(alloc::vec![]);
        assert_eq!(
            d.driver.no_signal_outcome(Millis::new(0)),
            Outcome::ReadFailed
        );
        assert_eq!(
            d.driver.no_signal_outcome(Millis::new(99)),
            Outcome::ReadFailed
        );
        assert_eq!(
            d.driver.no_signal_outcome(Millis::new(100)),
            Outcome::ReadFailed,
            "100 ms is the C++'s threshold and is exactly one transmission period"
        );
        assert_eq!(
            d.driver.no_signal_outcome(Millis::new(250)),
            Outcome::NotConnected
        );
        assert_eq!(
            d.driver.no_signal_outcome(Millis::new(9_000)),
            Outcome::NotConnected
        );
    }

    // ============================================================== the mapping

    #[test]
    fn the_two_sentinels_map_to_different_probe_faults() {
        // 221 means "go and look at the wiring"; 222 means "this reading is not
        // trustworthy". Collapsing them would send an operator to the wrong
        // place.
        assert_eq!(
            Outcome::NotConnected.probe_fault(),
            Some(ProbeFault::NotConnected)
        );
        assert_eq!(
            Outcome::ReadFailed.probe_fault(),
            Some(ProbeFault::ReadFailed)
        );
        assert!(Outcome::NotConnected
            .probe_fault()
            .is_some_and(ProbeFault::is_disconnection));
        assert!(!Outcome::ReadFailed
            .probe_fault()
            .is_some_and(ProbeFault::is_disconnection));
    }

    #[test]
    fn a_rejected_reading_is_not_a_probe_reading() {
        assert_eq!(
            as_probe(Outcome::Reading(quantised(90.0))),
            Some(ProbeReading {
                celsius: quantised(90.0),
                source: ProbeSource::Tsic306,
            })
        );
        assert_eq!(as_probe(Outcome::ReadFailed), None);
        assert_eq!(as_probe(Outcome::NotConnected), None);
    }

    #[test]
    fn the_ring_source_hands_over_edges_and_then_nothing() {
        // The device-side seam, exercised without a device: push a transmission
        // into a real ring, drain it, and see the waveform come out the other
        // end. This is the only test that covers `RingSource` at all, and it is
        // why `RingSource` is worth having as a separate type.
        let mut source = RingSource::new();
        for edge in Waveform::for_raw(0b101_1010_1010).edges() {
            assert_eq!(source.ring().push(edge.at_us, edge.high), Ok(()));
        }
        let mut buffer = EdgeBuffer::new();
        source.take_edges(&mut buffer);
        assert_eq!(
            buffer.as_slice(),
            Waveform::for_raw(0b101_1010_1010).edges(),
            "the ring must not reorder or alter an edge"
        );
        source.take_edges(&mut buffer);
        assert!(buffer.is_empty(), "and must drain completely");
        assert!(!source.take_overrun());
        assert_eq!(source.dropped(), 0);
    }

    #[test]
    fn a_full_ring_surfaces_as_a_222_and_not_as_a_wrong_temperature() {
        // The end-to-end overflow path, through the real ring: push more edges
        // than the ring holds and confirm the driver refuses rather than
        // assembling a frame from a hole. This is the property the module docs of
        // `ring` promise.
        let source = RingSource::new();
        for at_us in 0..(2 * usize::from(protocol::TRANSMISSION_BITS) * 4) {
            // Deliberately ignore the result: the point is to overflow it.
            let _ = source.ring().push(at_us as u32, at_us % 2 == 1);
        }
        assert!(source.dropped() > 0, "the ring must have overflowed");
        let mut d = Tsic306::new(source);
        let mut buffer = EdgeBuffer::new();
        // The drain resynchronises and reports the overrun, so the driver sees
        // an empty buffer plus the flag and answers 222.
        let outcome = d.poll(&mut buffer);
        assert!(
            matches!(outcome, Outcome::ReadFailed | Outcome::Reading(_)),
            "an overflowed drain is either refused or trivially empty: {outcome}"
        );
        // And the *next* frame decodes correctly, which is the recovery property.
        let clean = RingSource::new();
        for edge in Waveform::for_raw(0b011_0001_1000).edges() {
            assert_eq!(clean.ring().push(edge.at_us, edge.high), Ok(()));
        }
        let mut d = Tsic306::new(clean);
        let mut buffer = EdgeBuffer::new();
        assert_eq!(
            d.poll(&mut buffer),
            Outcome::Reading(
                decode::Frame {
                    raw: 792,
                    strobe_us: 62
                }
                .celsius()
            )
        );
    }

    #[test]
    fn a_sequence_of_transmissions_at_ten_hertz_all_decode() {
        // The steady state the firmware depends on: the sensor transmits at
        // 10 Hz and every transmission must be usable. 50 transmissions, half of
        // them damaged, must alternate accepted and refused with no drift in the
        // previous value across a refusal.
        let mut script = Vec::new();
        for index in 0..50u32 {
            let celsius = 90.0 + (index % 3) as f32 * 0.1;
            script.push(wire_for(celsius));
            if index % 2 == 1 {
                script.push(Waveform::for_raw(0b011_0001_1000).damaged(Damage {
                    flip_data_bit: Some(2),
                    ..Damage::none()
                }));
            }
        }
        let mut d = driver(script);
        let mut accepted = 0;
        let mut refused = 0;
        for _ in 0..100 {
            match d.poll() {
                Outcome::Reading(_) => accepted += 1,
                Outcome::ReadFailed => refused += 1,
                Outcome::NotConnected => panic!("a good sequence must not report 221"),
            }
        }
        assert_eq!(accepted, 50);
        assert_eq!(refused, 50);
        assert!(
            !d.driver.has_error(),
            "50 good readings clears the counter each time"
        );
        assert!(d.driver.is_latched(), "the warm sequence latches");
    }

    #[test]
    fn a_one_wire_waveform_is_refused_by_the_zacwire_decoder() {
        // A cross-check worth having, because the two protocols share one GPIO in
        // this firmware and the wrong driver must reject the other's traffic.
        // 1-Wire slots are 65 us low out of 70, with a 480 us reset pulse first.
        //
        // Note *how* it is refused. The 480 us reset pulse is rejected outright by
        // the strobe window, but the 65 us slots sit at 52 % duty — inside the
        // window — so acquisition succeeds on one of them and the frame is
        // refused by the bit-period check instead (the slots are 70 us apart,
        // against a floor of 94). The important property is the rejection, not
        // which check produced it: either way nothing decodes, and nothing
        // reaches the PID.
        let mut edges = Vec::new();
        let mut at = 0u32;
        edges.push(Edge {
            at_us: at,
            high: false,
        });
        at += 480;
        edges.push(Edge {
            at_us: at,
            high: true,
        });
        at += 70;
        for _ in 0..40 {
            edges.push(Edge {
                at_us: at,
                high: false,
            });
            at += 65;
            edges.push(Edge {
                at_us: at,
                high: true,
            });
            at += 5;
        }
        let verdict = decode::decode_frame(&edges);
        assert!(
            verdict.is_err(),
            "1-Wire traffic must not decode: {verdict:?}"
        );
    }
}
