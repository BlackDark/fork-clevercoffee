//! Shot counting and the backflush reminder: the qualification rule, the counter's
//! increment, and the reminder predicate.
//!
//! Small and pure, and it is the state machine's only maintenance behaviour. Only the
//! parts the reducer touches live here: the qualification test `BrewFinished` calls, the
//! reset `BackflushFinished` calls, and the reminder predicate. Persisting the counter is
//! the store's job --
//! [`Machine::shots_since_backflush`](crate::machine::Machine::shots_since_backflush) is
//! the value, and the shell writes it.
//!
//! # Where the counter is changed, and why it is here
//!
//! Incrementing on a completed shot is three facts, each with a different owner: the
//! decision ([`qualifies_as_counted_shot`]), the increment (which can only be written
//! where `Machine` is written, i.e. the reducer), and the write (the task that owns the
//! store). [`record_brew_if_qualified`] does the first two, and the shell is told the
//! outcome through [`Effect::RecordBrew`](crate::effect::Effect::RecordBrew).
//!
//! The decision is therefore made **once**. Emitting the three facts and leaving the
//! qualification to the applier moves the rule to a place reachable only by wiring it up,
//! and a rule that has to be *wired* can be forgotten: the counter would never move and
//! `/api/status` would report `shotsSinceBackflush: 0` for the life of the firmware.
/// A brew is long enough to count on time alone: five seconds.
pub const MIN_BREW_TIME_MS: f64 = 5_000.0;

/// A brew is long enough to count on weight alone: ten grams.
pub const MIN_BREW_WEIGHT_G: f32 = 10.0;

/// The shipped reminder threshold, in counted shots.
///
/// The default rather than the rule: the operator sets
/// `maintenance.backflush_reminder.threshold` (`1..=500`) and the firmware uses whatever
/// that says, which is why [`is_reminder_due`] takes it as an argument. This constant is
/// the value a default configuration produces, and the one the tests below use to walk a
/// machine up to the boundary.
pub const BACKFLUSH_REMINDER_THRESHOLD: i32 = 50;

/// Whether a completed brew should increment the shot counter.
///
/// Either arm qualifies: at least [`MIN_BREW_TIME_MS`] of brew time, or -- with a scale
/// fitted -- at least [`MIN_BREW_WEIGHT_G`] of brew weight.
///
/// The weight arm is gated on a fitted scale, and the gate is the point. With no scale
/// fitted the reading is 0, and "0 >= 10" is false anyway; but a *stale* non-zero weight
/// left over from a failed tare would otherwise count a two-second flush as a shot. The
/// gate is what stops that.
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

/// [`qualifies_as_counted_shot`] plus the increment, and nothing else.
///
/// Returns whether the shot counted, which is what the emitted effect carries, so the shell
/// persists a decision rather than re-deriving one.
///
/// The increment saturates. The release profile sets `overflow-checks = true`, so a plain
/// `+= 1` on an `i32` would panic at 2^31 -- a machine plugged in for a year would reboot
/// rather than stop counting. The threshold is 500 at most, so reaching the limit at all
/// means the stored value is already corrupt.
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

/// Whether the backflush reminder should be shown: enabled, and at or past the threshold.
#[must_use]
pub const fn is_reminder_due(shots: i32, enabled: bool, threshold: i32) -> bool {
    enabled && shots >= threshold
}

/// The width of the stored shot counter, in bytes.
///
/// A four-byte integer, so an `nvs_dump` of a development board shows a recognisable
/// number rather than an escape.
pub const SHOT_COUNT_BYTES: usize = 4;

/// The shot counter as the bytes NVS holds for it.
///
/// Little-endian, like every other integer this firmware writes
/// ([`cc_protocol::sensor::hx711`](../../cc_domain/sensor/hx711/index.html)'s
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

    // ---- the qualification rule and the increment --------------------------
    //
    // The two pure halves, driven directly. The counter's persistence is a host test in
    // `tests/maintenance_coordinator.rs`.

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
        // A stale weight with no scale fitted is a failed tare, not a shot, which is why
        // the scale flag is explicit rather than inferred from the reading.
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
