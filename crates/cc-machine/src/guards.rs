//! The global transition guards, in the C++'s order.
//!
//! Port of `BaseState<StateId, DerivedState>::checkTransitions`
//! (`include/clevercoffee/state/BaseState.h:136-175`):
//!
//! ```cpp
//! if (context.isEmergencyStop())  return EMERGENCY_STOP;
//! if (context.hasSensorError())   return SENSOR_ERROR;
//! if constexpr (StateId != WATER_TANK_EMPTY && StateId != STANDBY) {
//!     if (!context.isWaterTankFull()) return WATER_TANK_EMPTY;
//! }
//! if constexpr (StateId != PID_DISABLED && StateId != PID_NORMAL && ... ) {
//!     if (!context.isPidRuntimeEnabled()) return PID_DISABLED;
//! }
//! return checkSpecificTransitions(context);
//! ```
//!
//! # Two things the `if constexpr` becomes here
//!
//! The C++ compiles the exclusion lists away, so "this state is excluded" is a
//! property of the *type*. Here it is a property of the *value*, expressed by
//! two `match`es with no wildcard arm — the same mechanism ADR-0004 adopts for
//! `cc_safety::water_flow_allowed`. Adding a 19th state therefore fails to
//! compile in four places (this file, `cc_safety::water_flow_allowed`,
//! [`states::on_entry`](crate::states::on_entry),
//! [`states::on_exit`](crate::states::on_exit)) instead of silently inheriting
//! whichever behaviour the new variant happened to fall into.
//!
//! # The order is load-bearing
//!
//! Emergency beats sensor error beats tank-empty beats PID-disabled. Each of
//! those can be true at the same time — a dry tank *and* a hot boiler *and* a
//! dead probe — and the order decides which state the operator sees. The C++
//! order is reproduced exactly and pinned by
//! `guards::emergency_beats_sensor_error`, `guards::sensor_error_beats_tank_empty`
//! and `guards::tank_empty_beats_pid_disabled` in `tests/exhaustive_state_event.rs`.

use cc_domain::state::MachineState;

use crate::machine::Machine;

/// Which global guard fired, if any.
///
/// Kept separate from [`MachineState`] so the exhaustive table can assert *which
/// rule* produced a transition and not merely where it went — two different
/// rules can produce the same destination from different states, and a table
/// that only checked the destination would let them be swapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guard {
    /// `context.isEmergencyStop()` → `EMERGENCY_STOP`
    /// (`BaseState.h:139-142`). **No exclusions.**
    EmergencyStop,
    /// `context.hasSensorError()` → `SENSOR_ERROR` (`BaseState.h:145-148`).
    /// **No exclusions.**
    SensorError,
    /// `!context.isWaterTankFull()` → `WATER_TANK_EMPTY` (`BaseState.h:153-158`).
    WaterTankEmpty,
    /// `!context.isPidRuntimeEnabled()` → `PID_DISABLED` (`BaseState.h:163-171`).
    PidRuntimeDisabled,
    /// No global guard fired; [`states::check_specific`](crate::states::check_specific)
    /// decides.
    None,
}

/// The destination of a global guard, or `None` if none fired.
///
/// # Guard 1: emergency stop has **no** exclusion
///
/// `BaseState.h:139-142` has no `if constexpr`, so it fires even in
/// `EMERGENCY_STOP` itself. The result is a *self*-transition, which
/// `StateMachine::executeTransition` discards
/// (`StateMachine.cpp:107-111`, "Skipping self-transition"), so the observable
/// effect is "stay put, and `update()` has already re-run the emergency
/// shutdown". This function returns `EmergencyStop` and the reducer performs the
/// self-transition check, which is where the C++ puts it.
#[must_use]
pub const fn global_guard(machine: &Machine) -> Guard {
    if machine.is_emergency_stop() {
        return Guard::EmergencyStop;
    }
    if machine.has_sensor_error() {
        return Guard::SensorError;
    }
    // `BaseState.h:153`: excluded in WATER_TANK_EMPTY (so its own refill and
    // standby logic can run) and in STANDBY (heater already off; an empty tank
    // must not wake the machine).
    if !excluded_from_tank_check(machine.state) && !machine.is_water_tank_full() {
        return Guard::WaterTankEmpty;
    }
    if !excluded_from_pid_check(machine.state) && !machine.is_pid_runtime_enabled() {
        return Guard::PidRuntimeDisabled;
    }
    Guard::None
}

/// The state a fired guard transitions to.
#[must_use]
pub const fn guard_destination(guard: Guard) -> Option<MachineState> {
    match guard {
        Guard::EmergencyStop => Some(MachineState::EmergencyStop),
        Guard::SensorError => Some(MachineState::SensorError),
        Guard::WaterTankEmpty => Some(MachineState::WaterTankEmpty),
        Guard::PidRuntimeDisabled => Some(MachineState::PidDisabled),
        Guard::None => None,
    }
}

/// `if constexpr (StateId != WATER_TANK_EMPTY && StateId != STANDBY)`
/// (`BaseState.h:153`).
///
/// The C++ comment gives both reasons: `WATER_TANK_EMPTY` must "reach its
/// specific transitions to honor standby request/timeout while the tank stays
/// empty", and `STANDBY` must not be dragged out of power saving by a dry tank.
#[must_use]
pub const fn excluded_from_tank_check(state: MachineState) -> bool {
    matches!(state, MachineState::WaterTankEmpty | MachineState::Standby)
}

/// `if constexpr (StateId != PID_DISABLED && StateId != PID_NORMAL &&
/// StateId != STANDBY && StateId != INIT && StateId != EMERGENCY_STOP &&
/// StateId != SENSOR_ERROR && StateId != WATER_TANK_EMPTY &&
/// StateId != EEPROM_ERROR)` (`BaseState.h:163-166`).
///
/// Eight exclusions, and the C++ comment explains only some of them:
///
/// * `PID_DISABLED` — already there.
/// * `PID_NORMAL` — "handles it explicitly": `PidNormalState::checkSpecificTransitions`
///   opens with its own `if (!isPidRuntimeEnabled())` (`PidStates.cpp:48-51`).
/// * `STANDBY` — "PID intentionally off". **This is why standby is a latched
///   state and not a timeout**: the runtime flag is off for as long as the
///   machine is in standby, so including it here would make the guard a no-op
///   cycle.
/// * `INIT` — `InitState::checkSpecificTransitions` routes on exactly this
///   condition (`InitState.cpp:24-31`).
/// * `EMERGENCY_STOP`, `SENSOR_ERROR`, `WATER_TANK_EMPTY`, `EEPROM_ERROR` — the
///   error states, which must be able to recover to a PID state rather than
///   being bounced out of recovery by the guard.
#[must_use]
pub const fn excluded_from_pid_check(state: MachineState) -> bool {
    matches!(
        state,
        MachineState::PidDisabled
            | MachineState::PidNormal
            | MachineState::Standby
            | MachineState::Init
            | MachineState::EmergencyStop
            | MachineState::SensorError
            | MachineState::WaterTankEmpty
            | MachineState::EepromError
    )
}

/// `ProcessController::shouldPIDBeEnabled` (`ProcessController.cpp:246-255`).
///
/// A second, *independent* PID gate: this one is about the heater, not about
/// the state machine. The state machine can be in `PID_NORMAL` with the runtime
/// flag on while this says no, and then the heater is off.
///
/// ```cpp
/// const bool tankEmptyBlocksHeater =
///     machineState == WATER_TANK_EMPTY && !keepHeaterOnEmpty;
/// return !(machineState == PID_DISABLED || tankEmptyBlocksHeater ||
///          machineState == SENSOR_ERROR || machineState == EMERGENCY_STOP ||
///          machineState == EEPROM_ERROR || machineState == STANDBY ||
///          isBackflushState(machineState) || isProcessBrewPidDisabled());
/// ```
///
/// # The backflush exclusion is worth reading twice
///
/// The PID is off for the whole backflush. That is deliberate — the backflush
/// is a cold-water operation and the heater must not run against it — and it is
/// also why the *water* valve still has a whitelist that includes
/// `BACKFLUSH_FILLING` and `BACKFLUSH_FLUSHING` while the heater does not.
#[must_use]
pub fn should_pid_be_enabled(
    state: MachineState,
    keep_heater_on_empty: bool,
    brew_pid_disabled: bool,
) -> bool {
    let tank_empty_blocks_heater = state == MachineState::WaterTankEmpty && !keep_heater_on_empty;
    !(state == MachineState::PidDisabled
        || tank_empty_blocks_heater
        || state == MachineState::SensorError
        || state == MachineState::EmergencyStop
        || state == MachineState::EepromError
        || state == MachineState::Standby
        || state.is_backflush_state()
        || brew_pid_disabled)
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::machine::Machine;

    fn machine_in(state: MachineState) -> Machine {
        Machine {
            state,
            initialized: true,
            ..Machine::cold()
        }
    }

    #[test]
    fn a_healthy_machine_fires_no_global_guard() {
        let m = machine_in(MachineState::PidNormal);
        assert_eq!(global_guard(&m), Guard::None);
    }

    #[test]
    fn the_tank_check_excludes_exactly_two_states() {
        // `BaseState.h:153`. Ten states in, two out.
        assert!(excluded_from_tank_check(MachineState::WaterTankEmpty));
        assert!(excluded_from_tank_check(MachineState::Standby));
        let excluded = cc_domain::state::ALL
            .iter()
            .filter(|s| excluded_from_tank_check(**s))
            .count();
        assert_eq!(excluded, 2);
    }

    #[test]
    fn the_pid_check_excludes_exactly_eight_states() {
        // `BaseState.h:163-166`.
        let mut excluded: Vec<MachineState> = cc_domain::state::ALL
            .iter()
            .copied()
            .filter(|s| excluded_from_pid_check(*s))
            .collect();
        // Compare as sets: `cc_domain::state::ALL` is in discriminant order and
        // the expectation is written in the C++'s declaration order
        // (`BaseState.h:163-166`), which is not.
        excluded.sort_by_key(|s| s.id());
        let mut expected = [
            MachineState::PidDisabled,
            MachineState::PidNormal,
            MachineState::Standby,
            MachineState::Init,
            MachineState::EmergencyStop,
            MachineState::SensorError,
            MachineState::WaterTankEmpty,
            MachineState::EepromError,
        ];
        expected.sort_by_key(|s| s.id());
        assert_eq!(excluded, expected);
    }

    #[test]
    fn the_two_exclusion_lists_overlap_in_exactly_two_states() {
        // Worth pinning because it is not obvious from either `if constexpr`:
        // `WATER_TANK_EMPTY` and `STANDBY` are excluded from *both* checks, so
        // a machine sitting in either one is not bounced out of it by a guard.
        // `STANDBY` gets away with it because its runtime PID is off for as long
        // as it is in standby, and `WATER_TANK_EMPTY` because it must be able to
        // reach its own refill and standby logic.
        let mut in_both: Vec<MachineState> = cc_domain::state::ALL
            .iter()
            .copied()
            .filter(|s| excluded_from_pid_check(*s) && excluded_from_tank_check(*s))
            .collect();
        in_both.sort_by_key(|s| s.id());
        let mut expected = [MachineState::Standby, MachineState::WaterTankEmpty];
        expected.sort_by_key(|s| s.id());
        assert_eq!(in_both, expected);

        let mut pid_only: Vec<MachineState> = cc_domain::state::ALL
            .iter()
            .copied()
            .filter(|s| excluded_from_pid_check(*s) && !excluded_from_tank_check(*s))
            .collect();
        pid_only.sort_by_key(|s| s.id());
        let mut expected_pid_only = [
            MachineState::PidDisabled,
            MachineState::PidNormal,
            MachineState::Init,
            MachineState::EmergencyStop,
            MachineState::SensorError,
            MachineState::EepromError,
        ];
        expected_pid_only.sort_by_key(|s| s.id());
        assert_eq!(pid_only, expected_pid_only);

        // No state is excluded from the tank check alone.
        let tank_only = cc_domain::state::ALL
            .iter()
            .filter(|s| !excluded_from_pid_check(**s) && excluded_from_tank_check(**s))
            .count();
        assert_eq!(tank_only, 0);
    }

    #[test]
    fn emergency_has_no_exclusion_at_all() {
        // `BaseState.h:139-142` has no `if constexpr`, so the guard fires even
        // in EMERGENCY_STOP. The self-transition is discarded downstream.
        let m = Machine {
            safety: cc_safety::SafetyState {
                last_sample_seq: None,
                latched: true,
                high_reading_count: 3,
            },
            ..machine_in(MachineState::EmergencyStop)
        };
        assert_eq!(global_guard(&m), Guard::EmergencyStop);
    }

    #[test]
    fn the_pid_gate_blocks_the_heater_in_backflush_and_standby() {
        for s in [
            MachineState::BackflushIdle,
            MachineState::BackflushFilling,
            MachineState::BackflushFlushing,
            MachineState::BackflushFinished,
            MachineState::Standby,
        ] {
            assert!(!should_pid_be_enabled(s, false, false), "{}", s.name());
        }
    }

    #[test]
    fn keep_heater_on_empty_overrides_the_tank_rule() {
        // `ProcessController.cpp:247-248`. This is the flag the WATER_TANK_EMPTY
        // state doc-comment warns about (`ErrorStates.cpp:83-85`).
        assert!(!should_pid_be_enabled(
            MachineState::WaterTankEmpty,
            false,
            false
        ));
        assert!(should_pid_be_enabled(
            MachineState::WaterTankEmpty,
            true,
            false
        ));
    }

    #[test]
    fn the_brew_delay_window_blocks_the_heater_anywhere() {
        assert!(should_pid_be_enabled(
            MachineState::BrewRunning,
            false,
            false
        ));
        assert!(!should_pid_be_enabled(
            MachineState::BrewRunning,
            false,
            true
        ));
    }

    #[test]
    fn the_pid_gate_allows_the_brew_states_otherwise() {
        for s in [
            MachineState::PidNormal,
            MachineState::BrewPreinfusion,
            MachineState::BrewPreinfusionPause,
            MachineState::BrewRunning,
            MachineState::BrewFinished,
            MachineState::ManualFlushRunning,
            MachineState::SteamRunning,
            MachineState::Init,
        ] {
            assert!(should_pid_be_enabled(s, false, false), "{}", s.name());
        }
    }
}
