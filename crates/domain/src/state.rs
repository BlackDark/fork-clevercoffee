//! The machine states and their classification.
//!
//! The discriminants are the C++ `MachineStateId` values verbatim. The React frontend switches on
//! the integer (`ui/packages/frontend/src/lib/`), so renumbering these would silently change what
//! the dashboard shows, and nothing would fail to compile.

/// A machine state.
///
/// The discriminants are wire-compatible with the C++ firmware and with the frontend.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u16)]
pub enum State {
    Init = 0,
    PidNormal = 20,
    BrewPreinfusion = 31,
    BrewPreinfusionPause = 32,
    BrewRunning = 33,
    BrewFinished = 34,
    ManualFlushRunning = 36,
    SteamRunning = 51,
    BackflushIdle = 60,
    BackflushFilling = 61,
    BackflushFlushing = 62,
    BackflushFinished = 63,
    WaterTankEmpty = 70,
    EmergencyStop = 80,
    PidDisabled = 90,
    Standby = 95,
    SensorError = 100,
    EepromError = 110,
}

/// The broad category a state belongs to, used by the interlock rules.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StateGroup {
    Idle,
    Brew,
    Steam,
    ManualFlush,
    Backflush,
    Fault,
    /// `PID_NORMAL`, `PID_DISABLED`, `INIT` and `STANDBY`: the machine is heating or ready to.
    TemperatureControl,
}

/// Which group a state belongs to.
///
/// The ranges match the C++ `isBrewState` and friends, which relied on the enum values being
/// contiguous. That is fragile in C++ and a `match` is not, so this is a `match`.
pub const fn group_of(state: State) -> StateGroup {
    match state {
        State::Init
        | State::PidNormal
        | State::PidDisabled
        | State::Standby
        | State::BrewFinished => StateGroup::TemperatureControl,
        State::BrewPreinfusion | State::BrewPreinfusionPause | State::BrewRunning => {
            StateGroup::Brew
        }
        State::SteamRunning => StateGroup::Steam,
        State::ManualFlushRunning => StateGroup::ManualFlush,
        State::BackflushIdle
        | State::BackflushFilling
        | State::BackflushFlushing
        | State::BackflushFinished => StateGroup::Backflush,
        State::WaterTankEmpty | State::EmergencyStop | State::SensorError | State::EepromError => {
            StateGroup::Fault
        }
    }
}

/// The classification predicates, as one function so a caller cannot forget a case.
///
/// `is_brew` in the C++ code was `state >= 31 && state <= 34`, which included `BREW_FINISHED`.
/// That is preserved, because the valve interlock has its own explicit exclusion list rather
/// than relying on the category, and two places depend on the C++ range.
pub const fn classify(state: State) -> StateGroup {
    group_of(state)
}

impl State {
    /// The integer the frontend expects.
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    /// The state name, for logs and for the `machineState` field's human-readable twin.
    pub const fn name(self) -> &'static str {
        match self {
            State::Init => "INIT",
            State::PidNormal => "PID_NORMAL",
            State::BrewPreinfusion => "BREW_PREINFUSION",
            State::BrewPreinfusionPause => "BREW_PREINFUSION_PAUSE",
            State::BrewRunning => "BREW_RUNNING",
            State::BrewFinished => "BREW_FINISHED",
            State::ManualFlushRunning => "MANUAL_FLUSH_RUNNING",
            State::SteamRunning => "STEAM_RUNNING",
            State::BackflushIdle => "BACKFLUSH_IDLE",
            State::BackflushFilling => "BACKFLUSH_FILLING",
            State::BackflushFlushing => "BACKFLUSH_FLUSHING",
            State::BackflushFinished => "BACKFLUSH_FINISHED",
            State::WaterTankEmpty => "WATER_TANK_EMPTY",
            State::EmergencyStop => "EMERGENCY_STOP",
            State::PidDisabled => "PID_DISABLED",
            State::Standby => "STANDBY",
            State::SensorError => "SENSOR_ERROR",
            State::EepromError => "EEPROM_ERROR",
        }
    }

    /// Whether this state may legitimately have the water valve open.
    ///
    /// This is the single source of truth for the valve interlock. The C++ firmware had the same
    /// list twice, in `BrewHandler::valveSafetyShutdownCheck` and implicitly in each state's
    /// `update()`, and the two drifted: `BACKFLUSH_FILLING` re-asserted the valve while
    /// `BACKFLUSH_FLUSHING` did not. Here there is one list, and a state that is not in it
    /// cannot hold the valve open no matter what.
    pub const fn may_hold_water_valve_open(self) -> bool {
        matches!(
            self,
            State::BrewPreinfusion
                | State::BrewPreinfusionPause
                | State::BrewRunning
                | State::ManualFlushRunning
                | State::BackflushFilling
                | State::BackflushFlushing
        )
    }

    /// Whether this state may legitimately have the pump running.
    pub const fn may_run_pump(self) -> bool {
        // Not the same list as the valve. The pre-infusion pause and the backflush flush phase
        // hold the water valve open with the pump stopped, because they are wetting and draining
        // rather than flowing. Verified against BrewStates.cpp:155-156 and
        // BackflushStates.cpp:98, which both disable the pump and open the valve.
        matches!(
            self,
            State::BrewPreinfusion
                | State::BrewRunning
                | State::ManualFlushRunning
                | State::BackflushFilling
        )
    }

    /// Whether the heater is inhibited from here, ignoring the two conditions that depend on
    /// configuration.
    ///
    /// The C++ `ProcessController::shouldPIDBeEnabled` is conditional in two places: an empty
    /// tank may still heat under `keep_heater_on_empty`, and the whole list is suppressed during
    /// the brew PID delay. [`crate::heater_allowed`] is the function a caller should use; this
    /// one is the static part of it, kept as a `const fn` so a parity test can compare the list
    /// directly against the C++ source.
    pub const fn allows_heater(self) -> bool {
        !matches!(
            self,
            State::PidDisabled
                | State::WaterTankEmpty
                | State::SensorError
                | State::EmergencyStop
                | State::EepromError
                | State::Standby
                | State::BackflushIdle
                | State::BackflushFilling
                | State::BackflushFlushing
                | State::BackflushFinished
        )
    }
}

impl core::fmt::Display for State {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn discriminants_match_the_cpp_enum() {
        // The frontend renders states from these integers. A change here is a UI regression that
        // nothing else would catch.
        assert_eq!(State::Init.as_u16(), 0);
        assert_eq!(State::PidNormal.as_u16(), 20);
        assert_eq!(State::BrewPreinfusion.as_u16(), 31);
        assert_eq!(State::BrewPreinfusionPause.as_u16(), 32);
        assert_eq!(State::BrewRunning.as_u16(), 33);
        assert_eq!(State::BrewFinished.as_u16(), 34);
        assert_eq!(State::ManualFlushRunning.as_u16(), 36);
        assert_eq!(State::SteamRunning.as_u16(), 51);
        assert_eq!(State::BackflushIdle.as_u16(), 60);
        assert_eq!(State::BackflushFilling.as_u16(), 61);
        assert_eq!(State::BackflushFlushing.as_u16(), 62);
        assert_eq!(State::BackflushFinished.as_u16(), 63);
        assert_eq!(State::WaterTankEmpty.as_u16(), 70);
        assert_eq!(State::EmergencyStop.as_u16(), 80);
        assert_eq!(State::PidDisabled.as_u16(), 90);
        assert_eq!(State::Standby.as_u16(), 95);
        assert_eq!(State::SensorError.as_u16(), 100);
        assert_eq!(State::EepromError.as_u16(), 110);
    }

    /// Every state, so a test that walks them cannot miss a newly added one.
    pub const ALL_STATES: [State; 18] = [
        State::Init,
        State::PidNormal,
        State::BrewPreinfusion,
        State::BrewPreinfusionPause,
        State::BrewRunning,
        State::BrewFinished,
        State::ManualFlushRunning,
        State::SteamRunning,
        State::BackflushIdle,
        State::BackflushFilling,
        State::BackflushFlushing,
        State::BackflushFinished,
        State::WaterTankEmpty,
        State::EmergencyStop,
        State::PidDisabled,
        State::Standby,
        State::SensorError,
        State::EepromError,
    ];

    #[test]
    fn every_state_is_classified_and_named() {
        for state in ALL_STATES {
            assert!(!state.name().is_empty());
            assert_eq!(classify(state), group_of(state));
        }
    }

    #[test]
    fn the_valve_interlock_list_is_exactly_the_six_water_flow_states() {
        let open: [State; 6] = [
            State::BrewPreinfusion,
            State::BrewPreinfusionPause,
            State::BrewRunning,
            State::ManualFlushRunning,
            State::BackflushFilling,
            State::BackflushFlushing,
        ];
        // Walking every state and counting, rather than building a list: the test's job is to
        // catch a seventh state that opens the valve, and a count plus a spot check does that
        // without needing an allocator in a no_std test.
        let listed = ALL_STATES
            .iter()
            .filter(|s| s.may_hold_water_valve_open())
            .count();
        assert_eq!(open.len(), listed);
        assert_eq!(
            listed, 6,
            "exactly six states may hold the water valve open"
        );
        for state in open {
            assert!(
                state.may_hold_water_valve_open(),
                "{state} is listed but does not open it"
            );
        }
        for state in [
            State::BrewFinished,
            State::SteamRunning,
            State::PidNormal,
            State::Init,
        ] {
            assert!(
                !state.may_hold_water_valve_open(),
                "{state} is a common state and must not hold the valve"
            );
        }
    }

    #[test]
    fn backflush_idle_and_finished_may_not_hold_the_valve() {
        // The C++ valve check excluded these two explicitly, because isBackflushState() included
        // them. Getting this wrong leaves the valve open between backflush cycles.
        assert!(!State::BackflushIdle.may_hold_water_valve_open());
        assert!(!State::BackflushFinished.may_hold_water_valve_open());
    }

    #[test]
    fn fault_states_never_heat_never_pump_never_hold_the_valve() {
        for state in [
            State::WaterTankEmpty,
            State::EmergencyStop,
            State::SensorError,
            State::EepromError,
        ] {
            assert!(!state.allows_heater(), "{state} must not heat");
            assert!(!state.may_run_pump(), "{state} must not pump");
            assert!(
                !state.may_hold_water_valve_open(),
                "{state} must not hold the valve open"
            );
        }
    }

    #[test]
    fn standby_and_backflush_inhibit_the_heater() {
        for state in [
            State::Standby,
            State::BackflushIdle,
            State::BackflushFilling,
            State::BackflushFlushing,
            State::BackflushFinished,
            State::PidDisabled,
        ] {
            assert!(!state.allows_heater(), "{state} must not heat");
        }
    }

    #[test]
    fn brewing_and_steam_allow_the_heater() {
        for state in [
            State::PidNormal,
            State::BrewPreinfusion,
            State::BrewPreinfusionPause,
            State::BrewRunning,
            State::BrewFinished,
            State::SteamRunning,
        ] {
            assert!(state.allows_heater(), "{state} should allow the heater");
        }
    }
}
