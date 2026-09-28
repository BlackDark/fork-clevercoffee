//! Display layout: framebuffer, `DrawTarget`, the profont/fub glyph atlases
//! converted from U8g2, the ported `DisplayLayoutUtils` helpers, and the six
//! templates.
//!
//! # Rules
//!
//! * Portable and `no_std`: it renders into a byte buffer, so the whole thing is
//!   golden-image testable on the host (R1-04 / R2-10) with no hardware.
//! * The AGENTS.md OLED rules apply here as assertions, not as conventions:
//!   everything fits 128x64, no rows overlap, counting fields use a fixed pixel
//!   width so digits do not shift, a bar and its label share a vertical
//!   midline, and bottom rows are anchored from `DISPLAY_HEIGHT`.
//!
//! Owner: R1-04 (spike) then R2-10 (templates). This is the R1-01 skeleton.

#![no_std]
