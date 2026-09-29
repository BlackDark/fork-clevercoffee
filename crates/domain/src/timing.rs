//! Timing constants, in one place.
//!
//! Values that came from the C++ source are reproduced exactly, because changing them changes how
//! the machine behaves and nothing in the port asked for that. Each cites the file it came from,
//! so a reader comparing the two trees can line them up and see which constants are a new
//! decision. Constants with no C++ counterpart are the ones the concurrency model added: the C++
//! had a single loop and no tasks.

use core::time::Duration;

/// Milliseconds.
pub const fn ms(value: u32) -> Duration {
    Duration::from_millis(value as u64)
}

pub struct Timing;

impl Timing {
    /// The heater PWM window and the ISR period. C++: `Timing.h:16` is 10 000 us, and
    /// `ProcessState.h:183` sets the window to 1000.
    pub const HEATER_ISR_INTERVAL: Duration = ms(10);
    pub const HEATER_PWM_WINDOW: u32 = 1000;

    /// The PID sample period. C++: `SystemInitializer.cpp:551` sets 1000 ms.
    pub const PID_SAMPLE: Duration = ms(1000);

    /// Sensor intervals. C++: `Timing.h:42-45`. The water tank period has no C++ constant; the
    /// C++ polled it from the temperature tick, so 200 ms is a new decision that only changes
    /// responsiveness.
    pub const TEMPERATURE: Duration = ms(400);
    pub const PRESSURE: Duration = ms(50);
    pub const SCALE: Duration = ms(100);
    pub const WATER_TANK: Duration = ms(200);

    /// The control task tick. 1 ms is the granularity of the state machine's own decisions; the
    /// state machine does not need to be faster than the fastest thing it waits for, which is the
    /// PID at 1 s and the sensor at 400 ms.
    pub const CONTROL_TICK: Duration = ms(1);

    /// Display. C++: `Timing.h:38` is 100 ms. The C++ auto-sleep is
    /// `CleverCoffee::Display::AUTO_SLEEP_MINUTES = 35` (`Timing.h:61`), which is 2 100 000 ms.
    /// The C++ `StandbyCoordinator` used 600 000 ms for a different timer, the display-off
    /// timeout, so reproducing 600 000 here would have been reproducing the wrong constant.
    pub const DISPLAY_RENDER: Duration = ms(100);
    pub const DISPLAY_AUTO_SLEEP: Duration = ms(2_100_000);

    /// How long a finished steam or hot-water dispense shows before returning. C++: `Timing.h:18-19`.
    pub const STEAM_STOPPED_DISPLAY: Duration = ms(2000);
    pub const HOT_WATER_STOPPED_DISPLAY: Duration = ms(2000);

    /// How long a finished brew or backflush stays on screen. C++: `Timing.h:69` and `Timing.h:84`,
    /// both 3 000 ms. These are configurable in the new firmware, so these are the C++ defaults.
    pub const BREW_FINISHED_DISPLAY: Duration = ms(3000);
    pub const BACKFLUSH_FINISHED_DISPLAY: Duration = ms(3000);

    /// Pre-infusion defaults. C++: `Timing.h:70-72` and `defaults.h:22-23`.
    pub const PRE_INFUSION: Duration = ms(2000);
    pub const PRE_INFUSION_PAUSE: Duration = ms(5000);

    /// Switch handling. C++: `IOSwitch.h:63-64`.
    pub const DEBOUNCE: Duration = ms(20);
    pub const LONG_PRESS: Duration = ms(500);

    /// The power switch is ignored for this long after boot, so plugging the machine in cannot
    /// toggle it. C++: `PowerHandler.h:118,143`.
    pub const POWER_SWITCH_BOOT_GUARD: Duration = ms(5000);
    /// C++: `PowerHandler.h:144`.
    pub const POWER_SWITCH_REBOOT_LONG_PRESS: Duration = ms(1000);

    /// The pump run-time limits. The C++ firmware declared these and never armed them, so a
    /// stuck switch ran the pump indefinitely (defect D09). Here the safety task enforces them.
    pub const BREW_PUMP_TIMEOUT: Duration = ms(300_000);
    pub const HOT_WATER_PUMP_TIMEOUT: Duration = ms(60_000);

    /// Error recovery. C++: `Timing.h:27-28`.
    pub const SENSOR_ERROR_RECOVERY: Duration = ms(5000);
    pub const EEPROM_RECOVERY_TIMEOUT: Duration = ms(300_000);

    /// The watchdogs. `SAFETY_FEED` is well inside `SAFETY_TIMEOUT`, so a hung safety task trips
    /// the watchdog while a merely slow one does not.
    pub const SAFETY_TIMEOUT: Duration = ms(5000);
    pub const SAFETY_FEED: Duration = ms(50);

    /// The task watchdog on the chip, as configured by `esp-rtos`.
    pub const TASK_WATCHDOG: Duration = ms(5000);

    /// Housekeeping. `HOMEASSISTANT_DISCOVERY` is C++ `Timing.h:45`; the rest are new.
    pub const LOGGER_HEARTBEAT: Duration = ms(30_000);
    pub const HOMEASSISTANT_DISCOVERY: Duration = ms(300_000);
    pub const STANDBY_COORDINATOR_TICK: Duration = ms(1000);

    /// The display geometry. C++: `Timing.h:56-57`.
    pub const OLED_WIDTH: u16 = 128;
    pub const OLED_HEIGHT: u16 = 64;
}

impl core::fmt::Debug for Timing {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Timing")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_isr_interval_divides_the_pwm_window_exactly() {
        // The software PWM in the ISR counts the window in ISR ticks. If the window is not a
        // whole number of ticks the duty cycle is quantised and the heater is not really at the
        // requested power.
        let isr_ms = Timing::HEATER_ISR_INTERVAL.as_millis();
        let window_ms = Timing::HEATER_PWM_WINDOW as u128;
        assert_eq!(
            window_ms % isr_ms,
            0,
            "the window must be a whole number of ISR ticks"
        );
    }

    #[test]
    fn the_safety_feed_is_far_inside_its_timeout() {
        // A safety task that is merely slow must not trip the watchdog. One that has hung must.
        assert!(Timing::SAFETY_FEED.as_millis() * 10 < Timing::SAFETY_TIMEOUT.as_millis());
    }

    #[test]
    fn the_debounce_is_long_enough_for_a_switch_and_short_enough_to_feel_immediate() {
        assert!(
            Timing::DEBOUNCE.as_millis() >= 5,
            "below 5 ms contacts bounce visibly"
        );
        assert!(
            Timing::DEBOUNCE.as_millis() <= 50,
            "above 50 ms the switch feels laggy"
        );
        assert!(
            Timing::LONG_PRESS.as_millis() > Timing::DEBOUNCE.as_millis(),
            "a long press must outlast a debounce, or it can never be detected"
        );
    }

    #[test]
    fn the_pump_timeouts_are_sane() {
        assert!(Timing::BREW_PUMP_TIMEOUT.as_secs() >= 60);
        assert!(Timing::HOT_WATER_PUMP_TIMEOUT.as_secs() >= 30);
        assert!(
            Timing::BREW_PUMP_TIMEOUT > Timing::HOT_WATER_PUMP_TIMEOUT,
            "a brew may legitimately run longer than a hot-water dispense"
        );
    }

    #[test]
    fn the_display_auto_sleep_is_thirty_five_minutes() {
        // C++ Timing.h:61, AUTO_SLEEP_MINUTES = 35.
        assert_eq!(Timing::DISPLAY_AUTO_SLEEP.as_secs(), 35 * 60);
    }

    #[test]
    fn the_preinfusion_defaults_match_the_cpp() {
        assert_eq!(Timing::PRE_INFUSION.as_millis(), 2_000);
        assert_eq!(Timing::PRE_INFUSION_PAUSE.as_millis(), 5_000);
    }

    #[test]
    fn the_pwm_window_is_ten_tenths() {
        // The heater output is 0 to 1000 and the display shows it as a percentage, so the
        // conversion is a plain divide by ten.
        assert_eq!(Timing::HEATER_PWM_WINDOW / 10, 100);
    }
}
