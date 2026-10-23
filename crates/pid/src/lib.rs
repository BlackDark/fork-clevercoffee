//! PID controller with the semantics of the Arduino `PID_v1` fork CleverCoffee used.
//!
//! Kept bit-compatible on purpose: the tunings users already have (Kp, Tn, Tv, I-max)
//! must behave the same on the new firmware. Differences from the C++:
//! - time is passed in (`now_ms`), never read from a global clock;
//! - the derivative divides by the real sample time in seconds (the C++ used integer
//!   division, identical for the 1000 ms sample time it actually ran with).
//!
//! Two proportional modes:
//! - [`Proportional::OnError`]: classic P on error, filtered derivative on input,
//!   conditional integration (anti-windup) and an extra integrator clamp.
//! - [`Proportional::OnMeasurement`]: P acts on the input change and is folded into
//!   the integrator ("PonM"), which avoids overshoot after setpoint changes.

#![no_std]

/// Where the proportional term acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proportional {
    OnError,
    OnMeasurement,
}

/// User-facing gains (Kp in output units per °C, Ki per second, Kd in seconds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tunings {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    pub proportional: Proportional,
}

impl Tunings {
    /// Gains from the parameters users configure: Kp, Tn (integral time, s) and Tv
    /// (derivative time, s). `Tn == 0` disables the integral part.
    pub fn from_kp_tn_tv(kp: f64, tn: f64, tv: f64, proportional: Proportional) -> Self {
        let ki = if tn == 0.0 { 0.0 } else { kp / tn };
        Self { kp, ki, kd: tv * kp, proportional }
    }
}

/// The controller. Output and integrator are clamped to `[out_min, out_max]`.
#[derive(Debug, Clone)]
pub struct Pid {
    tunings: Tunings,
    // Gains scaled by the sample time.
    kp: f64,
    ki: f64,
    kd: f64,
    sample_time_ms: u32,
    out_min: f64,
    out_max: f64,
    i_min: f64,
    i_max: f64,
    filter_alpha: f64,
    automatic: bool,
    integrator: f64,
    last_input: f64,
    last_filtered_input: f64,
    output: f64,
    last_time_ms: Option<u64>,
    last: Parts,
}

/// The contributions of the last computation, for display and diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Parts {
    pub p: f64,
    pub i: f64,
    pub d: f64,
    pub error: f64,
}

impl Pid {
    /// A controller in manual mode with output 0.
    pub fn new(tunings: Tunings, sample_time_ms: u32, out_min: f64, out_max: f64) -> Self {
        assert!(sample_time_ms > 0, "sample time must be positive");
        assert!(out_min < out_max, "empty output range");
        let mut pid = Self {
            tunings,
            kp: 0.0,
            ki: 0.0,
            kd: 0.0,
            sample_time_ms,
            out_min,
            out_max,
            i_min: -100.0,
            i_max: 100.0,
            filter_alpha: 0.0,
            automatic: false,
            integrator: 0.0,
            last_input: 0.0,
            last_filtered_input: 0.0,
            output: 0.0,
            last_time_ms: None,
            last: Parts::default(),
        };
        pid.set_tunings(tunings);
        pid
    }

    /// Changes gains. Negative gains are ignored, as in the C++. `ki == 0` clears the
    /// integrator.
    pub fn set_tunings(&mut self, tunings: Tunings) {
        if tunings.kp < 0.0 || tunings.ki < 0.0 || tunings.kd < 0.0 {
            return;
        }
        let secs = self.sample_secs();
        self.tunings = tunings;
        self.kp = tunings.kp;
        self.ki = tunings.ki * secs;
        self.kd = tunings.kd / secs;
        if tunings.ki == 0.0 {
            self.integrator = 0.0;
        }
    }

    pub fn tunings(&self) -> Tunings {
        self.tunings
    }

    /// Limits the integrator in P-on-error mode (in P-on-measurement mode the
    /// integrator carries P and is only limited by the output range).
    pub fn set_integrator_limits(&mut self, min: f64, max: f64) {
        if min >= max {
            return;
        }
        self.i_min = min;
        self.i_max = max;
        if self.automatic {
            self.integrator = self.integrator.clamp(min, max);
        }
    }

    /// Smoothing of the input for the derivative: `filtered = a * filtered + (1 - a) * input`.
    pub fn set_smoothing(&mut self, alpha: f64) {
        self.filter_alpha = alpha.clamp(0.0, 1.0);
    }

    /// Switches between automatic and manual. Going to automatic seeds the integrator
    /// with the current output for a bumpless transfer.
    pub fn set_automatic(&mut self, automatic: bool, input: f64) {
        if automatic && !self.automatic {
            self.integrator = self.output.clamp(self.out_min, self.out_max);
            self.last_input = input;
            self.last_filtered_input = input;
        }
        self.automatic = automatic;
    }

    pub fn is_automatic(&self) -> bool {
        self.automatic
    }

    /// Sets the output directly (meaningful in manual mode, e.g. forcing 0).
    pub fn set_output(&mut self, output: f64) {
        self.output = output.clamp(self.out_min, self.out_max);
    }

    pub fn output(&self) -> f64 {
        self.output
    }

    pub fn last_parts(&self) -> Parts {
        self.last
    }

    /// Runs one step if automatic and a full sample time has passed (the first call
    /// always computes). Returns the new output when it computed.
    pub fn compute(&mut self, now_ms: u64, input: f64, setpoint: f64) -> Option<f64> {
        if !self.automatic {
            return None;
        }
        if let Some(last) = self.last_time_ms
            && now_ms.saturating_sub(last) < u64::from(self.sample_time_ms)
        {
            return None;
        }
        let on_error = self.tunings.proportional == Proportional::OnError;
        let error = setpoint - input;

        if !on_error || (self.output < self.out_max - 0.01 && self.output > self.out_min + 0.01) {
            self.integrator += self.ki * error;
        }

        let old_filtered = self.last_filtered_input;
        self.last_filtered_input =
            self.filter_alpha * self.last_filtered_input + (1.0 - self.filter_alpha) * input;
        let d_input = if on_error {
            (self.last_filtered_input - old_filtered) / self.sample_secs()
        } else {
            input - self.last_input
        };

        if !on_error {
            self.integrator -= self.kp * d_input;
        }
        self.integrator = self.integrator.clamp(self.out_min, self.out_max);
        if on_error {
            self.integrator = self.integrator.clamp(self.i_min, self.i_max);
        }

        let p = if on_error { self.kp * error } else { 0.0 };
        let d = -self.kd * d_input;
        self.output = (p + self.integrator + d).clamp(self.out_min, self.out_max);

        self.last_input = input;
        self.last_time_ms = Some(now_ms);
        self.last = Parts { p, i: self.integrator, d, error };
        Some(self.output)
    }

    fn sample_secs(&self) -> f64 {
        f64::from(self.sample_time_ms) / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regular() -> Pid {
        // The firmware's configuration: 1 s sample, output in promille of the heater window.
        let mut pid = Pid::new(
            Tunings::from_kp_tn_tv(62.0, 52.0, 11.5, Proportional::OnError),
            1000,
            0.0,
            1000.0,
        );
        pid.set_integrator_limits(0.0, 55.0);
        pid.set_smoothing(0.6);
        pid
    }

    #[test]
    fn manual_mode_never_computes() {
        let mut pid = regular();
        assert_eq!(pid.compute(0, 20.0, 95.0), None);
        assert_eq!(pid.output(), 0.0);
    }

    #[test]
    fn computes_once_per_sample_time() {
        let mut pid = regular();
        pid.set_automatic(true, 20.0);
        assert!(pid.compute(5_000, 20.0, 95.0).is_some());
        assert!(pid.compute(5_999, 20.0, 95.0).is_none());
        assert!(pid.compute(6_000, 20.0, 95.0).is_some());
    }

    #[test]
    fn cold_boiler_saturates_at_full_power() {
        let mut pid = regular();
        pid.set_automatic(true, 20.0);
        assert_eq!(pid.compute(0, 20.0, 95.0), Some(1000.0));
    }

    #[test]
    fn integrator_is_clamped_to_i_max_in_on_error_mode() {
        let mut pid = regular();
        pid.set_automatic(true, 94.0);
        for s in 0..200 {
            pid.compute(s * 1000, 94.9, 95.0);
        }
        assert!(pid.last_parts().i <= 55.0);
    }

    #[test]
    fn hot_boiler_turns_heater_off() {
        let mut pid = regular();
        pid.set_automatic(true, 100.0);
        assert_eq!(pid.compute(0, 100.0, 95.0), Some(0.0));
    }

    #[test]
    fn zero_tn_disables_integral() {
        let t = Tunings::from_kp_tn_tv(50.0, 0.0, 20.0, Proportional::OnError);
        assert_eq!(t.ki, 0.0);
        assert_eq!(t.kd, 1000.0);
    }

    #[test]
    fn negative_gains_are_ignored() {
        let mut pid = regular();
        let before = pid.tunings();
        pid.set_tunings(Tunings { kp: -1.0, ..before });
        assert_eq!(pid.tunings(), before);
    }

    #[test]
    fn going_automatic_is_bumpless() {
        let mut pid = regular();
        pid.set_output(400.0);
        pid.set_automatic(true, 94.0);
        // Integrator seeded from the output, then clamped by the next compute.
        assert_eq!(pid.integrator, 400.0);
    }

    #[test]
    fn p_on_measurement_reacts_to_input_change_not_error() {
        let mut pid = Pid::new(
            Tunings::from_kp_tn_tv(62.0, 0.0, 0.0, Proportional::OnMeasurement),
            1000,
            0.0,
            1000.0,
        );
        pid.set_output(300.0);
        pid.set_automatic(true, 90.0);
        // Same input as last time: no P change, Ki == 0, so the output holds.
        assert_eq!(pid.compute(0, 90.0, 95.0), Some(300.0));
        // Input rose 1 °C: output drops by Kp.
        assert_eq!(pid.compute(1000, 91.0, 95.0), Some(238.0));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn output_stays_in_range(
            kp in 0.0f64..500.0, tn in 0.0f64..200.0, tv in 0.0f64..200.0,
            ponm in any::<bool>(),
            inputs in prop::collection::vec(-20.0f64..200.0, 1..60),
            setpoint in 0.0f64..140.0,
        ) {
            let p = if ponm { Proportional::OnMeasurement } else { Proportional::OnError };
            let mut pid = Pid::new(Tunings::from_kp_tn_tv(kp, tn, tv, p), 1000, 0.0, 1000.0);
            pid.set_integrator_limits(0.0, 55.0);
            pid.set_smoothing(0.6);
            pid.set_automatic(true, inputs[0]);
            for (i, input) in inputs.iter().enumerate() {
                if let Some(out) = pid.compute(i as u64 * 1000, *input, setpoint) {
                    prop_assert!((0.0..=1000.0).contains(&out));
                }
            }
        }
    }
}
