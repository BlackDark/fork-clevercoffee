//! Allocation-free rendering into the panel-format framebuffer.
//!
//! Host checks cover pixels and layout. Follow AG-DISPLAY-1 through AG-DISPLAY-6.

#![no_std]

/// The OLED bitmaps, as raw arrays.
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
pub mod system_screens;
pub mod templates;
pub mod widgets;
