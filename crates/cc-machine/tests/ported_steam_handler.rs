//! Port of `test/test_steam_handler` — 12 C++ cases.
//!
//! ## The C++ suite is half mock plumbing
//!
//! Its first nine cases (`ConstructsWithSystemContext` through
//! `GetsSwitchTypeFromConfig`, `test_steam_handler/test_main.cpp:79-138`) test
//! gMock plumbing and the null-switch guard that exists only because the handler
//! holds a raw `Switch*`. The Rust handler is handed an
//! [`Event`](cc_machine::Event) and holds no pointer, so there is no null to
//! guard and nothing to construct. Those become `#[ignore]`d records.
//!
//! ## Case mapping
//!
//! | C++ case (`test_steam_handler/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `ConstructsWithSystemContext` (:79) | `constructing_with_no_switch_is_a_mock_case` (`#[ignore]`) |
//! | `SetHardwareSetsSwitch` (:84) | `set_hardware_sets_switch_is_a_mock_case` (`#[ignore]`) |
//! | `NullSwitchReturnsFalseForAllQueries` (:96) | `a_null_switch_is_a_mock_case` (`#[ignore]`) |
//! | `DetectsSwitchPress` (:107) | `detects_switch_press` |
//! | `DetectsSwitchRelease` (:115) | `detects_switch_release` |
//! | `ClearsSwitchStateChange` (:123) | `the_level_tracks_the_switch` |
//! | `GetsSwitchTypeFromConfig` (:131) | `reads_the_switch_type_from_the_config` |
//! | `ProcessReturnsEarlyWhenDisabled` (:141) | `the_handler_is_a_no_op_when_disabled` |
//! | `ProcessReturnsEarlyWithNullSwitch` (:150) | — (no null switch exists) |
//! | `ProcessDetectsSwitchChangeWithContext` (:155) | [`a_press_in_pid_normal_requests_steam`] |
//! | `MomentarySecondPressStopsSteam` (:177) | [`a_momentary_second_press_stops_steam`] |
//! | `ToggleSwitchActivationAndDeactivation` (:194) | [`a_toggle_switch_activates_and_deactivates`] |
//!
//! Plus the standby branch, which the C++ suite does not cover and which is the
//! most surprising thing in the file.

mod common;

use cc_domain::hardware::SwitchType;
use cc_domain::state::MachineState;
use cc_machine::{Request, SwitchId};
use common::Harness;

#[test]
#[ignore = "C++ mock case: gMock plumbing"]
fn constructing_with_no_switch_is_a_mock_case() {}

#[test]
#[ignore = "C++ mock case: the Rust handler holds no Switch pointer"]
fn set_hardware_sets_switch_is_a_mock_case() {}

#[test]
#[ignore = "C++ mock case: the Rust handler holds no Switch pointer"]
fn a_null_switch_is_a_mock_case() {}

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

// ---------------------------------------------------------------------------
// The standby branch, which the C++ suite does not cover
// ---------------------------------------------------------------------------

/// `SteamHandler.h:137-141`: with a **toggle** steam switch, a press while in
/// standby only counts if the previous reading was LOW.
///
/// ```cpp
/// if (currentState == MachineStateId::STANDBY) {
///     // In standby, only react to a rising edge so a left-on toggle does not
///     // wake steam
///     if (lastSwitchReading_ == LOW) {
///         context->setSteamStartRequested(true);
///     }
/// }
/// ```
///
/// The reducer is handed only *edges*, so a press event **is** a rising edge and
/// the condition is always true here. That is a strict subset of the C++'s
/// behaviour — the C++ additionally had to guard against a left-on toggle being
/// re-read every loop, and an event stream has no "re-read" to guard against.
///
/// The observable consequence is unchanged and is the point: a machine in
/// standby with the steam toggle already on **does** start steaming on a press,
/// and one that is merely *sampled* high does not. Pinned below.
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

/// The C++'s steam handler has **no** `WATER_TANK_EMPTY` permission check, unlike
/// the brew and hot-water handlers. So the steam switch is processed normally
/// with an empty tank.
///
/// It does not matter for safety — `STEAM_RUNNING` needs no water, and the steam
/// PID is what the empty tank must not block (S4 blocks the pump only) — but the
/// asymmetry is real and worth having on the record.
#[test]
fn the_steam_switch_is_not_denied_by_an_empty_tank() {
    let mut h = Harness::in_state(MachineState::WaterTankEmpty);
    h.config.hardware.switches.steam.r#type = SwitchType::Momentary;
    let _ = h.press(SwitchId::Steam);
    assert!(
        h.requested(Request::SteamStart),
        "the C++ SteamHandler has no WATER_TANK_EMPTY permission check \
         (contrast BrewHandler.h:147-150)"
    );
}

/// The steam request flag is drained by `PID_NORMAL` when it consumes it
/// (`PidStates.cpp:70-74`), so it cannot fire twice.
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
