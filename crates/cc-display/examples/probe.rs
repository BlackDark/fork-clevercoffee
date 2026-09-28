//! Scratch probe: dump a small region of a framebuffer as ASCII art.
//!
//! Not a test. `cargo run --example probe -p cc-display -- <op>` is how the
//! draw primitives were compared against the C++ oracle pixel by pixel during
//! the port. Kept because re-running that comparison is the first thing to do
//! when a primitive's behaviour is in doubt.
use cc_display::display::{Display, Framebuffer};
use cc_display::font;

fn dump(fb: &Framebuffer, name: &str, w: i32, h: i32) {
    println!("{name} lit={}", fb.lit_count());
    for y in 0..h {
        let mut row = String::new();
        for x in 0..w {
            row.push(if fb.pixel(x, y) { '#' } else { '.' });
        }
        println!("  {row}");
    }
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    if which == "all" || which == "frame" {
        for (x, y, w, h) in [(5, 5, 0, 4), (5, 5, 4, 0), (10, 10, 4, 3), (0, 0, 20, 8)] {
            let mut d = Display::new();
            d.draw_frame(x, y, w, h);
            dump(d.framebuffer(), &format!("frame({x},{y},{w},{h})"), 24, 12);
        }
        for (x, y, w, h) in [(5, 5, 0, 4), (5, 5, 4, 0), (0, 0, 6, 4)] {
            let mut d = Display::new();
            d.draw_box(x, y, w, h);
            dump(d.framebuffer(), &format!("box({x},{y},{w},{h})"), 24, 12);
        }
    }
    if which == "all" || which == "glyph" {
        for f in ["11", "20"] {
            let font = if f == "11" {
                font::profont11()
            } else {
                font::fub20()
            };
            for s in ["A", "Ay", "100.0", "0123"] {
                let mut d = Display::new();
                d.set_font(font);
                d.set_font_pos_top();
                d.draw_box(0, 0, 128, 64);
                d.draw_str(4, 6, s);
                dump(
                    d.framebuffer(),
                    &format!("p{f} {s:?} over full box"),
                    60,
                    40,
                );
            }
        }
    }
    if which == "all" || which == "tri" {
        for (a, b, c, e, f, g) in [(10, 5, 20, 5, 15, 25), (0, 0, 40, 0, 20, 30)] {
            let mut d = Display::new();
            d.draw_triangle(a, b, c, e, f, g);
            dump(
                d.framebuffer(),
                &format!("tri({a},{b})-({c},{e})-({f},{g})"),
                46,
                32,
            );
        }
    }
    if which == "all" || which == "missing" {
        let mut d = Display::new();
        d.set_font(font::profont11());
        d.set_font_pos_top();
        println!("adv U+4E2D (utf8 bytes) = {}", d.draw_str(0, 0, "\u{4e2d}"));
        println!("adv 0x7F = {}", d.draw_str(0, 0, "\u{7f}"));
        println!("adv 0xE4 = {}", d.draw_str(0, 0, "\u{e4}"));
    }
    if which == "all" || which == "bitmap" {
        let mut d = Display::new();
        d.draw_xbmp(2, 2, 8, 8, &cc_display::bitmaps_data::ANTENNA_OK_ICON);
        dump(d.framebuffer(), "antenna_ok", 14, 12);
        let mut d = Display::new();
        d.draw_xbmp(2, 2, 8, 9, &cc_display::bitmaps_data::BLUETOOTH_ICON);
        dump(d.framebuffer(), "bluetooth", 14, 13);
    }
    if which == "all" || which == "rot" {
        for r in [0, 1, 2, 3] {
            let mut d = Display::new();
            d.set_display_rotation(match r {
                0 => cc_display::display::Rotation::R0,
                1 => cc_display::display::Rotation::R1,
                2 => cc_display::display::Rotation::R2,
                _ => cc_display::display::Rotation::R3,
            });
            d.set_font(font::profont11());
            d.set_font_pos_top();
            d.draw_str(2, 2, "R");
            d.draw_h_line(0, 20, 10);
            d.draw_v_line(30, 0, 10);
            println!("R{r}: logical {}x{}", d.display_width(), d.display_height());
            dump(d.framebuffer(), &format!("  R{r}"), 40, 30);
        }
    }
}
