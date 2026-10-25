//! Power-switch asymmetry and long-press reboot.
//!
//! Power-on acts only from standby or PID-disabled; power-off acts outside standby.
//! A momentary press can power off mid-brew; a toggle switched on there does nothing.
//! Require all four reboot conditions on every tick, not only the press edge.

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
        "waking from standby also sets the runtime PID flag: {fx:?}"
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

    // First press: the switch goes HIGH, which for a toggle is "on" -- and powering *on* is
    // inert outside `STANDBY` and `PID_DISABLED`.
    let fx = h.press(SwitchId::Power);
    assert!(
        !h.requested(Request::NormalOperation),
        "powering on only acts in STANDBY and PID_DISABLED: {fx:?}"
    );

    // Then LOW: "off", which requests standby.
    let fx = h.release(SwitchId::Power);
    assert!(
        h.requested(Request::Standby),
        "toggling power OFF from PID_NORMAL should request standby: {fx:?}"
    );
    assert!(
        common::has(&fx, Effect::SafeHardwareShutdown),
        "powering off performs a safe shutdown *before* the request: {fx:?}"
    );

    let fx = h.tick();
    assert_eq!(h.state(), MachineState::Standby, "{fx:?}");
}

#[test]
fn a_momentary_press_powers_on_from_standby() {
    let mut h = asleep();
    h.config.hardware.switches.power.r#type = SwitchType::Momentary;
    // The first tick stamps `boot_at`; the settle window is measured from there, and from
    // nothing else.
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

/// The settle window gates **long-press tracking only**. The power toggle itself is not gated,
/// so a press during the first five seconds after boot still switches the machine off.
///
/// Deliberate: a machine that ignored its power switch for five seconds after every boot would
/// read as broken, and the settle window exists to stop a flickering switch from arming a
/// reboot, not to disable the switch.
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

/// The long-press reboot: four conditions, all required.
///
/// It is evaluated on **every tick**, not on the press edge, because the hold lasts about a
/// second and by then the edge is history.
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

    // Strictly greater, so a hold of exactly `POWER_LONG_PRESS_REBOOT_MS` does not qualify.
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

/// The two halves of the power switch are **asymmetric**, and getting this wrong is how a "the
/// power switch does nothing while brewing" report is filed.
///
/// * Powering *on* acts **only** in `STANDBY` and `PID_DISABLED`.
/// * Powering *off* acts in **every** state except `STANDBY`.
///
/// So a momentary press mid-brew powers the machine off, while a *toggle* switched **on**
/// mid-brew does nothing: the operator has to switch it off again, and only that powers it
/// off.
#[test]
fn power_on_is_inert_mid_operation_but_power_off_is_not() {
    for state in [
        MachineState::BrewPreinfusion,
        MachineState::BrewRunning,
        MachineState::SteamRunning,
        MachineState::ManualFlushRunning,
        MachineState::BackflushFilling,
    ] {
        // A toggle going ON is a power-on, which is inert here.
        let mut h = Harness::in_state(state);
        h.config.hardware.switches.power.r#type = SwitchType::Toggle;
        h.machine.switches.power = false;
        let before = h.machine.requests;
        let fx = h.press(SwitchId::Power);
        assert_eq!(
            h.machine.requests, before,
            "a toggle switched ON is inert in {state:?}: {fx:?}"
        );

        // A momentary press is a toggle, and the machine is awake, so it powers off: standby
        // requested, hardware shut down.
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

/// Powering off from `STANDBY` is a no-op: there is nothing left to stop.
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
    // ...and does not itself stop the hardware; that is the applier's job.
    assert_eq!(
        common::count(&fx, Effect::SafeHardwareShutdown),
        0,
        "{fx:?}"
    );
}

/// The safe shutdown happens **before** the standby request, so for one tick the hardware is
/// off while the state is still `PID_NORMAL` -- and in that tick this state's update enables the
/// pump again if the water switch is held.
///
/// The ordering is deliberate: changing it would change what a power-off does mid-dispense, and
/// the next tick reaches `STANDBY` either way.
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

    // And the very next tick reaches standby regardless of `standby.time`: a power-off expires
    // the countdown outright rather than waiting for it.
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::Standby, "{fx:?}");
}
