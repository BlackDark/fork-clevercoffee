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
    pub fn elapsed_since(self, earlier: Self, interval: Self) -> bool {
        self.since(earlier).0 >= interval.0
    }

    /// Whether the wall clock has reached `deadline`.
    ///
    /// This is **not** `self.since(deadline) == 0` and it is not
    /// `self.since(deadline) < 0`. `since` wraps, so a `self` that is *before*
    /// `deadline` produces a huge positive interval and every naive comparison
    /// gets it backwards — the bug this method exists to make impossible.
    ///
    /// The rule is the standard one: a signed 32-bit difference is negative
    /// exactly when the high bit is set, so "now has reached the deadline" is
    /// "the wrapping difference has its high bit clear". This is correct for
    /// any deadline less than 2^31 ms — 24.8 days — ahead, which every
    /// interval in this firmware is by orders of magnitude.
    #[must_use]
    pub fn has_reached(self, deadline: Self) -> bool {
        self.0.wrapping_sub(deadline.0) < 0x8000_0000
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
        Self::new(raw)
    }
}

/// Millisecond arithmetic, wrapping like the C++'s `unsigned long`.
///
/// `impl Add for Millis` is here because a deadline pipeline cannot be written
/// without "now plus an interval", and hand-rolling `Millis::new(now.raw() + n)`
/// at every call site is exactly the kind of unit mix-up these newtypes exist
/// to prevent. Wrapping rather than panicking is the same choice
/// [`Millis::since`] documents.
impl core::ops::Add for Millis {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl core::ops::AddAssign for Millis {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0.wrapping_add(rhs.0);
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
    fn millis_addition_wraps_rather_than_panicking() {
        // A deadline pipeline is written as `now + interval`, and the C++
        // relies on 32-bit wraparound to keep working across the 49.7-day
        // rollover. `saturating_add` would freeze a deadline for the last 49.7
        // days of every cycle, so this must wrap.
        assert_eq!(Millis::new(10) + Millis::new(5), Millis::new(15));
        assert_eq!(
            Millis::new(0xFFFF_FFFF) + Millis::new(2),
            Millis::new(1),
            "a deadline must wrap, not saturate"
        );
        let mut now = Millis::new(0xFFFF_FFF0);
        now += Millis::new(0x20);
        assert_eq!(now, Millis::new(0x10));
    }

    #[test]
    fn millis_deadlines_are_reached_in_the_right_order() {
        // The case that a naive `now.since(deadline) < interval` gets backwards:
        // `now` is BEFORE the deadline, so `since` wraps to ~4.29e9.
        let now = Millis::new(11);
        let deadline = now + Millis::new(50);
        assert!(
            !now.has_reached(deadline),
            "11 ms has not reached a deadline of 61 ms"
        );
        assert!(
            !(now + Millis::new(49)).has_reached(deadline),
            "49 ms short"
        );
        assert!(
            (now + Millis::new(50)).has_reached(deadline),
            "exactly on it"
        );
        assert!((now + Millis::new(51)).has_reached(deadline));
        // And across the rollover, which is where the wrapping actually bites.
        let before = Millis::new(u32::MAX - 10);
        let across = before + Millis::new(20);
        assert!(!before.has_reached(across));
        assert!(
            !(before + Millis::new(19)).has_reached(across),
            "1 ms short"
        );
        assert!((before + Millis::new(20)).has_reached(across), "wrapped");
        assert_eq!(across.raw(), 9);
    }

    #[test]
    fn duty_bound_is_the_chopper_window() {
        // ProcessState.h:183 windowSize_ = 1000
        assert!(Duty::new(0.0).is_valid());
        assert!(Duty::new(1000.0).is_valid());
        assert!(!Duty::new(1001.0).is_valid());
    }
}
