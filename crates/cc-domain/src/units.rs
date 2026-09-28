//! Unit newtypes.
//!
//! Every physical quantity in the firmware is wrapped. A bare `f32` in a
//! function signature is a defect waiting to happen: passing millimetres where
//! inches are expected is a class of bug the type system is here to prevent.
//!
//! Two different kinds of bound live here, and they must not be confused:
//!
//! * **Sensor validity** — the range outside which a *reading* cannot be
//!   believed. `Celsius::is_plausible` is the one safety path S1 depends on
//!   (`EmergencyStopManager.cpp:25`).
//! * **Configuration range** — the range a *user setting* may take. Those live
//!   with the parameter that owns them, in `cc-config`, not here, because they
//!   are policy rather than physics.

/// Define a `f32` newtype with a physical validity range and a `Display` impl.
macro_rules! float_unit {
    (
        $(#[$meta:meta])*
        $name:ident, $min:expr, $max:expr
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
        pub struct $name(f32);

        impl $name {
            /// Smallest reading the hardware can be trusted to produce.
            pub const MIN: Self = Self($min);
            /// Largest reading the hardware can be trusted to produce.
            pub const MAX: Self = Self($max);

            /// The raw value. Named rather than `Deref` so that a unit can never
            /// be silently used as the unit it wraps.
            #[must_use]
            pub const fn raw(self) -> f32 {
                self.0
            }

            /// Build from a raw value without validating. Prefer the validating
            /// constructor; this exists for arithmetic and for deserialisation.
            #[must_use]
            pub const fn new(raw: f32) -> Self {
                Self(raw)
            }

            /// Build from a raw value, rejecting anything outside
            /// `[MIN, MAX]`.
            ///
            /// Returns `None` rather than clamping: a clamped sensor reading
            /// looks like a plausible temperature, and a plausible wrong
            /// temperature is more dangerous than an obviously bad one.
            #[must_use]
            pub fn checked(raw: f32) -> Option<Self> {
                if !($min..=$max).contains(&raw) {
                    return None;
                }
                Some(Self(raw))
            }

            /// Whether this reading is physically possible, i.e. inside
            /// `[MIN, MAX]` and not `NaN`.
            ///
            /// This is safety path S1's gate: an implausible reading trips
            /// emergency stop immediately, with no debounce.
            #[must_use]
            pub fn is_valid(self) -> bool {
                // `RangeInclusive::contains` on floats is false for NaN, which
                // is exactly the behaviour wanted here.
                ($min..=$max).contains(&self.0)
            }
        }

        impl From<f32> for $name {
            fn from(raw: f32) -> Self {
                Self(raw)
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                // Three decimals is enough for every use in this firmware and
                // keeps the OLED render deterministic.
                write!(f, "{:.3}", self.0)
            }
        }
    };
}

float_unit!(
    /// A temperature in degrees Celsius.
    ///
    /// The validity range is the C++ sensor-plausibility range from
    /// `include/clevercoffee/constants/Temperature.h:14-15`
    /// (`MIN_VALID_TEMP_C = 0.0`, `MAX_VALID_TEMP_C = 200.0`), which is the
    /// range `EmergencyStopManager::checkEmergencyConditions` uses to reject a
    /// disconnected or faulted probe.
    Celsius, 0.0, 200.0
);

float_unit!(
    /// A pressure in bar.
    ///
    /// Range from the fitted sensor: `ABP2_pmin` / `ABP2_pmax` in
    /// `include/clevercoffee/hardware/pressureSensor.h:19-20`, which describe
    /// the ABP2LANT010BG2A3XX 0-10 bar differential gauge.
    Bar, 0.0, 10.0
);

float_unit!(
    /// A mass in grams.
    ///
    /// Range from the widest mass the firmware's scale calibration can express:
    /// `SCALE_KNOWN_WEIGHT_MAX` = 2000 g (`include/clevercoffee/defaults.h:117`).
    Grams, 0.0, 2000.0
);

float_unit!(
    /// A duration in seconds.
    ///
    /// The longest interval the firmware schedules is
    /// `HASSIO_DISCOVERY_INTERVAL_MS` = 300 s
    /// (`include/clevercoffee/constants/Timing.h:45`); the upper bound is an
    /// hour, which is comfortably above anything a brewing state needs and low
    /// enough that a unit mix-up (seconds for milliseconds, say) is caught.
    Seconds, 0.0, 3600.0
);

float_unit!(
    /// A heater duty, expressed in milliseconds of energisation per chopper
    /// window.
    ///
    /// The C++ heater output is a 1 Hz chopper whose window is
    /// `windowSize_ = 1000` ms (`include/clevercoffee/context/ProcessState.h:183`),
    /// and the PID output is bounded by exactly that window
    /// (`SystemInitializer.cpp:552`, `setPidOutputLimits(0, processWindowSize())`).
    /// So the PID output *is* a millisecond duty, and the upper bound is the
    /// window length. R1-07 may move this to LEDC, which would change the unit
    /// — that is a recorded behaviour change, not a refactor (04 §5).
    Duty, 0.0, 1000.0
);

/// A millisecond timestamp or interval.
///
/// Wrapping rather than panicking is deliberate: the C++ does
/// `unsigned long now = millis(); timeChange = now - lastTime;`
/// (`PID_v1.cpp:60-61`) on a 32-bit `unsigned long`, so it deliberately relies
/// on `u32` wraparound to keep working across the 49.7-day rollover. A Rust
/// port that panicked there would be a behaviour change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Millis(u32);

impl Millis {
    /// The timestamp/interval zero.
    pub const ZERO: Self = Self(0);

    /// Wrap a raw millisecond count.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw millisecond count.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Time elapsed from `earlier` to `self`, wrapping like the C++.
    ///
    /// The result is only meaningful when it is genuinely smaller than ~24 days;
    /// longer intervals are indistinguishable from a short one, exactly as in
    /// the C++.
    #[must_use]
    pub const fn since(self, earlier: Self) -> Self {
        Self(self.0.wrapping_sub(earlier.0))
    }

    /// Whether at least `interval` has elapsed since `earlier`.
    #[must_use]
    pub const fn elapsed_since(self, earlier: Self, interval: Self) -> bool {
        self.since(earlier).0 >= interval.0
    }

    /// Seconds as an `f64`, for the few places that need a float duration.
    #[must_use]
    pub fn as_secs_f64(self) -> f64 {
        // Milliseconds are 2^32 at most, which is exactly representable in an
        // f64, and the division is by a power of two. Exact.
        f64::from(self.0) / 1000.0
    }
}

impl From<u32> for Millis {
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}

impl core::fmt::Display for Millis {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn celsius_validity_matches_the_cpp_sensor_range() {
        // constants/Temperature.h:14-15
        assert!(Celsius::new(0.0).is_valid());
        assert!(Celsius::new(200.0).is_valid());
        assert!(!Celsius::new(-0.1).is_valid());
        assert!(!Celsius::new(200.1).is_valid());
        assert!(!Celsius::new(f32::NAN).is_valid());
    }

    #[test]
    fn tsic_fault_sentinels_are_rejected() {
        // 01 §3: the TSIC-306 driver reports 221 / 222 for a faulted probe.
        // Both must be implausible, or S1 would debounce a hard sensor fault.
        assert!(!Celsius::new(221.0).is_valid());
        assert!(!Celsius::new(222.0).is_valid());
    }

    #[test]
    fn checked_rejects_rather_than_clamps() {
        assert_eq!(Celsius::checked(95.0), Some(Celsius::new(95.0)));
        assert_eq!(Celsius::checked(250.0), None);
    }

    #[test]
    fn bar_range_matches_the_abp2_sensor() {
        assert!(Bar::new(0.0).is_valid());
        assert!(Bar::new(10.0).is_valid());
        assert!(!Bar::new(10.5).is_valid());
    }

    #[test]
    fn millis_since_wraps_like_unsigned_long() {
        let before = Millis::new(0xFFFF_F000);
        let after = Millis::new(0x0000_0100);
        assert_eq!(after.since(before), Millis::new(0x1100));
        assert!(after.elapsed_since(before, Millis::new(1000)));
        assert!(!after.elapsed_since(before, Millis::new(5000)));
    }

    #[test]
    fn duty_bound_is_the_chopper_window() {
        // ProcessState.h:183 windowSize_ = 1000
        assert!(Duty::new(0.0).is_valid());
        assert!(Duty::new(1000.0).is_valid());
        assert!(!Duty::new(1001.0).is_valid());
    }
}
