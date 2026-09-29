//! The transition table.
//!
//! This is the safety-critical core. In the C++ firmware it was spread across `BaseState`'s four
//! priority checks plus a `checkSpecificTransitions` override in each of eighteen state classes,
//! and a reader could not see the whole table in one place. Here it is one function that returns
//! the next state or "stay", and every test asserts the exact target.
//!
//! Two C++ behaviours are reproduced deliberately even though they look wrong, and both are
//! recorded in the defects register rather than quietly changed:
//!
//! - `PID_NORMAL` transitions to `PID_DISABLED` when the heater is switched off
//!   (`PidStates.cpp:48-51`). That is what the state is for.
//! - `BREW_FINISHED` always returns to `BREW_PREINFUSION` on a new brew
//!   (`BrewStates.cpp:330-335`), even in manual mode; the pre-infusion state then advances
//!   immediately, so the observable difference is one tick.
//!
//! One C++ behaviour is *not* reproduced, because the C++ contradicts itself: the backflush flush
//! phase. See [`Actuators::DRAIN`] and defect D49.

use crate::backflush::{resolve_cycle_advance, CycleAdvanceEffect};
use crate::state::State;

/// Everything the state machine is allowed to see.
///
/// Deliberately a plain struct with no interior mutability: the caller owns it, the function
/// borrows it, and nothing can change under the decision.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Inputs {
    // Cross-cutting, evaluated in this order because the order is the behaviour.
    pub emergency_stop: bool,
    /// Latched by the sensor layer, not a momentary read. A disconnected DS18B20 looks identical
    /// to "still converting" for a whole read cycle, so a momentary read cannot see it.
    pub sensor_fault: bool,
    pub water_tank_full: bool,
    pub pid_runtime_enabled: bool,
    /// `hardware.sensors.watertank.keep_heater_on_empty`. The one configuration under which an
    /// empty tank still heats.
    pub keep_heater_on_empty: bool,
    /// True during the configured brew PID delay, when the heater is held off at the start of a
    /// brew so the pump gets clean water first.
    pub brew_pid_delay_active: bool,

    // Per-state requests. The C++ consumed each flag on use, so a stale one could not terminate
    // a phase. These are edge requests: the control task raises them for one tick.
    pub brew_start_requested: bool,
    pub brew_stop_requested: bool,
    pub steam_requested: bool,
    pub manual_flush_requested: bool,
    pub backflush_mode_active: bool,
    pub backflush_cycle_start: bool,
    pub backflush_stop_requested: bool,
    pub standby_requested: bool,
    pub normal_operation_requested: bool,

    // Timers. See the field docs for what each is measured from, because the C++ measured two
    // of them from different origins and the difference is observable.
    /// Milliseconds since this state was entered.
    pub state_elapsed_ms: u32,
    /// Milliseconds since the sensor fault *cleared*, not since entry. The C++ resets this while
    /// the fault persists (`ErrorStates.cpp:38-50`), so a flapping sensor waits the full delay
    /// after the last bad reading rather than hopping straight back.
    pub fault_clear_elapsed_ms: u32,
    /// Milliseconds since the brew started, counting pre-infusion and the pause. The C++
    /// compares a total against a total (`BrewStates.cpp:279-289`), so the pre-infusion time is
    /// part of the user's 27 seconds.
    pub brew_elapsed_ms: u32,

    // Configured durations, in milliseconds.
    pub brew_by_time_enabled: bool,
    pub brew_by_weight_enabled: bool,
    /// Pre-infusion plus pause plus the target. Zero when brewing by weight.
    pub brew_total_target_ms: u32,
    pub brew_weight_g: f64,
    pub brew_target_weight_g: f64,
    pub manual_brew_mode: bool,
    pub preinfusion_enabled: bool,
    pub preinfusion_ms: u32,
    pub preinfusion_pause_ms: u32,
    pub brew_finished_timeout_ms: u32,
    pub backflush_finished_timeout_ms: u32,
    pub backflush_fill_ms: u32,
    pub backflush_flush_ms: u32,
    pub backflush_cycles: u8,
    /// 0-based index within the current backflush run.
    pub backflush_current_cycle: u8,
    pub standby_timeout_ms: u32,
    pub sensor_error_recovery_ms: u32,
    pub eeprom_recovery_timeout_ms: u32,
}

/// What the machine must be commanded to do, given a state.
///
/// Three independent booleans rather than a set of named combinations, because the three relays
/// are independent: "valve open, pump stopped, heater on" is the pre-infusion pause, and an enum
/// with one variant per combination cannot hold it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Actuators {
    pub pump: bool,
    pub water_valve: bool,
    pub steam_valve: bool,
    pub heater_enabled: bool,
}

impl Actuators {
    /// Pump off, valve closed, heater off.
    pub const ALL_OFF: Self = Self {
        pump: false,
        water_valve: false,
        steam_valve: false,
        heater_enabled: false,
    };
    /// Pump off, valve closed, heater under PID control.
    pub const HEATER_ONLY: Self = Self {
        pump: false,
        water_valve: false,
        steam_valve: false,
        heater_enabled: true,
    };
    /// Pump on, valve open, heater under PID control.
    pub const PUMP_AND_HEATER: Self = Self {
        pump: true,
        water_valve: true,
        steam_valve: false,
        heater_enabled: true,
    };
    /// Valve open, pump off, heater on: the pre-infusion pause, wetting the puck.
    pub const PAUSE: Self = Self {
        pump: false,
        water_valve: true,
        steam_valve: false,
        heater_enabled: true,
    };
    /// Valve open, pump off, heater off: the backflush flush phase, draining by gravity.
    pub const DRAIN: Self = Self {
        pump: false,
        water_valve: true,
        steam_valve: false,
        heater_enabled: false,
    };
    /// Pump on, valve open, heater on, for a hot-water dispense out of `PID_NORMAL` or
    /// `STEAM_RUNNING`. Not a state: the C++ ran this pump inside those states' `update()`
    /// whenever the hot-water switch was held (`PidStates.cpp:33-43`,
    /// `SteamStates.cpp:36-46`), with no dedicated state.
    pub const HOT_WATER: Self = Self {
        pump: true,
        water_valve: true,
        steam_valve: false,
        heater_enabled: true,
    };

    /// Whether any water is moving. The interlock's single question.
    pub const fn flowing(self) -> bool {
        self.pump || self.water_valve || self.steam_valve
    }
}

/// Returns the state to move to, or `None` to stay put.
///
/// The four cross-cutting checks run first, in the C++ priority order, because that order is
/// part of the behaviour: emergency stop outranks a sensor fault, and a sensor fault outranks an
/// empty tank. Changing the order would change which state the machine rests in after a fault.
pub fn next_state(current: State, input: &Inputs) -> Option<State> {
    if let Some(next) = cross_cutting(current, input) {
        return Some(next);
    }
    specific(current, input)
}

fn cross_cutting(current: State, input: &Inputs) -> Option<State> {
    if input.emergency_stop {
        return Some(State::EmergencyStop);
    }
    if input.sensor_fault {
        return Some(State::SensorError);
    }
    // An empty tank must not trap the machine in WATER_TANK_EMPTY, and must not wake it out of
    // standby, so both states are excluded. An explicit user request still wakes it, which is a
    // different input and handled in `specific`.
    if !input.water_tank_full && !matches!(current, State::WaterTankEmpty | State::Standby) {
        return Some(State::WaterTankEmpty);
    }
    // PID disable. The excluded states are the ones already off, INIT which has not chosen, and
    // the fault states, which have their own recovery paths.
    let pid_checked = !matches!(
        current,
        State::PidDisabled
            | State::PidNormal
            | State::Standby
            | State::Init
            | State::EmergencyStop
            | State::SensorError
            | State::WaterTankEmpty
            | State::EepromError
    );
    if pid_checked && !input.pid_runtime_enabled {
        return Some(State::PidDisabled);
    }
    None
}

fn specific(current: State, input: &Inputs) -> Option<State> {
    match current {
        State::Init => Some(parking_state(input)),

        State::PidNormal => pid_normal(input),

        State::BrewPreinfusion => {
            if input.brew_stop_requested {
                return Some(parking_state(input));
            }
            if input.manual_brew_mode || !input.preinfusion_enabled {
                return Some(State::BrewRunning);
            }
            (input.state_elapsed_ms >= input.preinfusion_ms).then_some(
                if input.preinfusion_pause_ms > 0 {
                    State::BrewPreinfusionPause
                } else {
                    State::BrewRunning
                },
            )
        }

        State::BrewPreinfusionPause => {
            if input.brew_stop_requested {
                return Some(parking_state(input));
            }
            (input.state_elapsed_ms >= input.preinfusion_pause_ms).then_some(State::BrewRunning)
        }

        State::BrewRunning => {
            if input.brew_stop_requested {
                return Some(State::BrewFinished);
            }
            // The C++ evaluates the stop conditions only in automatic mode
            // (`BrewStates.cpp:277`), and compares a total elapsed time against a total target
            // that already includes pre-infusion.
            if !input.manual_brew_mode
                && input.brew_by_time_enabled
                && input.brew_total_target_ms > 0
                && input.brew_elapsed_ms >= input.brew_total_target_ms
            {
                return Some(State::BrewFinished);
            }
            if !input.manual_brew_mode
                && input.brew_by_weight_enabled
                && input.brew_target_weight_g > 0.0
                && input.brew_weight_g >= input.brew_target_weight_g
            {
                return Some(State::BrewFinished);
            }
            None
        }

        // The C++ returns to BREW_PREINFUSION unconditionally (`BrewStates.cpp:330-335`). In
        // manual mode the pre-infusion state advances on its next tick, so the difference is one
        // tick of a full-page-pump state that opens the valve either way.
        State::BrewFinished => {
            if input.brew_start_requested {
                return Some(State::BrewPreinfusion);
            }
            (input.state_elapsed_ms >= input.brew_finished_timeout_ms)
                .then_some(parking_state(input))
        }

        State::ManualFlushRunning => {
            if input.manual_flush_requested {
                return Some(if input.backflush_mode_active {
                    State::BackflushIdle
                } else {
                    parking_state(input)
                });
            }
            None
        }

        State::SteamRunning => (!input.steam_requested).then_some(parking_state(input)),

        State::BackflushIdle => {
            if !input.backflush_mode_active {
                return Some(parking_state(input));
            }
            if input.backflush_cycle_start {
                return Some(State::BackflushFilling);
            }
            if input.manual_flush_requested {
                return Some(State::ManualFlushRunning);
            }
            None
        }

        State::BackflushFilling => {
            if !input.backflush_mode_active {
                return Some(parking_state(input));
            }
            if input.backflush_stop_requested {
                return Some(State::BackflushIdle);
            }
            (input.state_elapsed_ms >= input.backflush_fill_ms).then_some(State::BackflushFlushing)
        }

        State::BackflushFlushing => {
            if !input.backflush_mode_active {
                return Some(parking_state(input));
            }
            if input.backflush_stop_requested {
                return Some(State::BackflushIdle);
            }
            if input.state_elapsed_ms < input.backflush_flush_ms {
                return None;
            }
            // The single place the cycle predicate is evaluated, so the u8 overflow guard in
            // `resolve_cycle_advance` cannot be bypassed by a second copy of the arithmetic.
            match resolve_cycle_advance(input.backflush_current_cycle, input.backflush_cycles) {
                CycleAdvanceEffect::StartNextCycle => Some(State::BackflushFilling),
                CycleAdvanceEffect::CompleteAllCycles => Some(State::BackflushFinished),
            }
        }

        State::BackflushFinished => {
            if !input.backflush_mode_active {
                return Some(parking_state(input));
            }
            if input.backflush_stop_requested {
                return Some(State::BackflushIdle);
            }
            if input.backflush_cycle_start {
                return Some(State::BackflushFilling);
            }
            (input.state_elapsed_ms >= input.backflush_finished_timeout_ms)
                .then_some(State::BackflushIdle)
        }

        // The C++ checks the refill before the standby request (`ErrorStates.cpp:75-82`), so a
        // refill and a standby request in the same tick go back to heating rather than into
        // standby, from which an empty tank cannot wake the machine.
        State::WaterTankEmpty => {
            if input.water_tank_full {
                return Some(parking_state(input));
            }
            if input.standby_requested || input.state_elapsed_ms >= input.standby_timeout_ms {
                return Some(State::Standby);
            }
            None
        }

        State::EmergencyStop => (!input.emergency_stop).then_some(State::Init),

        State::PidDisabled => {
            if input.pid_runtime_enabled {
                return Some(State::PidNormal);
            }
            (input.state_elapsed_ms >= input.standby_timeout_ms).then_some(State::Standby)
        }

        State::Standby => {
            if input.normal_operation_requested
                || input.brew_start_requested
                || input.steam_requested
            {
                return Some(parking_state(input));
            }
            None
        }

        // Recovery is measured from the fault clearing, not from entry, so a flapping sensor
        // waits the full delay after the last bad reading.
        State::SensorError => (!input.sensor_fault
            && input.fault_clear_elapsed_ms >= input.sensor_error_recovery_ms)
            .then_some(parking_state(input)),

        // The C++ waits a timeout and never consults the sensor flag
        // (`ErrorStates.cpp:111-124`).
        State::EepromError => (input.state_elapsed_ms >= input.eeprom_recovery_timeout_ms)
            .then_some(State::PidDisabled),
    }
}

fn pid_normal(input: &Inputs) -> Option<State> {
    if !input.pid_runtime_enabled {
        return Some(State::PidDisabled);
    }
    if input.brew_start_requested {
        return Some(preinfusion_entry(input));
    }
    if input.steam_requested {
        return Some(State::SteamRunning);
    }
    if input.backflush_mode_active {
        return Some(State::BackflushIdle);
    }
    if input.manual_flush_requested {
        return Some(State::ManualFlushRunning);
    }
    if input.standby_requested || input.state_elapsed_ms >= input.standby_timeout_ms {
        return Some(State::Standby);
    }
    None
}

/// Manual brew skips pre-infusion; automatic brew does not.
fn preinfusion_entry(input: &Inputs) -> State {
    if input.manual_brew_mode || !input.preinfusion_enabled {
        State::BrewRunning
    } else {
        State::BrewPreinfusion
    }
}

/// The temperature-control state the machine rests in. The C++ `getPidState()`.
fn parking_state(input: &Inputs) -> State {
    if input.pid_runtime_enabled {
        State::PidNormal
    } else {
        State::PidDisabled
    }
}

/// The actuator command for a state, before the hot-water switch is considered.
///
/// The C++ firmware spread this across each state's `onEntryImpl`, `onExitImpl` and `update()`,
/// and the three drifted. Here it is one match, so a state cannot enter doing something different
/// from what it re-asserts every tick.
pub const fn actuators_for(state: State) -> Actuators {
    match state {
        // The water-flow states that also heat: pump on, valve open.
        State::BrewPreinfusion | State::BrewRunning | State::ManualFlushRunning => {
            Actuators::PUMP_AND_HEATER
        }
        // The pre-infusion pause: the valve stays open to wet the puck, the pump stops, and the
        // heater keeps running. `BrewStates.cpp:155-156` and `:173-174`.
        State::BrewPreinfusionPause => Actuators::PAUSE,
        // The backflush fill phase pushes water into the group with the heater off, like every
        // other backflush phase. `BackflushStates.cpp:62-63`.
        State::BackflushFilling => Actuators {
            pump: true,
            water_valve: true,
            steam_valve: false,
            heater_enabled: false,
        },
        // The backflush flush phase drains: valve open, pump off, heater off.
        //
        // This is a deliberate deviation from the C++. `BackflushFlushingState::onEntryImpl`
        // calls `cleanupPumpAndValve`, which closes the valve (`BackflushStates.cpp:97-98`), but
        // the same file logs "flushing into drip tray", and `BrewHandler::valveSafetyShutdownCheck`
        // explicitly whitelists `BACKFLUSH_FLUSHING` as a state that may hold the valve open
        // (`BrewHandler.h:114`). The C++ contradicts itself; with the valve shut, the group
        // cannot drain, which is what the phase is for. Recorded as defect D49.
        State::BackflushFlushing => Actuators::DRAIN,
        // Heating or ready to, no water moving.
        State::Init | State::PidNormal | State::BrewFinished | State::SteamRunning => {
            Actuators::HEATER_ONLY
        }
        // The backflush idle and finished states hold the heater off. The C++
        // `shouldPIDBeEnabled` inhibited the heater in all four backflush phases, and a backflush
        // that heats the boiler is a backflush that scalds the user.
        State::BackflushIdle | State::BackflushFinished => Actuators::ALL_OFF,
        // Everything else is off, including every fault state. That is the fix for the C++ "safe
        // mode", which was a log-only stub.
        State::WaterTankEmpty
        | State::EmergencyStop
        | State::PidDisabled
        | State::Standby
        | State::SensorError
        | State::EepromError => Actuators::ALL_OFF,
    }
}

/// The full command, including the hot-water dispense.
///
/// The C++ ran a pump inside `PID_NORMAL` and `STEAM_RUNNING` whenever the hot-water switch was
/// held, with no state of its own (`PidStates.cpp:33-43`). That is the one pump not derivable
/// from the state alone, so it is an explicit input rather than a hidden special case.
pub const fn actuators_with_switches(state: State, hot_water_held: bool) -> Actuators {
    let base = actuators_for(state);
    if !hot_water_held {
        return base;
    }
    // Only the two states the C++ allowed it from, and never into a fault or a fault-adjacent
    // state: holding the switch must not defeat an interlock.
    if matches!(state, State::PidNormal | State::SteamRunning) {
        Actuators {
            pump: true,
            water_valve: true,
            steam_valve: false,
            heater_enabled: base.heater_enabled,
        }
    } else {
        base
    }
}

/// Whether the heater may be controlled from here.
///
/// The C++ `shouldPIDBeEnabled` is conditional in two places that this reproduces: an empty tank
/// may still heat when `keep_heater_on_empty` is set, and the whole list is suppressed while the
/// brew PID delay is active.
pub const fn heater_allowed(state: State, input: &Inputs) -> bool {
    if input.brew_pid_delay_active {
        return false;
    }
    match state {
        State::WaterTankEmpty => input.keep_heater_on_empty,
        State::PidDisabled
        | State::SensorError
        | State::EmergencyStop
        | State::EepromError
        | State::Standby
        | State::BackflushIdle
        | State::BackflushFilling
        | State::BackflushFlushing
        | State::BackflushFinished => false,
        State::Init
        | State::PidNormal
        | State::BrewPreinfusion
        | State::BrewPreinfusionPause
        | State::BrewRunning
        | State::BrewFinished
        | State::ManualFlushRunning
        | State::SteamRunning => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::ALL_STATES;

    fn inputs() -> Inputs {
        Inputs {
            emergency_stop: false,
            sensor_fault: false,
            water_tank_full: true,
            pid_runtime_enabled: true,
            standby_timeout_ms: u32::MAX,
            sensor_error_recovery_ms: 5000,
            eeprom_recovery_timeout_ms: u32::MAX,
            backflush_cycles: 3,
            brew_total_target_ms: 34_000,
            ..Inputs::default()
        }
    }

    #[test]
    fn emergency_stop_outranks_everything() {
        let i = Inputs {
            emergency_stop: true,
            sensor_fault: true,
            water_tank_full: false,
            ..inputs()
        };
        assert_eq!(
            next_state(State::BrewRunning, &i),
            Some(State::EmergencyStop)
        );
        assert_eq!(next_state(State::PidNormal, &i), Some(State::EmergencyStop));
    }

    #[test]
    fn the_cross_cutting_priority_order_is_the_cpp_one() {
        assert_eq!(
            next_state(
                State::PidNormal,
                &Inputs {
                    emergency_stop: true,
                    sensor_fault: true,
                    ..inputs()
                }
            ),
            Some(State::EmergencyStop),
            "emergency outranks a sensor fault"
        );
        assert_eq!(
            next_state(
                State::PidNormal,
                &Inputs {
                    sensor_fault: true,
                    water_tank_full: false,
                    ..inputs()
                }
            ),
            Some(State::SensorError),
            "a sensor fault outranks an empty tank"
        );
        assert_eq!(
            next_state(
                State::PidNormal,
                &Inputs {
                    water_tank_full: false,
                    pid_runtime_enabled: false,
                    ..inputs()
                }
            ),
            Some(State::WaterTankEmpty),
            "an empty tank outranks a PID disable"
        );
    }

    #[test]
    fn an_empty_tank_does_not_trap_or_wake_on_its_own() {
        let empty = Inputs {
            water_tank_full: false,
            ..inputs()
        };
        assert_eq!(next_state(State::WaterTankEmpty, &empty), None);
        assert_eq!(next_state(State::Standby, &empty), None);
        // An explicit user request still wakes it: a different input.
        let asked = Inputs {
            water_tank_full: false,
            normal_operation_requested: true,
            ..inputs()
        };
        assert_eq!(next_state(State::Standby, &asked), Some(State::PidNormal));
    }

    #[test]
    fn a_refill_beats_a_standby_request_in_the_same_tick() {
        // ErrorStates.cpp:75-82 checks the refill first. Getting this backwards strands the
        // machine in standby, because an empty tank cannot wake it.
        let i = Inputs {
            water_tank_full: true,
            standby_requested: true,
            ..inputs()
        };
        assert_eq!(
            next_state(State::WaterTankEmpty, &i),
            Some(State::PidNormal)
        );
    }

    #[test]
    fn pid_disable_is_suppressed_in_the_states_that_are_already_off() {
        // PID stays enabled here, so a fault state that recovers lands in PID_NORMAL and a
        // transition to PID_DISABLED can only have come from the cross-cutting check.
        let off = Inputs {
            pid_runtime_enabled: true,
            ..inputs()
        };
        for state in [
            State::PidDisabled,
            State::Standby,
            State::EmergencyStop,
            State::SensorError,
            State::WaterTankEmpty,
            State::EepromError,
        ] {
            assert_ne!(
                next_state(state, &off),
                Some(State::PidDisabled),
                "{state} must not be forced to PID_DISABLED by the cross-cutting check"
            );
        }
    }

    #[test]
    fn pid_disable_catches_an_active_operation() {
        let off = Inputs {
            pid_runtime_enabled: false,
            ..inputs()
        };
        for state in [
            State::BrewPreinfusion,
            State::BrewRunning,
            State::SteamRunning,
            State::ManualFlushRunning,
            State::BackflushFilling,
            State::BackflushFlushing,
        ] {
            assert_eq!(
                next_state(state, &off),
                Some(State::PidDisabled),
                "{state} must be forced out to PID_DISABLED"
            );
        }
    }

    #[test]
    fn turning_the_heater_off_from_pid_normal_enters_pid_disabled() {
        // PidStates.cpp:48-51. That is what the state is for.
        let off = Inputs {
            pid_runtime_enabled: false,
            ..inputs()
        };
        assert_eq!(next_state(State::PidNormal, &off), Some(State::PidDisabled));
    }

    #[test]
    fn init_picks_the_parking_state() {
        assert_eq!(next_state(State::Init, &inputs()), Some(State::PidNormal));
        let off = Inputs {
            pid_runtime_enabled: false,
            ..inputs()
        };
        assert_eq!(next_state(State::Init, &off), Some(State::PidDisabled));
    }

    #[test]
    fn a_brew_start_uses_preinfusion_only_in_automatic_mode() {
        let auto = Inputs {
            brew_start_requested: true,
            preinfusion_enabled: true,
            manual_brew_mode: false,
            ..inputs()
        };
        assert_eq!(
            next_state(State::PidNormal, &auto),
            Some(State::BrewPreinfusion)
        );
        let manual = Inputs {
            brew_start_requested: true,
            preinfusion_enabled: true,
            manual_brew_mode: true,
            ..inputs()
        };
        assert_eq!(
            next_state(State::PidNormal, &manual),
            Some(State::BrewRunning)
        );
    }

    #[test]
    fn preinfusion_times_out_into_the_pause_then_into_running() {
        let base = Inputs {
            preinfusion_enabled: true,
            preinfusion_ms: 2000,
            preinfusion_pause_ms: 5000,
            manual_brew_mode: false,
            ..inputs()
        };
        assert_eq!(
            next_state(
                State::BrewPreinfusion,
                &Inputs {
                    state_elapsed_ms: 1999,
                    ..base
                }
            ),
            None
        );
        assert_eq!(
            next_state(
                State::BrewPreinfusion,
                &Inputs {
                    state_elapsed_ms: 2000,
                    ..base
                }
            ),
            Some(State::BrewPreinfusionPause)
        );
        let no_pause = Inputs {
            preinfusion_ms: 2000,
            preinfusion_pause_ms: 0,
            state_elapsed_ms: 2000,
            manual_brew_mode: false,
            ..inputs()
        };
        assert_eq!(
            next_state(State::BrewPreinfusion, &no_pause),
            Some(State::BrewRunning)
        );
        assert_eq!(
            next_state(
                State::BrewPreinfusionPause,
                &Inputs {
                    state_elapsed_ms: 4999,
                    ..base
                }
            ),
            None
        );
        assert_eq!(
            next_state(
                State::BrewPreinfusionPause,
                &Inputs {
                    state_elapsed_ms: 5000,
                    ..base
                }
            ),
            Some(State::BrewRunning)
        );
    }

    #[test]
    fn a_stop_request_parks_the_machine_where_it_should() {
        let stop = Inputs {
            brew_stop_requested: true,
            ..inputs()
        };
        for state in [State::BrewPreinfusion, State::BrewPreinfusionPause] {
            assert_eq!(next_state(state, &stop), Some(State::PidNormal));
        }
        assert_eq!(
            next_state(State::BrewRunning, &stop),
            Some(State::BrewFinished)
        );
    }

    #[test]
    fn brew_by_time_uses_the_total_including_preinfusion() {
        // BrewStates.cpp:279-289 compares a total against a total. With the shipped config the
        // target is 27 s of which 7 s is pre-infusion and pause, so the shot must stop at 27 s
        // from the start of the brew, not 34 s from the start of the flow.
        let by_time = Inputs {
            brew_by_time_enabled: true,
            brew_total_target_ms: 34_000,
            brew_elapsed_ms: 33_999,
            ..inputs()
        };
        assert_eq!(next_state(State::BrewRunning, &by_time), None);
        let done = Inputs {
            brew_by_time_enabled: true,
            brew_total_target_ms: 34_000,
            brew_elapsed_ms: 34_000,
            ..inputs()
        };
        assert_eq!(
            next_state(State::BrewRunning, &done),
            Some(State::BrewFinished)
        );
        // A zero total disables the time stop rather than stopping immediately.
        let zero = Inputs {
            brew_by_time_enabled: true,
            brew_total_target_ms: 0,
            brew_elapsed_ms: 99_999,
            ..inputs()
        };
        assert_eq!(next_state(State::BrewRunning, &zero), None);
    }

    #[test]
    fn brew_by_weight_stops_on_the_scale() {
        let light = Inputs {
            brew_by_weight_enabled: true,
            brew_weight_g: 30.0,
            brew_target_weight_g: 36.0,
            brew_elapsed_ms: 99_999,
            ..inputs()
        };
        assert_eq!(next_state(State::BrewRunning, &light), None);
        let heavy = Inputs {
            brew_by_weight_enabled: true,
            brew_weight_g: 36.0,
            brew_target_weight_g: 36.0,
            brew_elapsed_ms: 99_999,
            ..inputs()
        };
        assert_eq!(
            next_state(State::BrewRunning, &heavy),
            Some(State::BrewFinished)
        );
    }

    #[test]
    fn manual_mode_ignores_both_automatic_stop_conditions() {
        // BrewStates.cpp:277 gates the whole block on automatic mode.
        let manual = Inputs {
            manual_brew_mode: true,
            brew_by_time_enabled: true,
            brew_total_target_ms: 1000,
            brew_elapsed_ms: 99_999,
            brew_by_weight_enabled: true,
            brew_weight_g: 500.0,
            brew_target_weight_g: 36.0,
            ..inputs()
        };
        assert_eq!(next_state(State::BrewRunning, &manual), None);
    }

    #[test]
    fn brew_finished_returns_to_preinfusion_regardless_of_mode() {
        // BrewStates.cpp:330-335 is unconditional.
        let manual = Inputs {
            brew_start_requested: true,
            manual_brew_mode: true,
            ..inputs()
        };
        assert_eq!(
            next_state(State::BrewFinished, &manual),
            Some(State::BrewPreinfusion)
        );
    }

    #[test]
    fn manual_flush_and_backflush_enter_are_separate_edges() {
        let flush = Inputs {
            manual_flush_requested: true,
            backflush_mode_active: false,
            ..inputs()
        };
        assert_eq!(
            next_state(State::PidNormal, &flush),
            Some(State::ManualFlushRunning)
        );
        let backflush = Inputs {
            backflush_mode_active: true,
            backflush_cycle_start: true,
            ..inputs()
        };
        assert_eq!(
            next_state(State::PidNormal, &backflush),
            Some(State::BackflushIdle)
        );
        assert_eq!(
            next_state(State::BackflushIdle, &backflush),
            Some(State::BackflushFilling)
        );
    }

    #[test]
    fn a_full_backflush_runs_the_configured_cycles() {
        let base = Inputs {
            backflush_mode_active: true,
            backflush_fill_ms: 5000,
            backflush_flush_ms: 10_000,
            backflush_cycles: 3,
            ..inputs()
        };
        assert_eq!(
            next_state(
                State::BackflushFilling,
                &Inputs {
                    state_elapsed_ms: 5000,
                    ..base
                }
            ),
            Some(State::BackflushFlushing)
        );
        assert_eq!(
            next_state(
                State::BackflushFlushing,
                &Inputs {
                    state_elapsed_ms: 9999,
                    backflush_current_cycle: 0,
                    ..base
                }
            ),
            None
        );
        for cycle in 0..2u8 {
            assert_eq!(
                next_state(
                    State::BackflushFlushing,
                    &Inputs {
                        state_elapsed_ms: 10_000,
                        backflush_current_cycle: cycle,
                        ..base
                    }
                ),
                Some(State::BackflushFilling),
                "cycle {cycle} should repeat"
            );
        }
        assert_eq!(
            next_state(
                State::BackflushFlushing,
                &Inputs {
                    state_elapsed_ms: 10_000,
                    backflush_current_cycle: 2,
                    ..base
                }
            ),
            Some(State::BackflushFinished)
        );
    }

    #[test]
    fn a_maxed_cycle_index_does_not_overflow_into_an_infinite_backflush() {
        // A u8 at 255 plus one wraps to 0 in a release build, which would look like "back to the
        // first cycle" and loop forever. The release build has overflow checks off, so this is
        // the case that matters.
        let i = Inputs {
            backflush_mode_active: true,
            backflush_flush_ms: 10_000,
            backflush_cycles: 255,
            backflush_current_cycle: 255,
            state_elapsed_ms: 10_000,
            ..inputs()
        };
        assert_eq!(
            next_state(State::BackflushFlushing, &i),
            Some(State::BackflushFinished)
        );
    }

    #[test]
    fn turning_backflush_off_parks_the_machine_from_any_phase() {
        let off = Inputs {
            backflush_mode_active: false,
            ..inputs()
        };
        for state in [
            State::BackflushIdle,
            State::BackflushFilling,
            State::BackflushFlushing,
            State::BackflushFinished,
        ] {
            assert_eq!(next_state(state, &off), Some(State::PidNormal), "{state}");
        }
    }

    #[test]
    fn a_sensor_fault_recovers_only_after_the_full_delay_from_the_last_bad_reading() {
        let clearing = Inputs {
            sensor_fault: false,
            fault_clear_elapsed_ms: 4999,
            ..inputs()
        };
        assert_eq!(next_state(State::SensorError, &clearing), None);
        let recovered = Inputs {
            sensor_fault: false,
            fault_clear_elapsed_ms: 5000,
            ..inputs()
        };
        assert_eq!(
            next_state(State::SensorError, &recovered),
            Some(State::PidNormal)
        );
    }

    #[test]
    fn the_eeprom_state_recovers_on_a_timeout_not_on_a_sensor_reading() {
        // ErrorStates.cpp:111-124. Keying it on the sensor flag meant it left on the first tick
        // whenever the sensor was healthy, which was not the intent.
        let healthy = Inputs {
            sensor_fault: false,
            state_elapsed_ms: 0,
            eeprom_recovery_timeout_ms: 300_000,
            ..inputs()
        };
        assert_eq!(next_state(State::EepromError, &healthy), None);
        let timed_out = Inputs {
            state_elapsed_ms: 300_001,
            eeprom_recovery_timeout_ms: 300_000,
            ..inputs()
        };
        assert_eq!(
            next_state(State::EepromError, &timed_out),
            Some(State::PidDisabled)
        );
    }

    #[test]
    fn standby_wakes_only_for_real_activity() {
        let idle = inputs();
        assert_eq!(next_state(State::Standby, &idle), None);
        for req in [
            Inputs {
                normal_operation_requested: true,
                ..inputs()
            },
            Inputs {
                brew_start_requested: true,
                ..inputs()
            },
            Inputs {
                steam_requested: true,
                ..inputs()
            },
        ] {
            assert_eq!(next_state(State::Standby, &req), Some(State::PidNormal));
        }
    }

    #[test]
    fn every_state_has_an_explicit_actuator_command_that_matches_the_interlocks() {
        for state in ALL_STATES {
            let a = actuators_for(state);
            assert_eq!(
                a.water_valve,
                state.may_hold_water_valve_open(),
                "{state}: valve command disagrees with the interlock"
            );
            assert_eq!(
                a.pump,
                state.may_run_pump(),
                "{state}: pump command disagrees with the interlock"
            );
            assert!(!a.steam_valve, "{state} must not open the steam valve");
            assert_eq!(
                a.heater_enabled,
                heater_allowed(state, &inputs()),
                "{state}: heater command disagrees with heater_allowed"
            );
        }
    }

    #[test]
    fn a_fault_state_never_moves_water_or_heat() {
        for state in [
            State::EmergencyStop,
            State::SensorError,
            State::EepromError,
            State::PidDisabled,
            State::Standby,
        ] {
            let a = actuators_for(state);
            assert!(!a.flowing(), "{state} must not move any water");
            assert!(!a.heater_enabled, "{state} must not heat");
        }
    }

    #[test]
    fn the_backflush_flush_phase_drains_with_the_pump_stopped() {
        // Fill pushes water into the group; flush opens the valve and lets it drain by gravity.
        // The C++ closed the valve here (defect D49), which would leave the group unable to
        // drain, contradicting its own log message and its own interlock whitelist.
        let a = actuators_for(State::BackflushFlushing);
        assert!(!a.pump, "the flush phase must not run the pump");
        assert!(
            a.water_valve,
            "the flush phase must open the valve to drain"
        );
        assert!(!a.heater_enabled);
    }

    #[test]
    fn the_backflush_fill_phase_pushes_water_with_the_heater_off() {
        let a = actuators_for(State::BackflushFilling);
        assert!(a.pump);
        assert!(a.water_valve);
        assert!(!a.heater_enabled, "a backflush must not heat the boiler");
    }

    #[test]
    fn the_preinfusion_pause_wets_with_the_pump_stopped() {
        let a = actuators_for(State::BrewPreinfusionPause);
        assert!(!a.pump);
        assert!(a.water_valve);
        assert!(
            a.heater_enabled,
            "the C++ firmware kept the heater on through the pause"
        );
    }

    #[test]
    fn the_hot_water_switch_pumps_only_from_the_two_states_the_cpp_allowed() {
        // PidStates.cpp:33-43 and SteamStates.cpp:36-46.
        assert!(actuators_with_switches(State::PidNormal, true).pump);
        assert!(actuators_with_switches(State::SteamRunning, true).pump);
        assert!(!actuators_with_switches(State::PidNormal, false).pump);
        // Only states whose base command has the pump off, so the assertion is about the switch
        // and not about a state that already pumps.
        for state in [
            State::EmergencyStop,
            State::SensorError,
            State::EepromError,
            State::WaterTankEmpty,
            State::BrewPreinfusionPause,
            State::BackflushFlushing,
            State::Standby,
            State::BrewFinished,
        ] {
            assert!(
                !actuators_for(state).pump,
                "{state} is expected not to pump on its own"
            );
            assert!(
                !actuators_with_switches(state, true).pump,
                "{state} must not be pumpable by holding the switch"
            );
        }
    }

    #[test]
    fn an_empty_tank_may_still_heat_only_when_configured_to() {
        let keep = Inputs {
            keep_heater_on_empty: true,
            ..inputs()
        };
        assert!(heater_allowed(State::WaterTankEmpty, &keep));
        assert!(!heater_allowed(State::WaterTankEmpty, &inputs()));
    }

    #[test]
    fn the_brew_pid_delay_suppresses_the_heater_everywhere() {
        // ProcessController.cpp:254 gates the whole list on the brew PID delay.
        let delay = Inputs {
            brew_pid_delay_active: true,
            ..inputs()
        };
        for state in ALL_STATES {
            assert!(
                !heater_allowed(state, &delay),
                "{state} must not heat during the brew PID delay"
            );
        }
    }
}
