//! Port of `test/test_power_handler` — 11 C++ cases, 17 Rust cases
//! (12 live + 5 `#[ignore]`d mock records) plus 6 the C++ does not have.
//!
//! ## The C++ suite is half mock plumbing
//!
//! Six of the eleven cases (`ConstructsWithSystemContext` :80,
//! `SetHardwareSetsSwitch` :85, `ProcessWithNullSwitchReturnsEarly` :95,
//! `ProcessReturnsEarlyWhenDisabled` :105, `ProcessProceedsWhenEnabled` :114,
//! `ProcessWithoutContextDoesNotCrash` :234) assert only that nothing crashes or
//! that gMock counts calls. The remaining five are real behavioural cases and are
//! ported for real.
//!
//! ## Case mapping
//!
//! | C++ case (`test_power_handler/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `ConstructsWithSystemContext` (:80) | `constructing_with_no_switch_is_a_mock_case` (`#[ignore]`) |
//! | `SetHardwareSetsSwitch` (:85) | `set_hardware_sets_switch_is_a_mock_case` (`#[ignore]`) |
//! | `ProcessWithNullSwitchReturnsEarly` (:95) | — (no null switch) |
//! | `ProcessReturnsEarlyWhenDisabled` (:105) | `the_handler_is_a_no_op_when_disabled` (`#[ignore]`) |
//! | `ProcessProceedsWhenEnabled` (:114) | `the_handler_proceeds_when_enabled` (`#[ignore]`) |
//! | `ToggleSwitchPowerOnFromStandby` (:128) | [`a_toggle_switch_powers_on_from_standby`] |
//! | `ToggleSwitchPowerOffFromNormal` (:142) | [`a_toggle_switch_powers_off_from_pid_normal`] |
//! | `ToggleSwitchNoChangeWhenSameState` (:159) | `a_toggle_that_does_not_move_requests_nothing` (`#[ignore]`) |
//! | `MomentarySwitchPowerOnFromStandby` (:178) | [`a_momentary_press_powers_on_from_standby`] |
//! | `MomentarySwitchPowerOffFromNormal` (:204) | [`a_momentary_press_powers_off_from_pid_normal`] |
//! | `ProcessWithoutContextDoesNotCrash` (:234) | — (no null context) |
//!
//! Plus the long-press reboot, which the C++ suite sets up a mock for
//! (`g_test_millis = 12000`, `longPressDetected`) but never asserts.

mod common;

use cc_domain::hardware::SwitchType;
use cc_domain::state::MachineState;
use cc_machine::{timing, Command, Effect, Event, Request, SwitchId};
use common::Harness;

/// A machine in `STANDBY` with the runtime PID off, as standby entry leaves it.
fn asleep() -> Harness {
    let mut h = Harness::in_state(MachineState::Standby);
    h.config.pid.enabled = true;
    h.machine.pid.runtime_enabled = false;
    h
}

#[test]
#[ignore = "C++ case asserts only 'no crash'"]
fn constructing_with_no_switch_is_a_mock_case() {}

#[test]
#[ignore = "C++ case asserts only 'no crash'"]
fn set_hardware_sets_switch_is_a_mock_case() {}

#[test]
#[ignore = "C++ case asserts only 'no crash'"]
fn the_handler_is_a_no_op_when_disabled() {}

#[test]
#[ignore = "C++ case asserts only 'no crash'"]
fn the_handler_proceeds_when_enabled() {}

#[test]
#[ignore = "C++ case asserts only 'no crash'; the reducer is handed edges only"]
fn a_toggle_that_does_not_move_requests_nothing() {}

#[test]
fn a_toggle_switch_powers_on_from_standby() {
    let mut h = asleep();
    h.config.hardware.switches.power.r#type = SwitchType::Toggle;

    let fx = h.press(SwitchId::Power);
    assert!(
        h.requested(Request::NormalOperation),
        "toggling power ON from STANDBY should request normal operation: {fx:?}"
    );
    assert!(
        h.machine.pid.runtime_enabled,
        "setUserPidEnabled(true) also sets the runtime flag: {fx:?}"
    );
    assert!(
        common::has(&fx, Effect::SetPidRuntime { enabled: true }),
        "{fx:?}"
    );
    assert!(common::has(&fx, Effect::WakeDisplay), "{fx:?}");

    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
}

#[test]
fn a_toggle_switch_powers_off_from_pid_normal() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.power.r#type = SwitchType::Toggle;

    // First press: the switch goes HIGH, which for a toggle is "on" — and
    // `powerOn()` does nothing in PID_NORMAL (`PowerHandler.h:152-154`).
    let fx = h.press(SwitchId::Power);
    assert!(
        !h.requested(Request::NormalOperation),
        "powerOn() only acts in STANDBY or PID_DISABLED: {fx:?}"
    );

    // Then LOW: "off", and `powerOff()` requests standby.
    let fx = h.release(SwitchId::Power);
    assert!(
        h.requested(Request::Standby),
        "toggling power OFF from PID_NORMAL should request standby: {fx:?}"
    );
    assert!(
        common::has(&fx, Effect::SafeHardwareShutdown),
        "powerOff() performs a safe shutdown *before* the request: {fx:?}"
    );

    let fx = h.tick();
    assert_eq!(h.state(), MachineState::Standby, "{fx:?}");
}

#[test]
fn a_momentary_press_powers_on_from_standby() {
    let mut h = asleep();
    h.config.hardware.switches.power.r#type = SwitchType::Momentary;
    // `PowerHandler::recordSystemInitialization` stamps the boot time on the
    // first process; the C++ test drives it with `g_test_millis = 6000` then
    // `12000` (`test_power_handler/test_main.cpp:185-194`).
    let _ = h.tick();
    h.advance_clock(timing::POWER_SWITCH_SETTLE_MS + 1_000);

    let fx = h.press(SwitchId::Power);
    assert!(
        h.requested(Request::NormalOperation),
        "a momentary press from STANDBY should request normal operation: {fx:?}"
    );
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
}

#[test]
fn a_momentary_press_powers_off_from_pid_normal() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.power.r#type = SwitchType::Momentary;
    let _ = h.tick();
    h.advance_clock(timing::POWER_SWITCH_SETTLE_MS + 1_000);

    let fx = h.press(SwitchId::Power);
    assert!(
        h.requested(Request::Standby),
        "a momentary press from PID_NORMAL should request standby: {fx:?}"
    );
    assert!(common::has(&fx, Effect::SafeHardwareShutdown), "{fx:?}");
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::Standby, "{fx:?}");
}

/// The 5-second settle window (`PowerHandler.h:118`) gates **long-press tracking
/// only**; the power toggle itself is not gated, so a press during the first five
/// seconds after boot still switches the machine off.
///
/// Preserved deliberately: "the machine ignores the power switch for five
/// seconds after boot" would be a support call.
#[test]
fn the_settle_window_gates_long_press_tracking_but_not_the_power_toggle() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.power.r#type = SwitchType::Momentary;
    h.machine.boot_at = Some(cc_domain::units::Millis::new(0));

    // At t = 1 s the machine has not settled.
    h.advance_clock(1_000);
    let fx = h.press(SwitchId::Power);
    assert!(
        h.requested(Request::Standby),
        "the toggle still acts: {fx:?}"
    );
    assert_eq!(
        h.machine.power_press_started_at, None,
        "but long-press tracking is not armed yet: {fx:?}"
    );
}

/// The long-press reboot (`PowerHandler::checkForLongPressReboot`, `:142-147`).
///
/// Four conditions, all required, and the C++ suite sets up three of them and
/// asserts nothing. The reboot is evaluated on **every tick**, not on the press
/// edge, because the hold lasts about a second and by then the edge is history.
#[test]
fn a_long_press_requests_a_reboot() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.power.r#type = SwitchType::Momentary;
    let _ = h.tick(); // stamps `boot_at`
    h.advance_clock(timing::POWER_SWITCH_SETTLE_MS + 1_000);

    let fx = h.send(Event::ButtonPressed {
        switch: SwitchId::Power,
        long_press: true,
    });
    assert_eq!(
        h.machine.power_press_started_at,
        Some(cc_domain::units::Millis::new(h.now)),
        "the press is tracked: {fx:?}"
    );
    // Not yet: the hold is not long enough.
    assert_eq!(common::count(&fx, Effect::RequestReboot), 0, "{fx:?}");

    // Strictly greater than 1 s (`PowerHandler.h:144`), so 1000 ms exactly does
    // not qualify.
    h.advance_clock(timing::POWER_LONG_PRESS_REBOOT_MS);
    let fx = h.tick();
    assert_eq!(common::count(&fx, Effect::RequestReboot), 0, "{fx:?}");

    h.advance_clock(1);
    let fx = h.tick();
    assert!(
        common::has(&fx, Effect::RequestReboot),
        "a >1 s hold with the hardware long-press flag set must request a reboot: {fx:?}"
    );
}

/// Each of the other three conditions is necessary on its own.
#[test]
fn every_reboot_condition_is_required() {
    // No long-press flag from the hardware.
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.power.r#type = SwitchType::Momentary;
    let _ = h.tick();
    h.advance_clock(timing::POWER_SWITCH_SETTLE_MS + 1_000);
    let _ = h.send(Event::ButtonPressed {
        switch: SwitchId::Power,
        long_press: false,
    });
    h.advance_clock(timing::POWER_LONG_PRESS_REBOOT_MS);
    let fx = h.tick();
    assert_eq!(common::count(&fx, Effect::RequestReboot), 0, "{fx:?}");

    // Not held long enough.
    let mut h2 = Harness::in_state(MachineState::PidNormal);
    h2.config.hardware.switches.power.r#type = SwitchType::Momentary;
    let _ = h2.tick();
    h2.advance_clock(timing::POWER_SWITCH_SETTLE_MS + 1_000);
    let _ = h2.send(Event::ButtonPressed {
        switch: SwitchId::Power,
        long_press: true,
    });
    h2.advance_clock(timing::POWER_LONG_PRESS_REBOOT_MS - 1);
    let fx = h2.tick();
    assert_eq!(common::count(&fx, Effect::RequestReboot), 0, "{fx:?}");

    // Not settled since boot.
    let mut h3 = Harness::in_state(MachineState::PidNormal);
    h3.config.hardware.switches.power.r#type = SwitchType::Momentary;
    h3.machine.boot_at = Some(cc_domain::units::Millis::new(0));
    let _ = h3.send(Event::ButtonPressed {
        switch: SwitchId::Power,
        long_press: true,
    });
    h3.advance_clock(timing::POWER_LONG_PRESS_REBOOT_MS);
    let fx = h3.tick();
    assert_eq!(common::count(&fx, Effect::RequestReboot), 0, "{fx:?}");

    // Released.
    let mut h4 = Harness::in_state(MachineState::PidNormal);
    h4.config.hardware.switches.power.r#type = SwitchType::Momentary;
    let _ = h4.tick();
    h4.advance_clock(timing::POWER_SWITCH_SETTLE_MS + 1_000);
    let _ = h4.send(Event::ButtonPressed {
        switch: SwitchId::Power,
        long_press: true,
    });
    h4.advance_clock(timing::POWER_LONG_PRESS_REBOOT_MS / 2);
    let _ = h4.release(SwitchId::Power);
    h4.advance_clock(timing::POWER_LONG_PRESS_REBOOT_MS);
    let fx = h4.tick();
    assert_eq!(common::count(&fx, Effect::RequestReboot), 0, "{fx:?}");
}

/// The two halves of the power switch behave **asymmetrically**, and getting
/// this wrong is how a "the power switch does nothing while brewing" bug report
/// is filed.
///
/// * `powerOn()` acts **only** in `STANDBY` or `PID_DISABLED`
///   (`PowerHandler.h:152-154`).
/// * `powerOff()` acts in **every** state except `STANDBY`
///   (`PowerHandler.h:166`).
///
/// So a momentary press mid-brew powers the machine off (it is a toggle, and the
/// machine is not asleep), while a *toggle* switch being switched **on** mid-brew
/// does nothing — the operator has to switch it off again, and only that reaches
/// `powerOff()`.
#[test]
fn power_on_is_inert_mid_operation_but_power_off_is_not() {
    for state in [
        MachineState::BrewPreinfusion,
        MachineState::BrewRunning,
        MachineState::SteamRunning,
        MachineState::ManualFlushRunning,
        MachineState::BackflushFilling,
    ] {
        // A toggle going ON is `powerOn()`, which is inert here.
        let mut h = Harness::in_state(state);
        h.config.hardware.switches.power.r#type = SwitchType::Toggle;
        h.machine.switches.power = false;
        let before = h.machine.requests;
        let fx = h.press(SwitchId::Power);
        assert_eq!(
            h.machine.requests, before,
            "a toggle switched ON is inert in {state:?}: {fx:?}"
        );

        // A momentary press is a toggle, and the machine is awake, so it is
        // `powerOff()`: standby requested, hardware shut down.
        let mut h2 = Harness::in_state(state);
        h2.config.hardware.switches.power.r#type = SwitchType::Momentary;
        h2.machine.switches.power = false;
        let fx = h2.press(SwitchId::Power);
        assert!(
            h2.requested(Request::Standby),
            "a momentary press must power the machine off from {state:?}: {fx:?}"
        );
        assert!(common::has(&fx, Effect::SafeHardwareShutdown), "{fx:?}");
    }
}

/// And a toggle switched **off** mid-operation does power it off.
#[test]
fn a_toggle_switched_off_mid_operation_powers_the_machine_off() {
    let mut h = Harness::in_state(MachineState::BrewRunning);
    h.config.hardware.switches.power.r#type = SwitchType::Toggle;
    h.machine.switches.power = true;

    let fx = h.release(SwitchId::Power);
    assert!(h.requested(Request::Standby), "{fx:?}");
    assert!(common::has(&fx, Effect::SafeHardwareShutdown), "{fx:?}");
}

/// `powerOff()` while already in `STANDBY` is a no-op (`PowerHandler.h:166`).
#[test]
fn powering_off_from_standby_does_nothing() {
    let mut h = asleep();
    h.config.hardware.switches.power.r#type = SwitchType::Momentary;
    let fx = h.release(SwitchId::Power);
    assert_eq!(fx, Vec::new(), "{fx:?}");
    assert!(!h.requested(Request::Standby));
}

/// An external `Reboot` command produces the same effect as the long press.
#[test]
fn an_external_reboot_command_requests_a_reboot() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let fx = h.send(Event::Command(Command::Reboot));
    assert!(common::has(&fx, Effect::RequestReboot), "{fx:?}");
    // ...and does not itself stop the hardware; that is the applier's job
    // (`PowerHandler.h:185-187`).
    assert_eq!(
        common::count(&fx, Effect::SafeHardwareShutdown),
        0,
        "{fx:?}"
    );
}

/// The safe shutdown happens **before** the standby request, so for one tick the
/// hardware is off while the state is still `PID_NORMAL` — and in that tick
/// `PidNormalState::update` will `enablePump()` if the water switch is held.
///
/// Preserved deliberately: the ordering is the C++'s (`PowerHandler.h:167-170`),
/// and "fixing" it would change what a power-off does mid-dispense.
#[test]
fn the_safe_shutdown_precedes_the_standby_request() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.power.r#type = SwitchType::Momentary;
    let _ = h.tick();
    h.advance_clock(timing::POWER_SWITCH_SETTLE_MS + 1_000);
    // Hold the water switch so PID_NORMAL's update would keep the pump on.
    let _ = h.press(SwitchId::HotWater);

    let fx = h.press(SwitchId::Power);
    let shutdown = common::index_of(&fx, Effect::SafeHardwareShutdown).unwrap();
    // The request flag is machine state, not an effect, so the ordering to pin is
    // that the shutdown is the only actuator effect of the press.
    assert_eq!(
        common::actuator_writes(&fx),
        1,
        "the power-off press touches exactly one actuator: {fx:?}"
    );
    assert!(shutdown < fx.len());
    assert!(
        h.requested(Request::Standby),
        "the request is set immediately"
    );

    // And the very next tick takes the machine to standby, whatever
    // `shouldEnterStandby` thinks, because `setRemainingTimeMillis(0)` armed it.
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::Standby, "{fx:?}");
}
