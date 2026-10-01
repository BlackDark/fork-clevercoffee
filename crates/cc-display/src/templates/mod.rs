//! The six display templates, and the CRTP stage order they share.
//!
//! # The stage order is the design, not an implementation detail
//!
//! ADR-0001 §1 fixes the pipeline: **fullscreen modes -> system screens ->
//! the template's own layout**, with a per-template [`TemplatePolicy`] choosing
//! which shared stages run. The C++ realises that with CRTP
//! (`DisplayTemplateBase<Derived>`) and a `DisplayPolicy` type alias per
//! template; there is no virtual override table.
//!
//! The port keeps the *behaviour* and drops the *mechanism*. CRTP exists in C++
//! to get static dispatch without a vtable on a small MCU; in Rust the same
//! effect comes from a trait, and "no virtual table" is free. So [`Template`] is
//! a plain trait and [`render`] is the fixed stage order, with no way for a
//! template to reorder it — which is the property CRTP was buying.
//!
//! [`TemplatePolicy`] is public and `const` because the policy is *data*, and
//! data is what a test can assert on: `Standard`/`Scale`/`Upright` enable all
//! four shared stages, `Minimal` and `TemperatureOnly` enable none, and `Modern`
//! opts out of the shared heating logo because it draws its own `HEATING` row
//! instead. That asymmetry is the whole reason the policy exists, and it is
//! checked by `the_policy_table_matches_the_cplusplus` rather than left to a
//! reviewer.

pub mod modern_layout;
pub mod render;

use crate::display::Display;
use crate::model::{BrewTimerState, Config, DisplayInput};

/// Which shared stages a template opts into.
///
/// The four flags are the C++ `DisplayPolicy<SharedHeatingLogo,
/// SharedFullscreenBrew, SharedFullscreenManualFlush, SharedFullscreenHotWater>`
/// template parameters, renamed for what they gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// Four independent opt-outs, one per shared stage. Grouping them into a
// "shared_stages: Stages" sub-struct would be a design the C++
// `DisplayPolicy<...>` template parameters do not have, and the four flags are
// only ever set together (as `ALL`, `NONE` or `MODERN`).
#[allow(
    clippy::struct_excessive_bools,
    reason = "one flag per shared stage; see the type's doc comment"
)]
pub struct TemplatePolicy {
    /// `DisplayPolicy::sharedHeatingLogoScreen` — the fullscreen heating logo.
    pub shared_heating_logo: bool,
    /// `DisplayPolicy::sharedFullscreenBrewTimer`.
    pub shared_fullscreen_brew_timer: bool,
    /// `DisplayPolicy::sharedFullscreenManualFlushTimer`.
    pub shared_fullscreen_manual_flush_timer: bool,
    /// `DisplayPolicy::sharedFullscreenHotWaterTimer`.
    pub shared_fullscreen_hot_water_timer: bool,
}

impl TemplatePolicy {
    /// `DefaultDisplayPolicy` — all four on. Used by Standard, Scale, Upright.
    pub const ALL: Self = Self {
        shared_heating_logo: true,
        shared_fullscreen_brew_timer: true,
        shared_fullscreen_manual_flush_timer: true,
        shared_fullscreen_hot_water_timer: true,
    };

    /// `DisplayPolicy<false>`: no shared stages. `Minimal` and `TemperatureOnly`.
    pub const NONE: Self = Self {
        shared_heating_logo: false,
        shared_fullscreen_brew_timer: false,
        shared_fullscreen_manual_flush_timer: false,
        shared_fullscreen_hot_water_timer: false,
    };

    /// What `ModernTemplate` uses: no shared fullscreen stages and no shared
    /// heating logo, because it draws a `HEATING` row in its own idle layout.
    pub const MODERN: Self = Self {
        shared_heating_logo: false,
        ..Self::NONE
    };
}

/// One of the six templates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TemplateId {
    /// `StandardTemplate` — the default: status bar, thermometer, temperature,
    /// brew, PID, progress bar, maintenance footer.
    Standard,
    /// `ScaleTemplate` — Standard plus weight and pressure.
    Scale,
    /// `MinimalTemplate` — status bar, temperature, brew, progress bar.
    Minimal,
    /// `TemperatureOnlyTemplate` — the big readout and nothing else.
    TemperatureOnly,
    /// `UprightTemplate` — the portrait layout, drawn in the R1/R3 space.
    Upright,
    /// `ModernTemplate` — the fixed-width, stable-field layout.
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

    /// The C++ `System::DisplayTemplate` discriminant.
    ///
    /// The order is the C++ enum's, because `/api/parameters` reports the
    /// template as an integer and the parity harness diffs that.
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
    /// The OTA screen ran and owns the frame (`drawOtaScreen`).
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
    /// The template's own `renderNormalDisplay`.
    Normal,
}

impl Stage {
    /// Whether the frame is flushed later, or immediately by the stage that drew
    /// it.
    ///
    /// The C++ sets `displayBufferReady` differently per stage, and
    /// `LoopManager::updateDisplay` only flushes when the flag is set
    /// (`docs/display-architecture.md`, "Buffer policy"). Getting this wrong
    /// stalls the SSE stream, which is the failure mode ADR-0001 §7 fixed.
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

/// A display template.
///
/// One method, [`Template::render_normal`], mirroring the C++'s
/// `renderNormalDisplay()`. It is the *last* stage of [`render`] and the only
/// one a template controls.
pub trait Template {
    /// Which template this is.
    const ID: TemplateId;

    /// Draw the template's own layout.
    ///
    /// Every C++ implementation starts with `clearBuffer()`. Kept here rather
    /// than in [`render`] because the fullscreen and system screens also clear,
    /// and clearing in the wrong place is the bug; clearing twice is harmless.
    fn render_normal(&self, d: &mut Display, input: &DisplayInput, config: &Config);
}

/// The fixed stage order, from ADR-0001 §1.
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

    // `OledDriver::prepareDisplay`, run per frame here rather than once at boot
    // -- see `Display::prepare_display`. It has to happen before stage 0, since
    // the OTA screen draws into the buffer it just cleared.
    d.prepare_display(config.rotation());

    // Stage 0: OTA. Not a policy flag: `printScreen()` calls `drawOtaScreen`
    // unconditionally and it declines internally.
    if let Some(stage) = crate::ota::draw(d, input) {
        return Rendered {
            display: d.clone(),
            stage,
        };
    }

    // Stage 1: fullscreen modes, in the C++'s order.
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

/// The brew-timer FSM, as the C++ mutates it inside `shouldDisplayBrewTimer`.
///
/// ADR-0001 §4 made this one FSM in `UICoordinator`. The C++ advances it as a
/// side effect of a predicate; the port makes the step explicit so the render
/// path is a pure function of the state and the transition is testable on its
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
            // `static_cast<uint32_t>(duration * 1000)`. The config field is
            // bounded to [1, 60] by `displayPostBrewTimerDuration`'s ParamDef,
            // so the value is 1000..=60000 and the conversion is exact.
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
/// `shouldDisplayBrewTimer` returns `state != Idle`, which is why the post-brew
/// screen is a brew-timer state and not a separate one.
#[must_use]
pub const fn should_display_brew_timer(state: BrewTimerState) -> bool {
    matches!(state, BrewTimerState::Running | BrewTimerState::PostBrew)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_table_matches_the_cplusplus() {
        // ModernTemplate: `DisplayPolicy<false, false>`.
        assert_eq!(TemplateId::Modern.policy(), TemplatePolicy::MODERN);
        assert!(
            !TemplateId::Modern.policy().shared_heating_logo,
            "Modern draws its own HEATING row"
        );
        assert!(!TemplateId::Modern.policy().shared_fullscreen_brew_timer);

        // MinimalTemplate and TemperatureOnlyTemplate: `DisplayPolicy<false>`.
        for id in [TemplateId::Minimal, TemplateId::TemperatureOnly] {
            assert_eq!(id.policy(), TemplatePolicy::NONE, "{}", id.name());
        }

        // Standard, Scale, Upright: `DefaultDisplayPolicy`.
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
        // The C++ enum is ordered; the order is the reported integer.
        assert_eq!(TemplateId::Standard.as_u8(), 0);
        assert_eq!(TemplateId::Modern.as_u8(), 5);
    }

    #[test]
    fn the_buffer_policy_is_deferred_only_for_the_four_stages_that_say_so() {
        // `docs/display-architecture.md` "Buffer policy": the fullscreen timers
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
