//! Port of `include/clevercoffee/display/DisplayFullscreenModes.h`.
//!
//! The four modes that take over the whole panel: the brew timer, the manual
//! flush, the hot-water timer, and the offline splash. They are checked before
//! the system screens, so a machine that is simultaneously, say, in a
//! backflush *and* flushing shows the flush — which is the behaviour that makes
//! the backflush's own screen reachable at all.
//!
//! # The Upright template's coordinates are the surprising part
//!
//! `displayFullscreenBrewTimer` draws its cup at `(12, 12)` in the Upright
//! template and its digits at `(5, 70)` / `(5, 100)` — y = 70 and y = 100 are
//! off the bottom of a 64-row panel, and are only inside it because the Upright
//! template renders in the R1/R3 *portrait* space where the height is 128. The
//! same applies to `displayBrewtimeFs`, which takes its `(x, y)` from the
//! caller. This is reproduced exactly: the coordinates are logical, the rotation
//! transform is applied on the way into the buffer, and
//! `tests/golden.rs` asserts the frames are inside 128x64 *after* the transform.

use crate::bitmaps_data as bm;
use crate::display::Display;
use crate::fmt::{format_fixed, format_int, truncate_to_i32};
use crate::font;
use crate::helpers::is_manual_flush_state;
use crate::model::{BrewTimerState, Config, DisplayInput};
use crate::templates::{should_display_brew_timer, Stage, TemplateId};
use crate::widgets;

/// `displayFullscreenBrewTimer`.
///
/// Returns `None` when the mode does not apply, which is every time the feature
/// is off or no brew is running.
pub fn draw_brew_timer(
    d: &mut Display,
    input: &DisplayInput,
    config: &Config,
    template: TemplateId,
) -> Option<Stage> {
    if !config.fullscreen_brew_timer {
        return None;
    }
    if !should_display_brew_timer(input.brew_timer) {
        return None;
    }
    let upright = template.is_upright();
    d.clear_buffer();

    if upright {
        d.draw_xbmp(12, 12, 40, 40, &bm::BREW_CUP_LOGO);
        if config.scale_enabled {
            d.set_font(font::profont22());
            d.set_cursor(5, 70);
            d.print(format_int(truncate_to_i32(input.brew_time_ms / 1000.0)).as_str());
            d.print("s");
            d.set_cursor(5, 100);
            d.print(format_fixed(f64::from(input.brew_weight), 1).as_str());
            d.print("g");
            d.set_font(font::profont11());
        } else {
            widgets::display_brew_time_fs(d, 1, 80, input.brew_time_ms, true);
        }
    } else {
        d.draw_xbmp(-1, 11, 40, 40, &bm::BREW_CUP_LOGO);
        if config.scale_enabled {
            d.set_font(font::profont22());
            d.set_cursor(48, 36);
            d.print(format_int(truncate_to_i32(input.brew_time_ms / 1000.0)).as_str());
            d.print("s");
            d.set_cursor(48, 58);
            d.print(format_fixed(f64::from(input.brew_weight), 1).as_str());
            d.print("g");
            d.set_font(font::profont11());
        } else {
            widgets::display_brew_time_fs(d, 48, 25, input.brew_time_ms, false);
        }
    }
    Some(Stage::FullscreenBrew)
}

/// `displayFullscreenManualFlushTimer`.
///
/// Two conditions, not one: the machine must be in a manual-flush state *and*
/// the current state must be `MANUAL_FLUSH_RUNNING` specifically. The second is
/// redundant today (they are the same state) but the C++ checks both, and
/// narrowing it is the kind of "simplification" that changes behaviour when a
/// second flush state is added.
pub fn draw_manual_flush_timer(
    d: &mut Display,
    input: &DisplayInput,
    template: TemplateId,
) -> Option<Stage> {
    if !is_manual_flush_state(input.state) {
        return None;
    }
    if input.state != cc_domain::state::MachineState::ManualFlushRunning {
        return None;
    }
    let upright = template.is_upright();
    d.clear_buffer();
    if upright {
        d.draw_xbmp(12, 12, 40, 40, &bm::MANUAL_FLUSH_LOGO);
        widgets::display_brew_time_fs(d, 1, 80, input.brew_time_ms, true);
    } else {
        d.draw_xbmp(0, 12, 40, 40, &bm::MANUAL_FLUSH_LOGO);
        widgets::display_brew_time_fs(d, 48, 25, input.brew_time_ms, false);
    }
    Some(Stage::FullscreenManualFlush)
}

/// `displayFullscreenHotWaterTimer`.
pub fn draw_hot_water_timer(
    d: &mut Display,
    input: &DisplayInput,
    template: TemplateId,
) -> Option<Stage> {
    if !crate::helpers::should_display_hot_water_timer(input) {
        return None;
    }
    let upright = template.is_upright();
    d.clear_buffer();
    if upright {
        d.draw_xbmp(12, 12, 40, 40, &bm::HOT_WATER_LOGO);
        widgets::display_brew_time_fs(d, 1, 80, input.pump_on_time_ms, true);
    } else {
        d.draw_xbmp(0, 12, 40, 40, &bm::HOT_WATER_LOGO);
        widgets::display_brew_time_fs(d, 48, 25, input.pump_on_time_ms, false);
    }
    Some(Stage::FullscreenHotWater)
}

/// `displayOfflineMode`.
///
/// The offline splash is a countdown: it shows for `displayOffline` frames
/// (1..=19) and the caller increments the counter each time. The C++ checks
/// `> 0 && < 20`; `0` means "not offline" and `>= 20` means the fallback has
/// run its course. Reproduced, including the off-by-one at both ends.
pub fn draw_offline(d: &mut Display, input: &DisplayInput) -> Option<Stage> {
    if input.display_offline == 0 || input.display_offline >= 20 {
        return None;
    }
    widgets::display_message(d, ["", "", "", "", "Begin Fallback,", "No Wifi"]);
    Some(Stage::Offline)
}

/// Whether the post-brew screen is showing, for the Modern template's dispatch.
///
/// Not a stage of its own: in the C++ the post-brew screen is a *branch of
/// `ModernTemplate::renderNormalDisplay`*, checked against the brew-timer FSM.
#[must_use]
pub const fn is_post_brew(state: BrewTimerState) -> bool {
    matches!(state, BrewTimerState::PostBrew)
}
