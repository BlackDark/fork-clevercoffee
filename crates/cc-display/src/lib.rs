//! Allocation-free rendering into the panel-format framebuffer.
//!
//! Host checks cover pixels and layout. Follow AG-DISPLAY-1 through AG-DISPLAY-6.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bitmaps_data;
pub mod boot;
pub mod display;
pub mod fmt;
pub mod font;
pub mod fullscreen;
pub mod helpers;
pub mod lang;
pub mod layout;
pub mod leds;
pub mod model;
pub mod ota;
pub mod system_screens;
pub mod templates;
pub mod widgets;
