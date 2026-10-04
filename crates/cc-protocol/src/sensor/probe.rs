//! The vocabulary both temperature drivers report in.
//!
//! Owner: **R1-03** and **R3-07**.
//!
//! # What this module is for
//!
//! `cc-machine` needs a temperature and a fault, and it must not know whether
//! the probe on the other end of it is a DS18B20 on 1-Wire or a TSIC-306 on
//! `ZACwire`. The C++ gets that from `TempSensor` being a base class
//! (`include/clevercoffee/hardware/tempsensors/TempSensor.h:17`).
//!
//! **In Rust it is not a trait here.** It was — `TemperatureProbe`, with
//! `poll` / `has_error` / `bad_readings` / `source` — and it had **zero impls**.
//! What actually exists, and is host-tested, is the three types below plus two
//! free functions: [`ds18b20::as_probe`] and
//! [`tsic306::as_probe`](crate::sensor::tsic306::as_probe). Each driver
//! keeps its own concrete type — [`ds18b20::Driver`],
//! [`tsic306::Tsic306`](crate::sensor::tsic306::Tsic306) — and
//! `as_probe` collapses one of its outcomes onto [`ProbeReading`] / `None`. A
//! trait with no implementors could only ever have been tested against a mock of
//! itself; the mapping is a *decision*, and decisions belong in functions where a
//! test can drive them with the driver's own outcome type.
//!
//! Non-blocking is preserved at the type level by the drivers themselves, not by
//! this boundary. Both were already non-blocking in the C++ —
//! `TempSensorDallas` calls `setWaitForConversion(false)`
//! (`TempSensorDallas.cpp:24`) and the TSIC has no read call at all, only a
//! callback-driven waveform — and `as_probe` takes an already-polled `Poll` /
//! `Outcome`, so nothing here can sleep or wait.
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
/// Carried on every reading so a log line, an MQTT payload or a web response
/// can say *which* sensor produced the number. The C++ cannot: `TempSensor`
/// exposes `getSensorType()` returning the literal `"TempSensor"` for both
/// drivers (`TempSensor.h:148-151`), so a diagnostic that wanted to name the
/// probe had no way to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeSource {
    /// A DS18B20 on 1-Wire.
    Ds18b20,
    /// A TSIC-306 on `ZACwire`.
    Tsic306,
}

impl ProbeSource {
    /// The name the C++'s `TempSensor::getSensorType` would print, had it been
    /// specific. `DallasTemperature` and `ZACwire` are the two driver types
    /// `TempSensorDallas` / `TempSensorTSIC` wrap (`TempSensorDallas.cpp:5`,
    /// `TempSensorTSIC.cpp:16`).
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

/// A sensor-level fault, i.e. one the bus reported rather than the transport.
///
/// This is the union of what the two C++ drivers can say about a *reading*,
/// with the two drivers' own sentinel values collapsed onto the outcome rather
/// than the number. [`ReadFailed`](ProbeFault::ReadFailed) is `ZACwire`'s 222
/// and [`NotConnected`](ProbeFault::NotConnected) is its 221
/// (`ZACwire.h:30-31`); both are preserved as distinct variants because the C++
/// logs them differently and, more importantly, 221 means *the probe is gone*
/// while 222 means *this reading was not trustworthy* — S1's debounce treats a
/// disconnected probe differently from a noisy one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeFault {
    /// Nothing is on the bus.
    ///
    /// The 1-Wire reset produced no presence pulse
    /// ([`onewire::OneWireError::NoPresence`](crate::sensor::onewire::OneWireError::NoPresence)),
    /// or no `ZACwire` start bit was
    /// seen for longer than the no-signal timeout — which is what
    /// `ZACwire::connectionCheck` reports as 221.
    NotConnected,
    /// A reading arrived but could not be trusted.
    ///
    /// The C++'s 222, `ZACwire::errorMisreading`: a frame that failed parity,
    /// or whose start bit was not a 50 % pulse, or which did not decode to a
    /// temperature the C++'s range check would accept, or which the adaptive
    /// change-rate limiter rejected. The one deliberate loss of detail: the
    /// 1-Wire CRC failure is also this, because `TempSensorDallas` cannot tell
    /// a CRC failure from a short read either — both arrive as
    /// `DEVICE_DISCONNECTED_C` (`TempSensorDallas.cpp:29-31`).
    ReadFailed,
    /// A DS18B20-specific fault.
    ///
    /// A power-on reset (`-251`) or insufficient power (`-250`). **Rejected**,
    /// unlike the C++ — see [`ds18b20`] and 09 §17/§20.
    Ds18b20Fault(ds18b20::Ds18b20Fault),
}

impl ProbeFault {
    /// The Celsius sentinel the C++ would have logged for this fault.
    ///
    /// `TempSensorTSIC.cpp:46-54` logs `"Temperature reading failed"` for 222
    /// and `"Temperature sensor not connected"` for 221;
    /// `TempSensorDallas` logs `"Temperature sensor not connected"` for
    /// `DEVICE_DISCONNECTED_C` (-127) and `"Issue with temperature sensor
    /// connection, check wiring"` for the three wiring faults. The 1-Wire column
    /// is uniform because `rawToCelsius` folds every raw sentinel to -127 — see
    /// [`ds18b20::Ds18b20Fault::cpp_sentinel`].
    ///
    /// `None` for a reading the driver rejected on range: the C++ has no such
    /// check, so it has no sentinel and no log line, and inventing one would be
    /// a claim about the C++ that is not true.
    #[must_use]
    pub const fn cpp_sentinel(self) -> Option<f32> {
        match self {
            Self::NotConnected => Some(-127.0),
            Self::ReadFailed => Some(222.0),
            Self::Ds18b20Fault(fault) => fault.cpp_sentinel(),
        }
    }

    /// Whether the sensor is *gone*, as opposed to one reading being untrusted.
    ///
    /// This is the distinction S1 needs and the C++ does not make: ten
    /// consecutive `NotConnected` faults mean a probe that is not there, and ten
    /// consecutive `ReadFailed` faults mean a probe that is there and being
    /// misread. Both latch `error_` in the C++; only the first should make an
    /// operator go and look at the wiring.
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
    /// Unfiltered because the C++ is: a value outside the sensor's physical
    /// range is what trips S1's emergency stop
    /// (`EmergencyStopManager.cpp:25-30`), and a driver that quietly held the
    /// last good value instead would turn a latched emergency stop into a
    /// machine that keeps heating. The TSIC driver is the exception the C++
    /// itself makes, and it is a driver-level decision, not a trait-level one —
    /// see [`tsic306`](crate::sensor::tsic306).
    pub celsius: f32,
    /// Which bus produced it.
    pub source: ProbeSource,
}

impl ProbeReading {
    /// Whether the reading is inside the range S1 will act on.
    ///
    /// **A query, not a filter.** `Temperature::MIN_VALID_TEMP_C` (0.0) and
    /// `MAX_VALID_TEMP_C` (200.0), `constants/Temperature.h:14-15`, enforced by
    /// `EmergencyStopManager::checkEmergencyConditions`
    /// (`EmergencyStopManager.cpp:25-30`).
    ///
    /// This is the range `TempSensor::isValidTemperature` (`TempSensor.h:91-93`,
    /// -50..150) *would* have been if anything had called it. Nothing did
    /// (09 §18), so this function exists to make the question answerable
    /// without making the answer a filter.
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
        // The 1-Wire disconnected sentinel is -127 and the ZACwire one is 221.
        // If these ever collide, a TSIC not-connected would be indistinguishable
        // from a DS18B20 one, and `TempSensorTSIC.cpp:51` would be comparing a
        // -127 against 221 and never matching.
        assert_eq!(ProbeFault::NotConnected.cpp_sentinel(), Some(-127.0));
        assert_eq!(ProbeFault::ReadFailed.cpp_sentinel(), Some(222.0));
        assert_ne!(
            ProbeFault::NotConnected.cpp_sentinel(),
            ProbeFault::ReadFailed.cpp_sentinel()
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
    fn the_ds18b20_power_on_faults_carry_the_cpps_uniform_minus_127() {
        // 🔴 CORRECTED. An earlier version of this repository claimed the C++
        // reports these as -251 and -250. It does not: `rawToCelsius`
        // (`DallasTemperature.cpp:406-410`) folds every raw sentinel to -127, so
        // *all six* DS18B20 faults reach `TempSensorDallas` as -127 and are
        // rejected as "not connected". What the port keeps is the *reason*.
        // See `cc_protocol::sensor::onewire::div7_*`.
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
                mapped.cpp_sentinel(),
                Some(-127.0),
                "{fault} is -127 to the C++, not a distinct sentinel"
            );
        }
        // The one with no C++ sentinel at all is the one this port adds.
        assert_eq!(
            ProbeFault::Ds18b20Fault(ds18b20::Ds18b20Fault::OutOfRange).cpp_sentinel(),
            None
        );
    }

    #[test]
    fn usability_is_s1s_range_not_the_dead_helpers_range() {
        // `TempSensor::isValidTemperature` is -50..150 (`TempSensor.h:91-93`) and
        // is never called. S1's is 0.0..200.0 (`constants/Temperature.h:14-15`).
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
        // The C++'s `getSensorType()` returns the literal "TempSensor" for both
        // (`TempSensor.h:148-151`), so this is the fix rather than the parity.
        assert_eq!(ProbeSource::Ds18b20.name(), "DS18B20");
        assert_eq!(ProbeSource::Tsic306.name(), "TSIC-306");
        assert_ne!(ProbeSource::Ds18b20.name(), ProbeSource::Tsic306.name());
    }
}
