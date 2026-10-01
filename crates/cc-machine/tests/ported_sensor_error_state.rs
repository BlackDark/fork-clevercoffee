//! Port of `test/test_sensor_error_state` — 4 C++ cases, 4 Rust cases.
//!
//! These are regression tests for "Bug #1": a 60-second timeout in
//! `SensorErrorState` used to move the machine to `PID_DISABLED` with no user
//! action. The fix removed the timeout and replaced it with a recovery delay
//! that starts when the error *clears*, not when the state is entered
//! (`ErrorStates.cpp:47-50`).
//!
//! ## The C++ suite's stubs decide the outcome
//!
//! `test_sensor_error_state` runs with `HandlerTestStubs`, whose
//! `hasSensorError()` and `hasTemperatureError()` both return **false** — so
//! every case exercises the *recovered* branch, and the persisting-error branch
//! is only reached by the sub-assertion that within the delay window nothing
//! happens. The port makes both branches explicit instead of relying on which
//! way a stub happens to point.
//!
//! ## Case mapping
//!
//! | C++ case (`test_sensor_error_state/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `NeverTransitionsToPidDisabledOnTimeout` (:92) | [`never_transitions_to_pid_disabled_on_timeout`] |
//! | `StaysInSensorErrorWithinRecoveryDelay` (:118) | [`stays_in_sensor_error_within_the_recovery_delay`] |
//! | `RecoversToPidNormalAfterDelayWhenErrorClears` (:137) | [`recovers_to_pid_normal_after_the_delay_when_the_error_clears`] |
//! | `RecoversToPidDisabledWhenUserHadDisabledPid` (:160) | [`recovers_to_pid_disabled_when_the_user_had_disabled_pid`] |

mod common;

use cc_domain::state::MachineState;
use cc_domain::units::Celsius;
use cc_machine::{timing, Sensors};
use common::Harness;

/// A sample with a faulted temperature probe.
fn probe_faulted() -> Sensors {
    Sensors {
        has_temperature_error: true,
        ..Sensors::healthy()
    }
}

/// A machine in `SENSOR_ERROR` with the error already stamped at `t = 0`.
fn entered(now: u32) -> Harness {
    let mut h = Harness::in_state(MachineState::SensorError);
    h.now = now;
    h.machine.now = cc_domain::units::Millis::new(now);
    let _ = h.on_entry(MachineState::SensorError);
    h
}

#[test]
fn never_transitions_to_pid_disabled_on_timeout() {
    let mut h = entered(0);
    h.config.pid.enabled = true;

    // The fault clears and we are far past both the old 60 s timeout and the
    // 5 s recovery delay.
    let _ = h.send(cc_machine::Event::SensorUpdated(Sensors::healthy()));
    let _ = h.elapse_to(120_000);

    assert_eq!(
        h.state(),
        MachineState::PidNormal,
        "should have transitioned after the recovery delay, to PID_NORMAL"
    );
}

#[test]
fn stays_in_sensor_error_within_the_recovery_delay() {
    let mut h = entered(0);
    let _ = h.send(cc_machine::Event::SensorUpdated(Sensors::healthy()));
    let _ = h.elapse_to(500);

    assert_eq!(
        h.state(),
        MachineState::SensorError,
        "SENSOR_ERROR should not transition before the recovery delay expires"
    );
}

#[test]
fn recovers_to_pid_normal_after_the_delay_when_the_error_clears() {
    let mut h = entered(0);
    h.config.pid.enabled = true;
    let _ = h.send(cc_machine::Event::SensorUpdated(Sensors::healthy()));
    let _ = h.elapse_to(6_000);

    assert_eq!(h.state(), MachineState::PidNormal);
}

/// # Preserved deliberately, and a new finding: the recovery clock is **not**
/// reset when the error persists
///
/// `SensorErrorState::checkSpecificTransitions` says:
///
/// ```cpp
/// } else {
///     // Error still present - keep resetting the clock so recovery delay
///     // is measured from when the error actually clears, not from entry.
///     errorStartTime_ = millis();
/// }
/// ```
/// (`ErrorStates.cpp:47-50`)
///
/// **That branch is unreachable.** `BaseState::checkTransitions` tests
/// `hasSensorError()` with **no exclusion** (`BaseState.h:145-148`), so while the
/// probe is faulted the guard returns `SENSOR_ERROR` (a discarded
/// self-transition) and `checkSpecificTransitions` is never called. The
/// clock reset never runs, and the recovery delay is measured from **entry**.
///
/// The observable consequence: a probe fault that persists for an hour and then
/// clears does **not** wait five seconds — the machine recovers on the very next
/// loop. That is safe in the direction that matters (recovery only happens once
/// the error is genuinely clear, and the guard re-checks it every loop), but it
/// is not what the comment says, and it is what the C++ does.
///
/// The C++ has no test for this at all — `test_sensor_error_state`'s stubs always
/// report the error clear — so it was never noticed. Preserved here, and pinned.
#[test]
fn a_persisting_error_does_not_postpone_recovery() {
    let mut h = entered(0);
    h.config.pid.enabled = true;

    // Ten minutes with the probe still faulted.
    for minute in 1..=10 {
        let _ = h.send(cc_machine::Event::SensorUpdated(probe_faulted()));
        let _ = h.elapse_to(minute * 60_000);
        assert_eq!(h.state(), MachineState::SensorError, "minute {minute}");
    }

    // The error clears. Because the clock was never reset, the delay has already
    // elapsed, so recovery happens on the very next loop — not five seconds
    // later. This is the *preserved* C++ behaviour; see the test's doc comment.
    let _ = h.send(cc_machine::Event::SensorUpdated(Sensors::healthy()));
    let _ = h.elapse_to(10 * 60_000);
    assert_eq!(
        h.state(),
        MachineState::PidNormal,
        "preserved deliberately: the delay is measured from entry, not from the \
         moment the error cleared, because ErrorStates.cpp:49 is unreachable"
    );
}

#[test]
fn recovers_to_pid_disabled_when_the_user_had_disabled_pid() {
    let mut h = entered(0);
    h.config.pid.enabled = false;
    let _ = h.send(cc_machine::Event::SensorUpdated(Sensors::healthy()));
    let _ = h.elapse_to(6_000);

    assert_eq!(
        h.state(),
        MachineState::PidDisabled,
        "should recover to PID_DISABLED when the user had disabled PID"
    );
}

/// A persisting error is a **self-transition**, because the sensor-error guard
/// has no exclusion (`BaseState.h:145-148`). The state machine must therefore
/// not run `onExit`/`onEntry` every loop — which for `SENSOR_ERROR` would mean
/// re-stamping `error_since` forever and never recovering.
#[test]
fn a_persisting_error_is_a_discarded_self_transition() {
    let mut h = entered(0);
    let _ = h.send(cc_machine::Event::SensorUpdated(probe_faulted()));

    let fx = h.elapse_to(1_000);
    assert_eq!(h.state(), MachineState::SensorError);
    assert_eq!(
        common::count(
            &fx,
            cc_machine::Effect::EnterState(MachineState::SensorError)
        ),
        0,
        "the self-transition must be discarded, or error_since would never advance: {fx:?}"
    );
}

/// Entry stamps the recovery clock, which the C++ gets for free because a
/// transition builds a fresh state object (`StateFactory.cpp:24`).
#[test]
fn entry_stamps_the_recovery_clock() {
    let mut h = Harness::in_state(MachineState::SensorError);
    h.now = 4_000;
    h.machine.now = cc_domain::units::Millis::new(4_000);
    h.machine.error_since = None;

    let _ = h.on_entry(MachineState::SensorError);
    assert_eq!(
        h.machine.error_since,
        Some(cc_domain::units::Millis::new(4_000))
    );

    // And exit clears it, so a stale one can never be read.
    let _ = h.on_exit(MachineState::SensorError);
    assert_eq!(h.machine.error_since, None);
}

/// The emergency guard beats the sensor-error guard (`BaseState.h:139-148`,
/// emergency first) and the C++'s own `isEmergencyStop` check inside
/// `SensorErrorState::checkSpecificTransitions` is therefore unreachable.
#[test]
fn emergency_wins_over_the_sensor_error_guard() {
    let mut h = entered(0);
    let _ = h.send(cc_machine::Event::SensorUpdated(probe_faulted()));
    let latched = cc_machine::Event::Safety(cc_safety::Outcome {
        state: cc_safety::SafetyState {
            last_sample_seq: None,
            latched: true,
            high_reading_count: cc_safety::DEBOUNCE_COUNT,
        },
        verdict: cc_safety::Verdict {
            may_heat: false,
            may_pump: false,
            may_open_water: false,
            may_open_steam: false,
            latched: true,
            reason: None,
        },
    });
    let _ = h.send(latched);
    assert_eq!(
        h.next_state_this_tick(),
        Some(MachineState::EmergencyStop),
        "emergency is the first guard and has no exclusion"
    );
}

/// `SENSOR_ERROR` recovers to the **config** PID state, not the runtime one.
///
/// `SENSOR_ERROR` is excluded from the PID-runtime guard (`BaseState.h:165`), so
/// a machine sitting there with the runtime PID off is not bounced out; when it
/// recovers it uses `getPidState()` (`ErrorStates.cpp:44`), which reads
/// `config.pidEnabled` and not the runtime flag. So a sensor error that is
/// cleared recovers into `PID_NORMAL` even though the runtime PID is off — and
/// `PID_NORMAL`'s own first check then immediately takes it to `PID_DISABLED` on
/// the *following* loop. Two loops, not one.
#[test]
fn a_sensor_error_recovers_to_the_config_pid_state_not_the_runtime_one() {
    let mut h = entered(0);
    h.config.pid.enabled = true;
    h.machine.pid.runtime_enabled = false;
    let _ = h.send(cc_machine::Event::SensorUpdated(Sensors::healthy()));
    let _ = h.elapse_to(10_000);
    assert_eq!(
        h.state(),
        MachineState::PidNormal,
        "recovery uses getPidState(), which reads the config, not the runtime flag"
    );

    // And on the next loop PID_NORMAL's own check notices the runtime flag is off.
    let _ = h.elapse_to(10_001);
    assert_eq!(h.state(), MachineState::PidDisabled);
}

/// `EEPROM_ERROR` recovers to `PID_DISABLED` **unconditionally** after five
/// minutes — not to `getPidState()` as every other recovery path does
/// (`ErrorStates.cpp:121`). Pinned because it is the one recovery that ignores
/// the user's configuration.
#[test]
fn eeprom_error_recovers_to_pid_disabled_even_when_pid_is_enabled() {
    let mut h = Harness::in_state(MachineState::EepromError);
    h.config.pid.enabled = true;
    h.now = 0;
    h.machine.now = cc_domain::units::Millis::new(0);
    let _ = h.on_entry(MachineState::EepromError);

    let _ = h.elapse_to(timing::EEPROM_RECOVERY_TIMEOUT_MS);
    assert_eq!(h.state(), MachineState::EepromError, "not yet");

    let _ = h.elapse(1);
    assert_eq!(
        h.state(),
        MachineState::PidDisabled,
        "ErrorStates.cpp:121 returns PID_DISABLED, not getPidState()"
    );
}

/// `EEPROM_ERROR` entry forces the runtime PID off (`ErrorStates.cpp:97`).
#[test]
fn eeprom_error_entry_forces_the_runtime_pid_off() {
    let mut h = Harness::in_state(MachineState::EepromError);
    h.machine.pid.runtime_enabled = true;
    let fx = h.on_entry(MachineState::EepromError);
    assert!(
        common::has(&fx, cc_machine::Effect::SetPidRuntime { enabled: false }),
        "{fx:?}"
    );
    assert!(!h.machine.pid.runtime_enabled);
}

/// An implausible temperature is S1's job, not the state's — and the S1 verdict
/// routes the machine to `EMERGENCY_STOP`, not to `SENSOR_ERROR`, because
/// `hasSensorError()` is about the *probe reporting an error*, not about the
/// reading being implausible.
#[test]
fn an_implausible_reading_goes_to_emergency_stop_not_sensor_error() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.send(cc_machine::Event::SensorUpdated(Sensors {
        temperature: Celsius::new(222.0),
        ..Sensors::healthy()
    }));

    // The S1 verdict, as `cc_safety::reduce` produces it for a reading outside
    // [0, 200].
    let outcome = cc_safety::reduce(
        &cc_safety::SafetyState::CLEAR,
        &cc_safety::Telemetry::new(Celsius::new(222.0), true, MachineState::PidNormal),
        &cc_safety::SafetyConfig::default(),
        cc_domain::units::Millis::new(0),
    );
    assert!(outcome.state.latched);
    let _ = h.send(cc_machine::Event::Safety(outcome));
    assert_eq!(h.next_state_this_tick(), Some(MachineState::EmergencyStop));
}
