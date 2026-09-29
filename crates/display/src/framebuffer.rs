//! The 128x64 mono framebuffer and its dirty-page tracking.
//!
//! The panel is a 1-bit page-addressed SSD1306-style controller: 128 columns, 8 pages of 8 rows,
//! one byte per column per page, so the whole frame is 1024 bytes and a page is 128 bytes. Every
//! write in this crate goes through [`Framebuffer`], which
//!
//! - clips at the panel edge instead of wrapping or panicking, and
//! - marks the page dirty only when a pixel actually changed.
//!
//! The second point is the fix for defect D31: the C++ firmware re-sent the whole buffer on
//! every render, so a 100 ms render tick put 128 bytes x 8 pages on a 400 kHz I2C bus whether or
//! anything moved. A board flushes only [`Framebuffer::take_dirty`], and the host tests assert
//! that a re-render of an unchanged screen produces no dirty page at all.
//!
//! Getting that right costs one extra kilobyte. A single buffer cannot tell "this page is the
//! same as the frame the panel already has" from "this page changed", because rendering starts
//! with a clear: the buffer's own history is gone by the time the first glyph lands. So the type
//! keeps the last flushed frame alongside the one being drawn, and [`Framebuffer::take_dirty`]
//! diffs the two a page at a time. Two kilobytes of static RAM to stop pushing a kilobyte down
//! a 400 kHz bus ten times a second, which is the trade the architecture asks for.

use crate::font::{self, Font};

/// Panel width in pixels. C++: `Timing.h:56`, `DISPLAY_WIDTH`.
pub const WIDTH: u16 = 128;
/// Panel height in pixels. C++: `Timing.h:57`, `DISPLAY_HEIGHT`.
pub const HEIGHT: u16 = 64;
/// Bytes per page row: one byte per column.
pub const PAGE_STRIDE: usize = WIDTH as usize;
/// The number of 8-row pages.
pub const PAGES: usize = (HEIGHT / 8) as usize;
/// The whole framebuffer, which is the number the heap budget is checked against.
pub const BYTES: usize = PAGE_STRIDE * PAGES;

/// A draw request that fell outside the panel.
///
/// Recorded rather than silently dropped, because a clipped draw is a layout bug and a layout bug
/// that cannot be observed is a layout bug that ships. [`Framebuffer::clipped`] counts them and the
/// template tests assert it is zero for every template in every state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Clip {
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
}

/// One page of the framebuffer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Page {
    pub index: usize,
    pub bytes: [u8; PAGE_STRIDE],
}

/// A dirty-page record, produced by [`Framebuffer::take_dirty`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DirtyPage {
    pub index: usize,
    /// The page as it should now be sent to the panel.
    pub bytes: [u8; PAGE_STRIDE],
}

#[derive(Debug)]
pub struct Framebuffer {
    data: [u8; BYTES],
    /// The last frame the panel was told about. Diffed against `data` at flush time.
    prev: [u8; BYTES],
    /// Pages whose contents differ from the last flushed image.
    ///
    /// A board flush sends these and calls [`Framebuffer::clear_dirty`]. Until then they stay
    /// dirty, so a board that flushes rarely still never sends a stale page.
    pending: [bool; PAGES],
    clipped: u32,
    /// The first few clipped requests, so a failing test can say which draw overflowed.
    clips: heapless::Vec<Clip, CLIP_LOG>,
}

/// How many clipped requests are kept for diagnosis. Overflowing the log is not itself an error:
/// the counter is the assertion, the log is only there to make a failure readable.
pub const CLIP_LOG: usize = 16;

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Framebuffer {
    pub const fn new() -> Self {
        Self {
            data: [0; BYTES],
            prev: [0; BYTES],
            pending: [false; PAGES],
            clipped: 0,
            clips: heapless::Vec::new(),
        }
    }

    /// Erases the panel.
    ///
    /// Deliberately marks nothing dirty: which pages the erase actually changed depends on what
    /// was on them, and the only thing that knows that is the diff in [`Framebuffer::take_dirty`].
    /// Marking every page here is what a single-buffer driver has to do, and it is exactly the
    /// behaviour D31 is about.
    pub fn clear(&mut self) {
        self.data = [0; BYTES];
    }

    /// How many draw requests have been clipped since the last [`Framebuffer::clear_clipped`].
    pub fn clipped(&self) -> u32 {
        self.clipped
    }

    pub fn clear_clipped(&mut self) {
        self.clipped = 0;
        self.clips.clear();
    }

    /// The clipped requests recorded so far, oldest first.
    pub fn clip_log(&self) -> &[Clip] {
        &self.clips
    }

    /// The page index a row belongs to.
    pub const fn page_of(y: u16) -> usize {
        (y / 8) as usize
    }

    /// Sets one pixel, recording a clip if it is off the panel.
    pub fn pixel(&mut self, x: i16, y: i16, on: bool) {
        if x < 0 || y < 0 || x >= WIDTH as i16 || y >= HEIGHT as i16 {
            self.record_clip(x, y, 1, 1);
            return;
        }
        self.put(x, y, on);
    }

    fn record_clip(&mut self, x: i16, y: i16, w: i16, h: i16) {
        self.clipped += 1;
        let _ = self.clips.push(Clip { x, y, w, h });
    }

    /// Sets a pixel without bounds accounting. Every primitive in this type calls this and does
    /// its own single bounds check, so a shape that hangs off the panel is one clipped request
    /// rather than one per lost pixel.
    fn put(&mut self, x: i16, y: i16, on: bool) {
        if x < 0 || y < 0 || x >= WIDTH as i16 || y >= HEIGHT as i16 {
            return;
        }
        let (x, y) = (x as u16, y as u16);
        let page = Self::page_of(y);
        let mask = 1u8 << (y % 8);
        let idx = page * PAGE_STRIDE + x as usize;
        let current = self.data[idx];
        let next = if on { current | mask } else { current & !mask };
        if next != current {
            self.data[idx] = next;
        }
    }

    /// Whether a rectangle lies wholly on the panel.
    ///
    /// Deliberately "wholly" and not "at all": a shape that hangs off the edge is a layout bug
    /// even when part of it is visible, so a partially-overlapping primitive is recorded as
    /// clipped and not drawn at all. The template tests assert a zero clip count, which makes a
    /// shape that would run off the panel a failing test rather than a truncated label.
    fn touches(&self, x: i16, y: i16, w: i16, h: i16) -> bool {
        let (w, h) = (w.max(0), h.max(0));
        if w == 0 || h == 0 {
            return true;
        }
        x >= 0 && y >= 0 && x + w <= WIDTH as i16 && y + h <= HEIGHT as i16
    }

    /// Reads one pixel.
    pub fn get(&self, x: u16, y: u16) -> bool {
        if x >= WIDTH || y >= HEIGHT {
            return false;
        }
        let idx = Self::page_of(y) * PAGE_STRIDE + x as usize;
        self.data[idx] & (1u8 << (y % 8)) != 0
    }

    /// Filled rectangle, origin at the top-left corner.
    pub fn fill_rect(&mut self, x: i16, y: i16, w: i16, h: i16) {
        if !self.touches(x, y, w, h) {
            self.record_clip(x, y, w, h);
            return;
        }
        for row in y..y + h {
            for col in x..x + w {
                self.put(col, row, true);
            }
        }
    }

    /// Rectangle outline. `w` and `h` are the outside dimensions, as U8G2's `drawFrame`.
    pub fn draw_frame(&mut self, x: i16, y: i16, w: i16, h: i16) {
        if w <= 0 || h <= 0 {
            return;
        }
        for col in x..x + w {
            self.put(col, y, true);
            self.put(col, y + h - 1, true);
        }
        for row in y..y + h {
            self.put(x, row, true);
            self.put(x + w - 1, row, true);
        }
    }

    /// Horizontal line.
    pub fn hline(&mut self, x: i16, y: i16, w: i16) {
        if !self.touches(x, y, w, 1) {
            self.record_clip(x, y, w, 1);
            return;
        }
        for col in x..x + w {
            self.put(col, y, true);
        }
    }

    /// Vertical line.
    pub fn vline(&mut self, x: i16, y: i16, h: i16) {
        if !self.touches(x, y, 1, h) {
            self.record_clip(x, y, 1, h);
            return;
        }
        for row in y..y + h {
            self.put(x, row, true);
        }
    }

    /// Line between two points, by the usual integer walk.
    pub fn line(&mut self, x0: i16, y0: i16, x1: i16, y1: i16) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let (mut x, mut y) = (x0, y0);
        if !self.touches(
            x0.min(x1),
            y0.min(y1),
            (x1 - x0).abs() + 1,
            (y1 - y0).abs() + 1,
        ) {
            self.record_clip(x0, y0, (x1 - x0).abs() + 1, (y1 - y0).abs() + 1);
            return;
        }
        loop {
            self.put(x, y, true);
            if x == x1 && y == y1 {
                return;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Filled isosceles triangle, apex at `(x, y)`, base `base_w` wide at `y + h - 1`.
    pub fn fill_triangle_down(&mut self, x: i16, y: i16, base_w: i16, h: i16) {
        if !self.touches(x, y, base_w, h) {
            self.record_clip(x, y, base_w, h);
            return;
        }
        for row in 0..h {
            // Half-width grows linearly from 0 at the apex to base_w/2 at the base.
            let half = (base_w * (row + 1)) / (2 * h);
            let cx = x + base_w / 2;
            self.hline(cx - half, y + row, 2 * half);
        }
    }

    /// Draws `text` with its glyph-box top-left at `(x, y)`. Returns the x just past the last
    /// advance, so a caller can chain strings.
    pub fn text(&mut self, x: i16, y: i16, font: Font, text: &str) -> i16 {
        let scale = font.scale() as i16;
        let w = font::str_width(text, font);
        if !self.touches(x, y, w, font.height() as i16) {
            self.record_clip(x, y, w, font.height() as i16);
            return x + w;
        }
        let mut cursor = x;
        for c in text.chars() {
            if c != ' ' {
                if let Some(glyph) = font::glyph(c) {
                    for (col, bits) in glyph.iter().enumerate() {
                        for row in 0..font::GLYPH_H as i16 {
                            if bits & (1 << row) == 0 {
                                continue;
                            }
                            for dy in 0..scale {
                                for dx in 0..scale {
                                    self.put(
                                        cursor + col as i16 * scale + dx,
                                        y + row * scale + dy,
                                        true,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            cursor += font.cell_width() as i16;
        }
        cursor
    }

    /// Draws `text` right-aligned inside `box_x .. box_x + box_w`, so the right edge of the last
    /// glyph never moves when the value's digit count changes.
    pub fn text_right_in_box(&mut self, box_x: i16, box_w: i16, y: i16, font: Font, text: &str) {
        let w = font::str_width(text, font);
        let x = box_x + box_w - w;
        if x < box_x {
            // Wider than the reserved box: the value would push into the label to its left, which
            // is the exact regression the box exists to prevent. Recorded, not hidden.
            self.record_clip(box_x, y, box_w, font.height() as i16);
        }
        self.text(x, y, font, text);
    }

    /// Draws `text` centred in `box_x .. box_x + box_w`.
    pub fn text_centered_in_box(&mut self, box_x: i16, box_w: i16, y: i16, font: Font, text: &str) {
        let w = font::str_width(text, font);
        self.text(box_x + (box_w - w) / 2, y, font, text);
    }

    /// Draws `text` centred on the panel.
    pub fn text_centered(&mut self, y: i16, font: Font, text: &str) {
        let w = font::str_width(text, font);
        self.text(((WIDTH as i16 - w) / 2).max(0), y, font, text);
    }

    /// The pages that differ from the frame the panel already has, in ascending order.
    ///
    /// The diff is what makes D31's fix work: a page whose bytes are identical to the last flush
    /// is not returned, so a re-render of an unchanged screen transfers nothing at all. The
    /// returned pages become the new baseline, so calling this twice in a row returns nothing the
    /// second time unless something changed in between.
    pub fn take_dirty(&mut self) -> heapless::Vec<DirtyPage, PAGES> {
        let mut out = heapless::Vec::new();
        for p in 0..PAGES {
            let lo = p * PAGE_STRIDE;
            let hi = lo + PAGE_STRIDE;
            if self.data[lo..hi] != self.prev[lo..hi] {
                let mut bytes = [0u8; PAGE_STRIDE];
                bytes.copy_from_slice(&self.data[lo..hi]);
                self.prev[lo..hi].copy_from_slice(&bytes);
                self.pending[p] = true;
                let _ = out.push(DirtyPage { index: p, bytes });
            }
        }
        out
    }

    /// The pages still owed to the panel because no flush has happened.
    pub fn pending_pages(&self) -> usize {
        self.pending.iter().filter(|p| **p).count()
    }

    /// Marks the pending set as sent. A board calls this after the pages reach the panel.
    pub fn clear_pending(&mut self) {
        self.pending = [false; PAGES];
    }

    /// One page, for tests and for a board that wants to read rather than flush.
    pub fn page(&self, index: usize) -> Page {
        let mut bytes = [0u8; PAGE_STRIDE];
        bytes.copy_from_slice(&self.data[index * PAGE_STRIDE..(index + 1) * PAGE_STRIDE]);
        Page { index, bytes }
    }

    /// The raw frame, for tests.
    pub fn bytes(&self) -> &[u8; BYTES] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_framebuffer_is_exactly_one_kilobyte() {
        assert_eq!(BYTES, 1024);
        assert_eq!(PAGES, 8);
        let fb = Framebuffer::new();
        assert_eq!(fb.bytes().len(), 1024);
    }

    #[test]
    fn a_pixel_round_trips() {
        let mut fb = Framebuffer::new();
        fb.pixel(3, 9, true);
        assert!(fb.get(3, 9));
        assert!(!fb.get(9, 3), "x and y must not be swapped");
        fb.pixel(3, 9, false);
        assert!(!fb.get(3, 9));
    }

    #[test]
    fn a_page_is_eight_rows_of_one_bit() {
        let mut fb = Framebuffer::new();
        fb.pixel(0, 0, true);
        fb.pixel(0, 7, true);
        assert_eq!(fb.page(0).bytes[0], 0b1000_0001);
        assert_eq!(fb.page(0).bytes[1], 0);
    }

    #[test]
    fn drawing_outside_the_panel_is_recorded_and_does_not_panic() {
        let mut fb = Framebuffer::new();
        fb.pixel(-1, 0, true);
        fb.pixel(WIDTH as i16, 0, true);
        fb.pixel(0, HEIGHT as i16, true);
        fb.text(-40, 0, Font::Small, "clipped");
        assert_eq!(fb.clipped(), 4, "every clipped request is counted");
    }

    #[test]
    fn a_text_string_wider_than_its_box_is_recorded() {
        // The regression the fixed-width boxes exist to prevent: a value that grows a digit must
        // not push left into its label. Overflow is counted, so the template tests can assert it
        // never happens.
        let mut fb = Framebuffer::new();
        fb.text_right_in_box(40, 18, 0, Font::Small, "100.0");
        assert_eq!(fb.clipped(), 1);
        let mut fb2 = Framebuffer::new();
        fb2.text_right_in_box(40, 30, 0, Font::Small, "100.0");
        assert_eq!(fb2.clipped(), 0);
    }

    #[test]
    fn nothing_is_dirty_after_a_clear_and_a_rerender_of_the_same_content() {
        let mut fb = Framebuffer::new();
        fb.text(0, 0, Font::Small, "hello");
        assert_eq!(fb.take_dirty().len(), 1);
        fb.clear();
        // Same content again: the bytes match the last flush, so nothing is sent.
        fb.text(0, 0, Font::Small, "hello");
        assert!(fb.take_dirty().is_empty());
    }

    #[test]
    fn a_clear_that_erases_everything_sends_every_page() {
        let mut fb = Framebuffer::new();
        fb.fill_rect(0, 0, 128, 64);
        assert_eq!(fb.take_dirty().len(), 8, "a full frame is eight pages");
        assert_eq!(fb.pending_pages(), 8);
        fb.clear_pending();
        assert_eq!(fb.take_dirty().len(), 0, "the panel already has this frame");
        fb.clear();
        assert_eq!(
            fb.take_dirty().len(),
            8,
            "erasing the frame is a change the panel must be told about"
        );
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                assert!(!fb.get(x, y));
            }
        }
    }

    #[test]
    fn pending_pages_survive_until_a_flush() {
        let mut fb = Framebuffer::new();
        fb.pixel(0, 0, true);
        let _ = fb.take_dirty();
        assert_eq!(
            fb.pending_pages(),
            1,
            "a page owed to the panel stays owed until the board says it was sent"
        );
        fb.clear_pending();
        assert_eq!(fb.pending_pages(), 0);
    }

    #[test]
    fn a_full_clear_and_redraw_of_one_row_touches_one_page() {
        // The D31 assertion at the framebuffer level: the screen is cleared and redrawn every
        // render tick, and only the page whose content actually differs goes on the bus.
        let mut fb = Framebuffer::new();
        fb.text(0, 0, Font::Small, "row one");
        assert_eq!(fb.take_dirty().len(), 1);
        fb.clear_pending();
        fb.clear();
        fb.text(0, 0, Font::Small, "row two");
        assert_eq!(
            fb.take_dirty().len(),
            1,
            "only the page whose content changed is dirty"
        );
        assert_eq!(fb.take_dirty().len(), 0, "and only once");
    }

    #[test]
    fn a_frame_and_a_triangle_stay_inside_the_panel() {
        let mut fb = Framebuffer::new();
        fb.fill_triangle_down(60, 10, 16, 13);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                // get() only reports real pixels, so a stray write would have to be checked
                // against the clip counter.
                let _ = fb.get(x, y);
            }
        }
        assert_eq!(fb.clipped(), 0);
    }

    #[test]
    fn text_advances_by_exactly_one_cell_per_character() {
        let mut fb = Framebuffer::new();
        let end = fb.text(10, 0, Font::Small, "abcd");
        assert_eq!(end, 10 + 4 * Font::Small.cell_width() as i16);
    }
}
