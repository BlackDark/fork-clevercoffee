//! The 18 machine states and their category predicates.
//!
//! Discriminants are the C++ `MachineStateId` values from
//! `include/clevercoffee/state/MachineStateIds.h:11-39`, byte for byte. They
//! are part of the machine's external contract: `/api/parameters` and the MQTT
//! payloads report them as integers, and `intentional-diffs.md` / the parity
//! harness diff them. Renumbering a variant is a breaking change.

/// Every state the machine can be in.
///
/// Exactly 18 variants, matching `MachineStateIds.h`. Adding a variant is
/// deliberate and forces a review of every `match` over `MachineState` — which
/// is the point (04 §4, "adding a water-flow state is a compile error until the
/// whitelist is updated").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum MachineState {
    /// Bring-up, before the first transition.
    Init = 0,
    /// PID enabled, idle, heater holding the brew setpoint.
    PidNormal = 20,
    /// Pre-infusion: pump and valve open, no heat.
    BrewPreinfusion = 31,
    /// Pre-infusion pause: valve closed, pump off, heat on.
    BrewPreinfusionPause = 32,
    /// Main brew: pump and valve open, PID delayed for `brew.pid_delay`.
    BrewRunning = 33,
    /// Brew complete, post-brew timer running.
    BrewFinished = 34,
    /// Manual flush: pump and valve open.
    ManualFlushRunning = 36,
    /// Steam: steam valve open, steam setpoint.
    SteamRunning = 51,
    /// Backflush idle, waiting for a request.
    BackflushIdle = 60,
    /// Backflush filling: pump and valve open.
    BackflushFilling = 61,
    /// Backflush flushing: valve open, pump off.
    BackflushFlushing = 62,
    /// Backflush complete.
    BackflushFinished = 63,
    /// Water tank reported empty; pump blocked.
    WaterTankEmpty = 70,
    /// Emergency stop latched; every energising call refused.
    EmergencyStop = 80,
    /// PID deliberately off (power switch, or a global guard fired).
    PidDisabled = 90,
    /// Standby: heater off, awaiting a wake request.
    Standby = 95,
    /// The temperature sensor is not producing valid readings.
    SensorError = 100,
    /// Non-volatile storage could not be read or written.
    EepromError = 110,
}

/// Every state, in discriminant order.
///
/// Used by the exhaustive tables in `cc-safety` and `cc-machine` so a new
/// variant cannot be added without those tables noticing.
pub const ALL: [MachineState; 18] = [
    MachineState::Init,
    MachineState::PidNormal,
    MachineState::BrewPreinfusion,
    MachineState::BrewPreinfusionPause,
    MachineState::BrewRunning,
    MachineState::BrewFinished,
    MachineState::ManualFlushRunning,
    MachineState::SteamRunning,
    MachineState::BackflushIdle,
    MachineState::BackflushFilling,
    MachineState::BackflushFlushing,
    MachineState::BackflushFinished,
    MachineState::WaterTankEmpty,
    MachineState::EmergencyStop,
    MachineState::PidDisabled,
    MachineState::Standby,
    MachineState::SensorError,
    MachineState::EepromError,
];

impl MachineState {
    /// The C++ enumerator name, e.g. `"BREW_PREINFUSION"`.
    ///
    /// Used in logs and in the parity harness, where the C++ log text is the
    /// comparison target. Deliberately the `SCREAMING_SNAKE` spelling rather
    /// than a `Debug`-derived one: parity is measured against the C++ strings.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Init => "INIT",
            Self::PidNormal => "PID_NORMAL",
            Self::BrewPreinfusion => "BREW_PREINFUSION",
            Self::BrewPreinfusionPause => "BREW_PREINFUSION_PAUSE",
            Self::BrewRunning => "BREW_RUNNING",
            Self::BrewFinished => "BREW_FINISHED",
            Self::ManualFlushRunning => "MANUAL_FLUSH_RUNNING",
            Self::SteamRunning => "STEAM_RUNNING",
            Self::BackflushIdle => "BACKFLUSH_IDLE",
            Self::BackflushFilling => "BACKFLUSH_FILLING",
            Self::BackflushFlushing => "BACKFLUSH_FLUSHING",
            Self::BackflushFinished => "BACKFLUSH_FINISHED",
            Self::WaterTankEmpty => "WATER_TANK_EMPTY",
            Self::EmergencyStop => "EMERGENCY_STOP",
            Self::PidDisabled => "PID_DISABLED",
            Self::Standby => "STANDBY",
            Self::SensorError => "SENSOR_ERROR",
            Self::EepromError => "EEPROM_ERROR",
        }
    }

    /// The numeric `MachineStateId`.
    #[must_use]
    pub const fn id(self) -> u16 {
        self as u16
    }

    /// Recover a state from its `MachineStateId`.
    ///
    /// Returns `None` for an id the firmware does not know. The C++ restarts the
    /// device in that case (`src/state/StateFactory.cpp:65-69`); the Rust port
    /// must not, so the caller decides.
    #[must_use]
    pub const fn from_id(id: u16) -> Option<Self> {
        match id {
            0 => Some(Self::Init),
            20 => Some(Self::PidNormal),
            31 => Some(Self::BrewPreinfusion),
            32 => Some(Self::BrewPreinfusionPause),
            33 => Some(Self::BrewRunning),
            34 => Some(Self::BrewFinished),
            36 => Some(Self::ManualFlushRunning),
            51 => Some(Self::SteamRunning),
            60 => Some(Self::BackflushIdle),
            61 => Some(Self::BackflushFilling),
            62 => Some(Self::BackflushFlushing),
            63 => Some(Self::BackflushFinished),
            70 => Some(Self::WaterTankEmpty),
            80 => Some(Self::EmergencyStop),
            90 => Some(Self::PidDisabled),
            95 => Some(Self::Standby),
            100 => Some(Self::SensorError),
            110 => Some(Self::EepromError),
            _ => None,
        }
    }

    /// Whether this is one of the four brew states.
    ///
    /// Port of `isBrewState` (`MachineStateIds.h:42-44`), which is a range test
    /// over the enum: `>= BREW_PREINFUSION && <= BREW_FINISHED`. The range is
    /// spelled out rather than compared numerically so that reordering the
    /// variants cannot silently change the answer.
    #[must_use]
    pub const fn is_brew_state(self) -> bool {
        matches!(
            self,
            Self::BrewPreinfusion
                | Self::BrewPreinfusionPause
                | Self::BrewRunning
                | Self::BrewFinished
        )
    }

    /// Whether this is the steam state.
    ///
    /// Port of `isSteamState` (`MachineStateIds.h:46-48`).
    #[must_use]
    pub const fn is_steam_state(self) -> bool {
        matches!(self, Self::SteamRunning)
    }

    /// Whether this is one of the four backflush states.
    ///
    /// Port of `isBackflushState` (`MachineStateIds.h:50-52`).
    #[must_use]
    pub const fn is_backflush_state(self) -> bool {
        matches!(
            self,
            Self::BackflushIdle
                | Self::BackflushFilling
                | Self::BackflushFlushing
                | Self::BackflushFinished
        )
    }

    /// Whether this is the manual-flush state.
    ///
    /// Port of `isManualFlushState` (`MachineStateIds.h:54-56`).
    #[must_use]
    pub const fn is_manual_flush_state(self) -> bool {
        matches!(self, Self::ManualFlushRunning)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ports `test/test_state_classification/test_main.cpp`, which is the C++
    /// oracle for these four predicates.
    #[test]
    fn brew_preinfusion_is_brew_state() {
        assert!(MachineState::BrewPreinfusion.is_brew_state());
    }

    #[test]
    fn brew_preinfusion_pause_is_brew_state() {
        assert!(MachineState::BrewPreinfusionPause.is_brew_state());
    }

    #[test]
    fn brew_running_is_brew_state() {
        assert!(MachineState::BrewRunning.is_brew_state());
    }

    #[test]
    fn brew_finished_is_brew_state() {
        assert!(MachineState::BrewFinished.is_brew_state());
    }

    #[test]
    fn non_brew_state_is_not_brew_state() {
        for s in [
            MachineState::Init,
            MachineState::PidNormal,
            MachineState::SteamRunning,
            MachineState::BackflushIdle,
            MachineState::WaterTankEmpty,
            MachineState::EmergencyStop,
        ] {
            assert!(!s.is_brew_state(), "{s:?} must not be a brew state");
        }
    }

    #[test]
    fn steam_running_is_steam_state() {
        assert!(MachineState::SteamRunning.is_steam_state());
    }

    #[test]
    fn non_steam_state_is_not_steam_state() {
        for s in [
            MachineState::Init,
            MachineState::BrewRunning,
            MachineState::BackflushIdle,
            MachineState::WaterTankEmpty,
            MachineState::EmergencyStop,
        ] {
            assert!(!s.is_steam_state(), "{s:?} must not be a steam state");
        }
    }

    #[test]
    fn all_four_backflush_states_are_backflush_states() {
        for s in [
            MachineState::BackflushIdle,
            MachineState::BackflushFilling,
            MachineState::BackflushFlushing,
            MachineState::BackflushFinished,
        ] {
            assert!(s.is_backflush_state(), "{s:?} must be a backflush state");
        }
    }

    #[test]
    fn non_backflush_state_is_not_backflush_state() {
        for s in [
            MachineState::Init,
            MachineState::BrewRunning,
            MachineState::SteamRunning,
            MachineState::WaterTankEmpty,
            MachineState::EmergencyStop,
        ] {
            assert!(
                !s.is_backflush_state(),
                "{s:?} must not be a backflush state"
            );
        }
    }

    #[test]
    fn manual_flush_running_is_manual_flush_state() {
        assert!(MachineState::ManualFlushRunning.is_manual_flush_state());
    }

    #[test]
    fn non_manual_flush_state_is_not_manual_flush_state() {
        for s in [
            MachineState::Init,
            MachineState::BrewRunning,
            MachineState::SteamRunning,
            MachineState::BackflushIdle,
            MachineState::WaterTankEmpty,
            MachineState::EmergencyStop,
        ] {
            assert!(
                !s.is_manual_flush_state(),
                "{s:?} must not be a manual flush state"
            );
        }
    }

    #[test]
    fn brew_states_are_not_steam_states() {
        assert!(!MachineState::BrewRunning.is_steam_state());
        assert!(!MachineState::BrewFinished.is_steam_state());
    }

    #[test]
    fn steam_state_is_not_a_brew_state() {
        assert!(!MachineState::SteamRunning.is_brew_state());
    }

    #[test]
    fn backflush_states_are_not_brew_states() {
        for s in [
            MachineState::BackflushIdle,
            MachineState::BackflushFilling,
            MachineState::BackflushFlushing,
            MachineState::BackflushFinished,
        ] {
            assert!(!s.is_brew_state(), "{s:?} must not be a brew state");
        }
    }

    #[test]
    fn error_states_are_not_brew_states() {
        for s in [
            MachineState::WaterTankEmpty,
            MachineState::SensorError,
            MachineState::EepromError,
        ] {
            assert!(!s.is_brew_state(), "{s:?} must not be a brew state");
        }
    }

    #[test]
    fn emergency_stop_is_in_no_category() {
        let s = MachineState::EmergencyStop;
        assert!(!s.is_brew_state());
        assert!(!s.is_steam_state());
        assert!(!s.is_backflush_state());
        assert!(!s.is_manual_flush_state());
    }

    #[test]
    fn standby_is_in_no_category() {
        let s = MachineState::Standby;
        assert!(!s.is_brew_state());
        assert!(!s.is_steam_state());
        assert!(!s.is_backflush_state());
        assert!(!s.is_manual_flush_state());
    }

    #[test]
    fn there_are_exactly_eighteen_distinct_states() {
        assert_eq!(ALL.len(), 18);
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(a.id(), b.id(), "{a:?} and {b:?} share a discriminant");
            }
        }
    }

    #[test]
    fn discriminants_match_the_cpp_enum() {
        // MachineStateIds.h:11-39, transcribed.
        let expected: [(MachineState, u16); 18] = [
            (MachineState::Init, 0),
            (MachineState::PidNormal, 20),
            (MachineState::BrewPreinfusion, 31),
            (MachineState::BrewPreinfusionPause, 32),
            (MachineState::BrewRunning, 33),
            (MachineState::BrewFinished, 34),
            (MachineState::ManualFlushRunning, 36),
            (MachineState::SteamRunning, 51),
            (MachineState::BackflushIdle, 60),
            (MachineState::BackflushFilling, 61),
            (MachineState::BackflushFlushing, 62),
            (MachineState::BackflushFinished, 63),
            (MachineState::WaterTankEmpty, 70),
            (MachineState::EmergencyStop, 80),
            (MachineState::PidDisabled, 90),
            (MachineState::Standby, 95),
            (MachineState::SensorError, 100),
            (MachineState::EepromError, 110),
        ];
        for (state, id) in expected {
            assert_eq!(state.id(), id, "{} discriminant", state.name());
            assert_eq!(MachineState::from_id(id), Some(state));
        }
        assert_eq!(MachineState::from_id(1), None);
        assert_eq!(MachineState::from_id(999), None);
    }

    #[test]
    fn all_covers_every_variant() {
        // Round-tripping through `from_id` is only exhaustive if ALL lists
        // every discriminant, so check the two agree.
        for state in ALL {
            assert_eq!(MachineState::from_id(state.id()), Some(state));
        }
    }

    #[test]
    fn names_match_the_cpp_enumerators() {
        assert_eq!(
            MachineState::BrewPreinfusionPause.name(),
            "BREW_PREINFUSION_PAUSE"
        );
        assert_eq!(MachineState::EepromError.name(), "EEPROM_ERROR");
    }
}
