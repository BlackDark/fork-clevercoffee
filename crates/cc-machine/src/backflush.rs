//! Pure backflush decisions; state transitions remain in [`crate::states`].

/// What applying a backflush-mode request does.
///
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeChangeInput {
    /// Whether backflush mode was on before the change.
    pub was_active: bool,
    /// What the caller asked for.
    pub request_active: bool,
    /// `config.backflush.cycles`.
    pub configured_cycles: i32,
}

/// Whether applying a backflush-mode request should change state or flags.
///
/// Four outcomes, in order: nothing, when the request already matches the current mode; a
/// rejection, when enabling with a non-positive cycle count; enable; disable.
///
/// # The `<= 0` on `cycles`
///
/// `backflush.cycles` is range-checked to `2 ..= 20`, so the rejection arm is unreachable
/// from a well-formed configuration. It is kept because `backflush.cycles` arrives through
/// a JSON import and this rule is fail-closed: a zero or negative count cannot turn headless
/// cycling on. It guards the *enable* direction only, so a corrupt `cycles` can never trap
/// the machine in backflush mode -- a disable is always allowed.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleAdvanceEffect {
    /// Go back to `BACKFLUSH_FILLING` and increment the counter.
    StartNextCycle,
    /// Go to `BACKFLUSH_FINISHED`.
    CompleteAllCycles,
}

/// After a flush phase completes, whether another cycle should run.
///
/// # Why `<` and not `<=`
///
/// The cycle counter starts at 1, and [`apply_backflush_mode`]'s `Enable` arm re-arms it
/// to 1. With `backflush.cycles = 5` the machine therefore runs cycles 1, 2, 3, 4 and 5,
/// and reaches `BACKFLUSH_FINISHED` after the fifth flush. Getting this wrong by one
/// silently runs six cycles or four.
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

/// The machine's backflush bookkeeping after a mode change: the effect, and the flags it
/// leaves behind.
///
/// Resolving the effect and mutating the flags are one decision, which is why they are one
/// value: a caller applies the whole outcome or none of it, and the reducer's only job is
/// to write these fields into [`crate::machine::Requests`].
// Four of the six fields are booleans, and the point of the struct is that one decision sets
// them *together*. Splitting them into a state machine would model a transition that cannot
// happen: `on` and `enter_requested` are not independent, and nothing wants them to be.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one decision sets the flags together; they are not a state machine"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeChangeOutcome {
    /// The effect that was applied.
    pub effect: ModeChangeEffect,
    /// Backflush mode after the change. Unchanged on `None` and on rejection.
    pub on: bool,
    /// The cycle counter after the change. Re-armed to 1 on `Enable`.
    pub cycle: i32,
    /// Whether entering backflush mode is requested.
    pub enter_requested: bool,
    /// Whether the next cycle is requested.
    pub cycle_start_requested: bool,
    /// Whether leaving backflush mode is requested.
    pub stop_requested: bool,
}

/// Apply a backflush-mode request to the current flags.
///
/// A rejection is reported through the returned [`ModeChangeEffect`] so the caller can log
/// it; this function does no logging itself.
///
/// `current` is the machine's present flags, and the `None` and `RejectedInvalidCycles` arms
/// **echo it unchanged**, which is why the parameter exists. A caller that assigned `false`
/// for those two arms -- the obvious simplification -- would silently clear a pending
/// cycle-start request every time a redundant "enter backflush mode" arrived, and the
/// redundant request is the common case because the switch is level-triggered.
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
        // Disabling clears all three backflush request flags, including a cycle start that
        // was already pending. `DisableClearsCycleStartRequestFlag` is the case that pins
        // it.
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
