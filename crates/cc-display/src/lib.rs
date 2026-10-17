//! Display layout: the framebuffer and `DrawTarget`, the profont and fub glyph
//! atlases converted from U8g2, the layout helpers, and the display templates.
//!
//! # Rules
//!
//! * Portable and `no_std`: it renders into a byte buffer, so the whole thing is
//!   golden-image testable on the host with no hardware.
//! * The AGENTS.md OLED rules apply here as assertions, not as conventions:
//!   everything fits 128x64, no rows overlap, counting fields use a fixed pixel
//!   width so digits do not shift, a bar and its label share a vertical
//!   midline, and bottom rows are anchored from `DISPLAY_HEIGHT`.

#![no_std]

/// The OLED bitmaps: [`bitmaps_data`] holds the bytes, [`bitmaps`] names them.
pub mod bitmaps;
pub mod bitmaps_data;
pub mod boot;
pub mod display;
pub mod fmt;
pub mod font;
pub mod fullscreen;
pub mod helpers;
pub mod lang;
pub mod layout;
/// Which of the three status LEDs should be lit, as a pure function. It lives
/// here, next to `helpers`, because the LED rules are part of what the display
/// shows; the module's own doc records why `cc-web` and `cc-machine` would have
/// been wrong.
pub mod leds;
pub mod model;
pub mod ota;
#[cfg(feature = "scenarios")]
pub mod scenario;
pub mod system_screens;
pub mod templates;
pub mod widgets;
