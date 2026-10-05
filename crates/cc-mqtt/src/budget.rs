//! The publish cadence: the budget per call and the three intervals, and the
//! one function that picks an interval from the machine state.
//!
//! The budget is a device claim — 20 % of the control task's 10 ms period — and
//! the *intervals* are the C++'s, but both are plain `u32`s and
//! `interval_for` is a match on a [`cc_domain::state::MachineState`]. Nothing
//! here reads a clock; the caller passes `now`.

/// The publish budget per iteration, in milliseconds.
///
/// **Re-derived against this firmware's control period, not the C++'s.** The
/// C++ sets `timeBudget_ = 10` (`MQTTManager.h:259`), checked after each publish
/// at `MQTTManager.cpp:501, 517, 546`, and justifies it against a 400 ms
/// temperature-sensor interval. R4-01 moved the loop to 100 Hz, so that
/// justification is stale and the ratio is not: `cc_hal_esp32::mqtt::Feed::service` is called from
/// **every** tick, and the control period is **10 ms**
/// (`cc-firmware/src/main.rs:189`, `CONTROL_PERIOD_MS`). A budget equal to the
/// whole period bounds nothing — it permits one publish attempt to occupy a
/// tick entirely, and the control task's own work (the SENSE reading, the PID,
/// the applier, the display hand-off) is what gets squeezed instead.
///
/// At 2 ms, a full pass of the ~46 registered topics still finishes well inside
/// the slowest interval that matters ([`INTERVAL_BREW_MS`], 500 ms) even when
/// every publish misses the budget and one topic is published per tick, so the
/// budget costs throughput nothing; it only costs a broker that has stopped
/// draining its outbox. 20 % of the period leaves the rest of the tick to the
/// machine.
pub const TIME_BUDGET_MS: u32 = 2;

/// The interval between full telemetry passes, in milliseconds.
///
/// `MQTTManager.h:260` `intervalMQTT_ = 5000`.
pub const INTERVAL_MS: u32 = 5_000;

/// The interval while a brew state is active, in milliseconds.
///
/// `MQTTManager.h:261` `intervalMQTTBrew_ = 500`. See [`interval_for`] and the
/// module documentation on why this is reachable.
pub const INTERVAL_BREW_MS: u32 = 500;

/// The interval in `STANDBY`, in milliseconds. `MQTTManager.h:262`.
pub const INTERVAL_STANDBY_MS: u32 = 10_000;

/// How often the Home Assistant discovery payloads are republished, in
/// milliseconds.
///
/// `Timing::HASSIO_DISCOVERY_INTERVAL_MS`, wired at `LoopManager.cpp:382-384`.
/// 300 s, so a restarted Home Assistant or a broker that lost its retained store
/// re-learns the machine within one coffee.
pub const DISCOVERY_INTERVAL_MS: u32 = 300_000;

/// The interval [`interval_for`] selects, from the machine state.
///
/// `MQTTManager.cpp:384-387`:
///
/// ```cpp
/// bool isBrewActive = (isBrewState(currentState) && currentState != BREW_FINISHED);
/// unsigned long interval = isBrewActive ? intervalMQTTBrew_
///                        : (currentState == STANDBY) ? intervalMQTTStandby_
///                        : intervalMQTT_;
/// ```
///
/// `BREW_FINISHED` is excluded from the brew arm, which is the same predicate
/// `BrewHandler::isBrewActive` uses (`BrewHandler.h:98-103`) and the same one
/// `cc_domain::state::MachineState::is_brew_state` plus the explicit exclusion
/// reproduces.
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
