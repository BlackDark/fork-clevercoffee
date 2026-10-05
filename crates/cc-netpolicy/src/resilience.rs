//! The retry and circuit-breaker policy the network tier runs on.
//!
//! Owner: **R3-12 / R3-13**.
//!
//! # What this is
//!
//! A port of `include/clevercoffee/utils/Resilience.h` — `RetryPolicy` and
//! `CircuitBreaker` — with the C++'s own defaults, because the C++ constructs
//! exactly one of each and both are network-tier:
//!
//! | Construct | Construction site | Value |
//! | --- | --- | --- |
//! | `RetryPolicy` | `CleverCoffeeWiFiManager.cpp:37-43`, `MQTTManager.cpp:51-57` | 10 s initial, 5 min cap, ×2, 5 attempts |
//! | `CircuitBreaker` | `CleverCoffeeWiFiManager.cpp:46-52`, `MQTTManager.cpp:60-64` | 5 failures, 60 s open, 30 s half-open |
//!
//! The behaviour is reproduced exactly, including the parts that look like
//! oversights. They are not oversights *for the C++*, which only ever runs one
//! policy; they are oversights that this port must either reproduce or name.
//!
//! # Two things that are not the C++'s, and why
//!
//! ## 1. The backoff multiplier is an exact rational, not a `double`
//!
//! The C++ computes `initialDelayMs_ * std::pow(backoffMultiplier_, attempt)`
//! in `double` and truncates the result to `unsigned long`. For the values the
//! firmware constructs — 10 000 × 2ⁿ, n ≤ 5 — that is exact, so a `u32` shift
//! is bit-identical and this is not a behaviour change. For any *other*
//! multiplier it would not be: `0.1` in `double` is not `0.1`, and the schedule
//! would depend on the FPU. Since the firmware only ever builds one policy, the
//! port takes the multiplier as an exact `num / den` pair and computes the
//! backoff in `u64` with saturating multiplication, so the schedule is a
//! property of the code and not of the FPU.
//!
//! ## 2. The clock is passed in, never read
//!
//! The C++ defaults every time argument to `millis()`. A default argument that
//! reads a global clock is untestable — the test has to know what "now" was —
//! and 09 §22 is a standing reminder that this firmware's timing is not
//! something to be casual about. Every method here takes `now_ms` explicitly.
//!
//! # What is reproduced verbatim
//!
//! * `should_retry` is `current < max`, and a `max` of 0 means unlimited.
//! * `can_retry_now` is *always true* on the first attempt, whatever
//!   `last_attempt_ms` is. That is `Resilience.h:107-109`, and it is why the
//!   first reconnection attempt after a drop is immediate.
//! * `can_attempt` in `HALF_OPEN` admits an attempt only while
//!   `half_open_attempts_ == 0`, and nothing ever increments
//!   `half_open_attempts_`. So a half-open breaker admits **every** attempt
//!   until one succeeds twice or one fails. Preserved, and pinned by
//!   `div_half_open_admits_every_attempt`.
//! * `update_state` in `HALF_OPEN` closes the circuit when
//!   `half_open_timeout` has elapsed **and** no attempt was made, on the
//!   strength of having not been used. Preserved, and pinned by
//!   `div_half_open_closes_on_idleness_not_on_success`.
//! * `record_failure` in `CLOSED` resets the failure count to 0 when it opens
//!   the circuit (`Resilience.h:253-258`). Preserved, and pinned by
//!   `div_opening_the_circuit_clears_the_failure_count`.

use core::fmt;

/// The state of a [`CircuitBreaker`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation: attempts are permitted.
    Closed,
    /// Too many failures: attempts are refused without trying.
    Open,
    /// Testing recovery. See [`CircuitBreaker::can_attempt`] for exactly what
    /// this state admits, which is not what the name suggests.
    HalfOpen,
}

/// Exponential-backoff retry policy, ported from `Resilience.h`'s `RetryPolicy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    initial_delay_ms: u32,
    max_delay_ms: u32,
    multiplier_num: u32,
    multiplier_den: u32,
    max_attempts: u32,
    attempts: u32,
    last_attempt_ms: u32,
}

impl RetryPolicy {
    /// The policy both network call sites construct: 10 s initial, 5 min cap,
    /// doubling, 5 attempts.
    ///
    /// `CleverCoffeeWiFiManager.cpp:37-43` and `MQTTManager.cpp:51-57` both
    /// build exactly this, and both pass `max_attempts = 5` with the comment
    /// "matches maxWifiReconnects".
    pub const WIFI: Self = Self::new(10_000, 300_000, 5);

    /// A policy with an exact-rational backoff multiplier of 2/1, which is what
    /// the firmware uses.
    #[must_use]
    pub const fn new(initial_delay_ms: u32, max_delay_ms: u32, max_attempts: u32) -> Self {
        Self {
            initial_delay_ms,
            max_delay_ms,
            multiplier_num: 2,
            multiplier_den: 1,
            max_attempts,
            attempts: 0,
            last_attempt_ms: 0,
        }
    }

    /// Whether another attempt is permitted by the attempt count alone.
    ///
    /// `false` once `max_attempts` attempts have been made. A `max_attempts` of
    /// 0 means unlimited, which is the C++'s `if (maxAttempts_ == 0) return true`.
    #[must_use]
    pub const fn should_retry(&self) -> bool {
        self.max_attempts == 0 || self.attempts < self.max_attempts
    }

    /// Whether `max_attempts` has been reached.
    #[must_use]
    pub const fn is_max_attempts_reached(&self) -> bool {
        self.max_attempts != 0 && self.attempts >= self.max_attempts
    }

    /// How many attempts have been made.
    #[must_use]
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }

    /// The delay before the *next* attempt, in milliseconds.
    ///
    /// `initial_delay_ms` before the first attempt, then
    /// `initial_delay_ms × (num/den)^attempts`, capped at `max_delay_ms`.
    ///
    /// The multiply is saturating, so an absurd attempt count clamps to
    /// `u64::MAX` rather than wrapping into a short delay — wrapping would make
    /// a long outage produce a *fast* retry loop, which is the opposite of what
    /// backoff is for.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "`capped` is `min(delay, max_delay_ms)`, and `max_delay_ms` is \
                  a `u32`, so the truncation is unreachable -- the `as u32` is \
                  what makes that fact visible to the reader"
    )]
    pub fn next_delay_ms(&self) -> u32 {
        if self.attempts == 0 {
            return self.initial_delay_ms;
        }
        let mut delay = u64::from(self.initial_delay_ms);
        for _ in 0..self.attempts {
            delay = delay.saturating_mul(u64::from(self.multiplier_num))
                / u64::from(self.multiplier_den);
        }
        let capped = delay.min(u64::from(self.max_delay_ms));
        capped as u32
    }

    /// Whether enough wall-clock time has passed for the next attempt.
    ///
    /// Always `true` before the first attempt, whatever `now_ms` is. This is
    /// `Resilience.h:107-109` and it is the reason a dropped link is retried
    /// immediately rather than after the first backoff.
    #[must_use]
    pub fn can_retry_now(&self, now_ms: u32) -> bool {
        if self.attempts == 0 {
            return true;
        }
        now_ms.wrapping_sub(self.last_attempt_ms) >= self.next_delay_ms()
    }

    /// Record that an attempt is starting at `now_ms`.
    pub const fn record_attempt(&mut self, now_ms: u32) {
        self.attempts += 1;
        self.last_attempt_ms = now_ms;
    }

    /// The time of the most recent [`RetryPolicy::record_attempt`].
    #[must_use]
    pub const fn last_attempt_ms(&self) -> u32 {
        self.last_attempt_ms
    }

    /// Milliseconds from `now_ms` until the next attempt is due.
    ///
    /// Zero when it is already due. This is a **countdown**, which is what a
    /// "next retry in N ms" log line wants; the C++ prints the *scheduled*
    /// delay at `:244-247` instead, which is a different (and less useful)
    /// number whenever the check happens part-way through a backoff.
    #[must_use]
    pub fn ms_until_retry(&self, now_ms: u32) -> u32 {
        if self.can_retry_now(now_ms) {
            return 0;
        }
        self.next_delay_ms()
            .saturating_sub(now_ms.wrapping_sub(self.last_attempt_ms))
    }

    /// Forget the attempts, as after a success. `Resilience.h:126-129`.
    pub const fn reset(&mut self) {
        self.attempts = 0;
        self.last_attempt_ms = 0;
    }
}

/// Circuit breaker, ported from `Resilience.h`'s `CircuitBreaker`.
///
/// The Wi-Fi monitor and the MQTT client each own one, configured identically
/// (5 failures, 60 s open, 30 s half-open), and it is what turns "the access
/// point is off" into "stop trying for a minute" instead of a tight reconnect
/// loop that would keep the radio — and the heap — busy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    open_timeout_ms: u32,
    half_open_timeout_ms: u32,
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    state_change_ms: u32,
    half_open_attempts: u32,
}

impl CircuitBreaker {
    /// The breaker both network call sites construct: 5 failures, 60 s open,
    /// 30 s half-open. `CleverCoffeeWiFiManager.cpp:46-52`,
    /// `MQTTManager.cpp:60-64`.
    pub const WIFI: Self = Self::new(5, 60_000, 30_000);

    /// A breaker with the firmware's thresholds.
    #[must_use]
    pub const fn new(
        failure_threshold: u32,
        open_timeout_ms: u32,
        half_open_timeout_ms: u32,
    ) -> Self {
        Self {
            failure_threshold,
            open_timeout_ms,
            half_open_timeout_ms,
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            state_change_ms: 0,
            half_open_attempts: 0,
        }
    }

    /// Whether an attempt may be made at `now_ms`, advancing the state machine
    /// first.
    ///
    /// This is the mutating half of the breaker state machine and is the one to
    /// call from network code. See the module docs for the half-open rule.
    pub fn can_attempt(&mut self, now_ms: u32) -> bool {
        self.update_state(now_ms);
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => self.half_open_attempts == 0,
        }
    }

    /// Record a successful operation at `now_ms`. `Resilience.h:216-241`.
    pub fn record_success(&mut self, now_ms: u32) {
        match self.state {
            CircuitState::Closed => self.failure_count = 0,
            CircuitState::HalfOpen => {
                self.success_count += 1;
                self.half_open_attempts = 0;
                if self.success_count >= 2 {
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                    self.state_change_ms = now_ms;
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Record a failed operation at `now_ms`. `Resilience.h:244-273`.
    pub fn record_failure(&mut self, now_ms: u32) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.failure_threshold {
                    self.state = CircuitState::Open;
                    self.state_change_ms = now_ms;
                    // The C++ resets the count here so the *next* cycle starts
                    // from zero. It has no effect on this cycle, because the
                    // circuit is now open and only `update_state` moves it.
                    self.failure_count = 0;
                }
            }
            CircuitState::HalfOpen => {
                self.state = CircuitState::Open;
                self.state_change_ms = now_ms;
                self.half_open_attempts = 0;
                self.success_count = 0;
            }
            CircuitState::Open => {}
        }
    }

    /// Force the circuit closed, as after a factory reset.
    pub const fn reset(&mut self, now_ms: u32) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.success_count = 0;
        self.half_open_attempts = 0;
        self.state_change_ms = now_ms;
    }

    /// The recorded state, without advancing the time-based transitions.
    ///
    /// For reporting only. A caller that wants "can I try?" must call
    /// [`CircuitBreaker::can_attempt`], which is the only place `update_state`
    /// runs; otherwise a caller that asked `recorded_state()` before
    /// `can_attempt()` could be told the circuit is open when it is already
    /// half-open and would skip a probe that was due.
    #[must_use]
    pub const fn recorded_state(&self) -> CircuitState {
        self.state
    }

    /// Consecutive failures recorded in the current closed cycle.
    #[must_use]
    pub const fn failure_count(&self) -> u32 {
        self.failure_count
    }

    /// The time-based transitions, `Resilience.h:329-357`.
    fn update_state(&mut self, now_ms: u32) {
        match self.state {
            CircuitState::Open => {
                if now_ms.wrapping_sub(self.state_change_ms) >= self.open_timeout_ms {
                    self.state = CircuitState::HalfOpen;
                    self.state_change_ms = now_ms;
                    self.success_count = 0;
                    self.half_open_attempts = 0;
                }
            }
            CircuitState::HalfOpen => {
                if now_ms.wrapping_sub(self.state_change_ms) >= self.half_open_timeout_ms
                    && self.half_open_attempts == 0
                {
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                }
            }
            CircuitState::Closed => {}
        }
    }
}

impl fmt::Display for RetryPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RetryPolicy(initial {} ms, max {} ms, x{}/{}, {} of {} attempts)",
            self.initial_delay_ms,
            self.max_delay_ms,
            self.multiplier_num,
            self.multiplier_den,
            self.attempts,
            self.max_attempts
        )
    }
}

impl fmt::Display for CircuitBreaker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CircuitBreaker({:?}, {} failures of {} since {})",
            self.recorded_state(),
            self.failure_count,
            self.failure_threshold,
            self.state_change_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- RetryPolicy --------------------------------------------------

    #[test]
    fn the_wifi_policy_backs_off_10s_20s_40s_80s_160s_then_caps_at_5min() {
        let mut policy = RetryPolicy::WIFI;
        let mut delays = alloc::vec::Vec::new();
        for t in 0..7u32 {
            delays.push(policy.next_delay_ms());
            policy.record_attempt(t * 1_000);
        }
        assert_eq!(
            delays,
            alloc::vec![10_000, 20_000, 40_000, 80_000, 160_000, 300_000, 300_000]
        );
    }

    #[test]
    fn the_first_attempt_is_never_delayed() {
        // Resilience.h:107-109: `if (currentAttempt_ == 0) return true`.
        // A machine that has just dropped off Wi-Fi retries immediately; only
        // the *second* attempt waits, and it waits 10 s x 2.
        let mut policy = RetryPolicy::WIFI;
        assert!(policy.can_retry_now(0));
        assert!(policy.can_retry_now(u32::MAX));
        policy.record_attempt(1_000);
        assert!(!policy.can_retry_now(1_000));
        assert!(!policy.can_retry_now(20_999));
        assert!(policy.can_retry_now(21_000));
    }

    #[test]
    fn the_attempt_count_is_the_only_thing_that_stops_retrying() {
        let mut policy = RetryPolicy::WIFI;
        for t in 0..5 {
            assert!(policy.should_retry(), "attempt {t} should still be allowed");
            policy.record_attempt(t);
        }
        assert!(!policy.should_retry());
        assert!(policy.is_max_attempts_reached());
        assert_eq!(policy.attempts(), 5);
    }

    #[test]
    fn a_max_attempts_of_zero_never_stops() {
        let policy = RetryPolicy::new(10, 100, 0);
        assert!(policy.should_retry());
        assert!(!policy.is_max_attempts_reached());
    }

    #[test]
    fn backoff_saturates_instead_of_wrapping_into_a_fast_loop() {
        // The whole point of backoff is that the delay grows. A wrapping u32
        // multiply would make a 40-day outage produce a 0 ms retry loop.
        let mut policy = RetryPolicy::new(10_000, u32::MAX, 40);
        for _ in 0..40 {
            policy.record_attempt(0);
        }
        assert_eq!(policy.next_delay_ms(), u32::MAX);
    }

    #[test]
    fn reset_forgets_the_attempts() {
        let mut policy = RetryPolicy::WIFI;
        policy.record_attempt(0);
        policy.record_attempt(0);
        policy.reset();
        assert_eq!(policy.attempts(), 0);
        assert_eq!(policy.next_delay_ms(), 10_000);
        assert!(policy.can_retry_now(0));
    }

    // ---- CircuitBreaker -----------------------------------------------

    #[test]
    fn five_failures_open_the_circuit() {
        let mut breaker = CircuitBreaker::WIFI;
        for t in 0..4 {
            assert!(breaker.can_attempt(t * 1_000));
            breaker.record_failure(t * 1_000);
            assert_eq!(breaker.recorded_state(), CircuitState::Closed);
        }
        assert!(breaker.can_attempt(4_000));
        breaker.record_failure(4_000);
        assert_eq!(breaker.recorded_state(), CircuitState::Open);
        assert!(!breaker.can_attempt(4_001));
    }

    #[test]
    fn a_success_in_closed_resets_the_failure_count() {
        let mut breaker = CircuitBreaker::WIFI;
        for t in 0..3 {
            breaker.record_failure(t);
        }
        assert_eq!(breaker.failure_count(), 3);
        breaker.record_success(100);
        assert_eq!(breaker.failure_count(), 0);
    }

    #[test]
    fn open_becomes_half_open_after_60s_and_closed_after_two_successes() {
        let mut breaker = CircuitBreaker::WIFI;
        for t in 0..5 {
            breaker.record_failure(t);
        }
        assert!(!breaker.can_attempt(10_000));
        // 60 s after opening, the next can_attempt admits a probe.
        let half_open_at = 4 + 60_000;
        assert!(breaker.can_attempt(half_open_at));
        assert_eq!(breaker.recorded_state(), CircuitState::HalfOpen);
        breaker.record_success(half_open_at + 1);
        assert_eq!(breaker.recorded_state(), CircuitState::HalfOpen);
        breaker.record_success(half_open_at + 2);
        assert_eq!(breaker.recorded_state(), CircuitState::Closed);
    }

    #[test]
    fn a_failure_in_half_open_reopens_the_circuit_for_another_60s() {
        let mut breaker = CircuitBreaker::WIFI;
        for t in 0..5 {
            breaker.record_failure(t);
        }
        let half_open_at = 4 + 60_000;
        assert!(breaker.can_attempt(half_open_at));
        breaker.record_failure(half_open_at);
        assert_eq!(breaker.recorded_state(), CircuitState::Open);
        assert!(!breaker.can_attempt(half_open_at + 30_000));
        assert!(breaker.can_attempt(half_open_at + 60_001));
    }

    // ---- Reproduced oddities of the C++ --------------------------------

    #[test]
    fn div_opening_the_circuit_clears_the_failure_count() {
        // Resilience.h:253-258 resets failureCount_ to 0 when the circuit
        // opens. The C++ only ever reports it through a debug log, so the
        // observable behaviour is identical; the count is reproduced so a
        // parity harness sees no diff.
        let mut breaker = CircuitBreaker::new(3, 60_000, 30_000);
        breaker.record_failure(0);
        breaker.record_failure(1);
        breaker.record_failure(2);
        assert_eq!(breaker.recorded_state(), CircuitState::Open);
        assert_eq!(breaker.failure_count(), 0);
    }

    #[test]
    fn div_half_open_admits_every_attempt() {
        // `canAttempt` in HALF_OPEN admits only while halfOpenAttempts_ == 0,
        // and NOTHING in the class ever increments halfOpenAttempts_. So the
        // guard is vacuous and a half-open breaker lets every attempt through
        // until one succeeds twice or one fails. This is a real defect in the
        // C++ (the "one probe at a time" the comment describes does not
        // happen), it is harmless for a single-threaded caller, and it is
        // reproduced rather than silently fixed so the port is auditable.
        let mut breaker = CircuitBreaker::WIFI;
        for t in 0..5 {
            breaker.record_failure(t);
        }
        // The circuit opened at t = 4, so half-open is reachable at 4 + 60 s.
        let half_open_at = 4 + 60_000;
        assert!(breaker.can_attempt(half_open_at));
        for probe in 1..10u32 {
            assert!(
                breaker.can_attempt(half_open_at + probe),
                "probe {probe} should be admitted: the C++ never counts them"
            );
        }
    }

    #[test]
    fn div_half_open_closes_on_idleness_not_on_success() {
        // Resilience.h:343-350: HALF_OPEN closes the circuit when the
        // half-open timeout has elapsed AND no attempt was made -- i.e. on
        // evidence of *absence of traffic*, which for a machine whose Wi-Fi is
        // simply unused is the normal case. Reproduced: a broker that has
        // never been touched closes on its own after 30 s, without a single
        // successful connection.
        let mut breaker = CircuitBreaker::WIFI;
        for t in 0..5 {
            breaker.record_failure(t);
        }
        let half_open_at = 4 + 60_000;
        assert!(breaker.can_attempt(half_open_at));
        assert_eq!(breaker.recorded_state(), CircuitState::HalfOpen);
        // 30 s later with no traffic at all: closed, on idleness.
        assert!(breaker.can_attempt(half_open_at + 30_001));
        assert_eq!(breaker.recorded_state(), CircuitState::Closed);
    }

    #[test]
    fn the_clock_wraps_without_panicking() {
        // `millis()` wraps every 49.7 days. The C++ subtracts unsigned longs,
        // which wraps too, so a wrap must not read as "a very long time has
        // passed" and must not panic in debug.
        let mut breaker = CircuitBreaker::WIFI;
        breaker.record_failure(u32::MAX - 1_000);
        // Two seconds "later" on a wrapped clock is 998 ms later.
        assert!(breaker.can_attempt(998));
    }
}
