//! The backflush mode and cycle logic, ported verbatim.
//!
//! `include/clevercoffee/backflush/BackflushModeLogic.h` is already a pure,
//! `constexpr`, dependency-free header — two functions and two enums. It is
//! ported as-is, in the same shape, with the same names. There is nothing to
//! improve here and nothing to decide, which is why this is a 60-line module
//! and not part of the reducer: the state *transitions* are in
//! [`crate::states`], and only the two decisions below were ever out of them.
//!
//! `test/test_backflush_mode` is the oracle, and it is 10 cases.

/// What applying a backflush-mode request does.
///
/// `BackflushModeLogic.h:10-15`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeChangeEffect {
    /// Nothing changed — already in the requested mode.
    None,
    /// Enter backflush mode.
    Enable,
    /// Leave backflush mode.
    Disable,
    /// Refused: `backflush.cycles` is not positive.
    RejectedInvalidCycles,
}

/// The three inputs to [`resolve_mode_change`].
///
/// `BackflushModeLogic.h:17-22`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeChangeInput {
    /// `backflushOn_` before the change.
    pub was_active: bool,
    /// What the caller asked for.
    pub request_active: bool,
    /// `config.backflush.cycles`.
    pub configured_cycles: i32,
}

/// Whether applying a backflush-mode request should change state or flags.
///
/// `BackflushModeLogic.h:26-34`, transcribed:
///
/// ```cpp
/// [[nodiscard]] constexpr ModeChangeEffect resolveModeChange(const ModeChangeInput& input) noexcept {
///     if (input.requestActive == input.wasActive) {
///         return ModeChangeEffect::None;
///     }
///     if (input.requestActive && input.configuredCycles <= 0) {
///         return ModeChangeEffect::RejectedInvalidCycles;
///     }
///     return input.requestActive ? ModeChangeEffect::Enable : ModeChangeEffect::Disable;
/// }
/// ```
///
/// # The `<= 0` on `cycles`
///
/// `backflush.cycles` is range-checked to `2 ..= 20` by the C++'s `Config`
/// (`defaults.h:104`, `Config.h`), so the rejection arm is unreachable in the
/// running firmware. It is kept because it is a fail-closed rule for a value
/// that has been through a JSON import, and because `test_backflush_mode` pins
/// it. Note it only guards the *enable* direction: a disable is always allowed,
/// so a corrupt `cycles` can never trap the machine in backflush mode.
#[must_use]
pub const fn resolve_mode_change(input: ModeChangeInput) -> ModeChangeEffect {
    if input.request_active == input.was_active {
        return ModeChangeEffect::None;
    }
    if input.request_active && input.configured_cycles <= 0 {
        return ModeChangeEffect::RejectedInvalidCycles;
    }
    if input.request_active {
        ModeChangeEffect::Enable
    } else {
        ModeChangeEffect::Disable
    }
}

/// What to do once a flush phase's timer has expired.
///
/// `BackflushModeLogic.h:36-40`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleAdvanceEffect {
    /// Go back to `BACKFLUSH_FILLING` and increment the counter.
    StartNextCycle,
    /// Go to `BACKFLUSH_FINISHED`.
    CompleteAllCycles,
}

/// After a flush phase completes, whether another cycle should run.
///
/// `BackflushModeLogic.h:44-50`, transcribed:
///
/// ```cpp
/// [[nodiscard]] constexpr CycleAdvanceEffect resolveCycleAdvance(int currentCycle, int configuredCycles) noexcept {
///     if (currentCycle < configuredCycles) {
///         return CycleAdvanceEffect::StartNextCycle;
///     }
///     return CycleAdvanceEffect::CompleteAllCycles;
/// }
/// ```
///
/// # Why `<` and not `<=`
///
/// `currBackflushCycles_` starts at 1 (`MachineStateContext.h:788`) and
/// `applyBackflushMode`'s `Enable` arm re-arms it to 1
/// (`MachineStateContext.cpp:368`). With `backflush.cycles = 5` the machine
/// therefore runs cycles 1, 2, 3, 4, 5 and reaches `BACKFLUSH_FINISHED` after
/// the fifth flush. Getting this wrong by one silently runs six cycles or four.
#[must_use]
pub const fn resolve_cycle_advance(
    current_cycle: i32,
    configured_cycles: i32,
) -> CycleAdvanceEffect {
    if current_cycle < configured_cycles {
        CycleAdvanceEffect::StartNextCycle
    } else {
        CycleAdvanceEffect::CompleteAllCycles
    }
}

/// The state of the machine's backflush bookkeeping after applying a mode
/// change.
///
/// `MachineStateContext::applyBackflushMode` (`MachineStateContext.cpp:354-381`)
/// is not one function: it resolves an effect and then mutates flags per arm.
/// Splitting the mutation out keeps this module a pure decision, which is what
/// the C++ header was for, and leaves the mutation in the reducer.
// Four of the five fields are booleans because they are the C++'s four
// `requestBackflush*_` flags plus `backflushOn_`, and the point of this struct is
// that they are set *together* by one decision. Splitting them into a state
// machine would model a transition that cannot happen: `on` and `enter_requested`
// are not independent, and nothing here ever wants them to be.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeChangeOutcome {
    /// The effect that was applied.
    pub effect: ModeChangeEffect,
    /// The new `backflushOn_`. Unchanged on `None` and on rejection.
    pub on: bool,
    /// The new `currBackflushCycles_`. Re-armed to 1 on `Enable`.
    pub cycle: i32,
    /// The new `requestEnterBackflush_`.
    pub enter_requested: bool,
    /// The new `requestBackflushCycleStart_`.
    pub cycle_start_requested: bool,
    /// The new `requestBackflushStop_`.
    pub stop_requested: bool,
}

/// Apply a backflush-mode request to the current flags.
///
/// The `match` over [`ModeChangeEffect`] in `MachineStateContext.cpp:360-379`,
/// transcribed. The C++ returns `false` for `RejectedInvalidCycles` and logs a
/// warning; the effect carries that so the caller logs it.
///
/// `current` is the machine's present flags. The `None` and
/// `RejectedInvalidCycles` arms **echo it unchanged**, which is the C++'s
/// behaviour and the reason the parameter exists: `applyBackflushMode`'s `None`
/// arm is `return true;` having written nothing
/// (`MachineStateContext.cpp:361-362`), and the rejection arm logs and returns
/// `false`, also having written nothing (`:364-365`). A caller that assigned
/// `false` for those arms — which is the obvious transcription — would silently
/// clear a pending cycle-start request every time a redundant "enter backflush
/// mode" arrived, and `test_backflush_states`'s
/// `DisableClearsCycleStartRequestFlag` would still pass while the *enable* path
/// was quietly wrong.
#[must_use]
pub const fn apply_backflush_mode(
    was_on: bool,
    was_cycle: i32,
    request_active: bool,
    configured_cycles: i32,
    current: &crate::machine::Requests,
) -> ModeChangeOutcome {
    let effect = resolve_mode_change(ModeChangeInput {
        was_active: was_on,
        request_active,
        configured_cycles,
    });

    match effect {
        ModeChangeEffect::None | ModeChangeEffect::RejectedInvalidCycles => ModeChangeOutcome {
            effect,
            on: was_on,
            cycle: was_cycle,
            enter_requested: current.backflush_enter,
            cycle_start_requested: current.backflush_cycle_start,
            stop_requested: current.backflush_stop,
        },
        ModeChangeEffect::Enable => ModeChangeOutcome {
            effect,
            on: true,
            cycle: 1,
            enter_requested: true,
            cycle_start_requested: false,
            stop_requested: false,
        },
        // `MachineStateContext.cpp:373-377`: disabling clears all three
        // backflush request flags. This is the `DisableClearsCycleStartRequestFlag`
        // case in `test_backflush_states`.
        ModeChangeEffect::Disable => ModeChangeOutcome {
            effect,
            on: false,
            cycle: was_cycle,
            enter_requested: false,
            cycle_start_requested: false,
            stop_requested: false,
        },
    }
}
