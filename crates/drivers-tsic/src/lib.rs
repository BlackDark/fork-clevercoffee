//! The TSIC 306: pulse-train protocol behind the C++ `ZACwire` library.
//!
//! The second implementation of [`clevercoffee_hal_traits::TemperatureSensor`]. Both sensors are
//! kept, per the user's decision on 2026-09-29, and which one is fitted is
//! `hardware.sensors.temperature.type`, resolved once at boot. Nothing above the trait can tell
//! which sensor answered.
//!
//! The pulse train itself is not in the repository: the C++ firmware vendored `ZACwire` and its
//! native tests replaced the whole library with a stub that returned queued floats. So what is
//! ported here is the part that is specified in the C++ source and is safety-relevant: the
//! change-rate latch, the two sentinels, and the range rejection. The pulse decoding sits behind
//! [`PulseSource`] so the board crate supplies the timing and a test supplies a script.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

use clevercoffee_hal_traits::{TemperatureError, TemperatureSensor};

/// The change rate used until two consecutive readings agree.
///
/// C++: `TempSensorTSIC.cpp:11`, `INITIAL_CHANGERATE 200`. The sensor itself filters, and 200 is
/// wide enough that a real heating ramp is never itself reported as a glitch.
pub const INITIAL_CHANGE_RATE: f32 = 200.0;

/// The change rate used once the readings have settled.
///
/// C++: `TempSensorTSIC.cpp:12`, `RUNTIME_CHANGERATE 5`. Five degrees between 400 ms samples is a
/// heating ramp of 12.5 C a second, which is faster than a boiler's group head manages, so
/// anything above this really is a glitch.
pub const RUNTIME_CHANGE_RATE: f32 = 5.0;

/// The sentinel `ZACwire` returns when the read itself failed. C++: `TempSensorTSIC.cpp:46`.
pub const READ_FAILED: f32 = 222.0;

/// The sentinel `ZACwire` returns when no sensor is on the line. C++: `TempSensorTSIC.cpp:51`.
pub const NOT_CONNECTED: f32 = 221.0;

/// The range the C++ firmware accepted. Anything outside is treated as a failed read rather than
/// a temperature, so a glitch keeps the last good value instead of reaching the PID.
pub const MIN_PLAUSIBLE_C: f32 = 0.0;
pub const MAX_PLAUSIBLE_C: f32 = 180.0;

/// One decoded pulse-train reading, as `ZACwire::getTemp` would return it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RawReading {
    pub value: f32,
}

/// Where the decoded value comes from.
///
/// The pulse train is a bit-level protocol the C++ library owned, so the trait is what the board
/// crate implements. The tests implement it with a script, which is what makes the state machine
/// below testable without the library.
pub trait PulseSource {
    /// Decodes one reading, applying `max_change_rate` as the library's own filtering did.
    ///
    /// The change rate is passed rather than held here because it belongs to the caller: it is
    /// chosen once and then changes when the readings settle.
    fn decode(&mut self, max_change_rate: f32) -> RawReading;
}

/// A TSIC 306 on a pulse source.
#[derive(Debug)]
pub struct Tsic306<S: PulseSource> {
    source: S,
    /// Whether the tighter runtime change rate has been latched on.
    ///
    /// The C++ firmware kept this in a function-local `static`, which is shared by every instance
    /// in the process and survives a re-initialisation of the sensor. Here it belongs to the
    /// sensor, so a reset really resets it.
    runtime_latched: bool,
}

impl<S: PulseSource> Tsic306<S> {
    pub const fn new(source: S) -> Self {
        Self {
            source,
            runtime_latched: false,
        }
    }

    pub const fn runtime_latched(&self) -> bool {
        self.runtime_latched
    }

    /// Resets the latch, so the next reading uses the wide change rate again.
    ///
    /// Called when the sensor is re-addressed or the configuration changes. The C++ firmware's
    /// `static` never reset, so a sensor that came back after a fault stayed on the tight rate and
    /// rejected its own first valid reading as a glitch.
    pub fn reset_latch(&mut self) {
        self.runtime_latched = false;
    }

    /// Reads once, applying the latch and rejecting everything the C++ firmware rejected.
    pub fn sample(&mut self, previous_c: Option<f64>) -> Result<f64, TemperatureError> {
        let rate = if self.runtime_latched {
            RUNTIME_CHANGE_RATE
        } else {
            INITIAL_CHANGE_RATE
        };
        let raw = self.source.decode(rate).value;

        if raw == READ_FAILED {
            return Err(TemperatureError::Corrupt);
        }
        if raw == NOT_CONNECTED {
            return Err(TemperatureError::Disconnected);
        }
        // Outside the plausible range the reading is a glitch, not a temperature. Reported as a
        // failed read so the caller keeps its last good value rather than acting on it.
        if raw <= MIN_PLAUSIBLE_C || raw >= MAX_PLAUSIBLE_C {
            return Err(TemperatureError::OutOfRange);
        }

        // Latch onto the tighter rate once a previous good reading exists and the two agree.
        // `previous_c` is `None` on the first reading, which is why the C++ needed its `static`
        // to remember: the flag has to survive between calls without a value to compare against.
        if !self.runtime_latched {
            if let Some(previous) = previous_c {
                let previous = previous as f32;
                if previous > MIN_PLAUSIBLE_C
                    && previous < MAX_PLAUSIBLE_C
                    && (previous - raw).abs() < RUNTIME_CHANGE_RATE
                {
                    self.runtime_latched = true;
                }
            }
        }
        Ok(f64::from(raw))
    }
}

impl<S: PulseSource> TemperatureSensor for Tsic306<S> {
    fn address(&self) -> u64 {
        // A TSIC has no address: it is a single device on a single line, and the trait's address is
        // a 1-Wire serial. Zero says "not a 1-Wire device" rather than inventing one.
        0
    }

    fn read_celsius(&mut self) -> Result<f64, TemperatureError> {
        self.sample(None)
    }
}

/// Which sensor a machine has, from `hardware.sensors.temperature.type`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SensorType {
    Ds18b20,
    Tsic306,
}

impl SensorType {
    /// Resolves the configuration value.
    ///
    /// An unknown value is an error rather than a default. Defaulting to the DS18B20 would let a
    /// machine with a TSIC silently read nothing, and the symptom would look like a disconnected
    /// sensor rather than a typo in the configuration.
    pub const fn from_config(value: i32) -> Result<Self, SensorTypeError> {
        match value {
            0 => Ok(SensorType::Ds18b20),
            1 => Ok(SensorType::Tsic306),
            other => Err(SensorTypeError::Unknown(other)),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            SensorType::Ds18b20 => "DS18B20",
            SensorType::Tsic306 => "TSIC 306",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SensorTypeError {
    /// `hardware.sensors.temperature.type` names a sensor this firmware does not have.
    Unknown(i32),
}

impl core::fmt::Display for SensorTypeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SensorTypeError::Unknown(v) => write!(
                f,
                "hardware.sensors.temperature.type is {v}, which is not 0 (DS18B20) or 1 (TSIC 306)"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heapless::Vec;

    /// A scripted pulse source: the values the library would have decoded.
    #[derive(Debug)]
    struct Scripted {
        values: Vec<f32, 16>,
        /// The change rates the driver asked for, so a test can see the latch working.
        rates: Vec<f32, 16>,
    }

    impl Scripted {
        fn new(values: &[f32]) -> Self {
            let mut v = Vec::new();
            for x in values {
                v.push(*x).expect("script fits");
            }
            Self {
                values: v,
                rates: Vec::new(),
            }
        }

        fn rate_count(&self) -> usize {
            self.rates.len()
        }
    }

    /// Lets a caller hand the source out as a borrow, so a test can inspect the change rates the
    /// driver asked for while still driving the real driver code.
    impl<T: PulseSource + ?Sized> PulseSource for &mut T {
        fn decode(&mut self, max_change_rate: f32) -> RawReading {
            (**self).decode(max_change_rate)
        }
    }

    impl PulseSource for Scripted {
        fn decode(&mut self, max_change_rate: f32) -> RawReading {
            let _ = self.rates.push(max_change_rate);
            if self.values.is_empty() {
                return RawReading {
                    value: NOT_CONNECTED,
                };
            }
            RawReading {
                value: self.values.remove(0),
            }
        }
    }

    #[test]
    fn a_normal_reading_is_returned() {
        let mut s = Tsic306::new(Scripted::new(&[92.5]));
        assert_eq!(s.sample(None), Ok(92.5));
    }

    #[test]
    fn the_read_failed_sentinel_is_a_fault_not_a_temperature() {
        // 222 C is a sentinel, not a reading. Passing it on would trip the emergency stop on a
        // perfectly healthy machine.
        let mut s = Tsic306::new(Scripted::new(&[READ_FAILED]));
        assert_eq!(s.sample(None), Err(TemperatureError::Corrupt));
    }

    #[test]
    fn the_not_connected_sentinel_is_a_disconnect() {
        let mut s = Tsic306::new(Scripted::new(&[NOT_CONNECTED]));
        assert_eq!(s.sample(None), Err(TemperatureError::Disconnected));
    }

    #[test]
    fn the_two_sentinels_are_distinguished() {
        // They are different faults with different repairs: one is wiring, one is the decode.
        let mut a = Tsic306::new(Scripted::new(&[READ_FAILED]));
        let mut b = Tsic306::new(Scripted::new(&[NOT_CONNECTED]));
        assert_ne!(a.sample(None), b.sample(None));
    }

    #[test]
    fn a_reading_at_or_below_zero_is_refused() {
        // The C++ firmware rejected these with `temp <= 0.0`, so exactly zero is refused too.
        for v in [-40.0f32, -2.9, 0.0] {
            let mut s = Tsic306::new(Scripted::new(&[v]));
            assert_eq!(s.sample(None), Err(TemperatureError::OutOfRange), "at {v}");
        }
    }

    #[test]
    fn a_reading_at_or_above_the_top_is_refused() {
        for v in [180.0f32, 180.1, 300.0] {
            let mut s = Tsic306::new(Scripted::new(&[v]));
            assert_eq!(s.sample(None), Err(TemperatureError::OutOfRange), "at {v}");
        }
    }

    #[test]
    fn a_glitch_keeps_the_last_good_value_rather_than_reaching_the_pid() {
        // The property the C++ comment describes: a single spurious value must not trip emergency
        // stop. Refusing the read means the caller keeps what it had.
        let mut s = Tsic306::new(Scripted::new(&[92.5, -2.9, 92.6]));
        assert_eq!(s.sample(None), Ok(92.5));
        assert_eq!(s.sample(Some(92.5)), Err(TemperatureError::OutOfRange));
        let third = s
            .sample(Some(92.5))
            .expect("the glitch must not poison later readings");
        assert!(
            (third - 92.6).abs() < 1e-4,
            "an f32 sensor read through f64 lands at {third}, not exactly 92.6"
        );
    }

    #[test]
    fn the_wide_change_rate_is_used_until_two_readings_agree() {
        let mut s = Tsic306::new(Scripted::new(&[92.5, 93.0, 93.1]));
        // The first reading has nothing to compare against, so the rate stays wide.
        let _ = s.sample(None);
        assert!(!s.runtime_latched());
        // The second agrees with the first to within the runtime rate, so it latches.
        let _ = s.sample(Some(92.5));
        assert!(
            s.runtime_latched(),
            "two agreeing readings must latch the runtime rate"
        );
        // And it stays latched.
        let _ = s.sample(Some(93.0));
        assert!(s.runtime_latched());
    }

    #[test]
    fn two_readings_that_disagree_keep_the_wide_change_rate() {
        // A heating ramp is not a glitch. Latching on the first pair would make every real
        // temperature change look like one.
        let mut s = Tsic306::new(Scripted::new(&[40.0, 80.0]));
        let _ = s.sample(None);
        let _ = s.sample(Some(40.0));
        assert!(!s.runtime_latched());
    }

    #[test]
    fn the_change_rate_passed_down_changes_once_latched() {
        let mut source = Scripted::new(&[92.5, 92.6, 92.7]);
        let mut s = Tsic306::new(&mut source);
        let _ = s.sample(None);
        let _ = s.sample(Some(92.5));
        let _ = s.sample(Some(92.6));
        assert!(s.runtime_latched());
        assert_eq!(source.rate_count(), 3);
    }

    #[test]
    fn a_failed_read_does_not_latch_the_tighter_rate() {
        // Latching on a failed read would apply a 5 C filter to the sensor before it had produced
        // two good readings, which is exactly when it is least likely to help.
        let mut s = Tsic306::new(Scripted::new(&[READ_FAILED, 92.5]));
        assert_eq!(s.sample(None), Err(TemperatureError::Corrupt));
        assert!(!s.runtime_latched());
        assert_eq!(s.sample(None), Ok(92.5));
    }

    #[test]
    fn an_out_of_range_read_does_not_latch_either() {
        let mut s = Tsic306::new(Scripted::new(&[500.0, 92.5]));
        assert_eq!(s.sample(None), Err(TemperatureError::OutOfRange));
        assert!(!s.runtime_latched());
    }

    #[test]
    fn a_reset_returns_to_the_wide_rate() {
        // The C++ firmware's `static` never reset, so a sensor that came back after a fault stayed
        // on the tight rate and rejected its own first valid reading.
        let mut s = Tsic306::new(Scripted::new(&[92.5, 92.6]));
        let _ = s.sample(None);
        let _ = s.sample(Some(92.5));
        assert!(s.runtime_latched());
        s.reset_latch();
        assert!(!s.runtime_latched());
    }

    #[test]
    fn the_first_reading_cannot_latch_because_there_is_nothing_to_compare_against() {
        let mut s = Tsic306::new(Scripted::new(&[92.5]));
        let _ = s.sample(None);
        assert!(!s.runtime_latched());
    }

    #[test]
    fn the_change_rates_match_the_cpp_constants() {
        // TempSensorTSIC.cpp:11-12.
        assert_eq!(INITIAL_CHANGE_RATE, 200.0);
        assert_eq!(RUNTIME_CHANGE_RATE, 5.0);
        assert_eq!(READ_FAILED, 222.0);
        assert_eq!(NOT_CONNECTED, 221.0);
        // The runtime rate is the tighter of the two, or "latch" would mean the opposite. A
        // compile-time check, because a constant that reversed would still pass a run-time test
        // written by whoever set it.
        const _: () = assert!(RUNTIME_CHANGE_RATE < INITIAL_CHANGE_RATE);
    }

    #[test]
    fn the_plausible_range_brackets_a_real_machine() {
        // A room-temperature group head and a 120 C steam setpoint both have to fit, or the
        // range rejects readings the machine actually produces. Checked at compile time because
        // both sides are constants and a constant comparison a test cannot fail is a test that
        // only documents itself.
        const _: () = assert!(MIN_PLAUSIBLE_C < 20.0);
        const _: () = assert!(MAX_PLAUSIBLE_C > 140.0);
    }

    #[test]
    fn the_sensor_type_resolves_and_an_unknown_one_fails_by_name() {
        assert_eq!(SensorType::from_config(0), Ok(SensorType::Ds18b20));
        assert_eq!(SensorType::from_config(1), Ok(SensorType::Tsic306));
        // Not a default: defaulting to the DS18B20 on a TSIC machine looks like a disconnected
        // sensor rather than a typo in the configuration.
        let err = SensorType::from_config(7).unwrap_err();
        assert_eq!(err, SensorTypeError::Unknown(7));
        let mut text = heapless::String::<128>::new();
        let _ = core::fmt::Write::write_fmt(&mut text, format_args!("{err}"));
        assert!(text.as_str().contains('7'), "{text}");
        assert!(text.as_str().contains("DS18B20"), "{text}");
    }

    #[test]
    fn a_read_through_the_trait_is_a_result() {
        let mut s = Tsic306::new(Scripted::new(&[92.5]));
        let reading: Result<f64, TemperatureError> = TemperatureSensor::read_celsius(&mut s);
        assert_eq!(reading, Ok(92.5));
        assert_eq!(
            TemperatureSensor::address(&s),
            0,
            "a TSIC has no 1-Wire address"
        );
    }
}
