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

/// The brew handler's maximum brew time. `BrewHandler`'s constructor:
/// `pumpTimer_(300000) // 5 minute max brew time safety`
/// (`BrewHandler.h:32`).
///
/// **Preserved deliberately, and dead.** `PumpTimer::isExpired()` returns false
/// unless `start()` was called, and nothing in `BrewHandler` ever calls it (see
/// [`PUMP_TIMEOUTS_NEVER_ARM`]). The constant is carried so the port is
/// faithful, not because anything reads it.
pub const BREW_PUMP_TIMEOUT_MS: u32 = 300_000;

/// The hot-water handler's maximum run time.
/// `HotWaterHandler`'s constructor: `pumpTimer_(60000) // 60 second max run
/// time` (`HotWaterHandler.h:28`). Dead in the same way as
/// [`BREW_PUMP_TIMEOUT_MS`].
pub const HOT_WATER_PUMP_TIMEOUT_MS: u32 = 60_000;

/// Whether the two `PumpTimer` watchdog checks can ever fire.
///
/// # Preserved deliberately, see `09-cpp-findings.md` §11
///
/// `BrewHandler::checkPumpTimeout` (`BrewHandler.h:254-262`) and
/// `HotWaterHandler::checkPumpTimeout` (`HotWaterHandler.h:114-122`) are the
/// firmware's only *run-time* bound on how long the pump may run continuously.
/// Both are inert:
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
/// This is a **new finding**, not in doc 09. It is a real loss of protection
/// (S-class: an unbounded pump run heats the boiler path and can be triggered
/// by a stuck or shorted switch) and it is preserved rather than fixed so that
/// the port's diff against the C++ is empty. Closing it is a deliberate change
/// for R4-09's `intentional-diffs.md`.
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
