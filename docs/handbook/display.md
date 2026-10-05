# Display — the entry point

One topic, three files. Read this page first; it says which of the three you
want and, just as importantly, which one is a *rule* and which is a *record*.

The display is the part of this firmware with the most moving pieces that a
regression in it is **invisible** until someone stands in front of the machine.
That is why the layout rules below are binding (`AG-DISPLAY-1` … `AG-DISPLAY-6`)
rather than advisory.

| File | What it is | Read it when |
| --- | --- | --- |
| [`display-modern-layout.md`](display-modern-layout.md) | **The rules.** The 128×64 constraint, the U8G2 font→pixel-height mapping, the fixed-width numeric fields, the row maps, the bar-and-label pairing. | You are moving anything on the screen. Start here. |
| [`display-architecture.md`](display-architecture.md) | **How the Rust renderer works.** Component roles, the frame lifecycle, the I²C chunking, shared-vs-template ownership. | You need to know *when* a frame is actually pushed to the panel. |
| [`display-parity.md`](display-parity.md) | **What is proven, and what is not.** The three checks (engine parity against real U8g2, template goldens, metric parity), what each caught, and — the part that matters — where the checking stops. | You are about to claim the display is correct, or you are regenerating a golden. |

## The rules, condensed

These are stated in full in [`display-modern-layout.md`](display-modern-layout.md)
and are binding under `AG-DISPLAY-1` … `AG-DISPLAY-6`:

- **Everything fits in 128×64.** Nothing clips at an edge. Not a glyph, not a
  bar, not a bitmap.
- **Y positions come from U8G2 bbox heights with `setFontPosTop()`**, not from
  the number in the font name. `fub20` is 23 px tall; `profont17` is 15 px.
- **Counting fields get a reserved pixel width.** Reserve the widest expected
  string (`"100.0"`, `"999 s"`) with a `getStrWidth` probe and draw the live value
  right-aligned inside it. `9` → `10` and `9.9` → `10.0` must not shift
  anything.
- **Composite blocks are centered once.** Center the block by its reserved box,
  never re-center a whole line from the live string width.
- **A progress bar and its value label share a vertical midline.** Do not
  bottom-edge-align a bar against a taller label.
- **Re-read the row map after every edit.** Anchor bottom rows from
  `DISPLAY_HEIGHT`. Layout regressions are blocking, not a review nit.

## The honest caveat

[`display-parity.md`](display-parity.md) § "What is not proven" says it plainly
and it is worth reading before you trust any of this: **the six normal layouts
are not compared against the C++.** The goldens record what the Rust templates
*do*, and where the port misread the C++, a golden will happily protect the
misreading. "The Modern layout puts its temperature at y=14" is a claim about
[`display-modern-layout.md`](display-modern-layout.md), not a measured
comparison with the C++.

## Related

- [`docs/adr/0001`](../adr/0001-display-subsystem-architecture.md) — the
  accepted decision: one render pipeline, shared defaults with template
  overrides, one source of truth for thresholds.
- [`docs/operations/integration-checklist.md`](../operations/integration-checklist.md)
  — the on-device checks, including the screen-fit ones. `AG-REPO-18`: add any
  new failure mode you discover, in the same commit.
- [`docs/handbook/differences.md`](differences.md) §6 and §29 — the two display
  entries in the behavioural-difference index.
