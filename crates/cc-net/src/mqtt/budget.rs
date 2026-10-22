//! Publish cadence: per-call budget, pass intervals, and the state → interval
//! selector. Plain `u32`s; the caller owns the clock.

/// Publish budget per call, in milliseconds.
///
/// A fraction of the 10 ms control period, not of itself. `Feed::service` runs on
/// **every** tick, so a budget equal to the whole period bounds nothing: one
/// publish would take the tick and the sensor read, the PID and the applier are
/// what get squeezed.
///
/// 2 ms still finishes a whole pass inside the slowest interval that matters even
/// at one topic per tick, so throughput costs nothing. It only costs a broker that
/// has stopped draining its outbox.
pub const TIME_BUDGET_MS: u32 = 2;

/// Interval between full telemetry passes, in milliseconds.
pub const INTERVAL_MS: u32 = 5_000;

/// Interval while brewing, in milliseconds.
///
/// Shorter than idle: the weight is moving and an operator is watching.
pub const INTERVAL_BREW_MS: u32 = 500;

/// Interval in `STANDBY`, in milliseconds. Nothing in standby is worth
/// watching at 2 Hz.
pub const INTERVAL_STANDBY_MS: u32 = 10_000;

/// How often the Home Assistant discovery payloads are republished, in
/// milliseconds.
///
/// Documents are retained, so they only need republishing when something may have
/// lost them. 300 s re-learns after a restart within one coffee.
pub const DISCOVERY_INTERVAL_MS: u32 = 300_000;

/// The interval for `state`.
///
/// `BREW_FINISHED` is excluded from the brew arm: the result screen is static for
/// its whole three seconds, so it keeps the slow cadence.
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
