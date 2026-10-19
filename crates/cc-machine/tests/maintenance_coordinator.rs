//! The shot counter and the backflush reminder, end to end.
//!
//! # What is real here and what is modelled
//!
//! **Real:** the reducer's increment, the qualification rule
//! ([`cc_machine::maintenance::qualifies_as_counted_shot`]), the effect the reducer emits, and
//! [`cc_machine::apply`]'s routing of it. Every case below drives a machine through a real brew and
//! a real transition, so a regression in any of those fails here.
//!
//! **Modelled:** the store and the shell's write. `Nvs` below stands in for the NVS partition, and
//! `Side` is the shape of the firmware's side channel plus the control task's one drain step.
//! Neither is host-testable where it lives -- the store is an `EspNvs` reached through the control
//! task's `&mut`, and the side channel is in a device crate -- so the protocol is re-implemented
//! here and the real implementations are held to it by review rather than by this file. What is
//! *not* modelled is the part that once failed: the increment and the effect.
//!
//! The counter only earns its place if the storage survives a reboot, so "reboot" here means
//! the stored bytes are read back and handed to a fresh machine -- the same shape `Control::boot`
//! (`cc-firmware/src/control.rs`) has.

mod common;

use cc_domain::state::MachineState;
use cc_domain::units::Millis;
use cc_machine::maintenance::{
    decode_shot_count, encode_shot_count, is_reminder_due, qualifies_as_counted_shot,
    BACKFLUSH_REMINDER_THRESHOLD, MIN_BREW_TIME_MS, MIN_BREW_WEIGHT_G,
};
use cc_machine::{Actuators, MachineChannels, Request};
use common::Harness;

// ---------------------------------------------------------------------------
// The qualification rule, which is pure
// ---------------------------------------------------------------------------

/// A brew shorter than the minimum does not count when no scale is fitted.
#[test]
fn rejects_short_brew_without_scale_weight() {
    assert!(!qualifies_as_counted_shot(3_000.0, 0.0, false));
}

/// A brew exactly at the minimum counts, and one millisecond short does not: the time arm is a
/// single `>=` and both sides of it are pinned.
#[test]
fn accepts_the_minimum_duration_and_not_a_millisecond_less() {
    assert!(qualifies_as_counted_shot(MIN_BREW_TIME_MS, 0.0, false));
    assert!(!qualifies_as_counted_shot(
        MIN_BREW_TIME_MS - 1.0,
        0.0,
        false
    ));
}

/// `AcceptsWeightWhenScaleEnabled`. The weight arm is gated on the scale, so
/// this is 1 s — well under the time minimum — and counts anyway.
#[test]
fn accepts_the_minimum_weight_when_a_scale_is_fitted() {
    assert!(qualifies_as_counted_shot(1_000.0, MIN_BREW_WEIGHT_G, true));
}

/// `RejectsShortBrewWithLowWeightWhenScaleEnabled`.
#[test]
fn rejects_a_short_brew_below_the_minimum_weight() {
    assert!(!qualifies_as_counted_shot(1_000.0, 5.0, true));
    // And the same weight without a scale fitted, which is the case the gate
    // exists for: a stale reading must not count a two-second shot.
    assert!(!qualifies_as_counted_shot(
        1_000.0,
        MIN_BREW_WEIGHT_G,
        false
    ));
}

/// The reminder boundary: due at the threshold, not one shot before it.
#[test]
fn the_reminder_is_due_at_fifty_shots_and_not_at_forty_nine() {
    assert!(!is_reminder_due(49, true, BACKFLUSH_REMINDER_THRESHOLD));
    assert!(is_reminder_due(50, true, BACKFLUSH_REMINDER_THRESHOLD));
    assert!(!is_reminder_due(50, false, BACKFLUSH_REMINDER_THRESHOLD));
}

// ---------------------------------------------------------------------------
// the coordinator — the counter, end to end
// ---------------------------------------------------------------------------

/// One qualifying brew, then a fresh machine over the same storage.
///
/// "Reboot" is what it is everywhere in this file: the stored bytes are read back and handed to
/// a new machine, which is what boot does with the count.
#[test]
fn a_qualified_brew_counts_and_survives_a_reboot() {
    let mut nvs = Nvs::default();
    let mut side = Side::with_stored(nvs.load());
    let mut h = harness();

    brew_once(&mut h, &mut side, &mut nvs);

    assert_eq!(h.machine.shots_since_backflush, 1);
    assert_eq!(nvs.stored(), Some(1), "and it was written down");

    // The reboot: the stored bytes are read back and handed to a fresh machine,
    // which is what `begin()` does and what `Control::boot` is given.
    let mut rebooted = harness();
    rebooted.machine.shots_since_backflush = nvs.load().unwrap_or(0);
    assert_eq!(rebooted.machine.shots_since_backflush, 1);
}

/// `SkipsUnqualifiedBrew`: 1 s, no scale, so neither arm of the rule fires, and
/// — the part that matters on a device — **nothing is written**. A brew that did
/// not count must not cost an NVS erase-and-commit.
#[test]
fn an_unqualified_brew_is_skipped_and_writes_nothing() {
    let mut nvs = Nvs::default();
    let mut side = Side::with_stored(nvs.load());
    let mut h = manual_harness();

    short_brew(&mut h, &mut side, &mut nvs);

    assert_eq!(
        h.machine.shots_since_backflush, 0,
        "a 1 s brew with no scale is not a shot"
    );
    assert_eq!(side.writes, 0, "no count, no write");
    assert_eq!(nvs.stored(), None);
}

/// Entering `BACKFLUSH_FINISHED` clears the counter: the reducer emits
/// `Effect::ResetShotsSinceBackflush`, the real applier routes it, and the cleared count is read
/// back out of storage.
#[test]
fn a_completed_backflush_clears_the_counter_and_survives_a_reboot() {
    let mut nvs = Nvs::default();
    let mut side = Side::with_stored(nvs.load());
    let mut h = harness();
    brew_once(&mut h, &mut side, &mut nvs);
    assert_eq!(nvs.stored(), Some(1));

    // Enter `BACKFLUSH_FINISHED` the way the reducer does.
    let fx = h.on_entry(MachineState::BackflushFinished);
    cc_machine::apply(&mut NoopActuators, &mut side, &h.machine, &fx);
    persist(&mut side, &mut nvs);

    assert_eq!(h.machine.shots_since_backflush, 0);
    assert_eq!(nvs.stored(), Some(0), "and it survives a reboot");
}

/// The default threshold, reached through the real reducer with a real write on every shot. A loop
/// over a helper that incremented a local would only re-test the arithmetic.
#[test]
fn fifty_brews_make_the_reminder_due_at_the_default_threshold() {
    let mut nvs = Nvs::default();
    let mut side = Side::with_stored(nvs.load());
    let mut h = harness();
    let threshold = h.config.maintenance.backflush_reminder.threshold;
    let enabled = h.config.maintenance.backflush_reminder.enabled;
    assert_eq!(
        threshold, BACKFLUSH_REMINDER_THRESHOLD,
        "the shipped default threshold, and the loop runs exactly that many times"
    );

    for _ in 0..threshold {
        // Back to `PID_NORMAL` between brews: each iteration is a separate brew, and the count is
        // the only thing that carries across them.
        let fx = h.elapse(4_000);
        cc_machine::apply(&mut NoopActuators, &mut side, &h.machine, &fx);
        persist(&mut side, &mut nvs);
        brew_once(&mut h, &mut side, &mut nvs);
    }

    assert_eq!(h.machine.shots_since_backflush, threshold);
    assert!(is_reminder_due(
        h.machine.shots_since_backflush,
        enabled,
        threshold
    ));
}

/// The count still rises with the reminder disabled, and re-enabling the reminder makes it due with
/// no change to the count.
#[test]
fn a_disabled_reminder_still_counts_but_is_never_due() {
    let mut nvs = Nvs::default();
    let mut side = Side::with_stored(nvs.load());
    let mut h = harness();
    let threshold = h.config.maintenance.backflush_reminder.threshold;
    h.config.maintenance.backflush_reminder.enabled = false;

    for _ in 0..threshold {
        let fx = h.elapse(4_000);
        cc_machine::apply(&mut NoopActuators, &mut side, &h.machine, &fx);
        persist(&mut side, &mut nvs);
        brew_once(&mut h, &mut side, &mut nvs);
    }

    assert_eq!(h.machine.shots_since_backflush, threshold);
    assert!(!is_reminder_due(
        h.machine.shots_since_backflush,
        false,
        threshold
    ));
    assert!(is_reminder_due(
        h.machine.shots_since_backflush,
        true,
        threshold
    ));
}

/// The operator's reset route: `POST /api/maintenance/reset-backflush-counter`.
///
/// There is no reducer event for it -- the count is not reducer state -- so this covers the
/// firmware's two lines (`Control::reset_shots_since_backflush` and
/// `FirmwareSide::on_reset_shots_since_backflush`) and the drain. The second line was the one
/// missing once: the counter cleared and came straight back on the next boot.
#[test]
fn the_operator_reset_route_resets_and_persists() {
    let mut nvs = Nvs::default();
    let mut side = Side::with_stored(nvs.load());
    let mut h = harness();
    brew_once(&mut h, &mut side, &mut nvs);
    assert_eq!(nvs.stored(), Some(1));

    // The route.
    h.machine.shots_since_backflush = 0;
    side.on_reset_shots_since_backflush(0);
    persist(&mut side, &mut nvs);

    assert_eq!(nvs.stored(), Some(0));
    let mut rebooted = harness();
    rebooted.machine.shots_since_backflush = nvs.load().unwrap_or(0);
    assert_eq!(rebooted.machine.shots_since_backflush, 0);
}

/// A failed write leaves the medium unchanged, and the count in memory is still right: the next
/// counted shot carries it forward and writes it.
#[test]
fn a_failed_write_leaves_the_previous_value_and_is_retried() {
    let mut nvs = Nvs::default();
    let mut side = Side::with_stored(nvs.load());
    let mut h = harness();
    h.machine.requests.set(Request::BrewStart, true);
    let fx = h.tick();
    cc_machine::apply(&mut NoopActuators, &mut side, &h.machine, &fx);
    nvs.fail_next_write = true;
    step_all(&mut h, &mut side, &mut nvs, 5_200);
    h.machine.brew.elapsed_ms = 30_000.0;
    h.machine.requests.set(Request::BrewStop, true);
    let fx = h.tick();
    cc_machine::apply(&mut NoopActuators, &mut side, &h.machine, &fx);
    persist(&mut side, &mut nvs);

    assert_eq!(nvs.stored(), None, "the write did not land");
    assert_eq!(
        h.machine.shots_since_backflush, 1,
        "and the count in memory is still right"
    );

    // The next brew carries the count forward and writes it.
    let fx = h.elapse(4_000);
    cc_machine::apply(&mut NoopActuators, &mut side, &h.machine, &fx);
    persist(&mut side, &mut nvs);
    brew_once(&mut h, &mut side, &mut nvs);
    assert_eq!(nvs.stored(), Some(2));
}

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

/// A machine configured so that a brew reaches `BREW_RUNNING` on its own: automatic, with 3 s of
/// pre-infusion and a 2 s pause.
fn harness() -> Harness {
    let mut h = Harness {
        config: common::automatic_brew_with_preinfusion(),
        machine: cc_machine::Machine::cold(),
        now: 0,
    };
    // The fixture every brew case starts from: `PID_NORMAL` with the runtime PID on, so the
    // machine is in the state the control loop spends its life in.
    h.machine = cc_machine::boot(Millis::new(0), &common::context_for(&h.config)).0;
    h.machine.state = MachineState::PidNormal;
    h.machine.pid.runtime_enabled = true;
    h
}

/// The same machine brewing **manually** with pre-infusion off, so a brew can be
/// short enough not to count without the fixture lying about the clock.
fn manual_harness() -> Harness {
    let mut h = harness();
    h.config.brew.mode = cc_domain::process::BrewMode::Manual;
    h.config.brew.pre_infusion.enabled = false;
    h
}

/// One brew, driven through the reducer, that counts.
///
/// Every effect goes through the real applier and the real drain, so the
/// increment, the effect, the routing and the write are all exercised: a
/// regression in any one of them fails here rather than on a machine.
fn brew_once(h: &mut Harness, side: &mut Side, nvs: &mut Nvs) {
    h.machine.requests.set(Request::BrewStart, true);
    step(h, side, nvs);
    assert_eq!(h.state(), MachineState::BrewPreinfusion);

    // Past the 3 s pre-infusion and the 2 s pause
    // (`automatic_brew_with_preinfusion`), so the brew is already over the 5 s
    // minimum before it is stopped.
    step_all(h, side, nvs, 5_200);
    assert_eq!(h.state(), MachineState::BrewRunning);
    assert!(
        h.machine.brew.elapsed_ms >= MIN_BREW_TIME_MS,
        "this brew is meant to count, and it is {} ms",
        h.machine.brew.elapsed_ms
    );

    h.machine.requests.set(Request::BrewStop, true);
    step(h, side, nvs);
    assert_eq!(h.state(), MachineState::BrewFinished);
}

/// One brew short enough **not** to count: manual, no pre-infusion, stopped immediately.
///
/// An automatic brew's timer includes the pre-infusion base and a manual one starts at zero, so
/// this is the only way to reach `BREW_FINISHED` with a brew time under the 5 s minimum without
/// reaching into the machine to lie about it.
fn short_brew(h: &mut Harness, side: &mut Side, nvs: &mut Nvs) {
    h.machine.requests.set(Request::BrewStart, true);
    step(h, side, nvs);
    assert_eq!(
        h.state(),
        MachineState::BrewRunning,
        "manual brewing skips pre-infusion"
    );
    h.machine.requests.set(Request::BrewStop, true);
    step(h, side, nvs);
    assert_eq!(h.state(), MachineState::BrewFinished);
    assert!(
        h.machine.brew.elapsed_ms < MIN_BREW_TIME_MS,
        "this brew is meant not to count, and it is {} ms",
        h.machine.brew.elapsed_ms
    );
}

/// One tick: reduce, apply, drain.
fn step(h: &mut Harness, side: &mut Side, nvs: &mut Nvs) {
    let fx = h.tick();
    cc_machine::apply(&mut NoopActuators, side, &h.machine, &fx);
    persist(side, nvs);
}

/// `elapse_ms` worth of ticks, 100 ms each — the control period.
fn step_all(h: &mut Harness, side: &mut Side, nvs: &mut Nvs, elapse_ms: u32) {
    let mut left = elapse_ms;
    while left > 0 {
        let slice = left.min(100);
        let fx = h.elapse(slice);
        cc_machine::apply(&mut NoopActuators, side, &h.machine, &fx);
        persist(side, nvs);
        left -= slice;
    }
}

/// The control task's step 7b: take what the applier asked for and write it.
fn persist(side: &mut Side, nvs: &mut Nvs) {
    let Some(shots) = side.take_shots_to_persist() else {
        return;
    };
    side.writes += 1;
    if nvs.save(shots).is_ok() {
        side.note_shots_persisted(shots);
    }
}

/// The store, with its one failure path: a write that reports failure.
#[derive(Default)]
struct Nvs {
    blob: Option<Vec<u8>>,
    fail_next_write: bool,
}

impl Nvs {
    /// An absent key reads as "no stored count".
    fn load(&self) -> Option<i32> {
        self.stored()
    }

    /// A write, on the same four bytes.
    fn save(&mut self, shots: i32) -> Result<(), ()> {
        if self.fail_next_write {
            self.fail_next_write = false;
            return Err(());
        }
        self.blob = Some(encode_shot_count(shots).to_vec());
        Ok(())
    }

    /// What `decode_shot_count` makes of the stored bytes: the raw read, so a test asserts on what
    /// is on the medium rather than on a parsed copy of it.
    fn stored(&self) -> Option<i32> {
        self.blob.as_deref().and_then(decode_shot_count)
    }
}

/// `cc_hal_esp32::FirmwareSide`'s maintenance half, plus the two things the
/// control task adds around it: the `take` and the `note` after a successful
/// write. `writes` counts the attempts, so "an unqualified brew wrote nothing"
/// is a real assertion and not an inference from the stored value.
#[derive(Default)]
struct Side {
    pending: Option<i32>,
    last: Option<i32>,
    writes: u32,
}

impl Side {
    /// `FirmwareSide::with_shots_since_backflush`.
    fn with_stored(stored: Option<i32>) -> Self {
        Self {
            pending: None,
            last: stored,
            writes: 0,
        }
    }

    /// `FirmwareSide::take_shots_to_persist`.
    fn take_shots_to_persist(&mut self) -> Option<i32> {
        let asked = self.pending?;
        if self.last == Some(asked) {
            return None;
        }
        self.pending = None;
        Some(asked)
    }

    /// `FirmwareSide::note_shots_persisted`.
    fn note_shots_persisted(&mut self, shots: i32) {
        self.last = Some(shots);
    }
}

impl MachineChannels for Side {
    /// `FirmwareSide::on_record_brew`.
    fn on_record_brew(&mut self, counted: bool, shots_since_backflush: i32) {
        if counted {
            self.pending = Some(shots_since_backflush);
        }
    }

    /// `FirmwareSide::on_reset_shots_since_backflush`.
    fn on_reset_shots_since_backflush(&mut self, shots_since_backflush: i32) {
        self.pending = Some(shots_since_backflush);
    }

    /// Not exercised here: this file is about the counter, and a diagnostics
    /// sink would only get in the way. `FirmwareSide` returns `Some`.
    fn on_request_reboot(&mut self) {}
}

/// The applier's hardware half, discarding every write.
///
/// The state under test never leaves `BREW_RUNNING` for anywhere that matters,
/// and a real `Actuators` needs pins. What matters is that [`cc_machine::apply`]
/// is the thing routing the effects, so the routing is real and only the writes
/// are absent.
struct NoopActuators;

impl Actuators for NoopActuators {
    fn enable_pump(&mut self) {}
    fn disable_pump(&mut self) {}
    fn open_water_valve(&mut self) {}
    fn close_water_valve(&mut self) {}
    fn open_steam_valve(&mut self) {}
    fn close_steam_valve(&mut self) {}
    fn enable_heater(&mut self) {}
    fn disable_heater(&mut self) {}
    fn set_heater_duty(&mut self, _duty: f32) {}
    fn emergency_shutdown(&mut self) {}
    fn safe_hardware_shutdown(&mut self) {}
}
