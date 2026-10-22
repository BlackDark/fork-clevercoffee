//! Sensor-fault recovery delay starts at state entry, not fault clearance.
//!
//! A persistent fault can recover immediately once clear. Implausible readings
//! instead enter emergency stop; keep that path distinct from a reported probe fault.

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

/// The recovery delay is measured from **entry**, not from the moment the fault clears.
///
/// While a fault persists the guard answers `SENSOR_ERROR` for a machine already in it, and a
/// self-transition is discarded, so the state's own rules do not run and the entry stamp is never
/// re-written. The consequence is observable: a fault that persists for an hour and then clears
/// recovers on the next tick rather than five seconds later.
///
/// Recovery is safe in the direction that matters: it happens only once the error is genuinely
/// clear, and the guard is re-checked on every tick. The missing delay is a preserved
/// behaviour, pinned by
/// `parity_findings::the_sensor_error_recovery_delay_is_measured_from_entry`.
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

    // The error clears. The stamp was never re-written, so the delay has already elapsed and
    // recovery happens on the next tick rather than five seconds later. See the test's doc comment.
    let _ = h.send(cc_machine::Event::SensorUpdated(Sensors::healthy()));
    let _ = h.elapse_to(10 * 60_000);
    assert_eq!(
        h.state(),
        MachineState::PidNormal,
        "preserved deliberately: the delay is measured from entry, not from the \
         moment the error cleared"
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

/// A persisting error is a **self-transition**: the guard names the state the machine is already
/// in. The hooks must therefore not run every tick, which for `SENSOR_ERROR` would mean re-stamping
/// the recovery clock forever and never recovering.
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

/// Entry stamps the recovery clock.
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

/// The emergency guard beats the sensor-error guard, and the guards are answered before the
/// state's own rules, so a latched machine is routed to the emergency stop and the faulted-probe
/// branch is only reached when it is not latched.
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

/// `SENSOR_ERROR` recovers to the **configured** PID state, not the runtime one.
///
/// The state is excluded from the PID-runtime guard, so a machine sitting there with the runtime
/// PID off is not bounced out; when it recovers it reads the configured flag. A cleared sensor
/// recovers into `PID_NORMAL` even though the runtime PID is off -- and `PID_NORMAL`'s own first
/// check then takes it to `PID_DISABLED` on the *following* tick. Two ticks, not one.
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
        "recovery uses the configured PID flag, not the runtime one"
    );

    // And on the next loop PID_NORMAL's own check notices the runtime flag is off.
    let _ = h.elapse_to(10_001);
    assert_eq!(h.state(), MachineState::PidDisabled);
}

/// `EEPROM_ERROR` recovers to `PID_DISABLED` **unconditionally** once its timeout elapses, rather
/// than to the configured PID state as every other recovery path does. The one recovery that
/// ignores the user's configuration, and pinned for that reason.
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
        "the EEPROM recovery returns PID_DISABLED, not the configured PID state"
    );
}

/// `EEPROM_ERROR` entry forces the runtime PID off.
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

/// An implausible temperature is S1's job, not the state's -- and the S1 verdict routes the machine
/// to `EMERGENCY_STOP`, not here: a fault flag is about the *probe reporting an error*, not about
/// the reading being implausible.
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

/// A latched probe fault stops heating even when its last sample is plausible.
/// Keep this distinct from an implausible reading, which takes the S1 emergency path.
#[test]
fn a_plausible_reading_from_a_faulted_probe_stops_the_heater() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.machine.pid.runtime_enabled = true;
    h.machine.pid.mode_enabled = true;
    h.machine.pid.output = 0.8;

    // The loop's order: the sample arrives, then the tick that judges it. The guard is reached from
    // the tick rather than from the sample, which is the order the control loop uses.
    let _ = h.send(cc_machine::Event::SensorUpdated(Sensors {
        temperature: Celsius::new(95.0),
        has_temperature_error: true,
        ..Sensors::healthy()
    }));
    let fx = h.tick();

    assert_eq!(
        h.state(),
        MachineState::SensorError,
        "a latched probe error is the sensor-error guard, whatever the reading said"
    );
    // Process control's own PID gate excludes `SENSOR_ERROR`, so the duty is zeroed on the same
    // tick the guard fires rather than on a later one.
    assert_eq!(
        h.machine.pid.output.to_bits(),
        0.0f32.to_bits(),
        "the heater must be de-energised in the same tick the fault is seen"
    );
    assert!(
        common::has(&fx, cc_machine::Effect::DisableHeater),
        "{fx:?}"
    );
}
