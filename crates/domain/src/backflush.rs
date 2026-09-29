//! Backflush mode and cycle logic, ported from the C++ `BackflushModeLogic`.
//!
//! This is the one piece of the C++ firmware that was already a pure, unit-tested `constexpr`
//! function, so it ports directly. The value of keeping it separate is that the cycle arithmetic
//! is testable without a machine.

/// What applying the backflush mode setting should do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModeChangeEffect {
    /// Already in the requested mode. Nothing changes.
    None,
    Enable,
    Disable,
    /// Asked to enable with zero or fewer cycles configured, which would do nothing at all.
    RejectedInvalidCycles,
}

#[derive(Clone, Copy, Debug)]
pub struct ModeChangeInput {
    pub was_active: bool,
    pub request_active: bool,
    pub configured_cycles: u8,
}

pub const fn resolve_mode_change(input: ModeChangeInput) -> ModeChangeEffect {
    if input.request_active == input.was_active {
        return ModeChangeEffect::None;
    }
    if input.request_active && input.configured_cycles == 0 {
        return ModeChangeEffect::RejectedInvalidCycles;
    }
    if input.request_active {
        ModeChangeEffect::Enable
    } else {
        ModeChangeEffect::Disable
    }
}

/// What happens after a flush phase completes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CycleAdvanceEffect {
    StartNextCycle,
    CompleteAllCycles,
}

/// `current_cycle` is 0-based.
pub const fn resolve_cycle_advance(current_cycle: u8, configured_cycles: u8) -> CycleAdvanceEffect {
    // saturating_add, not `+`: a u8 at 255 wraps to 0 in a debug build and panics, and in a
    // release build it would look like "back to the first cycle" and loop the backflush forever.
    if current_cycle.saturating_add(1) < configured_cycles {
        CycleAdvanceEffect::StartNextCycle
    } else {
        CycleAdvanceEffect::CompleteAllCycles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requesting_the_mode_it_is_already_in_does_nothing() {
        assert_eq!(
            resolve_mode_change(ModeChangeInput {
                was_active: true,
                request_active: true,
                configured_cycles: 5
            }),
            ModeChangeEffect::None
        );
        assert_eq!(
            resolve_mode_change(ModeChangeInput {
                was_active: false,
                request_active: false,
                configured_cycles: 5
            }),
            ModeChangeEffect::None
        );
    }

    #[test]
    fn enabling_and_disabling_move_the_mode() {
        assert_eq!(
            resolve_mode_change(ModeChangeInput {
                was_active: false,
                request_active: true,
                configured_cycles: 5
            }),
            ModeChangeEffect::Enable
        );
        assert_eq!(
            resolve_mode_change(ModeChangeInput {
                was_active: true,
                request_active: false,
                configured_cycles: 5
            }),
            ModeChangeEffect::Disable
        );
    }

    #[test]
    fn enabling_with_zero_cycles_is_rejected() {
        assert_eq!(
            resolve_mode_change(ModeChangeInput {
                was_active: false,
                request_active: true,
                configured_cycles: 0
            }),
            ModeChangeEffect::RejectedInvalidCycles
        );
    }

    #[test]
    fn a_single_cycle_backflush_runs_once() {
        assert_eq!(
            resolve_cycle_advance(0, 1),
            CycleAdvanceEffect::CompleteAllCycles
        );
    }

    #[test]
    fn the_last_cycle_completes_and_the_others_repeat() {
        assert_eq!(
            resolve_cycle_advance(0, 3),
            CycleAdvanceEffect::StartNextCycle
        );
        assert_eq!(
            resolve_cycle_advance(1, 3),
            CycleAdvanceEffect::StartNextCycle
        );
        assert_eq!(
            resolve_cycle_advance(2, 3),
            CycleAdvanceEffect::CompleteAllCycles
        );
    }

    #[test]
    fn a_cycle_index_past_the_end_completes_rather_than_overflowing() {
        // A u8 at 255 plus one wraps to 0, which would look like "start the first cycle again"
        // and loop forever, or panic outright with overflow checks on. The test pins both ends.
        assert_eq!(
            resolve_cycle_advance(253, 255),
            CycleAdvanceEffect::StartNextCycle
        );
        assert_eq!(
            resolve_cycle_advance(254, 255),
            CycleAdvanceEffect::CompleteAllCycles
        );
        assert_eq!(
            resolve_cycle_advance(255, 255),
            CycleAdvanceEffect::CompleteAllCycles
        );
    }
}
