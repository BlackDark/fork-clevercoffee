//! The timing constants the state machine depends on, transcribed.
//!
//! Every value here has a C++ source. Nothing is invented and nothing is
//! rounded: if a constant is not in this file, the state machine does not use
//! it, and if a value here is wrong the C++ is wrong in the same way (which is
//! the point — a port that quietly "fixed" a constant would be a silent
//! behavioural change).
//!
//! | Constant | C++ |
//! | --- | --- |
//! | [`ERROR_RECOVERY_DELAY_MS`] | `Timing::ERROR_RECOVERY_DELAY_MS` (`constants/Timing.h:32`) |
//! | [`EEPROM_RECOVERY_TIMEOUT_MS`] | `Timing::EEPROM_RECOVERY_TIMEOUT_MS` (`constants/Timing.h:33`) |
//! | [`BREW_FINISHED_DISPLAY_TIMEOUT_MS`] | `BrewTiming::FINISHED_DISPLAY_TIMEOUT_MS` (`constants/Timing.h:64`) |
//! | [`BACKFLUSH_FINISHED_DISPLAY_TIMEOUT_MS`] | `BackflushTiming::FINISHED_DISPLAY_TIMEOUT_MS` (`constants/Timing.h:75`) |
//! | [`POWER_SWITCH_SETTLE_MS`] | `PowerHandler.h:118,143` (the literal `5000`) |
//! | [`POWER_LONG_PRESS_REBOOT_MS`] | `PowerHandler.h:144` (the literal `1000`) |
//! | [`POWER_REBOOT_DISPLAY_MS`] | `PowerHandler.h:183,189` (the two `delay(1000)` calls) |
//! | [`STANDBY_UPDATE_GRANULARITY_MS`] | `StandbyCoordinator.h:39` (the literal `1000`) |
//! | [`STANDBY_DISPLAY_OFF_MS`] | `StandbyCoordinator.h:14` (`10 * 60 * 1000`) |
//! | [`BREW_PUMP_TIMEOUT_MS`] | `BrewHandler.h:32` (`pumpTimer_(300000)`) |
//! | [`HOT_WATER_PUMP_TIMEOUT_MS`] | `HotWaterHandler.h:28` (`pumpTimer_(60000)`) |

/// How long a sensor error must be clear before `SENSOR_ERROR` resumes normal
/// operation. `Timing::ERROR_RECOVERY_DELAY_MS = 5000`
/// (`include/clevercoffee/constants/Timing.h:32`).
pub const ERROR_RECOVERY_DELAY_MS: u32 = 5_000;

/// How long `EEPROM_ERROR` waits before attempting recovery by transitioning to
/// `PID_DISABLED`. `Timing::EEPROM_RECOVERY_TIMEOUT_MS = 300000`
/// (`include/clevercoffee/constants/Timing.h:33`).
pub const EEPROM_RECOVERY_TIMEOUT_MS: u32 = 300_000;

/// How long `BREW_FINISHED` shows the result before returning to the PID state.
/// `BrewTiming::FINISHED_DISPLAY_TIMEOUT_MS = 3000`
/// (`include/clevercoffee/constants/Timing.h:64`).
pub const BREW_FINISHED_DISPLAY_TIMEOUT_MS: u32 = 3_000;

/// How long `BACKFLUSH_FINISHED` is shown before returning to
/// `BACKFLUSH_IDLE`. `BackflushTiming::FINISHED_DISPLAY_TIMEOUT_MS = 3000`
/// (`include/clevercoffee/constants/Timing.h:75`).
pub const BACKFLUSH_FINISHED_DISPLAY_TIMEOUT_MS: u32 = 3_000;

/// The power switch ignores presses for this long after boot.
/// `PowerHandler::handlePowerButtonPress` requires
/// `currentMillis - systemInitializedTime_ > 5000` (`PowerHandler.h:118`).
pub const POWER_SWITCH_SETTLE_MS: u32 = 5_000;

/// How long the power switch must be held (and must report a hardware long
/// press) before a reboot is requested.
/// `PowerHandler::checkForLongPressReboot` requires
/// `currentMillis - longPressStartTime_ > 1000` (`PowerHandler.h:144`).
pub const POWER_LONG_PRESS_REBOOT_MS: u32 = 1_000;

/// How long the "REBOOTING" message is shown before the reboot is issued.
/// The two `delay(1000)` calls at `PowerHandler.h:183` and `PowerHandler.h:189`.
pub const POWER_REBOOT_DISPLAY_MS: u32 = 1_000;

/// `StandbyCoordinator::update()` only recomputes the countdown once a second
/// (`StandbyCoordinator.h:39`), so the standby transition is quantised to this
/// granularity. Preserved: a port that recomputed every tick would enter
/// standby up to a second earlier.
pub const STANDBY_UPDATE_GRANULARITY_MS: u32 = 1_000;

/// How long the panel stays lit after the machine enters standby.
///
/// `StandbyCoordinator::getDisplayOffTimeoutMillis()`
/// (`StandbyCoordinator.h:13-15`) — `10 * 60 * 1000`, ten minutes, and a
/// literal in the C++ rather than a configuration parameter. The countdown runs
/// from the same start time as the standby countdown, so the panel goes dark
/// `standby.time` + 10 minutes after the last activity, not 10 minutes after
/// entering standby.
///
/// Ported at the same time as `StandbyTimer::display_off_remaining_ms`, which
/// had been declared and left un-ported; the symptom was a panel that blanked
/// the moment standby was entered.
pub const STANDBY_DISPLAY_OFF_MS: u32 = 10 * 60 * 1_000;

/// The brew handler's maximum brew time. `BrewHandler`'s constructor:
/// `pumpTimer_(300000) // 5 minute max brew time safety`
/// (`BrewHandler.h:32`).
///
/// **Dead in the C++, live here** — see 09 §11 and
/// [`PUMP_TIMEOUTS_NEVER_ARM`]. The port arms it on the pump-on edge in
/// [`crate::handlers::arm_pump_watchdogs`].
pub const BREW_PUMP_TIMEOUT_MS: u32 = 300_000;

/// The hot-water handler's maximum run time.
/// `HotWaterHandler`'s constructor: `pumpTimer_(60000) // 60 second max run
/// time` (`HotWaterHandler.h:28`). Dead in the C++ in the same way as
/// [`BREW_PUMP_TIMEOUT_MS`].
pub const HOT_WATER_PUMP_TIMEOUT_MS: u32 = 60_000;

/// Whether the two `PumpTimer` watchdog checks can ever fire **in the C++**.
///
/// # Dead in the C++, armed in the port — see 09 §11
///
/// `BrewHandler::checkPumpTimeout` (`BrewHandler.h:254-262`) and
/// `HotWaterHandler::checkPumpTimeout` (`HotWaterHandler.h:114-122`) are the
/// firmware's only *run-time* bound on how long the pump may run continuously.
/// In the C++ both are inert:
///
/// ```cpp
/// bool isExpired() const {
///     if (!isRunning_ || startTime_ == 0) return false;
///     return (millis() - startTime_) > maxRunTime_;
/// }
/// ```
/// (`handlers/PumpTimer.h:23-26`)
///
/// `isRunning_` is initialised `false` (`PumpTimer.h:14`) and `start()` is
/// **never called** — `rg -n 'pumpTimer_\.' src/ include/` finds only the two
/// declarations, two constructor arguments and the two `isExpired()` reads. So
/// both `isExpired()` calls return `false` unconditionally and the pump can run
/// for as long as the operator holds the switch.
///
/// **This constant is a statement about the C++, not about this port.** It
/// stays `true` because the fact it records is still true of the firmware we are
/// replacing. The port's behaviour is the opposite: both watchdogs are armed on
/// the activating edge by [`crate::handlers::arm_pump_watchdogs`], a trip emits
/// [`crate::Effect::PumpTimeoutFired`] so it is visible in the log, and then the
/// C++'s own action — a brew-stop request, or `DisablePump`.
///
/// Deliberately **not** done: rejecting the deadline at construction time
/// instead of arming it. The check is a protection, it is the C++'s own, and the
/// only question was whether it could ever run.
///
/// This is a **deliberate divergence** and is line 1 of
/// `docs/rust-migration/intentional-diffs.md`.
pub const PUMP_TIMEOUTS_NEVER_ARM: bool = true;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_the_cpp_headers() {
        assert_eq!(ERROR_RECOVERY_DELAY_MS, 5_000);
        assert_eq!(EEPROM_RECOVERY_TIMEOUT_MS, 300_000);
        assert_eq!(BREW_FINISHED_DISPLAY_TIMEOUT_MS, 3_000);
        assert_eq!(BACKFLUSH_FINISHED_DISPLAY_TIMEOUT_MS, 3_000);
        assert_eq!(POWER_SWITCH_SETTLE_MS, 5_000);
        assert_eq!(POWER_LONG_PRESS_REBOOT_MS, 1_000);
        assert_eq!(POWER_REBOOT_DISPLAY_MS, 1_000);
        assert_eq!(STANDBY_UPDATE_GRANULARITY_MS, 1_000);
        assert_eq!(STANDBY_DISPLAY_OFF_MS, 600_000);
        assert_eq!(BREW_PUMP_TIMEOUT_MS, 300_000);
        assert_eq!(HOT_WATER_PUMP_TIMEOUT_MS, 60_000);
    }

    /// Pins the C++'s own comment: the test fixture docstring at
    /// `test/test_sensor_error_state/test_main.cpp:144` says
    /// "Jump past `ERROR_RECOVERY_DELAY_MS` (5000 ms)". The `2000` ms in the
    /// neighbouring docstring at line 122 is stale.
    #[test]
    fn error_recovery_delay_is_five_seconds_not_two() {
        assert_eq!(ERROR_RECOVERY_DELAY_MS, 5_000);
    }
}
