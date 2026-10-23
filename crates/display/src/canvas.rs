//! A 128×64 1-bit frame with u8g2-style drawing calls (top-aligned text, XBM
//! bitmaps, lines, boxes, circles), in landscape or portrait orientation.

use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle};
use u8g2_fonts::FontRenderer;
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};

use crate::bitmaps::Xbm;

pub const WIDTH: usize = 128;
pub const HEIGHT: usize = 64;
pub const FRAME_BYTES: usize = WIDTH * HEIGHT / 8;

/// How the logical canvas maps onto the panel (u8g2's R0..R3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    R0,
    /// 90° clockwise: portrait, 64 wide × 128 high.
    R90,
    R180,
    R270,
}

impl Rotation {
    pub const fn portrait(self) -> bool {
        matches!(self, Rotation::R90 | Rotation::R270)
    }

    /// Rotation for a template, flipped 180° when the display is mounted upside down.
    pub const fn for_display(portrait: bool, inverted: bool) -> Self {
        match (portrait, inverted) {
            (false, false) => Rotation::R0,
            (true, false) => Rotation::R90,
            (false, true) => Rotation::R180,
            (true, true) => Rotation::R270,
        }
    }
}

/// The pixels, in the controller's page layout: `bytes[page * 128 + x]`, bit 0 the top
/// row of the page.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub bytes: [u8; FRAME_BYTES],
}

impl core::fmt::Debug for Frame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Frame")
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            bytes: [0; FRAME_BYTES],
        }
    }
}

impl Frame {
    /// Physical pixel.
    pub fn get(&self, x: usize, y: usize) -> bool {
        self.bytes[(y / 8) * WIDTH + x] & (1 << (y % 8)) != 0
    }

    fn set(&mut self, x: usize, y: usize, on: bool) {
        let b = &mut self.bytes[(y / 8) * WIDTH + x];
        if on {
            *b |= 1 << (y % 8);
        } else {
            *b &= !(1 << (y % 8));
        }
    }
}

/// Drawing state over a frame.
#[derive(Debug)]
pub struct Canvas<'f> {
    frame: &'f mut Frame,
    rotation: Rotation,
    /// `true` draws pixels on, `false` clears them (u8g2 draw color 1/0).
    pub color: bool,
    /// Set when something tried to draw outside the canvas (layout bug detector).
    pub clipped: bool,
    /// The first pixel that fell outside, for diagnostics.
    pub first_clip: Option<(i32, i32)>,
    /// Set when a font lacked a glyph (font subset too small).
    pub missing_glyph: bool,
}

impl<'f> Canvas<'f> {
    pub fn new(frame: &'f mut Frame, rotation: Rotation) -> Self {
        frame.bytes = [0; FRAME_BYTES];
        Self {
            frame,
            rotation,
            color: true,
            clipped: false,
            first_clip: None,
            missing_glyph: false,
        }
    }

    pub fn width(&self) -> i32 {
        if self.rotation.portrait() {
            HEIGHT as i32
        } else {
            WIDTH as i32
        }
    }

    pub fn height(&self) -> i32 {
        if self.rotation.portrait() {
            WIDTH as i32
        } else {
            HEIGHT as i32
        }
    }

    pub fn pixel(&mut self, x: i32, y: i32) {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            self.clipped = true;
            self.first_clip.get_or_insert((x, y));
            return;
        }
        let (x, y) = (x as usize, y as usize);
        let (px, py) = match self.rotation {
            Rotation::R0 => (x, y),
            Rotation::R90 => (WIDTH - 1 - y, x),
            Rotation::R180 => (WIDTH - 1 - x, HEIGHT - 1 - y),
            Rotation::R270 => (y, HEIGHT - 1 - x),
        };
        self.frame.set(px, py, self.color);
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32) {
        for i in 0..w {
            self.pixel(x + i, y);
        }
    }

    pub fn vline(&mut self, x: i32, y: i32, h: i32) {
        for i in 0..h {
            self.pixel(x, y + i);
        }
    }

    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
        let _ = Line::new(Point::new(x0, y0), Point::new(x1, y1))
            .into_styled(style)
            .draw(self);
    }

    /// Outline rectangle.
    pub fn frame_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.hline(x, y, w);
        self.hline(x, y + h - 1, w);
        self.vline(x, y, h);
        self.vline(x + w - 1, y, h);
    }

    /// Filled rectangle.
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        for j in 0..h.max(0) {
            self.hline(x, y + j, w);
        }
    }

    /// Circle outline of radius `r` around (x, y), like u8g2 `drawCircle`.
    pub fn circle(&mut self, x: i32, y: i32, r: i32) {
        let style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
        let _ = Circle::with_center(Point::new(x, y), (2 * r + 1) as u32)
            .into_styled(style)
            .draw(self);
    }

    pub fn disc(&mut self, x: i32, y: i32, r: i32) {
        let style = PrimitiveStyle::with_fill(BinaryColor::On);
        let _ = Circle::with_center(Point::new(x, y), (2 * r + 1) as u32)
            .into_styled(style)
            .draw(self);
    }

    /// Triangle outline.
    pub fn triangle(&mut self, a: (i32, i32), b: (i32, i32), c: (i32, i32)) {
        self.line(a.0, a.1, b.0, b.1);
        self.line(b.0, b.1, c.0, c.1);
        self.line(c.0, c.1, a.0, a.1);
    }

    /// XBM bitmap with its top-left corner at (x, y); set bits are drawn.
    pub fn xbm(&mut self, x: i32, y: i32, img: &Xbm) {
        let stride = img.width.div_ceil(8) as usize;
        for row in 0..img.height as usize {
            for col in 0..img.width as usize {
                if img.bits[row * stride + col / 8] & (1 << (col % 8)) != 0 {
                    self.pixel(x + col as i32, y + row as i32);
                }
            }
        }
    }

    /// Text with its top at `y`. Returns the advance width.
    pub fn text(&mut self, font: &FontRenderer, x: i32, y: i32, s: &str) -> i32 {
        // Glyphs are emitted "on"; `draw_iter` applies the current draw color.
        match font.render(
            s,
            Point::new(x, y),
            VerticalPosition::Top,
            FontColor::Transparent(BinaryColor::On),
            self,
        ) {
            Ok(d) => d.advance.x,
            Err(_) => {
                self.missing_glyph = true;
                0
            }
        }
    }

    pub fn text_width(&mut self, font: &FontRenderer, s: &str) -> i32 {
        match font.get_rendered_dimensions(s, Point::zero(), VerticalPosition::Top) {
            Ok(d) => d.advance.x,
            Err(_) => {
                self.missing_glyph = true;
                0
            }
        }
    }

    pub fn text_right(&mut self, font: &FontRenderer, box_x: i32, box_w: i32, y: i32, s: &str) {
        let w = self.text_width(font, s);
        self.text(font, box_x + box_w - w, y, s);
    }

    pub fn text_centered(&mut self, font: &FontRenderer, y: i32, s: &str) {
        let w = self.text_width(font, s);
        let x = (self.width() - w) / 2;
        self.text(font, x, y, s);
    }

    /// Centered within the canvas width using the font's own alignment.
    pub fn text_centered_aligned(&mut self, font: &FontRenderer, y: i32, s: &str) {
        let x = self.width() / 2;
        if font
            .render_aligned(
                s,
                Point::new(x, y),
                VerticalPosition::Top,
                HorizontalAlignment::Center,
                FontColor::Transparent(BinaryColor::On),
                self,
            )
            .is_err()
        {
            self.missing_glyph = true;
        }
    }
}

impl OriginDimensions for Canvas<'_> {
    fn size(&self) -> Size {
        Size::new(self.width() as u32, self.height() as u32)
    }
}

impl DrawTarget for Canvas<'_> {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<BinaryColor>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        for Pixel(p, c) in pixels {
            let keep = self.color;
            self.color = c.is_on() == keep;
            self.pixel(p.x, p.y);
            self.color = keep;
        }
        Ok(())
    }
}

/// Clips a filled rectangle check used by tests: does the frame have any pixel in the
/// physical rectangle?
pub fn any_pixel(frame: &Frame, r: Rectangle) -> bool {
    r.points().any(|p| frame.get(p.x as usize, p.y as usize))
}
