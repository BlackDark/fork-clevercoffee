//! Portable heater duty, ISR chopping, and the latching output gate.
//!
//! [`chopper_tick_level`] compares millisecond duty with a counter advancing by
//! [`CHOPPER_STEP_MS`] over [`WINDOW_MS`]. The 1000 ms window and 10 ms step give
//! 1% resolution. The PID sample interval and output limits use that same window;
//! changing the transport must not change the control law or invalidate tuning.
//!
//! A constant intermediate duty makes at most two relay edges per second, despite
//! 100 ISR entries. A 100 Hz carrier would make 200 edges without improving power
//! delivery and would wear the contactor. Any alternative carrier must preserve the
//! 1 Hz rate and at least the 10 ms resolution.
//!
//! Contactor minimum on/off times, coil compatibility, and realised pin duty remain
//! unmeasured. Arithmetic tests cannot replace a scope and dummy load with the
//! boiler disconnected; LEDC stays out of the firmware.
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::units::{Duty, Millis};

/// The chopper window, in milliseconds.
///
/// The PID's sample period and its output limits are both this number, so **1 Hz** is
/// also the control rate.
pub const WINDOW_MS: u32 = 1000;

/// How much the counter advances per tick, in milliseconds.
///
/// The timer period and this step are the same number by design: one ISR entry per step.
pub const CHOPPER_STEP_MS: u32 = 10;

/// How many steps a window has: 1000 / 10.
///
/// This is also the chopper's duty resolution — one step is 1 % of the window.
pub const CHOPPER_STEPS: u32 = WINDOW_MS / CHOPPER_STEP_MS;

/// The PWM carrier's frequency, in hertz: **one period per control window**.
///
/// This is the number a hardware carrier's duty is derived from, and it is 100 times below
/// the ISR rate because the ISR rate is not the level-change rate. The module docs carry
/// the derivation. Written as `1_000 / WINDOW_MS` rather than as a literal so that "one
/// period per window" is what the code states, and so that changing [`WINDOW_MS`] cannot
/// silently leave the carrier behind.
pub const CARRIER_HZ: u32 = 1_000 / WINDOW_MS;

/// The timer resolution the carrier uses, in bits.
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
/// Stated in time rather than as a percentage because that is the quantity the hardware
/// has to hit. 0.01 ms is 1/1000 of the 10 ms quantisation step the chopper resolves to,
/// i.e. a full three orders of magnitude inside the 1 % a heater needs to be honest about,
/// and it is still 1.3 counts at [`CHOSEN_MAX_DUTY`], so it is a real bound rather than a
/// tautology.
pub const REPRODUCTION_TOLERANCE_MS: f64 = 0.01;

/// The heater level for one tick: **off** when `pid_output <= counter_ms`, on otherwise.
///
/// `counter_ms` is one of `0, 10, …, 990`. This is the reference definition of the output
/// and the specification [`chopper_on_ticks`] is tested against; the ISR uses its integer
/// twin, [`chopper_tick_level_ms`].
#[expect(
    clippy::cast_precision_loss,
    reason = "a chopper counter is at most WINDOW_MS (1000), exact in f32"
)]
#[must_use]
pub fn chopper_tick_level(pid_output: Duty, counter_ms: u32) -> bool {
    // The level is the negation of `<=`: off at equality, not below it.
    //
    // Spelled with `partial_cmp` on purpose rather than as `a > b`. A `NaN` compares `<=`
    // **false**, so a `NaN` duty energises the relay here, which is the safe direction for
    // a heater; `a > b` would say the opposite and differ silently. The workspace denies
    // `clippy::neg_cmp_op_on_partial_ord`, and here it is right: this is a comparison
    // against a threshold whose domain includes `NaN`, not a numeric range check.
    let counter = counter_ms as f32;
    !matches!(
        pid_output.raw().partial_cmp(&counter),
        Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
    )
}

/// [`chopper_tick_level`] with **integer** inputs, for the ISR.
///
/// This is what [`AtomicChopper::tick`] calls, and the reason it exists is a
/// hardware one rather than a stylistic one:
///
/// **An Xtensa FPU instruction executed in interrupt context is a fatal
/// exception, not a slow one.** On the original ESP32 the FPU is coprocessor 0
/// (`XCHAL_CP_MASK 0x01`, `core-isa.h:122` `XCHAL_HAVE_FP 1`), and ESP-IDF's
/// coprocessor exception handler exists to *move* the FP save area between
/// threads — which is meaningless inside an ISR, because an ISR has no thread
/// save area to hand. `xtensa_vectors.S:1005-1046` therefore calls
/// `XT_RTOS_CP_STATE`, and on getting a null save area jumps to
/// `.L_xt_coproc_invalid` (`xtensa_vectors.S:1198-1201`), which writes
/// `PANIC_RSN_COPROCEXCEPTION` — 4 — into `EXCCAUSE` and panics. That is the
/// exact "Coprocessor exception" label the boot log shows, and it is a real
/// coprocessor fault after all, not the mislabelled `exccause 4` the panic
/// handler's own `reason[]` table claims (`"Level1Interrupt"`,
/// `panic_arch.c:230`). The permissive path exists behind
/// `CONFIG_FREERTOS_FPU_IN_ISR` (`freertos/Kconfig:462`, **default `n`**), which
/// is off here.
///
/// So the ISR must not generate an FP instruction, and it does not need to: both operands
/// are `u32` and at most `WINDOW_MS` (1000), so the `f32` round trip in the naive spelling
/// is lossless and the comparison is *identical* in integers.
/// `an_integer_level_matches_the_f32_reference_for_every_duty_and_counter` walks all 1001
/// duties x 100 counter values to prove that, so this function is a provable rewrite of
/// [`chopper_tick_level`] rather than a second opinion about it.
///
/// # Panics
///
/// Never.
#[must_use]
pub const fn chopper_tick_level_ms(duty_ms: u32, counter_ms: u32) -> bool {
    // The level is `>` and **not** `>=`: off at equality. That is the off-by-one a duty of
    // exactly 10 ms runs into -- one tick, not two -- and it is behaviour the tests pin.
    //
    // Spelled `>` where the `f32` [`chopper_tick_level`] spells `!(a <= b)`. That is not a
    // drift, it is the one place the two *must* differ: the `f32` form has to survive a
    // `NaN`, and a `u32` cannot be one. By the time a duty reaches the ISR it has been
    // truncated to a whole millisecond by `set_duty`, so a `NaN` became 0 before this ran.
    // `an_integer_level_matches_the_f32_reference_for_every_duty_and_counter` proves the
    // two agree over the whole reachable input space.
    duty_ms > counter_ms
}

/// How many 10 ms ticks of a window the chopper energises.
///
/// This is the aggregate a carrier has to reproduce: `on_fraction` below is `ticks / 100`,
/// the fraction of the second the relay is energised.
///
/// The two edges worth stating, because they are the ones a naive `output / 10` gets
/// wrong:
///
/// * **A duty below one step still gets a whole step.** The level is on at counter 0
///   whenever `pid_output > 0`, so a PID output of 0.4 -- a tenth of a percent -- is
///   delivered as a full 10 ms tick. A 0.1 % duty on a 2 kW boiler is physically
///   meaningless; a 10 ms pulse every second is what the machine does.
/// * **A duty of exactly `n * 10` is `n` steps, not `n + 1`.** The comparison is
///   `pid_output > counter`, so at a duty of exactly 10 the counter-10 tick is off.
///
/// Computed by counting [`chopper_tick_level`] rather than by `ceil(output / 10.0)`, so
/// the two cannot disagree.
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
/// This is the number a carrier's duty cycle is set from: the chopper's on-time, on its
/// 10 ms quantisation, expressed as a fraction.
#[must_use]
pub fn on_fraction(pid_output: Duty) -> f64 {
    f64::from(chopper_on_ticks(pid_output)) / f64::from(CHOPPER_STEPS)
}

/// The LEDC duty count for a PID output, at a given timer resolution.
///
/// `max_duty` is `hal::ledc::LedcDriver::get_max_duty()`, the count that means 100 % at
/// the configured resolution.
///
/// # Rounding, and why it is round-half-away-from-zero
///
/// `f64::round` rounds halves away from zero, so duty `0.5` of the way between
/// two counts goes to the higher one. The alternative — truncation — biases
/// every duty *down* by half a count, which on a heater is a systematic
/// under-power rather than a symmetric error. The error is bounded by half a
/// count, so with the resolution chosen in `cc-hal-esp32::heater` it is 3.8 µs of a
/// 1000 ms window -- far inside the 1 % a heater needs to be honest about.
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
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 0..=max_duty just above: non-negative and exact in u32"
        )]
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
/// the PID output within 1 %"), expressed so it can be compared against the chopper's
/// `pid_output` directly: the chopper delivers `chopper_on_ticks * CHOPPER_STEP_MS` ms.
#[must_use]
pub fn delivered_on_time_ms(pid_output: Duty, max_duty: u32) -> f64 {
    if max_duty == 0 {
        return 0.0;
    }
    let period_ms = f64::from(1_000 / CARRIER_HZ);
    f64::from(duty_counts(pid_output, max_duty)) * period_ms / f64::from(max_duty)
}

// Everything in this module that reasons about "one carrier period *is* one
// control window" — `delivered_on_time_ms` compared against the chopper's
// millisecond duty above all — depends on this, and it must not be able to rot
// into an assumption. It also makes the `1_000 / CARRIER_HZ` in that function
// exact.
const _: () = assert!(CARRIER_HZ * WINDOW_MS == 1_000);

/// Level changes per second the chopper makes at a constant duty.
///
/// Computed from [`chopper_tick_level`] rather than asserted, so it is a measurement of
/// the predicate and not a claim about it. The wrap edge is counted: the last tick of a
/// window is counter 990, the first of the next is counter 0, and the change between them
/// is a real contactor operation.
///
/// The result is 0 or 2, never 1, and never more: the predicate is monotone in the
/// counter, so a constant duty is one contiguous ON run per window with at most one
/// falling edge, plus the rising edge at the wrap.
#[must_use]
pub fn chopper_transitions_per_second(pid_output: Duty) -> u32 {
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
/// [`CARRIER_HZ`] is 1, which is the whole point: the chopper's answer for the same duty
/// is [`chopper_transitions_per_second`], and at 1 Hz the two agree for every reachable
/// duty. At 100 Hz this function would return 200, and
/// `the_carrier_switches_the_contactor_no_more_than_the_chopper_does` would fail loudly.
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
/// `f64::round` lives in `std` (`core` has no float intrinsics without a libm), and this
/// crate is `no_std`. The only value ever passed
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
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "the only caller passes a ratio of two u32s times a u32: non-negative and within u32"
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
    /// The recovered firmware's boot log says it verbatim: `"heater interrupt running on
    /// GPIO2 (active high), output held off until the supervisor beats"`.
    ///
    /// Without this, a non-zero PID output computed during start-up -- before the state
    /// machine, the sensors and the configuration are all live -- would reach the relay.
    /// The gate makes "the supervisor is running" a precondition of heating rather than a
    /// hope about task ordering.
    SupervisorNotBeating,
    /// The supervisor beat, and then stopped.
    ///
    /// The deadman: if the heartbeat has not been refreshed within [`DEADMAN_TIMEOUT_MS`],
    /// the heater is de-energised.
    ///
    /// Stronger than the task watchdog, which fires at 5 s and does nothing but reset the
    /// chip: the deadman drops the heater within about one interlock period, so a hung
    /// supervisor cannot leave the boiler heating.
    DeadmanExpired {
        /// How long since the last beat.
        elapsed: Millis,
    },
}

/// How often the shell re-asserts the actuator states, from the recovered firmware's boot
/// log: `config: ... interlock 500 ms`.
///
/// It is the granularity at which the gate is consulted and at which the duty is
/// pushed to the hardware, so the worst-case time between "the supervisor
/// stopped" and "the heater is off" is one interlock period plus the deadman
/// timeout.
pub const INTERLOCK_PERIOD_MS: u32 = 500;

/// How long the supervisor may be silent before the heater is dropped.
///
/// **Not recovered from the oracle:** the binary's strings carry the phrase
/// `deadman=armed` in the periodic log line but not the value. It is chosen, not
/// recovered, and the reasoning is the point:
///
/// * it must be shorter than the 5 s task watchdog, or the deadman buys nothing
///   over the reset;
/// * it must be longer than one interlock period, or a single long
///   preemption of the supervisor — a flash erase, an OTA write, an HTTP body —
///   would drop the heater of a machine that is otherwise perfectly healthy.
///
/// Two interlock periods satisfies both with room to spare, and bounds the de-energisation
/// at 1.5 s rather than the watchdog's 5 s. If a hardware test or a field report says
/// otherwise, this is the constant to change, and it is in one place for exactly that
/// reason.
pub const DEADMAN_TIMEOUT_MS: u32 = 2 * INTERLOCK_PERIOD_MS;

/// The latching gate in front of the heater output.
///
/// The two properties it exists to provide, both of which the recovered firmware's boot
/// log records:
///
/// 1. **The output is held at zero until the supervisor's first heartbeat.**
///    [`HeaterGate::new`] starts closed and there is no way to open it except
///    [`HeaterGate::heartbeat`]. A default-constructed gate is therefore the
///    safe state, which is the only acceptable default for a 2 kW heater.
/// 2. **A supervisor that stops beating drops the heater.** The gate does not latch
///    permanently -- a supervisor that hangs and then recovers must be able to heat again
///    -- it simply refuses to pass a non-zero duty while the heartbeat is late.
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

    /// The `(pid_output, counter) -> level` table, at duty 0, at full duty and at every
    /// wrap boundary.
    ///
    /// Checked against the two cases that define it and against every one of the 100
    /// counters in between.
    #[test]
    fn the_tick_table_is_the_chopper_predicate() {
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
    #[expect(
        clippy::cast_precision_loss,
        reason = "the product is at most 1000, exact in f32"
    )]
    fn the_ten_millisecond_quantisation() {
        // Below one step: still a whole step, because the chopper turns the relay on
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
    /// reproduction error of a duty the chopper produces is half a count = **3.8 µs**.
    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "the product is at most 1000, exact in f32"
    )]
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

    // ---- the carrier: does it reproduce the chopper, and at what wear? --------

    /// The headline requirement, stated as an assertion: the LEDC duty computed
    /// for a PID output reproduces the chopper's on-time, in milliseconds,
    /// to within [`REPRODUCTION_TOLERANCE_MS`].
    ///
    /// The chopper's on-time is `chopper_on_ticks(pid) * CHOPPER_STEP_MS` ms. The
    /// hardware's is `delivered_on_time_ms(pid, CHOSEN_MAX_DUTY)`. The two are
    /// compared over the whole range at half-millisecond granularity, which
    /// covers every one of the chopper's 101 reachable levels twice over and every
    /// quantisation edge in between.
    ///
    /// The bound is **not** loosened to make the test pass. Two facts fix it:
    /// the chopper itself only resolves to 10 ms (`CHOPPER_STEP_MS`), and the
    /// rounding in `duty_counts` is half a count, i.e. `0.5 / 131 072` of the
    /// window = 3.815 µs. So the *floor* on the achievable error is 3.815 µs and
    /// [`REPRODUCTION_TOLERANCE_MS`] is 10 µs — 2.6× the floor, and 1000×
    /// inside the chopper's own quantisation. The test
    /// `the_chosen_carrier_reproduces_the_chopper_on_time_at_the_analytic_bound` pins the
    /// floor itself.
    #[test]
    fn the_chosen_carrier_reproduces_the_chopper_on_time() {
        let mut worst = 0.0f64;
        let mut worst_at = 0.0f32;
        let mut output = 0.0f32;
        while output <= 1000.0 {
            let wanted = f64::from(chopper_on_ticks(duty(output))) * f64::from(CHOPPER_STEP_MS);
            let delivered = delivered_on_time_ms(duty(output), CHOSEN_MAX_DUTY);
            let error = (delivered - wanted).abs();
            assert!(
                error <= REPRODUCTION_TOLERANCE_MS,
                "pid_output {output}: chopper on-time {wanted} ms, carrier {delivered} ms, \
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
    #[expect(
        clippy::cast_precision_loss,
        reason = "the product is at most 1000, exact in f32"
    )]
    fn the_chosen_carrier_reproduces_the_chopper_on_time_at_the_analytic_bound() {
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

    /// The requirement a 100 Hz carrier fails: **the contactor must not be switched more
    /// often than the chopper switches it.**
    ///
    /// Both numbers are computed, not asserted: the chopper's from [`chopper_tick_level`]
    /// walked over a whole window including the wrap, and the carrier's from the duty that
    /// would actually be written to the register. The test requires them to be *equal* --
    /// not merely bounded -- because the carrier exists to deliver the same mechanical
    /// duty, not merely the same power.
    ///
    /// If someone "corrects" [`CARRIER_HZ`] to 100, this fails on the first interior duty
    /// with 200 against 2.
    #[test]
    fn the_carrier_switches_the_contactor_no_more_than_the_chopper_does() {
        assert_eq!(
            CARRIER_HZ, 1,
            "one carrier period per control window; see the module docs"
        );
        let mut output = 0.0f32;
        while output <= 1000.0 {
            let chopper_ops = chopper_transitions_per_second(duty(output));
            let carrier = carrier_transitions_per_second(duty(output), CHOSEN_MAX_DUTY);
            assert_eq!(
                carrier, chopper_ops,
                "pid_output {output}: the chopper switches the contactor {chopper_ops}/s, \
                 the carrier {carrier}/s"
            );
            assert!(
                carrier <= 2,
                "pid_output {output}: {carrier} transitions/s is a mechanical \
                 duty the chopper never asked for"
            );
            output += 0.5;
        }
        // Spelled out at the five duties in the module docs' table, because a
        // silent rewrite of the carrier constant should be obviously wrong here.
        for (output, expected) in [(0.0f32, 0), (50.0, 2), (500.0, 2), (950.0, 2), (1000.0, 0)] {
            assert_eq!(
                chopper_transitions_per_second(duty(output)),
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

    /// The narrowest pulse the hardware can be asked for is a whole 10 ms step -- not
    /// something finer.
    ///
    /// This is a contactor question, and this is the part of it the *software* can settle:
    /// `duty_counts` is driven from `on_fraction`, which is quantised to `CHOPPER_STEPS`,
    /// so no PID output anywhere in `0..=1000` can produce a pulse shorter than 1 % of the
    /// window. Whether 10 ms is *itself* acceptable to the contactor is unmeasured, and the
    /// module docs record that.
    #[test]
    fn the_minimum_pulse_is_the_ten_millisecond_step() {
        // Duty 0 is the one steady low level.
        assert_close(delivered_on_time_ms(duty(0.0), CHOSEN_MAX_DUTY), 0.0);

        // Everything above zero is at least a whole step, exactly as the
        // `pid_output > counter` predicate delivers it.
        let mut output = f32::MIN_POSITIVE;
        while output <= 1000.0 {
            let delivered = delivered_on_time_ms(duty(output), CHOSEN_MAX_DUTY);
            assert!(
                delivered >= f64::from(CHOPPER_STEP_MS),
                "pid_output {output} produced a {delivered} ms pulse, narrower \
                 than the chopper's own {CHOPPER_STEP_MS} ms step"
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

// ===========================================================================
// The 10 ms GPTimer ISR
// ===========================================================================

/// Convert a duty fraction at a given `max_duty` back to the chopper's millisecond
/// scale.
///
/// The inverse of [`duty_counts`], and it exists because the two sides disagree
/// about what a duty *is*: [`duty_counts`] turns the chopper's millisecond duty into
/// a count at a carrier's resolution — which is what a register-driven carrier
/// such as `LEDC` would want — and this turns such a count back into
/// milliseconds for `cc_hal_esp32::heater::TimerIsrPwm`, which chops in the
/// chopper's own units.
///
/// The result is rounded to the **nearest** 10 ms step. The register resolution
/// ([`CHOSEN_MAX_DUTY`], 131 072) is not a multiple of the 100 steps in a window,
/// so a round trip cannot be exact, and the error is bounded by half a step —
/// 5 ms of a 1000 ms window, i.e. half a per cent. Rounding rather than
/// truncating is what bounds it at half a step instead of a whole one.
///
/// That is the chopper's own resolution and no worse. And the *duty* the ISR is given is a
/// whole number of 10 ms steps, which is a value the predicate accepts directly -- so the
/// delivered duty is one the chopper could have produced.
///
/// # Errors
///
/// None. [`f64`] maths on values in `0.0 ..= 1.0`.
#[must_use]
pub fn duty_ms_from_fraction(fraction: f32, max_duty: u32) -> u32 {
    if fraction <= 0.0 || max_duty == 0 {
        return 0;
    }
    // Round to the *nearest* 10 ms step, not down. Truncating would make every
    // duty biased low by up to a whole step (10 ms of 1000, i.e. 1 %); rounding
    // bounds the error at half a step, which is 0.5 %.
    let scaled = f64::from(max_duty) * f64::from(fraction.clamp(0.0, 1.0));
    let exact_steps = scaled / f64::from(max_duty) * f64::from(CHOPPER_STEPS);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "`min(CHOPPER_STEPS)` bounds the value to 0..=100"
    )]
    let steps = if exact_steps <= 0.0 {
        0u32
    } else {
        // `floor(x + 0.5)` without a libm: `exact_steps` is in `0..=100`, well
        // inside the range where `f64` addition and subtraction are exact.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "`exact_steps` is in 0..=100, so the rounded value is non-negative and small"
        )]
        let rounded = (exact_steps + 0.5) as u64;
        rounded.min(u64::from(CHOPPER_STEPS)) as u32
    };
    steps * CHOPPER_STEP_MS
}

/// The 10 ms hardware-timer period the heater ISR runs at, in microseconds.
///
/// **This is the shipped heater output**, not the ESP-IDF LEDC carrier: the carrier's
/// `ledc_ll_set_duty_start` spin runs with interrupts masked and takes up to one carrier
/// period, which at 1 Hz exceeds the 300 ms interrupt watchdog on this chip. The ISR has
/// no such problem -- 100 interrupts a second on a 240 MHz Xtensa is negligible -- and it
/// is the output this hardware is proven with.
pub const ISR_INTERVAL_US: u32 = 10_000;

/// One heater ISR tick's worth of state, and the decision it makes.
///
/// # The trade it embodies
///
/// A 1 Hz PWM carrier would switch the contactor twice a second instead of a hundred
/// times, and that is a real mechanical saving. It is not available on this chip. ESP-IDF's
/// `ledc_ll_set_duty_start` on the original ESP32
/// (`components/hal/esp32/include/hal/ledc_ll.h:485-489`) spins
///
/// ```c
/// while (hw->channel_group[speed_mode].channel[channel_num].conf1.duty_start);
/// ```
///
/// and `duty_start` is only cleared by the hardware at the next timer period, with
/// interrupts masked by `portENTER_CRITICAL(&ledc_spinlock)`
/// (`components/esp_driver_ledc/src/ledc.c:1603-1606`). At a 1 Hz carrier that is up to one
/// second against a 300 ms interrupt watchdog, so **every** duty write panics, including
/// the duty-0 write a transport's constructor makes. Any carrier slow enough to be
/// mechanically kind is slow enough to trip the watchdog, so the two requirements conflict
/// on this part.
///
/// The stated trade is therefore **contactor wear, not watchdog panics**: a hundred relay
/// operations per second instead of two, which is what this machine's chopper has always
/// done and what its contactor has survived. The carrier arithmetic stays in this module
/// because a chip whose LEDC does not spin can use it unchanged.
///
/// # The state machine
///
/// ```text
/// level   = chopper_tick_level(pid_output, counter_ms)
/// counter = (counter_ms + CHOPPER_STEP_MS) % window_ms
/// ```
///
/// One comparison, one addition, one comparison. It is here, and not in the device crate,
/// so that it can be walked a full window at a time on the host and the *level pattern*
/// can be asserted rather than the arithmetic.
///
/// # Why a struct and not just [`chopper_tick_level`]
///
/// [`chopper_tick_level`] is the *level* at a given counter and stays the reference
/// definition. This adds the two things the ISR has and the function does not: the
/// counter's own state, and the **armed** flag.
///
/// # The armed flag is a real hardware state, not a formality
///
/// An ISR that drives a relay before the pin is configured is a defect, and the timer must
/// be running before the pin is trusted while the pin must sit at the inactive level from
/// before the timer starts. [`arm`](Self::arm) and [`disarm`](Self::disarm) are the only
/// ways to change `armed`, so a caller cannot leave the ISR chopping while the gate is
/// shut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsrChopper {
    counter_ms: u32,
    /// The PID output in milliseconds of on-time per window, as the chopper holds it.
    pid_output: Duty,
    window_ms: u32,
    armed: bool,
}

impl Default for IsrChopper {
    fn default() -> Self {
        Self::new()
    }
}

impl IsrChopper {
    /// A disarmed chopper at counter 0, with no duty.
    ///
    /// **Disarmed**, so a default-constructed one cannot drive anything.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            counter_ms: 0,
            pid_output: Duty::new(0.0),
            window_ms: WINDOW_MS,
            armed: false,
        }
    }

    /// Let the ISR drive the relay. Idempotent.
    pub const fn arm(&mut self) {
        self.armed = true;
    }

    /// Stop the ISR driving the relay. Idempotent.
    ///
    /// The counterpart of [`HeaterGate`]'s refusal: the recovered firmware's boot log
    /// records `"output held off until the supervisor beats"`, and a deadman that cannot
    /// de-energise the heater is not a deadman.
    pub const fn disarm(&mut self) {
        self.armed = false;
    }

    /// Whether the ISR is allowed to drive.
    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.armed
    }

    /// Set the duty for the next window, and return the counter to 0.
    ///
    /// A fresh window on a new duty. The counter returns to 0 so the on-time is measured
    /// from the moment the duty took effect, not from wherever the old window happened to
    /// be.
    pub const fn set_duty(&mut self, pid_output: Duty) {
        self.pid_output = pid_output;
        self.counter_ms = 0;
    }

    /// The current counter, in milliseconds into the window.
    #[must_use]
    pub const fn counter_ms(&self) -> u32 {
        self.counter_ms
    }

    /// The configured control window, in milliseconds.
    #[must_use]
    pub const fn window_ms(&self) -> u32 {
        self.window_ms
    }

    /// Change the control window.
    ///
    /// The window is configuration, not a constant, so it is settable at runtime. Clamped
    /// to at least one step, because a window the counter can never fit inside would never
    /// wrap and the relay would latch on.
    pub const fn set_window_ms(&mut self, window_ms: u32) {
        self.window_ms = if window_ms < CHOPPER_STEP_MS {
            CHOPPER_STEP_MS
        } else {
            window_ms
        };
        if self.counter_ms >= self.window_ms {
            self.counter_ms = 0;
        }
    }

    /// One ISR tick: decide the level, advance the counter, and report it.
    ///
    /// Returns the level the relay should be at. `None` means the chopper is disarmed and
    /// the pin is to be left alone.
    ///
    /// This is the *whole* of the heater ISR's arithmetic, and the device crate's callback
    /// is this call plus one `pin.set_level`: an ISR must do nothing beyond that one GPIO
    /// write, and a handful of integer operations plus a comparison keeps it there.
    pub fn tick(&mut self) -> Option<bool> {
        if !self.armed {
            return None;
        }
        let level = chopper_tick_level(self.pid_output, self.counter_ms);
        let next = self.counter_ms + CHOPPER_STEP_MS;
        self.counter_ms = if next >= self.window_ms { 0 } else { next };
        Some(level)
    }
}

#[cfg(test)]
mod isr_tests {
    use super::*;
    use alloc::vec::Vec;

    /// Walk a full window of ISR ticks and return the level pattern.
    fn window(counter_steps: u32, pid: Duty) -> Vec<bool> {
        let mut chopper = IsrChopper::new();
        chopper.set_duty(pid);
        chopper.arm();
        (0..counter_steps)
            .map(|_| chopper.tick().unwrap_or(false))
            .collect()
    }

    #[test]
    fn a_disarmed_chopper_drives_nothing_and_a_new_one_starts_disarmed() {
        // A default-constructed chopper must not be able to energise a relay by being
        // ticked.
        let mut chopper = IsrChopper::new();
        assert!(!chopper.is_armed());
        for _ in 0..1_000 {
            assert_eq!(chopper.tick(), None, "a disarmed chopper reports no level");
        }
        // And the counter does not advance either, so arming later starts from 0.
        assert_eq!(chopper.counter_ms(), 0);
    }

    #[test]
    fn arming_is_idempotent_and_ticking_before_it_does_nothing() {
        let mut chopper = IsrChopper::new();
        chopper.set_duty(Duty::new(1000.0));
        chopper.arm();
        chopper.arm();
        assert!(chopper.is_armed());
        assert_eq!(chopper.tick(), Some(true));
        chopper.disarm();
        chopper.disarm();
        assert!(!chopper.is_armed());
        assert_eq!(chopper.tick(), None);
    }

    #[test]
    fn a_full_window_at_half_duty_is_fifty_ticks_then_fifty_not() {
        // The steady state at pid_output 500 is one contiguous ON run of 500 ms followed by
        // 500 ms of OFF. The pattern is asserted, not just the count, because "50 on ticks"
        // is also what a broken ISR that chops at the wrong phase would produce.
        let levels = window(CHOPPER_STEPS, Duty::new(500.0));
        assert_eq!(levels.len(), 100);
        assert!(levels[..50].iter().all(|level| *level), "first 500 ms on");
        assert!(
            levels[50..].iter().all(|level| !*level),
            "last 500 ms off -- one contiguous run, not 50 scattered pulses"
        );
        assert_eq!(chopper_on_ticks(Duty::new(500.0)), 50);
    }

    #[test]
    fn the_counter_makes_exactly_one_hundred_ticks_per_window() {
        // 1000 ms of window, 10 ms of step. If this ever became 99 or 101, the delivered
        // duty would drift from the commanded duty by a per cent per window.
        let mut chopper = IsrChopper::new();
        chopper.set_duty(Duty::new(0.0));
        chopper.arm();
        assert_eq!(chopper.counter_ms(), 0, "a new duty starts at 0");
        // One tick per 10 ms, so the counter returns to 0 after exactly
        // `CHOPPER_STEPS` ticks. The bound is a failsafe, not the expectation:
        // without it a counter that never wrapped would hang the test rather
        // than fail it.
        let mut ticks = 1u32;
        chopper.tick();
        while chopper.counter_ms() != 0 {
            chopper.tick();
            ticks += 1;
            assert!(ticks <= 10_000, "the counter never wrapped");
        }
        assert_eq!(ticks, CHOPPER_STEPS);
        assert_eq!(ticks, 100);
    }

    #[test]
    fn the_counter_visits_exactly_the_choppers_counter_values() {
        // The counter walks 0, 10, 20, ... 990 and then wraps. Anything else -- 5, 15, a
        // value that skips, a value that lands on 1000 -- would be a different duty.
        let mut chopper = IsrChopper::new();
        chopper.set_duty(Duty::new(0.0));
        chopper.arm();
        let mut seen = Vec::new();
        for _ in 0..CHOPPER_STEPS {
            seen.push(chopper.counter_ms());
            chopper.tick();
        }
        let expected: Vec<u32> = (0..WINDOW_MS).step_by(CHOPPER_STEP_MS as usize).collect();
        assert_eq!(seen, expected);
        assert_eq!(seen.first(), Some(&0));
        assert_eq!(seen.last(), Some(&990));
        assert!(!seen.contains(&WINDOW_MS), "1000 is never a counter value");
    }

    #[test]
    fn the_isr_pattern_follows_the_predicate_at_every_counter() {
        // The state machine and the reference function must not be able to
        // disagree: for every duty the chopper can express and every counter in the
        // window, `tick()` at that counter must equal `chopper_tick_level`.
        for output in [0.0f32, 0.4, 10.0, 250.0, 500.0, 750.0, 999.0, 1000.0] {
            let mut chopper = IsrChopper::new();
            chopper.set_duty(Duty::new(output));
            chopper.arm();
            for counter in (0..WINDOW_MS).step_by(CHOPPER_STEP_MS as usize) {
                assert_eq!(chopper.counter_ms(), counter, "duty {output}");
                let got = chopper.tick().unwrap_or(false);
                assert_eq!(
                    got,
                    chopper_tick_level(Duty::new(output), counter),
                    "duty {output} at counter {counter}"
                );
            }
        }
    }

    #[test]
    fn the_delivered_on_time_matches_the_chopper_for_every_representable_duty() {
        // The end-to-end property: whatever the ISR chops over a window is what
        // the chopper would have delivered. Summed from the tick pattern, not from
        // `chopper_on_ticks`, so the state machine is on the hook rather than
        // only the counting helper.
        for output in 0..=1_000u16 {
            let duty = Duty::new(f32::from(output));
            let on_ticks = window(CHOPPER_STEPS, duty)
                .iter()
                .filter(|level| **level)
                .count();
            #[expect(clippy::cast_possible_truncation, reason = "at most CHOPPER_STEPS")]
            let on_ticks = on_ticks as u32;
            assert_eq!(on_ticks, chopper_on_ticks(duty), "pid_output {output} ms");
        }
    }

    #[test]
    fn a_duty_of_exactly_one_step_is_one_tick_and_not_two() {
        // The off-by-one, walked rather than asserted: the level is off at
        // `pid_output <= counter`, so at a duty of exactly 10 the counter-0 tick is on and
        // the counter-10 tick is off. One tick, not two.
        let levels = window(CHOPPER_STEPS, Duty::new(10.0));
        assert!(levels[0], "counter 0 is on");
        assert!(!levels[1], "counter 10 is off -- one tick, not two");
        assert_eq!(levels.iter().filter(|level| **level).count(), 1);
    }

    #[test]
    fn a_sub_step_duty_still_gets_a_whole_tick() {
        // 0.4 ms is a tenth of a per cent, and the chopper delivers a full 10 ms because the
        // level is on at counter 0 whenever the duty is above 0. Preserved because it is
        // what the machine does: rounding a sub-step duty down to nothing would be a change
        // to the contactor's duty, not to the software's arithmetic.
        let levels = window(CHOPPER_STEPS, Duty::new(0.4));
        assert!(levels[0]);
        assert_eq!(levels.iter().filter(|level| **level).count(), 1);
    }

    #[test]
    fn setting_a_duty_restarts_the_window() {
        // The chopper gets this for free because its counter wraps every second and
        // its PID output changes on a 1 s timescale. Making it explicit means a
        // setpoint change takes effect on the next window rather than up to one
        // second later, which is the same thing the chopper does and is worth
        // stating rather than leaving to the wrap.
        let mut chopper = IsrChopper::new();
        chopper.set_duty(Duty::new(1000.0));
        chopper.arm();
        for _ in 0..37 {
            chopper.tick();
        }
        assert_eq!(chopper.counter_ms(), 370);
        chopper.set_duty(Duty::new(0.0));
        assert_eq!(chopper.counter_ms(), 0, "a new duty starts a new window");
        assert_eq!(chopper.tick(), Some(false));
    }

    #[test]
    fn a_narrower_window_is_honoured_and_a_ludicrous_one_is_clamped() {
        // The window is configuration, not a constant, so a window the counter cannot fit
        // inside must be clamped rather than trusted: it would never wrap and the relay
        // would latch on.
        let mut chopper = IsrChopper::new();
        chopper.set_duty(Duty::new(1000.0));
        chopper.set_window_ms(100);
        chopper.arm();
        assert_eq!(chopper.window_ms(), 100);
        let levels: Vec<bool> = (0..10).map(|_| chopper.tick().unwrap_or(false)).collect();
        assert_eq!(levels, alloc::vec![true; 10]);

        chopper.set_window_ms(0);
        assert_eq!(chopper.window_ms(), CHOPPER_STEP_MS, "clamped to one step");
        chopper.set_window_ms(5);
        assert_eq!(chopper.window_ms(), CHOPPER_STEP_MS);
    }

    #[test]
    fn shrinking_the_window_below_the_counter_resets_it() {
        // The chopper compares the *next* counter against the window, so a window
        // shrunk mid-window is applied at the next wrap, not immediately — but a
        // counter that is already past the new window would chop outside it, so
        // this resets. The chopper has no equivalent because its window never
        // changes at run time.
        let mut chopper = IsrChopper::new();
        chopper.set_duty(Duty::new(1000.0));
        chopper.arm();
        for _ in 0..50 {
            chopper.tick();
        }
        assert_eq!(chopper.counter_ms(), 500);
        chopper.set_window_ms(100);
        assert_eq!(chopper.counter_ms(), 0, "500 was outside the new window");
    }

    #[test]
    fn the_isr_period_is_ten_milliseconds_and_matches_the_step() {
        // `Timing::ISR_TIMER_INTERVAL_US = 10000` and
        // `Timing::ISR_COUNTER_INCREMENT = 10`.
        // They must agree, or the counter and the clock drift apart and the
        // window is not a window.
        assert_eq!(ISR_INTERVAL_US, 10_000);
        assert_eq!(ISR_INTERVAL_US / 1_000, CHOPPER_STEP_MS);
        assert_eq!(ISR_INTERVAL_US / 1_000, 10);
        // And 100 of them is the chopper's 1 Hz window.
        assert_eq!(CHOPPER_STEPS * ISR_INTERVAL_US / 1_000, WINDOW_MS);
    }

    #[test]
    fn a_hundred_interrupts_a_second_is_what_the_chopper_asks_for() {
        // The cost statement, as arithmetic: 1000 / 10 ms.
        assert_eq!(1_000_000 / ISR_INTERVAL_US, 100);
        // Against a 240 MHz Xtensa at 100 Hz, and against the LEDC alternative
        // that panics on this chip. The ISR is 100 interrupts a second; the LEDC
        // carrier would be 0 and would not boot.
        assert_eq!(CARRIER_HZ, 1);
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;

    /// The millisecond duty a `pid_output` actually delivers, i.e. what the ISR
    /// will be asked to chop to after the register round trip.
    fn round_trip(output: u16) -> u32 {
        let duty = Duty::new(f32::from(output));
        let counts = duty_counts(duty, CHOSEN_MAX_DUTY);
        duty_ms_from_fraction(
            #[expect(
                clippy::cast_precision_loss,
                reason = "counts are at most CHOSEN_MAX_DUTY, exact in f32"
            )]
            {
                counts as f32 / CHOSEN_MAX_DUTY as f32
            },
            CHOSEN_MAX_DUTY,
        )
    }

    #[test]
    fn the_round_trip_lands_on_the_delivered_duty_the_chopper_would_have_produced() {
        // The real invariant, and the one that matters: whatever comes back is
        // the chopper's own delivered on-time, `chopper_on_ticks(pid) * 10 ms`, to
        // within half a step of register quantisation.
        //
        // **Note what this is not**: a round trip back to `pid_output`. It cannot be, and
        // should not be. The chopper turns a 1 ms output into a 10 ms tick -- the level is
        // on at counter 0 whenever the output is above 0 -- so a 1 ms output delivers
        // 10 ms, and the transport correctly reports 10. The *delivered* duty is the
        // quantity the contactor sees, and that is what has to match.
        for output in 0..=1_000u16 {
            let duty = Duty::new(f32::from(output));
            let expected = chopper_on_ticks(duty) * CHOPPER_STEP_MS;
            let back = round_trip(output);
            assert!(
                back.abs_diff(expected) <= CHOPPER_STEP_MS / 2,
                "pid_output {output}: delivered {expected} ms, round trip {back} ms"
            );
        }
    }

    #[test]
    fn a_sub_step_duty_delivers_a_whole_tick_through_the_transport_too() {
        // The chopper's quantisation survives the transport, which is the point: a
        // caller asking for 1 ms gets the 10 ms the chopper would have given it, not
        // a new 1 ms behaviour the contactor has never seen.
        assert_eq!(round_trip(1), CHOPPER_STEP_MS);
        assert_eq!(round_trip(9), CHOPPER_STEP_MS);
        assert_eq!(
            round_trip(10),
            CHOPPER_STEP_MS,
            "10 ms is one tick, not two"
        );
        assert_eq!(round_trip(20), 2 * CHOPPER_STEP_MS);
    }

    #[test]
    fn the_round_trip_lands_on_the_choppers_own_step_grid() {
        // Whatever comes back must be a whole number of 10 ms steps, because the
        // ISR's counter is a 10 ms counter: a value off the grid would be a duty
        // the chopper could never have produced.
        for output in 0..=1_000u16 {
            let duty = Duty::new(f32::from(output));
            let counts = duty_counts(duty, CHOSEN_MAX_DUTY);
            let back = duty_ms_from_fraction(
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "counts are at most CHOSEN_MAX_DUTY, exact in f32"
                )]
                {
                    counts as f32 / CHOSEN_MAX_DUTY as f32
                },
                CHOSEN_MAX_DUTY,
            );
            assert_eq!(back % CHOPPER_STEP_MS, 0, "{back} ms is off the 10 ms grid");
        }
    }

    #[test]
    fn a_zero_or_full_duty_survives_the_round_trip_exactly() {
        // The two ends are the ones that matter: a closed gate must be a hard 0
        // and a full-on must be a hard 1000, or the contactor is being asked for
        // something the chopper never asked for.
        for output in [0u16, 1_000] {
            let duty = Duty::new(f32::from(output));
            let counts = duty_counts(duty, CHOSEN_MAX_DUTY);
            let back = duty_ms_from_fraction(
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "counts are at most CHOSEN_MAX_DUTY, exact in f32"
                )]
                {
                    counts as f32 / CHOSEN_MAX_DUTY as f32
                },
                CHOSEN_MAX_DUTY,
            );
            assert_eq!(
                back,
                u32::from(output),
                "pid_output {output} did not survive"
            );
        }
    }

    #[test]
    fn a_nonsense_fraction_is_clamped_rather_than_trusted() {
        assert_eq!(duty_ms_from_fraction(0.0, CHOSEN_MAX_DUTY), 0);
        assert_eq!(duty_ms_from_fraction(-1.0, CHOSEN_MAX_DUTY), 0);
        assert_eq!(duty_ms_from_fraction(1.0, CHOSEN_MAX_DUTY), WINDOW_MS);
        assert_eq!(duty_ms_from_fraction(2.0, CHOSEN_MAX_DUTY), WINDOW_MS);
        assert_eq!(
            duty_ms_from_fraction(1.0, 0),
            0,
            "a zero resolution is zero duty"
        );
    }

    #[test]
    fn the_isr_period_is_the_choppers_and_the_window_is_one_hertz() {
        // The two numbers the ISR is built from, stated together: 10 ms and
        // 1000 ms. If either moved, the delivered duty would silently change.
        assert_eq!(ISR_INTERVAL_US, 10_000);
        assert_eq!(WINDOW_MS, 1_000);
        assert_eq!(CHOPPER_STEPS, 100);
        assert_eq!(CARRIER_HZ, 1);
    }
}

/// The same chopper, in a form an interrupt handler can hold.
///
/// [`IsrChopper`] is the reference: plain fields, no atomics, host-testable
/// tick by tick. It cannot go in a `FnMut + Send + 'static` ISR callback,
/// because `&mut IsrChopper` is neither, and this firmware denies `unsafe_code`
/// — so a raw-pointer or `static mut` counter is not available either.
///
/// This is the shape that *is*: three atomics, no `unsafe`, no allocation, and
/// the identical decision. The difference from [`IsrChopper`] is only that the
/// counter is read and written with relaxed-ordered atomics, which is correct
/// because **the ISR is the only writer of the counter** and the control task
/// never reads it except for diagnostics.
///
/// # Why this is not "a second implementation"
///
/// It is the *same* function. [`chopper_tick_level`] does the decision and
/// [`WINDOW_MS`] / [`CHOPPER_STEP_MS`] do the wrap, and both are shared. The only
/// thing that could drift is the wrap arithmetic, so
/// `the_atomic_chopper_and_the_reference_agree_for_a_whole_window` walks both for
/// every duty the chopper can express and asserts they are bit-identical.
#[derive(Debug)]
pub struct AtomicChopper {
    /// Milliseconds into the current window.
    counter_ms: AtomicU32,
    /// The duty, in milliseconds of on-time per window.
    duty_ms: AtomicU32,
    /// The control window. `u32` because it is an atomic; a window above 4.29e9 ms
    /// is 49 days, which the chopper's `processWindowSize()` could be set to and which
    /// would be nonsense, so it is clamped.
    window_ms: AtomicU32,
    /// Whether the ISR may drive the pin.
    armed: AtomicBool,
}

impl Default for AtomicChopper {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicChopper {
    /// A disarmed chopper at counter 0 with no duty.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            counter_ms: AtomicU32::new(0),
            duty_ms: AtomicU32::new(0),
            window_ms: AtomicU32::new(WINDOW_MS),
            armed: AtomicBool::new(false),
        }
    }

    /// Let the ISR drive. Idempotent.
    pub fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }

    /// Stop the ISR driving. Idempotent.
    pub fn disarm(&self) {
        self.armed.store(false, Ordering::SeqCst);
    }

    /// Whether the ISR may drive.
    #[must_use]
    pub fn is_armed(&self) -> bool {
        self.armed.load(Ordering::SeqCst)
    }

    /// Set the duty for the next window and return the counter to 0.
    ///
    /// Called from the control task, never from the ISR — which is why this takes
    /// `&self` and uses [`Ordering::SeqCst`]: the ISR reads [`Self::tick`]'s
    /// inputs with a relaxed load, and a duty that is half-published would show
    /// up as one window of the wrong duty.
    pub fn set_duty(&self, duty: Duty) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a duty is 0..=WINDOW_MS, which is exactly a u32"
        )]
        let millis = duty.raw() as u32;
        self.duty_ms.store(millis, Ordering::SeqCst);
        self.counter_ms.store(0, Ordering::Relaxed);
    }

    /// The duty, in milliseconds.
    #[must_use]
    pub fn duty_ms(&self) -> u32 {
        self.duty_ms.load(Ordering::SeqCst)
    }

    /// The current counter, in milliseconds into the window.
    #[must_use]
    pub fn counter_ms(&self) -> u32 {
        self.counter_ms.load(Ordering::Relaxed)
    }

    /// The control window, in milliseconds.
    #[must_use]
    pub fn window_ms(&self) -> u32 {
        self.window_ms.load(Ordering::SeqCst)
    }

    /// Change the control window, clamping a nonsensical one.
    pub fn set_window_ms(&self, window_ms: u32) {
        let window = window_ms.clamp(CHOPPER_STEP_MS, u32::MAX);
        self.window_ms.store(window, Ordering::SeqCst);
        if self.counter_ms() >= window {
            self.counter_ms.store(0, Ordering::Relaxed);
        }
    }

    /// One ISR tick: decide the level, advance the counter, report it.
    ///
    /// `None` means the chopper is disarmed and the pin is to be left alone.
    ///
    /// This is the entire body of the heater ISR's arithmetic. The device crate's
    /// callback is this call, a branch on the result, and one GPIO write.
    pub fn tick(&self) -> Option<bool> {
        if !self.armed.load(Ordering::SeqCst) {
            return None;
        }
        // Integer comparison, NOT `chopper_tick_level` on `f32`. This runs in
        // interrupt context, and an FPU instruction there is a fatal
        // coprocessor exception on this chip — see
        // [`chopper_tick_level_ms`], which is the argument. Both operands are
        // `u32` and at most `WINDOW_MS`, so nothing is lost by not going
        // through `f32`; the equivalence is walked exhaustively by
        // `an_integer_level_matches_the_f32_reference_for_every_duty_and_counter`.
        //
        // `set_duty` has already truncated the `Duty` to a whole millisecond, so
        // a `NaN` can never reach here either: it would have become 0 there.
        let counter = self.counter_ms.load(Ordering::Relaxed);
        let level = chopper_tick_level_ms(self.duty_ms(), counter);
        let next = counter.saturating_add(CHOPPER_STEP_MS);
        self.counter_ms.store(
            if next >= self.window_ms() { 0 } else { next },
            Ordering::Relaxed,
        );
        Some(level)
    }
}

#[cfg(test)]
mod atomic_chopper_tests {
    use super::*;
    use alloc::vec::Vec;

    /// Walk a whole window of [`AtomicChopper`] ticks.
    fn window(duty: Duty) -> Vec<bool> {
        let chopper = AtomicChopper::new();
        chopper.set_duty(duty);
        chopper.arm();
        (0..CHOPPER_STEPS)
            .map(|_| chopper.tick().unwrap_or(false))
            .collect()
    }

    #[test]
    fn the_atomic_chopper_and_the_reference_agree_for_a_whole_window() {
        // The anti-drift test, and the reason this type is not "a second
        // implementation": for every duty the chopper can express, the two produce
        // bit-identical level sequences for a whole window. If the wrap
        // arithmetic ever drifts, this fails.
        for output in 0..=1_000u16 {
            let duty = Duty::new(f32::from(output));

            let mut reference = IsrChopper::new();
            reference.set_duty(duty);
            reference.arm();
            let expected: Vec<bool> = (0..CHOPPER_STEPS)
                .map(|_| reference.tick().unwrap_or(false))
                .collect();

            assert_eq!(window(duty), expected, "pid_output {output} ms");
        }
    }

    #[test]
    fn a_disarmed_atomic_chopper_drives_nothing() {
        // A default-constructed one must not be able to energise a relay by
        // being ticked — the same property `IsrChopper` has, and the reason
        // `AtomicChopper::new` leaves `armed` false.
        let chopper = AtomicChopper::new();
        assert!(!chopper.is_armed());
        for _ in 0..1_000 {
            assert_eq!(chopper.tick(), None);
        }
        assert_eq!(chopper.counter_ms(), 0, "and the counter does not advance");
    }

    #[test]
    fn the_counter_visits_the_choppers_counter_values() {
        // Same walk as `the_counter_visits_exactly_the_choppers_counter_values`, on
        // the type the ISR actually uses.
        let chopper = AtomicChopper::new();
        chopper.set_duty(Duty::new(0.0));
        chopper.arm();
        let seen: Vec<u32> = (0..CHOPPER_STEPS)
            .map(|_| {
                let at = chopper.counter_ms();
                chopper.tick();
                at
            })
            .collect();
        let expected: Vec<u32> = (0..WINDOW_MS).step_by(CHOPPER_STEP_MS as usize).collect();
        assert_eq!(seen, expected);
    }

    #[test]
    fn setting_a_duty_over_the_transport_restarts_the_window() {
        // The property the ISR depends on: a new duty is picked up whole, not
        // half-way through a window. This is the sequence the transport's
        // `apply` and the ISR produce between them, and it is what makes the gate
        // meaningful — a deadman that trips at counter 500 of a window must stop
        // the heater for the rest of *that* window, not the next one.
        let chopper = AtomicChopper::new();
        chopper.set_duty(Duty::new(1000.0));
        chopper.arm();
        for _ in 0..50 {
            assert_eq!(chopper.tick(), Some(true));
        }
        assert_eq!(chopper.counter_ms(), 500);
        // The gate closes: duty 0.
        chopper.set_duty(Duty::new(0.0));
        assert_eq!(chopper.counter_ms(), 0);
        for _ in 0..50 {
            assert_eq!(chopper.tick(), Some(false), "the heater is off immediately");
        }
    }

    #[test]
    fn arming_and_disarming_take_effect_on_the_next_tick_not_the_current_one() {
        // The armed flag is read once per ISR entry, so a change lands on the next entry.
        // Pinned so the ordering cannot drift into a "check the flag after driving" shape,
        // which would energise the heater for one tick after a deadman trip.
        let chopper = AtomicChopper::new();
        chopper.set_duty(Duty::new(1000.0));
        chopper.arm();
        assert_eq!(chopper.tick(), Some(true));
        chopper.disarm();
        assert_eq!(chopper.tick(), None, "disarm takes effect at once");
        chopper.arm();
        assert_eq!(chopper.tick(), Some(true), "re-arming takes effect at once");
    }

    #[test]
    fn a_ludicrous_window_is_clamped_rather_than_letting_the_relay_latch() {
        // A window the counter cannot reach means the counter never wraps, so the
        // relay would stay at whatever the first tick decided. Clamped.
        let chopper = AtomicChopper::new();
        chopper.set_window_ms(0);
        assert_eq!(chopper.window_ms(), CHOPPER_STEP_MS);
        chopper.set_window_ms(5);
        assert_eq!(chopper.window_ms(), CHOPPER_STEP_MS);
        chopper.set_duty(Duty::new(1000.0));
        chopper.arm();
        assert_eq!(chopper.tick(), Some(true));
        assert_eq!(chopper.counter_ms(), 0, "one window, one tick, wrapped");
    }

    #[test]
    fn a_duty_written_by_the_control_task_is_seen_whole_by_the_isr() {
        // The cross-thread property. `set_duty` is `SeqCst` and `tick` reads with
        // `Relaxed`, which is only sound because `tick` runs on the same core as
        // the timer interrupt and the duty is published before the next tick
        // *entry*. This test cannot prove that ordering -- it can only pin that a
        // duty published between two ticks is the one the next tick uses, which
        // is the observable half of it.
        let chopper = AtomicChopper::new();
        chopper.arm();
        for output in [0u16, 250, 500, 750, 1_000] {
            chopper.set_duty(Duty::new(f32::from(output)));
            let expected = chopper_tick_level(Duty::new(f32::from(output)), 0);
            assert_eq!(chopper.tick(), Some(expected), "pid_output {output}");
        }
    }

    /// The test that licenses [`chopper_tick_level_ms`] in interrupt context.
    ///
    /// `AtomicChopper::tick` compares integers rather than `f32` because an FPU
    /// instruction in a level-1 ISR is a fatal coprocessor exception on the
    /// original ESP32 (see that function's docs). That is only a safe
    /// substitution if it cannot change a single decision, so this walks the
    /// **whole** input space: every duty `0..=WINDOW_MS` at every counter value
    /// the counter can actually hold, `0..WINDOW_MS` in `CHOPPER_STEP_MS`
    /// steps, plus the counters just past the window that a `set_window_ms`
    /// outside the clamp could leave behind.
    ///
    /// 1001 x 103 comparisons. If someone later changes the quantisation, the
    /// window, or either comparison, this fails rather than the chip.
    #[test]
    fn an_integer_level_matches_the_f32_reference_for_every_duty_and_counter() {
        for duty in 0..=WINDOW_MS {
            for counter in (0..WINDOW_MS + CHOPPER_STEP_MS).step_by(CHOPPER_STEP_MS as usize) {
                // `duty` is at most `WINDOW_MS` (1000) and every value in
                // `0..=1000` is exact in an `f32`, so this cast is lossless --
                // the same justification the module's other widenings carry.
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "duty is at most WINDOW_MS (1000), exact in f32"
                )]
                let expected = chopper_tick_level(Duty::new(duty as f32), counter);
                assert_eq!(
                    chopper_tick_level_ms(duty, counter),
                    expected,
                    "duty {duty} ms at counter {counter} ms"
                );
            }
        }
    }

    /// The same equivalence, walked through the real ISR entry point rather
    /// than through the free function.
    ///
    /// [`Self::tick`] is what runs in interrupt context, so proving only the
    /// helper agrees with the reference would leave the actual call site
    /// unverified. This drives `tick` for every duty the machine can ask for and
    /// compares against the `f32` reference for every counter in the window.
    #[test]
    fn the_isr_entry_point_agrees_with_the_f32_reference_for_every_duty() {
        for output in 0..=1_000u16 {
            let chopper = AtomicChopper::new();
            chopper.set_duty(Duty::new(f32::from(output)));
            chopper.arm();
            for step in 0..CHOPPER_STEPS {
                let counter = step * CHOPPER_STEP_MS;
                let expected = chopper_tick_level(Duty::new(f32::from(output)), counter);
                assert_eq!(
                    chopper.tick(),
                    Some(expected),
                    "pid_output {output} ms at counter {counter} ms"
                );
            }
        }
    }
}
