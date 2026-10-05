//! Port of `include/clevercoffee/display/displayHelpers.h`.
//!
//! Small, pure, and shared between the display and the status LEDs. ADR-0001 §3
//! made these the single source of truth for the tolerance thresholds, and the
//! two `isNearSetpoint*` overloads differ by exactly one comparison operator on
//! purpose:
//!
//! * [`is_near_setpoint_for_display`] is **strict `<`**, for the OLED blink.
//!   A value exactly `delta` away is *not* near, so the readout blinks at the
//!   boundary.
//! * [`is_near_setpoint_for_led`] is **`<=`**, and uses a wider tolerance in a
//!   steam state. A value exactly at the boundary *is* near, so the LED settles.
//!
//! That asymmetry is not an oversight and is asserted in the tests below; the
//! C++ comments at `displayHelpers.h:44-52` call it out as the reason the
//! helpers exist at all.
//!
//! [`is_heating_logo_condition_met`] is the third: it is `> HEATING_LOGO_THRESHOLD_C`
//! (5 °C) below setpoint, and it is the *only* place that constant is applied.
//! It additionally requires the PID to be in `PID_NORMAL` and
//! `displayHeatingLogo` to be enabled, because during a brew the temperature is
//! *supposed* to be below setpoint and the logo would otherwise take over the
//! screen.

use cc_domain::state::MachineState;

use crate::model::{Config, DisplayInput};

/// `Temperature::HEATING_LOGO_THRESHOLD_C`, 5 °C.
pub const HEATING_LOGO_THRESHOLD_C: f32 = 5.0;

/// `Temperature::TEMP_TOLERANCE_STEAM_C`, 5 °C.
///
/// The same number as the heating threshold, but a different concept: this is
/// how close the steam setpoint has to be before the LED is allowed to settle.
pub const TEMP_TOLERANCE_STEAM_C: f32 = 5.0;

/// `isManualFlushState` (`utils/SystemUtils.h`).
///
/// The C++ helper is `state == MANUAL_FLUSH_RUNNING`, and a handful of call
/// sites additionally test for it. Named here so a fourth state cannot be
/// added without this being revisited.
#[must_use]
pub const fn is_manual_flush_state(state: MachineState) -> bool {
    matches!(state, MachineState::ManualFlushRunning)
}

/// `isSteamState` (`utils/SystemUtils.h`).
#[must_use]
pub const fn is_steam_state(state: MachineState) -> bool {
    matches!(state, MachineState::SteamRunning)
}

/// `isBackflushState` (`utils/SystemUtils.h`).
#[must_use]
pub const fn is_backflush_state(state: MachineState) -> bool {
    matches!(
        state,
        MachineState::BackflushIdle
            | MachineState::BackflushFilling
            | MachineState::BackflushFlushing
            | MachineState::BackflushFinished
    )
}

/// `isNearSetpointForDisplay(temperature, setpoint, delta)` — **strict**.
///
/// `std::fabs(temperature - setpoint) < delta`.
#[must_use]
pub fn is_near_setpoint_for_display(temperature: f64, setpoint: f64, delta: f64) -> bool {
    (temperature - setpoint).abs() < delta
}

/// `isNearSetpointForDisplay(temperature, setpoint)` — the config-driven form.
#[must_use]
pub fn is_near_setpoint_with_config(temperature: f64, setpoint: f64, config: &Config) -> bool {
    is_near_setpoint_for_display(temperature, setpoint, config.blinking_delta)
}

/// `getStatusLedTolerance` — steam gets the wider tolerance.
///
/// Takes `blinking_delta` rather than the whole [`Config`] because that is the
/// only field it reads, and the LED rule is evaluated on every 10 ms control
/// tick: passing a ~40-field struct to read one `f64` out of it would be a
/// 100 Hz copy of the entire display configuration to obtain a single number.
/// `displayConfig` is what supplies it at the call site.
#[must_use]
pub fn status_led_tolerance(state: MachineState, blinking_delta: f64) -> f64 {
    if is_steam_state(state) {
        f64::from(TEMP_TOLERANCE_STEAM_C)
    } else {
        blinking_delta
    }
}

/// `isNearSetpointForStatusLed(temperature, setpoint, tolerance)` — **`<=`**.
#[must_use]
pub fn is_near_setpoint_for_led(temperature: f64, setpoint: f64, tolerance: f64) -> bool {
    (temperature - setpoint).abs() <= tolerance
}

/// `isBlinkPhaseOn` — `isrCounter() < 500`.
///
/// The ISR counter runs 0..999, so this is the first half of a 1 s cycle at
/// 1 kHz. Every blink in the display uses it, so all of them are in phase.
#[must_use]
pub const fn is_blink_phase_on(input: &DisplayInput) -> bool {
    input.isr_counter < 500
}

/// `isHeatingLogoConditionMet` — more than 5 °C below setpoint, PID normal, and
/// the feature enabled.
#[must_use]
pub fn is_heating_logo_condition_met(input: &DisplayInput, config: &Config) -> bool {
    if config.heating_logo == 0 {
        return false;
    }
    if input.state != MachineState::PidNormal {
        return false;
    }
    input.setpoint - input.temperature > f64::from(HEATING_LOGO_THRESHOLD_C)
}

/// `getCurrentDisplayState` — the state, or `INIT` when there is none.
#[must_use]
pub const fn current_display_state(input: &DisplayInput) -> MachineState {
    input.state
}

/// `shouldDisplayHotWaterTimer` (`DisplayFullscreenModes.h:31`).
///
/// True while the pump is running *and* the machine is in a state where the
/// water is going somewhere hot: PID normal (hot water into the group) or steam.
#[must_use]
pub fn should_display_hot_water_timer(input: &DisplayInput) -> bool {
    input.pump_on_time_ms > 0.0
        && matches!(
            input.state,
            MachineState::PidNormal | MachineState::SteamRunning
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_domain::state::MachineState;

    #[test]
    fn the_display_comparison_is_strict_and_the_led_one_is_not() {
        // ADR-0001 §3. The two differ by one operator, on purpose.
        let delta = 0.3;
        assert!(is_near_setpoint_for_display(94.71, 95.0, delta), "inside");
        assert!(!is_near_setpoint_for_display(94.69, 95.0, delta), "outside");
        assert!(
            is_near_setpoint_for_display(95.0, 95.0, delta),
            "equal is near"
        );

        assert!(is_near_setpoint_for_led(95.0, 95.0, delta), "equal is near");
        assert!(is_near_setpoint_for_led(94.7, 95.0, delta), "inside");
        assert!(!is_near_setpoint_for_led(94.69, 95.0, delta), "outside");
    }

    #[test]
    fn the_boundary_is_where_the_two_disagree() {
        // The single value the C++ comparator difference shows up on. If this
        // ever stops failing, one of the two has been "fixed" and the blink and
        // the LED have desynchronised at the boundary.
        //
        // The delta is 0.25 rather than the configured 0.3 because `95.0 - 0.3`
        // is not representable in binary floating point: the subtraction lands
        // one ulp below 0.3, so both comparators agree and the test would be
        // measuring rounding, not the operator.
        let delta: f64 = 0.25;
        let at = 95.0 - delta;
        // `95.0 - 94.75` is 0.25 exactly in binary; the assertion is here so
        // a change of `delta` that reintroduces the rounding problem is caught.
        assert!(
            (95.0 - at - delta).abs() < f64::EPSILON,
            "the boundary must be exact"
        );
        assert!(
            !is_near_setpoint_for_display(at, 95.0, delta),
            "display is strict"
        );
        assert!(
            is_near_setpoint_for_led(at, 95.0, delta),
            "LED is inclusive"
        );
    }

    #[test]
    fn steam_uses_the_wider_tolerance() {
        let delta = Config::default().blinking_delta;
        let steam = status_led_tolerance(MachineState::SteamRunning, delta);
        let normal = status_led_tolerance(MachineState::PidNormal, delta);
        assert!(
            (steam - 5.0).abs() < f64::EPSILON,
            "steam tolerance is 5 C, got {steam}"
        );
        assert!(
            (normal - delta).abs() < f64::EPSILON,
            "normal tolerance is the blink delta"
        );
    }

    #[test]
    fn the_heating_logo_needs_the_pid_normal_and_the_feature() {
        let mut input = DisplayInput {
            temperature: 80.0,
            setpoint: 95.0,
            ..DisplayInput::default()
        };
        let config = Config::default();
        assert!(
            is_heating_logo_condition_met(&input, &config),
            "15 C below setpoint"
        );

        input.state = MachineState::BrewRunning;
        assert!(
            !is_heating_logo_condition_met(&input, &config),
            "not PID_NORMAL"
        );

        input.state = MachineState::PidNormal;
        let off = Config {
            heating_logo: 0,
            ..Config::default()
        };
        assert!(
            !is_heating_logo_condition_met(&input, &off),
            "feature disabled"
        );
    }

    #[test]
    fn the_heating_logo_threshold_is_strict() {
        // `setpoint - temperature > 5`, so exactly 5 C below is NOT enough.
        let at_threshold = DisplayInput {
            temperature: 90.0,
            setpoint: 95.0,
            ..DisplayInput::default()
        };
        assert!(!is_heating_logo_condition_met(
            &at_threshold,
            &Config::default()
        ));
        let just_past = DisplayInput {
            temperature: 89.9,
            setpoint: 95.0,
            ..DisplayInput::default()
        };
        assert!(is_heating_logo_condition_met(
            &just_past,
            &Config::default()
        ));
    }

    #[test]
    fn the_blink_phase_is_the_first_half_of_the_counter() {
        for (counter, expected) in [(0u32, true), (499, true), (500, false), (999, false)] {
            let input = DisplayInput {
                isr_counter: counter,
                ..DisplayInput::default()
            };
            assert_eq!(
                is_blink_phase_on(&input),
                expected,
                "isr_counter = {counter}"
            );
        }
    }

    #[test]
    fn the_hot_water_timer_needs_both_the_pump_and_a_hot_state() {
        let mut input = DisplayInput {
            pump_on_time_ms: 1000.0,
            ..DisplayInput::default()
        };
        assert!(
            should_display_hot_water_timer(&input),
            "PID normal, pump on"
        );
        input.state = MachineState::SteamRunning;
        assert!(should_display_hot_water_timer(&input), "steam, pump on");
        input.state = MachineState::BrewRunning;
        assert!(
            !should_display_hot_water_timer(&input),
            "brewing is not hot water"
        );
        input.state = MachineState::PidNormal;
        input.pump_on_time_ms = 0.0;
        assert!(!should_display_hot_water_timer(&input), "pump not running");
    }
}
