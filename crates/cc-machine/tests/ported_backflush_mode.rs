//! Port of `test/test_backflush_mode` — 9 C++ cases, 10 Rust cases (9 one-to-one
//! plus 1 for `applyBackflushMode`'s flag mutation, which the C++ suite does not
//! test).
//!
//! `include/clevercoffee/backflush/BackflushModeLogic.h` is already two pure
//! `constexpr` functions, so this is a straight transcription with no decisions
//! in it. Every case name and every argument triple is preserved.
//!
//! ## Case mapping
//!
//! | C++ case (`test_backflush_mode/test_main.cpp`) | Rust test |
//! | --- | --- |
//! | `NoChangeWhenAlreadyActive` (:17) | [`no_change_when_already_active`] |
//! | `NoChangeWhenAlreadyInactive` (:21) | [`no_change_when_already_inactive`] |
//! | `EnableWhenInactiveAndCyclesConfigured` (:25) | [`enable_when_inactive_and_cycles_configured`] |
//! | `RejectEnableWhenCyclesZero` (:29) | [`reject_enable_when_cycles_zero`] |
//! | `RejectEnableWhenCyclesNegative` (:33) | [`reject_enable_when_cycles_negative`] |
//! | `DisableWhenActive` (:37) | [`disable_when_active`] |
//! | `StartsNextCycleWhenBelowConfigured` (:41) | [`starts_next_cycle_when_below_configured`] |
//! | `CompletesWhenConfiguredCyclesReached` (:46) | [`completes_when_configured_cycles_reached`] |
//! | `CompletesWhenCurrentExceedsConfigured` (:50) | [`completes_when_current_exceeds_configured`] |
//! | — | [`the_flag_outcome_matches_the_effect`] (the `applyBackflushMode`
//! mutation, `MachineStateContext.cpp:354-381`) |

use cc_machine::backflush::{
    apply_backflush_mode, resolve_cycle_advance, resolve_mode_change, CycleAdvanceEffect,
    ModeChangeEffect, ModeChangeInput,
};
use cc_machine::machine::Requests;

fn mode(was_active: bool, request_active: bool, configured_cycles: i32) -> ModeChangeEffect {
    resolve_mode_change(ModeChangeInput {
        was_active,
        request_active,
        configured_cycles,
    })
}

#[test]
fn no_change_when_already_active() {
    assert_eq!(mode(true, true, 5), ModeChangeEffect::None);
}

#[test]
fn no_change_when_already_inactive() {
    assert_eq!(mode(false, false, 5), ModeChangeEffect::None);
}

#[test]
fn enable_when_inactive_and_cycles_configured() {
    assert_eq!(mode(false, true, 5), ModeChangeEffect::Enable);
}

#[test]
fn reject_enable_when_cycles_zero() {
    assert_eq!(
        mode(false, true, 0),
        ModeChangeEffect::RejectedInvalidCycles
    );
}

#[test]
fn reject_enable_when_cycles_negative() {
    assert_eq!(
        mode(false, true, -1),
        ModeChangeEffect::RejectedInvalidCycles
    );
}

#[test]
fn disable_when_active() {
    assert_eq!(mode(true, false, 5), ModeChangeEffect::Disable);
}

#[test]
fn starts_next_cycle_when_below_configured() {
    assert_eq!(
        resolve_cycle_advance(1, 5),
        CycleAdvanceEffect::StartNextCycle
    );
    assert_eq!(
        resolve_cycle_advance(4, 5),
        CycleAdvanceEffect::StartNextCycle
    );
}

#[test]
fn completes_when_configured_cycles_reached() {
    assert_eq!(
        resolve_cycle_advance(5, 5),
        CycleAdvanceEffect::CompleteAllCycles
    );
}

#[test]
fn completes_when_current_exceeds_configured() {
    assert_eq!(
        resolve_cycle_advance(6, 5),
        CycleAdvanceEffect::CompleteAllCycles
    );
}

/// The `if constexpr`-free companion the C++ suite does not test: what
/// `MachineStateContext::applyBackflushMode` (`MachineStateContext.cpp:354-381`)
/// does to the flags in each arm.
#[test]
fn the_flag_outcome_matches_the_effect() {
    // `None` writes nothing: the flags come back exactly as they went in.
    let busy = Requests {
        backflush_enter: false,
        backflush_cycle_start: true,
        backflush_stop: true,
        ..Requests::CLEAR
    };
    let out = apply_backflush_mode(true, 3, true, 5, &busy);
    assert_eq!(out.effect, ModeChangeEffect::None);
    assert!(out.on);
    assert_eq!(out.cycle, 3);
    assert!(!out.enter_requested);
    assert!(
        out.cycle_start_requested,
        "a redundant enter must not eat it"
    );
    assert!(out.stop_requested);

    // `Enable` sets the mode, re-arms the counter to 1 and requests the
    // transition; it does **not** start a cycle.
    let out = apply_backflush_mode(false, 3, true, 5, &Requests::CLEAR);
    assert_eq!(out.effect, ModeChangeEffect::Enable);
    assert!(out.on);
    assert_eq!(out.cycle, 1);
    assert!(out.enter_requested);
    assert!(!out.cycle_start_requested);
    assert!(!out.stop_requested);

    // `Disable` turns the mode off and clears all three request flags — the
    // `DisableClearsCycleStartRequestFlag` case in `test_backflush_states`.
    let out = apply_backflush_mode(true, 4, false, 5, &busy);
    assert_eq!(out.effect, ModeChangeEffect::Disable);
    assert!(!out.on);
    assert!(!out.enter_requested);
    assert!(!out.cycle_start_requested);
    assert!(!out.stop_requested);

    // A rejection changes nothing at all, so a corrupt `cycles` can never trap
    // the machine in backflush mode.
    let out = apply_backflush_mode(false, 2, true, 0, &busy);
    assert_eq!(out.effect, ModeChangeEffect::RejectedInvalidCycles);
    assert!(!out.on);
    assert_eq!(out.cycle, 2);
    assert!(
        out.cycle_start_requested,
        "a rejection writes nothing either"
    );
    assert!(out.stop_requested);
}
