//! The emergency-stop evaluator.
//!
//! The C++ firmware had three sets of emergency thresholds that disagreed with each other: a
//! `GlobalTypes` constant of 145 C, a `Temperature.h` pair of 145 and 120 C, and the live
//! configuration of 150 C clearing at 100 C (defect D33). Any of them, read by mistake, gives the
//! wrong safety bound.
//!
//! There is one set here, it comes from the configuration, and it is evaluated by a pure
//! function, so a test can walk the whole temperature range and assert the machine's response
//! at every point.

use crate::sensor::SensorFault;

/// Configuration, in degrees Celsius and in samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thresholds {
    /// Above this, the machine stops heating and the pump.
    pub trip_c: f64,
    /// Below this, a trip may be cleared. Deliberately far below `trip_c` so a trip cannot
    /// oscillate on a noisy reading.
    pub clear_c: f64,
    /// Consecutive readings above `trip_c` needed to trip. One is enough for a reading outside
    /// the plausible range, which no real machine produces.
    pub consecutive_to_trip: u8,
    /// The C++ hysteresis, retained for the config schema even though `clear_c` is the
    /// mechanism that actually prevents chatter.
    pub hysteresis_c: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        // The values the C++ firmware actually ran with, since `safety.emergency_temp` was
        // never loaded from storage (defect D12) and these were the compiled defaults.
        Self {
            trip_c: 150.0,
            clear_c: 100.0,
            consecutive_to_trip: 3,
            hysteresis_c: 5.0,
        }
    }
}

/// What the machine should do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EmergencyDecision {
    /// Continue.
    Run,
    /// This reading was the one that tripped. The machine must stop now, without waiting for a
    /// confirmation reading.
    TripNow,
    /// The reading contributes to a trip but does not trip on its own.
    Counting,
    /// Currently tripped, and this reading is still too hot to clear.
    StayTripped,
    /// Currently tripped, and this reading is cool enough to clear.
    Clear,
}

/// The evaluator. Holds only the consecutive count, so it is `Copy` and cannot be shared.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmergencyStop {
    high_count: u8,
    tripped: bool,
}

impl EmergencyStop {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn is_tripped(&self) -> bool {
        self.tripped
    }

    /// Forces a trip. Used for a sensor fault, where the reading is not trustworthy rather than
    /// merely high.
    pub fn force_trip(&mut self) {
        self.tripped = true;
        self.high_count = 0;
    }

    /// Resets without clearing a trip, for a machine that has just been power-cycled.
    pub fn reset_counters(&mut self) {
        self.high_count = 0;
    }

    /// Evaluates one reading.
    pub fn evaluate(&mut self, celsius: Option<f64>, t: &Thresholds) -> EmergencyDecision {
        // No reading means the sensor is not trusted. A machine whose heater control depends on
        // a sensor it cannot read must not keep heating.
        let Some(celsius) = celsius else {
            self.force_trip();
            return EmergencyDecision::TripNow;
        };

        // A NaN passes every comparison in the C++ range check, so the C++ treated it as a
        // valid reading and kept heating. That is a hole in the safety check rather than parity
        // worth keeping, and the port is where it is closed.
        if !celsius.is_finite() {
            self.force_trip();
            return EmergencyDecision::TripNow;
        }

        if self.tripped {
            if celsius < t.clear_c {
                self.tripped = false;
                self.high_count = 0;
                return EmergencyDecision::Clear;
            }
            return EmergencyDecision::StayTripped;
        }

        if celsius >= t.trip_c {
            self.high_count = self.high_count.saturating_add(1);
            if self.high_count >= t.consecutive_to_trip {
                self.tripped = true;
                self.high_count = 0;
                return EmergencyDecision::TripNow;
            }
            return EmergencyDecision::Counting;
        }

        // Below the trip point, a partial count does not persist. A machine that saw one hot
        // reading an hour ago must not trip on the next one.
        self.high_count = 0;
        EmergencyDecision::Run
    }
}

/// Converts a sensor fault into a trip, for the one case where a fault is itself an emergency.
pub const fn fault_is_emergency(fault: SensorFault) -> bool {
    // Every fault, including out of range. A reading of 250 C is either a broken part or a
    // disconnected bus with the CRC check bypassed, and both mean the sensor cannot be trusted.
    matches!(
        fault,
        SensorFault::Disconnected
            | SensorFault::Corrupt
            | SensorFault::Timeout
            | SensorFault::OutOfRange { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_reading_does_nothing() {
        let mut e = EmergencyStop::new();
        let t = Thresholds::default();
        for c in [20.0, 80.0, 94.0, 95.0, 140.0] {
            assert_eq!(e.evaluate(Some(c), &t), EmergencyDecision::Run);
        }
        assert!(!e.is_tripped());
    }

    #[test]
    fn a_single_hot_reading_does_not_trip_by_default() {
        let mut e = EmergencyStop::new();
        let t = Thresholds::default();
        assert_eq!(e.evaluate(Some(151.0), &t), EmergencyDecision::Counting);
        assert!(!e.is_tripped());
    }

    #[test]
    fn three_consecutive_hot_readings_trip() {
        let mut e = EmergencyStop::new();
        let t = Thresholds::default();
        assert_eq!(e.evaluate(Some(151.0), &t), EmergencyDecision::Counting);
        assert_eq!(e.evaluate(Some(152.0), &t), EmergencyDecision::Counting);
        assert_eq!(e.evaluate(Some(153.0), &t), EmergencyDecision::TripNow);
        assert!(e.is_tripped());
    }

    #[test]
    fn a_hot_reading_separated_by_a_cold_one_does_not_accumulate() {
        let mut e = EmergencyStop::new();
        let t = Thresholds::default();
        assert_eq!(e.evaluate(Some(151.0), &t), EmergencyDecision::Counting);
        assert_eq!(e.evaluate(Some(90.0), &t), EmergencyDecision::Run);
        assert_eq!(e.evaluate(Some(151.0), &t), EmergencyDecision::Counting);
        assert!(!e.is_tripped(), "the count must not survive a cool reading");
    }

    #[test]
    fn a_trip_holds_until_the_reading_is_well_below() {
        let mut e = EmergencyStop::new();
        let t = Thresholds::default();
        e.force_trip();
        // Between the clear point and the trip point it stays tripped. This is the hysteresis
        // that stops a machine flapping in and out of emergency.
        assert_eq!(e.evaluate(Some(130.0), &t), EmergencyDecision::StayTripped);
        assert_eq!(e.evaluate(Some(110.0), &t), EmergencyDecision::StayTripped);
        assert_eq!(e.evaluate(Some(99.0), &t), EmergencyDecision::Clear);
        assert!(!e.is_tripped());
    }

    #[test]
    fn no_reading_at_all_trips_immediately() {
        // The C++ firmware had a disconnected sensor reading as 0.0 and carried on heating. Here
        // the absence of a reading is itself the emergency.
        let mut e = EmergencyStop::new();
        let t = Thresholds::default();
        assert_eq!(e.evaluate(None, &t), EmergencyDecision::TripNow);
        assert!(e.is_tripped());
    }

    #[test]
    fn a_single_reading_mode_trips_at_once() {
        let mut e = EmergencyStop::new();
        let t = Thresholds {
            consecutive_to_trip: 1,
            ..Thresholds::default()
        };
        assert_eq!(e.evaluate(Some(151.0), &t), EmergencyDecision::TripNow);
    }

    #[test]
    fn thresholds_come_from_one_place() {
        // The default is the C++ runtime behaviour: 150 to trip, 100 to clear. The point of the
        // test is that there is no second constant anywhere to disagree with it.
        let t = Thresholds::default();
        assert_eq!(t.trip_c, 150.0);
        assert_eq!(t.clear_c, 100.0);
        assert!(
            t.clear_c < t.trip_c,
            "a clear point above the trip point would chatter"
        );
    }

    #[test]
    fn a_not_a_number_trips() {
        // Every comparison with NaN is false, so a naive range check lets it through. The C++ had
        // that hole; the port closes it.
        let mut e = EmergencyStop::new();
        let t = Thresholds::default();
        assert_eq!(e.evaluate(Some(f64::NAN), &t), EmergencyDecision::TripNow);
        assert!(e.is_tripped());
        let mut e2 = EmergencyStop::new();
        assert_eq!(
            e2.evaluate(Some(f64::INFINITY), &t),
            EmergencyDecision::TripNow
        );
    }

    #[test]
    fn a_disconnect_is_an_emergency_but_an_out_of_range_reading_is_too() {
        assert!(fault_is_emergency(SensorFault::Disconnected));
        assert!(fault_is_emergency(SensorFault::Corrupt));
        assert!(fault_is_emergency(SensorFault::Timeout));
        assert!(fault_is_emergency(SensorFault::OutOfRange {
            celsius: 250.0
        }));
    }
}
