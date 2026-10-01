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
//! * **D-PID-1 — integer division in the `P_ON_E` derivative term. FIXED.**
//!   `PID_v1.cpp:85` reads
//!   `dInput = (lastFilteredInput - oldFiltered) / (SampleTime / 1000);`
//!   `SampleTime` is an `unsigned long` and `/ 1000` is **integer** division.
//!   Any sample time below 1000 ms therefore yields a zero divisor, the
//!   derivative is `±inf`, and the whole output becomes `NaN` — which the C++'s
//!   own output clamp cannot catch, because `if (x > max) … else if (x < min)`
//!   lets `NaN` through.
//!
//!   The shipped firmware escapes this **only by coincidence**:
//!   `SystemInitializer.cpp:551` sets the sample time to exactly
//!   `processWindowSize()` = 1000 ms, so `1000 / 1000 == 1`. Nothing enforces
//!   that, and R1-07 — changing the heater output method — is exactly the kind
//!   of change that would move the window.
//!
//!   **The port divides by the real elapsed time instead**
//!   ([`Controller::derivative_seconds_at`]), so the divisor is positive for
//!   every window and no configuration can produce a `NaN`. The trap stays
//!   visible: [`Controller::derivative_seconds`] is the nominal term, and this
//!   paragraph is the reason.
//!
//!   Parity at the shipped window is unaffected and is *measured*: all four
//!   oracle scenarios still match the C++ `to_bits()` on every step, maximum
//!   |delta| **0.0**. The port can only differ from the C++ when a step arrives
//!   *late*, where the C++ divides by the nominal 1.0 s and the port divides by
//!   the interval that actually elapsed. That is a deliberate, bounded
//!   divergence — see `docs/rust-migration/intentional-diffs.md` #4.
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

    /// The `P_ON_E` derivative divisor for a sample that lands **exactly** on the
    /// window.
    ///
    /// This is `SampleTime / 1000` computed in `f64` — the C++ computes it in
    /// `unsigned long` (`PID_v1.cpp:85`) and the *shape* of the expression is
    /// kept so the two are recognisably the same term. The difference is the
    /// `f64` division, and that difference is the whole of deliberate
    /// divergence D-PID-1.
    ///
    /// # D-PID-1, closed
    ///
    /// The C++ reads
    ///
    /// ```cpp
    /// dInput = (lastFilteredInput - oldFiltered) / (SampleTime / 1000);
    /// ```
    ///
    /// `SampleTime` is an `unsigned long`, so `/ 1000` is **integer** division.
    /// Any sample time below 1000 ms makes the divisor `0`, the derivative is
    /// `±inf`, and the whole output becomes `NaN` (it survives the C++'s own
    /// `if (x > max) … else if (x < min)` clamp, which lets `NaN` through
    /// untouched). The shipped firmware escapes this **by coincidence** —
    /// `SystemInitializer.cpp:551` sets the sample time to exactly
    /// `processWindowSize()` = 1000 ms, so `1000 / 1000 == 1` — and nothing
    /// enforces that.
    ///
    /// In the port the divisor is [`Self::derivative_seconds_at`] with the
    /// **actual elapsed time**, so it is strictly positive for every window and
    /// a `SetSampleTime` can never manufacture a `NaN`. The trap is still
    /// *visible*: [`Self::derivative_seconds`] is the nominal term, this
    /// function is what the code divides by, and both are in the module docs.
    ///
    /// # Parity
    ///
    /// At the shipped 1000 ms window, and for any step that lands exactly on
    /// it, this is `1.0` and the C++'s is `1.0`, so the term is bit-identical.
    /// That is measured, not asserted: see
    /// [`crate::pid_parity::scenario_a_production_pon_e_matches_the_cpp_library`]
    /// and its three siblings, which compare `to_bits()` on every step of all
    /// four oracle scenarios — maximum |delta| **0.0**.
    ///
    /// The one place the port can differ from the C++ at a 1000 ms window is a
    /// step that arrives *late*: the C++ divides by the nominal 1.0 s whatever
    /// the real interval was, the port divides by the real one. That is the
    /// point of the fix, it is bounded by the scheduling jitter, and it is
    /// quantified by
    /// [`crate::pid_parity::scenario_e_a_late_step_uses_the_real_interval`].
    #[must_use]
    pub fn derivative_seconds(&self) -> f64 {
        f64::from(self.sample_time.raw()) / 1000.0
    }

    /// The divisor the `P_ON_E` derivative term **actually** uses: the real
    /// time since the previous `compute`, in seconds.
    ///
    /// Strictly positive whenever `compute` got past its own guard, because
    /// that guard is `elapsed >= sample_time` and `set_sample_time` rejects a
    /// zero period. That is the property the C++ lacks.
    ///
    /// Why elapsed and not the configured window: the C++'s *intent* is
    /// plainly `dInput/dt` with `dt` the time between samples, and a controller
    /// whose `dt` is a configuration constant rather than a measurement is a
    /// controller whose derivative gain is wrong by exactly the ratio of the
    /// real interval to the nominal one. On a 100 Hz control loop with a 1000 ms
    /// window that error is under 1 %, which is why nobody has noticed; move the
    /// window to 500 ms and the C++ does not merely get it wrong, it divides by
    /// zero.
    #[must_use]
    pub fn derivative_seconds_at(&self, elapsed: Millis) -> f64 {
        f64::from(elapsed.raw()) / 1000.0
    }

    /// The three gains as the operator entered them — `PID_v1`'s `GetKp`,
    /// `GetKi` and `GetKd` (`PID_v1.h:279-292`).
    ///
    /// **Needed by the display, and the reason is a defect it caught.** The
    /// screen's PID row shows `Kp | Kp/Ki | Kd/Kp`
    /// (`DisplayTemplateBase.h:165`), and the firmware was feeding it the
    /// controller's *last P, I and D terms* instead — the integral accumulator
    /// among them, which grows without bound. The row read `4444|81|0 - 100%`:
    /// three numbers that mean nothing to an operator, the first wide enough to
    /// push the rest into the output column.
    ///
    /// The gains as entered, not `kp_eff`/`ki_eff`/`kd_eff`: the C++'s `GetKp`
    /// returns the entered value, and that is what the row is labelled with.
    #[must_use]
    pub fn gains(&self) -> (f64, f64, f64) {
        (self.kp, self.ki, self.kd)
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

    /// Whether the controller is in [`Mode::Automatic`].
    ///
    /// Exists so a caller that caches the mode can seed the cache from the
    /// controller rather than from what it *intends* the mode to be. Those are
    /// not the same thing, and assuming they are is a bug this port shipped:
    /// the firmware's cache was initialised to "Automatic" while the controller
    /// was in Manual, so the transition was never detected, `set_mode` was never
    /// called, and `compute` returned `false` on every tick — a machine in
    /// `PidNormal` with a live setpoint, a 7 K error and a permanently zero
    /// heater duty.
    ///
    /// Read this at construction and the cache cannot disagree with the thing it
    /// is caching.
    #[must_use]
    pub const fn in_automatic(&self) -> bool {
        self.in_auto
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
            // PID_v1.cpp:85, with the divisor fixed — see D-PID-1 in the module
            // docs. `elapsed` is the same `timeChange` the guard above used.
            (self.last_filtered_input - old_filtered) / self.derivative_seconds_at(elapsed)
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

    /// D-PID-1, closed: the divisor is the real elapsed time, so no window can
    /// produce a zero. The nominal term is still exposed so the trap is
    /// visible.
    #[test]
    fn derivative_divisor_is_never_zero_at_any_window() {
        let mut c = production(Millis::ZERO);
        assert_close(c.derivative_seconds(), 1.0);
        // At the shipped 1000 ms window the nominal term is exactly the C++'s
        // `1000 / 1000`, so the arithmetic is bit-identical.
        assert_eq!(c.derivative_seconds().to_bits(), 1.0f64.to_bits());

        for window_ms in [1_u32, 10, 100, 250, 499, 500, 999, 1000, 2_000, 60_000] {
            assert!(c.set_sample_time(Millis::new(window_ms)));
            assert!(
                c.derivative_seconds() > 0.0,
                "{window_ms} ms: the nominal divisor must be positive"
            );
            // And the divisor actually used is the elapsed time, which the
            // `compute` guard has already proved is >= the window.
            assert!(c.derivative_seconds_at(Millis::new(window_ms)) > 0.0);
        }
    }

    /// The new behaviour being bought: a window the C++ could not survive at all
    /// now produces a finite, correct derivative.
    ///
    /// The C++ at `SampleTime = 500` divides the filtered input difference by
    /// zero, so the derivative is `±inf` and the output is `NaN` — reproduced
    /// verbatim in oracle scenario D. Here it is `difference / 0.5 s`.
    ///
    /// The numbers are checked against the arithmetic by hand rather than against
    /// a table, because there is no C++ value to compare to: that is the point.
    #[test]
    fn a_sub_second_window_now_yields_a_finite_derivative() {
        let mut c = Controller::new(
            Millis::ZERO,
            30.0,
            0.5,
            0.25,
            ProportionalOn::Error,
            ControllerDirection::Direct,
        );
        c.input = 60.0;
        c.setpoint = 95.0;
        c.output = 0.0;
        assert!(c.set_output_limits(0.0, 500.0));
        assert!(c.set_integrator_limits(0.0, 55.0));
        // EMA 0.6, as production uses.
        c.set_smoothing_factor(0.6);
        assert!(c.set_sample_time(Millis::new(500)));
        c.set_mode(Mode::Automatic);

        assert!(c.compute(Millis::new(500)));
        assert!(c.output.is_finite(), "step 1: {}", c.output);
        // `Initialize()` seeded `lastFilteredInput` with the input, so the first
        // filtered difference is 0.6*60 + 0.4*60 - 60 = 0 and dInput = 0.
        assert_close(c.delta_input(), 0.0);

        c.input = 62.0;
        assert!(c.compute(Millis::new(1000)));
        assert!(c.output.is_finite(), "step 2: {}", c.output);
        // filtered = 0.6*60 + 0.4*62 = 60.8; delta = 0.8; dInput = 0.8/0.5 = 1.6.
        assert_close(c.delta_input(), 1.6);

        c.input = 61.0;
        assert!(c.compute(Millis::new(1500)));
        assert!(c.output.is_finite(), "step 3: {}", c.output);
        // filtered = 0.6*60.8 + 0.4*61 = 60.88; delta = 0.08; dInput = 0.16.
        assert_close(c.delta_input(), 0.16);

        // The integrator and the output are inside their limits, which the C++
        // can never be at this window.
        assert!((0.0..=500.0).contains(&c.output), "{}", c.output);
        assert!(
            (0.0..=55.0).contains(&c.last_i_part()),
            "{}",
            c.last_i_part()
        );
    }

    /// A late step: the C++ divides by the nominal window whatever the real
    /// interval was. This is the one place the port can differ from it at a
    /// 1000 ms window, so it is pinned explicitly rather than left implicit.
    #[test]
    fn a_late_step_uses_the_real_interval() {
        // Built by hand rather than via `production()` so the input is seeded
        // *before* `set_mode(Automatic)`, which is when `Initialize()` runs and
        // is therefore what the first filtered difference is measured from.
        let mut c = Controller::new(
            Millis::ZERO,
            1.0,
            0.0,
            1.0,
            ProportionalOn::Error,
            ControllerDirection::Direct,
        );
        assert!(c.set_output_limits(-1000.0, 1000.0));
        assert!(c.set_integrator_limits(-1000.0, 1000.0));
        assert!(c.set_sample_time(Millis::new(1000)));
        c.set_smoothing_factor(0.0); // identity: the filtered difference is raw
        c.input = 60.0;
        c.setpoint = 95.0;
        c.output = 0.0;
        c.set_mode(Mode::Automatic);

        // The constructor left `last_time = 0 - 100`, so the first compute at
        // t = 1000 sees 1100 ms: late even the first time, exactly as the C++.
        assert!(c.compute(Millis::new(1000)));
        // Nothing has changed yet, so the difference is 0 whatever it is divided
        // by — the C++ divides 0 by 1.0 and gets 0 too.
        assert_close(c.delta_input(), 0.0);
        assert_close(c.derivative_seconds_at(Millis::new(1100)), 1.1);
        assert_close(c.derivative_seconds(), 1.0);

        c.input = 61.0;
        // Exactly on the window: divisor 1.0 s, so dInput = 1.0 — the C++'s value.
        assert!(c.compute(Millis::new(2000)));
        assert_close(c.delta_input(), 1.0);
        assert_close(c.derivative_seconds_at(Millis::new(1000)), 1.0);

        // 500 ms late: divisor 1.5 s, so dInput = 2/1.5. The C++ would say 2.0.
        c.input = 63.0;
        assert!(c.compute(Millis::new(3500)));
        assert_close(c.delta_input(), 2.0 / 1.5);
        assert_close(c.derivative_seconds_at(Millis::new(1500)), 1.5);
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

#[cfg(test)]
mod mode_query_tests {
    use super::{Controller, ControllerDirection, Millis, Mode, ProportionalOn};

    /// A controller with the C++'s production gains (`ProcessController.cpp:296-305`).
    fn production() -> Controller {
        let mut pid = Controller::new(
            Millis::ZERO,
            62.0,
            62.0 / 52.0,
            11.5 * 62.0,
            ProportionalOn::Error,
            ControllerDirection::Direct,
        );
        assert!(pid.set_output_limits(0.0, 1000.0));
        assert!(pid.set_sample_time(Millis::new(100)));
        pid
    }

    /// A fresh controller is in Manual, and says so.
    ///
    /// This is the whole point of [`Controller::in_automatic`]. The firmware
    /// caches the mode so it only calls `set_mode` on a change, and the bug this
    /// pins is a cache seeded from *intent* rather than from the controller: the
    /// cache said Automatic, the controller was Manual, the change was never
    /// detected, and `compute` returned `false` forever. The machine looked
    /// healthy — right state, right setpoint, 7 K of error — with a heater that
    /// never switched on.
    ///
    /// A caller that reads `in_automatic()` at construction cannot make that
    /// mistake, so this asserts the value it reads.
    #[test]
    fn a_new_controller_reports_manual_so_a_caller_cannot_seed_the_cache_wrong() {
        let pid = production();
        assert!(
            !pid.in_automatic(),
            "Controller::new must report Manual, or a caching caller will believe \
             a mode the controller is not in"
        );
    }

    /// The query follows the mode, so a cache seeded from it stays correct.
    #[test]
    fn the_mode_query_tracks_set_mode() {
        let mut pid = production();
        assert!(!pid.in_automatic());

        pid.set_mode(Mode::Automatic);
        assert!(pid.in_automatic(), "set_mode(Automatic) did not take");

        pid.set_mode(Mode::Manual);
        assert!(!pid.in_automatic(), "set_mode(Manual) did not take");
    }

    /// A controller in Manual never computes — the condition that made the
    /// original bug invisible, since everything *looked* fine.
    #[test]
    fn manual_means_compute_returns_false_however_large_the_error() {
        let mut pid = production();
        pid.input = 22.0;
        pid.setpoint = 30.0;
        assert!(
            !pid.compute(Millis::new(1000)),
            "a Manual controller must not compute; a 8 K error and a zero duty is \
             the exact symptom of the bug this module documents"
        );
    }
}
