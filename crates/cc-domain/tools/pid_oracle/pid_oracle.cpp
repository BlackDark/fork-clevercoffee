/*
 * PID parity oracle — R2-04.
 *
 * Compiles the REAL vendored Arduino-PID-Library (`lib/Arduino-PID-Library/
 * PID_v1.cpp`, unmodified) against a hand-driven `millis()` and replays a fixed
 * input sequence through it. The Rust `cc-domain::pid::Controller` replays the
 * *same* sequence and asserts the same outputs.
 *
 * This is the ground truth. The expected values checked into
 * `crates/cc-domain/src/pid_parity.rs` are produced by THIS program, never by
 * hand.
 *
 * Build & run:  crates/cc-domain/tools/pid_oracle/run.sh
 *
 * Output lines, one per PID::Compute() call:
 *
 *   C <index> <now_ms> <computed:0|1> <output %.17g> <output_bits 0x%016llx>
 *
 * The `output_bits` column is the IEEE-754 bit pattern of the `double` output,
 * so the two implementations can be compared exactly as well as approximately.
 * Non-Compute operations are echoed as `O <label>` for readability and are not
 * part of the asserted table.
 *
 * NOTE — two upstream defects are deliberately reproduced here and in the Rust
 * port, so that parity is exact. See the module docs of
 * `crates/cc-domain/src/pid.rs`:
 *
 *   D-PID-1  `dInput = (lastFilteredInput - oldFiltered) / (SampleTime / 1000)`
 *            (PID_v1.cpp:85) uses INTEGER division for the divisor. Any
 *            `SampleTime < 1000` makes it 0 and the whole output becomes NaN.
 *            Scenario D pins this.
 *   D-PID-2  The constructor leaves `integrator`, `lastInput`, `lastFilteredInput`,
 *            `lastFilteredDifferential`, `lastError`, `lastPPart` and `lastDPart`
 *            uninitialised; correctness depends on `SetMode(AUTOMATIC)` calling
 *            `Initialize()` first.
 */

#include <cinttypes>
#include <cstdint>
#include <cstdio>
#include <cstring>

/* PID_v1.cpp:8 does `#if ARDUINO >= 100` and falls back to <WProgram.h> when the
 * macro is undefined. Pretend to be a modern core so it picks <Arduino.h>. */
#define ARDUINO 10819

#include "Arduino.h"

/* The vendored library, byte for byte. */
#include "PID_v1.cpp"

/* ------------------------------------------------------------------ harness */

/*
 * The C++ PID keeps `*myOutput` as a POINTER to a caller-owned double and
 * Compute() READS it for its anti-windup test (PID_v1.cpp:70), as well as
 * writing it. The oracle therefore owns the three variables — exactly as
 * `SystemContext::processTemperaturePtr() / processPidOutputPtr() /
 * processSetpointPtr()` do in the firmware — and hands their addresses to the
 * constructor. The Rust port keeps the same three values INSIDE the controller,
 * which is the same aliasing without the dangling-pointer risk (08 §6).
 */
static double g_input    = 0.0;
static double g_setpoint = 0.0;
static double g_output   = 0.0;

static int g_step = 0;

static uint64_t bits_of(double v) {
    uint64_t out = 0;
    std::memcpy(&out, &v, sizeof(out));
    return out;
}

/* Advance the virtual clock, call Compute(), and report. */
static void compute(PID& pid, uint32_t now) {
    g_oracle_millis = now;
    const bool      did = pid.Compute();
    std::printf(
        "C %d %" PRIu32 " %d %.17g 0x%016" PRIx64 "\n",
        g_step++,
        now,
        did ? 1 : 0,
        g_output,
        bits_of(g_output));
    (void)did;
}

/*
 * The exact call sequence of `SystemInitializer::initializePID()`
 * (src/core/SystemInitializer.cpp:544-556), in the same order. Order matters:
 * `SetSampleTime` rescales ki/kd, so it has to come after the tuning.
 */
static void configure_production(PID& pid, double kp, double ki, double kd, int p_on) {
    pid.SetTunings(kp, ki, kd, p_on);
    pid.SetSampleTime(1000); /* processWindowSize(), ProcessState.h:183 */
    pid.SetOutputLimits(0, 1000);
    pid.SetIntegratorLimits(0, 55.0); /* AGGIMAX, defaults.h:27 */
    pid.SetSmoothingFactor(0.6);      /* pidEmaFactor, defaults.h:32 */
    pid.SetMode(AUTOMATIC);
}

/* ---------------------------------------------------------------- scenarios */

static void scenario_a_production_pon_e() {
    std::printf("O scenario_a_production_pon_e\n");
    g_oracle_millis = 0; /* the C++ constructor reads millis() */

    /* The production configuration:
     *   kp = AGGKP = 62.0, Ki = kp/Tn = 62.0/52.0, kd = Tv*kp = 11.5*62.0
     *   (ProcessController::calculatePIDParameters(), ProcessController.cpp:382-390)
     *   POn = 1 (P_ON_E), DIRECT — SystemInitializer.cpp:299-304
     */
    g_input    = 22.5;
    g_setpoint = 95.0;
    g_output   = 0.0;
    PID pid(&g_input, &g_output, &g_setpoint, 62.0, 62.0 / 52.0, 11.5 * 62.0, 1, DIRECT);
    configure_production(pid, 62.0, 62.0 / 52.0, 11.5 * 62.0, 1);

    /* The C++ constructor sets lastTime = millis() - SampleTime, so the very
     * first Compute() at t = 0 already sees a full sample period elapse. */
    compute(pid, 0);

    /* Too soon — must not compute. */
    compute(pid, 999);

    /* Exactly one sample period. */
    compute(pid, 1000);

    /* A cold boiler climbing towards setpoint. */
    const double temps[] = {30.25, 45.5, 60.75, 75.0, 85.5, 91.25, 94.5, 95.25, 95.0};
    uint32_t    t       = 2000;
    for (double deg : temps) {
        g_input = deg;
        t += 1000;
        compute(pid, t);
    }

    /* Overshoot: input jumps above setpoint, output must clamp at outMin. */
    g_input = 108.0;
    t += 1000;
    compute(pid, t);
    g_input = 104.0;
    t += 1000;
    compute(pid, t);

    /* Saturated-high case: setpoint far above input drives the output into
     * outMax and the integrator into its own limit. */
    g_input    = 22.0;
    g_setpoint = 130.0;
    t += 1000;
    compute(pid, t);
    compute(pid, t + 1000);
    compute(pid, t + 2000);

    /* Recover, and let the integrator come back off its limit. */
    g_input    = 95.0;
    g_setpoint = 95.0;
    t += 3000;
    compute(pid, t);
    t += 1000;
    compute(pid, t);

    /* u32 wrap: 0xFFFFF000 -> 0x00000100 is +0x1100 ms across the wrap. */
    g_input = 96.5;
    compute(pid, 0xFFFFF000u);
    compute(pid, 0x00000100u);
    compute(pid, 0x00000200u);
}

static void scenario_b_brew_detection_pon_m() {
    std::printf("O scenario_b_brew_detection_pon_m\n");
    g_oracle_millis = 0;

    /* Brew-detection tunings, ProcessController::calculateBrewDetectionPIDParameters()
     * with AGGBKP = 50, AGGBTN = 0 (so aggbKi_ = 0), AGGBTV = 20, P_ON_M. */
    g_input    = 93.0;
    g_setpoint = 95.0;
    g_output   = 0.0;
    PID pid(&g_input, &g_output, &g_setpoint, 50.0, 0.0, 20.0 * 50.0, P_ON_M, DIRECT);
    configure_production(pid, 50.0, 0.0, 20.0 * 50.0, P_ON_M);

    const double temps[] = {93.0, 93.5, 93.25, 92.5, 91.0, 88.5, 86.0, 84.25};
    uint32_t    t       = 0;
    for (double deg : temps) {
        g_input = deg;
        compute(pid, t);
        t += 1000;
    }

    /* The firmware switches to P_ON_M mid-run (ProcessController.cpp:214) and
     * back again, passing the proportional mode explicitly each time. */
    pid.SetTunings(50.0, 0.0, 20.0 * 50.0, P_ON_M);
    compute(pid, t);
    t += 1000;

    pid.SetTunings(50.0, 0.0, 20.0 * 50.0, 1);
    compute(pid, t);
    t += 1000;
    compute(pid, t);
}

static void scenario_c_manual_automatic_and_limits() {
    std::printf("O scenario_c_manual_automatic_and_limits\n");
    g_oracle_millis = 0;

    g_input    = 60.0;
    g_setpoint = 95.0;
    g_output   = 0.0;
    PID pid(&g_input, &g_output, &g_setpoint, 30.0, 0.5, 0.25, P_ON_E, DIRECT);
    pid.SetOutputLimits(0, 1000);
    pid.SetIntegratorLimits(0, 55.0);
    pid.SetSampleTime(1000);
    pid.SetSmoothingFactor(0.0); /* EMA 0: no input filtering at all. */
    pid.SetMode(MANUAL);

    /* MANUAL: Compute() must refuse, and the output must be untouched. */
    compute(pid, 0);
    compute(pid, 5000);

    /* Bumpless transfer. The firmware zeroes the output when it switches to
     * MANUAL (ProcessController.cpp:275-277) and then re-enables AUTOMATIC. */
    g_output = 0.0;
    pid.SetMode(AUTOMATIC);
    compute(pid, 6000);
    compute(pid, 7000);

    /* Re-tune and re-window mid-run. */
    pid.SetTunings(45.0, 0.9, 0.75, P_ON_E);
    pid.SetOutputLimits(-250.0, 250.0);
    pid.SetIntegratorLimits(-40.0, 40.0);
    compute(pid, 8000);
    compute(pid, 9000);
    compute(pid, 10000);

    /* A rejected tuning (negative gain) must be ignored entirely
     * (PID_v1.cpp:146). */
    pid.SetTunings(-1.0, 0.5, 0.5, P_ON_E);
    compute(pid, 11000);
    compute(pid, 12000);
}

/*
 * D-PID-1. `SampleTime < 1000` makes `SampleTime / 1000` zero at
 * PID_v1.cpp:85, so the P_ON_E derivative term is `x / 0` and the output is
 * NaN. P_ON_M is unaffected because it uses the unfiltered
 * `input - lastInput` difference with no time division.
 *
 * The shipped firmware never hits this because initializePID() sets the sample
 * time to `processWindowSize()` = 1000 ms, but it is one `SetSampleTime(500)`
 * away, and R1-07 (heater output method) is exactly the task that would
 * introduce a different window.
 *
 * NOTE: the Rust port FIXES this. `Controller::derivative_seconds_at` divides
 * by the real elapsed time in f64, so the port does not reproduce the NaN. This
 * scenario is retained verbatim as the record of what the C++ does, and
 * `crates/cc-domain/src/pid_parity.rs` measures the divergence against it. Do
 * not "fix" the oracle to agree with the port — that would destroy the only
 * evidence for intentional-diffs.md line 4.
 */
static void scenario_d_sub_second_sample_time_is_nan() {
    std::printf("O scenario_d_sub_second_sample_time_is_nan\n");
    g_oracle_millis = 0;

    g_input    = 60.0;
    g_setpoint = 95.0;
    g_output   = 0.0;
    PID pid(&g_input, &g_output, &g_setpoint, 30.0, 0.5, 0.25, P_ON_E, DIRECT);
    pid.SetOutputLimits(0, 500);
    pid.SetIntegratorLimits(0, 55.0);
    pid.SetSmoothingFactor(0.6);
    pid.SetSampleTime(500);
    pid.SetMode(AUTOMATIC);

    compute(pid, 500);
    g_input = 62.0;
    compute(pid, 1000);
    g_input = 61.0;
    compute(pid, 1500);

    /* The same input sequence with P_ON_M stays finite. */
    pid.SetTunings(30.0, 0.5, 0.25, P_ON_M);
    compute(pid, 2000);
    g_input = 63.0;
    compute(pid, 2500);
}

/*
 * D-PID-1, second half: at the *shipped* 1000 ms window the C++'s divisor is
 * `1000 / 1000` == 1 s, a constant, whatever the real interval between Compute()
 * calls was. This scenario calls Compute() on deliberately ragged timestamps so
 * the recorded vector shows that: the input steps by exactly 2.0 between samples
 * while the C++ reports a dInput that reflects a fixed 1.0 s.
 *
 * The port divides by the elapsed time, so it reports 2.0 / elapsed there. This
 * is the one place the port can differ from the C++ at the production window,
 * and it is quantified rather than asserted: `pid_parity.rs` replays this
 * sequence and reports the maximum |delta|.
 */
static void scenario_e_late_steps_use_the_nominal_window() {
    std::printf("O scenario_e_late_steps_use_the_nominal_window\n");
    g_oracle_millis = 0;

    g_input    = 60.0;
    g_setpoint = 95.0;
    g_output   = 0.0;
    PID pid(&g_input, &g_output, &g_setpoint, 1.0, 0.0, 1.0, P_ON_E, DIRECT);
    pid.SetOutputLimits(-1000.0, 1000.0);
    pid.SetIntegratorLimits(-1000.0, 1000.0);
    pid.SetSampleTime(1000);
    /* EMA 0: the filtered difference is then exactly the input difference, so
     * the printed dInput is directly readable. */
    pid.SetSmoothingFactor(0.0);
    pid.SetMode(AUTOMATIC);

    /* The constructor set lastTime = millis() - 100 (the *default* sample
     * time), so the first Compute() at t = 0 already sees a full 1000 ms
     * elapse. */
    compute(pid, 0);

    g_input = 62.0;
    compute(pid, 1000); /* exactly on the window */
    g_input = 64.0;
    compute(pid, 2500); /* 1500 ms late */
    g_input = 66.0;
    compute(pid, 3500); /* exactly on the window again */
    g_input = 68.0;
    compute(pid, 5500); /* 2000 ms late */
}

int main() {
    scenario_a_production_pon_e();
    scenario_b_brew_detection_pon_m();
    scenario_c_manual_automatic_and_limits();
    scenario_d_sub_second_sample_time_is_nan();
    scenario_e_late_steps_use_the_nominal_window();
    return 0;
}
