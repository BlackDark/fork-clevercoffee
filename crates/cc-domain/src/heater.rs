//! The heater output: the 1 Hz / 100-step chopper arithmetic, and the latching
//! gate that stands in front of it.
//!
//! # Why this is in `cc-domain` and not in `cc-hal-esp32`
//!
//! R1-07 decides *how* the heater is driven — LEDC hardware PWM, with a
//! `GPTimer` ISR as the fallback ([04 §5](../../docs/rust-migration/04-target-architecture.md#5-heater-output--the-one-hard-real-time-path)).
//! Everything *about* the decision is portable: what duty a PID output means,
//! how a millisecond duty becomes a fraction, and whether the output may be
//! non-zero at all. Those are arithmetic and policy, they must be unit-tested on
//! the host, and putting them behind an `esp_idf_hal` type would make them
//! untestable without a board. `cc-hal-esp32::heater` owns only the pin.
//!
//! # The parity baseline: the C++ chopper
//!
//! The C++ chops, in a hardware timer ISR every 10 ms
//! (`include/clevercoffee/isr.h:85-118`):
//!
//! ```cpp
//! if (currentPidOutput <= currentCounter) {
//!     relay->off();
//! } else {
//!     relay->on();
//! }
//! unsigned int newCounter = currentCounter + ISR_COUNTER_INCREMENT; // 10
//! if (newCounter >= ctx->processWindowSize()) newCounter = 0;        // 1000
//! ```
//!
//! with `windowSize_ = 1000` ms (`context/ProcessState.h:183`) and
//! `ISR_COUNTER_INCREMENT = 10` (`constants/Timing.h:17`). The PID output is
//! bounded by exactly the window — `setPidOutputLimits(0, processWindowSize())`
//! (`src/core/SystemInitializer.cpp:552`) — so **the PID output is a count of
//! milliseconds of energisation per second**.
//!
//! # What R1-07 changed, and what it did not
//!
//! **Unchanged: the window, and therefore the PID.** The controller still
//! computes once per second against a 1000 ms window. Moving the *output* method
//! must not move the control law, or every tuning in
//! `include/clevercoffee/defaults.h` would be wrong.
//!
//! **Changed: how the on-time is delivered.** A 10 ms ISR on the highest
//! priority on the chip, that does a GPIO write and a counter add, is replaced
//! by a hardware carrier with **one period per control window — 1 Hz** — whose
//! duty *is* the on-time fraction. The delivered power is identical —
//! [`on_fraction`] is the C++ chopper's on-time fraction, computed from the same
//! 10 ms quantisation — and the CPU involvement goes from 100 interrupts per
//! second to zero.
//!
//! # The carrier must be LOW: deriving it from the C++
//!
//! The C++ ISR runs 100 times a second, and it is tempting to read that as
//! "100 Hz". It is not. The ISR *fires* 100 times a second and re-asserts the
//! same relay level almost every time; the **level** changes about **twice** a
//! second. Walking `isr.h:96-118` for a constant `pidOutput` over one 1000 ms
//! window (counters `0, 10, …, 990`, 100 of them, then wrap):
//!
//! | `pid_output` | ISR entries | on-ticks | on-time | relay level over the window | level changes/s |
//! | --- | --- | --- | --- | --- | --- |
//! | 0 | 100 | 0 | 0.0 % | OFF throughout | **0** |
//! | 50 | 100 | 5 | 5 % | ON 50 ms, then OFF 950 ms | **2** |
//! | 500 | 100 | 50 | 50 % | ON 500 ms, then OFF 500 ms | **2** |
//! | 950 | 100 | 95 | 95 % | ON 950 ms, then OFF 50 ms | **2** |
//! | 1000 | 100 | 100 | 100 % | ON throughout | **0** |
//!
//! Two edges per second, not a hundred: one falling edge inside the window where
//! the relay is switched off, and one rising edge at the wrap where it is
//! switched back on. The gate is monotone — `pidOutput > counter` — so a
//! constant duty gives **at most one** falling edge per window, whatever the
//! duty. [`cpp_transitions_per_second`] computes that from the same predicate
//! [`chopper_tick_level`] transcribes, and the test
//! `the_carrier_does_not_switch_the_contactor_more_than_the_cpp_does` checks the
//! carrier against it.
//!
//! So the target is:
//!
//! * **transitions ≤ 2 per second** — an `f` Hz square wave makes `2f`, so
//!   `f ≤ 1 Hz`. [`CARRIER_HZ`] is 1, and it equals `1 / WINDOW_MS` rather than a
//!   round number, because one period per control window is the *only* frequency
//!   at which the duty can mean the same thing it meant in the C++.
//! * **duty resolution at least as fine as the C++'s 10 ms steps.** One step of
//!   the C++ chopper is `CHOPPER_STEP_MS / WINDOW_MS` = 1 % of the window. A
//!   period of 1 s therefore needs `max_duty ≥ 100` to express all 101 levels;
//!   [`CHOSEN_RESOLUTION_BITS`] gives 131 072.
//!
//! **A high carrier would be a hardware defect, not a fidelity gain.** At 100 Hz
//! the same pin makes 200 level changes per second — a hundred times the C++'s
//! mechanical duty, on a 2 kW boiler contactor whose life is counted in
//! operations. The delivered *power* would match to within a rounding error and
//! the *wear* would be two orders of magnitude worse, which is precisely why the
//! frequency has to be derived from the C++'s edge count rather than from its ISR
//! count.
//!
//! # ⚠ Still unverified: the contactor itself
//!
//! Matching the C++'s transition rate is necessary and **not sufficient**. Nobody
//! has measured, and this repository does not guess:
//!
//! * **The contactor's minimum on-time and minimum off-time.** The C++ never
//!   pulses the heater narrower than 10 ms, and neither does this — the smallest
//!   duty [`duty_counts`] can be asked for is one whole 10 ms chopper step (see
//!   `the_minimum_pulse_is_the_cpp_ten_millisecond_step`). Whether 10 ms is
//!   *itself* long enough for the contactor is a datasheet question.
//! * **Whether a hardware-PWM output is acceptable to the coil at all.** LEDC
//!   drives a square wave into the same relay pin the C++ drove. A contactor coil
//!   is an inductive load; whether the driver's rise/fall and any freewheeling
//!   path tolerate a 1 Hz square wave is exactly as unknown as it was for the
//!   C++'s 10 ms one, but the *frequency* is now a new variable.
//! * **The realised frequency and duty on the pin.** Everything above is
//!   arithmetic. `ledc_timer_config`'s divider selection and the duty register
//!   have not been read back from hardware.
//!
//! Until a person with a scope, a dummy load and the boiler **disconnected**
//! measures it, R1-07 stays open. See `docs/rust-migration/intentional-diffs.md`
//! #5.

use crate::units::{Duty, Millis};

/// The chopper window, in milliseconds.
///
/// `ProcessState::windowSize_ = 1000` (`include/clevercoffee/context/ProcessState.h:183`),
/// which is also what `setPidSampleTime` and `setPidOutputLimits` are given
/// (`src/core/SystemInitializer.cpp:551-552`). **1 Hz.**
pub const WINDOW_MS: u32 = 1000;

/// How much the ISR counter advances per 10 ms tick.
///
/// `Timing::ISR_COUNTER_INCREMENT = 10` with a 10 ms timer
/// (`constants/Timing.h:17`, `isr.h:87-88`).
pub const CHOPPER_STEP_MS: u32 = 10;

/// How many steps a window has: 1000 / 10.
///
/// This is also the chopper's duty resolution — one step is 1 % of the window.
pub const CHOPPER_STEPS: u32 = WINDOW_MS / CHOPPER_STEP_MS;

/// The PWM carrier's frequency, in hertz: **one period per control window**.
///
/// This is the number the hardware's contactor duty is derived from, and it is
/// the correction to the R1-07 brief's "100 Hz". 100 Hz is the *ISR* rate; the
/// carrier rate that reproduces the C++'s mechanical duty is 100 times lower.
/// The module docs carry the derivation. Expressed as `1000 / WINDOW_MS` rather
/// than as a literal so that the "one period per window" relationship is the
/// thing the code states, and so that a change to [`WINDOW_MS`] — which the C++
/// would allow via `setWindowSize` — cannot silently leave the carrier behind.
pub const CARRIER_HZ: u32 = 1_000 / WINDOW_MS;

/// The timer resolution R1-07 chose, in bits.
///
/// Not a free parameter: on the original ESP32 the LEDC divider is
/// `div_param = (src_clk << 8) / (freq_hz * 2^bits)` and ESP-IDF rejects
/// `div_param <= LEDC_LL_FRACTIONAL_MAX` (255, `hal/esp32/include/hal/ledc_ll.h:28`)
/// or `> LEDC_TIMER_DIV_NUM_MAX` = `0x3FFFF` (`esp_driver_ledc/src/ledc.c:115,111`).
/// At 1 Hz from the 80 MHz APB clock that admits **17, 18, 19 and 20 bits and
/// nothing coarser** — 16 bits already asks for a `div_param` of 312 500, which
/// is above the maximum. Seventeen is the coarsest that works, and the coarsest
/// that works is the right way round: it leaves the most margin against the
/// divider arithmetic being wrong, and 19 and 20 are the two that lose exact
/// frequency to rounding (0.999 987 Hz and 1.000 013 Hz respectively).
pub const CHOSEN_RESOLUTION_BITS: usize = 17;

/// `max_duty` at [`CHOSEN_RESOLUTION_BITS`], i.e. the count that means 100 %.
///
/// `2^17 = 131 072`, **not** `2^17 - 1`. The `2^N - 1` clamp that
/// `esp-idf-hal`'s `Resolution::max_duty` applies is for the timer's *maximum*
/// resolution (20-bit on this part), and ESP-IDF's own comment in
/// `ledc_channel_config` says outright that on the ESP32 "100 % duty cycle
/// (i.e. `2**duty_res`) is not reachable when the binded timer selects the
/// maximum duty resolution". Seventeen is not twenty, so a duty of exactly
/// `max_duty` is a *steady high level* and is distinguishable from duty 0, which
/// is a *steady low level*. `Resolution::Bits20` would break exactly that, and
/// the compile-time assert in `cc-hal-esp32::heater` is what keeps it out.
pub const CHOSEN_MAX_DUTY: u32 = 1 << CHOSEN_RESOLUTION_BITS;

// The two are the same statement about the same timer, and the domain crate's
// host tests are parameterised on the second one. Tying them here means they
// cannot drift apart.
const _: () = assert!(CHOSEN_MAX_DUTY == 1_u32 << CHOSEN_RESOLUTION_BITS);

/// The on-time error [`duty_counts`] is required to stay within, in milliseconds.
///
/// Stated in time rather than as a percentage because that is the quantity the
/// hardware has to hit. 0.01 ms is 1/1000 of the C++ chopper's own 10 ms
/// quantisation step, i.e. a full three orders of magnitude inside the 1 %
/// acceptance bound R1-07 was written against — and it is still 1.3 counts at
/// [`CHOSEN_MAX_DUTY`], so it is a real bound rather than a tautology.
pub const REPRODUCTION_TOLERANCE_MS: f64 = 0.01;

/// The C++ ISR's decision for one tick, transcribed exactly.
///
/// `isr.h:96-102`: the relay is **off** when `pidOutput <= counter`, and on
/// otherwise. `counter` is one of `0, 10, …, 990`.
///
/// Kept as the reference definition even though the LEDC path never calls it per
/// tick: it is the specification [`chopper_on_ticks`] is tested against, table
/// by table, and it is the only place the C++'s off-by-one (a duty of exactly
/// 10 ms is *one* tick, not two) can be seen.
// Justification: `counter_ms` is a chopper counter, so it is at most
// `WINDOW_MS` (1000) and every value in `0..=1000` is exactly representable in
// an `f32` — the cast is lossless, and the C++ makes the same widening.
#[allow(clippy::cast_precision_loss)]
// Justification: as above — the counter never exceeds 1000, which `f32` holds
// exactly.
#[allow(clippy::cast_sign_loss)]
#[must_use]
pub fn chopper_tick_level(pid_output: Duty, counter_ms: u32) -> bool {
    // `isr.h:96-102` turns the relay *off* when `currentPidOutput <=
    // currentCounter`, so the level is the negation of `<=`.
    //
    // Spelled with `partial_cmp` on purpose rather than as `a > b`: a `NaN`
    // compares `<=` **false** in both C++ and Rust, so the C++ would energise
    // the relay, and `!(a <= b)` reproduces that while `a > b` would silently
    // differ. `clippy::neg_cmp_op_on_partial_ord` is denied workspace-wide, and
    // here it is right: this is a transcription of a C++ comparison, not a
    // numeric range check.
    let counter = counter_ms as f32;
    !matches!(
        pid_output.raw().partial_cmp(&counter),
        Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
    )
}

/// How many 10 ms ticks of a window the C++ chopper would energise.
///
/// This is the aggregate the LEDC carrier has to reproduce: `on_fraction` below
/// is `ticks / 100`, which is exactly the fraction of the second the C++ leaves
/// the relay closed... energised.
///
/// The two edges worth stating, because they are the ones a naive
/// `output / 10` gets wrong:
///
/// * **A duty below one step still gets a whole step.** The C++ turns the relay
///   *on* at counter 0 whenever `pidOutput > 0`, so a PID output of 0.4 — a
///   tenth of a percent — is delivered as a full 10 ms tick. A 0.1 % duty on a
///   2 kW boiler is physically meaningless; a 10 ms pulse every second is what
///   the machine has always done.
/// * **A duty of exactly `n * 10` is `n` steps, not `n + 1`.** The comparison is
///   `pidOutput > counter`, so at `pidOutput == 10` the counter-10 tick is off.
///
/// Computed by counting rather than by `ceil(output / 10.0)` so it is a
/// transcription of the ISR and not a second opinion about it.
#[must_use]
pub fn chopper_on_ticks(pid_output: Duty) -> u32 {
    let mut ticks = 0;
    let mut counter = 0;
    while counter < WINDOW_MS {
        if chopper_tick_level(pid_output, counter) {
            ticks += 1;
        }
        counter += CHOPPER_STEP_MS;
    }
    ticks
}

/// The on-time fraction of the window, `0.0 ..= 1.0`.
///
/// This is the number the LEDC carrier's duty cycle is set from, and it is the
/// C++ chopper's duty: the same 10 ms quantisation, expressed as a fraction.
#[must_use]
pub fn on_fraction(pid_output: Duty) -> f64 {
    f64::from(chopper_on_ticks(pid_output)) / f64::from(CHOPPER_STEPS)
}

/// The LEDC duty count for a PID output, at a given timer resolution.
///
/// `max_duty` is `hal::ledc::LedcDriver::get_max_duty()`, i.e.
/// `Resolution::max_duty()` for the configured resolution.
///
/// # Rounding, and why it is round-half-away-from-zero
///
/// `f64::round` rounds halves away from zero, so duty `0.5` of the way between
/// two counts goes to the higher one. The alternative — truncation — biases
/// every duty *down* by half a count, which on a heater is a systematic
/// under-power rather than a symmetric error. The error is bounded by half a
/// count, so with the resolution chosen in [`cc-hal-esp32::heater`] it is far
/// inside R1-07's 1 % acceptance bound.
///
/// The result is clamped to `0 ..= max_duty`. That clamp is not decorative:
/// `esp-idf-hal`'s own comment on `Resolution::max_duty` says the duty "must not
/// exceed `2^N - 1` to avoid timer overflow" at the timer's *maximum*
/// resolution, and `LedcDriver::set_duty` silently clamps to `max_duty` rather
/// than erroring. Clamping here means the overflow case is a test and not a
/// surprise on the bench.
#[must_use]
pub fn duty_counts(pid_output: Duty, max_duty: u32) -> u32 {
    if max_duty == 0 {
        return 0;
    }
    let counts = round_half_away_from_zero(on_fraction(pid_output) * f64::from(max_duty));
    if counts >= f64::from(max_duty) {
        max_duty
    } else {
        // The cast is exact: `on_fraction` is in `0.0 ..= 1.0` and `max_duty` is
        // a `u32`, so `counts` is in `0.0 ..= f64::from(max_duty)` and the
        // truncation cannot lose anything.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // Justification: as above — the value has just been clamped into
        // `0.0 ..= f64::from(max_duty)`, a range every value of which is exactly
        // representable in a `u32` and none of which is negative.
        {
            counts as u32
        }
    }
}

/// The on-time the hardware actually delivers, in milliseconds.
///
/// `counts / max_duty` of one carrier period, converted with the period derived
/// from [`CARRIER_HZ`] rather than assumed — so the function stays correct if the
/// carrier constant is ever changed, and the *equivalence with
/// [`WINDOW_MS`]* is a separately checked invariant (below) rather than a silent
/// coupling.
///
/// This is the number the hardware acceptance criterion is about ("duty matching
/// the PID output within 1 %"), expressed so it can be compared against the C++'s
/// `pid_output` directly: the C++ delivers `chopper_on_ticks * CHOPPER_STEP_MS` ms.
#[must_use]
pub fn delivered_on_time_ms(pid_output: Duty, max_duty: u32) -> f64 {
    if max_duty == 0 {
        return 0.0;
    }
    let period_ms = f64::from(1_000 / CARRIER_HZ);
    f64::from(duty_counts(pid_output, max_duty)) * period_ms / f64::from(max_duty)
}

// Everything in this module that reasons about "one carrier period *is* one
// control window" — `delivered_on_time_ms` compared against the C++'s
// millisecond duty above all — depends on this, and it must not be able to rot
// into an assumption. It also makes the `1_000 / CARRIER_HZ` in that function
// exact.
const _: () = assert!(CARRIER_HZ * WINDOW_MS == 1_000);

/// Contact level changes per second the **C++** chopper makes at a constant duty.
///
/// Computed from [`chopper_tick_level`] rather than asserted, so it is a
/// measurement of the transcription and not a second opinion about it. The wrap
/// edge is counted: the last tick of a window is counter 990, the first of the
/// next is counter 0, and the change between them is a real contactor operation.
///
/// The result is 0 or 2, never 1, and never more: the C++'s predicate is
/// monotone in the counter, so a constant duty is one contiguous ON run per
/// window with at most one falling edge, plus the rising edge at the wrap.
#[must_use]
pub fn cpp_transitions_per_second(pid_output: Duty) -> u32 {
    let mut transitions = 0;
    let mut previous = chopper_tick_level(pid_output, 0);
    for counter in (CHOPPER_STEP_MS..WINDOW_MS).step_by(CHOPPER_STEP_MS as usize) {
        let level = chopper_tick_level(pid_output, counter);
        if level != previous {
            transitions += 1;
        }
        previous = level;
    }
    // The wrap: counter 990 back to counter 0.
    if previous != chopper_tick_level(pid_output, 0) {
        transitions += 1;
    }
    transitions
}

/// Contact level changes per second the **chosen carrier** makes at a constant duty.
///
/// A square wave makes two transitions per period — one rising, one falling —
/// except at the two ends, which are steady levels and make none. So this is
/// `2 * CARRIER_HZ` for any interior duty and 0 at duty 0 and at full duty.
///
/// [`CARRIER_HZ`] is 1, which is the whole point: the C++'s answer for the same
/// duty is in [`cpp_transitions_per_second`], and at 1 Hz the two agree for
/// every reachable duty. At the R1-07 brief's 100 Hz this function would return
/// 200, and `the_carrier_does_not_switch_the_contactor_more_than_the_cpp_does`
/// would fail loudly.
#[must_use]
pub fn carrier_transitions_per_second(pid_output: Duty, max_duty: u32) -> u32 {
    if max_duty == 0 {
        return 0;
    }
    let counts = duty_counts(pid_output, max_duty);
    if counts == 0 || counts >= max_duty {
        0
    } else {
        2 * CARRIER_HZ
    }
}

/// Round half away from zero, without `std`.
///
/// `f64::round` lives in `std` (`core` has no float intrinsics without a
/// libm), and this crate is `no_std` by rule 04 §1. The only value ever passed
/// in is `on_fraction * max_duty`, which is in `0.0 ..= u32::MAX` — far inside
/// the `2^53` where every `f64` is an exact integer, so the truncation and the
/// subtraction below are exact and the result is the same number `round` would
/// give.
///
/// A `NaN` cannot reach it (`on_fraction` is a ratio of two `u32`s) but is
/// handled rather than assumed: it returns `0.0`, which is the safe answer.
fn round_half_away_from_zero(value: f64) -> f64 {
    if value.is_nan() || value <= 0.0 {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    // Justification: `value` is in `0.0 ..= u32::MAX` by the only caller's
    // arithmetic — `on_fraction` is a ratio of two `u32`s and `max_duty` is a
    // `u32` — so it is positive, exactly representable as a `u64`, and the
    // truncation loses no information.
    let whole = value as u64 as f64;
    if value - whole >= 0.5 {
        whole + 1.0
    } else {
        whole
    }
}

/// Why the heater output is being held at zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GateBlock {
    /// The supervisor has not beaten yet.
    ///
    /// The recovered firmware's boot log says, verbatim:
    /// `"heater interrupt running on GPIO2 (active high), output held off until
    /// the supervisor beats"` ([08 §3](../../docs/rust-migration/08-recovered-oracle.md)).
    /// Without this, a non-zero PID output computed during start-up — before the
    /// state machine, the sensors and the config are all live — would reach the
    /// relay. The gate makes "the supervisor is running" a precondition of
    /// heating rather than a hope about task ordering.
    SupervisorNotBeating,
    /// The supervisor beat, and then stopped.
    ///
    /// The deadman: if the heartbeat has not been refreshed within
    /// [`DEADMAN_TIMEOUT_MS`], the heater is de-energised. Stronger than the
    /// watchdog, which only fires at 5 s and only resets the chip
    /// ([04 §3.4](../../docs/rust-migration/04-target-architecture.md#watchdog));
    /// the deadman drops the heater in about one interlock period.
    DeadmanExpired {
        /// How long since the last beat.
        elapsed: Millis,
    },
}

/// How often the shell re-asserts the actuator states, from the recovered
/// firmware's boot log: `config: ... interlock 500 ms` ([08 §3](../../docs/rust-migration/08-recovered-oracle.md)).
///
/// It is the granularity at which the gate is consulted and at which the duty is
/// pushed to the hardware, so the worst-case time between "the supervisor
/// stopped" and "the heater is off" is one interlock period plus the deadman
/// timeout.
pub const INTERLOCK_PERIOD_MS: u32 = 500;

/// How long the supervisor may be silent before the heater is dropped.
///
/// **Not recovered from the oracle** — the binary's strings did not contain the
/// value, only the phrase "deadman=armed" in its periodic log line. Chosen, not
/// transcribed, and the reasoning is the point:
///
/// * it must be shorter than the 5 s task watchdog, or the deadman buys nothing
///   over the reset;
/// * it must be longer than one interlock period, or a single long
///   preemption of the supervisor — a flash erase, an OTA write, an HTTP body —
///   would drop the heater of a machine that is otherwise perfectly healthy.
///
/// Two interlock periods satisfies both with room to spare, and bounds the
/// de-energisation at 1.5 s rather than the watchdog's 5 s. If R1-07's hardware
/// test or a field report says otherwise, this is the constant to change, and it
/// is in one place for exactly that reason.
pub const DEADMAN_TIMEOUT_MS: u32 = 2 * INTERLOCK_PERIOD_MS;

/// The latching gate in front of the heater output.
///
/// The two properties it exists to provide, both from the recovered firmware
/// ([08 §3, §4](../../docs/rust-migration/08-recovered-oracle.md)):
///
/// 1. **The output is held at zero until the supervisor's first heartbeat.**
///    [`HeaterGate::new`] starts closed and there is no way to open it except
///    [`HeaterGate::heartbeat`]. A default-constructed gate is therefore the
///    safe state, which is the only acceptable default for a 2 kW heater.
/// 2. **A supervisor that stops beating drops the heater.** The gate does not
///    latch permanently — the C++ must be able to recover from a hang — it
///    simply refuses to pass a non-zero duty.
///
/// It is a value, not a cell: the shell owns it, the control task moves it in and
/// out of the task, and every decision is a pure function of `(gate, now)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeaterGate {
    /// The last heartbeat, or `None` if the supervisor has never beaten.
    last_heartbeat: Option<Millis>,
}

impl HeaterGate {
    /// A closed gate: the heater may not be energised.
    ///
    /// Deliberately the only constructor. There is no `new_armed()` and no
    /// public field, so "the gate starts open" is not expressible.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_heartbeat: None,
        }
    }

    /// Record a supervisor heartbeat.
    pub fn heartbeat(&mut self, now: Millis) {
        self.last_heartbeat = Some(now);
    }

    /// Whether the supervisor has ever beaten.
    #[must_use]
    pub const fn has_beat(&self) -> bool {
        self.last_heartbeat.is_some()
    }

    /// The last heartbeat, if any.
    #[must_use]
    pub const fn last_heartbeat(&self) -> Option<Millis> {
        self.last_heartbeat
    }

    /// Why the output is being held at zero, or `None` if it is permitted.
    #[must_use]
    pub fn blocked_at(&self, now: Millis) -> Option<GateBlock> {
        let Some(last) = self.last_heartbeat else {
            return Some(GateBlock::SupervisorNotBeating);
        };
        let elapsed = now.since(last);
        if elapsed.raw() > DEADMAN_TIMEOUT_MS {
            Some(GateBlock::DeadmanExpired { elapsed })
        } else {
            None
        }
    }

    /// The duty count to actually drive, given a requested one.
    ///
    /// **Zero is returned whenever the gate is closed, whatever was asked for.**
    /// That is the whole contract, and it is why this is a function rather than
    /// a boolean the caller branches on: a caller cannot forget the `else`.
    ///
    /// A `max_duty` of 0 is passed through as 0 — there is no hardware
    /// resolution to divide into, and 0 is the safe answer.
    #[must_use]
    pub fn resolve(&self, now: Millis, requested: u32, max_duty: u32) -> u32 {
        if self.blocked_at(now).is_some() {
            0
        } else {
            requested.min(max_duty)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::vec::Vec;

    fn duty(raw: f32) -> Duty {
        Duty::new(raw)
    }

    #[test]
    fn the_window_is_one_hertz_with_a_one_percent_step() {
        assert_eq!(WINDOW_MS, 1000);
        assert_eq!(CHOPPER_STEP_MS, 10);
        assert_eq!(CHOPPER_STEPS, 100);
    }

    /// R1-07 step 3: the `(pid_output, counter) -> level` table, at `pid_output`
    /// = 0, at `pid_output` = window, and at every wrap boundary.
    ///
    /// This is the C++ ISR, transcribed, checked against the two cases that
    /// define it and against every one of the 100 counters in between.
    #[test]
    fn the_tick_table_is_the_cpp_isr() {
        // pid_output = 0: `0 <= counter` for every counter, so the relay is off
        // for the whole window. This is `PidDisabled`, and it is the case a
        // gate that failed open would get wrong.
        for counter in (0..WINDOW_MS).step_by(CHOPPER_STEP_MS as usize) {
            assert!(
                !chopper_tick_level(duty(0.0), counter),
                "counter {counter} must be off at duty 0"
            );
        }
        assert_eq!(chopper_on_ticks(duty(0.0)), 0);
        assert_close(on_fraction(duty(0.0)), 0.0);

        // pid_output = window: `1000 <= counter` is false for every counter in
        // 0..1000, so the relay is on for the whole window. 100 % duty.
        for counter in (0..WINDOW_MS).step_by(CHOPPER_STEP_MS as usize) {
            assert!(
                chopper_tick_level(duty(1000.0), counter),
                "counter {counter} must be on at full duty"
            );
        }
        assert_eq!(chopper_on_ticks(duty(1000.0)), CHOPPER_STEPS);
        assert_close(on_fraction(duty(1000.0)), 1.0);

        // The wrap boundary, twice: counter 990 is the last tick, counter 1000
        // never occurs, and anything above the window is not a valid counter.
        assert!(chopper_tick_level(duty(1000.0), 990));
        assert!(!chopper_tick_level(duty(1000.0), 1000));
        assert_eq!(chopper_on_ticks(duty(1001.0)), CHOPPER_STEPS);
    }

    /// The quantisation, including the two edges a division gets wrong.
    #[test]
    // Justification: the loop bounds are `n <= CHOPPER_STEPS` (100) and
    // `ticks <= CHOPPER_STEPS`, so the product is at most 1000 and every value
    // involved is exactly representable in an `f32`.
    #[allow(clippy::cast_precision_loss)]
    fn the_ten_millisecond_quantisation() {
        // Below one step: still a whole step, because the C++ turns the relay on
        // at counter 0 for any positive output.
        assert_eq!(chopper_on_ticks(duty(0.0001)), 1);
        assert_eq!(chopper_on_ticks(duty(1.0)), 1);
        // Exactly n steps is n steps, not n + 1: the comparison is `>`.
        for n in 1..=CHOPPER_STEPS {
            assert_eq!(
                chopper_on_ticks(duty((n * CHOPPER_STEP_MS) as f32)),
                n,
                "{n} steps"
            );
        }
        // One unit over a step boundary is the next step up.
        assert_eq!(chopper_on_ticks(duty(10.5)), 2);
        assert_eq!(chopper_on_ticks(duty(19.9)), 2);
        assert_eq!(chopper_on_ticks(duty(20.0)), 2);
        assert_eq!(chopper_on_ticks(duty(20.1)), 3);
        // Saturates at the window.
        assert_eq!(chopper_on_ticks(duty(5000.0)), CHOPPER_STEPS);
    }

    /// Every fraction the chopper can produce, in order, and nothing else. This
    /// is the set of on-times the LEDC carrier has to be able to reproduce.
    #[test]
    fn the_set_of_reachable_fractions_is_exactly_the_hundredths() {
        let mut seen: Vec<u32> = Vec::new();
        let mut output = 0.0f32;
        while output <= 1000.0 {
            let ticks = chopper_on_ticks(duty(output));
            if !seen.contains(&ticks) {
                seen.push(ticks);
            }
            output += 0.25;
        }
        seen.sort_unstable();
        let expected: Vec<u32> = (0..=CHOPPER_STEPS).collect();
        assert_eq!(seen, expected, "only 0..=100 ticks are reachable");
    }

    /// The duty→count mapping at the resolution `cc-hal-esp32::heater` uses.
    ///
    /// That is LEDC `Bits17` on the original ESP32, whose `max_duty()` is
    /// `1 << 17` = 131 072 (`esp-idf-hal-0.47.0/src/ledc.rs`,
    /// `Resolution::max_duty` — note that the `2^N - 1` special case is for
    /// `Bits20` only, so seventeen gives a plain `1 << 17`).
    /// 131 072/100 = 1310.72 counts per 1 % step, so the worst-case
    /// reproduction error of a C++ duty is half a count = **3.8 µs**.
    #[test]
    // Justification: `ticks <= CHOPPER_STEPS` (100), so the product is at most
    // 1000 and exactly representable in an `f32`.
    #[allow(clippy::cast_precision_loss)]
    fn duty_counts_at_the_chosen_resolution() {
        const MAX_DUTY: u32 = CHOSEN_MAX_DUTY;

        assert_eq!(duty_counts(duty(0.0), MAX_DUTY), 0);
        assert_eq!(duty_counts(duty(1000.0), MAX_DUTY), MAX_DUTY);
        assert_eq!(
            duty_counts(duty(500.0), MAX_DUTY),
            MAX_DUTY / 2,
            "50 % is exact"
        );

        // Monotonic in the PID output: a heater that could ask for more power
        // and get less would be a serious defect.
        let mut previous = 0;
        let mut output = 0.0f32;
        while output <= 1000.0 {
            let counts = duty_counts(duty(output), MAX_DUTY);
            assert!(counts >= previous, "{output} ms: {counts} < {previous}");
            previous = counts;
            output += 1.0;
        }

        // The error against the ideal fraction is bounded by half a count.
        for ticks in 0..=CHOPPER_STEPS {
            let output = (ticks * CHOPPER_STEP_MS) as f32;
            let ideal = f64::from(ticks) * f64::from(MAX_DUTY) / f64::from(CHOPPER_STEPS);
            let actual = f64::from(duty_counts(duty(output), MAX_DUTY));
            assert!(
                (actual - ideal).abs() <= 0.5 + f64::EPSILON,
                "{ticks} ticks: {actual} vs {ideal}"
            );
        }
    }

    /// `max_duty == 0` is a degenerate resolution, and the answer must still be
    /// safe rather than a division by zero.
    #[test]
    fn a_zero_resolution_resolves_to_zero() {
        assert_eq!(duty_counts(duty(1000.0), 0), 0);
        assert_eq!(duty_counts(duty(500.0), 0), 0);
    }

    /// The `2^N - 1` caveat, as a test rather than as a comment.
    ///
    /// `esp-idf-hal` says the duty "must not exceed `2^N - 1` to avoid timer
    /// overflow" *at the timer's maximum resolution* (20-bit on the original
    /// ESP32). `LedcDriver::set_duty` clamps silently, so without a clamp of our
    /// own an out-of-range request would become a wrong-but-plausible duty
    /// instead of an error.
    #[test]
    fn the_count_never_exceeds_max_duty() {
        for max_duty in [1_u32, 2, 3, 7, 100, 255, 256, 1023, 1024, 4095, 65_535] {
            for output in [0.0f32, 0.5, 12.5, 250.0, 999.0, 1000.0, 1001.0, 1e6] {
                let counts = duty_counts(duty(output), max_duty);
                assert!(
                    counts <= max_duty,
                    "duty {output} at max_duty {max_duty} produced {counts}"
                );
            }
        }
    }

    // ---- the carrier: does it reproduce the C++, and at what wear? --------

    /// The headline requirement, stated as an assertion: the LEDC duty computed
    /// for a PID output reproduces the C++ chopper's on-time, in milliseconds,
    /// to within [`REPRODUCTION_TOLERANCE_MS`].
    ///
    /// The C++'s on-time is `chopper_on_ticks(pid) * CHOPPER_STEP_MS` ms. The
    /// hardware's is `delivered_on_time_ms(pid, CHOSEN_MAX_DUTY)`. The two are
    /// compared over the whole range at half-millisecond granularity, which
    /// covers every one of the C++'s 101 reachable levels twice over and every
    /// quantisation edge in between.
    ///
    /// The bound is **not** loosened to make the test pass. Two facts fix it:
    /// the C++ itself only resolves to 10 ms (`CHOPPER_STEP_MS`), and the
    /// rounding in `duty_counts` is half a count, i.e. `0.5 / 131 072` of the
    /// window = 3.815 µs. So the *floor* on the achievable error is 3.815 µs and
    /// [`REPRODUCTION_TOLERANCE_MS`] is 10 µs — 2.6× the floor, and 1000×
    /// inside the C++'s own quantisation. `the_chosen_carrier_reproduces_the_cpp_on_time_at_the_analytic_bound`
    /// pins the floor itself.
    #[test]
    fn the_chosen_carrier_reproduces_the_cpp_on_time() {
        let mut worst = 0.0f64;
        let mut worst_at = 0.0f32;
        let mut output = 0.0f32;
        while output <= 1000.0 {
            let wanted = f64::from(chopper_on_ticks(duty(output))) * f64::from(CHOPPER_STEP_MS);
            let delivered = delivered_on_time_ms(duty(output), CHOSEN_MAX_DUTY);
            let error = (delivered - wanted).abs();
            assert!(
                error <= REPRODUCTION_TOLERANCE_MS,
                "pid_output {output}: C++ {wanted} ms, carrier {delivered} ms, \
                 error {error} ms > {REPRODUCTION_TOLERANCE_MS} ms"
            );
            if error > worst {
                worst = error;
                worst_at = output;
            }
            output += 0.5;
        }
        // Printed so a regression shows the movement, not just the failure.
        assert!(
            worst <= REPRODUCTION_TOLERANCE_MS,
            "worst {worst} ms at pid_output {worst_at}"
        );
    }

    /// The analytic floor on the error: half a count at [`CHOSEN_MAX_DUTY`].
    ///
    /// Separate from the test above because that one passes at whatever the
    /// measured error happens to be, and this one cannot: it is arithmetic.
    #[test]
    // Justification: `ticks <= CHOPPER_STEPS` (100), so the product is at most
    // 1000 and every value involved is exactly representable in an `f32`.
    #[allow(clippy::cast_precision_loss)]
    fn the_chosen_carrier_reproduces_the_cpp_on_time_at_the_analytic_bound() {
        let half_a_count_ms = 0.5 * f64::from(WINDOW_MS) / f64::from(CHOSEN_MAX_DUTY);
        assert!(
            half_a_count_ms <= REPRODUCTION_TOLERANCE_MS,
            "the tolerance is tighter than the rounding floor {half_a_count_ms} ms"
        );
        // And 0.5 counts really is the worst case over all 101 reachable levels.
        for ticks in 0..=CHOPPER_STEPS {
            let output = (ticks * CHOPPER_STEP_MS) as f32;
            let wanted = f64::from(ticks) * f64::from(CHOPPER_STEP_MS);
            let error = (delivered_on_time_ms(duty(output), CHOSEN_MAX_DUTY) - wanted).abs();
            assert!(
                error <= half_a_count_ms + f64::EPSILON,
                "{ticks} ticks: {error} ms > {half_a_count_ms} ms"
            );
        }
    }

    /// The requirement the 100 Hz design failed: **the contactor must not be
    /// switched more often than the C++ switched it.**
    ///
    /// Both numbers are computed, not asserted: the C++'s from
    /// [`chopper_tick_level`] walked over a whole window including the wrap, the
    /// carrier's from the duty that would actually be written to the register. The
    /// test then requires them to be *equal* — not merely bounded — because the
    /// whole point of R1-07 is that the heater behaves as it always has.
    ///
    /// If someone "corrects" [`CARRIER_HZ`] back to 100, this fails on the first
    /// interior duty with 200 against 2.
    #[test]
    fn the_carrier_does_not_switch_the_contactor_more_than_the_cpp_does() {
        assert_eq!(
            CARRIER_HZ, 1,
            "one carrier period per control window; see the module docs"
        );
        let mut output = 0.0f32;
        while output <= 1000.0 {
            let cpp = cpp_transitions_per_second(duty(output));
            let carrier = carrier_transitions_per_second(duty(output), CHOSEN_MAX_DUTY);
            assert_eq!(
                carrier, cpp,
                "pid_output {output}: C++ switches the contactor {cpp}/s, \
                 the carrier {carrier}/s"
            );
            assert!(
                carrier <= 2,
                "pid_output {output}: {carrier} transitions/s is a mechanical \
                 duty the C++ never asked for"
            );
            output += 0.5;
        }
        // Spelled out at the five duties in the module docs' table, because a
        // silent rewrite of the carrier constant should be obviously wrong here.
        for (output, expected) in [(0.0f32, 0), (50.0, 2), (500.0, 2), (950.0, 2), (1000.0, 0)] {
            assert_eq!(
                cpp_transitions_per_second(duty(output)),
                expected,
                "{output}"
            );
            assert_eq!(
                carrier_transitions_per_second(duty(output), CHOSEN_MAX_DUTY),
                expected,
                "{output}"
            );
        }
    }

    /// The narrowest pulse the hardware can be asked for is the C++'s own 10 ms
    /// step — not something finer.
    ///
    /// This is a contactor question, and it is the part of it the *software* can
    /// settle: `duty_counts` is driven from `on_fraction`, which is quantised to
    /// `CHOPPER_STEPS`, so no PID output anywhere in `0..=1000` can produce a
    /// pulse shorter than 1 % of the window. Whether 10 ms is *itself* acceptable
    /// to the contactor is unmeasured and is recorded as such in the module docs
    /// and in `intentional-diffs.md` #5.
    #[test]
    fn the_minimum_pulse_is_the_cpp_ten_millisecond_step() {
        // Duty 0 is the one steady low level.
        assert_close(delivered_on_time_ms(duty(0.0), CHOSEN_MAX_DUTY), 0.0);

        // Everything above zero is at least a whole step, exactly as the C++'s
        // `pidOutput > counter` predicate delivers it.
        let mut output = f32::MIN_POSITIVE;
        while output <= 1000.0 {
            let delivered = delivered_on_time_ms(duty(output), CHOSEN_MAX_DUTY);
            assert!(
                delivered >= f64::from(CHOPPER_STEP_MS),
                "pid_output {output} produced a {delivered} ms pulse, narrower \
                 than the C++'s own {CHOPPER_STEP_MS} ms step"
            );
            output += 0.25;
        }
        // And the two ends are the two steady levels.
        assert_close(
            delivered_on_time_ms(duty(1000.0), CHOSEN_MAX_DUTY),
            f64::from(WINDOW_MS),
        );
    }

    /// 100 % duty is a distinct register value from 0, and 0 is what "disabled"
    /// means — so the two cannot be confused.
    ///
    /// `Resolution::Bits20` would break this: `Resolution::max_duty()` is
    /// `2^20 - 1` there, and ESP-IDF's own comment in `ledc_channel_config` says
    /// 100 % duty "is not reachable when the binded timer selects the maximum
    /// duty resolution" on the ESP32. At [`CHOSEN_RESOLUTION_BITS`] the count for
    /// full power is exactly `max_duty`, so the hardware output is a steady high
    /// level; at 0 it is a steady low level; and the gate reports which of the
    /// two it is holding the pin at.
    #[test]
    fn full_duty_is_distinguishable_from_disabled() {
        assert_eq!(
            CHOSEN_MAX_DUTY,
            1 << 17,
            "a plain 2^N, not the 2^N - 1 case"
        );
        assert_ne!(CHOSEN_MAX_DUTY, 0);

        let full = duty_counts(duty(1000.0), CHOSEN_MAX_DUTY);
        assert_eq!(full, CHOSEN_MAX_DUTY, "full power saturates, not wraps");
        assert_eq!(duty_counts(duty(0.0), CHOSEN_MAX_DUTY), 0);

        // A closed gate turns a full-power request into the disabled value; an
        // open one lets it through unchanged. The two are different numbers, so
        // "the heater is off" and "the heater is on" are distinguishable from
        // the register alone.
        let mut gate = HeaterGate::new();
        assert_eq!(gate.resolve(Millis::ZERO, full, CHOSEN_MAX_DUTY), 0);
        assert_eq!(
            gate.blocked_at(Millis::ZERO),
            Some(GateBlock::SupervisorNotBeating)
        );
        gate.heartbeat(Millis::ZERO);
        assert_eq!(
            gate.resolve(Millis::ZERO, full, CHOSEN_MAX_DUTY),
            CHOSEN_MAX_DUTY
        );
        assert_eq!(gate.blocked_at(Millis::ZERO), None);
        assert_ne!(
            gate.resolve(Millis::ZERO, full, CHOSEN_MAX_DUTY),
            gate.resolve(Millis::new(DEADMAN_TIMEOUT_MS + 1), full, CHOSEN_MAX_DUTY),
            "full power and a dropped deadman must not read the same"
        );
    }

    // ---- the gate ---------------------------------------------------------

    /// The property the recovered firmware's boot log states: `"output held off
    /// until the supervisor beats"`.
    #[test]
    fn a_fresh_gate_refuses_every_duty() {
        let gate = HeaterGate::new();
        assert!(!gate.has_beat());
        assert_eq!(
            gate.blocked_at(Millis::new(0)),
            Some(GateBlock::SupervisorNotBeating)
        );
        for requested in [0, 1, 100, 512, 1024] {
            assert_eq!(gate.resolve(Millis::new(0), requested, 1024), 0);
            assert_eq!(gate.resolve(Millis::new(u32::MAX), requested, 1024), 0);
        }
    }

    /// And the deadman: a gate that has been beating, and stops.
    #[test]
    fn the_deadman_drops_the_heater_and_recovers() {
        let mut gate = HeaterGate::new();
        gate.heartbeat(Millis::new(1_000));
        assert!(gate.has_beat());
        assert_eq!(gate.last_heartbeat(), Some(Millis::new(1_000)));
        assert_eq!(gate.blocked_at(Millis::new(1_000)), None);
        assert_eq!(gate.resolve(Millis::new(1_000), 512, 1024), 512);

        // Still alive at exactly the timeout: the test is a strict `>`, the same
        // comparison `PumpTimer::isExpired` uses.
        let at_timeout = Millis::new(1_000 + DEADMAN_TIMEOUT_MS);
        assert_eq!(gate.blocked_at(at_timeout), None);
        assert_eq!(gate.resolve(at_timeout, 512, 1024), 512);

        // One millisecond later it is not.
        let after = Millis::new(1_001 + DEADMAN_TIMEOUT_MS);
        let expired = gate.blocked_at(after);
        assert!(
            matches!(expired, Some(GateBlock::DeadmanExpired { .. })),
            "{expired:?}"
        );
        assert_eq!(gate.resolve(after, 512, 1024), 0);
        assert_eq!(gate.resolve(after, 0, 1024), 0);

        // A beat brings it back. The deadman is not a latch — the machine has to
        // be able to recover from a supervisor that wedged and then did not.
        gate.heartbeat(Millis::new(1_002 + DEADMAN_TIMEOUT_MS));
        assert_eq!(
            gate.blocked_at(Millis::new(1_002 + DEADMAN_TIMEOUT_MS)),
            None
        );
        assert_eq!(
            gate.resolve(Millis::new(1_002 + DEADMAN_TIMEOUT_MS), 512, 1024),
            512
        );
    }

    /// The millisecond clock wraps every 49.7 days, and the heater is running
    /// across that boundary for the whole of it. `since` is wrapping, so the
    /// deadman must be too.
    #[test]
    fn the_deadman_survives_the_millis_rollover() {
        let mut gate = HeaterGate::new();
        gate.heartbeat(Millis::new(u32::MAX - 10));
        // 16 ms after the wrap, measured the wrapping way round.
        assert_eq!(gate.blocked_at(Millis::new(5)), None);
        // A minute after the wrap, the wrapping way round: `now - (u32::MAX - 10)`
        // is `now + 11`.
        let after = Millis::new(60_000);
        assert_eq!(
            gate.blocked_at(after),
            Some(GateBlock::DeadmanExpired {
                elapsed: Millis::new(60_000 + 11)
            })
        );
    }

    /// `max_duty == 0` and a requested duty above the resolution are both
    /// clamped rather than passed through.
    #[test]
    fn resolve_clamps_to_the_resolution() {
        let mut gate = HeaterGate::new();
        gate.heartbeat(Millis::ZERO);
        assert_eq!(gate.resolve(Millis::ZERO, 5000, 1024), 1024);
        assert_eq!(gate.resolve(Millis::ZERO, 5000, 0), 0);
    }

    #[track_caller]
    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "expected {expected}, got {actual}"
        );
    }
}
