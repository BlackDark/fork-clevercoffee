# Display parity: how it is checked, and what it does and does not prove

`cc-display` claims its pixels are the C++ firmware's pixels. This is how that
claim is checked, what each check covers, and — the part that matters most —
where it stops.

## The three checks

| Check | Test | Compares against | Catches |
| --- | --- | --- | --- |
| Engine parity | `tests/parity.rs` | real U8g2 2.36.18, via `tools/oracle` | a wrong glyph, a wrong clip, a wrong rotation, a wrong primitive |
| Template goldens | `tests/goldens.rs` | 48 committed P4 images | a wrong y, a swapped label, a stage that stops winning |
| Metric parity | `tests/glyph_parity.rs`, `tests/fmt_parity.rs` | measured tables, real C `snprintf` | a one-pixel metric error, a formatting difference |

They are complementary. A scenario typo passes the goldens and fails engine
parity. A template bug passes engine parity and fails the goldens. Neither
subsumes the other.

## Engine parity

`crates/cc-display/tools/oracle/display_oracle.cpp` links the **real** U8g2, fetched by
`just u8g2` into `target/u8g2` at upstream tag `2.36.18`, and replays a scenario
file through it. `cc_display::scenario` replays the *same file* through
`Display`. The two 128×64 framebuffers must be **bit identical** — zero
differing pixels, not "close enough", because one pixel is one wrong glyph
advance or one off-by-one in a clip.

The oracle's artwork is generated from the firmware's own
`cc_display::bitmaps::ALL` at build time, so the bytes it compares are the bytes
that ship.

```sh
just test-display-parity     # ~90 s; fetches U8g2 itself on first run
```

The corpus is `tests/scenarios/`: eleven files covering all ten fonts at both
font positions, all four rotations, the clip-window edges and degenerate sizes,
the draw-colour and power-save states, the `print` overloads, every bitmap, the
`drawStr`/`drawUTF8` and three ref-height modes, and transcriptions of the
shared offline, OTA and fullscreen-brew stages.

A scenario is a line-oriented draw-call script and the format is implemented
**twice**, once in C++ and once in Rust, on purpose. Two independent parsers is
the point: a disagreement shows up as a failing diff rather than as two copies
of the same mistake. That is also why the two number-detection rules must match
exactly — a token is a number only if it is unquoted *and* parses entirely as an
integer, or `printf1 0 32 93.5` silently becomes `93` with an empty value slot.

## What engine parity proved

Findings below were found *by* the oracle, not by reading the C++. Each is a
place where the obvious port is wrong.

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
  of the panel. `Display::prepare_display` now mirrors it; the scenarios
  deliberately skip it so `Display::new()` and the oracle's bare `U8G2` agree.

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

Stated plainly, because a parity harness that overstates itself is worse than
none:

- **The six normal layouts are not compared against the C++.** The C++ template
  classes are methods on `UICoordinator` and pull their values through
  `SystemContext`. Running them off-device means standing up a context graph the
  C++ has no support for, at which point the oracle is no longer measuring U8g2
  but a reimplementation of `SystemContext`. The goldens record what the Rust
  templates *do*, and the unit assertions in
  `src/templates/modern_layout.rs` pin the row map — but "the Modern layout puts
  its temperature at y=14" is a claim about `docs/display/layout-rules.md`, not
  a measured comparison with the C++.
- **The goldens are only as good as the review that first read the C++.** They
  record the port's behaviour. Where the port misread the C++, the golden
  records the misreading and then protects it.
- **`embedded-graphics::DrawTarget` is not implemented.** The task called for it;
  the crate implements its own framebuffer instead, on the grounds that
  `cc-display` is `no_std` with no `alloc` and the framebuffer is a fixed
  `[u8; 1024]`. That is a deviation from the stated architecture and needs a
  decision, not a footnote.
- **SH1106's 2-column transfer offset** is modelled and documented but there is
  no driver or I2C code here; that is the separate I2C concern.
- **The oracle builds for the host.** It is U8g2 compiled natively, not
  cross-compiled for the Xtensa target. The algorithms are the same C, but this
  is not a claim about ESP32 codegen.

## The remaining work this exposes

The goldens earned their keep immediately. The first `modern` render showed the
large temperature clipped off the top of the display — the `prepareDisplay`
finding above. There is more of the same waiting: the Modern row map, the
bottom-row bar, and the per-field boxes have been checked against
`docs/display/layout-rules.md` and against U8g2's metrics, but not against a
rendered C++ frame. That comparison is Potential ([`../attention.md`](../attention.md)). It needs a
`SystemContext` stub that is small enough to be obviously faithful.
