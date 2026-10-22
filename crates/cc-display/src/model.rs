//! Value snapshots for rendering, without access to live machine state.
//!
//! Firmware maps observables into [`DisplayInput`] and display settings into
//! [`Config`]; host tests construct the same values directly.

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

/// The OTA screen's input.
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

/// The brew-timer display FSM.
///
/// An explicit value rather than state hidden in a predicate, so a test can step
/// it and the render path is a pure function of it. A predicate that both reports
/// the state and advances it cannot be tested without a machine.
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
/// Flat fields rather than grouped sub-structs, because each one is a separate
/// fact about the machine at one instant and the render code reads them
/// individually.
#[derive(Clone, Copy, Debug, PartialEq)]
// A snapshot, not a behaviour: the flags are independent facts about the machine
// at one instant, and the lint is about the risk of a confusing *constructor*;
// this type is only ever built by field name.
#[allow(
    clippy::struct_excessive_bools,
    reason = "a flat snapshot of the machine's display reads; see the type's doc comment"
)]
pub struct DisplayInput {
    // ---- process state
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
    /// The ISR counter. A value under 500 is the "on" half of the blink.
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
/// Exactly the parameters the display reads, so the set is closed and auditable.
/// The defaults are the schema's, so an unconfigured host render shows the same
/// screen a freshly-flashed device does.
#[derive(Clone, Copy, Debug, PartialEq)]
// A flat copy of the parameters the display reads. Grouping the feature flags
// into sub-structs would make the device's mapping code translate rather than
// copy, which is where a field would get missed.
#[allow(
    clippy::struct_excessive_bools,
    reason = "a flat copy of the display's feature flags; see the type's doc comment"
)]
pub struct Config {
    /// Whether the brew switch is fitted and enabled.
    pub brew_switch_enabled: bool,
    /// Whether the scale is fitted and enabled.
    pub scale_enabled: bool,
    /// Which scale the machine has.
    pub scale_type: ScaleType,
    /// Whether the pressure sensor is fitted and enabled.
    pub pressure_enabled: bool,
    /// Whether the OLED is fitted and enabled.
    pub oled_enabled: bool,
    /// Whether the selected template draws in the portrait space.
    pub upright_template: bool,
    /// Whether the panel is mounted upside down.
    pub inverted: bool,
    /// The interface language.
    pub language: Language,
    /// 0 disables the fullscreen heating logo.
    pub heating_logo: u8,
    /// 1 enables the PID-off / standby logo screens.
    pub pid_off_logo: u8,
    /// Whether the fullscreen brew timer is enabled.
    pub fullscreen_brew_timer: bool,
    /// Whether the fullscreen manual-flush timer is enabled.
    pub fullscreen_manual_flush_timer: bool,
    /// Whether the fullscreen hot-water timer is enabled.
    pub fullscreen_hot_water_timer: bool,
    /// How long the post-brew timer shows, in seconds.
    pub post_brew_timer_duration_s: f64,
    /// The setpoint tolerance that blinks the readout, in degrees.
    pub blinking_delta: f64,
    /// Whether the backflush reminder is enabled.
    pub backflush_reminder_enabled: bool,
    /// The brew mode the machine is in.
    pub brew_mode: BrewMode,
    /// Whether the by-time target is in use.
    pub brew_by_time_enabled: bool,
    /// Whether the by-weight target is in use.
    pub brew_by_weight_enabled: bool,
    /// The by-weight target, in grams.
    pub brew_by_weight_target: f64,
    /// Whether MQTT is enabled.
    pub mqtt_enabled: bool,
    /// How many backflush cycles run.
    pub backflush_cycles: u8,
}

impl Default for Config {
    /// The schema's defaults, so an unconfigured render matches a freshly flashed
    /// device. The upright template and the inverted display are both off by
    /// default.
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
            // These five are the schema's defaults, and they matter because this
            // `Default` is what every **host test** renders, including
            // `screen_matrix.rs`'s "no features" config: when they were wrong, the
            // "everything off" case came up with all three fullscreen timers on,
            // which is how a default-on parity bug hid inside the checker that was
            // supposed to find it.
            pid_off_logo: 1,
            fullscreen_brew_timer: false,
            fullscreen_manual_flush_timer: false,
            fullscreen_hot_water_timer: false,
            post_brew_timer_duration_s: 3.0,
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
    /// The rotation the panel needs: +2 for an inverted display, +1 for the
    /// upright template, so both add and the result is taken modulo the four
    /// quarter turns.
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
