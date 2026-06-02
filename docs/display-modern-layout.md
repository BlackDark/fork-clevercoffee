# Modern OLED layout — font metrics and row map

Hardware: **128×64** monochrome OLED, U8G2 with **`setFontPosTop()`** (`OledDriver::prepareDisplay()`).

Constants live in `include/clevercoffee/display/templates/ModernTemplate.h` (`ModernTemplateLayout` namespace). Layout helpers: `include/clevercoffee/display/DisplayLayoutUtils.h`.

## U8G2 font → pixel mapping

With **`setFontPosTop()`**, `drawStr(x, y, …)` uses **y as the top edge** of the glyph bounding box. Layout uses **bbox height** (not the number in the font name).

| Font | Bbox height (px) | Used for |
|------|------------------|----------|
| `u8g2_font_fub20_tf` | 23 | Idle main temperature digits |
| `u8g2_font_profont17_tf` | 15 | Idle °C unit; brew time/temp fields |
| `u8g2_font_profont11_tf` | 11 | Status bar; HEATING/READY label |
| `u8g2_font_profont10_tf` | 10 | Setpoint label; brew footer; target suffix |

## Stable numeric fields

Reserve fixed pixel width using widest probe string (`"100.0"`, `"999 s"`, `"100.0°C"`), draw live values **right-aligned** inside each box. Composite blocks are centered once — digit count changes must not shift the block.

## Bottom row (idle + brew)

10 px row at **y=54..63**. Progress bar (4 px) is **vertically centered** with the adjacent label in that row. Bar+label cluster is **horizontally centered** on screen.

```
 y=54..63 ─ bottom row (bar midline aligned with label)
 y=57..60 ─ progress bar (4 px, centered in row)
```

`kContentBottomY = 53` — main content must end here.

## Modern idle row map

```
 y=0  ─ status bar (WiFi, MQTT, uptime)
 y=12 ─ separator
 y=14 ─ main temp: fub20 digits (fixed "100.0" box) + profont17 °C
 y=38 ─ 16×16 icon + HEATING/READY text (ends y=53)
 y=54 ─ centered bar + setpoint label cluster
```

Bar tick = **`setpoint − HEATING_LOGO_THRESHOLD_C`** (5 °C).

## Modern brew row map

```
 y=0  ─ phase indicator
 y=12 ─ separator
 y=14 ─ fixed fields: [999 s][ - ][100.0°C] (profont17, block centered)
 y=43 ─ footer: weight / pressure
 y=54 ─ centered brew bar + target label cluster
```

## Modern post-brew row map

```
 y=2  ─ coffee cup bitmap (40×40, centered at x=44)
 y=42 ─ end of cup
 y=46 ─ brew time: profont17, fixed "999.5" box + " s" (ends y=60)
```

Time digits right-aligned in fixed-width box; composite block centered on screen.
Font: `u8g2_font_profont17_tf` (15 px bbox). Do NOT use fub25 — it overflows at y=52.

## Verification on device

1. Seconds 9→10 and temp 9.9→10.0 do not shift centered readouts.
2. Bar and side label share vertical midline; cluster centered horizontally.
3. No overlap between status row (ends y=53) and bottom row (starts y=54).
