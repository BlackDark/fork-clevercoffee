//! The vocabulary both temperature drivers report in.
//!
//! # What this module is for
//!
//! `cc-machine` needs a temperature and a fault, and it must not know whether the probe on the
//! other end of it is a DS18B20 on 1-Wire or a TSIC-306 on `ZACwire`.
//!
//! **There is deliberately no trait.** One would have to be implemented once per driver to say what
//! the two free functions already say, and nothing needs `dyn`: `cc-machine` never holds a boxed
//! probe. What exists instead, and is host-tested, is the three types below plus
//! [`ds18b20::as_probe`] and [`tsic306::as_probe`](crate::sensor::tsic306::as_probe). Each driver
//! keeps its own concrete type -- [`ds18b20::Driver`],
//! [`tsic306::Tsic306`](crate::sensor::tsic306::Tsic306) -- and `as_probe` collapses one of its
//! outcomes onto [`ProbeReading`] / `None`. The mapping is a *decision*, and a decision belongs in
//! a function a test can drive with the driver's own outcome type.
//!
//! Neither function can block. Both drivers are non-blocking by construction -- the DS18B20
//! pipeline never waits for a conversion, and the TSIC-306 has no read call at all, only a
//! callback-driven waveform -- and `as_probe` takes an already-polled `Poll` / `Outcome`, so there
//! is nothing here that could sleep or wait.
//!
//! # Why `ProbeFault` is one enum and not the union of the two drivers' errors
//!
//! The device crates do have richer errors:
//! [`onewire::OneWireError`](crate::sensor::onewire::OneWireError) carries a
//! transport error, [`ds18b20::Ds18b20Fault`] distinguishes six sensor faults.
//! Collapsing them at this boundary would lose the `EspError` a driver got from
//! a failing peripheral, which is exactly the diagnostic a bring-up needs.
//!
//! So the lossless direction is preserved: a transport failure stays the
//! driver's own error type and never becomes a [`ProbeFault`], because a failing
//! peripheral is not a sensor reading. [`ProbeFault`] is what a device crate
//! maps its sensor-level faults onto, and it is the type the *state machine*
//! sees once it is past the device layer.

use core::fmt;

use crate::sensor::ds18b20;

/// Which bus a reading arrived on.
///
/// Carried on every reading so a log line, an MQTT payload or a web response can say *which* sensor
/// produced the number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeSource {
    /// A DS18B20 on 1-Wire.
    Ds18b20,
    /// A TSIC-306 on `ZACwire`.
    Tsic306,
}

impl ProbeSource {
    /// The name to print for this source: `DS18B20` or `TSIC-306`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ds18b20 => "DS18B20",
            Self::Tsic306 => "TSIC-306",
        }
    }
}

impl fmt::Display for ProbeSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A sensor-level fault: one the device reported, as opposed to a transport error.
///
/// The two drivers' sentinel values are collapsed onto the outcome rather than onto the number.
/// [`ReadFailed`](ProbeFault::ReadFailed) is the TSIC-306's 222 and
/// [`NotConnected`](ProbeFault::NotConnected) is its 221, and the two stay distinct because 221
/// means *the probe is gone* while 222 means *this reading was not trustworthy*: S1's debounce
/// treats a disconnected probe differently from a noisy one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeFault {
    /// Nothing is on the bus.
    ///
    /// The 1-Wire reset produced no presence pulse
    /// ([`onewire::OneWireError::NoPresence`](crate::sensor::onewire::OneWireError::NoPresence)),
    /// or no `ZACwire` start bit was seen for longer than the no-signal timeout: the TSIC-306's
    /// 221.
    NotConnected,
    /// A reading arrived but could not be trusted.
    ///
    /// The TSIC-306's 222: a frame that failed parity, or whose start bit was not a 50 % pulse, or
    /// which did not decode to a temperature the range check would accept, or which the adaptive
    /// change-rate limiter rejected.
    ///
    /// The 1-Wire CRC failure is also this, and that is the one deliberate loss of detail: a CRC
    /// failure and a short read both mean *this scratchpad does not verify*, and the driver has
    /// nothing further to say about which.
    ReadFailed,
    /// A DS18B20-specific fault.
    ///
    /// A value the bus reported that the driver will not turn into a temperature -- an out-of-range
    /// count, a power-on reset, or insufficient power. **Rejected**, and each for its own reason;
    /// see [`ds18b20`].
    Ds18b20Fault(ds18b20::Ds18b20Fault),
}

impl ProbeFault {
    /// The Celsius sentinel that stands for this fault, or `None` when it has none.
    ///
    /// -127 for a disconnection (the 1-Wire convention for a probe that is not there), 222 for a
    /// reading that could not be trusted, and whatever the DS18B20's own fault maps to -- all six
    /// of its faults carry one value, because the convention has a number for "disconnected" and no
    /// vocabulary for the rest. See [`ds18b20::Ds18b20Fault::sentinel`].
    ///
    /// `None` for a reading rejected on range. There is no sentinel for it and no log line to
    /// match: inventing a number would put one in the telemetry that means nothing outside this
    /// file.
    #[must_use]
    pub const fn sentinel(self) -> Option<f32> {
        match self {
            Self::NotConnected => Some(-127.0),
            Self::ReadFailed => Some(222.0),
            Self::Ds18b20Fault(fault) => fault.sentinel(),
        }
    }

    /// Whether the sensor is *gone*, as opposed to one reading being untrusted.
    ///
    /// Ten consecutive `NotConnected` faults mean a probe that is not there; ten consecutive
    /// `ReadFailed` faults mean a probe that is there and being misread. Only the first should send
    /// an operator to look at the wiring, so the distinction has to survive the trip through
    /// [`ProbeFault`].
    #[must_use]
    pub const fn is_disconnection(self) -> bool {
        matches!(self, Self::NotConnected)
    }
}

impl fmt::Display for ProbeFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => f.write_str("not connected"),
            Self::ReadFailed => f.write_str("read failed"),
            Self::Ds18b20Fault(fault) => write!(f, "{fault}"),
        }
    }
}

/// One accepted temperature reading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeReading {
    /// The decoded temperature in °C, **unfiltered**.
    ///
    /// Unfiltered on purpose: a value outside the sensor's physical range is what trips S1's
    /// emergency stop, and a driver that quietly held the last good value instead would turn a
    /// latched emergency stop into a machine that keeps heating. The TSIC-306 driver applies its
    /// own range before a reading gets this far -- a driver-level decision, not one made here; see
    /// [`tsic306`](crate::sensor::tsic306).
    pub celsius: f32,
    /// Which bus produced it.
    pub source: ProbeSource,
}

impl ProbeReading {
    /// Whether the reading is inside the range S1 will act on.
    ///
    /// **A query, not a filter.** It answers S1's own band, 0.0 ..= 200.0, by asking
    /// [`Celsius::is_valid`](cc_domain::units::Celsius::is_valid), so the query cannot drift from
    /// the trip test it exists to predict.
    ///
    /// Nothing here rejects a reading. Whether an out-of-band reading is rejected is each driver's
    /// own decision, and the two drivers answer it differently.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        cc_domain::units::Celsius::new(self.celsius).is_valid()
    }
}

impl fmt::Display for ProbeReading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} C ({})", self.celsius, self.source)
    }
}

// There was a `TemperatureProbe` trait here — `poll`, `has_error`,
// `bad_readings`, `source` — as the "one interface both drivers expose" that
// `sensor/mod.rs` describes. **It is deleted.** It had zero impls, zero uses as
// a bound, and zero `dyn` uses: the drivers' own `Driver` / `Tsic306` types are
// what `cc-hal-esp32` holds and what the machine is wired to, and the boundary
// it was supposed to police turned out to be two free functions —
// [`ds18b20::as_probe`] and [`tsic306::as_probe`] — that map a driver's outcome
// onto the three types above. Those are where the mapping is tested, and a trait
// nobody implemented could only ever have been tested against a mock of itself.
//
// A trait with no implementors is worse than no trait: `sensor/mod.rs` said
// `cc-machine` "takes a `&mut dyn TemperatureProbe`", which was false and would
// have stayed false until someone believed it.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_families_never_claim_each_others_sentinel() {
        // The 1-Wire disconnected sentinel is -127 and the ZACwire one is 221. If these ever
        // collide, a TSIC probe that is not connected becomes indistinguishable from a DS18B20 one.
        assert_eq!(ProbeFault::NotConnected.sentinel(), Some(-127.0));
        assert_eq!(ProbeFault::ReadFailed.sentinel(), Some(222.0));
        assert_ne!(
            ProbeFault::NotConnected.sentinel(),
            ProbeFault::ReadFailed.sentinel()
        );
        // 221 is itself implausible as a temperature, which is the property
        // `cc_domain::units`' `tsic_fault_sentinels_are_rejected` pins from the
        // other side: if it were plausible, the range check that runs *after*
        // the sentinel checks would never see it.
        assert!(!cc_domain::units::Celsius::new(222.0).is_valid());
        assert!(!cc_domain::units::Celsius::new(-127.0).is_valid());
    }

    #[test]
    fn only_a_missing_probe_counts_as_a_disconnection() {
        assert!(ProbeFault::NotConnected.is_disconnection());
        assert!(!ProbeFault::ReadFailed.is_disconnection());
        assert!(!ProbeFault::Ds18b20Fault(ds18b20::Ds18b20Fault::Disconnected).is_disconnection());
    }

    #[test]
    fn every_ds18b20_fault_carries_the_same_minus_127_sentinel() {
        // All six faults carry the same sentinel. -127 is the 1-Wire convention for a probe that is
        // not there, and the driver does not invent a number per fault -- what it keeps is the
        // *reason* for each rejection. See `cc_protocol::sensor::onewire::div7_*`.
        for fault in [
            ds18b20::Ds18b20Fault::Disconnected,
            ds18b20::Ds18b20Fault::Open,
            ds18b20::Ds18b20Fault::ShortGnd,
            ds18b20::Ds18b20Fault::ShortVdd,
            ds18b20::Ds18b20Fault::PowerOnReset,
            ds18b20::Ds18b20Fault::InsufficientPower,
        ] {
            let mapped = ProbeFault::Ds18b20Fault(fault);
            assert_eq!(
                mapped.sentinel(),
                Some(-127.0),
                "{fault} must carry the same -127 sentinel"
            );
        }
        // The fault this port added a range check for: no sentinel, because nothing outside the
        // driver can act on a number for it.
        assert_eq!(
            ProbeFault::Ds18b20Fault(ds18b20::Ds18b20Fault::OutOfRange).sentinel(),
            None
        );
    }

    #[test]
    fn usability_is_s1s_range_not_the_drivers_range() {
        // S1's band is 0.0 ..= 200.0, which is wider than either driver's own range: a reading the
        // drivers refused never reaches this query, and one between the two ranges is usable here.
        for celsius in [0.0f32, 22.9, 150.0, 200.0] {
            let reading = ProbeReading {
                celsius,
                source: ProbeSource::Ds18b20,
            };
            assert!(reading.is_usable(), "{celsius} should be usable");
        }
        for celsius in [-50.0f32, -0.1, 200.1, 250.0] {
            let reading = ProbeReading {
                celsius,
                source: ProbeSource::Tsic306,
            };
            assert!(!reading.is_usable(), "{celsius} should not be usable");
        }
        assert!(!ProbeReading {
            celsius: f32::NAN,
            source: ProbeSource::Ds18b20
        }
        .is_usable());
    }

    #[test]
    fn the_two_probe_sources_name_themselves_distinctly() {
        // The point of the name is that a log line answers *which* probe produced the number, so
        // the two sources must not print the same string.
        assert_eq!(ProbeSource::Ds18b20.name(), "DS18B20");
        assert_eq!(ProbeSource::Tsic306.name(), "TSIC-306");
        assert_ne!(ProbeSource::Ds18b20.name(), ProbeSource::Tsic306.name());
    }
}
