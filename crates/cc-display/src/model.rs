//! The input a template reads, and the config it reads.
//!
//! The C++ templates read two things: `SystemContext` (a service locator over
//! the machine state, sensors, coordinators and the process state) and the
//! `Config` singleton. Neither exists on the host, and neither should be
//! reconstructed here — the display needs a *snapshot*, not the machine.
//!
//! So this is [`DisplayInput`]: a plain value holding exactly the fields the
//! display code reads, plus [`Config`], a plain value holding exactly the
//! parameters the display code reads. The device builds them from the real
//! `SystemContext`/`Config` in `cc-firmware`; the host tests build them
//! directly, which is what makes every screen golden-testable.
//!
//! Nothing here is a port of `SystemContext`. The mapping lives in
//! `cc-firmware`'s `build_display_input()`, so a field added to the machine and
//! forgotten here is a compile error at the call site rather than a silently
//! stale screen.

use cc_domain::state::MachineState;

/// The controller's brew mode (`Process::BrewMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrewMode {
    /// The operator presses the switch.
    Manual,
    /// The firmware stops on time or weight.
    Automatic,
}

/// Which physical scale is fitted (`Hardware::ScaleType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleType {
    /// A USB/Bluetooth scale read over BLE.
    Bluetooth,
    /// An HX711 load cell.
    Hx711,
}

/// The display language (`System::Language`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// English.
    English,
    /// German.
    German,
    /// Spanish.
    Spanish,
}

/// The OTA update phase (`OTA::Status`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtaStatus {
    /// Nothing in flight.
    Idle,
    /// Firmware or filesystem bytes are arriving.
    Uploading,
    /// The new image is being written.
    Processing,
    /// Done; the device is about to restart.
    Complete,
    /// Failed; the message is shown.
    Error,
}

/// What kind of image is being written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtaKind {
    /// The application image.
    Firmware,
    /// The `LittleFS` data partition.
    Filesystem,
}

/// The OTA screen's input (`DisplayOtaScreen.cpp`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OtaInput {
    /// Whether the OTA screen should be shown at all.
    pub show: bool,
    /// Where the update is.
    pub status: OtaStatus,
    /// What is being written.
    pub kind: OtaKind,
    /// 0..=100.
    pub progress: u8,
    /// The error text, when `status` is [`OtaStatus::Error`].
    pub error_message: &'static str,
}

impl Default for OtaInput {
    fn default() -> Self {
        Self {
            show: false,
            status: OtaStatus::Idle,
            kind: OtaKind::Firmware,
            progress: 0,
            error_message: "",
        }
    }
}

/// The brew-timer display FSM (`DisplayBrewTimerState.h`).
///
/// The C++ keeps this in `UICoordinator` and mutates it inside
/// `shouldDisplayBrewTimer()`, which is a *side effect in a predicate* — the
/// function both reads and advances the state. Ported as an explicit value so a
/// test can step it, and so the render path is a pure function of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrewTimerState {
    /// No brew is running; the screen shows the idle layout.
    #[default]
    Idle,
    /// A brew is in progress; the screen shows the brew layout.
    Running,
    /// The brew has finished; the post-brew screen is showing.
    PostBrew,
}

/// Everything a template reads to draw one frame.
///
/// Grouped the way the C++ reads it, so a reviewer can check the port against
/// the original call by call. The C++ reaches for these through
/// `systemContext_->processTemperature()` and friends; here they are fields.
#[derive(Clone, Copy, Debug, PartialEq)]
// A snapshot, not a behaviour: the 11 bools are independent facts about the
// machine at one instant, and grouping them into sub-structs would be a design
// that the C++ `SystemContext` does not have. The lint is about the risk of a
// confusing *constructor*; this type is only ever built by field name.
#[allow(
    clippy::struct_excessive_bools,
    reason = "a flat mirror of SystemContext's display reads; see the type's doc comment"
)]
pub struct DisplayInput {
    // ---- process state (SystemContext::processState)
    /// The measured temperature, degrees Celsius.
    pub temperature: f64,
    /// The target temperature, degrees Celsius.
    pub setpoint: f64,
    /// Heater output, 0..=1000.
    pub pid_output: f64,
    /// Milliseconds into the current brew.
    pub brew_time_ms: f64,
    /// The brew's target duration in milliseconds, or 0 for manual.
    pub target_brew_time_ms: f64,

    // ---- PID tuning (shown by the Standard template's PID row)
    /// Proportional gain.
    pub pid_kp: f64,
    /// Integral gain.
    pub pid_ki: f64,
    /// Derivative gain.
    pub pid_kd: f64,

    // ---- sensors
    /// Pump runtime for the hot-water timer, milliseconds.
    pub pump_on_time_ms: f64,
    /// Pump pressure, bar.
    pub pressure: f32,
    /// Weight in the portafilter during a brew, grams.
    pub brew_weight: f32,
    /// Weight on the scale, grams.
    pub weight: f32,
    /// Whether the scale has faulted.
    pub scale_fault: bool,

    // ---- machine state
    /// The current state.
    pub state: MachineState,

    // ---- animation
    /// The ISR counter. `isBlinkPhaseOn()` is `isr_counter < 500`, so a value
    /// under 500 is the "on" half of the blink.
    pub isr_counter: u32,

    // ---- network
    /// Whether the device is in offline fallback mode.
    pub offline: bool,
    /// Reconnect count, shown next to a down radio icon.
    pub wifi_reconnects: u32,
    /// Whether the radio is associated.
    pub wifi_connected: bool,
    /// Signal strength, 0..=4.
    pub wifi_signal: u8,
    /// Whether MQTT is enabled *and* connected.
    pub mqtt_connected: bool,
    /// Whether MQTT is weak enough to warrant the "!" suffix.
    pub mqtt_weak: bool,

    // ---- maintenance
    /// Whether the backflush reminder is due.
    pub backflush_reminder_due: bool,
    /// The backflush cycle counter, shown by the backflush system screen.
    pub backflush_cycle_count: u8,

    // ---- timers
    /// The brew-timer display FSM.
    pub brew_timer: BrewTimerState,
    /// Whether a brew is physically running (the FSM's input).
    pub brew_active: bool,
    /// Milliseconds since the brew ended (the FSM's clock, for post-brew).
    pub now_ms: u32,
    /// When the brew ended, for the post-brew FSM.
    pub brew_end_ms: u32,
    /// The offline-splash countdown, 0 = off.
    pub display_offline: u8,

    // ---- OTA
    /// The OTA screen's input.
    pub ota: OtaInput,
}

impl Default for DisplayInput {
    /// A plausible idle machine: PID on, 92.4 °C measured against a 94 °C
    /// setpoint, radio up, no scale, no sensors.
    fn default() -> Self {
        Self {
            temperature: 92.4,
            setpoint: 94.0,
            pid_output: 0.0,
            brew_time_ms: 0.0,
            target_brew_time_ms: 30_000.0,
            pid_kp: 18.0,
            pid_ki: 0.05,
            pid_kd: 150.0,
            pump_on_time_ms: 0.0,
            pressure: 0.0,
            brew_weight: 0.0,
            weight: 0.0,
            scale_fault: false,
            state: MachineState::PidNormal,
            isr_counter: 0,
            offline: false,
            wifi_reconnects: 0,
            wifi_connected: true,
            wifi_signal: 3,
            mqtt_connected: true,
            mqtt_weak: false,
            backflush_reminder_due: false,
            backflush_cycle_count: 0,
            brew_timer: BrewTimerState::Idle,
            brew_active: false,
            now_ms: 0,
            brew_end_ms: 0,
            display_offline: 0,
            ota: OtaInput::default(),
        }
    }
}

/// The configuration parameters the display reads.
///
/// Every field here corresponds to a `Config::getInstance()....get()` in
/// `display/`, so the set is closed and auditable. The defaults are the C++
/// `ParamDef` defaults, so an unconfigured host render shows the same screen a
/// freshly-flashed device does.
#[derive(Clone, Copy, Debug, PartialEq)]
// A flat mirror of the `Config` parameters the display reads. Grouping the 15
// feature flags into sub-structs would be a design the C++ `Config` does not
// have, and would mean the device's mapping code translates rather than copies.
#[allow(
    clippy::struct_excessive_bools,
    reason = "a flat mirror of Config's feature flags; see the type's doc comment"
)]
pub struct Config {
    /// `hardwareSwitchesBrewEnabled`
    pub brew_switch_enabled: bool,
    /// `hardwareSensorsScaleEnabled`
    pub scale_enabled: bool,
    /// `hardwareSensorsScaleType`
    pub scale_type: ScaleType,
    /// `hardwareSensorsPressureEnabled`
    pub pressure_enabled: bool,
    /// `hardwareOledEnabled`
    pub oled_enabled: bool,
    /// `displayTemplate`
    pub upright_template: bool,
    /// `displayInverted`
    pub inverted: bool,
    /// `displayLanguage`
    pub language: Language,
    /// `displayHeatingLogo` — 0 disables the fullscreen heating logo.
    pub heating_logo: u8,
    /// `displayPidOffLogo` — 1 enables the PID-off / standby logo screens.
    pub pid_off_logo: u8,
    /// `displayFullscreenBrewTimer`
    pub fullscreen_brew_timer: bool,
    /// `displayFullscreenManualFlushTimer`
    pub fullscreen_manual_flush_timer: bool,
    /// `displayFullscreenHotWaterTimer`
    pub fullscreen_hot_water_timer: bool,
    /// `displayPostBrewTimerDuration`, seconds.
    pub post_brew_timer_duration_s: f64,
    /// `displayBlinkingDelta`, degrees.
    pub blinking_delta: f64,
    /// `maintenanceBackflushReminderEnabled`
    pub backflush_reminder_enabled: bool,
    /// `brewMode`
    pub brew_mode: BrewMode,
    /// `brewByTimeEnabled`
    pub brew_by_time_enabled: bool,
    /// `brewByWeightEnabled`
    pub brew_by_weight_enabled: bool,
    /// `brewByWeightTargetWeight`, grams.
    pub brew_by_weight_target: f64,
    /// `mqttEnabled`
    pub mqtt_enabled: bool,
    /// `backflushCycles`
    pub backflush_cycles: u8,
}

impl Default for Config {
    /// The C++ `ParamDef` defaults, so an unconfigured render matches a
    /// freshly flashed device. `displayTemplate` defaults to Standard, so
    /// `upright_template` is false; `displayInverted` defaults to false.
    fn default() -> Self {
        Self {
            brew_switch_enabled: true,
            scale_enabled: false,
            scale_type: ScaleType::Hx711,
            pressure_enabled: false,
            oled_enabled: true,
            upright_template: false,
            inverted: false,
            language: Language::English,
            heating_logo: 1,
            pid_off_logo: 0,
            fullscreen_brew_timer: true,
            fullscreen_manual_flush_timer: true,
            fullscreen_hot_water_timer: true,
            post_brew_timer_duration_s: 10.0,
            blinking_delta: 0.3,
            backflush_reminder_enabled: true,
            brew_mode: BrewMode::Manual,
            brew_by_time_enabled: false,
            brew_by_weight_enabled: false,
            brew_by_weight_target: 36.0,
            mqtt_enabled: false,
            backflush_cycles: 5,
        }
    }
}

impl Config {
    /// The rotation `OledDriver::prepareDisplay` computes.
    ///
    /// `src/ui/OledDriver.cpp:66-79`:
    /// ```cpp
    /// int rotation = 0;
    /// if (displayInverted) rotation += 2;
    /// if (displayTemplate == UPRIGHT) rotation++;
    /// ```
    #[must_use]
    pub fn rotation(&self) -> crate::display::Rotation {
        let mut rotation = 0;
        if self.inverted {
            rotation += 2;
        }
        if self.upright_template {
            rotation += 1;
        }
        crate::display::Rotation::from_index(rotation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::Rotation;

    #[test]
    fn the_default_rotation_is_r0() {
        assert_eq!(Config::default().rotation(), Rotation::R0);
    }

    #[test]
    fn rotation_follows_the_oled_driver_rule() {
        // The four combinations of the two config flags, and the U8G2_Rn each
        // maps to. This is the mapping the Upright template's portrait
        // coordinates depend on.
        let cases = [
            (false, false, Rotation::R0),
            (false, true, Rotation::R1),
            (true, false, Rotation::R2),
            (true, true, Rotation::R3),
        ];
        for (inverted, upright, expected) in cases {
            let c = Config {
                inverted,
                upright_template: upright,
                ..Config::default()
            };
            assert_eq!(
                c.rotation(),
                expected,
                "inverted={inverted} upright={upright}"
            );
        }
    }

    #[test]
    fn only_the_upright_template_is_portrait() {
        assert!(!Rotation::R0.is_portrait());
        assert!(Rotation::R1.is_portrait());
        assert!(!Rotation::R2.is_portrait());
        assert!(Rotation::R3.is_portrait());
    }
}
