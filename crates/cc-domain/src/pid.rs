//! A port of the vendored Arduino `PID_v1` library.
//!
//! Source of truth: `lib/Arduino-PID-Library/PID_v1.cpp` and `PID_v1.h`, which
//! the firmware constructs in `SystemInitializer::initializeContext`
//! (`src/core/SystemInitializer.cpp:296-305`) and configures in
//! `initializePID` (`:536-566`).
//!
//! # Parity
//!
//! Every arithmetic step below is the corresponding line of `PID_v1.cpp`, in the
//! same order, in `f64` — because the C++ uses `double`. Nothing is
//! "improved": the algorithm is reproduced, including its defects, because a
//! controller that behaves differently is a controller that boils differently.
//!
//! Parity is measured, not asserted by inspection. A fixed four-scenario input
//! sequence is replayed against the **real** C++ library by
//! `crates/cc-domain/tools/pid_oracle/run.sh`, and the expected outputs in that
//! module's test tables (`src/pid_parity.rs`, compiled only under `cfg(test)`)
//! are that program's output.
//!
//! # Deliberate differences from the C++
//!
//! | # | C++ | Here | Why |
//! | --- | --- | --- | --- |
//! | D1 | `PID(double* Input, double* Output, double* Setpoint, …)` — three raw pointers to caller-owned variables | [`Controller`] owns the three values as fields | The C++ aliases the caller's `double`s and reads `*myOutput` for its anti-windup test (`PID_v1.cpp:70`). Keeping the values inside the controller is the same aliasing with no possibility of a dangling pointer or an accidental write from elsewhere. The shell assigns `input`/`setpoint`/`output` fields directly. |
//! | D2 | the constructor calls `millis()` | [`Controller::new`] takes `now: Millis` | A pure function of its arguments. The C++ value is `millis() - SampleTime` (`PID_v1.cpp:42`) and it is reproduced exactly. |
//! | D3 | seven members are left uninitialised (`PID_v1.h:108-113`) | all initialised to `0.0` | Unobservable in the firmware's usage — `SetMode(AUTOMATIC)` calls `Initialize()` before the first `Compute()` — but a Rust struct cannot be partially uninitialised, and zero is the safe value. See also D-PID-2 below. |
//! | D4 | `if (Kp < 0 \|\| Ki < 0 \|\| Kd < 0) return;` silently discards the call | [`Controller::set_tunings`] returns `bool` | A rejected tuning is a configuration error, and the caller deserves to know. The firmware's own callers ignore it (`ProcessController.cpp:214`), which is why this is safe. |
//!
//! # Reproduced upstream defects
//!
//! These are **bugs in the vendored library**, reproduced on purpose so that
//! parity is exact. They are not endorsed, and R2-04's report flags them for a
//! human decision rather than fixing them silently.
//!
//! * **D-PID-1 — integer division in the `P_ON_E` derivative term.**
//!   `PID_v1.cpp:85` reads
//!   `dInput = (lastFilteredInput - oldFiltered) / (SampleTime / 1000);`
//!   `SampleTime` is an `unsigned long` and `/ 1000` is **integer** division.
//!   Any sample time below 1000 ms therefore yields a zero divisor, and the
//!   whole output becomes `NaN` (or `±inf`). The shipped firmware never trips
//!   this because `initializePID` sets the sample time to
//!   `processWindowSize()` = 1000 ms exactly — but nothing enforces that, and
//!   R1-07 (heater output method) is precisely the change that would alter the
//!   window. `Controller::derivative_seconds()` exposes the value so the next
//!   reader can see the trap rather than rediscover it as a NaN in the field.
//!
//! * **D-PID-2 — the constructor does not initialise the controller state.**
//!   `integrator`, `lastInput`, `lastFilteredInput`, `lastFilteredDifferential`,
//!   `lastError`, `lastPPart` and `lastDPart` have no initialiser in
//!   `PID_v1.h:108-113`. Correctness depends entirely on `SetMode(AUTOMATIC)`
//!   calling `Initialize()` (`PID_v1.cpp:238-244`) before the first
//!   `Compute()`. Construct the controller, leave it in `Manual`, and call
//!   `compute()` forever and nothing happens — harmless — but a future edit that
//!   makes `compute()` run in manual mode would read uninitialised memory.
//!
//! * **D-PID-3 — the anti-windup integrator gate uses a strict `<`/`>` pair
//!   with a dead band.** `PID_v1.cpp:70` only accumulates when the previous
//!   output is strictly inside `(outMin + 0.01, outMax - 0.01)`. With the
//!   production limits `(0, 1000)`, an output of exactly `0.0` does **not**
//!   accumulate, so the very first compute after start-up has `integrator == 0`
//!   no matter how large the error. That is the shipped behaviour and the
//!   parity vector pins it.

use crate::units::Millis;

/// Whether the proportional term acts on the error or on the measurement.
///
/// `P_ON_M` exists to avoid derivative kick when the setpoint steps. The
/// firmware selects it for the brew-detection tuning
/// (`src/control/ProcessController.cpp:214`) and the P term becomes zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProportionalOn {
    /// Proportional on error. `PID_v1.h` `P_ON_E` = 1.
    Error,
    /// Proportional on measurement. `PID_v1.h` `P_ON_M` = 0.
    Measurement,
}

/// Whether increasing the output raises or lowers the input.
///
/// `DIRECT` = 0, `REVERSE` = 1 in `PID_v1.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControllerDirection {
    /// Output up, input up. The only direction the firmware uses.
    Direct,
    /// Output up, input down.
    Reverse,
}

/// Whether the controller is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Hold the output where the shell put it; do not compute.
    ///
    /// The C++ `MANUAL` = 0.
    Manual,
    /// Compute on every sample period.
    ///
    /// The C++ `AUTOMATIC` = 1. Entering this mode from `Manual` performs the
    /// bumpless transfer (`Initialize()`), which is why
    /// [`Controller::set_mode`] has no extra arguments: the input and output
    /// live in the controller.
    Automatic,
}

/// A port of the Arduino `PID_v1` controller.
///
/// See the module documentation for the C++ line-by-line mapping and for the
/// list of upstream defects that are reproduced deliberately.
#[derive(Clone, Debug, PartialEq)]
pub struct Controller {
    // ---- externally-owned in the C++; fields here (difference D1) ----------
    /// The process variable. The firmware writes the temperature here.
    pub input: f64,
    /// The controller output, bounded by `out_min ..= out_max`. In this machine
    /// the output is a millisecond duty within a 1000 ms chopper window.
    pub output: f64,
    /// The desired value. The firmware writes the brew or steam setpoint here.
    pub setpoint: f64,

    // ---- tunings, in the form actually used for arithmetic ----------------
    /// The proportional gain as the operator entered it, before the
    /// sample-time and direction adjustments. This is what `GetKp()` returns.
    kp: f64,
    /// The integral gain as entered. See [`Self::kp`].
    ki: f64,
    /// The derivative gain as entered. See [`Self::kp`].
    kd: f64,
    /// The proportional gain actually used for arithmetic: `ki` is multiplied
    /// and `kd` divided by the sample period, and all three are negated when the
    /// controller is `Reverse`.
    kp_eff: f64,
    /// See [`Self::kp_eff`].
    ki_eff: f64,
    /// See [`Self::kp_eff`].
    kd_eff: f64,

    proportional_on: ProportionalOn,
    p_on_e: bool,
    direction: ControllerDirection,

    sample_time: Millis,
    last_time: Millis,

    integrator: f64,
    out_min: f64,
    out_max: f64,
    integrator_min: f64,
    integrator_max: f64,

    filter_alpha: f64,
    last_input: f64,
    last_filtered_input: f64,

    /// Only read by the diagnostics getters; kept for parity of the struct.
    last_filtered_differential: f64,
    /// Only read by the diagnostics getters.
    last_error: f64,
    /// Only read by the diagnostics getters.
    last_p_part: f64,
    /// Only read by the diagnostics getters.
    last_d_part: f64,

    in_auto: bool,
}

impl Controller {
    /// The library's default output limits (Arduino PWM range).
    pub const DEFAULT_OUT_MIN: f64 = 0.0;
    /// See [`Self::DEFAULT_OUT_MIN`].
    pub const DEFAULT_OUT_MAX: f64 = 255.0;
    /// The library's default integrator limits (`PID_v1.cpp:35`).
    pub const DEFAULT_INTEGRATOR_MIN: f64 = -100.0;
    /// See [`Self::DEFAULT_INTEGRATOR_MIN`].
    pub const DEFAULT_INTEGRATOR_MAX: f64 = 100.0;
    /// The library's default sample period in milliseconds (`PID_v1.cpp:37`).
    ///
    /// NOTE the firmware immediately overrides this with
    /// `processWindowSize()` = 1000 ms. The default is only observable in the
    /// window between construction and `SetSampleTime`, and it is what
    /// `last_time` is computed from — see difference D2.
    pub const DEFAULT_SAMPLE_TIME: Millis = Millis::new(100);
    /// The library's default input low-pass coefficient (`PID_v1.h:106`).
    pub const DEFAULT_FILTER_ALPHA: f64 = 0.9;

    /// Construct, mirroring `PID::PID(...)` (`PID_v1.cpp:20-43`).
    ///
    /// `now` is what the C++ would have read from `millis()`: the constructor
    /// sets `lastTime = millis() - SampleTime` using the *default* 100 ms
    /// sample time, before any `set_sample_time` call, and that value
    /// determines when the first `compute()` produces an output. The firmware's
    /// `initializePID` raises the sample time to 1000 ms straight afterwards, so
    /// in practice the first compute happens one 100 ms tick after boot rather
    /// than one 1000 ms tick — a quirk this port reproduces rather than
    /// smooths over.
    #[must_use]
    pub fn new(
        now: Millis,
        kp: f64,
        ki: f64,
        kd: f64,
        proportional_on: ProportionalOn,
        direction: ControllerDirection,
    ) -> Self {
        let mut this = Self {
            input: 0.0,
            output: 0.0,
            setpoint: 0.0,
            kp: 0.0,
            ki: 0.0,
            kd: 0.0,
            kp_eff: 0.0,
            ki_eff: 0.0,
            kd_eff: 0.0,
            proportional_on: ProportionalOn::Error,
            p_on_e: true,
            direction,
            sample_time: Self::DEFAULT_SAMPLE_TIME,
            last_time: now,
            integrator: 0.0,
            out_min: Self::DEFAULT_OUT_MIN,
            out_max: Self::DEFAULT_OUT_MAX,
            integrator_min: Self::DEFAULT_INTEGRATOR_MIN,
            integrator_max: Self::DEFAULT_INTEGRATOR_MAX,
            filter_alpha: Self::DEFAULT_FILTER_ALPHA,
            last_input: 0.0,
            last_filtered_input: 0.0,
            last_filtered_differential: 0.0,
            last_error: 0.0,
            last_p_part: 0.0,
            last_d_part: 0.0,
            in_auto: false,
        };
        // PID_v1.cpp:42: `lastTime = millis() - SampleTime;`
        this.last_time = now.since(Self::DEFAULT_SAMPLE_TIME);
        // PID_v1.cpp:33-35 already set the limits above; then 39-40:
        this.set_direction(direction);
        this.set_tunings(kp, ki, kd, proportional_on);
        this
    }

    /// The divisor the `P_ON_E` derivative term uses.
    ///
    /// This is `SampleTime / 1000` with **integer** division, exactly as
    /// `PID_v1.cpp:85` computes it. See defect D-PID-1: for any sample time
    /// under 1000 ms this is `0.0`, and the output becomes `NaN` or `±inf`. It
    /// is exposed so the trap is visible instead of being rediscovered from a
    /// NaN on the bench.
    #[must_use]
    pub const fn derivative_seconds(&self) -> f64 {
        (self.sample_time.raw() / 1000) as f64
    }

    /// `SetTunings(Kp, Ki, Kd, POn)` (`PID_v1.cpp:145-169`).
    ///
    /// Returns `false` and changes nothing if any gain is negative
    /// (`PID_v1.cpp:146`). The C++ silently returns; surfacing it is difference
    /// D4.
    pub fn set_tunings(
        &mut self,
        kp: f64,
        ki: f64,
        kd: f64,
        proportional_on: ProportionalOn,
    ) -> bool {
        if kp < 0.0 || ki < 0.0 || kd < 0.0 {
            return false;
        }

        self.proportional_on = proportional_on;
        self.p_on_e = proportional_on == ProportionalOn::Error;

        self.kp = kp;
        self.ki = ki;
        self.kd = kd;

        let sample_time_in_sec = f64::from(self.sample_time.raw()) / 1000.0;
        self.kp_eff = kp;
        self.ki_eff = ki * sample_time_in_sec;
        self.kd_eff = kd / sample_time_in_sec;

        if self.direction == ControllerDirection::Reverse {
            self.kp_eff = -self.kp_eff;
            self.ki_eff = -self.ki_eff;
            self.kd_eff = -self.kd_eff;
        }

        if ki == 0.0 {
            self.integrator = 0.0;
        }
        true
    }

    /// `SetTunings(Kp, Ki, Kd)` (`PID_v1.cpp:174-176`): keep the current
    /// proportional mode.
    pub fn set_tunings_keeping_mode(&mut self, kp: f64, ki: f64, kd: f64) -> bool {
        self.set_tunings(kp, ki, kd, self.proportional_on)
    }

    /// `SetSampleTime(NewSampleTime)` (`PID_v1.cpp:181-188`).
    ///
    /// Rescales `ki` and `kd` by the ratio of the new and old periods, exactly
    /// as the C++ does, so a change of window does not retune the controller.
    /// A non-positive period is rejected.
    pub fn set_sample_time(&mut self, new_sample_time: Millis) -> bool {
        if new_sample_time.raw() == 0 {
            return false;
        }
        let ratio = f64::from(new_sample_time.raw()) / f64::from(self.sample_time.raw());
        self.ki_eff *= ratio;
        self.kd_eff /= ratio;
        self.sample_time = new_sample_time;
        true
    }

    /// `SetSmoothingFactor(alpha)` (`PID_v1.cpp:190-192`).
    pub fn set_smoothing_factor(&mut self, alpha: f64) {
        self.filter_alpha = alpha;
    }

    /// `SetOutputLimits(Min, Max)` (`PID_v1.cpp:202-218`).
    ///
    /// Returns `false` and changes nothing if `min >= max` (`PID_v1.cpp:203`).
    /// The C++ additionally clamps the caller's output and the integrator when
    /// running; here the output is a field, so both are clamped.
    pub fn set_output_limits(&mut self, min: f64, max: f64) -> bool {
        if min >= max {
            return false;
        }
        self.out_min = min;
        self.out_max = max;

        if self.in_auto {
            self.output = clamp(self.output, min, max);
            self.integrator = clamp(self.integrator, min, max);
        }
        true
    }

    /// `SetIntegratorLimits(Min, Max)` (`PID_v1.cpp:220-231`).
    ///
    /// Returns `false` and changes nothing if `min >= max`.
    pub fn set_integrator_limits(&mut self, min: f64, max: f64) -> bool {
        if min >= max {
            return false;
        }
        self.integrator_min = min;
        self.integrator_max = max;

        if self.in_auto {
            self.integrator = clamp(self.integrator, min, max);
        }
        true
    }

    /// `SetMode(Mode)` (`PID_v1.cpp:238-244`).
    ///
    /// Entering [`Mode::Automatic`] from [`Mode::Manual`] performs the bumpless
    /// transfer (`Initialize()`), so no arguments are needed.
    pub fn set_mode(&mut self, mode: Mode) {
        let new_auto = mode == Mode::Automatic;
        if new_auto && !self.in_auto {
            self.initialize();
        }
        self.in_auto = new_auto;
    }

    /// `SetControllerDirection(Direction)` (`PID_v1.cpp:266-273`).
    ///
    /// Flipping direction while running negates the gains in place, which is
    /// why the firmware only ever sets it in the constructor.
    pub fn set_direction(&mut self, direction: ControllerDirection) {
        if self.in_auto && direction != self.direction {
            self.kp_eff = -self.kp_eff;
            self.ki_eff = -self.ki_eff;
            self.kd_eff = -self.kd_eff;
        }
        self.direction = direction;
    }

    /// `Initialize()` (`PID_v1.cpp:250-258`): the bumpless transfer.
    ///
    /// Seeds the integrator from the current output and the input history from
    /// the current input, so the first automatic compute does not step.
    pub fn initialize(&mut self) {
        self.integrator = self.output;
        self.last_input = self.input;
        self.last_filtered_input = self.input;
        self.integrator = clamp(self.integrator, self.out_min, self.out_max);
    }

    /// `Compute()` (`PID_v1.cpp:58-138`).
    ///
    /// Returns `true` when a new output was produced, `false` when the sample
    /// period has not elapsed or the controller is in [`Mode::Manual`] — the
    /// same contract as the C++, so the caller can use the return value to
    /// decide whether to log.
    ///
    /// The C++ reads the clock itself; here it is `now`. The elapsed-time
    /// subtraction wraps, matching the 32-bit `unsigned long` in `PID_v1.cpp:60`.
    pub fn compute(&mut self, now: Millis) -> bool {
        if !self.in_auto {
            return false;
        }
        let elapsed = now.since(self.last_time);
        if elapsed.raw() < self.sample_time.raw() {
            return false;
        }

        let input = self.input;
        let error = self.setpoint - input;

        // Integral part, with the anti-windup gate. See D-PID-3: the gate is a
        // strict interior test with a 0.01 dead band, so an output sitting
        // exactly on a limit does not accumulate.
        let gated = self.output < self.out_max - 0.01 && self.output > self.out_min + 0.01;
        if !self.p_on_e || gated {
            self.integrator += self.ki_eff * error;
        }

        // Exponentially weighted moving average of the input, used only for the
        // P_ON_E derivative (PID_v1.cpp:74-79).
        let old_filtered = self.last_filtered_input;
        self.last_filtered_input =
            self.filter_alpha * self.last_filtered_input + (1.0 - self.filter_alpha) * input;

        // Differential of the input, not of the error: with a constant setpoint
        // the two are equal, and using the input avoids derivative kick when the
        // setpoint steps. P_ON_M uses the *unfiltered* difference because the
        // filter would otherwise leave it nothing to work with.
        let d_input = if self.p_on_e {
            (self.last_filtered_input - old_filtered) / self.derivative_seconds()
        } else {
            input - self.last_input
        };

        // Proportional on measurement: the kp * dInput contribution moves into
        // the integrator and there is no P part at all (PID_v1.cpp:91-94, 111-116).
        if !self.p_on_e {
            self.integrator -= self.kp_eff * d_input;
        }

        // Worst-case anti-windup against the output range, then against the
        // integrator's own range — the latter only in normal PID mode.
        self.integrator = clamp(self.integrator, self.out_min, self.out_max);
        if self.p_on_e {
            self.integrator = clamp(self.integrator, self.integrator_min, self.integrator_max);
        }

        let mut out = if self.p_on_e {
            self.kp_eff * error
        } else {
            0.0
        };
        out += self.integrator - self.kd_eff * d_input;
        out = clamp(out, self.out_min, self.out_max);
        self.output = out;

        self.last_filtered_differential = d_input;
        self.last_input = input;
        self.last_p_part = if self.p_on_e {
            self.kp_eff * error
        } else {
            0.0
        };
        self.last_d_part = -self.kd_eff * d_input;
        self.last_error = error;
        self.last_time = now;
        true
    }

    // ---- getters, one per PID_v1.h accessor ------------------------------

    /// `GetKp()` — the gain as entered, *not* the direction-adjusted value.
    #[must_use]
    pub const fn kp(&self) -> f64 {
        self.kp
    }

    /// `GetKi()` — the gain as entered.
    #[must_use]
    pub const fn ki(&self) -> f64 {
        self.ki
    }

    /// `GetKd()` — the gain as entered.
    #[must_use]
    pub const fn kd(&self) -> f64 {
        self.kd
    }

    /// The proportional gain actually used for arithmetic, after the
    /// sample-time scaling and any direction negation.
    #[must_use]
    pub const fn effective_kp(&self) -> f64 {
        self.kp_eff
    }

    /// The integral gain actually used for arithmetic. See
    /// [`Self::effective_kp`].
    #[must_use]
    pub const fn effective_ki(&self) -> f64 {
        self.ki_eff
    }

    /// The derivative gain actually used for arithmetic. See
    /// [`Self::effective_kp`].
    #[must_use]
    pub const fn effective_kd(&self) -> f64 {
        self.kd_eff
    }

    /// `GetMode()`.
    #[must_use]
    pub const fn mode(&self) -> Mode {
        if self.in_auto {
            Mode::Automatic
        } else {
            Mode::Manual
        }
    }

    /// `GetDirection()`.
    #[must_use]
    pub const fn direction(&self) -> ControllerDirection {
        self.direction
    }

    /// `GetPonE()`.
    #[must_use]
    pub const fn proportional_on_error(&self) -> bool {
        self.p_on_e
    }

    /// `GetDeltaInput()` — the `dInput` used in the last compute.
    #[must_use]
    pub const fn delta_input(&self) -> f64 {
        self.last_filtered_differential
    }

    /// `GetInputError()` — the error from the last compute.
    #[must_use]
    pub const fn last_error(&self) -> f64 {
        self.last_error
    }

    /// `GetLastPPart()`.
    #[must_use]
    pub const fn last_p_part(&self) -> f64 {
        self.last_p_part
    }

    /// `GetLastIPart()` — the integrator sum.
    #[must_use]
    pub const fn last_i_part(&self) -> f64 {
        self.integrator
    }

    /// `GetLastDPart()`.
    #[must_use]
    pub const fn last_d_part(&self) -> f64 {
        self.last_d_part
    }

    /// The current sample period.
    #[must_use]
    pub const fn sample_time(&self) -> Millis {
        self.sample_time
    }
}

/// `PID_v1.cpp`'s clamp idiom: a chain of `if`s, not a `max`/`min` pair.
///
/// The C++ writes `if (x > max) x = max; else if (x < min) x = min;` in six
/// places. The order matters for `NaN`: both comparisons are false, so a `NaN`
/// passes through *unchanged* rather than being snapped to a limit. `f64::clamp`
/// panics on `NaN` bounds and would not reproduce that, so this helper is the
/// faithful form.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value > max {
        max
    } else if value < min {
        min
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn production(now: Millis) -> Controller {
        // SystemInitializer.cpp:296-305 then :544-556.
        let mut c = Controller::new(
            now,
            62.0,
            62.0 / 52.0,
            11.5 * 62.0,
            ProportionalOn::Error,
            ControllerDirection::Direct,
        );
        assert!(c.set_tunings(62.0, 62.0 / 52.0, 11.5 * 62.0, ProportionalOn::Error));
        assert!(c.set_sample_time(Millis::new(1000)));
        assert!(c.set_output_limits(0.0, 1000.0));
        assert!(c.set_integrator_limits(0.0, 55.0));
        c.set_smoothing_factor(0.6);
        c.set_mode(Mode::Automatic);
        c
    }

    #[test]
    fn manual_mode_never_computes() {
        let mut c = production(Millis::ZERO);
        c.set_mode(Mode::Manual);
        c.input = 60.0;
        assert!(!c.compute(Millis::new(10_000)));
        assert_close(c.output, 0.0);
    }

    #[test]
    fn too_soon_does_not_compute() {
        let mut c = production(Millis::ZERO);
        c.input = 60.0;
        // last_time is (0 - 100) wrapping, so the first compute that satisfies
        // the 1000 ms period is at t = 999.
        assert!(!c.compute(Millis::ZERO));
        assert!(c.compute(Millis::new(999)));
        assert!(!c.compute(Millis::new(1000)));
    }

    #[test]
    fn negative_gains_are_rejected_without_side_effects() {
        let mut c = production(Millis::ZERO);
        let before = c.clone();
        assert!(!c.set_tunings(-1.0, 0.5, 0.5, ProportionalOn::Error));
        assert_eq!(c, before);
    }

    #[test]
    fn degenerate_limits_are_rejected() {
        let mut c = production(Millis::ZERO);
        let before = c.clone();
        assert!(!c.set_output_limits(10.0, 10.0));
        assert!(!c.set_output_limits(20.0, 10.0));
        assert!(!c.set_integrator_limits(5.0, 5.0));
        assert!(!c.set_sample_time(Millis::ZERO));
        assert_eq!(c, before);
    }

    /// A gentle controller used to exercise integrator behaviour without the
    /// production gains saturating the output on every step.
    /// Assert two floats agree to well below the parity tolerance.
    #[track_caller]
    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "expected {expected}, got {actual}"
        );
    }

    fn gentle(now: Millis) -> Controller {
        let mut c = Controller::new(
            now,
            1.0,
            1.0,
            0.0,
            ProportionalOn::Error,
            ControllerDirection::Direct,
        );
        assert!(c.set_sample_time(Millis::new(1000)));
        assert!(c.set_output_limits(0.0, 1000.0));
        assert!(c.set_integrator_limits(0.0, 55.0));
        c.set_smoothing_factor(0.0); // identity filter: dInput is exactly 0 here
        c.set_mode(Mode::Automatic);
        c.input = 0.0;
        c.setpoint = 1.0;
        c.output = 0.0;
        c
    }

    #[test]
    fn zero_integrator_gain_resets_the_integrator() {
        let mut c = gentle(Millis::ZERO);
        assert!(c.compute(Millis::new(1000))); // output 1.0
        assert!(c.compute(Millis::new(2000))); // integrator accumulates to 1.0
        assert!(c.last_i_part() > 0.0);
        // PID_v1.cpp:166-168
        assert!(c.set_tunings(1.0, 0.0, 0.0, ProportionalOn::Error));
        assert_close(c.last_i_part(), 0.0);
    }

    /// D-PID-3, pinned: the anti-windup gate is a *strict* interior test, so an
    /// output sitting exactly on a limit does not accumulate. The shipped
    /// firmware starts at output 0 with limits (0, 1000), so its integrator is
    /// frozen until the output leaves the exact boundary.
    #[test]
    fn an_output_exactly_on_a_limit_does_not_accumulate() {
        let mut c = gentle(Millis::ZERO);
        assert!(c.compute(Millis::new(1000)));
        assert_close(c.output, 1.0);
        // 1.0 is strictly inside (0.01, 999.99), so this one does accumulate.
        assert!(c.compute(Millis::new(2000)));
        assert!(c.last_i_part() > 0.0);

        // Now force the output back to exactly outMin and confirm the
        // integrator freezes rather than growing.
        c.output = 0.0;
        let frozen = c.last_i_part();
        assert!(c.compute(Millis::new(3000)));
        assert_close(c.last_i_part(), frozen); // output 0.0 is not > outMin + 0.01

        c.output = 0.005;
        assert!(c.compute(Millis::new(4000)));
        assert_close(c.last_i_part(), frozen); // still below the 0.01 dead band
    }

    #[test]
    fn entering_automatic_performs_a_bumpless_transfer() {
        let mut c = production(Millis::ZERO);
        c.set_mode(Mode::Manual);
        c.input = 60.0;
        c.output = 400.0;
        c.set_mode(Mode::Automatic);
        // Initialize() seeds the integrator from the output.
        assert_close(c.last_i_part(), 400.0);
    }

    /// D-PID-1: the integer-division divisor. Documented, reproduced, tested.
    #[test]
    fn derivative_divisor_is_integer_divided_sample_time() {
        let mut c = production(Millis::ZERO);
        assert_close(c.derivative_seconds(), 1.0);
        assert!(c.set_sample_time(Millis::new(500)));
        // This is upstream defect D-PID-1: 500/1000 == 0 in integer division.
        assert_close(c.derivative_seconds(), 0.0);
    }

    #[test]
    fn output_stays_within_limits() {
        let mut c = production(Millis::ZERO);
        c.setpoint = 130.0;
        for step in 0..40 {
            c.input = 20.0;
            if c.compute(Millis::new(u32::try_from(step).unwrap_or(0) * 1000 + 999)) {
                assert!(
                    (0.0..=1000.0).contains(&c.output),
                    "output {} outside limits",
                    c.output
                );
            }
        }
    }
}
