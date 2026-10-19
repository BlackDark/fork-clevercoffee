//! The six display templates, and the stage order they share.
//!
//! # The stage order is the design, not an implementation detail
//!
//! The pipeline is fixed: **fullscreen modes -> system screens -> the template's
//! own layout**, with a per-template [`TemplatePolicy`] choosing which shared
//! stages run.
//!
//! The order is realised as a `match` on [`TemplateId`] in `render::dispatch`,
//! which costs no vtable and needs no trait. [`render()`] is the fixed stage order
//! and no template can reorder it, which is the property that matters.
//!
//! [`TemplatePolicy`] is public and `const` because the policy is *data*, and data
//! is what a test can assert on: `Standard`/`Scale`/`Upright` enable all four
//! shared stages, `Minimal` and `TemperatureOnly` enable none, and `Modern` opts
//! out of the shared heating logo because it draws its own `HEATING` row instead.
//! That asymmetry is the whole reason the policy exists, and a test checks it
//! rather than leaving it to a reviewer.

pub mod modern_layout;
pub mod render;

use crate::display::Display;
use crate::model::{BrewTimerState, Config, DisplayInput};

/// Which shared stages a template opts into.
///
/// One flag per shared stage, named for what it gates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// Four independent opt-outs, one per shared stage. They are only ever set
// together (`ALL`, `NONE` or `MODERN`), and grouping them into a `shared_stages`
// sub-struct would add a layer with nothing in it.
#[allow(
    clippy::struct_excessive_bools,
    reason = "one flag per shared stage; see the type's doc comment"
)]
pub struct TemplatePolicy {
    /// The fullscreen heating logo.
    pub shared_heating_logo: bool,
    /// The fullscreen brew timer.
    pub shared_fullscreen_brew_timer: bool,
    /// The fullscreen manual-flush timer.
    pub shared_fullscreen_manual_flush_timer: bool,
    /// The fullscreen hot-water timer.
    pub shared_fullscreen_hot_water_timer: bool,
}

impl TemplatePolicy {
    /// All four stages on: the policy Standard, Scale and Upright share.
    pub const ALL: Self = Self {
        shared_heating_logo: true,
        shared_fullscreen_brew_timer: true,
        shared_fullscreen_manual_flush_timer: true,
        shared_fullscreen_hot_water_timer: true,
    };

    /// No shared stages. `Minimal` and `TemperatureOnly`.
    pub const NONE: Self = Self {
        shared_heating_logo: false,
        shared_fullscreen_brew_timer: false,
        shared_fullscreen_manual_flush_timer: false,
        shared_fullscreen_hot_water_timer: false,
    };

    /// What `Modern` uses: no shared fullscreen stages and no shared heating logo,
    /// because it draws a `HEATING` row in its own idle layout.
    pub const MODERN: Self = Self {
        shared_heating_logo: false,
        ..Self::NONE
    };
}

/// One of the six templates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TemplateId {
    /// The default: status bar, thermometer, temperature, brew, PID, progress bar,
    /// maintenance footer.
    Standard,
    /// Standard plus weight and pressure.
    Scale,
    /// Status bar, temperature, brew, progress bar.
    Minimal,
    /// The big readout and nothing else.
    TemperatureOnly,
    /// The portrait layout, drawn in the rotated space.
    Upright,
    /// The fixed-width, stable-field layout.
    Modern,
}

impl TemplateId {
    /// Every template, for the golden sweep.
    pub const ALL: [TemplateId; 6] = [
        Self::Standard,
        Self::Scale,
        Self::Minimal,
        Self::TemperatureOnly,
        Self::Upright,
        Self::Modern,
    ];

    /// The integer `/api/parameters` reports for this template.
    ///
    /// The order is the reported order: the parity harness diffs the integer, so
    /// renumbering would show up as a parameter change.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Standard => 0,
            Self::Scale => 1,
            Self::Minimal => 2,
            Self::TemperatureOnly => 3,
            Self::Upright => 4,
            Self::Modern => 5,
        }
    }

    /// The policy this template uses.
    #[must_use]
    pub const fn policy(self) -> TemplatePolicy {
        match self {
            Self::Standard | Self::Scale | Self::Upright => TemplatePolicy::ALL,
            Self::Minimal | Self::TemperatureOnly => TemplatePolicy::NONE,
            Self::Modern => TemplatePolicy::MODERN,
        }
    }

    /// Whether this template renders in the portrait (R1/R3) space.
    #[must_use]
    pub const fn is_upright(self) -> bool {
        matches!(self, Self::Upright)
    }

    /// A stable name, for the golden file.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Scale => "scale",
            Self::Minimal => "minimal",
            Self::TemperatureOnly => "temperature_only",
            Self::Upright => "upright",
            Self::Modern => "modern",
        }
    }
}

/// Which stage of the pipeline produced the frame.
///
/// Returned alongside the framebuffer so a test can assert *which* stage ran,
/// which is the only way to check a policy: a screen that looks right for the
/// wrong reason is a bug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The OTA screen ran and owns the frame.
    Ota,
    /// The fullscreen brew timer.
    FullscreenBrew,
    /// The fullscreen manual-flush timer.
    FullscreenManualFlush,
    /// The fullscreen hot-water timer.
    FullscreenHotWater,
    /// The offline splash.
    Offline,
    /// A shared system screen.
    SystemScreen(SystemScreenId),
    /// The template's normal layout ran.
    Normal,
}

impl Stage {
    /// Whether the frame is flushed later, or immediately by the stage that drew
    /// it.
    ///
    /// Deferred stages leave the buffer for the caller to flush; the others flush
    /// from the stage that drew them.
    ///
    /// `docs/display/rendering.md` ("Buffer policy") is the table. Getting it wrong
    /// stalls the SSE stream.
    #[must_use]
    pub const fn is_deferred(self) -> bool {
        matches!(
            self,
            Self::FullscreenBrew
                | Self::FullscreenManualFlush
                | Self::FullscreenHotWater
                | Self::Normal
        )
    }
}

/// Which shared system screen ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemScreenId {
    /// The fullscreen heating logo.
    Heating,
    /// The "PID is disabled manually" screen.
    PidDisabled,
    /// The "Standby mode" screen.
    Standby,
    /// The steam screen.
    Steam,
    /// The water-tank-empty screen.
    WaterTankEmpty,
    /// The backflush screen.
    Backflush,
    /// The emergency-stop screen.
    EmergencyStop,
    /// The temperature-sensor-error screen.
    SensorError,
    /// The EEPROM-error screen.
    EepromError,
}

/// A rendered frame, and which stage produced it.
#[derive(Clone, Debug)]
pub struct Rendered {
    /// The framebuffer and drawing state at the end of the frame.
    pub display: Display,
    /// Which stage drew it.
    pub stage: Stage,
}

// The six templates are free functions reached by value through
// `render::dispatch`, not types: they share no code beyond the fixed stage order,
// hold no associated data, and there is nothing a caller could hold as
// `&dyn Template`. A trait here would look like an extension point without being
// one.
//
// A template is added by adding a `TemplateId` variant and a `match` arm in
// `render::dispatch`, and the compiler then names every place that is not
// exhaustive.

/// The fixed stage order.
///
/// The order is not negotiable. A fullscreen timer beats the system screens (a
/// steam screen during a hot-water timer would hide the timer); the offline
/// splash beats both (an offline device showing its last normal layout is
/// worse than saying so); and the OTA screen beats everything, because an
/// in-progress update has to be able to report its own failure at any time.
#[allow(
    clippy::too_many_lines,
    reason = "the stage order IS the function; splitting it would hide it"
)]
pub fn render(
    template: TemplateId,
    d: &mut Display,
    input: &DisplayInput,
    config: &Config,
) -> Rendered {
    let policy = template.policy();

    // Run per frame rather than once at boot -- see `Display::prepare_display`. It
    // has to happen before stage 0, since the OTA screen draws into the buffer it
    // just cleared.
    d.prepare_display(config.rotation());

    // Stage 0: OTA. Not a policy flag: it is always offered and declines
    // internally.
    if let Some(stage) = crate::ota::draw(d, input) {
        return Rendered {
            display: d.clone(),
            stage,
        };
    }

    // Stage 1: fullscreen modes, in the order the layout documents.
    if policy.shared_fullscreen_brew_timer {
        if let Some(stage) = crate::fullscreen::draw_brew_timer(d, input, config, template) {
            return Rendered {
                display: d.clone(),
                stage,
            };
        }
    }
    if policy.shared_fullscreen_manual_flush_timer {
        if let Some(stage) = crate::fullscreen::draw_manual_flush_timer(d, input, config, template)
        {
            return Rendered {
                display: d.clone(),
                stage,
            };
        }
    }
    if policy.shared_fullscreen_hot_water_timer {
        if let Some(stage) = crate::fullscreen::draw_hot_water_timer(d, input, config, template) {
            return Rendered {
                display: d.clone(),
                stage,
            };
        }
    }
    if let Some(stage) = crate::fullscreen::draw_offline(d, input) {
        return Rendered {
            display: d.clone(),
            stage,
        };
    }

    // Stage 2: shared system screens.
    if let Some(screen) = crate::system_screens::draw(d, input, config, template, policy) {
        return Rendered {
            display: d.clone(),
            stage: Stage::SystemScreen(screen),
        };
    }

    // Stage 3: the template's own layout.
    render::dispatch(template, d, input, config);
    Rendered {
        display: d.clone(),
        stage: Stage::Normal,
    }
}

/// The brew-timer FSM, stepped once per frame.
///
/// The step is explicit rather than a side effect of a predicate, so the render
/// path stays a pure function of the state and the transition is testable on its
/// own. `DisplayInput::brew_timer` carries the state between frames.
#[must_use]
pub fn step_brew_timer(input: &mut DisplayInput, config: &Config) -> BrewTimerState {
    match input.brew_timer {
        BrewTimerState::Idle => {
            if input.brew_active {
                input.brew_timer = BrewTimerState::Running;
            }
        }
        BrewTimerState::Running => {
            if !input.brew_active {
                input.brew_timer = BrewTimerState::PostBrew;
                input.brew_end_ms = input.now_ms;
            }
        }
        BrewTimerState::PostBrew => {
            // `display.post_brew_timer_duration` is bounded to [1, 60] by its
            // ParamDef, so the value is 1000..=60000 after the conversion to
            // milliseconds and the result is exact.
            #[allow(
                clippy::cast_sign_loss,
                reason = "duration is in seconds and positive; the ParamDef bounds it to [1, 60]"
            )]
            let duration_ms =
                crate::fmt::truncate_to_i32(config.post_brew_timer_duration_s * 1000.0) as u32;
            if input.now_ms.wrapping_sub(input.brew_end_ms) > duration_ms {
                input.brew_timer = BrewTimerState::Idle;
            }
        }
    }
    input.brew_timer
}

/// Whether the brew timer is showing anything at all.
///
/// The timer shows in any state other than `Idle`, which is why the post-brew
/// screen is a brew-timer state and not a separate one.
#[must_use]
pub const fn should_display_brew_timer(state: BrewTimerState) -> bool {
    matches!(state, BrewTimerState::Running | BrewTimerState::PostBrew)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_table_matches_the_stage_order() {
        // Modern: no shared fullscreen stages and no shared heating logo.
        assert_eq!(TemplateId::Modern.policy(), TemplatePolicy::MODERN);
        assert!(
            !TemplateId::Modern.policy().shared_heating_logo,
            "Modern draws its own HEATING row"
        );
        assert!(!TemplateId::Modern.policy().shared_fullscreen_brew_timer);

        // Minimal and TemperatureOnly: no shared stages.
        for id in [TemplateId::Minimal, TemplateId::TemperatureOnly] {
            assert_eq!(id.policy(), TemplatePolicy::NONE, "{}", id.name());
        }

        // Standard, Scale and Upright: all four.
        for id in [TemplateId::Standard, TemplateId::Scale, TemplateId::Upright] {
            assert_eq!(id.policy(), TemplatePolicy::ALL, "{}", id.name());
        }
    }

    #[test]
    fn only_the_upright_template_is_portrait() {
        assert!(TemplateId::Upright.is_upright());
        for id in TemplateId::ALL {
            assert_eq!(id.is_upright(), id == TemplateId::Upright, "{}", id.name());
        }
    }

    #[test]
    fn every_template_has_a_distinct_discriminant_and_name() {
        for a in TemplateId::ALL {
            for b in TemplateId::ALL {
                if a != b {
                    assert_ne!(a.as_u8(), b.as_u8(), "{a:?} and {b:?} share a discriminant");
                    assert_ne!(a.name(), b.name());
                }
            }
        }
        // The order is the reported integer.
        assert_eq!(TemplateId::Standard.as_u8(), 0);
        assert_eq!(TemplateId::Modern.as_u8(), 5);
    }

    #[test]
    fn the_buffer_policy_is_deferred_only_for_the_four_stages_that_say_so() {
        // `docs/display/rendering.md` "Buffer policy": the fullscreen timers
        // and the normal layout defer; the OTA, offline and system screens
        // flush immediately. Getting this wrong stalls the web UI.
        assert!(Stage::FullscreenBrew.is_deferred());
        assert!(Stage::FullscreenManualFlush.is_deferred());
        assert!(Stage::FullscreenHotWater.is_deferred());
        assert!(Stage::Normal.is_deferred());
        assert!(!Stage::Ota.is_deferred());
        assert!(!Stage::Offline.is_deferred());
        assert!(!Stage::SystemScreen(SystemScreenId::Steam).is_deferred());
    }

    #[test]
    fn the_brew_timer_fsm_walks_idle_running_post_brew_and_back() {
        let config = Config {
            post_brew_timer_duration_s: 10.0,
            ..Config::default()
        };
        let mut input = DisplayInput {
            brew_active: false,
            now_ms: 0,
            ..DisplayInput::default()
        };

        assert_eq!(
            step_brew_timer(&mut input, &config),
            BrewTimerState::Idle,
            "idle stays idle"
        );

        input.brew_active = true;
        assert_eq!(
            step_brew_timer(&mut input, &config),
            BrewTimerState::Running,
            "a brew starts it"
        );
        assert_eq!(
            step_brew_timer(&mut input, &config),
            BrewTimerState::Running,
            "and it stays running"
        );

        input.brew_active = false;
        input.now_ms = 5_000;
        assert_eq!(
            step_brew_timer(&mut input, &config),
            BrewTimerState::PostBrew
        );
        assert_eq!(input.brew_end_ms, 5_000, "the end time is stamped");
        assert!(
            should_display_brew_timer(BrewTimerState::PostBrew),
            "post-brew still shows a timer"
        );

        input.now_ms = 5_000 + 10_000;
        assert_eq!(
            step_brew_timer(&mut input, &config),
            BrewTimerState::PostBrew,
            "10.0 s is not > 10.0 s"
        );

        input.now_ms = 5_000 + 10_001;
        assert_eq!(
            step_brew_timer(&mut input, &config),
            BrewTimerState::Idle,
            "then it expires"
        );
        assert!(!should_display_brew_timer(BrewTimerState::Idle));
    }
}
