//! The global transition guards, in a fixed order.
//!
//! Four conditions are checked before a state considers its own transitions:
//!
//! ```text
//! emergency stop  -> EMERGENCY_STOP   (no exclusions)
//! sensor error    -> SENSOR_ERROR     (no exclusions)
//! tank not full   -> WATER_TANK_EMPTY (except WATER_TANK_EMPTY and STANDBY)
//! PID not wanted  -> PID_DISABLED     (except the eight states listed below)
//! ```
//!
//! # The exclusion lists are `match`es with no wildcard arm
//!
//! "This state is excluded" is a property of the *value* here, expressed by two `match`es
//! with no `_` arm -- the same mechanism [`cc_safety::water_flow_allowed`] uses. Adding a
//! 19th state therefore fails to compile in four places (this file,
//! [`cc_safety::water_flow_allowed`], [`states::on_entry`](crate::states::on_entry),
//! [`states::on_exit`](crate::states::on_exit)) instead of silently inheriting whichever
//! behaviour the new variant happened to fall into.
//!
//! # The order is load-bearing
//!
//! Emergency beats sensor error beats tank-empty beats PID-disabled. More than one can be
//! true at once -- a dry tank *and* a hot boiler *and* a dead probe -- and the order decides
//! which state the operator sees. It is pinned by
//! `guards::emergency_beats_sensor_error`, `guards::sensor_error_beats_tank_empty` and
//! `guards::tank_empty_beats_pid_disabled` in `tests/exhaustive_state_event.rs`.

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
    /// An emergency stop was requested: `EMERGENCY_STOP`, from any state.
    EmergencyStop,
    /// A sensor has failed: `SENSOR_ERROR`, from any state.
    SensorError,
    /// The tank is empty: `WATER_TANK_EMPTY`.
    WaterTankEmpty,
    /// The PID is not wanted at runtime: `PID_DISABLED`.
    PidRuntimeDisabled,
    /// No global guard fired; [`states::check_specific`](crate::states::check_specific)
    /// decides.
    None,
}

/// The destination of a global guard, or `None` if none fired.
///
/// # The emergency guard has **no** exclusion
///
/// It fires even in `EMERGENCY_STOP` itself, so that state's guard is a *self*-transition,
/// which the reducer discards. The observable effect is "stay put, and the tick has already
/// re-run the emergency shutdown": returning to the same state re-applies its entry, which
/// is what keeps the relays down while the stop is latched. This function reports the guard;
/// discarding the self-transition is the reducer's job.
#[must_use]
pub const fn global_guard(machine: &Machine) -> Guard {
    if machine.is_emergency_stop() {
        return Guard::EmergencyStop;
    }
    if machine.has_sensor_error() {
        return Guard::SensorError;
    }
    // Excluded in WATER_TANK_EMPTY, so its own refill and standby logic can run, and in
    // STANDBY, where the heater is already off and an empty tank must not wake the machine.
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

/// Whether the tank-empty guard skips this state.
///
/// `WATER_TANK_EMPTY` is excluded so it reaches its own transitions, which honour a standby
/// request or timeout while the tank stays empty. `STANDBY` is excluded so a dry tank cannot
/// drag the machine out of power saving -- the heater is already off there, so there is
/// nothing to protect against.
#[must_use]
pub const fn excluded_from_tank_check(state: MachineState) -> bool {
    matches!(state, MachineState::WaterTankEmpty | MachineState::Standby)
}

/// Whether the PID-disabled guard skips this state.
///
/// Eight exclusions, for three reasons:
///
/// * `PID_DISABLED` is the destination, so the guard would be a self-transition.
/// * `PID_NORMAL` and `INIT` handle this condition themselves, in
///   [`states::check_specific`](crate::states::check_specific), because each routes on it
///   differently: `PID_NORMAL` leaves for `PID_DISABLED`, and `INIT` keeps its own
///   start-up sequence in charge.
/// * `STANDBY` means "PID intentionally off". **This is why standby is a latched state and
///   not a timeout**: the runtime flag is off for as long as the machine is in standby, so
///   without the exclusion the guard would fire every tick.
///
/// `EMERGENCY_STOP`, `SENSOR_ERROR`, `WATER_TANK_EMPTY` and `EEPROM_ERROR` are the error
/// states, which must be able to recover to a PID state rather than being bounced out of
/// recovery by the guard.
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

/// Whether the heater's PID may run in this state.
///
/// A second, *independent* gate from the state machine's: this one is about the heater, not
/// about which state the machine is in. The machine can sit in `PID_NORMAL` with the runtime
/// flag on while this says no, and then the heater stays off.
///
/// The PID is off in `PID_DISABLED`, `SENSOR_ERROR`, `EMERGENCY_STOP`, `EEPROM_ERROR` and
/// `STANDBY`; through every backflush state, because a backflush is a cold-water operation;
/// whenever the brew switch asks for it ([`brew_pid_disabled`](Self)); and in
/// `WATER_TANK_EMPTY` unless `hardware.sensors.watertank.keep_heater_on_empty` is set, which
/// is the one case where an empty tank is allowed to heat.
///
/// # The backflush exclusion is worth reading twice
///
/// It is why the *water* valve's whitelist includes `BACKFLUSH_FILLING` and
/// `BACKFLUSH_FLUSHING` while the heater is excluded from both: the same states legitimately
/// need water flowing and must not have heat.
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
        // Ten states in, two out.
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
        let mut excluded: Vec<MachineState> = cc_domain::state::ALL
            .iter()
            .copied()
            .filter(|s| excluded_from_pid_check(*s))
            .collect();
        // Compare as sets: `cc_domain::state::ALL` is in discriminant order and the
        // expectation below is not.
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
        // The emergency guard has no exclusion, so it fires even in EMERGENCY_STOP. The
        // self-transition is discarded downstream.
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
        // With the override off, an empty tank stops the heater; with it on, the machine
        // heats anyway, which is the flag `WATER_TANK_EMPTY`'s own documentation warns
        // about.
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
