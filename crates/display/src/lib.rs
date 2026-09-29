//! The display: a framebuffer, a font-metrics table, six templates and dirty-page tracking.
//!
//! Everything here is `no_std`, hardware-free and host-testable, which is the point. The C++
//! display could only be checked by looking at a physical panel, so layout regressions shipped;
//! here a template is rendered into a [`Framebuffer`] and the *pixels* are asserted: nothing
//! outside 128x64, no two rows overlapping, a numeric field the same width when the value changes
//! from `9` to `10`, and a bar and its label sharing a vertical midline.
//!
//! Three C++ problems are fixed structurally here:
//!
//! - **D31**, the whole framebuffer on the I2C bus every 100 ms: a page is dirty only when a
//!   pixel in it changed, and a board flushes [`Framebuffer::take_dirty`].
//! - **Stable numeric fields**: every counting value is drawn right-aligned in a box measured
//!   with [`crate::font::NUM_PROBE`], so gaining a digit cannot move its neighbours. This is the
//!   display half of the rule in `CLAUDE.md`.
//! - **D01's screen half**: an OTA in progress owns the whole panel, and the app refuses to start
//!   one unless the machine is idle, so there is no screen left showing a brew that is not
//!   happening.
//!
//! The font is the one thing that cannot be verified from the C++ tree, because the U8G2 bitmaps
//! live in a library PlatformIO fetched. [`font`] therefore carries its own table with metrics
//! the tests can pin, and the task list records that this is the display port's largest unknown.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod font;
pub mod framebuffer;
pub mod lang;
pub mod layout;
pub mod model;
pub mod templates;
pub mod text;
pub mod widgets;

pub use font::Font;
pub use framebuffer::{DirtyPage, Framebuffer, HEIGHT, WIDTH};
pub use lang::Language;
pub use model::{Features, NetStatus, OtaProgress, PidView, ScreenModel, Template};
pub use templates::{render, screen_for, Screen};

use clevercoffee_hal_traits::Display as DisplayTrait;

/// The panel-side driver: owns a framebuffer and knows how to push it.
///
/// Generic over the transport so the host tests can hand it a recorder and the board hands it an
/// SSD1306 over I2C. The transport only has to move 128-byte pages, which is what makes D31's fix
/// possible: nothing above this type can ask for a whole-frame flush.
pub struct Panel<T: PageSink> {
    fb: Framebuffer,
    sink: T,
    /// Frames since boot, for the auto-sleep timer.
    frames: u32,
    last_activity_ms: u32,
    now_ms: u32,
    asleep: bool,
    /// The auto-sleep threshold, 35 minutes as in the C++ `AUTO_SLEEP_MINUTES`.
    sleep_after_ms: u32,
}

/// Where dirty pages go. The board implements this over I2C at 400 kHz.
pub trait PageSink {
    fn send_page(&mut self, page: &DirtyPage);
    /// Optional hook so a panel that supports it can blank itself.
    fn set_power(&mut self, _on: bool) {}
}

/// The default auto-sleep, from `Timing::DISPLAY_AUTO_SLEEP`.
pub const AUTO_SLEEP_MS: u32 = 2_100_000;

impl<T: PageSink> Panel<T> {
    pub fn new(sink: T) -> Self {
        Self {
            fb: Framebuffer::new(),
            sink,
            frames: 0,
            last_activity_ms: 0,
            now_ms: 0,
            asleep: false,
            sleep_after_ms: AUTO_SLEEP_MS,
        }
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.fb
    }

    pub fn framebuffer_mut(&mut self) -> &mut Framebuffer {
        &mut self.fb
    }

    /// The page sink, for a board that needs to reach it.
    pub fn sink(&mut self) -> &mut T {
        &mut self.sink
    }

    /// Renders a model and pushes only the pages that changed.
    ///
    /// Returns the number of pages sent, so the display task can log a real number instead of
    /// assuming the whole frame went out.
    pub fn render(&mut self, m: &ScreenModel) -> usize {
        self.now_ms = self.now_ms.wrapping_add(RENDER_PERIOD_MS);
        render(&mut self.fb, m);
        self.frames = self.frames.wrapping_add(1);
        if self.now_ms.wrapping_sub(self.last_activity_ms) < self.sleep_after_ms {
            self.asleep = false;
            self.sink.set_power(true);
        } else if !self.asleep {
            self.asleep = true;
            self.sink.set_power(false);
        }
        if self.asleep {
            return 0;
        }
        self.flush()
    }

    /// Pushes the dirty pages and marks them sent.
    pub fn flush(&mut self) -> usize {
        let pages = self.fb.take_dirty();
        let n = pages.len();
        for p in &pages {
            self.sink.send_page(p);
        }
        self.fb.clear_pending();
        n
    }

    /// Notes that the user did something, resetting the auto-sleep countdown.
    pub fn note_activity(&mut self) {
        self.last_activity_ms = self.now_ms;
    }

    /// Overrides the auto-sleep threshold, for a configuration value.
    pub fn set_sleep_after(&mut self, ms: u32) {
        self.sleep_after_ms = ms;
    }

    /// Advances the panel's own clock, which the auto-sleep timer runs on.
    pub fn tick(&mut self, elapsed_ms: u32) {
        self.now_ms = self.now_ms.wrapping_add(elapsed_ms);
    }

    pub fn is_asleep(&self) -> bool {
        self.asleep
    }

    pub fn frames(&self) -> u32 {
        self.frames
    }
}

/// The render period, from `Timing::DISPLAY_RENDER`. `Panel::render` advances the panel clock by
/// it, so a host test that renders a thousand frames advances the auto-sleep timer by a hundred
/// seconds without a timer.
const RENDER_PERIOD_MS: u32 = 100;

impl<T: PageSink> DisplayTrait for Panel<T> {
    fn flush(&mut self) {
        Panel::flush(self);
    }

    fn elapsed_ms(&self) -> u32 {
        self.now_ms
    }

    fn is_awake(&self) -> bool {
        !self.asleep
    }
}

impl<T: PageSink> core::fmt::Debug for Panel<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Panel")
            .field("frames", &self.frames)
            .field("now_ms", &self.now_ms)
            .field("asleep", &self.asleep)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Template;

    #[derive(Debug, Default)]
    struct Recorder {
        pages: heapless::Vec<usize, 16>,
        power: bool,
    }

    impl PageSink for Recorder {
        fn send_page(&mut self, page: &DirtyPage) {
            let _ = self.pages.push(page.index);
        }

        fn set_power(&mut self, on: bool) {
            self.power = on;
        }
    }

    #[test]
    fn a_first_render_sends_the_pages_that_have_ink() {
        let mut p = Panel::new(Recorder::default());
        let n = p.render(&ScreenModel::idle());
        assert!(n > 0 && n <= 8, "sent {n} pages");
        assert_eq!(p.sink().pages.len(), n);
    }

    #[test]
    fn an_unchanged_screen_sends_nothing_on_the_next_tick() {
        // The D31 assertion at the driver level: a 100 ms render tick that changes nothing puts
        // nothing on the bus.
        let mut p = Panel::new(Recorder::default());
        let m = ScreenModel::idle();
        p.render(&m);
        p.sink().pages.clear();
        p.render(&m);
        assert!(
            p.sink().pages.is_empty(),
            "an unchanged screen sent {:?}",
            p.sink().pages
        );
        assert_eq!(p.frames(), 2);
    }

    #[test]
    fn a_changed_screen_sends_only_the_changed_pages() {
        // Without the thermometer graphic, a two-degree change moves one text row, so exactly the
        // page that row lives on is sent. With the thermometer, the fill column moves too, which
        // spans pages 1 to 5; that is five of eight, not eight of eight, and still the fix for
        // D31 rather than a whole frame every tick.
        let mut p = Panel::new(Recorder::default());
        let mut m = ScreenModel::idle();
        m.template = Template::Minimal;
        p.render(&m);
        p.sink().pages.clear();
        m.temperature_c = Some(95.5);
        p.render(&m);
        assert_eq!(p.sink().pages.len(), 1, "sent {:?}", p.sink().pages);

        let mut p = Panel::new(Recorder::default());
        let mut m = ScreenModel::idle();
        m.template = Template::Standard;
        p.render(&m);
        p.sink().pages.clear();
        m.temperature_c = Some(95.5);
        p.render(&m);
        assert!(p.sink().pages.len() <= 6, "sent {:?}", p.sink().pages);
    }

    #[test]
    fn the_panel_sleeps_after_the_configured_idle_time() {
        let mut p = Panel::new(Recorder::default());
        p.set_sleep_after(1000);
        let m = ScreenModel::idle();
        for _ in 0..5 {
            p.render(&m);
        }
        assert!(!p.is_asleep(), "five 100 ms ticks is 500 ms");
        p.note_activity();
        for _ in 0..15 {
            p.render(&m);
        }
        assert!(p.is_asleep(), "2000 ms of renders with no activity");
        assert!(!DisplayTrait::is_awake(&p));
    }

    #[test]
    fn activity_wakes_the_panel_again() {
        let mut p = Panel::new(Recorder::default());
        p.set_sleep_after(200);
        let m = ScreenModel::idle();
        for _ in 0..6 {
            p.render(&m);
        }
        assert!(p.is_asleep());
        p.note_activity();
        p.render(&m);
        assert!(!p.is_asleep());
        assert!(p.sink().power);
    }

    #[test]
    fn a_sleeping_panel_sends_no_pages() {
        let mut p = Panel::new(Recorder::default());
        p.set_sleep_after(0);
        let mut m = ScreenModel::idle();
        p.render(&m);
        p.sink().pages.clear();
        m.temperature_c = Some(11.0);
        assert_eq!(p.render(&m), 0, "a sleeping panel must not drive the bus");
    }

    #[test]
    fn the_default_auto_sleep_is_the_cpp_thirty_five_minutes() {
        assert_eq!(AUTO_SLEEP_MS, 35 * 60 * 1000);
    }
}
