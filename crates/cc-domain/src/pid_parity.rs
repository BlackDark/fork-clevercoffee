//! Parity harness for [`crate::pid::Controller`] — task R2-04.
//!
//! The expected values in the tables below are **generated**, never typed by
//! hand. They come from the real vendored C++ library:
//!
//! ```text
//! crates/cc-domain/tools/pid_oracle/run.sh > crates/cc-domain/tools/pid_oracle/expected.txt
//! ```
//!
//! `run.sh` compiles `lib/Arduino-PID-Library/PID_v1.cpp` — unmodified, byte
//! for byte — against a hand-driven `millis()`, replays the identical input
//! sequence, and prints one line per `Compute()` call. Each expected value is
//! stored here as the IEEE-754 bit pattern of the resulting `double`, so nothing
//! is lost in transcription and the two implementations can be compared either
//! exactly or approximately.
//!
//! The four scenarios mirror the C++ oracle one for one:
//!
//! * **A** — the production `P_ON_E` configuration and a boiler warm-up,
//!   including output saturation in both directions, integrator limiting,
//!   recovery, and a `u32` millisecond wrap across zero.
//! * **B** — the brew-detection `P_ON_M` tuning, including the mid-run
//!   proportional-mode switches the firmware performs.
//! * **C** — `Manual`/`Automatic` bumpless transfer, mid-run re-tuning and
//!   limit changes, and a rejected (negative) tuning.
//! * **D** — upstream defect D-PID-1: a sub-second sample time makes the
//!   `P_ON_E` derivative divisor zero and the output `NaN`. **The port fixes
//!   this**, so `SCENARIO_D_CPP` is the C++'s vector and the test asserts the
//!   divergence. See the [`crate::pid`] module documentation.
//! * **E** — the other half of D-PID-1: the shipped 1000 ms window driven on
//!   deliberately ragged timestamps, which is the only way the port can differ
//!   from the C++ at the production window.
//!
//! [`TOLERANCE`] is the R2-04 acceptance bound of 1e-6. The measured
//! agreement on scenarios A, B and C is *exact* — every one of the 47 steps
//! matches the C++ bit for bit, so the **maximum |delta| is 0.0** — and
//! [`assert_scenario`] additionally requires that, so the tolerance exists only
//! to make a future floating-point contraction difference visible as a failure
//! rather than as a silent drift.

// `alloc` is linked for tests only. The library itself uses no allocation at all
// (04 §1); a growable log of parity results is a test convenience, not a
// runtime requirement, and pulling in `alloc` unconditionally would be a lie
// about the crate's dependencies.
use alloc::vec::Vec;

use super::pid::{Controller, ControllerDirection, Mode, ProportionalOn};
use super::units::Millis;

/// Absolute tolerance for the output comparison. The R2-04 acceptance criterion
/// is 1e-6; the measured difference is 0.0 on every step.
const TOLERANCE: f64 = 1e-6;

/// One expected [`Controller::compute`] result, as printed by the C++ oracle.
#[derive(Clone, Copy, Debug)]
struct Expected {
    /// The virtual clock value the oracle set before calling `Compute()`.
    now: u32,
    /// Whether `Compute()` returned `true`.
    computed: bool,
    /// The IEEE-754 bit pattern of `*myOutput` afterwards.
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
    e(0, false, 0x0000_0000_0000_0000),            // C++ output 0
    e(999, true, 0x408f_4000_0000_0000),           // C++ output 1000
    e(1000, false, 0x408f_4000_0000_0000),         // C++ output 1000
    e(3000, true, 0x408f_4000_0000_0000),          // C++ output 1000
    e(4000, true, 0x0000_0000_0000_0000),          // C++ output 0
    e(5000, true, 0x0000_0000_0000_0000),          // C++ output 0
    e(6000, true, 0x0000_0000_0000_0000),          // C++ output 0
    e(7000, true, 0x0000_0000_0000_0000),          // C++ output 0
    e(8000, true, 0x0000_0000_0000_0000),          // C++ output 0
    e(9000, true, 0x0000_0000_0000_0000),          // C++ output 0
    e(10000, true, 0x0000_0000_0000_0000),         // C++ output 0
    e(11000, true, 0x0000_0000_0000_0000),         // C++ output 0
    e(12000, true, 0x0000_0000_0000_0000),         // C++ output 0
    e(13000, true, 0x0000_0000_0000_0000),         // C++ output 0
    e(14000, true, 0x408f_4000_0000_0000),         // C++ output 1000
    e(15000, true, 0x408f_4000_0000_0000),         // C++ output 1000
    e(16000, true, 0x408f_4000_0000_0000),         // C++ output 1000
    e(17000, true, 0x0000_0000_0000_0000),         // C++ output 0
    e(18000, true, 0x0000_0000_0000_0000),         // C++ output 0
    e(4_294_963_200, true, 0x0000_0000_0000_0000), // C++ output 0
    e(256, true, 0x0000_0000_0000_0000),           // C++ output 0
    e(512, false, 0x0000_0000_0000_0000),          // C++ output 0
];

/// Expected results for oracle scenario `scenario_b_brew_detection_pon_m`.
const SCENARIO_B: &[Expected] = &[
    e(0, false, 0x0000_0000_0000_0000),    // C++ output 0
    e(1000, true, 0x0000_0000_0000_0000),  // C++ output 0
    e(2000, true, 0x4070_6800_0000_0000),  // C++ output 262.5
    e(3000, true, 0x4089_0000_0000_0000),  // C++ output 800
    e(4000, true, 0x408f_4000_0000_0000),  // C++ output 1000
    e(5000, true, 0x408f_4000_0000_0000),  // C++ output 1000
    e(6000, true, 0x408f_4000_0000_0000),  // C++ output 1000
    e(7000, true, 0x408f_4000_0000_0000),  // C++ output 1000
    e(8000, true, 0x0000_0000_0000_0000),  // C++ output 0
    e(9000, true, 0x408f_4000_0000_0000),  // C++ output 1000
    e(10000, true, 0x408d_20ed_11e6_b216), // C++ output 932.11575679999328
];

/// Expected results for oracle scenario `scenario_c_manual_automatic_and_limits`.
const SCENARIO_C: &[Expected] = &[
    e(0, false, 0x0000_0000_0000_0000),    // C++ output 0
    e(5000, false, 0x0000_0000_0000_0000), // C++ output 0
    e(6000, true, 0x408f_4000_0000_0000),  // C++ output 1000
    e(7000, true, 0x408f_4000_0000_0000),  // C++ output 1000
    e(8000, true, 0x406f_4000_0000_0000),  // C++ output 250
    e(9000, true, 0x406f_4000_0000_0000),  // C++ output 250
    e(10000, true, 0x406f_4000_0000_0000), // C++ output 250
    e(11000, true, 0x406f_4000_0000_0000), // C++ output 250
    e(12000, true, 0x406f_4000_0000_0000), // C++ output 250
];

/// Expected results for oracle scenario `scenario_d_sub_second_sample_time_is_nan`.
///
/// **These are the C++'s outputs and the port deliberately does not reproduce
/// them.** The table is kept, unmodified, as the evidence for
/// `intentional-diffs.md` line 4; `scenario_d_the_cpp_goes_nan_and_this_port_does_not`
/// asserts the divergence rather than the agreement. Deleting this table would
/// delete the only record of what the firmware on the bench actually does at a
/// sub-second window.
const SCENARIO_D_CPP: &[Expected] = &[
    e(500, true, 0x7ff8_0000_0000_0000),  // C++ output nan
    e(1000, true, 0x0000_0000_0000_0000), // C++ output 0
    e(1500, true, 0x0000_0000_0000_0000), // C++ output 0
    e(2000, true, 0x4021_0000_0000_0000), // C++ output 8.5
    e(2500, true, 0x0000_0000_0000_0000), // C++ output 0
];

/// Expected results for oracle scenario
/// `scenario_e_late_steps_use_the_nominal_window` — again, the **C++'s**
/// outputs.
const SCENARIO_E_CPP: &[Expected] = &[
    e(0, false, 0x0000_0000_0000_0000),   // C++ output 0
    e(1000, true, 0x403f_0000_0000_0000), // C++ output 31
    e(2500, true, 0x403d_0000_0000_0000), // C++ output 29
    e(3500, true, 0x403b_0000_0000_0000), // C++ output 27
    e(5500, true, 0x4039_0000_0000_0000), // C++ output 25
];

/// The `SystemInitializer::initializePID()` call sequence, in the C++ order
/// (`src/core/SystemInitializer.cpp:544-556`).
///
/// Order is load-bearing: `set_sample_time` rescales `ki` and `kd`, so it must
/// follow the tuning, and `set_mode(Automatic)` performs the bumpless transfer
/// so it must follow everything that can move the integrator.
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
        "{name}: the replay visited a different number of Compute() calls than the oracle"
    );

    for (index, (want, got)) in expected.iter().zip(observed.iter()).enumerate() {
        assert_eq!(
            got.computed, want.computed,
            "{name} step {index} (t = {} ms): Compute() disagreed",
            want.now
        );
        let want_value = f64::from_bits(want.output_bits);
        // NaN on both sides is agreement, not a failure. Scenario D depends on
        // it: the C++ produces NaN and so must this port.
        if want_value.is_nan() {
            assert!(
                got.output.is_nan(),
                "{name} step {index} (t = {} ms): C++ produced NaN, this port produced {}",
                want.now,
                got.output
            );
            continue;
        }
        let difference = (got.output - want_value).abs();
        assert!(
            difference <= TOLERANCE,
            "{name} step {index} (t = {} ms): output {} vs C++ {want_value} (difference {difference})",
            want.now,
            got.output
        );
        assert_eq!(
            got.output.to_bits(),
            want.output_bits,
            "{name} step {index} (t = {} ms): not bit-identical to the C++",
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
fn scenario_a_production_pon_e_matches_the_cpp_library() {
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
fn scenario_b_brew_detection_pon_m_matches_the_cpp_library() {
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

    // ProcessController.cpp:214 switches proportional mode mid-brew.
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
fn scenario_c_manual_automatic_and_limits_matches_the_cpp_library() {
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

    // Bumpless transfer. The firmware zeroes the output when it disables the
    // PID (ProcessController.cpp:275-277) and re-enables it here.
    controller.output = 0.0;
    controller.set_mode(Mode::Automatic);
    step(&mut controller, 6000, &mut log);
    step(&mut controller, 7000, &mut log);

    // Re-tune and re-window mid-run. `set_output_limits` while running clamps
    // the existing output, exactly as PID_v1.cpp:207-216 does.
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

/// D-PID-1, **closed**. The C++ divides the filtered input difference by
/// `SampleTime / 1000` with *integer* division, so a 500 ms window is a divide
/// by zero and the output is `NaN` — and the C++'s own `if (x > max) … else if
/// (x < min)` clamp passes `NaN` straight through.
///
/// The port divides by the real elapsed time in `f64`, so the output is finite
/// and the derivative is the physically correct one. `P_ON_M` is unaffected in
/// both, which is why the same input sequence stays finite after the mode switch
/// in both.
///
/// This test asserts the **divergence**, and keeps the C++'s vector in
/// `SCENARIO_D_CPP` so the size of the difference is on the record rather than in
/// a commit message.
#[test]
fn scenario_d_the_cpp_goes_nan_and_this_port_does_not() {
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

    // The `computed` flags and the timestamps still agree exactly: the port
    // changed the divisor, not the sampling contract.
    assert_eq!(log.len(), SCENARIO_D_CPP.len());
    for (index, want) in SCENARIO_D_CPP.iter().enumerate() {
        assert_eq!(
            log[index].computed, want.computed,
            "step {index}: the sample contract must not change"
        );
    }

    // The C++'s recorded output at step 0 is NaN and the port's is not. If this
    // ever becomes true, the fix has been reverted and the oracle's evidence is
    // no longer being honoured.
    let cpp_first = f64::from_bits(SCENARIO_D_CPP[0].output_bits);
    assert!(cpp_first.is_nan(), "the C++ oracle recorded a NaN here");
    assert!(
        !log[0].output.is_nan(),
        "the port must not be NaN at a 500 ms window: {}",
        log[0].output
    );
    assert!(log[0].output.is_finite());
    assert_ne!(
        log[0].output.to_bits(),
        cpp_first.to_bits(),
        "the port and the C++ must not have converged at step 0"
    );

    // Every step is finite and inside the output limits, which the C++ can never
    // be at this window.
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

/// D-PID-1, second half: the *shipped* 1000 ms window, on ragged timestamps.
///
/// The C++'s divisor is the configuration constant `1000 / 1000` == 1 s,
/// whatever the real interval was. The port's is the real interval. So the two
/// agree **exactly** on a step that lands on the window, and differ by an amount
/// bounded by the lateness on a step that does not.
///
/// Measured, not asserted by inspection: the expected vector is the C++ oracle's
/// output, and the deltas below are `2.0 / elapsed_s` scaled by `kd = 1.0`.
#[test]
fn scenario_e_a_late_step_uses_the_real_interval() {
    // The elapsed time of each step, as the C++ sees it. `Compute()` at t = 0 does
    // not run — the constructor left `lastTime = millis() - 100`, the *default*
    // sample time, so only 100 ms have passed — so the first elapsed value is
    // 1000 - (0 - 100) = 1100 ms. `t(n) - t(n-1)` over the steps that computed,
    // wrapped in u32 exactly as `unsigned long` is on the chip.
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

    assert_eq!(log.len(), SCENARIO_E_CPP.len());

    let mut max_delta = 0.0f64;

    for (index, want) in SCENARIO_E_CPP.iter().enumerate() {
        let want_value = f64::from_bits(want.output_bits);
        let got = log[index].output;
        assert_eq!(log[index].computed, want.computed, "step {index}");
        if !log[index].computed {
            // t = 0 is past the constructor's `lastTime = millis() - 100`, so
            // neither implementation has a full window and neither produces an
            // output. There is nothing to differentiate.
            assert!(
                got.abs() < f64::EPSILON,
                "step {index} did not compute, so its output must still be 0, got {got}"
            );
            continue;
        }

        // The input steps by exactly 2.0 between samples and `kd = 1.0`, so the
        // C++'s dInput is a constant 2.0 — it divides by the *nominal* 1.0 s
        // whatever the real interval was. The port divides by the real one, so
        // the port's output is the C++'s plus back the 2.0 the C++ subtracted,
        // minus the 2.0/interval the port subtracts instead.
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

        // A step that lands exactly on the window is bit-identical to the C++.
        if ELAPSED_MS[index] == 1000 {
            assert_eq!(
                got.to_bits(),
                want.output_bits,
                "step {index} lands on the window and must be bit-identical to the C++"
            );
        }
    }

    // The measured spread, asserted so a change in the fix cannot quietly widen
    // it: at 2000 ms of lateness the D term differs by 2.0 - 1.0 = 1.0, and
    // `kd = 1.0`, so the output differs by 1.0.
    assert!(
        (max_delta - 1.0).abs() < 1e-12,
        "max |delta| against the C++ at the production window is {max_delta}, expected 1.0"
    );
}
