//! The 128x64 framebuffer and the U8g2 draw primitives.
//!
//! # Byte layout is U8g2's, and must stay that way
//!
//! The buffer is page-major, vertical, LSB-on-top: byte
//! `buf[(y >> 3) * 128 + x]` holds the eight pixels of column `x` for rows
//! `8 * page ..= 8 * page + 7`, with bit `y & 7` selecting the row. That is
//! exactly `u8g2_ll_hvline_vertical_top_lsb` (`u8g2_ll_hvline.c:72`), and it
//! is also the SSD1306/SH1106 page-addressing wire format, so the same 1024
//! bytes go straight out over I2C with no repacking (R3-09).
//!
//! The alternative — a linear `y * 128 + x` bitmap — is friendlier to read but
//! would mean a transposing pass on every flush. Keeping U8g2's layout is what
//! makes the framebuffer the *device's* format rather than a Rust-only one.
//!
//! # Coordinates and rotation
//!
//! Drawing takes *logical* coordinates in a `0..width` x `0..height` space that
//! [`Rotation`] selects. Under `R1`/`R3` that space is 64 wide and 128 tall,
//! and the mapping onto the physical 128x64 page buffer is U8g2's
//! `u8g2_draw_l90_r1`..`r3` (`u8g2_setup.c:349-441`), reproduced in
//! [`Display::draw_hv_line`](Display::draw_hv_line). The UPRIGHT template is
//! drawn in that rotated space
//! and the *coordinates in the templates are the rotated ones*, matching the
//! C++ — `UprightTemplate::displayHeatBar` really does draw at y=124, which is
//! off the top of a 64-row screen and only sensible under `R1`/`R3`.
//!
//! That is the single most surprising thing about porting this display, and it
//! is why [`Rotation::width`]/[`Rotation::height`] exist instead of hard-coding
//! 128x64 in the clip test.

use crate::font::{Font, HeightMode};

/// Panel width in pixels. `defaults.h:224`.
pub const DISPLAY_WIDTH: i32 = 128;

/// Panel height in pixels. `defaults.h:225`.
pub const DISPLAY_HEIGHT: i32 = 64;

/// Framebuffer size in bytes: 16 tiles across, 8 pages down.
pub const BUFFER_LEN: usize = (DISPLAY_WIDTH as usize) * (DISPLAY_HEIGHT as usize / 8);

/// `STATUS_BAR_Y_POS`, `defaults.h:226-227`.
pub const STATUS_BAR_Y_POS: i32 = 12;

/// The display rotation, and with it the logical coordinate space.
///
/// `OledDriver::getU8G2Rotation` (`src/ui/OledDriver.cpp:80-96`) maps an
/// integer `0..3` onto `U8G2_R0..R3`, and `prepareDisplay` (`:66-79`) computes
/// it as `displayInverted * 2 + (template == UPRIGHT)`. Ported verbatim:
///
/// | rotation | set when | logical space |
/// |----------|----------|---------------|
/// | [`R0`](Rotation::R0) | neither | 128 x 64 |
/// | [`R1`](Rotation::R1) | UPRIGHT template | 64 x 128 |
/// | [`R2`](Rotation::R2) | `displayInverted` | 128 x 64 |
/// | [`R3`](Rotation::R3) | both | 64 x 128 |
///
/// R1/R3 swap width and height because U8g2's `u8g2_update_dimension_r1`
/// overwrites `width` with `pixel_height` (`u8g2_setup.c:245`). R2 is a 180
/// degree turn of R0; R3 is a 90 degree turn of R2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    /// `U8G2_R0` — identity.
    R0,
    /// `U8G2_R1` — 90 degrees clockwise.
    R1,
    /// `U8G2_R2` — 180 degrees.
    R2,
    /// `U8G2_R3` — 270 degrees clockwise.
    R3,
}

impl Rotation {
    /// The rotation from `OledDriver`'s integer, where anything else is R0.
    ///
    /// The C++ `switch` has a `default: return U8G2_R0` arm
    /// (`OledDriver.cpp:93`), so out-of-range is not an error.
    #[must_use]
    pub const fn from_index(index: i32) -> Self {
        match index {
            1 => Self::R1,
            2 => Self::R2,
            3 => Self::R3,
            _ => Self::R0,
        }
    }

    /// The index `OledDriver` computes for this rotation.
    #[must_use]
    pub const fn index(self) -> i32 {
        match self {
            Self::R0 => 0,
            Self::R1 => 1,
            Self::R2 => 2,
            Self::R3 => 3,
        }
    }

    /// Whether the logical space is portrait.
    #[must_use]
    pub const fn is_portrait(self) -> bool {
        matches!(self, Self::R1 | Self::R3)
    }

    /// Logical width. R1/R3 use `pixel_height`.
    #[must_use]
    pub const fn width(self) -> i32 {
        if self.is_portrait() {
            DISPLAY_HEIGHT
        } else {
            DISPLAY_WIDTH
        }
    }

    /// Logical height. R1/R3 use `pixel_width`.
    #[must_use]
    pub const fn height(self) -> i32 {
        if self.is_portrait() {
            DISPLAY_WIDTH
        } else {
            DISPLAY_HEIGHT
        }
    }
}

/// A half-open rectangle, the shape every intersection test is written against.
///
/// Half-open on both axes (`x0..x1`, `y0..y1`) because that is U8g2's
/// convention: `u8g2_IsIntersection` rejects a zero-width box, which is why
/// `drawBox(x, y, 0, h)` draws nothing while `drawFrame(x, y, 0, h)` still
/// draws its two side lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// Left edge, inclusive.
    pub x0: i32,
    /// Top edge, inclusive.
    pub y0: i32,
    /// Right edge, exclusive.
    pub x1: i32,
    /// Bottom edge, exclusive.
    pub y1: i32,
}

impl Rect {
    /// A rectangle of `w` by `h` with its top-left at `(x, y)`.
    #[must_use]
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            x0: x,
            y0: y,
            x1: x + w,
            y1: y + h,
        }
    }

    /// Whether the two overlap.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        !(self.x0 >= other.x1 || other.x0 >= self.x1 || self.y0 >= other.y1 || other.y0 >= self.y1)
    }
}

/// The 1024-byte framebuffer.
///
/// Page-major, vertical, LSB-on-top — see the module docs. `Default` is an
/// empty (all-zero) buffer, which is also a cleared OLED: an SSD1306 pixel is
/// lit when its bit is **zero**, and U8g2 inverts the whole thing on the way
/// out, so "0" here means "off" here and "on" on the glass.
#[derive(Clone)]
pub struct Framebuffer {
    bytes: [u8; BUFFER_LEN],
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self {
            bytes: [0; BUFFER_LEN],
        }
    }
}

impl core::fmt::Debug for Framebuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let lit = self.lit_pixels().count();
        f.debug_struct("Framebuffer")
            .field("bytes", &self.bytes.len())
            .field("lit", &lit)
            .finish()
    }
}

impl Framebuffer {
    /// Zero the whole buffer. `u8g2_ClearBuffer`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The raw bytes, in U8g2's page layout.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; BUFFER_LEN] {
        &self.bytes
    }

    /// Set one *physical* pixel, ignoring rotation and clipping.
    ///
    /// `x` and `y` are physical panel coordinates. Out-of-range writes are
    /// dropped, which is the only bounds check in the whole file — every other
    /// layer relies on U8g2's clip window.
    pub fn set_pixel(&mut self, x: i32, y: i32) {
        if !(0..DISPLAY_WIDTH).contains(&x) || !(0..DISPLAY_HEIGHT).contains(&y) {
            return;
        }
        // The bounds check above makes both casts lossless; `usize` indexing is
        // the only way to address the array without a panic path in the hot loop.
        let ux = x.unsigned_abs() as usize;
        let uy = y.unsigned_abs() as usize;
        let idx = (uy >> 3) * (DISPLAY_WIDTH as usize) + ux;
        self.bytes[idx] |= 1 << (uy & 7);
    }

    /// Read one *physical* pixel.
    #[must_use]
    pub fn pixel(&self, x: i32, y: i32) -> bool {
        if !(0..DISPLAY_WIDTH).contains(&x) || !(0..DISPLAY_HEIGHT).contains(&y) {
            return false;
        }
        let ux = x.unsigned_abs() as usize;
        let uy = y.unsigned_abs() as usize;
        let idx = (uy >> 3) * (DISPLAY_WIDTH as usize) + ux;
        self.bytes[idx] & (1 << (uy & 7)) != 0
    }

    /// The bounding box of every lit pixel, inclusive. `None` if nothing is lit.
    #[must_use]
    pub fn lit_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;
        for y in 0..DISPLAY_HEIGHT {
            for x in 0..DISPLAY_WIDTH {
                if self.pixel(x, y) {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        if min_x > max_x {
            None
        } else {
            Some((min_x, min_y, max_x, max_y))
        }
    }

    /// Iterate every lit pixel as `(x, y)`.
    pub fn lit_pixels(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        (0..DISPLAY_HEIGHT)
            .flat_map(|y| (0..DISPLAY_WIDTH).map(move |x| (x, y)))
            .filter(move |&(x, y)| self.pixel(x, y))
    }

    /// The number of lit pixels. Cheap enough for a test to assert on.
    #[must_use]
    pub fn lit_count(&self) -> usize {
        self.lit_pixels().count()
    }
}

/// The U8g2 draw surface: a framebuffer plus the drawing state U8g2 carries.
///
/// The state that U8g2 keeps in `u8g2_t` and that the templates observe is
/// reproduced here: the current font, the font height mode, the font position
/// mode, the draw colour, the text cursor, the clip window and the rotation.
/// `setPowerSave` is a *request to the panel*, not a drawing state, so it is a
/// field the driver reads and this crate never acts on.
#[derive(Clone)]
pub struct Display {
    fb: Framebuffer,
    font: Option<Font>,
    height_mode: HeightMode,
    font_pos_top: bool,
    draw_color: u8,
    x: i32,
    y: i32,
    rotation: Rotation,
    /// Panel power-save request. `OledDriver`/`IDisplayManager` own acting on it.
    power_save: bool,
}

impl Default for Display {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for Display {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Display")
            .field("buffer", &self.fb)
            .field("font", &self.font)
            .field("height_mode", &self.height_mode)
            .field("font_pos_top", &self.font_pos_top)
            .field("rotation", &self.rotation)
            .field("draw_color", &self.draw_color)
            .field("cursor", &(self.x, self.y))
            .field("power_save", &self.power_save)
            .finish()
    }
}

impl Display {
    /// A cleared display, R0, with no font set.
    ///
    /// Matches `u8g2_SetupBuffer`: `draw_color = 1`, no font, baseline position.
    ///
    /// The ref-height default is `Text`, and it is load-bearing.
    /// `u8g2_setup.c:88` sets `font_height_mode = 0`, and
    /// `u8g2.h:1949` defines `0` as `U8G2_FONT_HEIGHT_MODE_TEXT`. The other two
    /// modes take a *larger* reference ascent: for `profont10`, `ExtendedText`
    /// uses `ascent_para = 7` where `Text` uses `ascent_A = 6`. Since
    /// `setFontPosTop` offsets every glyph by `ref_ascent + 1`
    /// (`u8g2_font_calc_vref_top`), picking the wrong default shifts every line
    /// of text down by a pixel, and nothing else notices because every row
    /// shifts with it. `tests/parity.rs` catches it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fb: Framebuffer::new(),
            font: None,
            height_mode: HeightMode::Text,
            // U8g2's default after `u8g2_SetupBuffer` is
            // `u8g2_SetFontPosBaseline` (u8g2_setup.c:110). The firmware
            // immediately calls `setFontPosTop()` in `prepareDisplay`, so this
            // default is only observable if a caller skips that.
            font_pos_top: false,
            draw_color: 1,
            x: 0,
            y: 0,
            rotation: Rotation::R0,
            power_save: false,
        }
    }

    /// The framebuffer, borrowed.
    ///
    /// The counterpart to [`Display::into_framebuffer`], and the one the
    /// firmware uses: the control task renders every 100 ms for the life of the
    /// process, so consuming the `Display` would mean building a fresh 1 KB
    /// `Framebuffer` every frame. Borrowing keeps one scratch buffer, which is
    /// what ADR-0002 wants anyway — a large buffer allocated once and measured,
    /// rather than churned where the heap report cannot see it.
    #[must_use]
    pub const fn framebuffer(&self) -> &Framebuffer {
        &self.fb
    }

    /// Consume the display and take its framebuffer.
    #[must_use]
    pub fn into_framebuffer(self) -> Framebuffer {
        self.fb
    }

    /// `clearBuffer`.
    pub fn clear_buffer(&mut self) {
        self.fb = Framebuffer::new();
    }

    /// `setPowerSave`. Recorded, not acted on — see R3-09.
    pub fn set_power_save(&mut self, enabled: bool) {
        self.power_save = enabled;
    }

    /// Whether [`Display::set_power_save`] was last called with `true`.
    #[must_use]
    pub const fn power_save(&self) -> bool {
        self.power_save
    }

    /// `setFontRefHeightExtendedText`.
    pub fn set_font_ref_height_extended_text(&mut self) {
        self.height_mode = HeightMode::ExtendedText;
    }

    /// `setFontRefHeightText`.
    pub fn set_font_ref_height_text(&mut self) {
        self.height_mode = HeightMode::Text;
    }

    /// `setFontRefHeightAll`.
    pub fn set_font_ref_height_all(&mut self) {
        self.height_mode = HeightMode::All;
    }

    /// `setFontPosTop`.
    pub fn set_font_pos_top(&mut self) {
        self.font_pos_top = true;
    }

    /// `setFontPosBaseline`.
    pub fn set_font_pos_baseline(&mut self) {
        self.font_pos_top = false;
    }

    /// `OledDriver::prepareDisplay` (`src/ui/OledDriver.cpp:34`).
    ///
    /// The firmware runs this once, when the display is first acquired, and
    /// every template then draws on top of whatever state it leaves. Two of the
    /// five lines are load-bearing and neither is the default:
    ///
    /// - `setFontRefHeightExtendedText()` -- the ref ascent becomes
    ///   `ascent_para` rather than `ascent_A` (7 instead of 6 in `profont10`),
    ///   which changes `getFontAscent`, the row height, and therefore every
    ///   `setFontPosTop` baseline.
    /// - `setFontPosTop()` -- without it `y` is a *baseline* and a glyph whose
    ///   box is taller than `y` is drawn half off the top of the display. The
    ///   Modern template's `fub20` temperature at y=14 lands at rows -9..13 that
    ///   way, and the first goldens showed a clipped readout.
    ///
    /// Idempotent, and cheap: it sets six fields. The pipeline calls it once per
    /// frame rather than once at boot, which is a superset of the C++'s
    /// behaviour -- no template can observe the difference, because each one
    /// sets the font it needs anyway -- and it keeps the rotation and the
    /// initial state in one place instead of two.
    pub fn prepare_display(&mut self, rotation: Rotation) {
        self.clear_buffer();
        self.font = Some(crate::font::profont11());
        self.set_font_ref_height_extended_text();
        self.draw_color = 1;
        self.set_font_pos_top();
        self.set_display_rotation(rotation);
    }

    /// `setDrawColor`. Values `>= 3` become 1, as U8g2 does
    /// (`u8g2_SetDrawColor`, `u8g2_box.c`).
    pub fn set_draw_color(&mut self, color: u8) {
        self.draw_color = if color >= 3 { 1 } else { color };
    }

    /// The current draw colour.
    #[must_use]
    pub const fn draw_color(&self) -> u8 {
        self.draw_color
    }

    /// `setFont`.
    pub fn set_font(&mut self, font: Font) {
        self.font = Some(font);
    }

    /// The current font, or `None`.
    #[must_use]
    pub const fn font(&self) -> Option<Font> {
        self.font
    }

    /// `setDisplayRotation`.
    ///
    /// Only the coordinate space and the line transform change; the buffer
    /// layout does not, which is why this can be called after drawing has
    /// started (the firmware does exactly that, in `prepareDisplay`).
    pub fn set_display_rotation(&mut self, rotation: Rotation) {
        self.rotation = rotation;
    }

    /// The current rotation.
    #[must_use]
    pub const fn rotation(&self) -> Rotation {
        self.rotation
    }

    /// `getDisplayWidth` — the *logical* width.
    #[must_use]
    pub const fn display_width(&self) -> i32 {
        self.rotation.width()
    }

    /// `getDisplayHeight` — the *logical* height.
    #[must_use]
    pub const fn display_height(&self) -> i32 {
        self.rotation.height()
    }

    /// `setCursor`.
    pub fn set_cursor(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    /// `getCursorX`.
    #[must_use]
    pub const fn cursor_x(&self) -> i32 {
        self.x
    }

    /// `getCursorY`.
    #[must_use]
    pub const fn cursor_y(&self) -> i32 {
        self.y
    }

    // ------------------------------------------------------------- clipping

    /// U8g2's clip window, derived from the rotation
    /// (`u8g2_update_page_win_r0..r3`).
    ///
    /// Reproduced as a window rather than as the individual arm it came from,
    /// because all four arms reduce to "the whole logical screen" for a
    /// single-page buffer: `buf_y0` is 0 and `buf_y1` is 64 for R0/R1, and
    /// `height - buf_y0` / `height - buf_y1` are likewise the full extent for
    /// R2/R3.
    #[must_use]
    pub const fn clip_window(&self) -> (i32, i32, i32, i32) {
        (0, 0, self.rotation.width(), self.rotation.height())
    }

    /// `u8g2_IsIntersection` / `u8g2_is_intersection_decision_tree`
    /// (`u8g2_intersection.c:122`), on `u8g2_uint_t` (16-bit unsigned)
    /// coordinates.
    ///
    /// `box_*` is the shape being tested and `win_*` is the clip window; U8g2's
    /// argument names are `a*` for the window and `v*` for the value, and the
    /// order is kept here so the two can be read side by side.
    ///
    /// # Why this is not [`Rect::intersects`]
    ///
    /// **A range that wraps counts as intersecting.** The decision tree has an
    /// explicit `if (v0 > v1) return 1` arm in both of its branches, and
    /// `v0 > v1` is exactly what a *wrapped* range looks like, because
    /// `u8g2_uint_t` is `uint16_t` and a glyph box that starts above the
    /// display underflows. U8g2 therefore does not drop such a glyph: it draws
    /// it, and the per-line `y >= height` check in [`Self::draw_hv_line`]
    /// drops only the individual rows still above the screen. Tall text near
    /// the top of the display is *clipped*, not dropped -- a 30 px digit at a
    /// baseline of y=12 shows its bottom 12 rows instead of vanishing.
    ///
    /// [`Rect::intersects`] takes `i32`, so it cannot represent a wrapped range
    /// at all: it reports "no overlap" and the glyph disappears.
    ///
    /// **The boundaries are asymmetric.** The tree tests `v0 < a1` and
    /// `v1 > a0`, so both upper limits are *excluded* and a box that only
    /// touches the window edge does not intersect.
    #[allow(
        clippy::too_many_arguments,
        reason = "the eight coordinates mirror U8g2's own (x0, y0, x1, y1) argument names for the two rectangles; collapsing them into two Rect values would hide which one is the window"
    )]
    fn is_intersection(shape: Rect, win: Rect) -> bool {
        Self::intersection_tree(win.y0, win.y1, shape.y0, shape.y1)
            && Self::intersection_tree(win.x0, win.x1, shape.x0, shape.x1)
    }

    /// `u8g2_is_intersection_decision_tree`, one axis.
    ///
    /// `a*` is the window, `v*` the value. The `v0 > v1` arms are the
    /// wrapped-range case, which is the whole point of taking `u8g2_uint_t`
    /// rather than `i32` -- see [`Self::is_intersection`].
    fn intersection_tree(a0: i32, a1: i32, v0: i32, v1: i32) -> bool {
        if Self::u16_of(v0) < Self::u16_of(a1) {
            if Self::u16_of(v1) > Self::u16_of(a0) {
                return true;
            }
            return Self::u16_of(v0) > Self::u16_of(v1);
        }
        if Self::u16_of(v1) > Self::u16_of(a0) {
            return Self::u16_of(v0) > Self::u16_of(v1);
        }
        false
    }

    /// Truncate to `u8g2_uint_t`, which is 16 bits unless `U8G2_16BIT` is off.
    ///
    /// A negative logical coordinate is exactly how U8g2's arithmetic spells
    /// "above the top of the screen", and that is what makes the wrapped-range
    /// arm reachable.
    const fn u16_of(v: i32) -> u16 {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the truncation to 16 bits IS the u8g2_uint_t cast"
        )]
        {
            v as u16
        }
    }

    /// `u8g2_clip_intersection2` (`u8g2_hvline.c`): clip `[a, a+len)` against
    /// `[c, d)`, returning the new start and length, or `None` if empty.
    ///
    /// The `a > b` repair arm matters. U8g2's callers can pass a negative
    /// start — `drawFrame` with `w == 0` computes `x += w; x--`, and
    /// `u8g2_draw_l90_r1` produces wrapped values — and without the repair the
    /// subtraction underflows a `u8g2_uint_t`. Ported rather than "cleaned up",
    /// because the two behaviours differ for exactly those inputs.
    fn clip_intersection2(a: i32, len: i32, c: i32, d: i32) -> Option<(i32, i32)> {
        let mut a = a;
        let mut b = a.wrapping_add(len);
        if a > b {
            // U8g2: if a < d, clamp b down to d-1; otherwise move a to c.
            if a < d {
                b = d - 1;
            } else {
                a = c;
            }
        }
        if a >= d {
            return None;
        }
        if b <= c {
            return None;
        }
        if a < c {
            a = c;
        }
        if b > d {
            b = d;
        }
        Some((a, b - a))
    }

    // ---------------------------------------------------------- line drawing

    /// `u8g2_DrawHVLine` — the primitive everything else is built from.
    ///
    /// Clips against the user window, applies the rotation transform, then
    /// writes into the page buffer. `dir` is 0 for left-to-right and 1 for
    /// top-to-bottom; 2 and 3 (right-to-left, bottom-to-top) are normalised
    /// here exactly as U8g2 does.
    pub fn draw_hv_line(&mut self, x: i32, y: i32, len: i32, dir: u8) {
        if len == 0 {
            return;
        }
        let (mut x, mut y, mut dir) = (x, y, dir);
        if len > 1 {
            if dir == 2 {
                x -= len - 1;
            } else if dir == 3 {
                y -= len - 1;
            }
        }
        dir &= 1;

        let (cx0, cy0, cx1, cy1) = self.clip_window();
        let clipped = if dir == 0 {
            if y < cy0 || y >= cy1 {
                return;
            }
            Self::clip_intersection2(x, len, cx0, cx1)
        } else {
            if x < cx0 || x >= cx1 {
                return;
            }
            Self::clip_intersection2(y, len, cy0, cy1)
        };
        let Some((start, n)) = clipped else { return };

        // `start` is the clipped coordinate along the line's own axis: the x
        // for a horizontal line, the y for a vertical one. The other axis is
        // whatever the caller passed, already bounds-checked above.
        let (px, py) = if dir == 0 { (start, y) } else { (x, start) };
        self.hv_line_physical(px, py, n, dir, self.draw_color);
    }

    /// The rotation transform, then the buffer write.
    ///
    /// This is `u8g2_draw_l90_r0..r3` (`u8g2_setup.c:309-441`) followed by
    /// `u8g2_draw_hv_line_2dir`. The R1/R3 arms both *flip* `dir`, which is why
    /// a horizontal line in portrait space lands as a vertical line in the
    /// page buffer.
    fn hv_line_physical(&mut self, x: i32, y: i32, len: i32, dir: u8, color: u8) {
        let (px, py, pdir) = match self.rotation {
            Rotation::R0 => (x, y, dir),
            Rotation::R1 => {
                // yy = x; xx = height - y - 1; dir ^= 1; if dir became 2 (was 1)
                // then xx -= len - 1 and dir = 0.
                let mut xx = self.rotation.height() - y - 1;
                let mut d = dir + 1;
                if d == 2 {
                    xx -= len - 1;
                    d = 0;
                }
                (xx, x, d)
            }
            Rotation::R2 => {
                // yy = height - y; xx = width - x; then per dir:
                //   0: yy -= 1, xx -= len
                //   1: xx -= 1, yy -= len
                let mut xx = self.rotation.width() - x;
                let mut yy = self.rotation.height() - y;
                if dir == 0 {
                    yy -= 1;
                    xx -= len;
                } else {
                    xx -= 1;
                    yy -= len;
                }
                (xx, yy, dir)
            }
            Rotation::R3 => {
                // xx = y; yy = width - x; then per dir:
                //   0: yy -= 1, yy -= len - 1, dir = 1
                //   1: yy -= 1, dir = 0
                let mut yy = self.rotation.width() - x;
                if dir == 0 {
                    yy -= 1;
                    yy -= len - 1;
                    (y, yy, 1)
                } else {
                    yy -= 1;
                    (y, yy, 0)
                }
            }
        };

        if pdir == 0 {
            for i in 0..len {
                self.set_physical(px + i, py, color);
            }
        } else {
            for i in 0..len {
                self.set_physical(px, py + i, color);
            }
        }
    }

    /// One physical pixel, honouring `color` (0 clears, 1 sets, 2 XORs).
    fn set_physical(&mut self, x: i32, y: i32, color: u8) {
        if !(0..DISPLAY_WIDTH).contains(&x) || !(0..DISPLAY_HEIGHT).contains(&y) {
            return;
        }
        let ux = x.unsigned_abs() as usize;
        let uy = y.unsigned_abs() as usize;
        let idx = (uy >> 3) * (DISPLAY_WIDTH as usize) + ux;
        let bit = 1u8 << (uy & 7);
        match color {
            0 => self.fb.bytes[idx] &= !bit,
            1 => self.fb.bytes[idx] |= bit,
            _ => self.fb.bytes[idx] ^= bit,
        }
    }

    /// `drawHLine`.
    pub fn draw_h_line(&mut self, x: i32, y: i32, len: i32) {
        self.draw_hv_line(x, y, len, 0);
    }

    /// `drawVLine`.
    pub fn draw_v_line(&mut self, x: i32, y: i32, len: i32) {
        self.draw_hv_line(x, y, len, 1);
    }

    /// `drawPixel`.
    pub fn draw_pixel(&mut self, x: i32, y: i32) {
        // U8g2's DrawPixel re-checks all four bounds before delegating
        // (u8g2_hvline.c), which matters because the delegation itself only
        // checks one axis.
        let (x0, y0, x1, y1) = self.clip_window();
        if y < y0 || y >= y1 || x < x0 || x >= x1 {
            return;
        }
        self.draw_hv_line(x, y, 1, 0);
    }

    /// `drawLine` — Bresenham, with U8g2's axis-swap and its `x2 == 255`
    /// guard.
    ///
    /// Ported from `u8g2_line.c`. The `x2 == 255` clamp exists because the
    /// loop condition is `x <= x2` on an 8-bit coordinate and 255 would wrap;
    /// it is reproduced because it changes the last pixel of a line that ends
    /// exactly at the right edge.
    pub fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let (mut x1, mut y1, mut x2, mut y2) = (x1, y1, x2, y2);
        let mut dx = (x1 - x2).abs();
        let mut dy = (y1 - y2).abs();
        let mut swap_xy = false;

        if dy > dx {
            swap_xy = true;
            core::mem::swap(&mut dx, &mut dy);
            core::mem::swap(&mut x1, &mut y1);
            core::mem::swap(&mut x2, &mut y2);
        }
        if x1 > x2 {
            core::mem::swap(&mut x1, &mut x2);
            core::mem::swap(&mut y1, &mut y2);
        }

        let mut err = dx >> 1;
        let ystep: i32 = if y2 > y1 { 1 } else { -1 };
        let mut y = y1;

        // U8g2's guard against a 255 wrap in the 8-bit loop counter.
        let x2 = if x2 == 255 { 254 } else { x2 };

        let mut x = x1;
        while x <= x2 {
            if swap_xy {
                self.draw_pixel(y, x);
            } else {
                self.draw_pixel(x, y);
            }
            err -= dy;
            if err < 0 {
                y += ystep;
                err += dx;
            }
            x += 1;
        }
    }

    /// `drawBox` — a filled rectangle. Does nothing for `w == 0` or `h == 0`.
    pub fn draw_box(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if !Self::intersects(Rect::new(x, y, w, h), self.clip_rect()) {
            return;
        }
        let mut y = y;
        let mut h = h;
        while h != 0 {
            self.draw_hv_line(x, y, w, 0);
            y += 1;
            h -= 1;
        }
    }

    /// `drawFrame` — a hollow rectangle. Does nothing for `w == 0` or `h == 0`.
    pub fn draw_frame(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if !Self::intersects(Rect::new(x, y, w, h), self.clip_rect()) {
            return;
        }
        let x_orig = x;
        self.draw_hv_line(x, y, w, 0);
        if h >= 2 {
            let h = h - 2;
            let mut y = y + 1;
            if h > 0 {
                self.draw_hv_line(x, y, h, 1);
                let x = x + w - 1;
                self.draw_hv_line(x, y, h, 1);
                y += h;
            }
            self.draw_hv_line(x_orig, y, w, 0);
        }
    }

    /// `u8g2_IsIntersection` (`u8g2_intersection.c`).
    ///
    /// Half-open on both axes, as U8g2 is: a zero-width box never intersects.
    /// Taken as two [`Rect`]s rather than eight `i32`s so the call sites read
    /// as the shape they are testing.
    #[must_use]
    fn intersects(a: Rect, b: Rect) -> bool {
        if a.x0 >= b.x1 || b.x0 >= a.x1 {
            return false;
        }
        if a.y0 >= b.y1 || b.y0 >= a.y1 {
            return false;
        }
        true
    }

    /// The clip window as a [`Rect`].
    #[must_use]
    fn clip_rect(&self) -> Rect {
        let (x0, y0, x1, y1) = self.clip_window();
        Rect { x0, y0, x1, y1 }
    }

    /// The clip window a *non-rotated* draw is tested against.
    ///
    /// The disc, the circle and the bitmap all use this, and all three test
    /// against the physical 128x64 rather than the logical space. That is
    /// U8g2's behaviour and it is load-bearing under `R1`/`R3`: the disc and
    /// the XBMPs are drawn by the fullscreen screens, which are the screens
    /// that do run portrait, and a logical-space test would reject a logo that
    /// is entirely inside the panel.
    #[must_use]
    const fn physical_rect() -> Rect {
        Rect {
            x0: 0,
            y0: 0,
            x1: DISPLAY_WIDTH,
            y1: DISPLAY_HEIGHT,
        }
    }

    /// `drawDisc` — a filled circle, midpoint algorithm.
    ///
    /// Ported from `u8g2_circle.c`'s `u8g2_draw_disc`, including the fact that
    /// it is built from *vertical* lines of `y+1` and `x+1` — which is what
    /// makes a filled disc solid rather than a ring.
    pub fn draw_disc(&mut self, x0: i32, y0: i32, rad: i32) {
        if !Self::intersects(
            Rect::new(x0 - rad, y0 - rad, 2 * rad + 1, 2 * rad + 1),
            Display::physical_rect(),
        ) {
            return;
        }
        let mut f = 1 - rad;
        let mut dd_f_x = 1;
        let mut dd_f_y = -2 * rad;
        let mut x = 0;
        let mut y = rad;
        self.disc_section(x0, y0, x, y);
        while x < y {
            if f >= 0 {
                y -= 1;
                dd_f_y += 2;
                f += dd_f_y;
            }
            x += 1;
            dd_f_x += 2;
            f += dd_f_x;
            self.disc_section(x0, y0, x, y);
        }
    }

    fn disc_section(&mut self, x0: i32, y0: i32, x: i32, y: i32) {
        // `u8g2_draw_disc_section` with `U8G2_DRAW_ALL`, i.e. all four
        // quadrants. **Eight** vertical lines, not twelve.
        //
        // The lower quadrants start at `y0` and run *down* for `y + 1`, so they
        // already reach the bottom of the disc. An earlier version of this also
        // drew the mirrored `y0 + y` lines as well, which over-drew the bottom
        // edge by up to a pixel on the steep side of the circle. The disc is
        // the Modern template's heating icon, so that is a visible pixel.
        //
        // The line lengths alternate: `y + 1` on the axis-aligned arm and
        // `x + 1` on the diagonal one. That asymmetry is U8g2's and is what
        // makes the fill solid rather than leaving a seam.
        self.draw_v_line(x0 + x, y0 - y, y + 1); // upper right
        self.draw_v_line(x0 + y, y0 - x, x + 1);
        self.draw_v_line(x0 - x, y0 - y, y + 1); // upper left
        self.draw_v_line(x0 - y, y0 - x, x + 1);
        self.draw_v_line(x0 + x, y0, y + 1); // lower right
        self.draw_v_line(x0 + y, y0, x + 1);
        self.draw_v_line(x0 - x, y0, y + 1); // lower left
        self.draw_v_line(x0 - y, y0, x + 1);
    }

    /// `drawCircle` — a circle outline, midpoint algorithm.
    pub fn draw_circle(&mut self, x0: i32, y0: i32, rad: i32) {
        if !Self::intersects(
            Rect::new(x0 - rad, y0 - rad, 2 * rad + 1, 2 * rad + 1),
            Display::physical_rect(),
        ) {
            return;
        }
        let mut f = 1 - rad;
        let mut dd_f_x = 1;
        let mut dd_f_y = -2 * rad;
        let mut x = 0;
        let mut y = rad;
        self.circle_section(x0, y0, x, y);
        while x < y {
            if f >= 0 {
                y -= 1;
                dd_f_y += 2;
                f += dd_f_y;
            }
            x += 1;
            dd_f_x += 2;
            f += dd_f_x;
            self.circle_section(x0, y0, x, y);
        }
    }

    fn circle_section(&mut self, x0: i32, y0: i32, x: i32, y: i32) {
        // `u8g2_draw_circle_section` with `U8G2_DRAW_ALL`: **eight** pixels,
        // two per quadrant.
        //
        // Both quadrants' second pixel is the diagonal partner -- `(x0 +- y,
        // y0 - x)` for the upper pair, `(x0 +- y, y0 + x)` for the lower. An
        // earlier version had only the six axis-aligned ones, which left a gap
        // on the shallow side of the circle wherever `x != y`.
        self.draw_pixel(x0 + x, y0 - y); // upper right
        self.draw_pixel(x0 + y, y0 - x);
        self.draw_pixel(x0 - x, y0 - y); // upper left
        self.draw_pixel(x0 - y, y0 - x);
        self.draw_pixel(x0 + x, y0 + y); // lower right
        self.draw_pixel(x0 + y, y0 + x);
        self.draw_pixel(x0 - x, y0 + y); // lower left
        self.draw_pixel(x0 - y, y0 + x);
    }

    /// `drawTriangle` — filled, via U8g2's convex-polygon scanline filler.
    ///
    /// Ported from `u8g2_polygon.c`. The polygon machinery exists to fill a
    /// triangle with no seams; `ModernTemplate::drawHeatingIcon` uses it, and
    /// a naive three-edge fill leaves gaps on the shallow side of the flame.
    pub fn draw_triangle(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, x2: i32, y2: i32) {
        self.fill_convex_polygon(&[(x0, y0), (x1, y1), (x2, y2)]);
    }

    /// U8g2's `pg_prepare`/`pg_exec` for a small convex polygon.
    ///
    /// Ported from `u8g2_polygon.c`. The machinery exists to fill a triangle
    /// with no seams: `ModernTemplate::drawHeatingIcon` uses it, and filling
    /// from the three edges directly leaves gaps on the shallow side of the
    /// flame, because the two sides step in x at different rates than the
    /// scanline steps in y.
    ///
    /// The polygon is filled between two edge walkers, one per side, each
    /// advancing one scanline per row with a running error accumulator.
    ///
    /// # The min-y bookkeeping, which is easy to get backwards
    ///
    /// U8g2 walks each side's start index off the run of vertices at the
    /// minimum y (`pg_expand_min_y`), then compares the two resulting x values.
    /// Its flag is named `is_min_y_not_flat`, and the initialiser is:
    ///
    /// ```c
    /// pg->is_min_y_not_flat = 1;
    /// if ( pg->list[LEFT].x != pg->list[RIGHT].x ) pg->is_min_y_not_flat = 0;
    /// else { pg->total_scan_line_cnt--; ... }
    /// ```
    ///
    /// so the flag is 1 when the two starts have the **same** x — which is the
    /// *apex* case, where the minimum y is a single vertex, not a flat edge. It
    /// reads backwards, and inverting it shifts the whole fill by one scanline.
    /// When it is set, the fill also *pre-advances* both edges by one row, so
    /// the first scanline drawn is `min_y + 1`.
    fn fill_convex_polygon(&mut self, points: &[(i32, i32)]) {
        if points.len() < 3 {
            return;
        }
        let poly = Polygon { points };

        // `pg_prepare`: find max/min y and the index of the minimum.
        let mut left_start = 0i32;
        let mut min_y = poly.at(0).1;
        let mut max_y = min_y;
        for i in 1..poly.len() {
            let y = poly.at(i).1;
            if max_y < y {
                max_y = y;
            }
            if min_y > y {
                left_start = i;
                min_y = y;
            }
        }

        let mut lines = max_y - min_y;
        if lines == 0 {
            return;
        }

        let right_start = poly.expand_min_y(left_start, 1, min_y);
        left_start = poly.expand_min_y(left_start, -1, min_y);

        let is_min_y_not_flat = poly.at(left_start).0 == poly.at(right_start).0;
        if is_min_y_not_flat {
            lines -= 1;
            if lines == 0 {
                return;
            }
        }

        let mut left = Edge::new(&poly, left_start, -1);
        let mut right = Edge::new(&poly, right_start, 1);
        if is_min_y_not_flat {
            left.next();
            right.next();
        }

        let mut remaining = lines;
        loop {
            // `pg_hline`: fill between the two walkers on the *right* walker's
            // row. The bounds handling is U8g2's verbatim, including the
            // `a = 0` / `a = W` reassignments in the `x1 >= x2` arm, which clamp
            // rather than reject.
            let y = right.current_y;
            if y >= 0 && y < self.rotation.height() {
                let x1 = left.current_x;
                let x2 = right.current_x;
                let w = self.rotation.width();
                if x1 < x2 {
                    if x2 >= 0 && x1 < w {
                        let a = x1.max(0);
                        let b = x2.min(w);
                        self.draw_h_line(a, y, b - a);
                    }
                } else if x1 >= 0 && x2 < w {
                    let mut a = x1;
                    let b = x2;
                    if b < 0 {
                        a = 0;
                    }
                    if a >= w {
                        a = w;
                    }
                    self.draw_h_line(b, y, a - b);
                }
            }

            // Re-seed a finished side from its last vertex, then step both.
            while !left.next() {
                left = Edge::new(&poly, left.idx, -1);
            }
            while !right.next() {
                right = Edge::new(&poly, right.idx, 1);
            }
            remaining -= 1;
            if remaining <= 0 {
                break;
            }
        }
    }

    // -------------------------------------------------------------- bitmaps

    /// `drawXBMP` — a big-endian 1bpp bitmap, `blen`-bytes per row.
    ///
    /// Ported from `u8g2_bitmap.c`'s `u8g2_DrawHXBMP` + `u8g2_DrawXBMP`,
    /// including the run-length form of the inner loop (U8g2 replaced the
    /// per-pixel version) and the fact that unset bits paint the *background*
    /// colour unless transparency is on. Transparency is off, as on the device.
    pub fn draw_xbmp(&mut self, x: i32, y: i32, w: i32, h: i32, mut bitmap: &[u8]) {
        let blen = (w + 7) >> 3;
        if !Self::intersects(Rect::new(x, y, w, h), Display::physical_rect()) {
            return;
        }
        let color = self.draw_color;
        let ncolor = u8::from(color == 0);
        let mut y = y;
        let mut h = h;
        while h > 0 {
            self.hxbmp_row(x, y, w, bitmap, color, ncolor);
            let stride = blen.unsigned_abs() as usize;
            bitmap = &bitmap[stride.min(bitmap.len())..];
            y += 1;
            h -= 1;
        }
    }

    /// One row of `u8g2_DrawHXBMP`, ported from `u8g2_bitmap.c`.
    ///
    /// U8g2's current form (the `#else` arm, with `#define OLD` commented out)
    /// scans forward over a run of *identical* bits, then draws the run in one
    /// `u8g2_DrawHVLine`. Both bit values paint — with the current colour for a
    /// set bit and the inverse for a clear one, because `bitmap_transparency`
    /// is 0 on the device (`u8g2_SetupBuffer`, `u8g2_setup.c:94`).
    fn hxbmp_row(&mut self, x0: i32, y: i32, len: i32, b: &[u8], color: u8, ncolor: u8) {
        // `mask = 1` then `mask <<= 1` (u8g2_bitmap.c): bit 0 is the LEFTMOST
        // pixel, so an XBMP row is little-endian within the byte. That is the
        // opposite of the font atlas convention and the reason the bitmaps in
        // `bitmaps_data` keep their original byte order rather than being
        // "helpfully" normalised.
        let mut i = 0usize; // byte index
        let mut mask = 1u8; // mask within that byte
        let mut remaining = len;
        while remaining > 0 {
            if i >= b.len() {
                return;
            }
            let current_bit = (b[i] & mask) != 0;
            let mut run_length = 0i32;
            while remaining > 0 && i < b.len() && ((b[i] & mask) != 0) == current_bit {
                run_length += 1;
                remaining -= 1;
                mask <<= 1;
                if mask == 0 {
                    mask = 1;
                    i += 1;
                }
            }
            self.draw_color = if current_bit { color } else { ncolor };
            self.draw_hv_line(x0 + (len - remaining) - run_length, y, run_length, 0);
        }
        self.draw_color = color;
    }

    // ----------------------------------------------------------------- text

    /// `getStrWidth` for the current font.
    ///
    /// Returns 0 with no font set, matching U8g2's behaviour of dereferencing
    /// a null `font` being undefined — the firmware always sets a font before
    /// measuring, and silently returning 0 would hide a bug rather than crash.
    #[must_use]
    pub fn str_width(&self, text: &str) -> i32 {
        self.font.map_or(0, |f| f.str_width(text))
    }

    /// `getMaxCharHeight` for the current font.
    #[must_use]
    pub fn max_char_height(&self) -> i32 {
        self.font.map_or(0, |f| f.max_char_height())
    }

    /// `getFontAscent` for the current font.
    #[must_use]
    pub fn font_ascent(&self) -> i32 {
        self.font
            .map_or(0, |f| i32::from(f.ref_height(self.height_mode).0))
    }

    /// `u8g2_font_calc_vref_top` — the y offset `setFontPosTop` adds.
    #[must_use]
    fn vref(&self) -> i32 {
        if self.font_pos_top {
            self.font_ascent() + 1
        } else {
            0
        }
    }

    /// `drawStr`. Returns the advance.
    ///
    /// # Code points, not bytes
    ///
    /// U8g2's `drawStr` walks the string a *byte* at a time
    /// (`u8x8_ascii_next`, `u8g2_font.c:1467`) and maps each byte to a glyph.
    /// The firmware's strings are therefore Latin-1: the degree sign is
    /// `static_cast<char>(176)`, a single `0xB0`.
    ///
    /// A Rust `&str` cannot hold a lone `0xB0` — it is not valid UTF-8 — so
    /// `"\u{b0}"` is *two* bytes, and a byte walk would look up `0xC2` and
    /// `0xB0` and measure **30 px instead of 25** in `profont10`. That is a
    /// silent golden-image difference, measured, and the fix is to walk *code
    /// points* and use the code point as the Latin-1 byte. Every string the
    /// firmware draws is ASCII plus that one byte, so the two walks agree on
    /// all real input.
    ///
    /// `Display::draw_utf8` was the same walk, mirroring the C++'s separate
    /// `drawUTF8` entry point. It had no callers and was deleted;
    /// [`Display::draw_str`] is the one string entry point.
    pub fn draw_str(&mut self, x: i32, y: i32, text: &str) -> i32 {
        self.draw_string(x, y, text)
    }

    /// `u8g2_draw_string`, one code point at a time.
    ///
    /// The decoder stops at `\n` — U8g2 does this so one string can hold
    /// several lines, and `displayWrappedMessage` relies on it. See
    /// [`Display::draw_str`] for why this walks code points.
    pub fn draw_string(&mut self, x: i32, y: i32, text: &str) -> i32 {
        let mut x = x;
        let mut sum = 0;
        for c in text.chars() {
            if c == '\n' {
                break;
            }
            let delta = self.draw_glyph(x, y, crate::font::latin1_of(c));
            x += delta;
            sum += delta;
        }
        sum
    }

    /// `u8g2_draw_string` with the UTF-8 decoder (`u8x8_utf8_next`).
    ///
    /// For the Latin-1 text the firmware draws this is identical to
    /// [`Display::draw_string`]; it is kept because `displayWrappedMessage`
    /// calls the C++'s `drawUTF8` explicitly, and because a genuinely non-Latin
    /// code point must be looked up as itself rather than as its UTF-8 bytes.
    pub fn draw_string_utf8(&mut self, x: i32, y: i32, text: &str) -> i32 {
        let mut x = x;
        let mut sum = 0;
        for c in text.chars() {
            if c == '\n' {
                break;
            }
            let delta = self.draw_glyph(x, y, crate::font::code_point_of(c));
            x += delta;
            sum += delta;
        }
        sum
    }

    /// `u8g2_DrawGlyph` — decode and draw one glyph at the pen.
    ///
    /// The glyph is decoded into horizontal runs, and each run is placed at
    /// `pen + (x_offset, -(height + y_offset))` and handed to
    /// [`Display::draw_hv_line`]. Runs with `draw_color == 0` are *cleared*,
    /// which is what makes a glyph's box erase what is behind it; the firmware
    /// runs in U8g2's non-transparent mode, so this is required for parity.
    fn draw_glyph(&mut self, x: i32, y: i32, encoding: u16) -> i32 {
        let Some(font) = self.font else { return 0 };
        let Some(header) = font.glyph_header(encoding) else {
            // U8g2 leaves the pen where it is for a missing glyph.
            return 0;
        };
        if header.width == 0 {
            return i32::from(header.delta_x);
        }

        let target_x = x + i32::from(header.x_offset);
        let target_y = y + self.vref() - i32::from(header.height) - i32::from(header.y_offset);

        // A glyph's own bounding box vs the clip window: U8g2 skips the whole
        // decode when it misses (`u8g2_font.c:640`, `u8g2_IsIntersection`).
        //
        // The test is the 16-bit one, not [`Rect::intersects`], because a box
        // that starts above the screen *wraps* rather than missing -- see
        // [`Self::is_intersection`]. Getting this wrong drops tall text near
        // the top of the display instead of clipping it.
        let (cx0, cy0, cx1, cy1) = self.clip_window();
        if !Self::is_intersection(
            Rect::new(
                target_x,
                target_y,
                i32::from(header.width),
                i32::from(header.height),
            ),
            Rect {
                x0: cx0,
                y0: cy0,
                x1: cx1,
                y1: cy1,
            },
        ) {
            return i32::from(header.delta_x);
        }

        // The run is drawn straight from the decoder, with no intermediate
        // buffer.
        //
        // A buffer looked harmless and was not. The bound was guessed as "~50
        // runs", and that is wrong by an order of magnitude: a `fub30` glyph is
        // up to 31 px wide and 30 px tall, the RLE emits up to three runs per
        // row, and `u8g2_font_decode_len` splits any run that crosses the right
        // edge -- so a single tall glyph can emit several hundred runs. The
        // buffer filled, the `if n < runs.len()` guard dropped the rest, and
        // the bottom of every large glyph silently vanished. `tests/parity.rs`
        // caught it as 163 missing pixels in the bottom rows of `fonts.txt`.
        //
        // Buffering was never necessary. `Font` is `Copy` and holds a
        // `&'static [u8]`, so the `font` binding above is an independent copy
        // and the closure can borrow `self` mutably.
        let fg = self.draw_color;
        let bg = u8::from(fg == 0);
        font.decode_into(&header, |lx, ly, len, is_fg| {
            let saved = self.draw_color;
            self.draw_color = if is_fg { fg } else { bg };
            self.draw_hv_line(
                target_x + i32::from(lx),
                target_y + i32::from(ly),
                i32::from(len),
                0,
            );
            self.draw_color = saved;
        });

        i32::from(header.delta_x)
    }

    /// `print(const char*)` at the cursor: draw and advance the cursor.
    pub fn print(&mut self, text: &str) {
        let advance = self.draw_string(self.x, self.y, text);
        self.x += advance;
    }

    /// `print(char)` at the cursor.
    ///
    /// U8G2 has no `print(char)`: the templates print the degree sign by
    /// casting it (`print(static_cast<char>(176))`), which on the Arduino
    /// `Print` goes through the byte stream as a Latin-1 byte. This takes a
    /// `char` and draws the single code point, which is what the firmware
    /// means by `static_cast<char>(176)` — U+00B0 DEGREE SIGN, not U+00B0's
    /// UTF-8 encoding of two bytes.
    pub fn print_char(&mut self, c: char) {
        // U8G2's encoding is `uint16_t`; a `char` above U+FFFF has no glyph in
        // any of the ten fonts, so it draws nothing and advances nothing --
        // which is what U8g2 does for a missing glyph.
        let advance =
            u16::try_from(u32::from(c)).map_or(0, |code| self.draw_glyph(self.x, self.y, code));
        self.x += advance;
    }
}

/// A small vertex list with U8g2's wrapping index arithmetic.
///
/// `pg_inc`/`pg_dec` in `u8g2_polygon.c` wrap with a `uint8_t` underflow, which
/// is what makes a clockwise and a counter-clockwise vertex order both work.
/// Modelling it as a `Polygon` with `i32` indices keeps the wrap explicit and out
/// of the callers.
struct Polygon<'a> {
    points: &'a [(i32, i32)],
}

impl Polygon<'_> {
    fn len(&self) -> i32 {
        // A `drawTriangle` has 3 points and a `PG_MAX_POINTS` of 6; the cast is
        // bounded by the caller's slice, which is a literal.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "bounded by PG_MAX_POINTS = 6, which U8g2 also asserts"
        )]
        let n = self.points.len() as i32;
        debug_assert!((3..=6).contains(&n), "U8g2's PG_MAX_POINTS is 6");
        n
    }

    /// `points[i]`, wrapping the index.
    fn at(&self, i: i32) -> (i32, i32) {
        let n = self.len();
        let i = if i >= n {
            i - n
        } else if i < 0 {
            i + n
        } else {
            i
        };
        self.points[usize::try_from(i).unwrap_or(0)]
    }

    /// `pg_expand_min_y`: walk off the run of vertices at `min_y`.
    ///
    /// U8g2 steps first, tests, and assigns on the way — so if the very first
    /// step already leaves the min-y run the start index is left where it was.
    /// Getting that wrong shifts the whole fill by a scanline.
    fn expand_min_y(&self, start: i32, step: i32, min_y: i32) -> i32 {
        let mut i = start;
        let mut last = start;
        loop {
            i += step;
            if i >= self.len() {
                i = 0;
            }
            if i < 0 {
                i = self.len() - 1;
            }
            if self.at(i).1 != min_y {
                return last;
            }
            last = i;
        }
    }
}

/// One polygon edge walker, from `pg_edge_struct` in `u8g2_polygon.c`.
struct Edge {
    current_x: i32,
    current_y: i32,
    x_offset: i32,
    x_direction: i32,
    error: i32,
    error_offset: i32,
    height: i32,
    max_y: i32,
    /// The index the *next* [`Edge::new`] must start from.
    idx: i32,
}

impl Edge {
    /// `pge_Init`: walk from `points[idx]` to the next point along `step`.
    ///
    /// The error accumulator is the classic integer DDA; the `1 - height` seed
    /// for a leftward edge biases the very first step, which is why a shallow
    /// edge does not drop its first pixel.
    fn new(poly: &Polygon<'_>, idx: i32, step: i32) -> Self {
        let (x1, y1) = poly.at(idx);
        let next = {
            let mut j = idx + step;
            if j >= poly.len() {
                j = 0;
            }
            if j < 0 {
                j = poly.len() - 1;
            }
            j
        };
        let (x2, y2) = poly.at(next);
        let dx = x2 - x1;
        let height = y2 - y1;
        let (x_direction, width, error) = if dx >= 0 {
            (1, dx, 0)
        } else {
            (-1, -dx, 1 - height)
        };
        Self {
            current_x: x1,
            current_y: y1,
            x_offset: if height != 0 { dx / height } else { 0 },
            error,
            error_offset: if height != 0 { width % height } else { 0 },
            height,
            max_y: y2,
            x_direction,
            idx: next,
        }
    }

    /// `pge_Next`: advance one scanline. `false` when the edge is finished.
    fn next(&mut self) -> bool {
        if self.current_y >= self.max_y {
            return false;
        }
        self.current_x += self.x_offset;
        self.error += self.error_offset;
        if self.error > 0 {
            self.current_x += self.x_direction;
            self.error -= self.height;
        }
        self.current_y += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font;

    /// `(first_x, last_x, count)` of row `y`, or `(0, 0, -1)` if it is empty.
    fn row_extent(fb: &Framebuffer, y: i32) -> (i32, i32, i32) {
        let mut first = None;
        let mut last = 0i32;
        for x in 0..DISPLAY_WIDTH {
            if fb.pixel(x, y) {
                first.get_or_insert(x);
                last = x;
            }
        }
        match first {
            Some(f) => (f, last, last - f + 1),
            None => (0, 0, -1),
        }
    }

    /// Every lit pixel, in a fixed-capacity array sorted by (y, x).
    ///
    /// `no_std` has no `Vec`, and the test module may not grow one.
    fn lit_sorted(fb: &Framebuffer) -> [(i32, i32); 8] {
        let mut out = [(0i32, 0i32); 8];
        let mut n = 0;
        for (x, y) in fb.lit_pixels() {
            out[n] = (x, y);
            n += 1;
        }
        out[..n].sort_unstable_by_key(|&(x, y)| (y, x));
        out
    }

    #[test]
    fn framebuffer_is_all_zero_when_new() {
        let fb = Framebuffer::new();
        assert_eq!(fb.lit_count(), 0);
        assert_eq!(fb.as_bytes().len(), 1024);
    }

    #[test]
    fn set_pixel_uses_page_layout() {
        let mut fb = Framebuffer::new();
        fb.set_pixel(3, 10);
        // Row 10 -> page 1, bit 2.
        // Row 10 is page 1, bit 2 -> 0b0000_0100.
        assert_eq!(fb.as_bytes()[128 + 3], 0b0000_0100);
        assert!(fb.pixel(3, 10));
        assert!(!fb.pixel(3, 9));
    }

    #[test]
    fn out_of_range_pixels_are_dropped_not_wrapped() {
        let mut fb = Framebuffer::new();
        fb.set_pixel(-1, 0);
        fb.set_pixel(128, 0);
        fb.set_pixel(0, -1);
        fb.set_pixel(0, 64);
        assert_eq!(fb.lit_count(), 0);
    }

    #[test]
    fn r0_has_the_physical_dimensions() {
        assert_eq!(Rotation::R0.width(), 128);
        assert_eq!(Rotation::R0.height(), 64);
        assert_eq!(Rotation::R2.width(), 128);
        assert_eq!(Rotation::R2.height(), 64);
    }

    #[test]
    fn r1_and_r3_swap_the_logical_dimensions() {
        // u8g2_update_dimension_r1: width = pixel_height = 64.
        assert_eq!(Rotation::R1.width(), 64);
        assert_eq!(Rotation::R1.height(), 128);
        assert_eq!(Rotation::R3.width(), 64);
        assert_eq!(Rotation::R3.height(), 128);
    }

    #[test]
    fn rotation_index_round_trips_and_falls_back_to_r0() {
        for i in 0..4 {
            assert_eq!(Rotation::from_index(i).index(), i);
        }
        assert_eq!(Rotation::from_index(7), Rotation::R0);
        assert_eq!(Rotation::from_index(-1), Rotation::R0);
    }

    #[test]
    fn the_oled_driver_rotation_rule_is_reproduced() {
        // prepareDisplay: rotation = displayInverted * 2 + (UPRIGHT ? 1 : 0).
        for inverted in [false, true] {
            for upright in [false, true] {
                let mut rotation = 0;
                if inverted {
                    rotation += 2;
                }
                if upright {
                    rotation += 1;
                }
                let expected = match (inverted, upright) {
                    (false, false) => Rotation::R0,
                    (false, true) => Rotation::R1,
                    (true, false) => Rotation::R2,
                    (true, true) => Rotation::R3,
                };
                assert_eq!(
                    Rotation::from_index(rotation),
                    expected,
                    "inverted={inverted} upright={upright}"
                );
            }
        }
    }

    #[test]
    fn r0_is_the_identity_transform() {
        let mut d = Display::new();
        d.draw_h_line(3, 4, 5);
        let fb = d.into_framebuffer();
        for i in 0..5 {
            assert!(fb.pixel(3 + i, 4), "x={}", 3 + i);
        }
        assert!(!fb.pixel(8, 4));
    }

    #[test]
    fn r2_mirrors_both_axes() {
        let mut d = Display::new();
        d.set_display_rotation(Rotation::R2);
        d.draw_h_line(0, 0, 3);
        let fb = d.into_framebuffer();
        // R2: xx = width - x, yy = height - y, then for dir 0: yy -= 1, xx -= len.
        // (0,0) len 3 -> xx = 128, then xx = 125, yy = 64, then yy = 63.
        // So the run lands at buffer x=125..127, y=63.
        for i in 0..3 {
            assert!(fb.pixel(125 + i, 63), "x={}", 125 + i);
        }
    }

    #[test]
    fn a_line_clipped_to_nothing_draws_nothing() {
        let mut d = Display::new();
        d.draw_h_line(0, 100, 10); // y outside the window
        d.draw_h_line(-20, 0, 5); // entirely left of the window
        d.draw_h_line(200, 0, 5); // entirely right
        assert_eq!(d.into_framebuffer().lit_count(), 0);
    }

    #[test]
    fn draw_box_fills_and_draw_frame_outlines() {
        let mut d = Display::new();
        d.draw_box(10, 10, 4, 3);
        let solid = d.into_framebuffer();
        assert_eq!(solid.lit_count(), 12, "4x3 filled");
        assert!(solid.pixel(13, 12));

        let mut d = Display::new();
        d.draw_frame(10, 10, 4, 3);
        let outline = d.into_framebuffer();
        // A 4x3 outline: 2*4 + 2*(3-2) = 10, matching the oracle.
        assert_eq!(outline.lit_count(), 10);
        assert!(!outline.pixel(11, 11), "interior must stay clear");
        assert!(outline.pixel(10, 10) && outline.pixel(13, 12));
    }

    #[test]
    fn a_zero_sized_box_draws_nothing() {
        // U8g2 documents "restriction: does not work for w = 0 or h = 0".
        // Measured against the oracle: `drawBox(5,5,0,4)` and `drawBox(5,5,4,0)`
        // both light 0 pixels -- `drawBox`'s loop runs `h` times over a
        // zero-length line, and each of those is dropped by `len == 0`.
        let mut d = Display::new();
        d.draw_box(5, 5, 0, 4);
        assert_eq!(d.framebuffer().lit_count(), 0);
        let mut d = Display::new();
        d.draw_box(5, 5, 4, 0);
        assert_eq!(d.framebuffer().lit_count(), 0);
    }

    #[test]
    fn a_zero_width_frame_draws_its_side_lines() {
        // The mirror image of the `drawBox` case, and a genuine U8g2 quirk
        // rather than a defect in this port. `drawFrame(5,5,0,4)` lights 4
        // pixels in C++: the top and bottom horizontals are zero-length and
        // vanish, but the two verticals are `drawHVLine(x, y, h-2, 1)` with
        // `h-2 == 2`, which are not. The second vertical lands at
        // `x + w - 1 == 4`, i.e. one column LEFT of the requested x, because
        // U8g2 computes it as `x += w; x--` unconditionally.
        //
        // Reproduced rather than guarded, because a caller that passes w == 0
        // gets what the device gives, and "fixing" it here would desynchronise
        // the oracle. The overlap check also passes: `intersects(5,5,5,9,...)`
        // is true, so U8g2 proceeds to draw.
        let mut d = Display::new();
        d.draw_frame(5, 5, 0, 4);
        assert_eq!(
            lit_sorted(d.framebuffer())[..4],
            [(4, 6), (5, 6), (4, 7), (5, 7)]
        );
    }

    #[test]
    fn a_zero_height_frame_draws_its_top_line() {
        // Oracle: `drawFrame(5,5,4,0)` lights 4 pixels in one row at y=5 --
        // the `h >= 2` arm is skipped, so only the top horizontal survives.
        let mut d = Display::new();
        d.draw_frame(5, 5, 4, 0);
        assert_eq!(
            lit_sorted(d.framebuffer())[..4],
            [(5, 5), (6, 5), (7, 5), (8, 5)]
        );
    }

    #[test]
    fn draw_color_zero_clears() {
        let mut d = Display::new();
        d.draw_box(0, 0, 8, 8);
        assert_eq!(d.framebuffer().lit_count(), 64);
        d.set_draw_color(0);
        d.draw_box(0, 0, 8, 8);
        assert_eq!(d.framebuffer().lit_count(), 0);
    }

    #[test]
    fn draw_color_two_xors() {
        let mut d = Display::new();
        d.draw_box(0, 0, 4, 4);
        d.set_draw_color(2);
        d.draw_box(0, 0, 4, 4);
        assert_eq!(
            d.framebuffer().lit_count(),
            0,
            "XOR of a set region clears it"
        );
    }

    #[test]
    fn an_out_of_range_draw_color_becomes_one() {
        // u8g2_SetDrawColor: `if (color >= 3) draw_color = 1`.
        let mut d = Display::new();
        d.set_draw_color(7);
        assert_eq!(d.draw_color(), 1);
    }

    #[test]
    fn a_glyph_clears_its_own_box() {
        // Non-transparent mode: a glyph paints its background too. Draw a box,
        // then a space-ish glyph over it, and the ink region must be cleared
        // where the glyph has background but not where it has ink.
        let mut d = Display::new();
        d.set_font(font::profont11());
        d.set_font_pos_top();
        d.draw_box(0, 0, 128, 64);
        assert_eq!(d.framebuffer().lit_count(), 128 * 64);
        d.clear_buffer();
        d.draw_box(0, 0, 128, 64);
        d.draw_str(0, 10, "A");
        // Inside A's box there must now be both ink and cleared background.
        let fb = d.into_framebuffer();
        // Oracle: drawing "A" at (0,10) over a fully-lit 128x64 leaves 19
        // pixels dark. That is A's own box: 8173 lit out of 8192.
        assert_eq!(8192 - fb.lit_count(), 19, "A's box must be exactly 19 px");
        let header = font::profont11().glyph_header(u16::from(b'A')).unwrap();
        let mut ink = 0;
        for y in 0..i32::from(header.height) {
            for x in 0..i32::from(header.width) {
                if fb.pixel(x, y) {
                    ink += 1;
                }
            }
        }
        assert!(ink > 0, "A must put down ink");
    }

    #[test]
    fn a_control_character_has_no_glyph_and_does_not_advance() {
        // The ten fonts encode 0x20..0x7E plus a symbol block; 0x01..0x1F are
        // absent from all of them. Verified against the oracle: the whole
        // 0x01..0x1F range is missing from every profont and every fub.
        for f in [
            font::profont10(),
            font::profont11(),
            font::profont12(),
            font::profont15(),
            font::profont17(),
            font::profont22(),
            font::fub17(),
            font::fub20(),
            font::fub25(),
            font::fub30(),
        ] {
            assert!(f.glyph_header(0x01).is_none());
        }
        let mut d = Display::new();
        d.set_font(font::profont11());
        assert_eq!(d.draw_str(0, 0, "\u{1}"), 0);
        assert_eq!(d.str_width("\u{1}"), 0);
    }

    #[test]
    fn a_missing_glyph_after_a_present_one_still_advances_the_pen() {
        // The pen must not stall: "A" then an unencoded byte advances by A's
        // delta, and the total is A's advance, not zero.
        let f = font::profont11();
        let mut d = Display::new();
        d.set_font(f);
        let a = i32::from(f.glyph_header(u16::from(b'A')).unwrap().delta_x);
        assert_eq!(d.draw_str(0, 0, "A\u{1}"), a);
    }

    #[test]
    fn the_degree_sign_is_present_wherever_the_templates_draw_it() {
        // Every template draws the degree sign by casting 176 to `char`, i.e.
        // U+00B0. Enumerated against the oracle: the six profonts all carry it;
        // the fub faces carry it too. Asserted so that swapping a font in a
        // template cannot silently drop the unit (a missing glyph advances the
        // pen by zero, which jams the digits against the "C").
        for f in [
            font::profont10(),
            font::profont11(),
            font::profont12(),
            font::profont15(),
            font::profont17(),
            font::profont22(),
            font::fub17(),
            font::fub20(),
            font::fub25(),
            font::fub30(),
        ] {
            let h = f
                .glyph_header(0x00b0)
                .expect("every template font carries U+00B0");
            assert!(
                h.delta_x > 0,
                "a zero advance would jam the unit against the digits"
            );
        }
    }

    #[test]
    fn print_advances_the_cursor() {
        let f = font::profont11();
        let mut d = Display::new();
        d.set_font(f);
        d.set_cursor(10, 20);
        d.print("12");
        assert_eq!(
            d.cursor_x(),
            10 + 2 * i32::from(f.glyph_header(u16::from(b'1')).unwrap().delta_x)
        );
        assert_eq!(d.cursor_y(), 20);
    }

    #[test]
    fn power_save_is_recorded_not_applied() {
        let mut d = Display::new();
        assert!(!d.power_save());
        d.set_power_save(true);
        assert!(d.power_save());
        d.set_power_save(false);
        assert!(!d.power_save());
    }

    #[test]
    fn a_filled_triangle_matches_the_oracle_scanline_by_scanline() {
        // The heating-icon shape: apex at the top, flat base at the bottom.
        // `min_y` is a single vertex, so U8g2's `is_min_y_not_flat` is SET and
        // the fill pre-advances one row -- the first scanline drawn is y=1, not
        // y=0. Row widths below are the oracle's, pixel for pixel.
        let mut d = Display::new();
        d.draw_triangle(8, 0, 3, 12, 13, 12);
        let fb = d.into_framebuffer();
        assert_eq!(
            row_extent(&fb, 0),
            (0, 0, -1),
            "y=0 is skipped: the pre-advance"
        );
        let expected = [
            (1, 8, 8),
            (2, 8, 8),
            (3, 7, 9),
            (4, 7, 9),
            (5, 6, 10),
            (6, 6, 10),
            (7, 6, 10),
            (8, 5, 11),
            (9, 5, 11),
            (10, 4, 12),
            (11, 4, 12),
        ];
        for &(y, first, last) in &expected {
            let (f, l, n) = row_extent(&fb, y);
            assert_eq!((f, l, n), (first, last, last - first + 1), "y={y}");
        }
        assert_eq!(
            row_extent(&fb, 12),
            (0, 0, -1),
            "y=12 is past the last scanline"
        );
    }

    #[test]
    fn a_flat_minimum_y_fills_the_base_row() {
        // The mirror case: base at the top, apex at the bottom. `min_y` is a
        // flat *edge*, so `is_min_y_not_flat` is clear, no pre-advance happens,
        // and the base row is filled at full width. The oracle's row widths for
        // this shape are 10, 9, 9, 9, 8, 7, 7, 7, 6, 5, 5, 5, 4, 3, 3, 3, 2, 1, 1, 1.
        let mut d = Display::new();
        d.draw_triangle(10, 5, 20, 5, 15, 25);
        let fb = d.into_framebuffer();
        let widths: [i32; 20] =
            core::array::from_fn(|i| row_extent(&fb, 5 + i32::try_from(i).unwrap_or(0)).2);
        assert_eq!(
            widths,
            [10, 9, 9, 9, 8, 7, 7, 7, 6, 5, 5, 5, 4, 3, 3, 3, 2, 1, 1, 1]
        );
        assert_eq!(row_extent(&fb, 5).0, 10);
        assert_eq!(row_extent(&fb, 5).1, 19);
    }

    #[test]
    fn a_degenerate_triangle_draws_nothing() {
        let mut d = Display::new();
        d.draw_triangle(5, 5, 5, 5, 5, 5);
        assert_eq!(d.into_framebuffer().lit_count(), 0);
    }
}
