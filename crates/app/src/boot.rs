//! The boot order and the control loop, as far as they are not the firmware's.
//!
//! `architecture.md` section 1.4 fixes the order, and the order is the safety property:
//!
//! 1. clocks, then **the actuators driven to their inactive level before anything else**;
//! 2. the config region, read and checked, falling back to the compiled defaults;
//! 3. the machine, built around actuators that are already safe;
//! 4. the network, last;
//! 5. provisioning, regardless of network state.
//!
//! Steps 1 and 4 are the board's and the firmware's. Step 2 is storage, step 3 is this module, and
//! step 5 is [`crate::prov`]. What lives here is the part that can be tested on a host: the
//! machine is created, and every tick is the same two calls in the same order.
//!
//! The two calls are [`control_step`] then [`safety_step`], and the order is not incidental. The
//! safety task exists so the interlocks cannot be starved by a control-path await, which means it
//! has to see the command the control path has just issued. Running it first would check the
//! previous tick's command, which is a check that passes one tick after the thing it was meant to
//! catch.

use crate::machine::{Machine, RuntimeConfig, Sensors, TickOutcome};
use crate::tasks::{control_step, safety_step};
use clevercoffee_hal_traits::Actuators;

/// The running machine, plus nothing else.
///
/// Deliberately not a struct with a config, a clock and a sensor set: the caller owns those, and a
/// boot order that has to reach through three types to change a setpoint is a boot order nobody
/// changes. [`Runtime::tick`] takes the sensor snapshot and the elapsed time as arguments, which is
/// also what makes a scenario test a loop of two calls.
#[derive(Debug)]
pub struct Runtime<A: Actuators> {
    pub machine: Machine<A>,
}

impl<A: Actuators> Runtime<A> {
    /// Step 3: builds the machine. Its constructor forces the actuators off, so this is safe to
    /// call even if the board's bring-up did not.
    ///
    /// A machine built from [`RuntimeConfig::default()`] is a machine with the compiled defaults,
    /// which is what a corrupt config region produces. That is deliberate: a machine that refuses
    /// to boot because its config is unreadable cannot be fixed over the network either, and the
    /// C++ firmware's own behaviour was to fall back.
    pub fn new(actuators: A, config: RuntimeConfig) -> Self {
        Self {
            machine: Machine::new(actuators, config),
        }
    }

    /// One millisecond of the machine: the control tick, then the safety tick.
    pub fn tick(&mut self, sensors: Sensors, elapsed_ms: u32) -> TickOutcome {
        control_step(&mut self.machine, sensors, elapsed_ms);
        let _ = safety_step(&mut self.machine);
        TickOutcome {
            ready: self.machine.is_ready(),
            ..TickOutcome::default()
        }
    }

    /// The machine, for a caller that needs to reach the state, the config or the actuators.
    pub fn machine(&self) -> &Machine<A> {
        &self.machine
    }

    pub fn machine_mut(&mut self) -> &mut Machine<A> {
        &mut self.machine
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Request;
    use clevercoffee_domain::State;
    use clevercoffee_hal_traits::RecordingActuators;

    #[test]
    fn a_new_runtime_forces_the_actuators_off_before_anything_else() {
        // Step 1 of the boot order, asserted: a machine that has not ticked yet still has its
        // relays de-energised, because the constructor does it.
        let mut actuators = RecordingActuators::new();
        actuators.command(
            clevercoffee_hal_traits::ActuatorCommand {
                pump: true,
                ..Default::default()
            },
            "pretend something turned the pump on",
        );
        let r = Runtime::new(actuators, RuntimeConfig::default());
        assert!(r.machine().actuators().is_idle());
        assert_eq!(
            r.machine().actuators().sequence().len(),
            2,
            "the command and the force-off"
        );
    }

    #[test]
    fn a_tick_is_the_control_step_then_the_safety_step() {
        let mut r = Runtime::new(RecordingActuators::new(), RuntimeConfig::default());
        for _ in 0..6 {
            r.tick(Sensors::at(93.0), 100);
        }
        assert_eq!(r.machine().state(), State::PidNormal);
        // The safety step ran, so the watchdog was fed even though nothing was wrong.
        assert_eq!(r.machine().watchdog_ms(), 0);
    }

    #[test]
    fn a_runtime_with_the_compiled_defaults_settles_and_heats() {
        let mut r = Runtime::new(RecordingActuators::new(), RuntimeConfig::default());
        for _ in 0..10 {
            r.tick(Sensors::at(93.0), 100);
        }
        assert_eq!(r.machine().state(), State::PidNormal);
        assert!(r.machine().last_command().heater_enabled);
        assert!(
            r.machine().is_ready(),
            "93 C against a 93 C setpoint is ready"
        );
    }

    #[test]
    fn the_ready_flag_is_the_machines_own() {
        let mut r = Runtime::new(RecordingActuators::new(), RuntimeConfig::default());
        r.tick(Sensors::at(30.0), 100);
        assert!(!r.tick(Sensors::at(30.0), 100).ready);
        // Long enough for the fifteen-sample filter to be full of the warm reading: `is_ready`
        // reads the filtered temperature, not the raw one, so a machine that has just been
        // refilled is not ready on the strength of one sample.
        for _ in 0..20 {
            r.tick(Sensors::at(93.0), 100);
        }
        assert!(r.tick(Sensors::at(93.0), 100).ready);
    }

    #[test]
    fn a_provisioning_session_over_a_running_brew_holds_everything_off() {
        // The boot order's step 5, asserted: provisioning and an OTA both go through service mode,
        // and a brew in progress does not survive one.
        let mut r = Runtime::new(RecordingActuators::new(), RuntimeConfig::default());
        for _ in 0..6 {
            r.tick(Sensors::at(93.0), 100);
        }
        r.machine_mut().request(Request::BrewStart);
        for _ in 0..20 {
            r.tick(Sensors::at(93.0), 100);
        }
        assert!(r.machine().last_command().flowing());
        r.machine_mut().set_service_mode(true);
        r.tick(Sensors::at(93.0), 100);
        assert!(r.machine().last_command().is_all_off());
    }
}
