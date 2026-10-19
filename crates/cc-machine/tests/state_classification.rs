//! The four state predicates, pinned where the machine depends on them.
//!
//! `cc-domain`'s `state.rs` owns the predicates (`is_brew_state`, `is_steam_state`,
//! `is_backflush_state`, `is_manual_flush_state`). This file re-tests the classification against
//! the *machine's* use of it, so a change to the machine that quietly depends on a different
//! categorisation fails here.
//!
//! One predicate deliberately disagrees with the safety whitelist: `is_brew_state` is a range
//! test that includes `BREW_FINISHED`, and `cc_safety::water_flow_allowed` excludes it. The
//! machine depends on that difference in two places, so
//! [`water_flow_excludes_brew_finished_but_the_brew_predicate_does_not`] checks both sides.
//!
//! [`the_classification_partitions_all_eighteen_states`] asserts the partition rather than a list
//! of examples: the four categories plus the eight uncategorised states account for all
//! eighteen, with nothing in two categories and nothing in none.

use cc_domain::state::MachineState;

const NON_BREW: [MachineState; 6] = [
    MachineState::Init,
    MachineState::PidNormal,
    MachineState::SteamRunning,
    MachineState::BackflushIdle,
    MachineState::WaterTankEmpty,
    MachineState::EmergencyStop,
];

const NON_STEAM: [MachineState; 5] = [
    MachineState::Init,
    MachineState::BrewRunning,
    MachineState::BackflushIdle,
    MachineState::WaterTankEmpty,
    MachineState::EmergencyStop,
];

const NON_BACKFLUSH: [MachineState; 5] = [
    MachineState::Init,
    MachineState::BrewRunning,
    MachineState::SteamRunning,
    MachineState::WaterTankEmpty,
    MachineState::EmergencyStop,
];

const NON_MANUAL_FLUSH: [MachineState; 6] = [
    MachineState::Init,
    MachineState::BrewRunning,
    MachineState::SteamRunning,
    MachineState::BackflushIdle,
    MachineState::WaterTankEmpty,
    MachineState::EmergencyStop,
];

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
    for s in NON_BREW {
        assert!(!s.is_brew_state(), "{s:?}");
    }
}

#[test]
fn steam_running_is_steam_state() {
    assert!(MachineState::SteamRunning.is_steam_state());
}

#[test]
fn non_steam_state_is_not_steam_state() {
    for s in NON_STEAM {
        assert!(!s.is_steam_state(), "{s:?}");
    }
}

#[test]
fn backflush_idle_is_backflush_state() {
    assert!(MachineState::BackflushIdle.is_backflush_state());
}

#[test]
fn backflush_filling_is_backflush_state() {
    assert!(MachineState::BackflushFilling.is_backflush_state());
}

#[test]
fn backflush_flushing_is_backflush_state() {
    assert!(MachineState::BackflushFlushing.is_backflush_state());
}

#[test]
fn backflush_finished_is_backflush_state() {
    assert!(MachineState::BackflushFinished.is_backflush_state());
}

#[test]
fn non_backflush_state_is_not_backflush_state() {
    for s in NON_BACKFLUSH {
        assert!(!s.is_backflush_state(), "{s:?}");
    }
}

#[test]
fn manual_flush_running_is_manual_flush_state() {
    assert!(MachineState::ManualFlushRunning.is_manual_flush_state());
}

#[test]
fn non_manual_flush_state_is_not_manual_flush_state() {
    for s in NON_MANUAL_FLUSH {
        assert!(!s.is_manual_flush_state(), "{s:?}");
    }
}

#[test]
fn brew_states_are_not_steam_states() {
    assert!(!MachineState::BrewRunning.is_steam_state());
    assert!(!MachineState::BrewFinished.is_steam_state());
}

#[test]
fn steam_running_is_not_a_brew_state() {
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
        assert!(!s.is_brew_state(), "{s:?}");
    }
}

#[test]
fn error_states_are_not_brew_states() {
    for s in [
        MachineState::WaterTankEmpty,
        MachineState::SensorError,
        MachineState::EepromError,
    ] {
        assert!(!s.is_brew_state(), "{s:?}");
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

/// The partition, rather than a list of examples per category: every state is in exactly one
/// category or in none, and the two groups together account for all eighteen.
#[test]
fn the_classification_partitions_all_eighteen_states() {
    let categorised = [
        MachineState::BrewPreinfusion,
        MachineState::BrewPreinfusionPause,
        MachineState::BrewRunning,
        MachineState::BrewFinished,
        MachineState::SteamRunning,
        MachineState::BackflushIdle,
        MachineState::BackflushFilling,
        MachineState::BackflushFlushing,
        MachineState::BackflushFinished,
        MachineState::ManualFlushRunning,
    ];
    let uncategorised = [
        MachineState::Init,
        MachineState::PidNormal,
        MachineState::PidDisabled,
        MachineState::Standby,
        MachineState::WaterTankEmpty,
        MachineState::EmergencyStop,
        MachineState::SensorError,
        MachineState::EepromError,
    ];
    assert_eq!(categorised.len() + uncategorised.len(), 18);

    for s in categorised {
        let count = usize::from(s.is_brew_state())
            + usize::from(s.is_steam_state())
            + usize::from(s.is_backflush_state())
            + usize::from(s.is_manual_flush_state());
        assert_eq!(
            count, 1,
            "{s:?} must be in exactly one category, was {count}"
        );
    }
    for s in uncategorised {
        assert!(
            !s.is_brew_state()
                && !s.is_steam_state()
                && !s.is_backflush_state()
                && !s.is_manual_flush_state(),
            "{s:?} must be in no category"
        );
    }
}

/// The S5 whitelist and the brew predicate deliberately disagree about `BREW_FINISHED`.
///
/// `is_brew_state` is a range test that includes it; `cc_safety::water_flow_allowed` excludes
/// it, and so does [`cc_machine::handlers::brew_is_active`]. The difference is a decision rather
/// than an oversight -- the shot is over by the time the state is entered, so nothing needs water
/// -- and it is pinned here so that changing either side has to be deliberate.
#[test]
fn water_flow_excludes_brew_finished_but_the_brew_predicate_does_not() {
    assert!(MachineState::BrewFinished.is_brew_state());
    assert!(!cc_safety::water_flow_allowed(MachineState::BrewFinished));
    // And the machine's own "is a brew running" helper excludes it too.
    assert!(cc_machine::handlers::brew_is_active(
        MachineState::BrewRunning
    ));
    assert!(!cc_machine::handlers::brew_is_active(
        MachineState::BrewFinished
    ));
}
