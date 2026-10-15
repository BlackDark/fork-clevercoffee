//! The publish cadence: the per-call budget, the three pass intervals, and the
//! function that picks one from the machine state.
//!
//! Plain `u32`s and a `match` on a [`cc_domain::state::MachineState`]. Nothing
//! here reads a clock; the caller passes `now`.

/// The publish budget per iteration, in milliseconds.
///
/// **A fraction of the control period, not of itself.** `Feed::service` runs on
/// **every** tick and the control period is 10 ms, so a budget equal to the whole
/// period bounds nothing: it lets one publish attempt occupy a tick entirely, and
/// the control task's own work — the sensor read, the PID, the applier, the
/// display hand-off — is what gets squeezed.
///
/// At 2 ms a full pass still finishes well inside the slowest interval that
/// matters ([`INTERVAL_BREW_MS`]) even in the worst case of every publish
/// missing the budget and one topic going out per tick, so the budget costs
/// throughput nothing. It only costs a broker that has stopped draining its
/// outbox. The remaining 80 % of the tick is the machine's.
pub const TIME_BUDGET_MS: u32 = 2;

/// The interval between full telemetry passes, in milliseconds.
pub const INTERVAL_MS: u32 = 5_000;

/// The interval while a brew state is active, in milliseconds.
///
/// Shorter because a weight is moving and an operator is watching it; see
/// [`interval_for`] for which states count.
pub const INTERVAL_BREW_MS: u32 = 500;

/// The interval in `STANDBY`, in milliseconds — longer than idle, because
/// nothing in standby is worth watching at 2 Hz.
pub const INTERVAL_STANDBY_MS: u32 = 10_000;

/// How often the Home Assistant discovery payloads are republished, in
/// milliseconds.
///
/// Long, because discovery documents are retained and only need republishing when
/// something may have lost them. At 300 s a restarted Home Assistant, or a broker
/// that lost its retained store, re-learns the machine within one coffee.
pub const DISCOVERY_INTERVAL_MS: u32 = 300_000;

/// The interval [`interval_for`] selects, from the machine state.
///
/// `BREW_FINISHED` is excluded from the brew arm, so the result display keeps the
/// slow cadence for its whole three seconds rather than reporting at 2 Hz for a
/// screen that is not changing.
#[must_use]
pub fn interval_for(state: cc_domain::state::MachineState) -> u32 {
    let brew_active =
        state.is_brew_state() && state != cc_domain::state::MachineState::BrewFinished;
    if brew_active {
        INTERVAL_BREW_MS
    } else if state == cc_domain::state::MachineState::Standby {
        INTERVAL_STANDBY_MS
    } else {
        INTERVAL_MS
    }
}
