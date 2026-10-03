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
//!
//! # Where the counter is changed, and why it is here
//!
//! The C++ calls `maintenanceCoordinator().recordBrewIfQualified(...)` from
//! `BrewFinishedState::onEntryImpl` (`BrewStates.cpp:306-312`), and that one
//! call does three things: apply [`qualifies_as_counted_shot`], increment, and
//! write NVS. In the port the three split, because the three have three owners:
//! the increment belongs to [`Machine`](crate::machine::Machine) and can only be
//! written where `Machine` is written — the reducer — and the write belongs to
//! the task that owns the store. So [`record_brew_if_qualified`] does the first
//! two at the point the C++ evaluates them, and the shell is told the outcome
//! through [`Effect::RecordBrew`](crate::effect::Effect::RecordBrew).
//!
//! The decision is therefore made **once**. An earlier version of this port
//! emitted the three facts and left the qualification to the applier, which
//! meant the C++'s rule was evaluated in a place that could only be reached by
//! wiring it up — and it was not, so the counter never moved and
//! `/api/status` reported `shotsSinceBackflush: 0` for the life of the
//! firmware. A rule that lives in one place and is evaluated in one place
//! cannot be forgotten; a rule that has to be *wired* can.

/// A brew is long enough to count on time alone. `defaults.h:46`:
/// `BACKFLUSH_REMINDER_MIN_BREW_TIME_MS = 5000.0`.
pub const MIN_BREW_TIME_MS: f64 = 5_000.0;

/// A brew is long enough to count on weight alone. `defaults.h:47`:
/// `BACKFLUSH_REMINDER_MIN_BREW_WEIGHT_G = 10.0`.
pub const MIN_BREW_WEIGHT_G: f32 = 10.0;

/// The shipped reminder threshold, shots. `defaults.h:43`:
/// `BACKFLUSH_REMINDER_THRESHOLD = 50`.
///
/// The default rather than the rule: an operator sets
/// `maintenance.backflush_reminder.threshold` and the firmware uses whatever
/// that says (`Config.h:1283`), which is why [`is_reminder_due`] takes it as an
/// argument. This constant is the value a default configuration produces, and
/// the one the tests below use to walk a machine up to the boundary.
pub const BACKFLUSH_REMINDER_THRESHOLD: i32 = 50;

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

/// `MaintenanceCoordinator::recordBrewIfQualified` with the NVS write removed:
/// the decision **and** the increment, and nothing else.
///
/// `MaintenanceCoordinator.cpp:30-49`. Returns whether the shot counted, which
/// is what the emitted effect carries, so that the shell persists a decision
/// rather than re-deriving one.
///
/// The increment saturates. The C++'s `++shotsSinceBackflush_` is a signed
/// increment that overflows at 2^31, and the release profile sets
/// `overflow-checks = true`, so a plain `+= 1` here would be a panic on a
/// machine that had been plugged in for a year rather than a counter that stops
/// moving. The threshold is 500 at most (`BACKFLUSH_REMINDER_THRESHOLD_MAX`,
/// `defaults.h:45`), so the only way here is a corrupted stored value.
#[must_use]
pub fn record_brew_if_qualified(
    shots: &mut i32,
    total_brew_time_ms: f64,
    brew_weight: f32,
    scale_enabled: bool,
) -> bool {
    if !qualifies_as_counted_shot(total_brew_time_ms, brew_weight, scale_enabled) {
        return false;
    }
    *shots = shots.saturating_add(1);
    true
}

/// Whether the backflush reminder should be shown.
///
/// `maintenance/BackflushReminderLogic.h:30-32`, transcribed.
#[must_use]
pub const fn is_reminder_due(shots: i32, enabled: bool, threshold: i32) -> bool {
    enabled && shots >= threshold
}

/// The width of the stored shot counter, in bytes.
///
/// `prefs.putInt(...)` (`MaintenanceCoordinator.cpp:82`) is a four-byte integer
/// and this is the same width, so an `nvs_dump` of a development board shows a
/// recognisable number rather than an escape.
pub const SHOT_COUNT_BYTES: usize = 4;

/// The shot counter as the bytes NVS holds for it.
///
/// Little-endian, like every other integer this firmware writes
/// ([`cc_domain::sensor::hx711`](../../cc_domain/sensor/hx711/index.html)'s
/// tare record), so a little-endian reader of the partition sees a plain
/// integer.
#[must_use]
pub const fn encode_shot_count(shots: i32) -> [u8; SHOT_COUNT_BYTES] {
    shots.to_le_bytes()
}

/// The shot counter back out of NVS, or `None` if the stored blob is not one.
///
/// `None` is not an error and the caller must not treat it as one: a key that is
/// absent, a key written by a firmware that is not this one, and a key holding
/// something else are all "no stored count", which is what a freshly erased
/// partition says too. `load_tare` in `cc-hal-esp32::nvs` is the same shape for
/// the same reason.
#[must_use]
pub fn decode_shot_count(bytes: &[u8]) -> Option<i32> {
    let raw: [u8; SHOT_COUNT_BYTES] = bytes.try_into().ok()?;
    Some(i32::from_le_bytes(raw))
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

    // ---- `MaintenanceCoordinator::recordBrewIfQualified` -------------------
    //
    // The C++'s `MaintenanceCoordinatorTest` (10 cases in
    // `test/test_maintenance_coordinator/test_main.cpp`) drives the rule through
    // the coordinator; these drive the two pure halves directly, and the
    // coordinator's persistence is a host test in
    // `tests/ported_maintenance_coordinator.rs`.

    #[test]
    fn a_qualified_brew_increments_and_an_unqualified_one_does_not() {
        let mut shots = 0;
        assert!(record_brew_if_qualified(
            &mut shots,
            MIN_BREW_TIME_MS,
            0.0,
            false
        ));
        assert_eq!(shots, 1);
        assert!(!record_brew_if_qualified(&mut shots, 1_000.0, 0.0, false));
        assert_eq!(shots, 1, "an unqualified brew must leave the count alone");
    }

    #[test]
    fn the_weight_arm_of_the_rule_needs_the_scale() {
        // `MaintenanceCoordinatorTest`'s fixtures pass `scaleEnabled` explicitly
        // for this reason; a stale weight with no scale fitted is a failed tare,
        // not a shot.
        let mut shots = 0;
        assert!(!record_brew_if_qualified(&mut shots, 1_000.0, 36.0, false));
        assert!(record_brew_if_qualified(&mut shots, 1_000.0, 36.0, true));
        assert_eq!(shots, 1);
    }

    #[test]
    fn a_disabled_reminder_still_counts() {
        // `DisabledReminderStillCountsButNotDue`: the enabled flag gates the
        // *reminder*, not the count. `record_brewIfQualified` is not given the
        // flag at all, and this is the test that says that is right.
        let mut shots = 0;
        for _ in 0..BACKFLUSH_REMINDER_THRESHOLD {
            assert!(record_brew_if_qualified(&mut shots, 30_000.0, 0.0, false));
        }
        assert_eq!(shots, BACKFLUSH_REMINDER_THRESHOLD);
        // …and the count is what makes it due, but only once the reminder is
        // enabled. `DisabledReminderStillCountsButNotDue` is exactly this pair of
        // assertions, and it is the reason `is_reminder_due` takes `enabled` at
        // all rather than the count alone being the answer.
        assert!(!is_reminder_due(shots, false, BACKFLUSH_REMINDER_THRESHOLD));
        assert!(is_reminder_due(shots, true, BACKFLUSH_REMINDER_THRESHOLD));
    }

    #[test]
    fn the_count_saturates_rather_than_overflowing() {
        // `overflow-checks = true` in the release profile, so this is the
        // difference between a counter that stops and a panic. 2^31 shots is not
        // reachable honestly; a corrupted stored value is.
        let mut shots = i32::MAX;
        assert!(record_brew_if_qualified(&mut shots, 30_000.0, 0.0, false));
        assert_eq!(shots, i32::MAX);
    }

    // ---- the stored form ---------------------------------------------------

    #[test]
    fn a_shot_count_round_trips_through_the_stored_form() {
        for shots in [0_i32, 1, 49, 50, i32::MAX, -1] {
            let bytes = encode_shot_count(shots);
            assert_eq!(bytes.len(), SHOT_COUNT_BYTES);
            assert_eq!(decode_shot_count(&bytes), Some(shots));
        }
    }

    #[test]
    fn a_blob_that_is_not_a_shot_count_is_no_shot_count() {
        // The failure a foreign writer or a half-written key produces. `None`,
        // so the boot path reports "no stored count" — the same thing a freshly
        // erased partition says.
        assert_eq!(decode_shot_count(&[]), None);
        assert_eq!(decode_shot_count(&[0, 0, 0]), None);
        assert_eq!(decode_shot_count(&[0, 0, 0, 0, 0]), None);
    }
}
