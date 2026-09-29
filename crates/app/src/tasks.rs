//! The task bodies.
//!
//! `architecture.md` section 1.1 names seven tasks. This module holds the five that are pure
//! logic — `control`, `safety`, `sensors`, `display` and the `logger`'s ring buffer — as
//! **synchronous step functions** over [`Machine`](crate::machine::Machine) and friends, and the
//! firmware crate wraps them in embassy tasks with the periods from the table.
//!
//! Why not put the embassy tasks here? Because then every scenario test would need an executor, a
//! timer driver and a simulated clock, and the tests would assert on "what the task graph did"
//! rather than on "what the machine commanded". Keeping the steps synchronous means a brew
//! scenario is a loop of two function calls and the assertion is on
//! `RecordingActuators::sequence()`. The scheduling, the priorities and the watchdog timer are
//! then the only things left in the firmware layer, and they are the things a host test cannot
//! honestly claim to have verified.
//!
//! The separation that matters for safety is `control` versus `safety`: they are two short
//! functions rather than one long loop, so the fail-safe property is checkable by reading them
//! side by side.

use clevercoffee_domain::Timing;
use clevercoffee_hal_traits::{ActuatorCommand, Actuators};

use crate::machine::{Machine, Request, Sensors, TickOutcome};

/// The `control` task body: one tick of the state machine.
///
/// Owns the state, the PID and the actuator command. Never blocks, never awaits, never allocates.
pub fn control_step<A: Actuators>(
    machine: &mut Machine<A>,
    sensors: Sensors,
    elapsed_ms: u32,
) -> TickOutcome {
    machine.tick(sensors, elapsed_ms)
}

/// What the `safety` task did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SafetyOutcome {
    /// The interlocks de-energised everything this tick.
    pub forced_off: bool,
    /// The valve interlock closed a valve the state is not allowed to hold open.
    pub valve_blocked: bool,
    /// The pump ran past its deadline.
    pub pump_timeout: bool,
    /// The emergency stop is active.
    pub emergency: bool,
    /// The machine has a temperature fault.
    pub sensor_fault: bool,
    /// The watchdog was fed.
    pub fed: bool,
}

/// The `safety` task body.
///
/// Runs on every control tick and is deliberately short. It re-checks the two things that must
/// hold no matter what the control path decided, in this order:
///
/// 1. **The valve interlock.** Only the states on
///    [`State::may_hold_water_valve_open`](clevercoffee_domain::state::State::may_hold_water_valve_open)
///    may hold the valve open. A state that is not on the list has its valve closed here even if
///    the command said open, which is what makes a missing entry in a state's own handler a
///    non-event rather than a flood.
/// 2. **The pump deadline.** Every pump command carries a deadline; a pump that has run past it
///    is stopped and the machine latches into emergency stop. This is the fix for D09, where the
///    limits existed as constants and were never armed.
///
/// It also feeds the watchdog, which is the last line of defence: a hung control loop still gets
/// fed here, and a hung safety loop trips the reset, which de-energises the relays in hardware.
pub fn safety_step<A: Actuators>(machine: &mut Machine<A>) -> SafetyOutcome {
    let mut out = SafetyOutcome::default();

    if machine.is_emergency_stopped() {
        machine.force_off("safety: emergency stop");
        out.forced_off = true;
        out.emergency = true;
    }
    if machine.sensor_fault().is_some() {
        out.sensor_fault = true;
        // A machine that cannot read its temperature must not heat, whatever the state table
        // says. `heater_allowed` already refuses, but a state that is not in that list, or a
        // future one that is, cannot leave the heater running on a stale number.
        let command = machine.last_command();
        if command.heater_enabled || command.flowing() {
            machine.force_off("safety: sensor fault");
            out.forced_off = true;
        }
    }

    let command = machine.last_command();
    if command.water_valve && !machine.state().may_hold_water_valve_open() {
        let corrected = ActuatorCommand {
            water_valve: false,
            ..command
        };
        machine
            .actuators_mut()
            .command(corrected, "safety: valve interlock");
        out.valve_blocked = true;
    }

    if machine.pump_deadline_exceeded() {
        machine.force_off("safety: pump deadline");
        machine.latch_emergency_stop();
        out.forced_off = true;
        out.pump_timeout = true;
    }

    // Feed the watchdog last, so a trip earlier in this function is not masked by a feed.
    machine.feed_watchdog();
    out.fed = true;
    out
}

/// One tick of the `sensors` task: the schedule of the four sensor periods.
///
/// The sensor drivers live in their own crates and are synchronous (`no_std`, no async), so this
/// only decides *when* each is read. The periods are the C++ ones from `Timing.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SensorSchedule {
    pub temperature: bool,
    pub pressure: bool,
    pub scale: bool,
    pub water_tank: bool,
}

/// Decides which sensors are due, given the elapsed time and the last time each was read.
pub fn sensor_schedule(now_ms: u32, last: &SensorScheduleAt) -> SensorSchedule {
    // `now_ms == 0` is boot, where every sensor is due: nothing has been read yet, and a machine
    // that waited a full period before its first temperature reading would sit in `Init` for 400 ms.
    let due = |last_ms: u32, period: core::time::Duration| {
        now_ms == 0 || now_ms.saturating_sub(last_ms) >= period.as_millis() as u32
    };
    SensorSchedule {
        temperature: due(last.temperature_ms, Timing::TEMPERATURE),
        pressure: due(last.pressure_ms, Timing::PRESSURE),
        scale: due(last.scale_ms, Timing::SCALE),
        water_tank: due(last.water_tank_ms, Timing::WATER_TANK),
    }
}

/// When each sensor was last read, which is what [`sensor_schedule`] compares against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SensorScheduleAt {
    pub temperature_ms: u32,
    pub pressure_ms: u32,
    pub scale_ms: u32,
    pub water_tank_ms: u32,
}

impl SensorScheduleAt {
    /// Records that a sensor was read now.
    pub fn mark(&mut self, which: SensorKind, now_ms: u32) {
        match which {
            SensorKind::Temperature => self.temperature_ms = now_ms,
            SensorKind::Pressure => self.pressure_ms = now_ms,
            SensorKind::Scale => self.scale_ms = now_ms,
            SensorKind::WaterTank => self.water_tank_ms = now_ms,
        }
    }
}

/// The four sensor reads the schedule tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorKind {
    Temperature,
    Pressure,
    Scale,
    WaterTank,
}

impl SensorKind {
    pub const ALL: [SensorKind; 4] = [
        SensorKind::Temperature,
        SensorKind::Pressure,
        SensorKind::Scale,
        SensorKind::WaterTank,
    ];
}

/// A power-switch press sequence, as the `power` handling needs it.
///
/// The C++ had a `PowerHandler` with a boot guard, a long-press reboot and a debounce, and the
/// three were in one function with a `now` parameter. Here the debounce lives in the switch
/// driver, the boot guard and the edge detection live in the machine, and this type only carries
/// the decision, so a test can drive the whole sequence without a clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    /// Nothing: the boot guard has not expired, or the press was too short.
    Ignore,
    /// Toggle standby and normal operation.
    ToggleStandby,
    /// Reboot.
    Reboot,
}

/// Decides what a power-switch event means.
pub fn power_action(
    long_press: bool,
    now_ms: u32,
    guard_ms: u32,
    reboot_long_press_ms: u32,
) -> PowerAction {
    if now_ms < guard_ms {
        return PowerAction::Ignore;
    }
    if long_press && now_ms >= reboot_long_press_ms {
        PowerAction::Reboot
    } else {
        PowerAction::ToggleStandby
    }
}

/// Applies a power action to the machine.
pub fn apply_power_action<A: Actuators>(machine: &mut Machine<A>, action: PowerAction) {
    match action {
        PowerAction::Ignore => {}
        PowerAction::ToggleStandby => {
            machine.request(if machine.state() == clevercoffee_domain::State::Standby {
                Request::NormalOperation
            } else {
                Request::Standby
            })
        }
        PowerAction::Reboot => machine.request(Request::PowerLongPress),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clevercoffee_hal_traits::RecordingActuators;

    fn machine() -> Machine<RecordingActuators> {
        Machine::new(RecordingActuators::new(), Default::default())
    }

    #[test]
    fn the_sensor_schedule_follows_the_cpp_periods() {
        let at = SensorScheduleAt::default();
        let s = sensor_schedule(0, &at);
        assert!(
            s.temperature && s.pressure && s.scale && s.water_tank,
            "all due at boot"
        );

        let s = sensor_schedule(100, &at);
        assert!(!s.temperature, "the temperature period is 400 ms");
        assert!(!s.water_tank, "the water tank period is 200 ms");
        assert!(s.scale, "the scale period is 100 ms");
        assert!(s.pressure, "the pressure period is 50 ms");

        let s = sensor_schedule(400, &at);
        assert!(s.temperature);
    }

    #[test]
    fn a_marked_sensor_is_not_due_again_until_its_period() {
        let mut at = SensorScheduleAt::default();
        at.mark(SensorKind::Temperature, 1000);
        let s = sensor_schedule(1100, &at);
        assert!(!s.temperature);
        let s = sensor_schedule(1400, &at);
        assert!(s.temperature);
    }

    #[test]
    fn the_boot_guard_swallows_a_press() {
        assert_eq!(
            power_action(false, 0, 5000, 1000),
            PowerAction::Ignore,
            "plugging the machine in must not toggle it"
        );
        assert_eq!(
            power_action(false, 6000, 5000, 1000),
            PowerAction::ToggleStandby
        );
        assert_eq!(power_action(true, 6000, 5000, 1000), PowerAction::Reboot);
    }

    #[test]
    fn a_long_press_before_the_guard_expires_is_ignored() {
        // The guard is checked first: a long press during boot must not reboot the machine the
        // instant it is plugged in.
        assert_eq!(power_action(true, 2000, 5000, 1000), PowerAction::Ignore);
    }

    #[test]
    fn the_safety_task_closes_a_valve_a_state_may_not_hold() {
        let mut m = machine();
        // Hand the machine a command it has no business holding, the way a stale command from a
        // previous state would.
        m.actuators_mut().command(
            ActuatorCommand {
                water_valve: true,
                ..ActuatorCommand::ALL_OFF
            },
            "test: stale",
        );
        for _ in 0..5 {
            control_step(&mut m, Sensors::at(93.0), 100);
        }
        let out = safety_step(&mut m);
        assert!(out.fed);
        assert!(
            !m.last_command().water_valve || m.state().may_hold_water_valve_open(),
            "the valve survived the interlock in {:?}",
            m.state()
        );
    }

    #[test]
    fn the_safety_task_stops_everything_on_a_sensor_fault() {
        let mut m = machine();
        for _ in 0..5 {
            control_step(&mut m, Sensors::at(93.0), 100);
        }
        assert!(
            m.last_command().heater_enabled,
            "the machine should be heating: {:?}",
            m.last_command()
        );
        let out = safety_step(&mut m);
        assert!(!out.sensor_fault);
        // Now the sensor dies.
        control_step(&mut m, Sensors::no_temperature(), 100);
        let out = safety_step(&mut m);
        assert!(out.sensor_fault);
        assert!(m.last_command().is_all_off(), "{:?}", m.last_command());
    }

    #[test]
    fn the_safety_task_feeds_the_watchdog() {
        let mut m = machine();
        for _ in 0..10 {
            control_step(&mut m, Sensors::at(93.0), 100);
            assert!(m.watchdog_ms() >= 100);
            safety_step(&mut m);
            assert_eq!(m.watchdog_ms(), 0, "a fed watchdog reads zero");
        }
    }
}
