//! Shot counting and the backflush reminder, ported from
//! `maintenance/BackflushReminderLogic.h`.
//!
//! Small, pure, and assigned to R2-08 by the coverage map in 06
//! (`test_maintenance_coordinator` → R2-08). Only the parts the state machine
//! touches are ported: the qualification test that `BrewFinishedState::onEntryImpl`
//! calls (`BrewStates.cpp:310`), the reset that `BackflushFinishedState::onEntryImpl`
//! calls (`BackflushStates.cpp:144`), and the reminder predicate. The
//! coordinator's NVS persistence has no analogue here —
//! [`Machine::shots_since_backflush`](crate::machine::Machine::shots_since_backflush)
//! is the value, and persisting it is the store's job at R3-08.

/// A brew is long enough to count on time alone. `defaults.h:46`:
/// `BACKFLUSH_REMINDER_MIN_BREW_TIME_MS = 5000.0`.
pub const MIN_BREW_TIME_MS: f64 = 5_000.0;

/// A brew is long enough to count on weight alone. `defaults.h:47`:
/// `BACKFLUSH_REMINDER_MIN_BREW_WEIGHT_G = 10.0`.
pub const MIN_BREW_WEIGHT_G: f32 = 10.0;

/// Whether a completed brew should increment the shot counter.
///
/// `maintenance/BackflushReminderLogic.h:18-28`, transcribed:
///
/// ```cpp
/// [[nodiscard]] inline constexpr bool qualifiesAsCountedShot(double totalBrewTimeMs,
///                                                            float  brewWeight,
///                                                            bool   scaleEnabled) noexcept {
///     if (totalBrewTimeMs >= BACKFLUSH_REMINDER_MIN_BREW_TIME_MS) {
///         return true;
///     }
///     if (scaleEnabled && brewWeight >= BACKFLUSH_REMINDER_MIN_BREW_WEIGHT_G) {
///         return true;
///     }
///     return false;
/// }
/// ```
///
/// Note that the weight arm is gated on `scaleEnabled`: with no scale fitted,
/// `brewWeight` is 0 and a 3-second shot must not be counted because "0 >= 10"
/// is false anyway — but with a *stale* non-zero weight it would be. The gate is
/// what stops a failed tare from counting a two-second shot.
#[must_use]
pub fn qualifies_as_counted_shot(
    total_brew_time_ms: f64,
    brew_weight: f32,
    scale_enabled: bool,
) -> bool {
    if total_brew_time_ms >= MIN_BREW_TIME_MS {
        return true;
    }
    scale_enabled && brew_weight >= MIN_BREW_WEIGHT_G
}

/// Whether the backflush reminder should be shown.
///
/// `maintenance/BackflushReminderLogic.h:30-32`, transcribed.
#[must_use]
pub const fn is_reminder_due(shots: i32, enabled: bool, threshold: i32) -> bool {
    enabled && shots >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_brew_counts_without_a_scale() {
        assert!(qualifies_as_counted_shot(MIN_BREW_TIME_MS, 0.0, false));
        assert!(qualifies_as_counted_shot(30_000.0, 0.0, false));
    }

    #[test]
    fn a_short_brew_with_enough_weight_counts_only_with_a_scale() {
        assert!(qualifies_as_counted_shot(1_000.0, 36.0, true));
        assert!(
            !qualifies_as_counted_shot(1_000.0, 36.0, false),
            "a stale weight must not count when no scale is fitted"
        );
    }

    #[test]
    fn a_short_brew_below_the_weight_minimum_does_not_count() {
        // `>=`, so exactly the minimum counts.
        assert!(qualifies_as_counted_shot(1_000.0, MIN_BREW_WEIGHT_G, true));
        assert!(!qualifies_as_counted_shot(1_000.0, 9.9, true));
    }

    #[test]
    fn a_brew_one_millisecond_short_of_the_minimum_does_not_count() {
        assert!(!qualifies_as_counted_shot(
            MIN_BREW_TIME_MS - 1.0,
            0.0,
            true
        ));
    }

    #[test]
    fn the_reminder_is_due_at_the_threshold_and_only_when_enabled() {
        assert!(is_reminder_due(50, true, 50));
        assert!(!is_reminder_due(49, true, 50));
        assert!(!is_reminder_due(50, false, 50));
        assert!(!is_reminder_due(0, true, 50));
    }
}
