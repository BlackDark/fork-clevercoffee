//! Sensor fusion and the sensor fault latch.
//!
//! The C++ firmware's temperature path had a defect that mattered: a disconnected DS18B20 was
//! never detected, because every failure returned the same "not ready" code that the coordinator
//! treated as "still converting". The cached reading stayed at 0, which was inside the valid
//! range, so nothing tripped, and the PID ran the heater flat out (defect D03).
//!
//! Here a reading is a `Result`. A failure is a fault that latches, and a latched fault inhibits
//! the heater. That is the whole fix, and it is only expressible because the read returns an
//! error rather than a sentinel value.

/// Why a temperature reading is not usable.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SensorFault {
    /// The bus is not answering, or no device answered on it.
    Disconnected,
    /// The device answered but the CRC did not match, so the bytes are noise.
    Corrupt,
    /// The value parsed but is outside what the part can physically produce.
    OutOfRange { celsius: f64 },
    /// The conversion was started but never completed within the deadline.
    Timeout,
}

impl SensorFault {
    pub const fn reason(self) -> &'static str {
        match self {
            SensorFault::Disconnected => "disconnected",
            SensorFault::Corrupt => "crc",
            SensorFault::OutOfRange { .. } => "out_of_range",
            SensorFault::Timeout => "timeout",
        }
    }
}

/// A temperature, valid only if the device said so.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub celsius: f64,
}

/// The plausibility window. A DS18B20 reads 0 to 85 C; the C++ firmware accepted 0 to 200,
/// which is wide enough for a disconnected bus reading to look fine if the CRC were ever
/// ignored. Kept at the C++ bounds so a real machine is not rejected for a warm group head.
pub const MIN_VALID_C: f64 = 0.0;
pub const MAX_VALID_C: f64 = 200.0;

/// A rolling mean over the last `N` samples, with a fault latch.
///
/// The C++ firmware used a 15-sample moving average. Kept, because smoothing a temperature that
/// is sampled every 400 ms and then driving a 10 ms PWM from it depends on the lag, and changing
/// it changes how the machine behaves.
#[derive(Clone, Debug)]
pub struct TemperatureFilter {
    window: usize,
    samples: [f64; TemperatureFilter::MAX_WINDOW],
    len: usize,
    next: usize,
    fault: Option<SensorFault>,
}

impl Default for TemperatureFilter {
    fn default() -> Self {
        Self::new(15)
    }
}

impl TemperatureFilter {
    pub const MAX_WINDOW: usize = 32;

    /// Clamped to [`Self::MAX_WINDOW`]; a longer window would not fit the stack on a C6.
    pub fn new(window: usize) -> Self {
        let window = window.clamp(1, Self::MAX_WINDOW);
        Self {
            window,
            samples: [0.0; Self::MAX_WINDOW],
            len: 0,
            next: 0,
            fault: None,
        }
    }

    /// Accepts a good reading.
    pub fn push(&mut self, celsius: f64) {
        // `contains` on a range is false for NaN, so this rejects it, but the explicit check
        // documents why: every later comparison involving NaN is also false, so a NaN that got
        // in would silently poison the whole window.
        if !celsius.is_finite() {
            self.fault = Some(SensorFault::Corrupt);
            return;
        }
        if !(MIN_VALID_C..=MAX_VALID_C).contains(&celsius) {
            self.fault = Some(SensorFault::OutOfRange { celsius });
            return;
        }
        // A good reading clears a latched fault only if it is plausible; a corrupt read does not
        // get to un-latch a disconnect.
        self.fault = None;
        self.samples[self.next] = celsius;
        self.next = (self.next + 1) % self.window;
        if self.len < self.window {
            self.len += 1;
        }
    }

    /// Records a failure. This is what the C++ firmware did not have.
    pub fn fail(&mut self, fault: SensorFault) {
        self.fault = Some(fault);
    }

    /// Clears the fault without a reading. Used when a sensor is deliberately disabled or
    /// swapped, so the machine does not stay inhibited forever.
    pub fn clear_fault(&mut self) {
        self.fault = None;
    }

    pub const fn fault(&self) -> Option<SensorFault> {
        self.fault
    }

    pub const fn is_faulted(&self) -> bool {
        self.fault.is_some()
    }

    /// The filtered temperature, or `None` while the window is not yet full **or a fault is
    /// latched**.
    ///
    /// Returning the last mean while faulted is the exact shape of the C++ defect D03: the
    /// caller reads a plausible number off a sensor that is not answering. Callers get no value
    /// until the fault clears, so a stale reading cannot reach the PID.
    pub fn celsius(&self) -> Option<f64> {
        if self.is_faulted() || self.len == 0 {
            return None;
        }
        let mut sum = 0.0;
        for sample in &self.samples[..self.len] {
            sum += sample;
        }
        Some(sum / self.len as f64)
    }

    /// The most recent good reading, unsmoothed. `None` while faulted, for the same reason as
    /// [`Self::celsius`].
    pub fn latest(&self) -> Option<f64> {
        if self.is_faulted() || self.len == 0 {
            None
        } else {
            Some(self.samples[(self.next + self.window - 1) % self.window])
        }
    }

    pub const fn sample_count(&self) -> usize {
        self.len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disconnected_sensor_faults_rather_than_reading_zero() {
        // The defect this whole type exists for. In the C++ firmware this case left the reading
        // at 0.0, which is inside the valid range, so nothing detected it.
        let mut f = TemperatureFilter::default();
        f.fail(SensorFault::Disconnected);
        assert!(f.is_faulted());
        assert_eq!(f.celsius(), None);
        assert_eq!(f.fault(), Some(SensorFault::Disconnected));
    }

    #[test]
    fn a_crc_failure_is_distinct_from_a_disconnect() {
        let mut f = TemperatureFilter::default();
        f.fail(SensorFault::Corrupt);
        assert_eq!(f.fault().unwrap().reason(), "crc");
    }

    #[test]
    fn an_implausible_value_faults_and_is_not_stored() {
        let mut f = TemperatureFilter::default();
        f.push(95.0);
        f.push(250.0);
        assert_eq!(f.fault(), Some(SensorFault::OutOfRange { celsius: 250.0 }));
        assert_eq!(
            f.sample_count(),
            1,
            "the bad sample must not enter the window"
        );
    }

    #[test]
    fn a_fault_clears_only_on_a_plausible_reading() {
        let mut f = TemperatureFilter::default();
        f.fail(SensorFault::Disconnected);
        assert!(f.is_faulted());
        f.push(90.0);
        assert!(!f.is_faulted());
    }

    #[test]
    fn a_failed_read_does_not_wipe_the_window_but_hides_it() {
        // The window is kept, so a recovered sensor resumes filtering rather than restarting.
        // The value is withheld, so nothing can read a stale number off a dead sensor.
        let mut f = TemperatureFilter::new(5);
        for _ in 0..5 {
            f.push(90.0);
        }
        f.fail(SensorFault::Timeout);
        assert_eq!(f.celsius(), None, "a faulted sensor must yield no value");
        assert_eq!(f.latest(), None);
        assert!(f.is_faulted());
        f.clear_fault();
        assert_eq!(f.celsius(), Some(90.0), "the window survives the fault");
    }

    #[test]
    fn the_window_is_a_rolling_mean() {
        let mut f = TemperatureFilter::new(3);
        f.push(90.0);
        assert_eq!(f.celsius(), Some(90.0));
        f.push(91.0);
        f.push(92.0);
        assert_eq!(f.celsius(), Some(91.0));
        // The oldest sample rolls off, so the mean rises rather than staying at 91.
        f.push(93.0);
        assert_eq!(f.celsius(), Some(92.0));
    }

    #[test]
    fn an_empty_filter_reads_nothing_rather_than_zero() {
        let f = TemperatureFilter::default();
        assert_eq!(f.celsius(), None);
        assert_eq!(f.latest(), None);
    }

    #[test]
    fn the_window_size_is_clamped_to_something_that_fits_the_stack() {
        assert_eq!(TemperatureFilter::new(0).sample_count(), 0);
        let mut big = TemperatureFilter::new(1000);
        assert_eq!(big.sample_count(), 0);
        big.push(50.0);
        assert_eq!(big.celsius(), Some(50.0));
    }

    #[test]
    fn a_recovered_sensor_keeps_filtering() {
        let mut f = TemperatureFilter::new(3);
        f.push(80.0);
        f.push(81.0);
        f.fail(SensorFault::Disconnected);
        f.clear_fault();
        f.push(82.0);
        assert_eq!(f.celsius(), Some(81.0));
        assert!(!f.is_faulted());
    }

    #[test]
    fn a_non_finite_reading_is_rejected() {
        // NaN fails every comparison, so a plain range check lets it through and it then
        // poisons the mean for the whole window.
        let mut f = TemperatureFilter::new(3);
        f.push(90.0);
        f.push(f64::NAN);
        assert!(f.is_faulted(), "NaN must not enter the window");
        assert_eq!(f.celsius(), None);
    }
}
