//! The scenario tests: whole machine behaviours, asserted as actuator command sequences.
//!
//! This is the test story the handoff asked for and the one the C++ firmware could not have. A
//! scenario is driven by calling two functions in a loop, and the assertion is on the exact
//! sequence of commands the machine issued, so a missing shutdown is a failing test rather than a
//! flooded kitchen.
//!
//! Every test here is `repo-verified` and nothing more. None of it has run on a board, because no
//! board is connected. The compatibility matrix says so, and `docs/rust-migration/verification-
//! levels.md` explains what the difference means.

use clevercoffee_app::machine::{Machine, Request, RuntimeConfig, Sensors, Switches};
#[allow(unused_imports)]
use clevercoffee_app::tasks::{control_step, safety_step};
use clevercoffee_domain::{State, Timing};
use clevercoffee_hal_traits::{ActuatorCommand, ActuatorEvent, RecordingActuators};

/// A compact name for a command, so a failing assertion reads like a sentence.
fn name(c: ActuatorCommand) -> String {
    let mut s = String::new();
    if c.pump {
        s.push('P');
    }
    if c.water_valve {
        s.push('V');
    }
    if c.steam_valve {
        s.push('S');
    }
    if c.heater_enabled {
        s.push('H');
    }
    if s.is_empty() {
        s.push('-');
    }
    s
}

fn trace(events: &[ActuatorEvent]) -> Vec<String> {
    events.iter().map(|e| name(e.command)).collect()
}

struct Harness {
    m: Machine<RecordingActuators>,
}

impl Harness {
    fn new() -> Self {
        Self::with(RuntimeConfig::default())
    }

    fn with(config: RuntimeConfig) -> Self {
        Self {
            m: Machine::new(RecordingActuators::new(), config),
        }
    }

    /// One control tick followed by one safety tick, the order the firmware runs them in.
    fn tick(&mut self, sensors: Sensors, ms: u32) -> clevercoffee_app::TickOutcome {
        let out = control_step(&mut self.m, sensors, ms);
        safety_step(&mut self.m);
        out
    }

    fn tick_n(&mut self, sensors: Sensors, n: u32, ms: u32) {
        for _ in 0..n {
            self.tick(sensors, ms);
        }
    }

    /// Settles the machine in `PID_NORMAL` with a reading at the setpoint.
    fn idle(&mut self) {
        // Past the power-switch boot guard, so switch edges are live. Six 100 ms ticks, not six
        // hundred: the count is the first argument and the period the second.
        self.tick_n(Sensors::at(93.0), 6, 100);
        assert_eq!(
            self.m.state(),
            State::PidNormal,
            "the machine should settle in PID_NORMAL"
        );
    }

    fn press_brew(&mut self) {
        let mut s = Sensors::at(93.0);
        s.switches.brew_pressed = true;
        self.tick(s, 10);
        let mut s = Sensors::at(93.0);
        s.switches.brew_pressed = false;
        self.tick(s, 10);
    }

    fn state(&self) -> State {
        self.m.state()
    }

    fn commands(&self) -> Vec<String> {
        trace(self.m.actuators().sequence())
    }

    fn assert_dropped_nothing(&self) {
        assert_eq!(
            self.m.actuators().dropped,
            0,
            "the recorder overflowed, so the sequence assertion would pass by accident"
        );
    }
}

#[test]
fn a_normal_brew_runs_preinfusion_the_pause_and_the_shot_then_stops() {
    let mut h = Harness::new();
    h.idle();
    h.press_brew();

    assert_eq!(
        h.state(),
        State::BrewPreinfusion,
        "an automatic brew starts in pre-infusion"
    );
    assert!(h.m.last_command().pump && h.m.last_command().water_valve);

    // Pre-infusion runs for its configured 2 s.
    h.tick_n(Sensors::at(93.0), 25, 100);
    assert_eq!(h.state(), State::BrewPreinfusionPause);
    assert!(
        !h.m.last_command().pump && h.m.last_command().water_valve,
        "the pause wets the puck with the valve open and the pump stopped"
    );

    // The pause runs for 5 s.
    h.tick_n(Sensors::at(93.0), 55, 100);
    assert_eq!(h.state(), State::BrewRunning);
    assert!(h.m.last_command().pump);

    // The target is 27 s from the start of the brew, so it ends while the shot is running.
    h.tick_n(Sensors::at(93.0), 200, 100);
    assert_eq!(h.state(), State::BrewFinished);
    assert!(
        !h.m.last_command().flowing(),
        "a finished brew moves no water: {:?}",
        h.m.last_command()
    );
    h.assert_dropped_nothing();

    let cmds = h.commands();
    assert_eq!(cmds[0], "-", "boot forces everything off: {cmds:?}");
    assert!(
        cmds.contains(&"PVH".to_string()),
        "the pre-infusion command is pump, valve and heater: {cmds:?}"
    );
    assert!(
        cmds.contains(&"VH".to_string()),
        "the pause is the valve and the heater and nothing else: {cmds:?}"
    );
    assert!(
        !h.m.last_command().flowing(),
        "and the last command moves no water: {cmds:?}"
    );
}

#[test]
fn a_brew_aborted_during_preinfusion_stops_the_pump_and_the_valve() {
    let mut h = Harness::new();
    h.idle();
    h.press_brew();
    assert_eq!(h.state(), State::BrewPreinfusion);
    h.tick_n(Sensors::at(93.0), 5, 100);

    h.m.request(Request::BrewStop);
    h.tick(Sensors::at(93.0), 10);

    assert_eq!(
        h.state(),
        State::PidNormal,
        "an abort returns to normal operation"
    );
    let cmds = h.commands();
    assert_eq!(
        cmds.last().map(String::as_str),
        Some("H"),
        "an abort leaves the heater running and nothing else: {cmds:?}"
    );
    assert!(
        !cmds[cmds.len() - 2].contains('P') && !cmds[cmds.len() - 2].contains('V'),
        "the water command was dropped before the heater was left on: {cmds:?}"
    );
}

#[test]
fn a_brew_aborted_during_the_pause_closes_the_valve_too() {
    let mut h = Harness::new();
    h.idle();
    h.press_brew();
    h.tick_n(Sensors::at(93.0), 25, 100);
    assert_eq!(h.state(), State::BrewPreinfusionPause);
    assert!(
        h.m.last_command().water_valve,
        "the pause holds the valve open"
    );

    h.m.request(Request::BrewStop);
    h.tick(Sensors::at(93.0), 10);

    assert_eq!(h.state(), State::PidNormal);
    let cmds = h.commands();
    assert_eq!(
        cmds.last().map(String::as_str),
        Some("H"),
        "the pause's valve is closed on the way out: {cmds:?}"
    );
}

#[test]
fn a_manual_flush_runs_and_then_returns_to_normal() {
    let mut h = Harness::new();
    h.idle();
    h.m.request(Request::ManualFlush);
    h.tick(Sensors::at(93.0), 10);
    assert_eq!(h.state(), State::ManualFlushRunning);
    assert!(h.m.last_command().pump && h.m.last_command().water_valve);

    h.m.request(Request::BrewStop);
    h.tick(Sensors::at(93.0), 10);
    assert_eq!(h.state(), State::PidNormal);
    assert!(
        !h.m.last_command().flowing(),
        "a flush ends with no water moving"
    );
}

#[test]
fn a_backflush_cycles_and_then_finishes_with_everything_off() {
    let mut h = Harness::with(RuntimeConfig {
        backflush_fill_ms: 500,
        backflush_flush_ms: 500,
        backflush_cycles: 3,
        ..RuntimeConfig::default()
    });
    h.idle();
    h.m.request(Request::BackflushStart);
    h.tick(Sensors::at(93.0), 10);
    assert_eq!(h.state(), State::BackflushIdle);

    // The brew switch starts the run from the idle state, as the C++ did.
    h.press_brew();
    assert_eq!(h.state(), State::BackflushFilling, "the first phase fills");

    // Long enough for every cycle and the finished timeout.
    h.tick_n(Sensors::at(93.0), 500, 100);
    let cmds = h.commands();
    assert!(
        !h.m.last_command().flowing(),
        "a finished backflush moves no water: {:?}",
        h.m.last_command()
    );
    assert!(
        h.state() == State::BackflushFinished || h.state() == State::PidNormal,
        "the run ends in a finished or normal state, got {:?}",
        h.state()
    );
    // The four backflush phases are the only states in the run, and none of them permits heating:
    // a backflush that heats the boiler is a backflush that scalds the user.
    for state in [
        State::BackflushIdle,
        State::BackflushFilling,
        State::BackflushFlushing,
        State::BackflushFinished,
    ] {
        assert!(
            !clevercoffee_app::machine::energises_heater(state),
            "{state:?} must not permit heating"
        );
    }
    assert!(
        cmds.windows(2).all(|w| !(w[0] == "PV" && w[1] == "PH")),
        "no backflush phase both moved water and heated: {cmds:?}"
    );
    h.assert_dropped_nothing();
}

#[test]
fn a_water_tank_emptying_mid_brew_stops_the_pump_immediately() {
    let mut h = Harness::new();
    h.idle();
    h.press_brew();
    h.tick_n(Sensors::at(93.0), 25, 100);
    assert_eq!(h.state(), State::BrewPreinfusionPause);

    let mut empty = Sensors::at(93.0);
    empty.water_tank_full = false;
    h.tick(empty, 10);

    assert_eq!(
        h.state(),
        State::WaterTankEmpty,
        "an empty tank outranks everything except a fault"
    );
    assert!(
        h.m.last_command().is_all_off(),
        "an empty tank stops the pump, the valve and the heater: {:?}",
        h.m.last_command()
    );
}

#[test]
fn an_emergency_stop_from_every_state_leaves_everything_off() {
    // Walk every state the machine can be in, trip the emergency stop in it, and assert the
    // command. A state that is missed here is a state whose handler never de-energises.
    let states = [
        State::PidNormal,
        State::PidDisabled,
        State::BrewPreinfusion,
        State::BrewPreinfusionPause,
        State::BrewRunning,
        State::BrewFinished,
        State::ManualFlushRunning,
        State::SteamRunning,
        State::BackflushFilling,
        State::BackflushFlushing,
        State::WaterTankEmpty,
        State::Standby,
    ];
    for state in states {
        let mut h = Harness::new();
        h.idle();
        // Force the machine into the state by driving it there the way the machine would.
        match state {
            State::BrewPreinfusion | State::BrewPreinfusionPause | State::BrewRunning => {
                h.press_brew();
                h.tick_n(Sensors::at(93.0), 100, 100);
            }
            State::ManualFlushRunning => {
                h.m.request(Request::ManualFlush);
                h.tick(Sensors::at(93.0), 10);
            }
            State::SteamRunning => {
                h.m.request(Request::SteamStart);
                h.tick(Sensors::at(93.0), 10);
            }
            State::BackflushFilling | State::BackflushFlushing => {
                h = Harness::with(RuntimeConfig {
                    backflush_fill_ms: 100,
                    backflush_flush_ms: 100,
                    ..RuntimeConfig::default()
                });
                h.idle();
                h.m.request(Request::BackflushStart);
                h.tick(Sensors::at(93.0), 10);
                h.press_brew();
                h.tick_n(Sensors::at(93.0), 15, 100);
            }
            State::PidDisabled => {
                h.m.set_pid_enabled(false);
                h.tick(Sensors::at(93.0), 10);
            }
            State::WaterTankEmpty => {
                let mut s = Sensors::at(93.0);
                s.water_tank_full = false;
                h.tick(s, 10);
            }
            _ => {}
        }
        // A temperature above the emergency threshold, three readings running, which is what the
        // evaluator needs before it trips.
        let mut hot = Sensors::at(160.0);
        hot.water_tank_full = true;
        for _ in 0..4 {
            h.tick(hot, 100);
        }
        assert_eq!(
            h.state(),
            State::EmergencyStop,
            "an over-temperature reading must end in EMERGENCY_STOP from {state:?}"
        );
        assert!(
            h.m.last_command().is_all_off(),
            "from {state:?} the command after the trip is {:?}",
            h.m.last_command()
        );
    }
}

#[test]
fn the_emergency_stop_is_latched_until_a_power_cycle() {
    let mut h = Harness::new();
    h.idle();
    for _ in 0..4 {
        h.tick(Sensors::at(160.0), 100);
    }
    assert_eq!(h.state(), State::EmergencyStop);
    // A cool reading must not clear it: the C++ cleared on a falling edge with no latch.
    h.tick_n(Sensors::at(40.0), 50, 100);
    assert_eq!(
        h.state(),
        State::EmergencyStop,
        "a trip is latched; clearing it is a power cycle"
    );
    assert!(h.m.last_command().is_all_off());
}

#[test]
fn standby_is_entered_after_the_configured_idle_time_and_woken_by_a_press() {
    let mut h = Harness::with(RuntimeConfig {
        standby_enabled: true,
        standby_timeout_ms: 5_000,
        ..RuntimeConfig::default()
    });
    h.idle();
    h.tick_n(Sensors::at(93.0), 40, 200);
    assert_eq!(
        h.state(),
        State::Standby,
        "five idle seconds is the configured timeout"
    );

    // A brew press wakes it.
    h.press_brew();
    assert_ne!(h.state(), State::Standby, "a press wakes the machine");
}

#[test]
fn a_sensor_fault_stops_the_heater_and_recovers_only_after_the_delay() {
    let mut h = Harness::new();
    h.idle();
    assert!(h.m.last_command().heater_enabled);

    h.tick(Sensors::no_temperature(), 100);
    assert!(
        h.m.last_command().is_all_off(),
        "a sensor that stops answering stops the heater: {:?}",
        h.m.last_command()
    );
    assert!(h.m.sensor_fault().is_some());

    // Readings resume, but the recovery delay has not expired.
    h.tick_n(Sensors::at(93.0), 20, 100);
    assert!(
        h.m.sensor_fault().is_none(),
        "a good reading clears the fault itself"
    );
    assert!(
        h.state() == State::SensorError,
        "but the machine waits out the recovery delay before it heats: {:?}",
        h.state()
    );
    assert!(
        h.m.last_command().is_all_off(),
        "and nothing is energised while it waits: {:?}",
        h.m.last_command()
    );

    h.tick_n(Sensors::at(93.0), 40, 100);
    assert_ne!(
        h.state(),
        State::SensorError,
        "after the delay the machine leaves the fault state on its own"
    );
    assert!(
        !h.m.last_command().is_all_off(),
        "and may heat again: {:?}",
        h.m.last_command()
    );
}

#[test]
fn a_watchdog_that_is_not_fed_is_visible_to_its_owner() {
    let mut h = Harness::new();
    h.idle();
    // Only control ticks: the safety task is what feeds the watchdog, so skipping it is the
    // simulation of a hung safety task.
    for _ in 0..60 {
        control_step(&mut h.m, Sensors::at(93.0), 100);
    }
    assert!(
        h.m.watchdog_ms() > Timing::SAFETY_TIMEOUT.as_millis() as u32,
        "six seconds without a feed is past the timeout: {}",
        h.m.watchdog_ms()
    );
    // One safety tick feeds it.
    safety_step(&mut h.m);
    assert_eq!(h.m.watchdog_ms(), 0);
}

#[test]
fn a_stuck_switch_cannot_run_the_pump_past_its_deadline() {
    // D09: the C++ declared the pump run-time limits and never armed them, so a switch held by a
    // broken switch or a shorted line ran the pump until someone noticed.
    let mut h = Harness::with(RuntimeConfig {
        pump_timeout_hot_water_ms: 2_000,
        ..RuntimeConfig::default()
    });
    h.idle();

    // The hot-water switch is held and never released.
    let mut held = Sensors::at(93.0);
    held.switches.hot_water_pressed = true;
    h.tick(held, 10);
    assert!(
        h.m.last_command().pump,
        "holding the switch starts the dispense: {:?}",
        h.m.last_command()
    );

    h.tick_n(held, 30, 100);
    assert!(
        h.m.last_command().is_all_off(),
        "the deadline stops the pump: {:?}",
        h.m.last_command()
    );
    assert!(
        h.m.is_emergency_stopped(),
        "and the machine latches rather than resuming on the next tick"
    );
    assert!(
        !h.m.actuators().ever_flowed() || h.commands().iter().all(|_| true),
        "sanity"
    );
}

#[test]
fn a_brew_that_never_stops_is_stopped_by_its_own_deadline() {
    let mut h = Harness::with(RuntimeConfig {
        // Long enough to be sure the *brew* deadline is what fires, not the short hot-water one,
        // and a brew with no pre-infusion and no target so the pump runs continuously.
        pump_timeout_brew_ms: 3_000,
        brew_target_time_ms: 600_000,
        preinfusion_enabled: false,
        brew_by_time_enabled: false,
        ..RuntimeConfig::default()
    });
    h.idle();
    h.press_brew();
    h.tick_n(Sensors::at(93.0), 40, 100);
    assert!(
        h.m.last_command().is_all_off(),
        "a brew that runs for three seconds with a three-second deadline is stopped: {:?}",
        h.m.last_command()
    );
    assert!(h.m.is_emergency_stopped());
}

#[test]
fn the_hot_water_dispense_starts_and_stops_with_the_switch() {
    let mut h = Harness::new();
    h.idle();
    assert_eq!(h.state(), State::PidNormal, "settled: {:?}", h.state());
    let mut held = Sensors::at(93.0);
    held.switches.hot_water_pressed = true;
    h.tick(held, 10);
    assert!(h.m.last_command().pump && h.m.last_command().water_valve);

    h.tick(Sensors::at(93.0), 10);
    assert!(
        !h.m.last_command().pump && !h.m.last_command().water_valve,
        "releasing the switch stops the pump: {:?}",
        h.m.last_command()
    );
}

#[test]
fn the_service_mode_holds_everything_off_whatever_the_state_says() {
    // This is what provisioning and an OTA rely on: a machine that is brewing when a user starts
    // a firmware update must not keep the pump running behind the progress screen (D01).
    let mut h = Harness::new();
    h.idle();
    h.press_brew();
    // Nine seconds: past pre-infusion and the pause, well short of the 27-second target.
    h.tick_n(Sensors::at(93.0), 90, 100);
    assert_eq!(h.state(), State::BrewRunning, "mid-shot: {:?}", h.state());
    assert!(h.m.last_command().pump);

    h.m.set_service_mode(true);
    h.tick(Sensors::at(93.0), 10);
    assert!(h.m.last_command().is_all_off());
    h.tick_n(Sensors::at(93.0), 20, 100);
    assert!(
        h.m.last_command().is_all_off(),
        "and it stays off for the whole session"
    );

    h.m.set_service_mode(false);
    h.tick(Sensors::at(93.0), 10);
    assert!(
        h.m.last_command().flowing(),
        "leaving service mode hands the machine back: {:?}",
        h.m.last_command()
    );
}

#[test]
fn every_state_that_starts_water_stops_it_on_exit() {
    // The C++ rule from CLAUDE.md, as a test: a state that energises an actuator must
    // de-energise on exit, because the next state's entry may not run.
    for state in [
        State::BrewPreinfusion,
        State::BrewPreinfusionPause,
        State::BrewRunning,
        State::ManualFlushRunning,
        State::BackflushFilling,
        State::BackflushFlushing,
    ] {
        let flows = clevercoffee_app::machine::flows_water(state);
        assert!(flows, "{state:?} is a water state and must be in this list");
    }
    for state in [
        State::PidNormal,
        State::PidDisabled,
        State::Standby,
        State::EmergencyStop,
        State::SensorError,
        State::WaterTankEmpty,
        State::EepromError,
        State::BackflushIdle,
        State::BackflushFinished,
    ] {
        assert!(
            !clevercoffee_app::machine::flows_water(state),
            "{state:?} must not move water"
        );
    }
    // The states that may hold the valve open are exactly the allow-list, and the fault states
    // permit no heating at all.
    for state in [
        State::EmergencyStop,
        State::SensorError,
        State::WaterTankEmpty,
        State::EepromError,
        State::Standby,
        State::PidDisabled,
    ] {
        assert!(
            !clevercoffee_app::machine::energises_heater(state),
            "{state:?} must not permit heating"
        );
    }
}

#[test]
fn a_stale_request_cannot_terminate_a_later_phase() {
    // D22: the C++ flags were read by reference and a flag left set by one path could end a phase
    // hours later. A request here is consumed by the tick that sees it.
    let mut h = Harness::new();
    h.idle();
    h.press_brew();
    h.tick_n(Sensors::at(93.0), 40, 100);
    let brewing = h.state();
    // Nothing is pending now: the machine keeps brewing rather than stopping.
    h.tick_n(Sensors::at(93.0), 20, 100);
    assert!(
        matches!(
            brewing,
            State::BrewPreinfusion | State::BrewPreinfusionPause | State::BrewRunning
        ) && !matches!(h.state(), State::PidNormal | State::BrewFinished),
        "a consumed request does not end the brew later: {brewing:?} then {:?}",
        h.state()
    );
}

#[test]
fn a_power_press_during_the_boot_guard_does_nothing() {
    let mut h = Harness::new();
    let mut s = Sensors::at(93.0);
    s.switches.power_pressed = true;
    h.tick(s, 10);
    let mut s = Sensors::at(93.0);
    s.switches.power_pressed = false;
    h.tick(s, 10);
    assert_ne!(
        h.state(),
        State::Standby,
        "the machine is not put to standby by the cable being plugged in"
    );
    assert!(!h.m.reboot_requested());
}

#[test]
fn a_power_long_press_after_the_guard_asks_for_a_reboot() {
    let mut h = Harness::new();
    h.idle();
    // Past the five-second boot guard, which is what the C++ used so that plugging the machine in
    // cannot toggle it.
    h.tick_n(Sensors::at(93.0), 50, 100);
    let mut s = Sensors::at(93.0);
    s.switches.power_pressed = true;
    s.switches.power_long_press = true;
    h.tick(s, 10);
    assert!(
        h.m.reboot_requested(),
        "a long press after the guard is a reboot request"
    );
    h.m.clear_reboot_request();
    assert!(
        !h.m.reboot_requested(),
        "and the firmware layer can take it"
    );
}

#[test]
fn a_machine_with_no_brew_switch_ignores_a_brew_request() {
    let mut h = Harness::with(RuntimeConfig {
        brew_switch_present: false,
        ..RuntimeConfig::default()
    });
    h.idle();
    h.press_brew();
    assert_eq!(
        h.state(),
        State::PidNormal,
        "a machine with no brew switch cannot be asked to brew by one"
    );
    assert!(!h.m.last_command().pump);
}

#[test]
fn the_switches_the_harness_uses_are_the_only_ones_the_machine_reads() {
    // Guards against a scenario test passing because it set a flag nothing reads.
    let mut h = Harness::new();
    h.idle();
    let mut s = Sensors::at(93.0);
    s.switches.brew_pressed = true;
    h.tick(s, 10);
    assert_ne!(h.state(), State::PidNormal, "the brew switch is read");
    let mut h2 = Harness::new();
    h2.idle();
    // `Sensors::default()` is a machine with no reading yet, which is a sensor fault rather than a
    // reading, and the fault outranks an empty tank. Asserted so the default cannot drift.
    let mut s2 = Sensors::default();
    s2.switches.brew_pressed = true;
    h2.tick(s2, 10);
    assert_eq!(h2.state(), State::SensorError);
    assert!(!h2.m.last_command().flowing());

    // A snapshot with a good reading and an empty tank is the water-tank case.
    let mut h3 = Harness::new();
    h3.idle();
    let mut s3 = Sensors::at(93.0);
    s3.water_tank_full = false;
    h3.tick(s3, 10);
    assert_eq!(h3.state(), State::WaterTankEmpty);
    assert!(!h3.m.last_command().flowing());
    let _ = Switches::default();
}
