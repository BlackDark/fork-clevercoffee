//! What a template renders: one plain struct, filled in by the caller.
//!
//! The C++ templates reached into a `SystemContext` and read singletons while drawing, so a
//! render could observe a half-updated set of values and a template could not be rendered at all
//! without a running machine. Here a [`ScreenModel`] is a value: the app builds one per render
//! tick, and a test builds one directly. That is what makes "render every template in every state
//! and assert the pixels" possible on the host.

use crate::lang::Language;
use clevercoffee_domain::State;

/// Which of the six templates to draw. The names match the C++ classes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Template {
    /// Large temperature, a temperature-to-setpoint bar, no thermometer graphic.
    #[default]
    Modern,
    /// The vertical layout for a panel mounted upright, with one large status word.
    Upright,
    /// The classic layout: thermometer on the left, temperature, brew and PID rows, output bar.
    Standard,
    /// The classic layout without the thermometer graphic.
    Minimal,
    /// Weight and pressure instead of the PID row.
    Scale,
    /// Temperature and setpoint only.
    TemperatureOnly,
}

impl Template {
    /// Every template, so a test can iterate them all.
    pub const ALL: [Template; 6] = [
        Template::Modern,
        Template::Upright,
        Template::Standard,
        Template::Minimal,
        Template::Scale,
        Template::TemperatureOnly,
    ];

    /// The value the configuration stores.
    pub const fn as_u8(self) -> u8 {
        match self {
            Template::Modern => 0,
            Template::Upright => 1,
            Template::Standard => 2,
            Template::Minimal => 3,
            Template::Scale => 4,
            Template::TemperatureOnly => 5,
        }
    }

    /// Parses the stored value, falling back to Modern for an unknown one, as for languages.
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Template::Upright,
            2 => Template::Standard,
            3 => Template::Minimal,
            4 => Template::Scale,
            5 => Template::TemperatureOnly,
            _ => Template::Modern,
        }
    }
}

/// Which optional hardware is present, so a template can leave out a row it has no data for.
///
/// This is the C++ `hardwareSensorsScaleEnabled` / `hardwareSensorsPressureEnabled` pair, read at
/// render time in the C++ tree and resolved once at boot here.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Features {
    pub scale: bool,
    pub pressure: bool,
    /// Whether the brew switch exists at all. With it disabled the C++ templates drew no brew
    /// row, because there is no way to start a brew.
    pub brew_switch: bool,
}

/// Network state, for the status bar.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct NetStatus {
    pub wifi: bool,
    pub mqtt: bool,
    /// Offline mode takes over the whole panel, as it did in the C++ tree.
    pub offline: bool,
}

/// An OTA in progress. While this is `Some`, nothing else is drawn, which is the direct fix for
/// defect D01's screen half: the C++ OTA screen left the pump and valve energised behind it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OtaProgress {
    pub percent: u8,
    pub error: bool,
}

/// The three PID gains, as the display shows them.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct PidView {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    pub output_permille: u16,
}

/// The single value a template draws from.
#[derive(Clone, Copy, Debug)]
pub struct ScreenModel {
    pub template: Template,
    pub language: Language,
    pub state: State,
    /// `None` when the sensor is faulted. A template must show a fault, never a stale number:
    /// the fused filter in `domain::sensor` yields `None` rather than its last mean for exactly
    /// this reason, and a template that formatted `unwrap_or(0.0)` would reintroduce D03's shape.
    pub temperature_c: Option<f64>,
    pub setpoint_c: f64,
    pub pid: PidView,
    pub brew_elapsed_ms: u32,
    /// The total the brew is running towards, when brewing by time.
    pub brew_target_ms: Option<u32>,
    pub brew_by_weight: bool,
    pub weight_g: Option<f64>,
    pub target_weight_g: Option<f64>,
    pub scale_fault: bool,
    pub pressure_bar: Option<f64>,
    pub uptime_ms: u32,
    pub net: NetStatus,
    pub ota: Option<OtaProgress>,
    pub features: Features,
    pub backflush_cycle: u8,
    pub backflush_cycles: u8,
    /// How close the current temperature has to be to the setpoint for the machine to count as
    /// ready. C++ `Temperature::HEATING_LOGO_THRESHOLD_C`, 2 C.
    pub ready_threshold_c: f64,
}

impl Default for ScreenModel {
    fn default() -> Self {
        Self::idle()
    }
}

impl ScreenModel {
    /// A machine at rest with no sensors attached: the state a freshly flashed board is in.
    pub fn idle() -> Self {
        Self {
            template: Template::default(),
            language: Language::default(),
            state: State::PidNormal,
            temperature_c: Some(23.0),
            setpoint_c: 93.0,
            pid: PidView {
                kp: 10.0,
                ki: 0.1,
                kd: 100.0,
                output_permille: 0,
            },
            brew_elapsed_ms: 0,
            brew_target_ms: None,
            brew_by_weight: false,
            weight_g: None,
            target_weight_g: None,
            scale_fault: false,
            pressure_bar: None,
            uptime_ms: 0,
            net: NetStatus::default(),
            ota: None,
            features: Features::default(),
            backflush_cycle: 0,
            backflush_cycles: 3,
            ready_threshold_c: 2.0,
        }
    }

    /// Whether the machine is hot enough to brew, within the configured threshold.
    ///
    /// With no temperature at all this is `false`: an unreadable sensor is not "ready", and
    /// treating it as ready would put the heating logo on a faulted machine.
    pub fn is_ready(&self) -> bool {
        match self.temperature_c {
            Some(t) => t >= self.setpoint_c - self.ready_threshold_c,
            None => false,
        }
    }

    /// Whether a brew, a flush or a backflush is on screen, which is what the C++ helper
    /// `shouldDisplayBrewTimer` decided.
    pub const fn shows_brew_timer(&self) -> bool {
        matches!(
            self.state,
            State::BrewPreinfusion
                | State::BrewPreinfusionPause
                | State::BrewRunning
                | State::BrewFinished
        )
    }

    /// Whether the machine is in a backflush phase that owns the panel.
    pub const fn shows_backflush(&self) -> bool {
        matches!(
            self.state,
            State::BackflushFilling | State::BackflushFlushing | State::BackflushFinished
        )
    }

    /// The large word the upright template shows.
    pub fn upright_status(&self) -> &'static str {
        let s = self.language.strings();
        if matches!(self.state, State::ManualFlushRunning) {
            s.upright_flushing
        } else if self.shows_brew_timer() {
            s.upright_brewing
        } else if self.shows_backflush() {
            s.upright_clean
        } else if self.is_ready() {
            s.upright_ready
        } else {
            s.upright_waiting
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_list_is_the_one_the_configuration_documents() {
        assert_eq!(Template::ALL.len(), 6);
        let mut seen: heapless::Vec<Template, 6> = heapless::Vec::new();
        for t in Template::ALL {
            assert!(!seen.contains(&t), "{t:?} appears twice");
            let _ = seen.push(t);
            assert_eq!(Template::from_u8(t.as_u8()), t);
        }
    }

    #[test]
    fn ready_needs_a_reading_within_the_threshold() {
        let mut m = ScreenModel::idle();
        m.temperature_c = Some(92.0);
        m.setpoint_c = 93.0;
        assert!(m.is_ready(), "within 2 C counts as ready");
        m.temperature_c = Some(80.0);
        assert!(!m.is_ready());
        m.temperature_c = None;
        assert!(
            !m.is_ready(),
            "an unreadable sensor is not ready, and must not read as zero-hot"
        );
    }

    #[test]
    fn the_brew_timer_is_shown_for_the_four_brew_states() {
        let mut m = ScreenModel::idle();
        for s in [
            State::BrewPreinfusion,
            State::BrewPreinfusionPause,
            State::BrewRunning,
            State::BrewFinished,
        ] {
            m.state = s;
            assert!(m.shows_brew_timer(), "{s:?} should show the brew timer");
        }
        m.state = State::PidNormal;
        assert!(!m.shows_brew_timer());
    }

    #[test]
    fn the_upright_word_follows_the_state() {
        let mut m = ScreenModel::idle();
        m.temperature_c = Some(93.0);
        assert_eq!(m.upright_status(), "OK");
        m.state = State::BrewRunning;
        assert_eq!(m.upright_status(), "BREW");
        m.state = State::ManualFlushRunning;
        assert_eq!(m.upright_status(), "FLUSH");
        m.state = State::BackflushFilling;
        assert_eq!(m.upright_status(), "CLEAN");
        m.state = State::PidNormal;
        m.temperature_c = Some(30.0);
        assert_eq!(m.upright_status(), "WAIT");
    }
}
