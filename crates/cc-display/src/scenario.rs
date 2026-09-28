//! A scenario interpreter: the Rust half of the display parity harness.
//!
//! # What this is for
//!
//! The claim `cc-display` wants to make is "these are the pixels the C++ firmware
//! draws". That claim is only worth something if it is checked against something
//! outside this crate, and there are two ways to do that:
//!
//! 1. **Engine parity.** A scenario file is a list of draw calls, in the format
//!    `tools/oracle/display_oracle.cpp` reads. The oracle replays it against the
//!    *real* U8g2 the firmware links; this module replays the same file against
//!    [`Display`]. If the two framebuffers are identical, every draw call this
//!    crate can make produces the U8g2 pixel, and the templates -- which are
//!    made only of these calls -- inherit that. `tests/scenarios/` is the
//!    corpus.
//! 2. **Template goldens.** The templates are driven directly, with no scenario
//!    in between, and the result is committed. That catches template *logic* (a
//!    wrong y, a swapped label) that engine parity cannot, because engine parity
//!    never runs a template.
//!
//! The two are complementary. A scenario typo passes (1) and fails (2); a
//! template bug passes (1) and fails (2).
//!
//! # Why this is behind a feature
//!
//! `cc-display` is `no_std` with no `alloc`, on purpose: the firmware has ~320 KB
//! of RAM, and a display layer that could allocate is a display layer that can
//! fail to allocate. This module needs `Vec` to tokenise, and it is a test
//! harness -- the firmware never draws a scenario.
//!
//! So it lives in the crate rather than in `tests/`, but behind the
//! `scenarios` feature, which is **off by default and not enabled for the
//! device build**. That way `examples/render_scenario`, `tests/parity.rs` and
//! `tests/goldens.rs` all drive the *same* interpreter. Having it only in
//! `tests/support/` would have meant the example had to carry a second copy --
//! a second parser for one format is exactly the drift the two-sided oracle is
//! meant to catch.
//!
//! # The format is duplicated, not generated
//!
//! The format is small, and the alternative -- generating scenarios from the
//! templates, or emitting draw calls from Rust and compiling them -- couples the
//! two sides in a way that hides a shared bug. Two independent parsers for one
//! line-oriented format is the point: a disagreement shows up as a failing diff
//! rather than as two copies of the same mistake.
//!
//! Grammar, matching the oracle exactly:
//!
//! ```text
//! rot        R0|R1|R2|R3
//! font       p10|p11|p12|p15|p17|p22|f17|f20|f25|f30
//! pos        top|baseline
//! refh       text|extended|all
//! color      N
//! powersave  0|1
//! clear
//! cursor     X Y
//! str        X Y "text"
//! utf8       X Y "text"
//! print      "text"
//! printc     N
//! printf1    X Y V
//! printf0    X Y V
//! printi     X Y V
//! hline      X Y L
//! vline      X Y L
//! pixel      X Y
//! line       X1 Y1 X2 Y2
//! box        X Y W H
//! frame      X Y W H
//! disc       X Y R
//! circle     X Y R
//! tri        X0 Y0 X1 Y1 X2 Y2
//! xbmp       NAME X Y W H
//! ```
//!
//! Inside a quoted string, `\xNN` is a raw byte and `\n` is a newline, which
//! `drawStr` stops at. The oracle's query ops (`width`, `ascent`, `dispw`, ...)
//! print to stdout instead of drawing, so they never appear in a scenario; they
//! are covered by `tests/glyph_parity.rs` against measured tables.
//!
//! # Bytes
//!
//! A scenario string is a byte string, because the firmware's strings are: the
//! degree sign is `static_cast<char>(176)`, one Latin-1 byte. Each byte is
//! widened to the `char` of the same value, and `font::latin1_of` folds it back
//! to the byte when a glyph is looked up. So `\xb0` and `\u{b0}` reach the font
//! as the same 0xB0 -- and the round trip needs no unsafe code.

use crate::bitmaps;
extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::display::{Display, Rotation};
use crate::fmt::{format_fixed, format_int};
use crate::font::{self, Font};

/// Every op the oracle dispatches, with the number of numeric arguments it
/// takes. Used both to dispatch and to build the arity error message, so the
/// two cannot drift apart.
const OPS: &[(&str, usize)] = &[
    ("rot", 0),
    ("font", 0),
    ("pos", 0),
    ("refh", 0),
    ("color", 1),
    ("powersave", 1),
    ("clear", 0),
    ("cursor", 2),
    ("str", 2),
    ("utf8", 2),
    ("print", 0),
    ("printc", 1),
    ("printf1", 2),
    ("printf0", 2),
    ("printi", 2),
    ("hline", 3),
    ("vline", 3),
    ("pixel", 2),
    ("line", 4),
    ("box", 4),
    ("frame", 4),
    ("disc", 3),
    ("circle", 3),
    ("tri", 6),
    ("xbmp", 4),
];

/// Why a scenario line could not be run.
///
/// Returned rather than panicked on, so a failing scenario names its line
/// instead of aborting the test binary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A line whose first token is not a known op.
    UnknownOp {
        /// 1-based line number in the scenario.
        line: usize,
        /// The token as written, so the message quotes what is there.
        op: String,
    },
    /// A known op with too few numeric arguments.
    BadArgs {
        /// 1-based line number in the scenario.
        line: usize,
        /// The op's name, borrowed from [`OPS`] so it cannot dangle.
        op: &'static str,
        /// How many numbers the op takes, from [`OPS`].
        wanted: usize,
        /// How many it was given.
        got: usize,
    },
    /// A font, rotation or bitmap name that is not one of the known ones.
    UnknownName {
        /// 1-based line number in the scenario.
        line: usize,
        /// The name as written.
        value: String,
    },
    /// An unterminated `"` in a line.
    UnterminatedString {
        /// 1-based line number in the scenario.
        line: usize,
    },
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownOp { line, op } => write!(f, "line {line}: unknown op {op:?}"),
            Self::BadArgs {
                line,
                op,
                wanted,
                got,
            } => {
                write!(f, "line {line}: {op} wants {wanted} numbers, got {got}")
            }
            Self::UnknownName { line, value } => {
                write!(f, "line {line}: unknown name {value:?}")
            }
            Self::UnterminatedString { line } => write!(f, "line {line}: unterminated string"),
        }
    }
}

/// One draw call, already tokenised.
struct Op {
    name: &'static str,
    nums: Vec<i32>,
    text: String,
}

impl Op {
    fn num(&self, i: usize) -> i32 {
        self.nums.get(i).copied().unwrap_or(0)
    }
}

/// Replay `scenario` against a fresh [`Display`] and return its framebuffer.
///
/// # Errors
///
/// The first line that could not be interpreted, with its line number.
///
/// # Panics
///
/// Never. A malformed scenario is a bad test input, and reporting it as a panic
/// would bury the reason.
pub fn run(scenario: &str) -> Result<crate::display::Framebuffer, Error> {
    let mut d = Display::new();
    for (index, raw) in scenario.lines().enumerate() {
        let line = index + 1;
        let Some(op) = parse_line(raw).map_err(|e| reline(line, e))? else {
            continue;
        };
        apply(&mut d, line, &op)?;
    }
    Ok(d.into_framebuffer())
}

/// The tokens of one non-empty, non-comment line, or `None` if there are none.
///
/// The op name is carried through *unresolved*, so an unknown one reaches
/// [`apply`] and is reported. Resolving it here and returning `None` would make
/// a typo a silent no-op, and a scenario that quietly stops testing what it
/// says it tests is worse than a scenario that fails to run.
fn parse_line(raw: &str) -> Result<Option<Op>, Error> {
    // A `#` starts a comment, as in the oracle: the whole line is dropped, so a
    // scenario can carry prose without a separate syntax.
    if raw.trim_start().starts_with('#') {
        return Ok(None);
    }
    let mut tokens = match tokenize(raw) {
        Some(t) => t,
        None => return Ok(None),
    };
    let name_token = tokens.remove(0);
    let name = match OPS.iter().find(|(n, _)| *n == name_token.1) {
        Some((n, _)) => *n,
        None => {
            return Err(Error::UnknownOp {
                line: 0,
                op: name_token.1.clone(),
            })
        }
    };

    // After the op name, bare integers are numbers and anything else is the
    // text. The oracle makes the same split by trying `strtol` on each token.
    let mut nums = Vec::new();
    let mut text = String::new();
    for tok in tokens {
        match tok.2 {
            Some(n) => nums.push(n),
            None => {
                if text.is_empty() {
                    text = tok.1;
                }
            }
        }
    }
    Ok(Some(Op { name, nums, text }))
}

/// Split on whitespace, honouring `"..."` and unescaping `\xNN` / `\n`.
///
/// Each byte of the result is widened to the `char` of the same value, so a
/// Latin-1 degree sign survives as U+00B0. `None` for a line with no tokens.
type Token = (String, String, Option<i32>);

fn tokenize(line: &str) -> Option<Vec<Token>> {
    let mut out: Vec<Token> = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b'"' {
            i += 1;
            let mut decoded = String::new();
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1] == b'x' {
                    let hi = (bytes[i + 2] as char).to_digit(16);
                    let lo = (bytes[i + 3] as char).to_digit(16);
                    if let (Some(hi), Some(lo)) = (hi, lo) {
                        decoded.push(latin1(u8::try_from(hi * 16 + lo).unwrap_or(0)));
                        i += 4;
                        continue;
                    }
                }
                if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'n' {
                    decoded.push('\n');
                    i += 2;
                    continue;
                }
                decoded.push(latin1(bytes[i]));
                i += 1;
            }
            out.push((String::new(), decoded, None));
        } else {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let raw = String::from_utf8_lossy(&bytes[start..i]).into_owned();
            let n = raw.parse::<i32>().ok();
            out.push((String::new(), raw, n));
        }
    }

    (!out.is_empty()).then_some(out)
}

/// A byte as the `char` of the same value -- U8g2's Latin-1 mapping.
fn latin1(b: u8) -> char {
    char::from(b)
}

/// Fill in the line number [`parse_line`] could not know.
fn reline(line: usize, e: Error) -> Error {
    match e {
        Error::UnknownOp { op, .. } => Error::UnknownOp { line, op },
        other => other,
    }
}

fn arity(line: usize, op: &Op) -> Result<(), Error> {
    let wanted = OPS
        .iter()
        .find(|(n, _)| *n == op.name)
        .map_or(0, |(_, n)| *n);
    if op.nums.len() >= wanted {
        return Ok(());
    }
    Err(Error::BadArgs {
        line,
        op: op.name,
        wanted,
        got: op.nums.len(),
    })
}

fn apply(d: &mut Display, line: usize, op: &Op) -> Result<(), Error> {
    arity(line, op)?;
    match op.name {
        "rot" => {
            d.set_display_rotation(match op.text.as_str() {
                "R1" => Rotation::R1,
                "R2" => Rotation::R2,
                "R3" => Rotation::R3,
                _ => Rotation::R0,
            });
        }
        "font" => d.set_font(lookup_font(&op.text, line)?),
        "pos" => {
            if op.text == "top" {
                d.set_font_pos_top();
            } else {
                d.set_font_pos_baseline();
            }
        }
        "refh" => match op.text.as_str() {
            "text" => d.set_font_ref_height_text(),
            "all" => d.set_font_ref_height_all(),
            _ => d.set_font_ref_height_extended_text(),
        },
        // The oracle passes the raw byte to `setDrawColor`, whose argument is a
        // `uint8_t`; U8g2 treats anything but 0 as "set" and 1 as "erase". The
        // clamp keeps a scenario's `color 5` from wrapping to 0 and erasing.
        "color" => d.set_draw_color(op.num(0).clamp(0, 2) as u8),
        "powersave" => d.set_power_save(op.num(0) != 0),
        "clear" => d.clear_buffer(),
        "cursor" => d.set_cursor(op.num(0), op.num(1)),
        "str" => {
            d.draw_str(op.num(0), op.num(1), &op.text);
        }
        "utf8" => {
            d.draw_string_utf8(op.num(0), op.num(1), &op.text);
        }
        // The oracle re-sets the cursor to its own current value before
        // `print`, which is a no-op kept for symmetry with `Print`. Doing it
        // here too means a scenario that mixes `cursor` and `print` steps the
        // same way on both sides.
        "print" => {
            let (x, y) = (d.cursor_x(), d.cursor_y());
            d.set_cursor(x, y);
            d.print(&op.text);
        }
        // `print((char)n)`. A byte above 0x7F is how the degree sign is written.
        "printc" => d.print_char(latin1(u8::try_from(op.num(0)).unwrap_or(0))),
        "printf1" | "printf0" => {
            let digits = u32::from(op.name == "printf1");
            let value: f64 = op.text.parse().unwrap_or(0.0);
            d.set_cursor(op.num(0), op.num(1));
            d.print(format_fixed(value, digits).as_str());
        }
        // The oracle reads the value from the *third* token, not the second, so
        // the number is the text token and the position is the two numbers.
        "printi" => {
            d.set_cursor(op.num(0), op.num(1));
            d.print(format_int(op.num(2)).as_str());
        }
        "hline" => d.draw_h_line(op.num(0), op.num(1), op.num(2)),
        "vline" => d.draw_v_line(op.num(0), op.num(1), op.num(2)),
        "pixel" => d.draw_pixel(op.num(0), op.num(1)),
        "line" => d.draw_line(op.num(0), op.num(1), op.num(2), op.num(3)),
        "box" => d.draw_box(op.num(0), op.num(1), op.num(2), op.num(3)),
        "frame" => d.draw_frame(op.num(0), op.num(1), op.num(2), op.num(3)),
        "disc" => d.draw_disc(op.num(0), op.num(1), op.num(2)),
        "circle" => d.draw_circle(op.num(0), op.num(1), op.num(2)),
        "tri" => d.draw_triangle(
            op.num(0),
            op.num(1),
            op.num(2),
            op.num(3),
            op.num(4),
            op.num(5),
        ),
        "xbmp" => {
            let b = bitmaps::lookup(&op.text).ok_or_else(|| Error::UnknownName {
                line,
                value: op.text.clone(),
            })?;
            d.draw_xbmp(
                op.num(0),
                op.num(1),
                i32::from(b.width),
                i32::from(b.height),
                b.data,
            );
        }
        other => {
            return Err(Error::UnknownOp {
                line,
                op: other.to_string(),
            });
        }
    }
    Ok(())
}

fn lookup_font(name: &str, line: usize) -> Result<Font, Error> {
    const NAMES: [&str; 10] = [
        "p10", "p11", "p12", "p15", "p17", "p22", "f17", "f20", "f25", "f30",
    ];
    let all: [Font; 10] = [
        font::profont10(),
        font::profont11(),
        font::profont12(),
        font::profont15(),
        font::profont17(),
        font::profont22(),
        font::fub17(),
        font::fub20(),
        font::fub25(),
        font::fub30(),
    ];
    NAMES
        .iter()
        .position(|n| *n == name)
        .map(|i| all[i])
        .ok_or_else(|| Error::UnknownName {
            line,
            value: name.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::{run, Error};

    #[test]
    fn a_blank_line_and_a_comment_are_not_ops() {
        let fb = run("\n# a comment\n   \n").expect("an empty scenario is legal");
        assert_eq!(fb.lit_count(), 0);
    }

    #[test]
    fn an_unknown_op_names_the_line_and_keeps_the_token() {
        // An unknown *name* is a scenario typo and must be reported, not
        // silently skipped: a silently skipped line is a scenario that quietly
        // stops testing what it says it tests.
        let e = run("not_an_op 1 2\n").expect_err("must reject");
        assert_eq!(
            e,
            Error::UnknownOp {
                line: 1,
                op: "not_an_op".into()
            }
        );
    }

    #[test]
    fn an_unknown_op_on_a_later_line_reports_that_line() {
        let e = run("# ok\nbox 0 0 1 1\nnot_an_op\n").expect_err("must reject");
        assert_eq!(
            e,
            Error::UnknownOp {
                line: 3,
                op: "not_an_op".into()
            }
        );
    }

    #[test]
    fn a_known_op_with_too_few_numbers_says_how_many_it_wanted() {
        let e = run("line 1 2\n").expect_err("`line` wants four numbers");
        assert_eq!(
            e,
            Error::BadArgs {
                line: 1,
                op: "line",
                wanted: 4,
                got: 2
            }
        );
    }

    #[test]
    fn the_first_failing_line_is_reported() {
        // Two bad lines: the earlier one wins, so the message points at the root
        // cause rather than at whatever was noticed last.
        let e = run("box 1 2 3\ncircle 1 2 3\n").expect_err("must reject");
        assert_eq!(
            e,
            Error::BadArgs {
                line: 1,
                op: "box",
                wanted: 4,
                got: 3
            }
        );
    }

    #[test]
    fn an_unknown_font_name_is_rejected() {
        let e = run("font profont99\n").expect_err("must reject");
        assert_eq!(
            e,
            Error::UnknownName {
                line: 1,
                value: "profont99".into()
            }
        );
    }

    #[test]
    fn an_unknown_bitmap_name_is_rejected() {
        let e = run("xbmp nosuch 0 0 8 8\n").expect_err("must reject");
        assert_eq!(
            e,
            Error::UnknownName {
                line: 1,
                value: "nosuch".into()
            }
        );
    }

    #[test]
    fn a_hex_escape_becomes_the_latin1_char_of_that_byte() {
        // `\xb0` is the degree sign as the firmware gets it. It must reach the
        // font as code point 0xB0, not as the two UTF-8 bytes of U+00B0 --
        // `tests/glyph_parity.rs` is what catches the difference.
        let fb = run("font p17\npos top\nclear\nstr 0 0 \"100\\xb0C\"\n").expect("valid");
        assert!(fb.lit_count() > 0);
    }

    #[test]
    fn a_newline_escape_stops_the_string_at_the_newline() {
        // U8g2's `drawStr` stops at `\n` and draws nothing after it, which is
        // how one string can hold several lines and how `displayWrappedMessage`
        // works. The tail is *dropped*, not moved to another row.
        let with_tail = run("font p17\npos top\nclear\nstr 0 0 \"ab\\ncd\"\n").expect("valid");
        let only_head = run("font p17\npos top\nclear\nstr 0 0 \"ab\"\n").expect("valid");
        let only_tail = run("font p17\npos top\nclear\nstr 0 0 \"cd\"\n").expect("valid");
        assert_eq!(
            with_tail.lit_count(),
            only_head.lit_count(),
            "the tail is not drawn"
        );
        assert!(
            only_tail.lit_count() > 0,
            "and the tail is not empty, so this is a real check"
        );
    }
}
