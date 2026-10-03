//! Port of `include/clevercoffee/display/DisplayLayoutUtils.h`.
//!
//! These helpers are what the AGENTS.md OLED rules are *implemented* in.
//! Every one of them encodes a rule that is otherwise easy to state and easy to
//! violate:
//!
//! * [`draw_str_right_in_box`] implements **stable numeric fields** (rule 3): a
//!   value is drawn inside a box whose pixel width was reserved from a
//!   *widest-case probe string*, so a digit count change moves the field's far
//!   edge, not its near edge.
//! * [`draw_str_centered_on_screen`] is the naive centring, kept separate so
//!   that using it where a fixed-width box belongs is a visible choice.
//! * [`layout_bar_label_cluster`] implements **paired-control midline
//!   alignment** (rule 4): a bar and its value label are vertically centred
//!   *independently within the same row*, so a 4 px bar and a 10 px label share
//!   a midline instead of being bottom- or top-edge aligned.
//!
//! The C++ is `inline void f(U8G2*, int, int, ...)`. That is four positional
//! `int`s and a `const char*`, and the call sites
//! (`ModernTemplate::drawLargeTemperature`, `drawTemperatureToSetpointBar`,
//! `drawBrewMainReadout`, `drawBrewProgressBar`, `drawPostBrewScreen`) are where
//! the bugs this guards against actually happened. So the port takes a typed
//! [`FixedBox`] instead of loose coordinates: a box cannot be built with
//! `box_x + box_w` the wrong way round, and `width_from_probe` is the only way
//! to make one, which means the widest-case string is always named at the point
//! of use.
//!
//! # Coordinates are pen-relative
//!
//! `drawStr(x, y, ...)` with `setFontPosTop()` puts `y` at the top of the glyph
//! *bounding box*, not on a baseline. Every helper here passes `y` straight
//! through, so callers must pass a top edge. See [`Font::ink_box`] for the
//! measured box, which is what the fit assertions check.

use crate::display::{Display, DISPLAY_WIDTH};
use crate::font::Font;

/// A reserved rectangle for one value, with a fixed pixel width.
///
/// The width is always derived from a *probe* — the widest string the field can
/// ever hold — never from the live value. That is the whole point: `9` -> `10`
/// -> `99` must not move the left edge, and it cannot if the box was sized once
/// from a string that is already the widest case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedBox {
    /// Left edge, in pen coordinates.
    pub x: i32,
    /// Top edge, in pen coordinates (`setFontPosTop` semantics).
    pub y: i32,
    /// Reserved width in pixels. Constant for the life of the box.
    pub width: i32,
}

impl FixedBox {
    /// A box of `width` pixels at `(x, y)`.
    ///
    /// Prefer [`FixedBox::from_probe`]: a width passed here has to have come
    /// from somewhere, and `from_probe` puts the widest-case string next to the
    /// number it came from.
    #[must_use]
    pub const fn new(x: i32, y: i32, width: i32) -> Self {
        Self { x, y, width }
    }

    /// A box as wide as `probe` renders in `font`, positioned at `(x, y)`.
    #[must_use]
    pub fn from_probe(x: i32, y: i32, font: Font, probe: &str) -> Self {
        Self {
            x,
            y,
            width: font.str_width(probe),
        }
    }

    /// The x a right-aligned value starts at.
    #[must_use]
    pub const fn right_edge(&self) -> i32 {
        self.x + self.width
    }

    /// Draw `text` right-aligned inside the box.
    pub fn draw_right(&self, d: &mut Display, text: &str) {
        d.draw_str(self.right_edge() - d.str_width(text), self.y, text);
    }

    /// Draw `text` centred inside the box.
    pub fn draw_centre(&self, d: &mut Display, text: &str) {
        d.draw_str(self.x + (self.width - d.str_width(text)) / 2, self.y, text);
    }
}

/// `drawStrRightInBox` (`DisplayLayoutUtils.h:11`).
///
/// Draws `text` so its **right** edge sits at `box_x + box_w`. This is the
/// helper every counting field uses: the far edge is pinned to a constant, so
/// adding a digit extends the string leftwards into the reserved space instead
/// of shifting the whole field right.
pub fn draw_str_right_in_box(d: &mut Display, box_x: i32, box_w: i32, y: i32, text: &str) {
    d.draw_str(box_x + box_w - d.str_width(text), y, text);
}

/// `drawStrCenteredOnScreen` (`DisplayLayoutUtils.h:19`).
///
/// Centres on `DISPLAY_WIDTH` — the *physical* width, even under `R1`/`R3`.
/// That is what the C++ does (`(DISPLAY_WIDTH - w) / 2`, and `DISPLAY_WIDTH` is
/// the constant 128), and it is correct for the templates that use it: the OTA
/// screen is drawn before the rotation is known, and the post-brew cup is
/// centred in the portrait width by the caller passing the rotated box.
pub fn draw_str_centered_on_screen(d: &mut Display, y: i32, text: &str) {
    let w = d.str_width(text);
    d.draw_str((DISPLAY_WIDTH - w) / 2, y, text);
}

/// A bar and the label beside it, already positioned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarLabelCluster {
    /// Bar's left edge.
    pub bar_x: i32,
    /// Bar's top edge.
    pub bar_y: i32,
    /// Label's left edge.
    pub label_x: i32,
    /// Label's top edge (a `setFontPosTop` top edge).
    pub label_y: i32,
}

impl BarLabelCluster {
    /// The bar's vertical centre, in half-pixels.
    ///
    /// Doubled to keep the division exact for odd heights: a 4 px bar has its
    /// centre at `y + 1.5`, and the label's at `y + font_h / 2.0`, and those
    /// are not comparable as integers. `assert_shared_midline` is the
    /// assertion; this is the number it compares.
    #[must_use]
    pub const fn bar_midline2(&self, bar_h: i32) -> i32 {
        2 * self.bar_y + bar_h
    }

    /// The label's vertical centre, in half-pixels.
    #[must_use]
    pub const fn label_midline2(&self, label_h: i32) -> i32 {
        2 * self.label_y + label_h
    }
}

/// `layoutBarLabelCluster` (`DisplayLayoutUtils.h:39`).
///
/// Horizontally centres `bar_w + gap + label_w` on the physical width, and
/// vertically centres the bar and the label *independently* within
/// `row_top_y .. row_top_y + row_h - 1`.
///
/// `label_font_h` is the label's **bbox height**, not the font's nominal size —
/// see [`crate::templates::modern_layout`] for how the Modern template derives
/// it, and [`Font::ref_box_height`] for U8g2's own arithmetic.
///
/// `max_label_probe` is the widest the label will ever be. Sizing the cluster
/// from it is what stops the bar from jumping sideways when the target changes
/// (`/ 30g` -> `/ 36g`).
#[must_use]
pub fn layout_bar_label_cluster(
    d: &Display,
    bar: BarBox,
    label: LabelBox,
    max_label_probe: &str,
) -> BarLabelCluster {
    let label_w = d.str_width(max_label_probe);
    let cluster_w = bar.w + label.gap + label_w;
    let cluster_x = (DISPLAY_WIDTH - cluster_w) / 2;

    BarLabelCluster {
        bar_x: cluster_x,
        bar_y: label.row_top_y + (label.row_h - bar.h) / 2,
        label_x: cluster_x + bar.w + label.gap,
        label_y: label.row_top_y + (label.row_h - label.font_h) / 2,
    }
}

/// The bar's own geometry, for [`layout_bar_label_cluster`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarBox {
    /// Bar width in pixels.
    pub w: i32,
    /// Bar height in pixels.
    pub h: i32,
}

/// The label's row, the gap, and the label's reserved height.
///
/// Grouped so the eight-argument C++ signature becomes four, and so
/// `font_h` and `row_h` sit next to each other: those are the two numbers a
/// mid-line assertion compares, and the C++ lets them drift apart silently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LabelBox {
    /// Top of the row both the bar and the label are centred within.
    pub row_top_y: i32,
    /// The row's reserved height.
    pub row_h: i32,
    /// The label's *reserved* height, not a font metric. See
    /// [`crate::templates::modern_layout`].
    pub font_h: i32,
    /// Horizontal gap between the bar's right edge and the label's left edge.
    pub gap: i32,
}

/// A bar and its label that do not share a vertical midline.
///
/// AGENTS.md rule 4. Carrying the numbers rather than a formatted string keeps
/// the library `no_std` with no `alloc`; the test formats it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidlineMismatch {
    /// The bar's centre, in half-pixels, so an odd height stays exact.
    pub bar_midline2: i32,
    /// The label's centre, in half-pixels.
    pub label_midline2: i32,
    /// The bar's top edge.
    pub bar_y: i32,
    /// The bar's height.
    pub bar_h: i32,
    /// The label's top edge.
    pub label_y: i32,
    /// The label's height.
    pub label_h: i32,
}

/// Assert AGENTS.md rule 4: the bar and the label share a vertical midline.
///
/// The layout code *computes* both midlines from the same `row_top_y` and
/// `row_h`, so the property holds by construction — which is exactly why it
/// needs to be checkable: a future edit that changes one `row_h` and not the
/// other still compiles and still looks plausible.
///
/// # Errors
///
/// [`MidlineMismatch`] when the two cent differ. The layout code computes both
/// from the same `row_top_y` and `row_h`, so this cannot fail for a cluster
/// from [`layout_bar_label_cluster`]; it exists so a hand-built
/// [`BarLabelCluster`], or a future edit that stops using the helper, is caught.
pub fn check_shared_midline(
    cluster: &BarLabelCluster,
    bar_h: i32,
    label_h: i32,
) -> Result<(), MidlineMismatch> {
    let bar_midline2 = cluster.bar_midline2(bar_h);
    let label_midline2 = cluster.label_midline2(label_h);
    if bar_midline2 == label_midline2 {
        Ok(())
    } else {
        Err(MidlineMismatch {
            bar_midline2,
            label_midline2,
            bar_y: cluster.bar_y,
            bar_h,
            label_y: cluster.label_y,
            label_h,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font;

    fn mk() -> Display {
        let mut d = Display::new();
        d.set_font(font::profont11());
        d.set_font_pos_top();
        d
    }

    use crate::display::Framebuffer;

    /// The rightmost column holding ink, scanning every row.
    fn ink_right(fb: &Framebuffer) -> Option<i32> {
        (0..DISPLAY_WIDTH)
            .rev()
            .find(|&x| (0..32).any(|y| fb.pixel(x, y)))
    }

    #[test]
    fn a_right_aligned_value_pins_its_far_edge() {
        // The rule-3 property: a value drawn in a fixed box ends at the same
        // column whether it is one digit or three.
        let f = font::profont11();
        let box_x = 20;
        let probe = f.str_width("999");

        let mut d = mk();
        draw_str_right_in_box(&mut d, box_x, probe, 0, "9");
        let one = d.into_framebuffer();

        let mut d = mk();
        draw_str_right_in_box(&mut d, box_x, probe, 0, "999");
        let three = d.into_framebuffer();

        // Both end at the same column...
        assert_eq!(ink_right(&one), ink_right(&three), "far edge must not move");
        assert!(ink_right(&three).is_some());
        // ...and the one-digit value starts further right by exactly the width
        // of the two digits it does not have.
        let one_left = (0..DISPLAY_WIDTH).find(|&x| (0..32).any(|y| one.pixel(x, y)));
        let three_left = (0..DISPLAY_WIDTH).find(|&x| (0..32).any(|y| three.pixel(x, y)));
        assert_eq!(
            one_left.expect("ink") - three_left.expect("ink"),
            probe - f.str_width("9"),
            "the shorter value is inset by the two digits it is missing"
        );
    }

    #[test]
    fn a_fixed_box_right_aligns_inside_its_own_width() {
        let f = font::profont11();
        let mut d = mk();
        let box_ = FixedBox::from_probe(10, 0, f, "999 s");
        let right = box_.right_edge();
        assert_eq!(right, 10 + f.str_width("999 s"));

        box_.draw_right(&mut d, "9 s");
        let a = d.clone().into_framebuffer();
        d.clear_buffer();
        box_.draw_right(&mut d, "999 s");
        let b = d.into_framebuffer();

        let ink_right = |fb: &Framebuffer| {
            (0..DISPLAY_WIDTH)
                .rev()
                .find(|&x| (0..16).any(|y| fb.pixel(x, y)))
        };
        assert_eq!(
            ink_right(&a),
            ink_right(&b),
            "both values end at the same column"
        );
    }

    #[test]
    fn a_centred_box_centres_the_shortest_and_longest_alike() {
        let f = font::profont11();
        let mut d = mk();
        let box_ = FixedBox::from_probe(0, 0, f, "999");
        assert_eq!(box_.width, f.str_width("999"));

        box_.draw_centre(&mut d, "999");
        let long = d.clone().into_framebuffer();
        d.clear_buffer();
        box_.draw_centre(&mut d, "1");
        let short = d.into_framebuffer();

        let extent = |fb: &Framebuffer| {
            let xs: [i32; 128] = core::array::from_fn(|i| i32::try_from(i).unwrap_or(0));
            let first = xs
                .iter()
                .find(|&&x| (0..16).any(|y| fb.pixel(x, y)))
                .copied();
            let last = xs
                .iter()
                .rev()
                .find(|&&x| (0..16).any(|y| fb.pixel(x, y)))
                .copied();
            match (first, last) {
                (Some(a), Some(b)) => (a, b),
                _ => (0, 0),
            }
        };
        let (l0, l1) = extent(&long);
        let (s0, s1) = extent(&short);
        // Same centre: (l0 + l1) == (s0 + s1), allowing the odd-width rounding
        // that `(w - text_w) / 2` introduces.
        assert!(
            (l0 + l1 - (s0 + s1)).abs() <= 1,
            "centres differ: {l0}..{l1} vs {s0}..{s1}"
        );
    }

    #[test]
    fn a_bar_and_its_label_share_a_vertical_midline() {
        // The Modern bottom row, verbatim from `ModernTemplateLayout`:
        // 10 px row at y=54..63, 4 px bar, 10 px label, gap 3.
        let f = font::profont10();
        let mut d = mk();
        d.set_font(f);
        let cluster = layout_bar_label_cluster(
            &d,
            BarBox { w: 72, h: 4 },
            LabelBox {
                row_top_y: 54,
                row_h: 10,
                font_h: 10,
                gap: 3,
            },
            "100\u{b0}C",
        );
        assert_eq!(cluster.bar_y, 57, "bar centre 59.0");
        assert_eq!(cluster.label_y, 54, "label centre 59.0");
        check_shared_midline(&cluster, 4, 10).expect("midlines must agree");
    }

    #[test]
    fn a_label_height_must_be_the_reserved_row_height_not_u8g2s_reference_box() {
        // This is the trap. U8g2's own reference box for profont10 under
        // `setFontRefHeightExtendedText()` is `ref_ascent + ref_descent` = 7 + -2
        // = 5, and passing that to `layout_bar_label_cluster` puts the label at
        // y=56 instead of y=54 -- half a pixel of drift that is invisible on
        // screen and breaks the "pair the bar with its label" rule.
        //
        // The C++ does not have this problem because `ModernTemplateLayout`
        // passes the hard-coded `kFontHeightProfont10 = 10`, the *reserved row*
        // height, not a font metric. So the templates must pass a reserved
        // height; this test exists to make that decision explicit rather than
        // incidental.
        let f = font::profont10();
        let ref_box = f.ref_box_height(crate::font::HeightMode::ExtendedText);
        assert_eq!(
            ref_box, 5,
            "U8g2's reference box is not the reserved row height"
        );
        assert_eq!(f.max_char_height(), 10, "max_char_height is, for this font");
    }

    #[test]
    fn the_cluster_is_centred_on_the_physical_width() {
        let f = font::profont10();
        let mut d = mk();
        d.set_font(f);
        let probe = "100\u{b0}C";
        let cluster = layout_bar_label_cluster(
            &d,
            BarBox { w: 72, h: 4 },
            LabelBox {
                row_top_y: 54,
                row_h: 10,
                font_h: 10,
                gap: 3,
            },
            probe,
        );
        // The cluster is sized from the *probe*, so the reserved span is what
        // gets centred, and integer division leaves 0 or 1 px of slack.
        let reserved = 72 + 3 + f.str_width(probe);
        assert_eq!(cluster.bar_x, (DISPLAY_WIDTH - reserved) / 2);
        assert_eq!(cluster.label_x, cluster.bar_x + 72 + 3);
        // "Centred" means the slack is split evenly, and integer division can
        // only ever put the odd pixel on one side.
        let left = cluster.bar_x;
        let right = DISPLAY_WIDTH - (cluster.label_x + f.str_width(probe));
        assert!(
            (left - right).abs() <= 1,
            "left margin {left} != right margin {right} (reserved {reserved} px)"
        );
    }

    #[test]
    fn the_cluster_width_does_not_depend_on_the_live_label() {
        // Sizing from the probe is what stops the bar sliding when the target
        // changes. Assert it directly: the same probe, three live labels.
        let f = font::profont10();
        let mut d = mk();
        d.set_font(f);
        let probe = "/ 999g";
        let boxes = (
            BarBox { w: 88, h: 4 },
            LabelBox {
                row_top_y: 54,
                row_h: 10,
                font_h: 10,
                gap: 3,
            },
        );
        let a = layout_bar_label_cluster(&d, boxes.0, boxes.1, probe);
        let b = layout_bar_label_cluster(&d, boxes.0, boxes.1, probe);
        assert_eq!(a, b);
        assert_eq!(a.bar_x, (DISPLAY_WIDTH - (88 + 3 + f.str_width(probe))) / 2);
    }

    #[test]
    fn a_mismatched_midline_is_reported_not_hidden() {
        // The check must be able to fail, or it is not a check.
        let cluster = BarLabelCluster {
            bar_x: 0,
            bar_y: 0,
            label_x: 0,
            label_y: 3,
        };
        let err = check_shared_midline(&cluster, 4, 10).expect_err("must fail");
        assert_eq!(
            err.bar_midline2, 4,
            "bar occupies y=0..3, centre 2.0 -> 4 half-px"
        );
        assert_eq!(
            err.label_midline2, 16,
            "label occupies y=3..12, centre 8.0 -> 16 half-px"
        );
    }

    #[test]
    fn odd_heights_stay_exact() {
        // A 5 px bar in a 12 px row with a 7 px label: both centres land on a
        // half-pixel, which integer arithmetic would round away.
        let cluster = BarLabelCluster {
            bar_x: 0,
            bar_y: 3,
            label_x: 0,
            label_y: 2,
        };
        assert_eq!(cluster.bar_midline2(5), 11); // 3.5 -> 7? no: 2*3+5 = 11 -> 5.5
        assert_eq!(cluster.label_midline2(7), 11); // 2*2+7 = 11 -> 5.5
        check_shared_midline(&cluster, 5, 7).expect("5.5 == 5.5");
    }
}
