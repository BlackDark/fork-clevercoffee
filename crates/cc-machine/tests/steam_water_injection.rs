//! In `STEAM_RUNNING`, held hot-water input injects water with the pump.

mod common;

use cc_domain::state::MachineState;
use cc_machine::{Effect, Event, SwitchId};
use common::Harness;

/// A machine in `STEAM_RUNNING`, with steam mode on.
fn steaming() -> Harness {
    let mut h = Harness::in_state(MachineState::SteamRunning);
    let _ = h.on_entry(MachineState::SteamRunning);
    h
}

/// Whether the water switch is currently held.
fn water_held(h: &Harness) -> bool {
    h.machine.switches.hot_water
}

#[test]
fn steam_water_switch_activates_pump() {
    let mut h = steaming();
    assert!(h.machine.steam_mode, "must be in steam mode");

    let _ = h.press(SwitchId::HotWater);
    assert!(water_held(&h));
    assert!(
        h.machine.steam_mode && water_held(&h),
        "water injection should be requested when the water switch is pressed in steam mode"
    );

    // `STEAM_RUNNING`'s update turns the pump on while the switch is held.
    let fx = h.update(MachineState::SteamRunning);
    assert!(
        common::has(&fx, Effect::EnablePump),
        "the pump must activate when the water switch is pressed in steam mode: {fx:?}"
    );

    // And through a whole tick, so the state machine's real loop is exercised.
    let fx = h.tick();
    assert!(common::has(&fx, Effect::EnablePump), "{fx:?}");
}

#[test]
fn pump_stays_active_while_the_water_switch_is_held() {
    let mut h = steaming();
    let _ = h.press(SwitchId::HotWater);

    for cycle in 0..10 {
        assert!(h.machine.steam_mode && water_held(&h), "cycle {cycle}");
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::EnablePump),
            "the pump must stay active while the switch is held, cycle {cycle}: {fx:?}"
        );
    }
}

#[test]
fn pump_deactivates_when_the_water_switch_is_released() {
    let mut h = steaming();
    let _ = h.press(SwitchId::HotWater);
    let _ = h.tick();
    assert!(common::has(&h.tick(), Effect::EnablePump));

    let _ = h.release(SwitchId::HotWater);
    assert!(
        !(h.machine.steam_mode && water_held(&h)),
        "water injection should NOT be requested when the switch is released"
    );
    let fx = h.tick();
    assert!(
        common::has(&fx, Effect::DisablePump),
        "the pump must deactivate when the water switch is released: {fx:?}"
    );
}

#[test]
fn the_system_stays_in_steam_mode_during_injection() {
    let mut h = steaming();
    let _ = h.press(SwitchId::HotWater);
    let fx = h.tick();
    assert!(common::has(&fx, Effect::EnablePump), "{fx:?}");
    assert_eq!(
        h.state(),
        MachineState::SteamRunning,
        "the system must stay in steam mode during water injection: {fx:?}"
    );
    assert!(h.machine.steam_mode);
}

#[test]
fn water_injection_only_works_in_steam_mode() {
    // The real analogue of leaving steam mode: be in `PID_NORMAL`, where the same switch is the
    // hot-water switch rather than the injection switch.
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.press(SwitchId::HotWater);

    // The switch is read, but the machine is not in steam mode.
    assert!(!h.machine.steam_mode);

    // In `PID_NORMAL` the same switch drives hot-water dispensing instead, and that also runs the
    // pump. So "the pump must not run outside steam mode" is false against the real machine, and
    // asserting it would be a lie. What holds is narrower: `PID_NORMAL` never opens the steam
    // valve, and `STEAM_RUNNING` never stops the pump while the switch is held.
    let fx = h.tick();
    assert!(common::has(&fx, Effect::EnablePump), "{fx:?}");
    assert_eq!(
        common::count(&fx, Effect::OpenSteamValve),
        0,
        "the state machine never opens the steam valve at all: {fx:?}"
    );
}

#[test]
fn multiple_press_release_cycles() {
    let mut h = steaming();
    for cycle in 0..5 {
        let _ = h.press(SwitchId::HotWater);
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::EnablePump),
            "cycle {cycle}: {fx:?}"
        );

        let _ = h.release(SwitchId::HotWater);
        let fx = h.tick();
        assert!(
            common::has(&fx, Effect::DisablePump),
            "cycle {cycle}: {fx:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// What the suite also checks
// ---------------------------------------------------------------------------

/// Leaving `STEAM_RUNNING` stops the injection pump.
#[test]
fn leaving_steam_running_disables_the_injection_pump() {
    let mut h = steaming();
    let _ = h.press(SwitchId::HotWater);
    let _ = h.tick();

    h.machine.requests.set(cc_machine::Request::SteamStop, true);
    let fx = h.tick();
    assert_eq!(h.state(), MachineState::PidNormal, "{fx:?}");

    let win = common::transition_window(&fx, MachineState::SteamRunning, MachineState::PidNormal);
    assert!(common::has(&win, Effect::DisablePump), "{win:?}");
    assert!(
        common::has(&win, Effect::SetSteamMode { enabled: false }),
        "{win:?}"
    );
    assert!(!h.machine.steam_mode);
}

/// The steam-first-on flag is set and cleared with the mode, so it names a run that began with a
/// cold boiler.
#[test]
fn the_steam_first_on_flag_tracks_the_mode() {
    let mut h = Harness::in_state(MachineState::PidNormal);
    let _ = h.on_entry(MachineState::SteamRunning);
    assert!(h.machine.steam_mode);
    assert!(h.machine.steam_first_on);

    let _ = h.on_exit(MachineState::SteamRunning);
    assert!(!h.machine.steam_mode);
    assert!(!h.machine.steam_first_on);
}

/// The hot-water switch is denied in `WATER_TANK_EMPTY`, so the handler layer does not start the
/// pump there. See `parity_findings.rs`.
#[test]
fn the_water_switch_is_denied_while_the_tank_is_empty() {
    let mut h = Harness::in_state(MachineState::WaterTankEmpty);
    let fx = h.send(Event::ButtonPressed {
        switch: SwitchId::HotWater,
        long_press: false,
    });
    assert!(
        !h.machine.hot_water_activity,
        "the handler must be refused in WATER_TANK_EMPTY: {fx:?}"
    );
}
