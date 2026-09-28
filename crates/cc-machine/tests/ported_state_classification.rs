//! Port of `test/test_state_classification` — 24 C++ cases, 24 Rust cases
//! (20 one-to-one plus 4 that the Rust file folds into one partition check and
//! one that pins the S5 whitelist's deliberate disagreement with the predicate).
//!
//! The four predicates (`isBrewState`, `isSteamState`, `isBackflushState`,
//! `isManualFlushState`) are ported in `cc-domain`'s `state.rs` by R2-04, which
//! owns them. This file re-states the C++ suite's twenty cases against the
//! *state machine's* use of them, so the classification is pinned where the
//! machine depends on it rather than only where it is defined.
//!
//! The distinction matters for one of them: `is_brew_state` is a **range test**
//! in the C++ (`MachineStateIds.h:42-44`, `>= BREW_PREINFUSION && <=
//! BREW_FINISHED`), and `BrewFinished`'s own `onExit` and the S5 whitelist both
//! treat it specially. A classification test that only checks the predicate would
//! not notice the whitelist disagreeing with it, so
//! [`water_flow_excludes_brew_finished_but_the_brew_predicate_does_not`] is here
//! too.
//!
//! ## Case mapping
//!
//! | C++ case (`test_state_classification/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `BrewPreinfusionIsBrewState` (:30) | `brew_preinfusion_is_brew_state` |
//! | `BrewPreinfusionPauseIsBrewState` (:34) | `brew_preinfusion_pause_is_brew_state` |
//! | `BrewRunningIsBrewState` (:38) | `brew_running_is_brew_state` |
//! | `BrewFinishedIsBrewState` (:42) | `brew_finished_is_brew_state` |
//! | `NonBrewStateIsNotBrewState` (:46) | `non_brew_state_is_not_brew_state` |
//! | `SteamRunningIsSteamState` (:66) | `steam_running_is_steam_state` |
//! | `NonSteamStateIsNotSteamState` (:70) | `non_steam_state_is_not_steam_state` |
//! | `BackflushIdleIsBackflushState` (:89) | `backflush_idle_is_backflush_state` |
//! | `BackflushFillingIsBackflushState` (:93) | `backflush_filling_is_backflush_state` |
//! | `BackflushFlushingIsBackflushState` (:97) | `backflush_flushing_is_backflush_state` |
//! | `BackflushFinishedIsBackflushState` (:101) | `backflush_finished_is_backflush_state` |
//! | `NonBackflushStateIsNotBackflushState` (:105) | `non_backflush_state_is_not_backflush_state` |
//! | `ManualFlushRunningIsManualFlushState` (:124) | `manual_flush_running_is_manual_flush_state` |
//! | `NonManualFlushStateIsNotManualFlushState` (:128) | `non_manual_flush_state_is_not_manual_flush_state` |
//! | `BrewStatesAreNotSteamStates` (:148) | `brew_states_are_not_steam_states` |
//! | `SteamStatesAreNotBrewStates` (:153) | `steam_running_is_not_a_brew_state` |
//! | `BackflushStatesAreNotBrewStates` (:157) | `backflush_states_are_not_brew_states` |
//! | `ErrorStatesAreNotBrewStates` (:175) | `error_states_are_not_brew_states` |
//! | `EmergencyStopIsNotAnyCategory` (:181) | `emergency_stop_is_in_no_category` |
//! | `StandbyStateIsNotAnyCategory` (:188) | `standby_is_in_no_category` |
//!
//! `AllBrewStatesCovered` (:206), `AllSteamStatesCovered` (:214),
//! `AllBackflushStatesCovered` (:219) and `AllManualFlushStatesCovered` (:227)
//! repeat the four assertions above verbatim, so they add no coverage; the
//! [`the_classification_partitions_all_eighteen_states`] case replaces all four
//! and adds the check the C++ does not have — that *every* state is in exactly
//! the right category, as a partition rather than as a list of examples.

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

/// The coverage check the C++ suite does not have.
///
/// The C++ asserts a list of examples per category. This asserts the
/// *partition*: the four categories, plus the four uncategorised states, account
/// for all eighteen with no overlap and nothing missing.
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

/// The S5 whitelist and the brew predicate deliberately disagree about
/// `BREW_FINISHED`.
///
/// `isBrewState` is a range test that includes it
/// (`MachineStateIds.h:42-44`); `valveSafetyShutdownCheck` excludes it
/// (`BrewHandler.h:111-112`, and `cc_safety::water_flow_allowed`). Two places
/// in the C++ depend on the difference — `isBrewActive` and the S5 expression
/// both spell out `&& state != BREW_FINISHED` — so the difference is a decision,
/// not an oversight, and it is pinned here.
#[test]
fn water_flow_excludes_brew_finished_but_the_brew_predicate_does_not() {
    assert!(MachineState::BrewFinished.is_brew_state());
    assert!(!cc_safety::water_flow_allowed(MachineState::BrewFinished));
    // And the machine's own "is a brew running" helper excludes it, matching
    // `BrewHandler::isBrewActive` (`BrewHandler.h:98-103`).
    assert!(cc_machine::handlers::brew_is_active(
        MachineState::BrewRunning
    ));
    assert!(!cc_machine::handlers::brew_is_active(
        MachineState::BrewFinished
    ));
}
