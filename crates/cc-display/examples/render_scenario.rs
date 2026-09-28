//! Render a scenario file through `cc-display` and write it as a P4 PPM.
//!
//! The counterpart of `tools/oracle/run.sh`, so a difference found by
//! `tests/parity.rs` can be *looked at* rather than only counted:
//!
//! ```text
//! cargo run -p cc-display --features scenarios \
//!     --example render_scenario -- tests/scenarios/fonts.txt /tmp/ours.ppm
//! bash tools/oracle/run.sh tests/scenarios/fonts.txt /tmp/theirs.ppm
//! ```
//!
//! It calls the same [`cc_display::scenario`] interpreter the tests do, so there
//! is no second parser to drift.

use std::io::Write;

use cc_display::display::Framebuffer;
use cc_display::scenario;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "usage: render_scenario <scenario.txt> <out.ppm>"
    );
    let text = std::fs::read_to_string(&args[1]).expect("can read the scenario");
    let fb =
        scenario::run(&text).unwrap_or_else(|e| panic!("scenario {} is malformed: {e}", args[1]));
    write_ppm(&args[2], &fb);
}

fn write_ppm(path: &str, fb: &Framebuffer) {
    // The same 1 = white, big-endian, 8-per-byte P4 the oracle writes, so the
    // two files can be compared with `cmp` as well as by eye.
    let mut out = Vec::with_capacity(cc_display::display::DISPLAY_WIDTH as usize * 8 + 10);
    out.extend_from_slice(b"P4\n128 64\n");
    for y in 0..cc_display::display::DISPLAY_HEIGHT {
        for x in (0..cc_display::display::DISPLAY_WIDTH).step_by(8) {
            let mut byte = 0u8;
            for b in 0..8 {
                if fb.pixel(x + b, y) {
                    byte |= 0x80 >> b;
                }
            }
            out.push(byte);
        }
    }
    let mut f = std::fs::File::create(path).expect("can create the output");
    f.write_all(&out).expect("can write the output");
}
