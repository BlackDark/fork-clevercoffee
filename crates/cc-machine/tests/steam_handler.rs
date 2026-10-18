//! The steam switch: start, stop, and what standby does with it.
//!
//! A momentary switch requests a start from `PID_NORMAL` and a stop while steaming. A toggle
//! requests a start on the rising edge and a stop on the falling edge, and the level is recorded
//! either way.
//!
//! Two things here are worth more than the happy path. The first is standby: a steam toggle left
//! on does **not** keep requesting steam, because the request is made on the press edge and
//! nowhere else. The second is an empty tank, which does not deny the steam switch -- steam needs
//! no water, so there is nothing for the tank to block.

mod common;

use cc_domain::hardware::SwitchType;
use cc_domain::state::MachineState;
use cc_machine::{Request, SwitchId};
use common::Harness;

#[test]
fn detects_switch_press() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    assert!(!h.machine.switches.steam);
    let _ = h.press(SwitchId::Steam);
    assert!(h.machine.switches.steam);
}

#[test]
fn detects_switch_release() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::Steam);
    let _ = h.release(SwitchId::Steam);
    assert!(!h.machine.switches.steam);
}

#[test]
fn the_level_tracks_the_switch() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    assert!(!h.machine.switches.steam);
    let _ = h.press(SwitchId::Steam);
    assert!(h.machine.switches.steam);
    let _ = h.release(SwitchId::Steam);
    assert!(!h.machine.switches.steam);
}

#[test]
fn reads_the_switch_type_from_the_config() {
    let mut h = Harness::new();
    h.config.hardware.switches.steam.r#type = SwitchType::Momentary;
    assert_eq!(h.ctx().steam_switch_type(), SwitchType::Momentary);
    h.config.hardware.switches.steam.r#type = SwitchType::Toggle;
    assert_eq!(h.ctx().steam_switch_type(), SwitchType::Toggle);
}

#[test]
fn the_handler_is_a_no_op_when_disabled() {
    let mut h = Harness::new();
    h.config.hardware.switches.steam.enabled = false;
    let fx = h.press(SwitchId::Steam);
    assert!(fx.is_empty(), "{fx:?}");
    assert!(!h.requested(Request::SteamStart));
}

#[test]
fn a_press_in_pid_normal_requests_steam() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.steam.r#type = SwitchType::Momentary;

    let _ = h.tick();
    assert!(!h.requested(Request::SteamStart), "no edge, no request");

    let fx = h.press(SwitchId::Steam);
    assert!(
        h.requested(Request::SteamStart),
        "steam start should be requested on a momentary press in PID_NORMAL: {fx:?}"
    );

    // And the flag drives the transition on the same loop's tick.
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::SteamRunning, "{fx:?}");
    assert!(h.machine.steam_mode, "{fx:?}");
}

#[test]
fn a_momentary_second_press_stops_steam() {
    let mut h = Harness::in_state(MachineState::SteamRunning);
    h.config.hardware.switches.steam.r#type = SwitchType::Momentary;
    h.machine.switches.steam = false;

    let fx = h.press(SwitchId::Steam);
    assert!(
        h.requested(Request::SteamStop),
        "a second momentary press while steaming should request a stop: {fx:?}"
    );

    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
    assert!(!h.machine.steam_mode);
}

#[test]
fn a_toggle_switch_activates_and_deactivates() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.config.hardware.switches.steam.r#type = SwitchType::Toggle;

    let _ = h.press(SwitchId::Steam);
    assert!(h.requested(Request::SteamStart));

    h.machine.requests.clear_all();
    h.enter_state_without_effects(MachineState::SteamRunning);
    let _ = h.release(SwitchId::Steam);
    assert!(
        h.requested(Request::SteamStop),
        "a toggle released while steaming must request a steam stop"
    );
}

/// The standby branch: a steam toggle left on must not keep the machine awake.
///
/// The request comes from the *edge*, so a level that is already high when the machine falls
/// asleep produces nothing on the ticks that follow, and one press edge is what wakes it.
#[test]
fn a_toggle_press_in_standby_wakes_the_machine() {
    let mut h = Harness::in_state(MachineState::Standby);
    h.config.hardware.switches.steam.r#type = SwitchType::Toggle;
    h.config.pid.enabled = true;
    h.machine.pid.runtime_enabled = false;

    // The toggle is already on when the machine falls asleep.
    h.machine.switches.steam = true;
    let fx = h.tick();
    assert_eq!(
        h.state(),
        MachineState::Standby,
        "a held-on toggle must not keep re-requesting steam: {fx:?}"
    );
    assert!(!h.requested(Request::SteamStart), "{fx:?}");

    // A genuine press edge wakes it.
    let _ = h.release(SwitchId::Steam);
    let _ = h.press(SwitchId::Steam);
    assert!(
        h.requested(Request::SteamStart),
        "a press edge must wake the machine"
    );
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");
}

/// An empty tank does not deny the steam switch, unlike the brew and hot-water switches.
///
/// It is safe: `STEAM_RUNNING` needs no water, and S4 shuts the pump down rather than the
/// heater, so blocking steam would block the one thing an empty tank does not endanger.
#[test]
fn the_steam_switch_is_not_denied_by_an_empty_tank() {
    let mut h = Harness::in_state(MachineState::WaterTankEmpty);
    h.config.hardware.switches.steam.r#type = SwitchType::Momentary;
    let _ = h.press(SwitchId::Steam);
    assert!(
        h.requested(Request::SteamStart),
        "steam is not gated on the water tank, unlike brew and hot water"
    );
}

/// The request flag is cleared when it is consumed, so one press cannot start steam twice.
#[test]
fn the_steam_start_request_is_drained_when_consumed() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    h.machine.requests.set(Request::SteamStart, true);
    let _ = h.tick();
    assert_eq!(h.state(), MachineState::SteamRunning);
    assert!(
        !h.requested(Request::SteamStart),
        "the flag must be consumed"
    );
}
