---
title: Add pressure to fullscreen brew timer
upstream_pr: 630
upstream_repo: rancilio-pid/clevercoffee
upstream_url: https://github.com/rancilio-pid/clevercoffee/pull/630
category: maybe
advise: skip
stake: "Pressure on the Standard/Upright fullscreen brew overlay only; Modern already shows it, and the PR's digit-count cursor jumps fail OLED rules."
effort: S
risk: low
priority: 9
status: planned
created: 2026-09-11
tags:
  - upstream-mr
  - plan
---

# PR 630 — fullscreen brew pressure

## Verdict

**Skip.** Do not port.

Modern already shows pressure in the brew footer. The fullscreen brew timer is opt-in (`display.fullscreen_brew_timer`, default **false**) and Modern does not run it: `DisplayPolicy<false, false>` turns off shared heating-logo + **brew** overlay (`sharedFullscreenBrewTimer() == false`). Flush/hot-water overlays still default on for Modern; this PR is only about the brew overlay. The PR only helps Standard / Scale / Upright users who turn that overlay on.

The PR layout is a hard reject against our OLED rules: live `setCursor` jumps when digit count changes (`< 9.95` → x=14 else x=7), compressed Y steps, and coords written for U8G2 **baseline** font pos. We use `setFontPosTop()`. Copying it would clip on upright 64-wide and shift `9.9` → `10.0`.

If demand appears later: rewrite with `DisplayLayoutUtils.h` fixed-width boxes — never their cursor hacks.

## Upstream

[PR 630](https://github.com/rancilio-pid/clevercoffee/pull/630) (open, `scottcondie24`): add pressure to `displayFullscreenBrewTimer()` in `src/display/displayCommon.h`. +76/−11. Also bumps `VERSION.txt` 4.0.3 → 4.1.0 (ignore).

Behavior:

- Landscape (`template != 4`) and upright (`template == 4`).
- When scale + pressure: shrink `yOffset` / `yStep`, third line `profont15` `"%.1f bar"`.
- When no scale + pressure: keep `displayBrewtimeFs`, then pressure with **digit-count cursor**:
  - Upright: `inputPressureFilter < 9.95 && > -0.05` → `setCursor(14, 110)` else `(7, 110)`.
  - Landscape: `displayBrewtimeFs(48, 12, …)` (was y=25) + `setCursor(68, 45)`.
- Author note: should right-align via `getUTF8Len`; they shipped cursor hacks instead.
- Dead locals: `brewByTimeEnabled`, `brewByWeightEnabled` — computed, unused.

Clip math (profont15 ≈ 8 px/glyph, `"10.0 bar"` = 8 glyphs = 64 px):

| Mode | PR origin | Width | Result |
|------|-----------|-------|--------|
| Landscape no-scale | x=68 | 68+64=132 | **clips** 128-wide at `10.0` |
| Upright no-scale | x=7 or 14 | 7+64=71 / 14+64=78 | **clips** 64-wide always |
| Upright + scale | x=5, three packed rows | 5+64=69 | **clips**; overlaps 40×40 cup at (12,12) |

PR is still open; no reviews.

## Current fork

Shared overlay: `include/clevercoffee/display/DisplayFullscreenModes.h` (`displayFullscreenBrewTimer`). Same cup + time/weight as upstream **before** this PR. **No pressure** — no `bar`, no `getPressure()`, no `hardwareSensorsPressureEnabled` in that file.

| Template | Fullscreen brew overlay | Pressure today |
|----------|-------------------------|----------------|
| **Modern** | **Off** (`DisplayPolicy<false, false>` → `sharedFullscreenBrewTimer() == false`) | Brew/flush footer `%.1f bar` at `kBrewFooterY` (y=43), `drawBrewFooter` in `ModernTemplate.h`, gated on `hardwareSensorsPressureEnabled` |
| Standard | On if config (`DefaultDisplayPolicy`) | None (idle/brew or overlay) |
| Scale | On if config | Normal brew only (`setCursor(0, 46)`); overlay hides it |
| Upright | On if config | Normal portrait (`langstring_pressure_ur` + value + `" bar"`, EN prefix `"P: "`); overlay hides it |
| Minimal / Temp-only | On if config (`DisplayPolicy<false>` = heating logo off, brew overlay still on) | None |

`OledDriver::prepareDisplay()` calls `setFontPosTop()`. OLED rules: 128×64, no clip, probe-width numeric fields, right-align in box, do not re-center from live `getStrWidth`. Helpers: `DisplayLayoutUtils.h` (`drawStrRightInBox`). Docs: `docs/display-modern-layout.md`, `docs/display-architecture.md`.

Existing overlay already violates those rules (`displayBrewtimeFs` jumps x when `brewtime < 9950`). Do not add a second jump for pressure.

Config: `display.fullscreen_brew_timer` default **false**. Default template is Standard; pressure users who care about brew telemetry are on Modern / Scale / Upright **without** the overlay.

## Need it?

No for the default path.

Gap is only: pressure sensor **and** fullscreen brew timer **and** Standard/Scale/Upright. That combo hides Scale/Upright's existing pressure line behind the cup overlay. Standard never showed pressure anyway — this PR would not make Standard a pressure UI.

Modern is the pressure-during-brew screen. Leave the overlay as a big timer.

## Plan

Skip. Users who want pressure during brew: **Modern** (footer at y=43, no fullscreen overlay). Scale or Upright with `display.fullscreen_brew_timer` left off also show pressure on the normal brew screen.

If later included (do not start this now):

1. Touch only `DisplayFullscreenModes.h`. Do not change Modern. Do not bump version. Do not copy unused brew-by-time/weight flags.
2. Gate on `hardwareSensorsPressureEnabled`. Use `sensorCoordinator().getPressure()` / `getFilteredPressure()` — not removed `inputPressureFilter()`.
3. `setFontPosTop()` + bbox heights. Recalculate Y; do not reuse PR baseline coords.
4. Fixed-width fields via `drawStrRightInBox` + probe strings. Right-align digits; units outside the box. Center the **block** once.
5. `displayBrewtimeFs` digit-jumps stay out of this change unless the same overlay pass is already open — do not expand scope to "fix all fullscreen layout" unless asked.

### Landscape 128×64 (Standard/Scale overlay)

Cup 40×40 at (−1, 11) → x≈0..38, y=11..50. Text to the **right** of the cup.

```
 y=11..50  cup
 y=14      time  [probe "99.9"] + "s"  (profont22 or keep current fs helper if unchanged)
 y=36      weight [probe "99.9"] + "g"  (scale only)
 y=52      pressure [probe "10.0"] + " bar"  (profont10/11, not 15 — 15 at x=48 is tight; 15 at x=68 clips)
```

Pressure probe `"10.0"` (or `"99.9"` if we allow >10 bar). Unit `" bar"` immediately after the box. Block X ≥ 48 so it clears the cup. Last glyph ≤ 127. `9.9` → `10.0` must not move the box.

No-scale: time + pressure only. Do not drop time to y=12 to "make room" by overlapping the cup.

### Upright 64×128

Cup 40×40 at (12, 12) → x=12..51, y=12..51. Usable text width is **64**. `"10.0 bar"` in profont15 is ~64 px — **does not fit** with any left margin. Use `"P:"` / `langstring_pressure_ur` + profont10/11, or drop the unit word `"bar"` to `"b"`.

```
 y=12..51  cup
 y=56      time   [probe "99.9"] + "s"
 y=78      weight [probe "99.9"] + "g"  (scale only)
 y=100     pressure [probe "10.0"] + "b" or "P: 10.0"
 y=127     last pixel; nothing below
```

If time+weight+pressure cannot all sit below the cup without clip/overlap: **omit pressure on upright overlay** (normal Upright template already shows it). Do not compress into the cup bitmap.

Completion = no clip on 128×64 or 64×128; `9.9` → `10.0` (and `9` → `10` s) does not shift; no overlap with cup.

## Do not copy

- Live `getStrWidth` centering of the whole line each frame.
- Digit-count `setCursor` jumps (`< 9.95` / `brewtime < 9950`).
- PR Y/X coords (baseline font pos, not `setFontPosTop()`).
- `yOffset`/`yStep` packing that walks into the cup.
- Landscape pressure at x=68 (`"10.0 bar"` overflows 128).
- `VERSION.txt` bump.
- Unused `brewByTimeEnabled` / `brewByWeightEnabled`.
- Direct `config.get<bool>("hardware.sensors.pressure.enabled")` string keys — use `Config::getInstance().hardwareSensorsPressureEnabled`.

## Tests / verification

Cannot fully native-render OLED. Document the row map and probe strings in this file (and in a short comment above the overlay if we ever implement).

If implemented later:

- Probe strings: time `"99.9"`, weight `"99.9"`, pressure `"10.0"` (and `"-9.9"` if negative allowed).
- Walk 9.9 → 10.0 bar, 9 → 10 s, 9.9 → 10.0 g — field X must be constant.
- Device: Standard+scale+pressure overlay; Standard no-scale+pressure; Upright both. Confirm last pixel ≤ edge; cup unobscured.
- `pio run -e esp32_usb -s` and `pio test -e native_test` (header-only draw; native will not catch clip).

## Risks

- **Upright 64-wide clip** — `"10.0 bar"` in profont15 is ~one full screen width.
- **Cup overlap** — 40×40 at (12,12) on 64-wide leaves little room; PR's y=60 start with `setFontPosTop()` draws into the bitmap.
- **Font-pos mismatch** — PR coords assume baseline; we are top-origin. Blind port places every row ~font-height too low (or into the bottom edge).
- **Modern double-draw** — Modern does not run this overlay; do not "enable it for Modern" as part of a port.
- **Scope creep** — overlay already has `displayBrewtimeFs` digit-jumps; fixing those is a separate layout cleanup.
