# CleverCoffee OLED Display Analysis & Design

## Hardware

| Property | Value |
|----------|-------|
| Resolution | 128 × 64 pixels |
| Type | Monochrome OLED (white on black) |
| Driver | SSD1306 (0.96") or SH1106 (1.3") |
| Interface | I2C (0x3C or 0x3D) |
| Library | U8G2 |
| Refresh | 100 ms |

## Current Display Templates

### Overview

> **Current architecture:** see [display-architecture.md](display-architecture.md) and [ADR 0001](adr/0001-display-subsystem-architecture.md).

| Template | Enum | Status |
|----------|------|--------|
| Standard | `STANDARD (0)` | Implemented — horizontal landscape |
| Minimal | `MINIMAL (1)` | Implemented — compact landscape |
| Temperature Only | `TEMPERATURE_ONLY (2)` | Implemented — large temp + ring |
| Scale | `SCALE (3)` | Implemented — weight-focused |
| Upright | `UPRIGHT (4)` | Implemented — portrait/rotated |
| Modern | `MODERN (5)` | Implemented — large temp, icons, setpoint bar |

### Current Screens (by machine state)

![Current Display Layouts](../assets/clevercoffee_current_displays.png)

#### 1. Standard - Normal (Idle/Ready)

```
┌────────────────────────────────────────────┐ y=0
│ [WiFi][BT] MQTT         02h 15m           │ status bar
├────────────────────────────────────────────┤ y=12
│  ║▓▓║─   Temp:  93.5 °C                   │ y=16
│  ║▓▓║    Set:   94.0 °C                   │ y=26
│  ║  ║    Brew:  25 s                       │ y=36
│  (●)     62|52|12    45.2%                 │ y=47
│          [═══════════ heat bar ══════]     │ y=60
│          Backflush recommended             │ y=62 (if due)
└────────────────────────────────────────────┘
```

- Left: thermometer outline with fill bar, setpoint marker, blinking at ±0.3°C
- Right: current/set temp (`profont11`), brew timer, PID values (Kp|Ti|Td), output %
- Bottom: progress bar for heater output
- **Missing**: no pressure, no weight, no flow rate

#### 2. Standard - Heating

```
┌────────────────────────────────────────────┐
│ [WiFi][BT] MQTT         02h 15m           │
├────────────────────────────────────────────┤
│                                            │
│  [flame icon]      87.3°                   │
│   40×40                                    │
│                                            │
└────────────────────────────────────────────┘
```

- Shown when temp is >5°C below setpoint
- 40×40 `Heating_Logo` + large temp in `fub25`

#### 3. Fullscreen Brew Timer

```
┌────────────────────────────────────────────┐
│                                            │
│  [cup icon]        25.3 s                  │
│   40×40                                    │
│                    18.5 g  (if scale)      │
│                                            │
└────────────────────────────────────────────┘
```

- Config-gated (`display.fullscreen_brew_timer`)
- Timer in `fub25`, weight in `profont22` (if scale enabled)

#### 4. Steam Mode

```
┌────────────────────────────────────────────┐
│                                            │
│  [steam icon]     135°                     │
│   40×40                                    │
│                                            │
└────────────────────────────────────────────┘
```

- `Steam_Logo` + large temp in `fub30`

#### 5. Upright - Normal (Portrait 64×128)

```
┌──────────┐
│WiFi  MQTT│ status bar
├──────────┤
│T: 93.5°C │
│S: 94.0°C │
│B: 25 s   │
│W: 18 g   │ (if scale)
│P: 9.1 bar│ (if pressure)
│          │
│  BREW    │ large status word
│          │
│[heat bar]│
└──────────┘
```

- Compact labels, large status word: OK/WAIT/BREW/FLUSH/CLEAN
- Only layout showing weight and pressure in normal view

#### 6. Standby / PID Off

```
┌────────────────────────────────────────────┐
│                                            │
│         [power icon]                       │
│          52×53                              │
│                                            │
│        Standby mode                        │
└────────────────────────────────────────────┘
```

### Fonts Used

| Font | Size | Usage |
|------|------|-------|
| `profont10_tf` | ~8px | Tiny labels, standby text |
| `profont11_tf` | ~9px | Default body, status bar |
| `profont12_tf` | ~10px | Backflush prompts |
| `profont15_tf` | ~12px | Timer suffix |
| `profont22_tf` | ~18px | Status word, brew weight |
| `fub17_tf` | ~14px | Backflush title |
| `fub20_tf` | ~16px | Upright fullscreen timer |
| `fub25_tf` | ~20px | Heating temp, brew timer |
| `fub30_tf` | ~24px | Main temperature display |

### Bitmaps (`bitmaps.h`)

| Bitmap | Size | Purpose |
|--------|------|---------|
| `CleverCoffee_Logo` | 40×40 | Boot splash |
| `Heating_Logo` | 40×40 | Heating screen |
| `Off_Logo` | 52×53 | PID off / standby |
| `Steam_Logo` | 40×40 | Steam mode |
| `Brew_Cup_Logo` | 40×40 | Fullscreen brew |
| `Hot_Water_Logo` | 40×40 | Hot water timer |
| `Manual_Flush_Logo` | 40×40 | Manual flush |
| `Water_Tank_Empty_Logo` | 47×64 | Empty tank warning |
| `Antenna_OK/NOK_Icon` | 8×8 | WiFi status |
| `Bluetooth_Icon` | 8×9 | BLE scale status |

### Issues with Current Design

1. **Legacy analysis** — template inventory above is kept for visual reference; implementation details live in `display-architecture.md`
2. **Standard lacks pressure/weight** - only available in Upright
3. **No flow rate display** anywhere
4. **No brew progress indication** - no visual feedback for target completion
5. **Fullscreen brew lacks context** - no temp or pressure during extraction
6. **Text-heavy labels** consume valuable pixel space
7. **No phase indicator** during brew (pre-infusion vs extraction vs done)
8. **Thermometer graphic** takes 25% of width but is hard to read

---

## Design Proposals

![Design Concepts](../assets/clevercoffee_design_proposals.png)

### Concept A: Arc Gauge
Semicircular temp gauge + compact data footer. Best for at-a-glance monitoring.

### Concept B: Dashboard
Three-zone layout: temp+trend, brew timeline bar, triple data cells. Maximum density.

### Concept C: Centered Focus
Giant temperature with progress ring showing proximity to setpoint. Minimal.

### Concept D: Split Screen Brew
Dual panels for time/weight progress. Great for dual-target brewing.

### Concept E: Analog Gauge
Skeuomorphic dial (80-100°C range). Visual appeal, analog machine aesthetic.

### Concept F: Minimal Modern
Ultra-clean. Large temp dominates, icon-only status, three-column footer.

### Brew Screen Concepts

![Brew Screen Concepts](../assets/clevercoffee_brew_screen_proposals.png)

#### Real-time Graph
Dual Y-axis chart: pressure curve + weight accumulation over time.

#### Progress Rings
Concentric rings for time/weight completion percentage.

#### Horizontal Bars
Three stacked bars (time, weight, pressure) with numeric values.

#### Shot Profile
Phase indicator (PRE-INFUSION → EXTRACTION → DONE) + dual readout + pressure profile.

---

## Implemented New Templates

### Modern (`MODERN`, enum value 5)

State-aware template: idle screen when ready, brew screen while extracting.

Layout constants and font metrics: **`docs/display-modern-layout.md`**, **`ModernTemplate.h`**.

Idle (variant D):

```
┌────────────────────────────────────────────┐
│ [WiFi] [BT] [MQTT]              02h 15m   │
├────────────────────────────────────────────┤  y=12
│              93.5°C                        │  y=14, fub25 (28px)
│         [cup] READY                        │  y=44, 12×12 icon + status
│  [██████████|░] 94°C                      │  y=58, fill + setpoint tick
└────────────────────────────────────────────┘
```

Brew:

```
┌────────────────────────────────────────────┐
│ PRE-INF ► BREW ► DONE           94°C      │
├────────────────────────────────────────────┤
│           25.3 s                           │
│          /35.0 s                           │
│  [████████████████░░░░░░░░░░]  72%        │
│  18.5g/36g                    9.2 bar      │
└────────────────────────────────────────────┘
```

Features:
- Phase indicator showing brew stage (pre-infusion, brewing, done)
- Large centered brew timer with optional target time
- Visual progress bar (by time or weight depending on config)
- Optional weight and pressure in footer when sensors are available
- Idle screen with large temp, target, READY/HEATING status, and progress bar
