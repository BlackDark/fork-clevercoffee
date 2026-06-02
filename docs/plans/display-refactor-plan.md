# Display subsystem refactor plan (v3)

**Goal:** KISS, clear structure, one source of truth for display-relevant calculations, shared defaults with optional template overrides, per-template files.

**Non-goals:** Visual redesign, new templates, ProcessController changes, frontend changes.

---

## Prerequisites (block Phase 3 until decided)

| # | Decision | Chosen default (implement unless overridden) |
|---|----------|-----------------------------------------------|
| P1 | Modern >5°C below setpoint | Fullscreen heating logo (same as Standard) |
| P2 | Upright system screens | **Rotation-aware default drawer** — one shared function branches on `DisplayTemplate::UPRIGHT` for portrait coords; delete dead `UprightTemplate::handleSpecialStates` code. No virtual interface. |

---

## Current problems (verified)

| Problem | Impact |
|---------|--------|
| `displayCommon.h` (~1017 lines) | God-file: widgets + machine screens + fullscreen modes + brew timer FSM |
| `ModernDisplayTemplate.h` (~957 lines) | All 6 templates + CRTP base + manager in one header |
| Threshold drift | Standard/Upright hardcode `< 0.3`; Modern uses `displayBlinkingDelta`; LED uses `<= displayBlinkingDelta` |
| Heating logo | Magic `5.` in `displayMachineState`; not a named constant |
| **Two brew-timer FSMs** | `displayCommon.h` (static) vs `UIManager` (member); LoopManager advances the wrong one when brew switch enabled → post-brew timer desync |
| Upright routing | Central `displayMachineState()` draws landscape coords; Upright portrait handlers exist but are **unreachable dead code** |
| Dead code | `HardwareManager::updateLEDs`, most `UIManager` display draw methods, `displayTemplateManager.cpp` one-liner |
| Misleading names | `ModernDisplayTemplate` / `ModernDisplayTemplateManager` name all templates |

---

## Target architecture (minimal layers)

### End-state roles (after refactor — no legacy overlap)

| Component | Responsibility | Removed |
|-----------|----------------|---------|
| **DisplayManager** | Hardware only: create U8G2, power save, `getDisplay()` | — |
| **DisplayTemplateManager** | Single entry: `printScreen()`, boot logo, template dispatch | `displayTemplateManager.cpp`, `ModernDisplayTemplateManager` name |
| **DisplayTemplateBase + templates/** | Inheritance pipeline + per-template layout overrides | `ModernDisplayTemplate.h`, `displayCommon.h` |
| **displayHelpers.h** | All tolerance/near-setpoint/heating-logo math (display **and** LED) | Magic numbers in LoopManager/templates |
| **UICoordinator** | Cross-cutting flags: `displayBufferReady`, website/MQTT busy, brew-timer FSM state | — |
| **LoopManager** | Timing orchestration only — calls helpers, never inline `fabs`/tolerance | Inline `nearSetpoint` in `updateLEDs` |
| **UIManager → deleted or renamed `OledDriver`** | **Only** `prepareDisplay()` + `forceUpdate()` (I2C flush) | All draw methods, duplicate FSM, duplicate widgets |

Today UIManager is **not** clean: it duplicates `displayCommon.h` but templates bypass it and draw via raw `U8G2*`. The plan **removes** that duplication; UIManager shrinks to ~50 lines of flush/setup or merges into DisplayManager.

```
LoopManager (orchestration — no tolerance math inline)
    ↓
DisplayTemplateManager::printScreen()     ← one entry point
    ↓
DisplayTemplateBase<Derived>::printScreen() ← inheritance pipeline (CRTP)
    ├─ shared stages (fullscreen, system screens)
    └─ Derived::renderNormalDisplay()       ← template override
    ↓
displayHelpers.h (thresholds for display + status LED)
    ↓
U8G2* via hardwareContext().display()
```

### Your inheritance idea = what we already have (CRTP)

You described exactly the current CRTP pattern — efficient on ESP32 (no virtual dispatch):

```cpp
// Base defines the pipeline; each template overrides one hook
template<typename Derived>
struct DisplayTemplateBase {
    void printScreen() {
        if (handleFullscreenModes()) return;
        if (handleMachineStates()) { setDisplayBufferReady(false); return; }
        static_cast<Derived*>(this)->renderNormalDisplay();  // ← override point
        setDisplayBufferReady(true);
    }
};

struct ModernTemplate : DisplayTemplateBase<ModernTemplate> {
    void renderNormalDisplay();  // Modern layout only
};
```

Shared screens (standby, heating logo) run **before** `renderNormalDisplay()` — default behavior for all templates. A template overrides only its normal idle/brew layout. v1 does **not** add virtual methods; optional per-template system-screen overrides stay future-only.

No snapshot struct. No virtual override interface.

---

## Per-tick data flow

```
LoopManager::run()
  ├─ updateLEDs()          → displayHelpers: isNearSetpointForStatusLed(ctx)  // replaces inline fabs/tolerance
  ├─ updateDisplay()       → if bufferReady && canUpdate → OledDriver::forceUpdate()  (today: UIManager)
  └─ updateWebsite()       → blocked while displayBufferReady (I2C arbitration)

100ms timer → DisplayTemplateManager::printScreen()
  └─ Template::printScreen()
       1. handleFullscreenModes()   → deferred fullscreen OR offline immediate send
       2. handleMachineStates()     → drawSystemScreen / sendBuffer + displayBufferReady=false
       3. renderNormalDisplay()     → displayBufferReady=true
```

**Owner rule:** Helpers read live `SystemContext` + `Config` at call site. No cached snapshot. LED and display timer may see slightly different millis — acceptable (same as today).

---

## Buffer policy (truth table — preserve semantics, fix one bug)

| Stage | sendBuffer | displayBufferReady after stage | LoopManager flush |
|-------|------------|-------------------------------|-------------------|
| Fullscreen brew / hot water / manual flush | No (deferred) | **true** (set explicitly) | Yes, when canUpdate |
| **Offline splash** (`displayOfflineMode` via `handleFullscreenModes`) | **Yes (immediate via `displayMessage`)** | **unchanged today (bug)** | No |
| System screen (standby, heating logo, steam, errors via `handleMachineStates`) | **Yes (immediate)** | **false** (set in `printScreen`) | No |
| Normal template draw | No (deferred) | **true** | Yes, when canUpdate |

**Phase 1 fix:** When `handleFullscreenModes()` returns true for offline (or any path that calls `displayMessage` / immediate `sendBuffer`), set `displayBufferReady = false` — same as system screens. Prevents stale buffer flag blocking web history.

Cross-ref: `docs/display-architecture.md`.

**Regression checklist (manual, each phase):**
- Standby screen visible before deep power-off
- Web temps/history continue during standby (buffer not stuck)
- Brew-state aggressive display flush
- Post-brew timer with brew switch ON and OFF

---

## Shared helpers (single source of truth)

New header: `include/clevercoffee/display/displayHelpers.h`

Free functions only — no struct. Used by **templates, LoopManager LEDs, and system screens**.

```cpp
constexpr float HEATING_LOGO_THRESHOLD_C = 5.0f;  // in constants/Temperature.h

// Tolerance selection (single place — replaces LoopManager#253-255 inline logic)
double getStatusLedTolerance(MachineStateId state);
// steam → TEMP_TOLERANCE_STEAM_C (5°C); else → displayBlinkingDelta

// Near-setpoint — preserve existing comparator semantics until intentionally unified:
bool isNearSetpointForDisplay(double temp, double setpoint);  // tempDelta < displayBlinkingDelta
bool isNearSetpointForStatusLed(double temp, double setpoint, MachineStateId state);
// tempDelta <= getStatusLedTolerance(state)  AND eligible PID/backflush state (LoopManager policy)

bool shouldShowHeatingLogo(const SystemContext&);             // config + PID_NORMAL + > threshold
bool isBlinkPhaseOn(const SystemContext&);                    // isrCounter() < 500
bool skipsFullscreenHeatingLogo(DisplayTemplate id);          // MINIMAL, TEMPERATURE_ONLY only
```

**LoopManager::updateLEDs** becomes:

```cpp
const bool nearSetpoint = isNearSetpointForStatusLed(temperature, setpoint, machineState);
const bool eligibleState = ...;  // keep state eligibility in LoopManager OR move to helper if reused
```

**Consumers migrate incrementally:**
- `LoopManager::updateLEDs` → `isNearSetpointForStatusLed` + `getStatusLedTolerance` (removes duplicate tolerance pick)
- `displayMachineState` heating gate → `shouldShowHeatingLogo`
- Standard/Upright thermometer → `isNearSetpointForDisplay` + `isBlinkPhaseOn`
- Modern idle row / bar tick → same

**Not unified in v1:** `<` vs `<=` at exact tolerance — preserved intentionally; add native tests documenting both.

**Delete in Phase 1:** `HardwareManager::updateLEDs` (dead; uses hardcoded 0.3).

---

## System screens: one default drawer

New: `DisplaySystemScreens.h` — extract from `displayCommon.h`

```cpp
bool drawSystemScreen(U8G2* d, SystemContext& ctx);
// Returns true if a system screen was drawn (caller sets buffer policy).
// Covers: heating logo → pid off → standby → steam → water empty → backflush → emergency → sensor/EEPROM errors
// (Offline splash stays in DisplayFullscreenModes — not here)
// UPRIGHT branch: portrait coordinates for standby, water empty, sensor error
// skipsFullscreenHeatingLogo(template): skip heating logo block for Minimal/Temp-only
```

**Upright fix (Prerequisite P2):** Portrait coords inside shared drawer — not a separate unreachable template path.

**CRTP hook (future-only — do not implement in v1):** `tryDrawSystemScreen()` default `false`. v1 uses template-id branch inside `drawSystemScreen()` only.

---

## Brew timer — one FSM (Phase 1 bugfix)

**Root cause:**

```cpp
// LoopManager — advances UIManager FSM when brew switch enabled
uiManager_.shouldDisplayBrewTimer();
// Templates — always read displayCommon static FSM
shouldDisplayBrewTimer(systemContext_);
```

**Fix:**
- Move brew-timer **state** to `UICoordinator` member fields (not static in header)
- Logic function in `DisplayBrewTimerState.h`: `bool shouldDisplayBrewTimer(SystemContext&)`
- Remove `UIManager::shouldDisplayBrewTimer` and LoopManager branch — templates and LoopManager call the same function
- Native test: FSM transitions (brew active → finished → post-brew window); Phase 0 tests document pre-fix behavior

---

## File split (Phase 2)

```
include/clevercoffee/display/
├── displayHelpers.h              # thresholds, near-setpoint, heating logo gate
├── DisplayBrewTimerState.h       # FSM + shouldDisplayBrewTimer
├── DisplaySystemScreens.h        # drawSystemScreen (standby, errors, steam, …)
├── DisplayFullscreenModes.h      # offline, fullscreen brew/hot-water/flush timers
├── DisplayWidgets.h              # status bar, thermometer, brew time/weight, progress
├── DisplayTemplateBase.h         # CRTP pipeline (extract from ModernDisplayTemplate.h)
├── DisplayTemplateManager.h      # enum dispatch (merge ModernDisplayTemplateManager)
├── templates/
│   ├── StandardTemplate.h
│   ├── UprightTemplate.h
│   ├── MinimalTemplate.h
│   ├── TemperatureOnlyTemplate.h
│   ├── ScaleTemplate.h
│   └── ModernTemplate.h          # uses existing ModernDisplayLayout.h
├── ModernDisplayLayout.h         # unchanged
├── DisplayManager.h / IDisplayManager.h
├── bitmaps.h, languages.h
```

**Delete after split (direct include updates — no re-export shim):**
- `displayCommon.h`
- `ModernDisplayTemplate.h`
- `src/display/displayTemplateManager.cpp` (inline into header in Phase 2)

**UIManager after cleanup (Phase 3):**
- Rename to **`OledDriver`** (or fold into `DisplayManager`: `prepare()` + `flush()`)
- Keep: `prepareDisplay`, `forceUpdate`, buffer sync flags
- Migrate boot: `SystemInitializer` → `DisplayTemplateManager::showBootLogo()` or shared `drawBootLogo()` in DisplayWidgets
- Delete: all duplicated draw/FSM methods (~350 lines in UIManager.cpp)

**Drawing path v1:** Templates keep raw `U8G2*` via `hardwareContext().display()`. DisplayManager stays hardware init only. Document in architecture doc.

---

## Implementation phases (3 phases, not 6)

### Phase 0 — Behavior baseline (no new production headers)
- Native tests in test file only: characterize **current** threshold comparators (`< 0.3` display vs `<= displayBlinkingDelta` LED) and **current** `shouldDisplayBrewTimer()` static FSM from `displayCommon.h`
- Add buffer truth table (including offline quirk) to `docs/display-architecture.md`
- No new `displayHelpers.h` yet — helpers land in Phase 1

### Phase 1 — Bugfixes + helpers (highest value, smallest diff)
- Add `HEATING_LOGO_THRESHOLD_C`, `displayHelpers.h`
- Wire heating logo, Standard/Upright/Modern to helpers
- Unify brew timer in UICoordinator; remove dual LoopManager call + UIManager duplicate
- Delete `HardwareManager::updateLEDs`
- Rename `usesTemplateBuiltInHeatingDisplay` → `skipsFullscreenHeatingLogo` in same PR (Minimal + Temp-only only)
- Inline `displayTemplateManager.cpp`
- Fix offline `displayBufferReady` flag (see buffer table)

### Phase 2 — Split god headers + per-template files
- Extract DisplayWidgets, DisplayFullscreenModes, DisplaySystemScreens, DisplayBrewTimerState
- Extract DisplayTemplateBase + templates/*.h
- Merge manager into DisplayTemplateManager.h
- Upright portrait coords in DisplaySystemScreens (Prerequisite P2)
- Delete dead Upright `handleSpecialStates` blocks
- Delete `displayCommon.h`, `ModernDisplayTemplate.h`
- **Verify:** compile smoke (all templates), native tests, regression checklist

### Phase 3 — Dead code + docs
- Gut UIManager display duplicates; migrate boot logo
- Update `docs/display-architecture.md`; trim stale `docs/display-analysis.md`
- Optional rename: `ModernDisplayTemplate` → `DisplayTemplateBase` if any references remain

---

## Risks & mitigations

| Risk | Mitigation |
|------|------------|
| Website/history stall (`displayBufferReady` stuck) | Truth table unchanged; regression checklist |
| `shouldTurnOffDisplay()` gating broken | Do not touch LoopManager gating in Phase 2 |
| Upright coordinate regression | P2 branch + manual rotation check |
| Near-setpoint behavior change | Preserve `<` vs `<=`; tests at boundary |
| ESP32 compile time / flash | Split headers only; heavy drawers stay header-inline unless compile pain |
| Native test Config coupling | Helpers take `(temp, setpoint, delta)` overloads for pure tests |

---

## Success criteria

- No file mixes widgets + system screens + template layouts
- Zero magic `0.3` / `5.` in template code (only in helpers/constants)
- One `shouldDisplayBrewTimer` + one state owner (`UICoordinator`)
- Standby/heating/errors: one `drawSystemScreen()` default path
- All phases: format, `pio run -e esp32_usb`, `pio test -e native_test`

---

## Explicitly out of scope (v1)

- `DisplayProcessView` snapshot struct
- Virtual `DisplaySystemScreenOverrides` interface
- Migrating templates to `IDisplayManager` / DisplayManager draw API
- Unifying `<` vs `<=` near-setpoint comparators
- Renaming CRTP base mid-refactor (only Phase 3 optional cleanup)
