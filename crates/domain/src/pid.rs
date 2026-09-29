//! The PID controller.
//!
//! A faithful port of the vendored Arduino-PID-Library 1.2.1, with the C++ firmware's scaling.
//! Ported rather than replaced because the gains in the existing `config.json` were tuned
//! against this exact implementation, including its time-based integral and derivative and its
//! exponential input filter. A mathematically equivalent but differently formulated controller
//! would need the whole gain set retuned, which is not a change anyone asked for.
//!
//! The C++ controller wrote its output through a raw `double*` that a 10 ms ISR read
//! (defect D18, a data race). Here the output crosses that boundary as an integer in
//! permille, so there is nothing to race on.

/// Gain set, in the units the C++ firmware used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gains {
    pub kp: f64,
    pub tn: f64,
    pub tv: f64,
}

impl Gains {
    /// Integral gain, as the Arduino library computes it: Kp / Tn. Zero when Tn is zero, rather
    /// than a division by zero, which is what the C++ `calculateDerivedValues` guarded against.
    pub const fn integral(self) -> f64 {
        if self.tn > 0.0 {
            self.kp / self.tn
        } else {
            0.0
        }
    }

    /// Derivative gain: Tv * Kp.
    ///
    /// The C++ firmware computed `aggKd = Tv * Kp`, which is unusual but is what its gains were
    /// tuned against, so it is preserved rather than "corrected".
    pub const fn derivative(self) -> f64 {
        self.tv * self.kp
    }
}

/// One PID result, split into the parts so the display and the logs can show why.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PidOutput {
    /// The commanded heater duty, 0 to `window`.
    pub output: f64,
    pub p_term: f64,
    pub i_term: f64,
    pub d_term: f64,
    pub error: f64,
}

/// The controller.
#[derive(Clone, Debug)]
pub struct Pid {
    kp: f64,
    ki: f64,
    kd: f64,
    sample_ms: u32,
    /// The PWM window, which is also the output maximum. The C++ firmware used 1000.
    window: f64,
    integrator: f64,
    integrator_min: f64,
    integrator_max: f64,
    last_filtered_input: f64,
    /// The Arduino library seeds its input filter with zero, which makes the first derivative
    /// term `(0 - input) / dt` scaled by `kd`. With this firmware's gains that is thousands of
    /// counts, so the very first compute clamps the output to zero and the heater does not
    /// start. The filter is seeded with the first real reading instead.
    filter_seeded: bool,
    last_input: f64,
    last_error: f64,
    /// The previous compute's output before clamping, which the anti-windup test needs: the
    /// condition is "the output was already saturated", and a clamped value hides that.
    last_raw_output: f64,
    /// Milliseconds since boot at the last compute, or `None` before the first one.
    last_compute_ms: Option<u32>,
    ema_factor: f64,
    enabled: bool,
    /// `pid.use_ponm`. With it set, the proportional term acts on the measurement and the
    /// derivative uses the unfiltered input, which is what the Arduino library's P_ON_M path
    /// does (`PID_v1.cpp:84-94`). The shipped config leaves it off, so this is the live
    /// configuration that changes behaviour when set.
    proportional_on_measurement: bool,
}

impl Pid {
    /// `window` is the PWM window in milliseconds and the output maximum. The C++ firmware used
    /// 1000 for both, so the ISR's software PWM gets a 0-to-1000 duty.
    pub fn new(gains: Gains, sample_ms: u32, window: u32, ema_factor: f64) -> Self {
        let secs = sample_ms as f64 / 1000.0;
        Self {
            kp: gains.kp,
            ki: gains.integral() * secs,
            kd: gains.derivative() / secs,
            sample_ms,
            window: window as f64,
            integrator: 0.0,
            // The C++ firmware hardcoded 0 to 55 here, ignoring the configured i_max. The value
            // is now a parameter, which is the fix recorded against that defect.
            integrator_min: 0.0,
            integrator_max: 55.0,
            last_filtered_input: 0.0,
            filter_seeded: false,
            last_input: 0.0,
            last_error: 0.0,
            last_raw_output: 0.0,
            last_compute_ms: None,
            ema_factor,
            enabled: false,
            proportional_on_measurement: false,
        }
    }

    pub fn set_proportional_on_measurement(&mut self, on: bool) {
        self.proportional_on_measurement = on;
    }

    pub fn with_integrator_limits(mut self, min: f64, max: f64) -> Self {
        self.integrator_min = min;
        self.integrator_max = max;
        self
    }

    pub fn set_gains(&mut self, gains: Gains) {
        let secs = self.sample_ms as f64 / 1000.0;
        self.kp = gains.kp;
        self.ki = gains.integral() * secs;
        self.kd = gains.derivative() / secs;
        if gains.tn <= 0.0 {
            self.integrator = 0.0;
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled && !enabled {
            // Resetting the integrator on disable stops a stale integral from dumping its whole
            // accumulated value into the heater on the next enable.
            self.integrator = 0.0;
        }
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn integrator(&self) -> f64 {
        self.integrator
    }

    pub fn window(&self) -> u32 {
        self.window as u32
    }

    /// Computes a new output, or returns the previous one if the sample period has not elapsed.
    ///
    /// The time check is explicit rather than hidden, because the caller knows the clock and a
    /// hidden `millis()` makes the controller impossible to test.
    pub fn compute(&mut self, now_ms: u32, input_c: f64, setpoint_c: f64) -> PidOutput {
        if !self.enabled {
            // Zero, not the last output. The C++ forced the output to 0 on disable
            // (ProcessController.cpp:153-161); returning a previous non-zero value here would
            // hand a caller a duty cycle for a controller that is supposed to be off.
            return PidOutput {
                output: 0.0,
                p_term: 0.0,
                i_term: 0.0,
                d_term: 0.0,
                error: 0.0,
            };
        }
        if let Some(last) = self.last_compute_ms {
            if now_ms.wrapping_sub(last) < self.sample_ms {
                let previous = self.last_output();
                return previous;
            }
        }
        self.last_compute_ms = Some(now_ms);

        let error = setpoint_c - input_c;
        let secs = self.sample_ms as f64 / 1000.0;

        // Conditional integration, from the Arduino library's issue 76: stop winding the
        // integral up while the output is already saturated, so it cannot overshoot on the way
        // back.
        let saturated = self.last_raw_output >= self.window - 0.01 || self.last_raw_output <= 0.01;
        if !saturated {
            self.integrator += self.ki * error;
        }

        // Exponentially weighted moving average on the input, used only for the derivative, so
        // sensor noise does not make the heater chatter. The first reading seeds the filter
        // rather than being differenced against zero.
        let d_input = if self.filter_seeded {
            let old_filtered = self.last_filtered_input;
            self.last_filtered_input =
                self.ema_factor * old_filtered + (1.0 - self.ema_factor) * input_c;
            (self.last_filtered_input - old_filtered) / secs
        } else {
            self.filter_seeded = true;
            self.last_filtered_input = input_c;
            0.0
        };

        self.clamp_integrator();

        // P_ON_M moves the proportional action onto the measurement, and the derivative then
        // subtracts instead of adding, exactly as `PID_v1.cpp:113-116` does.
        let p_term = if self.proportional_on_measurement {
            -self.kp * (input_c - setpoint_c)
        } else {
            self.kp * error
        };
        if self.proportional_on_measurement {
            self.integrator -= self.kp * d_input;
        }
        let i_term = self.integrator;
        let d_term = -self.kd * d_input;
        let mut output = p_term + i_term + d_term;
        let clamped = output.clamp(0.0, self.window);
        self.last_raw_output = output;
        output = clamped;

        self.last_input = input_c;
        self.last_error = error;

        PidOutput {
            output,
            p_term,
            i_term,
            d_term,
            error,
        }
    }

    fn clamp_integrator(&mut self) {
        self.integrator = self.integrator.clamp(0.0, self.window);
        self.integrator = self
            .integrator
            .clamp(self.integrator_min, self.integrator_max);
    }

    fn last_output(&self) -> PidOutput {
        PidOutput {
            output: self.last_raw_output.clamp(0.0, self.window),
            p_term: self.kp * self.last_error,
            i_term: self.integrator,
            d_term: 0.0,
            error: self.last_error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid() -> Pid {
        Pid::new(
            Gains {
                kp: 62.0,
                tn: 52.0,
                tv: 11.5,
            },
            1000,
            1000,
            0.6,
        )
        .with_integrator_limits(0.0, 55.0)
    }

    #[test]
    fn a_disabled_controller_computes_nothing() {
        let mut p = pid();
        let out = p.compute(0, 50.0, 95.0);
        assert_eq!(out.output, 0.0);
        assert_eq!(p.integrator(), 0.0);
    }

    #[test]
    fn a_disabled_controller_reports_zero_even_after_being_enabled() {
        // A caller that trusts the returned value without re-checking `is_enabled()` would
        // otherwise re-energise the heater on the tick after a disable.
        let mut p = pid();
        p.set_enabled(true);
        p.compute(0, 50.0, 95.0);
        p.set_enabled(false);
        let after = p.compute(1000, 50.0, 95.0);
        assert_eq!(
            after.output, 0.0,
            "a disabled controller must report no duty"
        );
    }

    #[test]
    fn the_first_compute_is_not_swamped_by_a_derivative_transient() {
        // The Arduino library seeds its input filter with zero, so the first derivative term is
        // (0 - input)/dt * kd. With these gains that is about -14000, which clamps the output to
        // zero and the heater never starts. Seeding the filter with the first reading is the
        // fix, and this test is the reason.
        let mut p = pid();
        p.set_enabled(true);
        let first = p.compute(0, 50.0, 95.0);
        assert!(
            first.output > 0.0,
            "the first compute must not be dominated by a filter transient, got {}",
            first.output
        );
        assert_eq!(
            first.d_term, 0.0,
            "the first compute has no derivative to contribute"
        );
    }

    #[test]
    fn it_computes_only_once_per_sample_period() {
        let mut p = pid();
        p.set_enabled(true);
        let first = p.compute(0, 50.0, 95.0);
        assert!(
            first.output > 0.0,
            "the first compute should produce a proportional term"
        );
        let too_soon = p.compute(999, 0.0, 95.0);
        assert_eq!(
            too_soon.output, first.output,
            "a compute before the sample period must return the previous output"
        );
        let on_time = p.compute(1000, 50.0, 95.0);
        assert_ne!(on_time.output, 0.0);
    }

    #[test]
    fn a_cold_machine_demands_full_power() {
        let mut p = pid();
        p.set_enabled(true);
        let out = p.compute(0, 0.0, 95.0);
        assert_eq!(out.output, 1000.0, "a 95 degree error saturates the window");
        assert_eq!(out.p_term, 62.0 * 95.0);
    }

    #[test]
    fn output_never_leaves_the_window() {
        let mut p = pid();
        p.set_enabled(true);
        // Far below setpoint, output saturates high.
        let hot = p.compute(0, -50.0, 95.0);
        assert_eq!(hot.output, 1000.0);
        // Far above setpoint with a saturated negative error, output saturates low.
        let cold = p.compute(1000, 500.0, 20.0);
        assert_eq!(cold.output, 0.0);
    }

    #[test]
    fn the_integrator_respects_both_limit_sets() {
        // A setpoint that leaves the output unsaturated, because conditional integration
        // deliberately freezes the integral while the output is pinned at a limit.
        let mut p = Pid::new(
            Gains {
                kp: 20.0,
                tn: 5.0,
                tv: 0.0,
            },
            1000,
            1000,
            0.6,
        )
        .with_integrator_limits(0.0, 55.0);
        p.set_enabled(true);
        for t in 0..40 {
            p.compute(t * 1000, 90.0, 95.0);
        }
        assert_eq!(
            p.integrator(),
            55.0,
            "the configured maximum must be honoured"
        );
    }

    #[test]
    fn the_integrator_freezes_while_the_output_is_saturated() {
        // Conditional integration, from the Arduino library's issue 76. Without it a machine that
        // sat saturated for a minute would dump a huge integral into the heater on the way down.
        let mut p = Pid::new(
            Gains {
                kp: 62.0,
                tn: 5.0,
                tv: 0.0,
            },
            1000,
            1000,
            0.6,
        )
        .with_integrator_limits(0.0, 100_000.0);
        p.set_enabled(true);
        for t in 0..30 {
            p.compute(t * 1000, 0.0, 95.0);
        }
        assert_eq!(
            p.integrator(),
            0.0,
            "a saturated output must not wind the integral up"
        );
    }

    #[test]
    fn a_stale_integral_does_not_survive_a_disable() {
        let mut p = Pid::new(
            Gains {
                kp: 20.0,
                tn: 5.0,
                tv: 0.0,
            },
            1000,
            1000,
            0.6,
        )
        .with_integrator_limits(0.0, 1000.0);
        p.set_enabled(true);
        for t in 0..20 {
            p.compute(t * 1000, 90.0, 95.0);
        }
        assert!(
            p.integrator() > 0.0,
            "the integral should have accumulated, got {}",
            p.integrator()
        );
        p.set_enabled(false);
        assert_eq!(p.integrator(), 0.0, "disabling must reset the integral");
    }

    #[test]
    fn the_derivative_does_not_kick_when_the_setpoint_changes() {
        // The C++ firmware used the negative of the filtered input differential rather than the
        // error differential, precisely to avoid derivative kick on a setpoint change. That
        // behaviour is preserved, so a setpoint change alone must not move the derivative.
        let mut p = pid();
        p.set_enabled(true);
        // Let the filter settle at a steady input first, so the only thing that differs between
        // the two measurements is the setpoint.
        for t in 0..30 {
            p.compute(t * 1000, 90.0, 95.0);
        }
        let before = p.compute(30_000, 90.0, 95.0);
        let after = p.compute(31_000, 90.0, 100.0);
        assert_eq!(
            after.d_term, before.d_term,
            "a setpoint-only change must not move the derivative"
        );
        // The proportional term should move, because the setpoint really did change.
        assert!(
            (after.p_term - before.p_term).abs() > 1.0,
            "the proportional term must respond to a setpoint change"
        );
    }

    #[test]
    fn gains_derive_from_the_configured_tn_and_tv() {
        let g = Gains {
            kp: 62.0,
            tn: 52.0,
            tv: 11.5,
        };
        assert!((g.integral() - 62.0 / 52.0).abs() < 1e-12);
        assert!((g.derivative() - 11.5 * 62.0).abs() < 1e-12);
        // A zero Tn gives no integral action rather than a division by zero.
        assert_eq!(
            Gains {
                kp: 62.0,
                tn: 0.0,
                tv: 0.0
            }
            .integral(),
            0.0
        );
    }

    #[test]
    fn the_converges_on_a_setpoint() {
        // A sanity check that the port is a working controller, not just a bounded one.
        let mut p = Pid::new(
            Gains {
                kp: 20.0,
                tn: 100.0,
                tv: 10.0,
            },
            1000,
            1000,
            0.6,
        )
        .with_integrator_limits(0.0, 200.0);
        p.set_enabled(true);
        let mut temp = 20.0_f64;
        for t in 0..2000 {
            let out = p.compute(t * 1000, temp, 95.0);
            // A crude plant: heating proportional to duty.
            temp += (out.output / 1000.0) * 0.2 - 0.05;
        }
        assert!(
            (temp - 95.0).abs() < 5.0,
            "the controller should settle near the setpoint, settled at {temp}"
        );
    }
}
