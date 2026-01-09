# g_state Global Variable Usage Analysis

**Report Generated:** 2026-01-08

## Overview

Comprehensive analysis of `g_state` global variable member usage across the CleverCoffee codebase. Total unique files with `g_state` references: **8 files**.

---

## Machine State Members

### 1. machineState

| Metric | Value |
|--------|-------|
| **Total References** | 3 |
| **File Count** | 1 |

**Files:**
- `examples/CustomFormattersDemo.cpp` (3 references)
  - Lines: 83, 87, 147

**Usage Pattern:** Demo/example code only. Conditional checks and formatting.

---

### 2. lastmachinestate

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 3. lastmachinestatepid

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 4. emergencyStop

| Metric | Value |
|--------|-------|
| **Total References** | 4 |
| **File Count** | 1 |

**Files:**
- `include/clevercoffee/utils/SystemUtils.h` (4 references)
  - Lines: 56, 57, 59, 60

**Usage Pattern:** Emergency temperature threshold logic. Read/write access for emergency stop control.

---

### 5. steamON

| Metric | Value |
|--------|-------|
| **Total References** | 5 |
| **File Count** | 2 |

**Top Files:**
1. `include/clevercoffee/embeddedWebserver.h` (3 references) - Lines: 375, 377, 379
2. `include/clevercoffee/utils/SystemUtils.h` (2 references) - Lines: 28, 30

**Usage Pattern:** Web server toggle control and utility functions for steam mode management.

---

### 6. steamFirstON

| Metric | Value |
|--------|-------|
| **Total References** | 2 |
| **File Count** | 1 |

**Files:**
- `include/clevercoffee/utils/SystemUtils.h` (2 references)
  - Lines: 31, 33

**Usage Pattern:** Tracks first steam activation state in utility functions.

---

### 7. backflushOn

| Metric | Value |
|--------|-------|
| **Total References** | 3 |
| **File Count** | 1 |

**Files:**
- `include/clevercoffee/embeddedWebserver.h` (3 references)
  - Lines: 405, 406, 410

**Usage Pattern:** Web server backflush mode toggle and state management.

---

### 8. currBackflushCycles

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 9. waterTankFull

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 10. systemInitialized

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 11. flags (nested fields)

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** No `g_state.machine.flags.*` references found in active codebase.

---

## Network State Members

### 1. cleverCoffeeWiFiManager

| Metric | Value |
|--------|-------|
| **Total References** | 4 |
| **File Count** | 3 |

**Top Files:**
1. `src/core/LoopManager.cpp` (1 reference) - Line: 488
2. `src/core/SystemInitializer.cpp` (1 reference) - Line: 309
3. `examples/CustomFormattersDemo.cpp` (2 references) - Lines: 153, 154

**Usage Pattern:** Pointer initialization and WiFi signal strength checking.

---

### 2. webServerManager

| Metric | Value |
|--------|-------|
| **Total References** | 2 |
| **File Count** | 2 |

**Top Files:**
1. `src/core/LoopManager.cpp` (1 reference) - Line: 517
2. `src/core/SystemInitializer.cpp` (1 reference) - Line: 326

**Usage Pattern:** Pointer initialization and null checks.

---

### 3. offlineMode

| Metric | Value |
|--------|-------|
| **Total References** | 3 |
| **File Count** | 2 |

**Top Files:**
1. `src/core/LoopManager.cpp` (2 references) - Lines: 440, 509
2. `include/clevercoffee/utils/SystemUtils.h` (1 reference) - Line: 76

**Usage Pattern:** Offline mode detection and state management in loop and utility functions.

---

### 4. wifiReconnects

| Metric | Value |
|--------|-------|
| **Total References** | 1 |
| **File Count** | 1 |

**Files:**
- `src/core/LoopManager.cpp` (1 reference)
  - Line: 484

**Usage Pattern:** Reset to zero after successful WiFi connection.

---

### 5. lastWifiConnectionAttempt

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 6. lastTempEvent

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 7. tempEventInterval

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 8. mqttManager

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 9. mqttVars

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 10. mqttSensors

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 11. mqtt_was_connected

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 12. MQTTReCnctCount

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 13. lastMQTTConnectionAttempt

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

### 14. hassioFailed

| Metric | Value |
|--------|-------|
| **Total References** | 4 |
| **File Count** | 2 |

**Top Files:**
1. `src/network/MQTTManager.cpp` (3 references) - Lines: 631, 706, 712
2. `src/context/SystemContext.cpp` (1 reference) - Line: 169

**Usage Pattern:** MQTT/Hassio failure flag management and state synchronization.

---

## Display State Members

### 1. displayOffline

| Metric | Value |
|--------|-------|
| **Total References** | 0 |
| **File Count** | 0 |

**Status:** Not found in active codebase.

---

## Summary Statistics

| Category | Total References | Files Used | Active Members |
|----------|------------------|------------|----------------|
| **Machine State** | 17 | 4 | 6/11 |
| **Network State** | 14 | 4 | 4/14 |
| **Display State** | 0 | 0 | 0/1 |
| **TOTAL** | **31** | **8** | **10/26** |

---

## Key Findings

### High-Usage Members
1. **emergencyStop** - 4 references (emergency safety logic)
2. **steamON** - 5 references (steam mode control)
3. **hassioFailed** - 4 references (MQTT integration)
4. **offlineMode** - 3 references (connectivity detection)
5. **backflushOn** - 3 references (backflush mode)

### Inactive Members (Not Referenced)
- `lastmachinestate`
- `lastmachinestatepid`
- `currBackflushCycles`
- `waterTankFull`
- `systemInitialized`
- `flags` (nested fields)
- All MQTT-related members except `hassioFailed`
- Network timing members (lastWifiConnectionAttempt, lastTempEvent, tempEventInterval)
- `displayOffline`

### File Concentration
- **Most used:** `include/clevercoffee/utils/SystemUtils.h` (9 references)
- **Second:** `include/clevercoffee/embeddedWebserver.h` (6 references)
- **Loop & Init:** `src/core/LoopManager.cpp` and `src/core/SystemInitializer.cpp` (5 references each)

### Access Patterns
- **Headers (utility/web):** Mostly direct member access
- **Core loop/init:** Pointer checks and null validation
- **MQTT/Context:** Boolean flags and state synchronization

---

## Recommendations

1. **Investigate inactive members** - Consider removing or documenting why `lastmachinestate`, `currBackflushCycles`, `waterTankFull`, and `systemInitialized` exist if unused.

2. **MQTT member inconsistency** - Only `hassioFailed` is actively used; investigate why other MQTT members (`mqttManager`, `mqttVars`, `mqttSensors`) aren't referenced.

3. **Display state unused** - `displayOffline` has zero references; determine if this represents incomplete implementation.

4. **Refactor utility functions** - `SystemUtils.h` has concentrated global state access; consider encapsulating this behavior.

5. **Network timing members** - `lastWifiConnectionAttempt`, `lastTempEvent`, `tempEventInterval` are unused; clean up or implement.

