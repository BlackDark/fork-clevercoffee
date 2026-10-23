# 0007: Display rendering and fonts

Status: accepted, 2026-10

## Decision

- Screens are drawn into a 1 KB page-layout frame by `cc-display` with a small u8g2-like canvas (embedded-graphics + `u8g2-fonts`), so the C++ layouts port almost one to one. Rotation (upright, inverted) is done in software.
- The firmware flushes only frames that changed.
- Fonts use the smallest u8g2 glyph set per size (`_tn` digits, `_tr` ASCII, `_tf` where German/Spanish text needs Latin-1): ~16 KB instead of ~43 KB.
- Layout is tested: every template × language × scenario renders without clipped pixels or missing glyphs (`crates/display/src/tests.rs`). `just screens` writes a PNG contact sheet for review.

## Consequences

- The tests found and fixed overflows the C++ firmware had (ADR 0004).
- New texts must fit: a too-long translation fails the build's tests, not the user's display.
