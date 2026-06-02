# Display subsystem architecture

## Component roles

| Component | Responsibility |
|-----------|----------------|
| **DisplayManager** | Hardware RAII: create U8G2, power save, `getDisplay()` |
| **OledDriver** | OLED setup (`prepareDisplay`) and deferred flush (`forceUpdate`) only |
| **UICoordinator** | Cross-cutting flags: `displayBufferReady`, brew-timer FSM state, website/MQTT busy |
| **DisplayTemplateManager** | Single entry: `printScreen()`, `setSystemContext()` |
| **DisplayTemplateBase + templates/** | CRTP pipeline + per-template `renderNormalDisplay()` override |
| **displayHelpers.h** | Tolerance / near-setpoint / heating condition math (template-agnostic) |
| **DisplayTemplatePolicy.h** | Per-template `DisplayPolicy` — which shared pipeline stages run |
| **DisplayWidgets / DisplayFullscreenModes / DisplaySystemScreens** | Shared drawing primitives and default system screens |

Templates draw via raw `U8G2*` from `hardwareContext().display()`. OledDriver does not draw content.

## Frame lifecycle

Each display timer tick calls `DisplayTemplateManager::printScreen()`:

1. **Fullscreen modes** — brew timer, hot water, manual flush (deferred flush); offline splash (immediate)
2. **`drawSystemScreen()`** — standby, heating logo, steam, errors (gated by template `DisplayPolicy`)
3. **`renderNormalDisplay()`** — template-specific idle/brew layout (deferred flush)

`LoopManager::updateDisplay()` flushes deferred buffers via `OledDriver::forceUpdate()` when bus arbitration allows it, then clears `displayBufferReady`.

## Buffer policy (truth table)

| Stage | sendBuffer | displayBufferReady after stage | LoopManager flush |
|-------|------------|-------------------------------|-------------------|
| Fullscreen brew / hot water / manual flush | No (deferred) | **true** | Yes, when canUpdate |
| Offline splash (`displayOfflineMode`) | Yes (immediate) | **false** | No |
| System screen (standby, heating, steam, errors) | Yes (immediate) | **false** | No |
| Normal template draw | No (deferred) | **true** | Yes, when canUpdate |

If `displayBufferReady` stays true while flush is blocked, website SSE/history can stall (`updateWebsite` waits for `!isDisplayBufferReady()`).

## Shared vs template-specific

| Concern | Default | Template override |
|---------|---------|-------------------|
| Standby, PID off, steam, water, errors | `drawSystemScreen()` | Upright portrait coords via template-id branch |
| Heating logo (>5°C below setpoint) | `DisplayPolicy::sharedHeatingLogoScreen()` | e.g. Modern/Minimal skip; idle shows HEATING row instead |
| Fullscreen brew / flush / hot water | `DisplayPolicy` flags | e.g. Modern skips shared fullscreen brew |
| Idle / brew layout | — | `renderNormalDisplay()` |
| Fully custom system screens | — | Override `tryDrawSystemScreen()` (protected) |

## Standby coordinator

- `getRemainingTimeMillis()` — countdown **until** standby activates
- `shouldTurnOffDisplay()` — true after standby + display-off delay (OLED power save)

Use `shouldTurnOffDisplay()` for display/network gating — not `getRemainingTimeMillis()`.

## Near-setpoint helpers (`displayHelpers.h`)

- `isNearSetpointForDisplay()` — strict `< displayBlinkingDelta` (OLED blink / READY)
- `isNearSetpointForStatusLed()` — `<= tolerance` (steam uses 5°C, else blink delta)
- `isHeatingLogoConditionMet()` — config + PID normal + more than `HEATING_LOGO_THRESHOLD_C` below setpoint (no template id; gated by `DisplayPolicy`)

See ADR 0001 for design decisions.
