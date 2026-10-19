# The display checks: what they cover, and where they stop

`cc-display` renders every pixel on the panel. This is how that is checked, what
each check covers, and — the part that matters most — where it stops.

## The two checks

| Check | Test | Compares against | Catches |
| --- | --- | --- | --- |
| Template goldens | `tests/goldens.rs` | 48 committed P4 images | a wrong y, a swapped label, a stage that stops winning |
| Metric tables | `tests/glyph_parity.rs`, `tests/fmt_parity.rs` | advances measured from U8g2 2.36.18, and C's `snprintf` | a one-pixel advance error, a formatting difference |

They are complementary. A template bug passes the metric tables and fails the
goldens; a wrong RLE header moves text without changing which stage wins. Neither
subsumes the other.

## The metric tables

`tests/glyph_parity.rs` checks every glyph of every embedded font against
`tests/support/glyph_widths.rs`, which holds real U8g2 2.36.18's advance for every
code point 0x20..=0x7E in all ten fonts. `tests/support/string_widths.rs` adds 26
multi-character strings, which is the only way to reach the two width-loop quirks:
the last-glyph ink substitution and the balanced first-glyph x-offset.

This is the narrowest check and the one that has to pass before any golden means
anything: a single wrong RLE header, signed-field decode or first-code offset moves
text sideways and shifts every row laid out around it.

`tests/fmt_parity.rs` is the same idea for number rendering: 80 000 doubles, each
compared against what C's `snprintf("%.*f")` printed for it, keyed by the exact
IEEE-754 bits so the two cannot disagree about which number they meant.

All three tables are **frozen**. The programs that measured them are not in the
tree, so a new or changed font means re-measuring against U8g2 2.36.18 by hand;
each table's header states that. Do not edit a row to make a test pass: the rows
are an independent measurement.

## What the U8g2 comparison established

The host comparison against real U8g2 is gone, but its findings are constraints the
code still carries. Each is a place where the obvious reading of U8g2 is wrong.

- **`U8G2_BALANCED_STR_WIDTH_CALCULATION` is on by default** (`u8g2.h:175`).
  `getStrWidth` adds the *first* glyph's x-offset back in (issue #1561). Without
  it every string starting with an inset glyph — `'!'`, `'('`, `'i'`, `'"'` in
  `profont10` — is one or two pixels narrow, which moves every right-aligned and
  centred label.
- **`u8g2_uint_t` wraps, and a wrapped range counts as intersecting.**
  `u8g2_is_intersection_decision_tree` has an explicit `if (v0 > v1) return 1`
  arm in both branches, and `v0 > v1` is what an underflowed coordinate looks
  like. A glyph box that starts above the display is therefore *clipped*, not
  dropped: a 30 px digit at a baseline of y=12 shows its bottom 12 rows.
- **`draw_glyph` needs no run buffer at all.** An earlier version buffered runs
  in a fixed 128-entry array and dropped the overflow, which silently ate the
  bottom of every large `fub*` glyph — 163 pixels in `fonts.txt`. A `fub30`
  glyph is up to 31×30 px and emits several hundred runs. `Font` is `Copy`, so
  the closure can borrow `self` directly.
- **`drawCircle` draws eight pixels per section, not six**; the second pixel of
  each quadrant is the diagonal partner `(x0 ± y, y0 ∓ x)`.
- **`drawDisc` draws eight vertical lines, not twelve**; the lower quadrants
  start at `y0` and run down, so the mirrored lines over-drew the bottom edge.
- **The degree sign is one Latin-1 byte.** U8g2's `drawStr` walks *bytes*
  (`u8x8_ascii_next`) and the firmware's strings are Latin-1: `static_cast<char>(176)`.
  A Rust `&str` cannot hold a lone `0xB0`, so `"\u{b0}"` is two bytes and a byte
  walk measures **30 px instead of 25** in `profont10`. `font::latin1_of` folds
  the code point back to the byte.
- **The bare `Display` and the *prepared* display differ.** Preparation sets
  `setFontRefHeightExtendedText()` and `setFontPosTop()` once at init.
  `ExtendedText` uses `ascent_para` where `Text` uses `ascent_A` (7 vs 6 in
  `profont10`), and without `pos top` a `y` is a *baseline*, so the Modern
  template's `fub20` readout at y=14 lands at rows −9..13 — clipped off the top
  of the panel. `Display::prepare_display` mirrors it.

## The goldens

`tests/goldens.rs` drives the six templates and every shared stage directly and
compares against 48 committed P4 images. It also asserts the **`Stage`** each
case reaches, because a wrong stage is a policy bug in the ADR-0001 order that
can render a perfectly plausible image of the wrong screen.

```sh
just snapshot-display        # regenerates; then READ THE DIFF
```

**Read the diff before committing a regenerated golden.** Every pixel is
supposed to stay put.

## What is not proven

Stated plainly, because a check that overstates itself is worse than
none:

- **Nothing is compared against a live external renderer.** The per-primitive
  corpus went with the U8g2 oracle: all ten fonts at both positions, all four
  rotations, clip-window edges and degenerate sizes, draw-colour and power-save,
  the `print` overloads, every bitmap, `drawStr`/`drawUTF8` and three ref-height
  modes. Those paths are checked now only where a golden or a metric table happens
  to exercise them, so a regression in, say, rotation 3 of a primitive that no
  template uses would pass.
- **The templates are not compared against any external reference.** The goldens
  record what the Rust templates *do*, and the unit assertions in
  `src/templates/modern_layout.rs` pin the row map — but "the Modern layout puts
  its temperature at y=14" is a claim about `docs/display/layout-rules.md`, not a
  measured comparison.
- **The goldens are only as good as the review that produced them.** Where the
  port misread the layout, the golden records the misreading and then protects it.
- **`embedded-graphics::DrawTarget` is not implemented.** The task called for it;
  the crate implements its own framebuffer instead, on the grounds that
  `cc-display` is `no_std` with no `alloc` and the framebuffer is a fixed
  `[u8; 1024]`. That is a deviation from the stated architecture and needs a
  decision, not a footnote.
- **SH1106's 2-column transfer offset** is modelled and documented but there is
  no driver or I2C code here; that is the separate I2C concern.

## What the goldens caught

The goldens earned their keep immediately: the first `modern` render showed the
large temperature clipped off the top of the display — the `prepare_display`
finding above. The Modern row map, the bottom-row bar and the per-field boxes are
now checked against `docs/display/layout-rules.md` and against U8g2's measured
metrics. A pixel that no template draws has neither check, which is the ceiling
above.
