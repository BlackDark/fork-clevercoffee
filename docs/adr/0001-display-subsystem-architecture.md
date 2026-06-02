# ADR 0001: Display subsystem architecture

## Status

Accepted (2026-05-22)

## Context

The OLED display code had grown into two large headers (`displayCommon.h`, `ModernDisplayTemplate.h`) with overlapping responsibilities. Multiple managers (UIManager, DisplayManager, DisplayTemplateManager) duplicated drawing logic. Threshold math (`0.3°C`, `5°C`, `displayBlinkingDelta`) was scattered across templates, LoopManager, and dead code. Two independent brew-timer state machines could desync when the brew switch was enabled.

## Decision

### 1. Single render pipeline (CRTP, no virtual interface)

All templates inherit `DisplayTemplateBase<Derived>` with a fixed stage order:

1. Fullscreen modes (`DisplayFullscreenModes.h`)
2. System screens (`DisplaySystemScreens.h` / `drawSystemScreen()`)
3. Template normal layout (`renderNormalDisplay()`)

No virtual override table. Each template defines `using DisplayPolicy = …` (`DisplayTemplatePolicy.h`) to opt in/out of shared heating logo and fullscreen timers. Optional `tryDrawSystemScreen()` override replaces all shared system screens for that template.

### 2. Shared defaults, template overrides only for layout

Standby, steam, water empty, errors, and PID-off screens are drawn centrally when the template policy allows. Heating logo and fullscreen brew timers are also policy-gated. Templates customize idle/brew via `renderNormalDisplay()`. Upright portrait coordinates use a template-id branch inside `drawSystemScreen()`.

### 3. Single source of truth for thresholds (`displayHelpers.h`)

- `HEATING_LOGO_THRESHOLD_C` (5°C) for fullscreen heating logo
- `isNearSetpointForDisplay()` — strict `<` for OLED blink semantics
- `isNearSetpointForStatusLed()` — `<=` with steam vs normal tolerance
- `isHeatingLogoConditionMet()`, `isBlinkPhaseOn()` — pure conditions; template `DisplayPolicy` decides whether shared heating logo runs

LoopManager `updateLEDs()` calls helpers; no inline `fabs`/tolerance logic.

Comparators `<` vs `<=` are intentionally preserved at the exact tolerance boundary.

### 4. One brew-timer display FSM

State lives on `UICoordinator`. Single function `shouldDisplayBrewTimer()` in `DisplayBrewTimerState.h`. Removed duplicate from UIManager and static header-local FSM.

### 5. Manager roles narrowed

| Component | Role |
|-----------|------|
| DisplayManager | Hardware init only |
| OledDriver | `prepareDisplay()` + `forceUpdate()` + buffer flags |
| UICoordinator | Coordination flags + brew-timer FSM state |
| DisplayTemplateManager | `printScreen()` dispatch |

Boot logo uses `displayLogo()` from `DisplayWidgets.h` via `SystemContext`, not OledDriver.

### 6. File split

```
display/
├── displayHelpers.h, DisplayBrewTimerState.h
├── DisplayWidgets.h, DisplayFullscreenModes.h, DisplaySystemScreens.h
├── DisplayTemplateBase.h, DisplayTemplateManager.h
├── templates/*.h
└── ModernTemplate.h (layout helpers in ModernTemplateLayout namespace)
```

No backward-compatibility shims; direct include updates.

### 7. Buffer policy unchanged (with one bugfix)

Deferred flush for normal/fullscreen brew paths; immediate `sendBuffer()` for system screens. Offline splash now clears `displayBufferReady` like other immediate paths to avoid web history deadlock.

## Consequences

**Positive**

- Clear ownership: drawing in `display/*`, flush in OledDriver, thresholds in helpers
- Real bugfixes: unified brew timer, offline buffer flag, Upright standby reachable
- Smaller compile units; templates in separate files
- Native tests for threshold comparators (`test_display_helpers`) and brew-timer FSM (`test_display_brew_timer`)

**Negative / tradeoffs**

- Templates still use raw `U8G2*` (DisplayManager draw API not adopted)
- Header-only drawers increase compile coupling (acceptable for ESP32 project size)

## Alternatives considered

- **DisplayProcessView snapshot struct** — rejected as over-engineering for ESP32
- **Virtual DisplaySystemScreenOverrides** — rejected; conflicts with CRTP and adds vtable cost
- **Merge OledDriver into DisplayManager** — rejected; separate flush driver kept for clarity

## References

- `docs/plans/display-refactor-plan.md`
- `docs/display-architecture.md`
- `docs/display-modern-layout.md`
