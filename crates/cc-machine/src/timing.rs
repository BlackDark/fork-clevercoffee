//! The timing constants the state machine depends on.
//!
//! Every value here is behaviour, not tuning: the machine's recovery delays, display
//! timeouts, standby cadence and pump watchdogs are pinned to these numbers, and changing one
//! changes what the machine does. They are gathered in one file so that a change is one diff.
/// How long a sensor fault must be clear before `SENSOR_ERROR` resumes normal operation:
/// five seconds.
pub const ERROR_RECOVERY_DELAY_MS: u32 = 5_000;

/// How long `EEPROM_ERROR` waits before attempting recovery by transitioning to
/// `PID_DISABLED`: five minutes.
pub const EEPROM_RECOVERY_TIMEOUT_MS: u32 = 300_000;

/// How long `BREW_FINISHED` shows the result before returning to the PID state: three
/// seconds.
pub const BREW_FINISHED_DISPLAY_TIMEOUT_MS: u32 = 3_000;

/// How long `BACKFLUSH_FINISHED` is shown before returning to `BACKFLUSH_IDLE`: three
/// seconds.
pub const BACKFLUSH_FINISHED_DISPLAY_TIMEOUT_MS: u32 = 3_000;

/// How long the power switch ignores presses after boot: the machine must have been
/// running for five seconds before a press counts.
pub const POWER_SWITCH_SETTLE_MS: u32 = 5_000;

/// How long the power switch must be held, and must report a hardware long press, before a
/// reboot is requested: one second.
pub const POWER_LONG_PRESS_REBOOT_MS: u32 = 1_000;

/// How long the "REBOOTING" message is shown before the reboot is issued: one second.
pub const POWER_REBOOT_DISPLAY_MS: u32 = 1_000;

/// The standby countdown is recomputed once a second, not every tick, so the transition is
/// quantised to this granularity. A machine that recomputed every tick would enter standby
/// up to a second earlier, which is a behaviour change and not an optimisation.
pub const STANDBY_UPDATE_GRANULARITY_MS: u32 = 1_000;

/// How long the panel stays lit after the last activity: ten minutes.
///
/// Not a configuration parameter, and its countdown runs from the same start time as the
/// standby countdown -- so the panel goes dark `standby.time` + 10 minutes after the last
/// activity, not 10 minutes after entering standby.
pub const STANDBY_DISPLAY_OFF_MS: u32 = 10 * 60 * 1_000;

/// The maximum brew time: five minutes.
///
/// Armed on the pump-on edge in [`crate::handlers::arm_pump_watchdogs`]. A trip emits
/// [`crate::Effect::PumpTimeoutFired`], so it is visible in the log, and then requests the
/// brew stop. §1 of `docs/history/divergences.md` records why arming it is deliberate.
pub const BREW_PUMP_TIMEOUT_MS: u32 = 300_000;

/// The maximum hot-water run time: sixty seconds.
///
/// The same watchdog as [`BREW_PUMP_TIMEOUT_MS`], armed on the same edge and reported the
/// same way.
pub const HOT_WATER_PUMP_TIMEOUT_MS: u32 = 60_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_timing_constants_are_the_pinned_values() {
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

    /// The recovery delay is five seconds, pinned by name so that a change to it shows up in
    /// the test list rather than only in a diff.
    #[test]
    fn error_recovery_delay_is_five_seconds_not_two() {
        assert_eq!(ERROR_RECOVERY_DELAY_MS, 5_000);
    }
}
