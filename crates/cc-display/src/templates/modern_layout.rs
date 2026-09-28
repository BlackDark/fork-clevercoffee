//! `ModernTemplateLayout` — the row map, as constants.
//!
//! Ported from `include/clevercoffee/display/templates/ModernTemplate.h:19-64`.
//! Every number here is a *reserved* row height or an absolute Y, and the
//! distinction is load-bearing.
//!
//! # The heights are NOT U8g2's font metrics
//!
//! `docs/display-modern-layout.md` says these are "the font bbox pixel heights
//! (not the number in the font name)". Measured against real U8g2, that is not
//! quite true, and the difference matters:
//!
//! | font | reserved here | U8g2 `ref_ascent + ref_descent` | `max_char_height` | ink, `"100.0"` | ink, `"Ay"` |
//! |------|---------------|-------------------------------|--------------------|-----------------|-------------|
//! | `fub20_tf` | 23 | 15 | 36 | 20 | 25 |
//! | `profont17_tf` | 15 | 10 | 17 | 11 | 14 |
//! | `profont11_tf` | 11 | 6 | 11 | 7 | 11 |
//! | `profont10_tf` | 10 | 5 | 10 | 6 | 9 |
//!
//! So the reserved height is neither. It is the height the row was given when
//! the layout was written, and it is consistently a little more than the ink
//! the row actually needs, which is the safe direction: the gap between rows is
//! what the "no overlapping rows" rule protects.
//!
//! # Consequence for `layout_bar_label_cluster`
//!
//! [`k_font_height_profont10`] is what must be passed as the label height, *not*
//! U8g2's reference box (5 for `profont10`). Passing 5 puts the label half a
//! pixel off the bar's midline. `Layout::bar_label_height_is_the_reserved_row`
//! exists to make that decision explicit rather than accidental.
//!
//! # Row map (idle), from `docs/display-modern-layout.md`
//!
//! ```text
//!  y=0  ─ status bar (radio, MQTT, uptime)
//!  y=12 ─ separator
//!  y=14 ─ main temp: fub20 digits (fixed "100.0" box) + profont17 °C
//!  y=38 ─ 16×16 icon + HEATING/READY text (ends y=53)
//!  y=54 ─ centered bar + setpoint label cluster
//! ```

use crate::display::{DISPLAY_HEIGHT, STATUS_BAR_Y_POS};

/// `kFontHeightFub20` — the reserved height of the big temperature row.
pub const K_FONT_HEIGHT_FUB20: i32 = 23;
/// `kFontHeightProfont17` — the reserved height of the `°C` / brew-field rows.
pub const K_FONT_HEIGHT_PROFONT17: i32 = 15;
/// `kFontHeightProfont11` — the reserved height of the status/HEATING rows.
pub const K_FONT_HEIGHT_PROFONT11: i32 = 11;
/// `kFontHeightProfont10` — the reserved height of the bottom row.
pub const K_FONT_HEIGHT_PROFONT10: i32 = 10;

/// `kStatusBarSeparatorY` — the rule under the status bar.
pub const K_STATUS_BAR_SEPARATOR_Y: i32 = STATUS_BAR_Y_POS;
/// `kRowGap` — the gap between the separator and the first content row.
pub const K_ROW_GAP: i32 = 2;

/// `kIdleTempY` — the big temperature row's top edge (y=14).
pub const K_IDLE_TEMP_Y: i32 = K_STATUS_BAR_SEPARATOR_Y + K_ROW_GAP;
/// `kIdleIconSize` — the width and height of the HEATING/READY icon.
pub const K_IDLE_ICON_SIZE: i32 = 16;
/// `kIdleIconGap` — the gap between the icon and its label.
pub const K_IDLE_ICON_GAP: i32 = 4;

/// `kBottomBarH` — the progress bar's height.
pub const K_BOTTOM_BAR_H: i32 = 4;
/// `kBottomBarW` — the temperature-to-setpoint bar's width.
pub const K_BOTTOM_BAR_W: i32 = 72;
/// `kBottomBarLabelGap` — the gap between that bar and its `94°C` label.
pub const K_BOTTOM_BAR_LABEL_GAP: i32 = 3;
/// `kBottomRowH` — the bottom row's reserved height.
pub const K_BOTTOM_ROW_H: i32 = K_FONT_HEIGHT_PROFONT10;
/// `kBottomRowY` — the bottom row's top edge, anchored from the panel height.
pub const K_BOTTOM_ROW_Y: i32 = DISPLAY_HEIGHT - K_BOTTOM_ROW_H;
/// `kContentBottomY` — the last row the main content may reach.
pub const K_CONTENT_BOTTOM_Y: i32 = K_BOTTOM_ROW_Y - 1;
/// `kIdleStatusRowY` — the HEATING/READY row's top edge.
pub const K_IDLE_STATUS_ROW_Y: i32 = K_CONTENT_BOTTOM_Y - K_IDLE_ICON_SIZE + 1;

/// `kBrewBarW` — the brew progress bar's width (wider than the idle bar).
pub const K_BREW_BAR_W: i32 = 88;

/// `kBrewMainY` — the brew screen's main readout row.
pub const K_BREW_MAIN_Y: i32 = K_STATUS_BAR_SEPARATOR_Y + K_ROW_GAP;
/// `kBrewFooterY` — the brew screen's weight/pressure footer.
pub const K_BREW_FOOTER_Y: i32 = K_CONTENT_BOTTOM_Y - K_FONT_HEIGHT_PROFONT10;

/// `kDegreeCProbe` — `"100°C"`, the widest the `°C` unit field can get.
pub const K_DEGREE_C_PROBE: &str = "100\u{b0}C";
/// `kSetpointProbe` — `"100°C"`, the widest the setpoint label can get.
pub const K_SETPOINT_PROBE: &str = "100\u{b0}C";
/// `kBrewTargetTimeProbe` — `"/ 999s"`, the widest the time target can get.
pub const K_BREW_TARGET_TIME_PROBE: &str = "/ 999s";
/// `kBrewTargetWeightProbe` — `"/ 999g"`, the widest the weight target can get.
pub const K_BREW_TARGET_WEIGHT_PROBE: &str = "/ 999g";

/// `kTempMapMin` — the temperature at which the bottom bar starts filling.
pub const K_TEMP_MAP_MIN: i32 = 20;

/// `isReadyForBrew` — within [`crate::helpers::HEATING_LOGO_THRESHOLD_C`] of
/// setpoint.
#[must_use]
pub fn is_ready_for_brew(temp_c: f64, setpoint_c: f64) -> bool {
    temp_c >= setpoint_c - f64::from(crate::helpers::HEATING_LOGO_THRESHOLD_C)
}

/// `mapTempToBarWidth` — the filled width of the temperature bar.
///
/// Zero below [`K_TEMP_MAP_MIN`], otherwise a linear map clamped to
/// `0..=inner_w`. Note the guard on `setpoint <= K_TEMP_MAP_MIN`: a setpoint at
/// or below 20 °C would make `map`'s divisor zero-ish, and the C++ returns 0
/// rather than dividing.
#[must_use]
pub fn map_temp_to_bar_width(temp_c: f64, setpoint_c: f64, inner_w: i32) -> i32 {
    if setpoint_c <= f64::from(K_TEMP_MAP_MIN) {
        return 0;
    }
    // `static_cast<int>` on both, exactly as `ModernTemplateLayout::mapTempToBarWidth`
    // does. Truncation toward zero is the C++ behaviour and is what the bar's
    // fill width depends on.
    let mapped = crate::widgets::arduino_map(
        crate::fmt::truncate_to_i32(temp_c),
        K_TEMP_MAP_MIN,
        crate::fmt::truncate_to_i32(setpoint_c),
        0,
        inner_w,
    );
    crate::widgets::constrain(mapped, 0, inner_w)
}

/// The three Modern screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// `drawIdleScreen`.
    Idle,
    /// `drawBrewScreen`, with the phase.
    Brew {
        /// `BrewPhase`.
        phase: BrewPhase,
        /// Whether this is the manual-flush rendering.
        flushing: bool,
    },
    /// `drawPostBrewScreen`.
    PostBrew,
}

/// `ModernTemplate::BrewPhase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrewPhase {
    /// `BREW_PREINFUSION` / `BREW_PREINFUSION_PAUSE`.
    PreInfusion,
    /// Anything else that is not `BREW_FINISHED`.
    Brewing,
    /// `BREW_FINISHED`.
    Done,
}

impl BrewPhase {
    /// The three phase labels, in the order `drawPhaseIndicator` lays them out.
    pub const LABELS: [&'static str; 3] = ["PRE-INF", "BREW", "DONE"];

    /// The x each label starts at (`drawPhaseIndicator`'s `xs`).
    pub const XS: [i32; 3] = [2, 46, 86];

    /// This phase's index into [`BrewPhase::LABELS`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::PreInfusion => 0,
            Self::Brewing => 1,
            Self::Done => 2,
        }
    }
}

/// Which of the Modern screens a state renders.
#[must_use]
pub fn screen_for(flushing: bool, post_brew: bool, brewing: bool) -> Screen {
    if flushing {
        Screen::Brew {
            phase: BrewPhase::Brewing,
            flushing: true,
        }
    } else if post_brew {
        Screen::PostBrew
    } else if brewing {
        Screen::Brew {
            phase: BrewPhase::Brewing,
            flushing: false,
        }
    } else {
        Screen::Idle
    }
}

/// `Brew_Cup_Logo_width` — the post-brew cup's width.
pub const BREW_CUP_LOGO_W: i32 = 40;
/// `Brew_Cup_Logo_height` — the post-brew cup's height.
pub const BREW_CUP_LOGO_H: i32 = 40;
/// `cupY` — where the cup starts, `docs/display-modern-layout.md` "y=2".
pub const BREW_CUP_Y: i32 = 2;

/// A change that made the Modern rows overlap would fail to **build**, not just
/// to pass a test. The `K_ROW_GAP`-derived constants are all `i32` literals, so
/// this is a compile-time assertion with no runtime cost.
const _: () = assert!(K_BREW_FOOTER_Y + K_FONT_HEIGHT_PROFONT10 - 1 < K_BOTTOM_ROW_Y);
const _: () = assert!(K_IDLE_TEMP_Y + K_FONT_HEIGHT_FUB20 - 1 < K_IDLE_STATUS_ROW_Y);
const _: () = assert!(K_IDLE_STATUS_ROW_Y + K_IDLE_ICON_SIZE - 1 <= K_CONTENT_BOTTOM_Y);
const _: () = assert!(K_CONTENT_BOTTOM_Y + 1 == K_BOTTOM_ROW_Y);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::{self, HeightMode};

    #[test]
    fn the_row_map_is_anchored_from_the_bottom() {
        // AGENTS.md rule 5: anchor the bottom rows from DISPLAY_HEIGHT. If the
        // panel height ever changes, the bottom row follows it and the content
        // bottom follows the bottom row -- the whole chain moves together.
        assert_eq!(K_BOTTOM_ROW_Y, 54);
        assert_eq!(K_CONTENT_BOTTOM_Y, 53);
        assert_eq!(K_IDLE_STATUS_ROW_Y, 38);
        assert_eq!(
            K_IDLE_STATUS_ROW_Y + K_IDLE_ICON_SIZE - 1,
            K_CONTENT_BOTTOM_Y,
            "the icon ends at the content bottom"
        );
        assert_eq!(
            K_BOTTOM_ROW_Y + K_BOTTOM_ROW_H,
            DISPLAY_HEIGHT,
            "the bottom row ends at the panel bottom"
        );
    }

    #[test]
    fn the_content_rows_do_not_overlap() {
        // AGENTS.md rule 2. The top-down chain: separator, temp row, status
        // row, bottom row.
        let separator = K_STATUS_BAR_SEPARATOR_Y;
        let temp_bottom = K_IDLE_TEMP_Y + K_FONT_HEIGHT_FUB20 - 1;
        let status_bottom = K_IDLE_STATUS_ROW_Y + K_IDLE_ICON_SIZE - 1;
        assert_eq!(
            K_IDLE_TEMP_Y,
            separator + K_ROW_GAP,
            "one gap below the separator"
        );
        assert!(
            temp_bottom < K_IDLE_STATUS_ROW_Y,
            "the temp row must end above the status row"
        );
        assert!(
            status_bottom <= K_CONTENT_BOTTOM_Y,
            "the status row must end at or above the content bottom"
        );
        assert_eq!(
            K_CONTENT_BOTTOM_Y + 1,
            K_BOTTOM_ROW_Y,
            "the bottom row starts where content ends"
        );
    }

    #[test]
    fn the_brew_rows_do_not_overlap() {
        // The brew screen's footer at y=43..52, the bottom row at y=54..63.
        assert_eq!(K_BREW_FOOTER_Y, 43);
        assert_eq!(K_BREW_FOOTER_Y + K_FONT_HEIGHT_PROFONT10 - 1, 52);
    }

    #[test]
    fn the_post_brew_time_row_ends_inside_the_panel() {
        // `docs/display-modern-layout.md`: "y=46, font 15px -> ends y=60".
        let time_y = BREW_CUP_Y + BREW_CUP_LOGO_H + 4;
        assert_eq!(time_y, 46);
        // The documented end is y=60, inside a 64-row panel.
        assert!(
            time_y + K_FONT_HEIGHT_PROFONT17 - 1 <= 60,
            "the documented end"
        );
        assert_eq!(time_y + K_FONT_HEIGHT_PROFONT17 - 1, 60);
    }

    #[test]
    fn bar_label_height_is_the_reserved_row() {
        // The point of this module: the bottom row's label height is
        // K_FONT_HEIGHT_PROFONT10 (10), which is NOT U8g2's reference box (5).
        let f = font::profont10();
        let ref_box = f.ref_box_height(HeightMode::ExtendedText);
        assert_eq!(ref_box, 5, "U8g2's reference box for profont10");
        assert_eq!(K_FONT_HEIGHT_PROFONT10, 10, "the reserved row height");
        assert_ne!(ref_box, K_FONT_HEIGHT_PROFONT10);

        // And the bar/label midlines only agree with the reserved height.
        let d = &{
            let mut d = crate::display::Display::new();
            d.set_font(f);
            d
        };
        let with_reserved = crate::layout::layout_bar_label_cluster(
            d,
            crate::layout::BarBox {
                w: K_BOTTOM_BAR_W,
                h: K_BOTTOM_BAR_H,
            },
            crate::layout::LabelBox {
                row_top_y: K_BOTTOM_ROW_Y,
                row_h: K_BOTTOM_ROW_H,
                font_h: K_FONT_HEIGHT_PROFONT10,
                gap: K_BOTTOM_BAR_LABEL_GAP,
            },
            K_SETPOINT_PROBE,
        );
        crate::layout::check_shared_midline(
            &with_reserved,
            K_BOTTOM_BAR_H,
            K_FONT_HEIGHT_PROFONT10,
        )
        .expect("the reserved height is the one that centres both");

        let with_ref_box = crate::layout::layout_bar_label_cluster(
            d,
            crate::layout::BarBox {
                w: K_BOTTOM_BAR_W,
                h: K_BOTTOM_BAR_H,
            },
            crate::layout::LabelBox {
                row_top_y: K_BOTTOM_ROW_Y,
                row_h: K_BOTTOM_ROW_H,
                font_h: ref_box,
                gap: K_BOTTOM_BAR_LABEL_GAP,
            },
            K_SETPOINT_PROBE,
        );
        assert!(
            crate::layout::check_shared_midline(&with_ref_box, K_BOTTOM_BAR_H, ref_box).is_err(),
            "passing U8g2's reference box must fail the midline check -- that is why this test exists"
        );
    }

    #[test]
    fn the_probes_are_the_widest_case_in_their_fonts() {
        // Rule 3 depends on the probe being the widest. These are the C++'s
        // literals, and the widths are *measured*, not assumed: each was read
        // out of the real-U8g2 oracle (`tools/oracle`) and then reproduced here.
        // The 25 px for "100C" is the width of the single Latin-1 byte 0xB0 --
        // two's worth of UTF-8 would measure 30, which is why
        // `font::latin1_of` exists.
        let f10 = font::profont10();
        let f17 = font::profont17();
        assert_eq!(
            f10.str_width(K_SETPOINT_PROBE),
            25,
            "the 100-degree-C probe in profont10"
        );
        assert_eq!(f10.str_width("/ 999g"), 29);
        assert_eq!(f10.str_width("/ 999s"), 29);
        // A three-digit target must not exceed the probe.
        assert!(f10.str_width("/ 36g") <= f10.str_width(K_BREW_TARGET_WEIGHT_PROBE));
        assert_eq!(
            f17.str_width(K_DEGREE_C_PROBE),
            44,
            "the 100-degree-C probe in profont17"
        );
        assert_eq!(
            f17.str_width("100.0"),
            44,
            "the idle temperature digits box"
        );
        assert_eq!(f17.str_width("999.9"), 44, "the post-brew time box");
    }

    #[test]
    fn the_temperature_bar_clamps_at_both_ends() {
        let inner = K_BOTTOM_BAR_W - 2;
        assert_eq!(
            map_temp_to_bar_width(94.0, 94.0, inner),
            inner,
            "at setpoint, full"
        );
        assert_eq!(
            map_temp_to_bar_width(20.0, 94.0, inner),
            0,
            "at the minimum, empty"
        );
        assert_eq!(
            map_temp_to_bar_width(15.0, 94.0, inner),
            0,
            "below the minimum, empty"
        );
        assert_eq!(map_temp_to_bar_width(0.0, 94.0, inner), 0);
        // A setpoint at or below 20 C returns 0 without dividing.
        assert_eq!(map_temp_to_bar_width(50.0, 20.0, inner), 0);
        assert_eq!(map_temp_to_bar_width(50.0, 0.0, inner), 0);
        // Above setpoint it clamps, it does not overflow.
        assert_eq!(map_temp_to_bar_width(120.0, 94.0, inner), inner);
    }

    #[test]
    fn the_bar_tick_is_five_degrees_below_setpoint() {
        // `docs/display-modern-layout.md`: "Bar tick = setpoint - 5 C".
        let inner = K_BOTTOM_BAR_W - 2;
        let at_tick = map_temp_to_bar_width(94.0 - 5.0, 94.0, inner);
        let just_past = map_temp_to_bar_width(94.0 - 5.1, 94.0, inner);
        assert!(at_tick < inner, "the tick is not at the end");
        assert!(at_tick > just_past, "and it moves with the temperature");
    }

    #[test]
    fn readiness_is_five_degrees_of_slack() {
        assert!(is_ready_for_brew(89.0, 94.0));
        assert!(is_ready_for_brew(89.0001, 94.0));
        assert!(!is_ready_for_brew(88.9, 94.0));
        // ...and a setpoint below 20 C still works, unlike the bar.
        assert!(is_ready_for_brew(15.0, 20.0));
    }
}
