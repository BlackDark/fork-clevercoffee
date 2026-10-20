//! Frozen reference vectors for [`crate::pid::Controller`]: the expected heater outputs, and the
//! evidence that this controller reproduces the reference implementation bit for bit.
//!
//! The expected values in the tables below were **generated, never typed by hand**. A
//! small program compiled the reference PID library against a hand-driven clock, replayed
//! a fixed input sequence and printed one line per step as the IEEE-754 bit pattern of the
//! resulting `double`, so nothing was lost in transcription and the two can be compared
//! exactly as well as approximately.
//!
//! **The tables are frozen.** That program and the reference library it compiled are not in
//! this tree, so the vectors cannot be regenerated here; they are the record of what the
//! reference did, kept because a controller's arithmetic has to be checked against something
//! other than itself. Re-deriving them means rebuilding that program. What runs on every
//! `just test` is this file: the same input sequences replayed against
//! [`crate::pid::Controller`] and compared with the frozen vectors.
//!
//! The five scenarios are the reference's, one for one:
//!
//! * **A** -- the production `P_ON_E` configuration and a boiler warm-up, including
//!   output saturation in both directions, integrator limiting, recovery, and a `u32`
//!   millisecond wrap across zero.
//! * **B** -- the brew-detection `P_ON_M` tuning, including the mid-run proportional-mode
//!   switches the firmware performs.
//! * **C** -- manual/automatic bumpless transfer, mid-run re-tuning and limit changes, and
//!   a rejected negative tuning.
//! * **D** -- upstream defect D-PID-1: a sub-second sample time makes the `P_ON_E`
//!   derivative divisor zero and the output `NaN`. **This firmware fixes that**, so
//!   `SCENARIO_D_ORACLE` holds the reference's vector and the test asserts the
//!   divergence. See the [`crate::pid`] module documentation.
//! * **E** -- the other half of D-PID-1: the shipped 1000 ms window driven on deliberately
//!   ragged timestamps, the only way this controller can differ at the production window.
//!
//! [`TOLERANCE`] of 1e-6 is an acceptance bound, not the measured agreement: on scenarios
//! A, B and C every one of the 42 steps matches the reference bit for bit, so the
//! **maximum |delta| is 0.0**. [`assert_scenario`] requires that as well, so the tolerance
//! exists only to make a future floating-point contraction difference visible as a failure
//! rather than as a silent drift.

// `alloc` is linked for tests only. The library itself uses no allocation at all; a
// growable log of parity results is a test convenience, not a runtime requirement, and
// pulling in `alloc` unconditionally would be a lie about the crate's dependencies.
use alloc::vec::Vec;

use super::pid::{Controller, ControllerDirection, Mode, ProportionalOn};
use super::units::Millis;

/// Absolute tolerance for the output comparison. The measured difference is 0.0 on every
/// step; the tolerance makes a future floating-point contraction difference fail loudly
/// rather than drift silently.
const TOLERANCE: f64 = 1e-6;

/// One expected [`Controller::compute`] result, as printed by the oracle.
#[derive(Clone, Copy, Debug)]
struct Expected {
    /// The virtual clock value the oracle set before the step.
    now: u32,
    /// Whether the step produced a new output.
    computed: bool,
    /// The IEEE-754 bit pattern of the output afterwards.
    output_bits: u64,
}

const fn e(now: u32, computed: bool, output_bits: u64) -> Expected {
    Expected {
        now,
        computed,
        output_bits,
    }
}

/// What the Rust controller actually did.
#[derive(Clone, Copy, Debug)]
struct Observed {
    computed: bool,
    output: f64,
}

/// Expected results for oracle scenario `scenario_a_production_pon_e`.
const SCENARIO_A: &[Expected] = &[
    e(0, false, 0x0000_0000_0000_0000),            // oracle: 0
    e(999, true, 0x408f_4000_0000_0000),           // oracle: 1000
    e(1000, false, 0x408f_4000_0000_0000),         // oracle: 1000
    e(3000, true, 0x408f_4000_0000_0000),          // oracle: 1000
    e(4000, true, 0x0000_0000_0000_0000),          // oracle: 0
    e(5000, true, 0x0000_0000_0000_0000),          // oracle: 0
    e(6000, true, 0x0000_0000_0000_0000),          // oracle: 0
    e(7000, true, 0x0000_0000_0000_0000),          // oracle: 0
    e(8000, true, 0x0000_0000_0000_0000),          // oracle: 0
    e(9000, true, 0x0000_0000_0000_0000),          // oracle: 0
    e(10000, true, 0x0000_0000_0000_0000),         // oracle: 0
    e(11000, true, 0x0000_0000_0000_0000),         // oracle: 0
    e(12000, true, 0x0000_0000_0000_0000),         // oracle: 0
    e(13000, true, 0x0000_0000_0000_0000),         // oracle: 0
    e(14000, true, 0x408f_4000_0000_0000),         // oracle: 1000
    e(15000, true, 0x408f_4000_0000_0000),         // oracle: 1000
    e(16000, true, 0x408f_4000_0000_0000),         // oracle: 1000
    e(17000, true, 0x0000_0000_0000_0000),         // oracle: 0
    e(18000, true, 0x0000_0000_0000_0000),         // oracle: 0
    e(4_294_963_200, true, 0x0000_0000_0000_0000), // oracle: 0
    e(256, true, 0x0000_0000_0000_0000),           // oracle: 0
    e(512, false, 0x0000_0000_0000_0000),          // oracle: 0
];

/// Expected results for oracle scenario `scenario_b_brew_detection_pon_m`.
const SCENARIO_B: &[Expected] = &[
    e(0, false, 0x0000_0000_0000_0000),    // oracle: 0
    e(1000, true, 0x0000_0000_0000_0000),  // oracle: 0
    e(2000, true, 0x4070_6800_0000_0000),  // oracle: 262.5
    e(3000, true, 0x4089_0000_0000_0000),  // oracle: 800
    e(4000, true, 0x408f_4000_0000_0000),  // oracle: 1000
    e(5000, true, 0x408f_4000_0000_0000),  // oracle: 1000
    e(6000, true, 0x408f_4000_0000_0000),  // oracle: 1000
    e(7000, true, 0x408f_4000_0000_0000),  // oracle: 1000
    e(8000, true, 0x0000_0000_0000_0000),  // oracle: 0
    e(9000, true, 0x408f_4000_0000_0000),  // oracle: 1000
    e(10000, true, 0x408d_20ed_11e6_b216), // oracle: 932.11575679999328
];

/// Expected results for oracle scenario `scenario_c_manual_automatic_and_limits`.
const SCENARIO_C: &[Expected] = &[
    e(0, false, 0x0000_0000_0000_0000),    // oracle: 0
    e(5000, false, 0x0000_0000_0000_0000), // oracle: 0
    e(6000, true, 0x408f_4000_0000_0000),  // oracle: 1000
    e(7000, true, 0x408f_4000_0000_0000),  // oracle: 1000
    e(8000, true, 0x406f_4000_0000_0000),  // oracle: 250
    e(9000, true, 0x406f_4000_0000_0000),  // oracle: 250
    e(10000, true, 0x406f_4000_0000_0000), // oracle: 250
    e(11000, true, 0x406f_4000_0000_0000), // oracle: 250
    e(12000, true, 0x406f_4000_0000_0000), // oracle: 250
];

/// Expected results for oracle scenario `scenario_d_sub_second_sample_time_is_nan`.
///
/// **These are the reference's outputs and this firmware deliberately does not reproduce
/// them.** The table is kept, unmodified, as the evidence for the deliberate
/// difference; `scenario_d_the_reference_goes_nan_and_this_port_does_not`
/// asserts the divergence rather than the agreement. Deleting this table would delete the
/// only record of what a sub-second window produces.
const SCENARIO_D_ORACLE: &[Expected] = &[
    e(500, true, 0x7ff8_0000_0000_0000),  // oracle: nan
    e(1000, true, 0x0000_0000_0000_0000), // oracle: 0
    e(1500, true, 0x0000_0000_0000_0000), // oracle: 0
    e(2000, true, 0x4021_0000_0000_0000), // oracle: 8.5
    e(2500, true, 0x0000_0000_0000_0000), // oracle: 0
];

/// Expected results for oracle scenario
/// `scenario_e_late_steps_use_the_nominal_window` -- again, the **reference's** outputs.
const SCENARIO_E_ORACLE: &[Expected] = &[
    e(0, false, 0x0000_0000_0000_0000),   // oracle: 0
    e(1000, true, 0x403f_0000_0000_0000), // oracle: 31
    e(2500, true, 0x403d_0000_0000_0000), // oracle: 29
    e(3500, true, 0x403b_0000_0000_0000), // oracle: 27
    e(5500, true, 0x4039_0000_0000_0000), // oracle: 25
];

/// The production configuration, in the order the firmware's bring-up applies it.
///
/// Order is load-bearing: `set_sample_time` rescales `ki` and `kd`, so it must follow the
/// tuning, and `set_mode(Automatic)` performs the bumpless transfer, so it must follow
/// everything that can move the integrator.
fn configure_production(
    controller: &mut Controller,
    kp: f64,
    ki: f64,
    kd: f64,
    proportional_on: ProportionalOn,
) {
    assert!(controller.set_tunings(kp, ki, kd, proportional_on));
    assert!(controller.set_sample_time(Millis::new(1000)));
    assert!(controller.set_output_limits(0.0, 1000.0));
    assert!(controller.set_integrator_limits(0.0, 55.0));
    controller.set_smoothing_factor(0.6);
    controller.set_mode(Mode::Automatic);
}

/// Compare one replayed scenario against the oracle, requiring exact agreement.
fn assert_scenario(name: &str, expected: &[Expected], observed: &[Observed]) {
    assert_eq!(
        observed.len(),
        expected.len(),
        "{name}: the replay produced a different number of results than the oracle"
    );

    for (index, (want, got)) in expected.iter().zip(observed.iter()).enumerate() {
        assert_eq!(
            got.computed, want.computed,
            "{name} step {index} (t = {} ms): the compute guard disagreed",
            want.now
        );
        let want_value = f64::from_bits(want.output_bits);
        // NaN on both sides is agreement, not a failure. Scenario D depends on that,
        // and on nothing else: the reference produces NaN there and so must this port.
        if want_value.is_nan() {
            assert!(
                got.output.is_nan(),
                "{name} step {index} (t = {} ms): the reference produced NaN, this \
                 port produced {}",
                want.now,
                got.output
            );
            continue;
        }
        let difference = (got.output - want_value).abs();
        assert!(
            difference <= TOLERANCE,
            "{name} step {index} (t = {} ms): output {} vs the reference {want_value} \
             (difference {difference})",
            want.now,
            got.output
        );
        assert_eq!(
            got.output.to_bits(),
            want.output_bits,
            "{name} step {index} (t = {} ms): not bit-identical to the reference",
            want.now
        );
    }
}

/// Convenience: compute once and record the result.
fn step(controller: &mut Controller, now: u32, log: &mut Vec<Observed>) {
    let computed = controller.compute(Millis::new(now));
    log.push(Observed {
        computed,
        output: controller.output,
    });
}

#[test]
fn scenario_a_production_pon_e_matches_the_reference_library() {
    let mut log = Vec::new();

    // The production configuration: AGGKP = 62, Ki = Kp/Tn = 62/52,
    // kd = Tv*Kp = 11.5*62, P_ON_E, DIRECT.
    let mut controller = Controller::new(
        Millis::ZERO,
        62.0,
        62.0 / 52.0,
        11.5 * 62.0,
        ProportionalOn::Error,
        ControllerDirection::Direct,
    );
    controller.input = 22.5;
    controller.setpoint = 95.0;
    controller.output = 0.0;
    configure_production(
        &mut controller,
        62.0,
        62.0 / 52.0,
        11.5 * 62.0,
        ProportionalOn::Error,
    );

    step(&mut controller, 0, &mut log);
    step(&mut controller, 999, &mut log);
    step(&mut controller, 1000, &mut log);

    let mut now = 3000u32;
    for temp in [30.25, 45.5, 60.75, 75.0, 85.5, 91.25, 94.5, 95.25, 95.0] {
        controller.input = temp;
        step(&mut controller, now, &mut log);
        now += 1000;
    }
    assert_eq!(
        now, 12_000,
        "the warm-up consumed timestamps up to 11_000 ms"
    );

    // Overshoot: output must clamp at outMin.
    controller.input = 108.0;
    step(&mut controller, 14_000, &mut log);
    controller.input = 104.0;
    step(&mut controller, 15_000, &mut log);

    // Saturated high: output into outMax, integrator into its own limit.
    controller.input = 22.0;
    controller.setpoint = 130.0;
    step(&mut controller, 16_000, &mut log);
    step(&mut controller, 17_000, &mut log);
    step(&mut controller, 18_000, &mut log);

    // Recover.
    controller.input = 95.0;
    controller.setpoint = 95.0;
    step(&mut controller, 21_000, &mut log);
    step(&mut controller, 22_000, &mut log);

    // u32 millisecond wrap: 0xFFFFF000 -> 0x00000100 is +0x1100 ms across zero.
    controller.input = 96.5;
    step(&mut controller, 0xFFFF_F000, &mut log);
    step(&mut controller, 0x0000_0100, &mut log);
    step(&mut controller, 0x0000_0200, &mut log);

    assert_scenario("scenario A (production P_ON_E)", SCENARIO_A, &log);
}

#[test]
fn scenario_b_brew_detection_pon_m_matches_the_reference_library() {
    let mut log = Vec::new();

    // Brew-detection tunings: AGGBKP = 50, AGGBTN = 0 so Ki = 0,
    // AGGBTV = 20 so kd = 20*50, P_ON_M.
    let mut controller = Controller::new(
        Millis::ZERO,
        50.0,
        0.0,
        20.0 * 50.0,
        ProportionalOn::Measurement,
        ControllerDirection::Direct,
    );
    controller.input = 93.0;
    controller.setpoint = 95.0;
    controller.output = 0.0;
    configure_production(
        &mut controller,
        50.0,
        0.0,
        20.0 * 50.0,
        ProportionalOn::Measurement,
    );

    let mut now = 0u32;
    for temp in [93.0, 93.5, 93.25, 92.5, 91.0, 88.5, 86.0, 84.25] {
        controller.input = temp;
        step(&mut controller, now, &mut log);
        now += 1000;
    }

    // The proportional mode switches mid-brew, as the firmware's own retune does.
    assert!(controller.set_tunings(50.0, 0.0, 20.0 * 50.0, ProportionalOn::Measurement));
    step(&mut controller, now, &mut log);
    now += 1000;

    assert!(controller.set_tunings(50.0, 0.0, 20.0 * 50.0, ProportionalOn::Error));
    step(&mut controller, now, &mut log);
    now += 1000;
    step(&mut controller, now, &mut log);

    assert_scenario("scenario B (brew detection P_ON_M)", SCENARIO_B, &log);
}

#[test]
fn scenario_c_manual_automatic_and_limits_matches_the_reference_library() {
    let mut log = Vec::new();

    let mut controller = Controller::new(
        Millis::ZERO,
        30.0,
        0.5,
        0.25,
        ProportionalOn::Error,
        ControllerDirection::Direct,
    );
    controller.input = 60.0;
    controller.setpoint = 95.0;
    controller.output = 0.0;
    assert!(controller.set_output_limits(0.0, 1000.0));
    assert!(controller.set_integrator_limits(0.0, 55.0));
    assert!(controller.set_sample_time(Millis::new(1000)));
    controller.set_smoothing_factor(0.0); // EMA 0: no input filtering at all.
    controller.set_mode(Mode::Manual);

    // MANUAL: compute must refuse and leave the output untouched.
    step(&mut controller, 0, &mut log);
    step(&mut controller, 5000, &mut log);

    // Bumpless transfer. The firmware zeroes the output when it disables the PID and
    // re-enables it here.
    controller.output = 0.0;
    controller.set_mode(Mode::Automatic);
    step(&mut controller, 6000, &mut log);
    step(&mut controller, 7000, &mut log);

    // Re-tune and re-window mid-run. `set_output_limits` on a running controller clamps
    // both the existing output and the integrator.
    assert!(controller.set_tunings(45.0, 0.9, 0.75, ProportionalOn::Error));
    assert!(controller.set_output_limits(-250.0, 250.0));
    assert!(controller.set_integrator_limits(-40.0, 40.0));
    step(&mut controller, 8000, &mut log);
    step(&mut controller, 9000, &mut log);
    step(&mut controller, 10_000, &mut log);

    // A negative gain is rejected outright, leaving the controller untouched.
    assert!(!controller.set_tunings(-1.0, 0.5, 0.5, ProportionalOn::Error));
    step(&mut controller, 11_000, &mut log);
    step(&mut controller, 12_000, &mut log);

    assert_scenario("scenario C (manual/automatic, limits)", SCENARIO_C, &log);
}

/// D-PID-1, **closed**: a sub-second window, where the reference divides by zero.
///
/// The reference divides the filtered input difference by `sample_time / 1000` with
/// *integer* division, so a 500 ms window is a division by zero and the output is `NaN`
/// -- and its own `if (x > max) ... else if (x < min)` clamp passes the `NaN` straight
/// through.
///
/// This controller divides by the real elapsed time in `f64`, so the output is finite and
/// the derivative is the physically correct one. `P_ON_M` is unaffected either way, which
/// is why the same input sequence stays finite on both sides of the mode switch.
///
/// This test asserts the **divergence**, and keeps the reference's vector in
/// `SCENARIO_D_ORACLE` so the size of the difference is on the record rather than in a
/// commit message.
#[test]
fn scenario_d_the_reference_goes_nan_and_this_port_does_not() {
    let mut log = Vec::new();

    let mut controller = Controller::new(
        Millis::ZERO,
        30.0,
        0.5,
        0.25,
        ProportionalOn::Error,
        ControllerDirection::Direct,
    );
    controller.input = 60.0;
    controller.setpoint = 95.0;
    controller.output = 0.0;
    assert!(controller.set_output_limits(0.0, 500.0));
    assert!(controller.set_integrator_limits(0.0, 55.0));
    controller.set_smoothing_factor(0.6);
    assert!(controller.set_sample_time(Millis::new(500)));
    controller.set_mode(Mode::Automatic);

    step(&mut controller, 500, &mut log);
    controller.input = 62.0;
    step(&mut controller, 1000, &mut log);
    controller.input = 61.0;
    step(&mut controller, 1500, &mut log);

    assert!(controller.set_tunings(30.0, 0.5, 0.25, ProportionalOn::Measurement));
    step(&mut controller, 2000, &mut log);
    controller.input = 63.0;
    step(&mut controller, 2500, &mut log);

    // The `computed` flags and the timestamps still agree exactly: the divisor changed,
    // not the sampling contract.
    assert_eq!(log.len(), SCENARIO_D_ORACLE.len());
    for (index, want) in SCENARIO_D_ORACLE.iter().enumerate() {
        assert_eq!(
            log[index].computed, want.computed,
            "step {index}: the sample contract must not change"
        );
    }

    // The oracle's recorded output at step 0 is NaN and this port's is not. If that ever
    // stops being true, the fix has been reverted and the frozen evidence is no longer
    // being honoured.
    let oracle_first = f64::from_bits(SCENARIO_D_ORACLE[0].output_bits);
    assert!(oracle_first.is_nan(), "the oracle recorded a NaN here");
    assert!(
        !log[0].output.is_nan(),
        "the port must not be NaN at a 500 ms window: {}",
        log[0].output
    );
    assert!(log[0].output.is_finite());
    assert_ne!(
        log[0].output.to_bits(),
        oracle_first.to_bits(),
        "this port and the reference must not have converged at step 0"
    );

    // Every step is finite and inside the output limits, which the reference can never be
    // at this window.
    for (index, step) in log.iter().enumerate() {
        assert!(step.output.is_finite(), "step {index}: {}", step.output);
        assert!(
            (-0.0..=500.0).contains(&step.output),
            "step {index}: {} is outside the output limits",
            step.output
        );
    }

    // The nominal term is still exposed and is now non-zero — the trap is
    // visible rather than merely gone.
    assert!((controller.derivative_seconds() - 0.5).abs() < f64::EPSILON);
    assert!(controller.derivative_seconds() > 0.0);
}

/// D-PID-1, second half: the *shipped* 1000 ms window, on deliberately ragged timestamps.
///
/// The reference's divisor is the configuration constant `1000 / 1000` == 1 s, whatever the
/// real interval was; this controller's is the real interval. So the two agree **exactly**
/// on a step that lands on the window, and differ by an amount bounded by the lateness on
/// a step that does not.
///
/// Measured, not asserted by inspection: the expected vector is the oracle's output, and
/// the deltas below are `2.0 / elapsed_s` scaled by `kd = 1.0`.
#[test]
fn scenario_e_a_late_step_uses_the_real_interval() {
    // The elapsed time of each step, as the reference sees it. The step at t = 0 does not
    // compute -- the constructor left `last_time = 0 - 100`, the *default* sample time, so
    // only 100 ms have passed -- so the first elapsed value is 1000 - (0 - 100) = 1100 ms.
    // `t(n) - t(n-1)` over the steps that computed, wrapped in u32.
    const ELAPSED_MS: [u32; 5] = [100, 1100, 1500, 1000, 2000];

    let mut log = Vec::new();

    let mut controller = Controller::new(
        Millis::ZERO,
        1.0,
        0.0,
        1.0,
        ProportionalOn::Error,
        ControllerDirection::Direct,
    );
    controller.input = 60.0;
    controller.setpoint = 95.0;
    controller.output = 0.0;
    assert!(controller.set_output_limits(-1000.0, 1000.0));
    assert!(controller.set_integrator_limits(-1000.0, 1000.0));
    assert!(controller.set_sample_time(Millis::new(1000)));
    controller.set_smoothing_factor(0.0); // the filtered difference is raw
    controller.set_mode(Mode::Automatic);

    step(&mut controller, 0, &mut log);
    controller.input = 62.0;
    step(&mut controller, 1000, &mut log);
    controller.input = 64.0;
    step(&mut controller, 2500, &mut log);
    controller.input = 66.0;
    step(&mut controller, 3500, &mut log);
    controller.input = 68.0;
    step(&mut controller, 5500, &mut log);

    assert_eq!(log.len(), SCENARIO_E_ORACLE.len());

    let mut max_delta = 0.0f64;

    for (index, want) in SCENARIO_E_ORACLE.iter().enumerate() {
        let want_value = f64::from_bits(want.output_bits);
        let got = log[index].output;
        assert_eq!(log[index].computed, want.computed, "step {index}");
        if !log[index].computed {
            // t = 0 is inside the constructor's default 100 ms sample period, so neither
            // implementation has a full window and neither produces an output. There is
            // nothing to differentiate.
            assert!(
                got.abs() < f64::EPSILON,
                "step {index} did not compute, so its output must still be 0, got {got}"
            );
            continue;
        }

        // The input steps by exactly 2.0 between samples and `kd = 1.0`, so the
        // reference's derivative term is a constant 2.0: it divides by the *nominal* 1.0 s
        // whatever the real interval was. This controller divides by the real one, so its
        // output is the reference's plus back the 2.0 the reference subtracted, minus the
        // 2.0/interval this one subtracts instead.
        let expected_port = want_value + 2.0 - 2.0 / (f64::from(ELAPSED_MS[index]) / 1000.0);
        assert!(
            (got - expected_port).abs() < 1e-12,
            "step {index} (t = {} ms, elapsed {} ms): port {got} vs hand-derived \
             {expected_port}",
            want.now,
            ELAPSED_MS[index],
        );

        let delta = (got - want_value).abs();
        if delta > max_delta {
            max_delta = delta;
        }

        // A step that lands exactly on the window is bit-identical to the reference.
        if ELAPSED_MS[index] == 1000 {
            assert_eq!(
                got.to_bits(),
                want.output_bits,
                "step {index} lands on the window and must be bit-identical to the reference"
            );
        }
    }

    // The measured spread, asserted so a change in the fix cannot quietly widen it: at
    // 2000 ms of lateness the derivative term differs by 2.0 - 1.0 = 1.0, and `kd = 1.0`,
    // so the output differs by 1.0.
    assert!(
        (max_delta - 1.0).abs() < 1e-12,
        "max |delta| against the reference at the production window is {max_delta}, \
         expected 1.0"
    );
}
