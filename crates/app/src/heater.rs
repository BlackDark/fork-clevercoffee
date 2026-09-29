//! The heater's power control: how the PID's number becomes a pin.
//!
//! Two halves, and keeping them apart is the point.
//!
//! - **This module is the policy** and it is pure: a 10 ms window, a duty from 0 to 1000, and the
//!   rules about what may energise the heater at all. It is host-tested.
//! - **The board crate is the mechanism**: an `esp-hal` PWM peripheral driving one pin.
//!
//! # Why hardware PWM and not the C++ software PWM
//!
//! The C++ firmware did the heater's power control in a 10 ms interrupt: read a duty, compare
//! against a counter, write a pin. That is defect D05's shape — an interrupt doing flash-adjacent
//! work on a machine with a network stack — and the architecture's own rule is that the only
//! interrupt-driven activity is arithmetic and a single register write. Hardware PWM removes the
//! interrupt entirely: the peripheral counts, and the duty is a register the control task writes
//! once a second when the PID samples.
//!
//! # The fail-safe
//!
//! The heater must not be at full power merely because nothing is driving its duty. A board whose
//! PWM could not be brought up holds the heater off and says so, rather than driving the pin from
//! the relay command, which is what the C++ did and what would give a 40 percent duty as 100
//! percent. [`stage::HeaterStage`] is that decision, made once at boot and testable.

use clevercoffee_domain::Timing;

/// The PWM period, from `Timing.h:16`.
pub const WINDOW_MS: u32 = Timing::HEATER_ISR_INTERVAL.as_millis() as u32;

/// The duty range: 0 to `Timing::HEATER_PWM_WINDOW`.
pub const WINDOW: u16 = Timing::HEATER_PWM_WINDOW as u16;

/// The PWM frequency that gives the 10 ms window.
pub const FREQUENCY_HZ: u32 = 1_000 / WINDOW_MS;

/// Converts a heater duty in permille to a PWM timestamp.
///
/// The maximum timestamp is one less than the window, so a duty of 1000 is the full period and a
/// duty of 1 is 0.1 percent rather than rounding to zero. A duty that rounds to zero must not be
/// sent as zero, because "one part in a thousand" and "off" are different answers and the first
/// is what the PID asked for.
pub const fn duty_to_timestamp(permille: u16) -> u16 {
    let clamped = if permille > WINDOW { WINDOW } else { permille };
    let max = WINDOW - 1;
    if clamped == 0 {
        return 0;
    }
    // `(clamped * max) / WINDOW` with a ceiling, so the smallest non-zero duty is one tick rather
    // than nothing.
    ((clamped as u32 * max as u32).div_ceil(WINDOW as u32)) as u16
}

/// The percentage the display shows, which is the duty the PID asked for and not what the
/// peripheral rounded it to.
pub const fn duty_to_percent(permille: u16) -> u8 {
    let clamped = if permille > WINDOW { WINDOW } else { permille };
    (clamped / 10) as u8
}

/// What the heater's power stage is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// Hardware PWM is driving the pin and the duty is what the PID asked for.
    Pwm,
    /// The PWM peripheral could not be brought up, so the heater is held **off**.
    ///
    /// Not "full power". A heater with no working power control that still heats is a machine
    /// that ignores its PID, and the C++ firmware had exactly that: `set_heater_duty` did nothing
    /// and the relay command drove the pin, so a 40 percent duty was 100 percent. Holding the
    /// heater off is a machine that does not heat, which is a machine the user notices.
    HeldOff,
    /// The machine has no heater fitted. The C6's reduced board, and a bench with the relay
    /// removed.
    Absent,
}

impl Stage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Stage::Pwm => "pwm",
            Stage::HeldOff => "held off: no power control",
            Stage::Absent => "absent",
        }
    }

    /// Whether the heater may be energised at all.
    pub const fn may_heat(self) -> bool {
        matches!(self, Stage::Pwm)
    }
}

/// The command the board applies, given the stage and the PID's duty.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Command {
    /// The pin level for the heater output.
    pub energised: bool,
    /// The PWM timestamp, when the stage is [`Stage::Pwm`].
    pub timestamp: u16,
}

impl Command {
    /// The only place a heater command is computed.
    ///
    /// Two independent conditions, both required: the stage must permit heating, and the machine
    /// must have commanded it. A duty of zero with the stage live is off, which is the normal case
    /// of a machine at setpoint.
    pub const fn for_duty(stage: Stage, energised: bool, permille: u16) -> Self {
        if stage.may_heat() && energised {
            Self {
                energised: true,
                timestamp: duty_to_timestamp(permille),
            }
        } else {
            Self {
                energised: false,
                timestamp: 0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_the_cpp_window() {
        assert_eq!(WINDOW_MS, 10);
        assert_eq!(FREQUENCY_HZ, 100, "100 Hz is a 10 ms period");
        assert_eq!(WINDOW, 1000);
    }

    #[test]
    fn a_duty_becomes_a_timestamp_in_range() {
        assert_eq!(duty_to_timestamp(0), 0);
        assert_eq!(
            duty_to_timestamp(1000),
            999,
            "full power is the full period"
        );
        assert_eq!(duty_to_timestamp(500), 500);
        for d in [1u16, 7, 100, 333, 999, 1000] {
            let t = duty_to_timestamp(d);
            assert!(t < WINDOW, "{d} mapped to {t}");
        }
    }

    #[test]
    fn a_duty_above_the_window_is_clamped_rather_than_wrapping() {
        assert_eq!(duty_to_timestamp(60_000), 999);
        assert_eq!(duty_to_percent(60_000), 100);
    }

    #[test]
    fn the_smallest_non_zero_duty_is_still_one_tick() {
        // One part in a thousand is not off, and a heater that is asked for 0.1 percent and gets
        // nothing is a PID that cannot control at the bottom of its range.
        assert_eq!(duty_to_timestamp(1), 1);
        assert!(duty_to_timestamp(1) > 0);
    }

    #[test]
    fn a_stage_with_no_power_control_holds_the_heater_off() {
        let c = Command::for_duty(Stage::HeldOff, true, 1000);
        assert!(
            !c.energised,
            "no power control means no heat, not full power"
        );
        assert_eq!(c.timestamp, 0);
    }

    #[test]
    fn a_live_stage_energises_only_when_asked() {
        assert!(Command::for_duty(Stage::Pwm, true, 500).energised);
        assert!(!Command::for_duty(Stage::Pwm, false, 500).energised);
        assert_eq!(Command::for_duty(Stage::Pwm, false, 500).timestamp, 0);
    }

    #[test]
    fn a_machine_with_no_heater_never_energises_one() {
        assert!(!Stage::Absent.may_heat());
        assert!(!Command::for_duty(Stage::Absent, true, 1000).energised);
    }

    #[test]
    fn the_percentage_is_the_duty_the_pid_asked_for() {
        assert_eq!(duty_to_percent(0), 0);
        assert_eq!(duty_to_percent(420), 42);
        assert_eq!(duty_to_percent(1000), 100);
        assert_eq!(duty_to_percent(99), 9, "9 percent is shown as 9, not 10");
    }

    #[test]
    fn the_display_percentage_never_shows_a_hundred_when_the_duty_is_not_one() {
        // The C++ printed `pidOutput / 10` with one decimal, which shows 100.0 at 999 and 100.0 at
        // 1000. Truncating is what the display does now, and this is the assertion that says so.
        for permille in [0u16, 1, 999, 1000] {
            let p = duty_to_percent(permille);
            assert!(p <= 100);
            if permille < 1000 {
                assert!(p < 100, "{permille} showed as {p}");
            }
        }
    }
}
