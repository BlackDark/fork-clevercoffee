# CleverCoffee — Task List & Recommendations

**Last Updated:** 2026-03-28  
**Status:** Build passes, 234 tests pass  
**Based on:** `improvement.md`, `architecture.md`, `code-review-2026-02-14.md`, `implementation-plan.md`

---

## Executive Summary

Build succeeds and all 234 tests pass. The codebase is in good shape with most critical issues already resolved from prior work.

---

## Priority 1: Critical Issues

| ID | Task | Status | Location | Notes |
|----|------|--------|----------|-------|
| P1.1 | VLA in MQTT callback | **FIXED** | `src/network/MQTTManager.cpp:236` | Already using fixed buffer (MAX_DATA_LEN=1024) |
| P1.2 | `sscanf` field-width limits | **FIXED** | `src/network/MQTTManager.cpp:247` | Already using bounded format `%119[^/]/%63[^/]` |
| P1.3 | CORS restriction | **IGNORED** | Per user request | Not a priority |
| P1.4 | Config upload size limit | **PENDING** | WebServerManager | Low risk - ESP32 has built-in limits |
| P1.5 | Password redaction | **PENDING** | WebServerManager | Low risk - internal network only |
| P1.6 | Water tank atomic check | **FIXED** | HardwareManager | Already uses `std::atomic<bool>` |

---

## Priority 2: Code Quality

| ID | Task | Status | Location | Notes |
|----|------|--------|----------|-------|
| P2.1 | Handler ownership with `unique_ptr` | **FIXED** | `SystemContext.cpp:492-495` | Already using `std::unique_ptr` |
| P2.2 | Nullable accessor `noexcept` | **FIXED** | `SystemContext.h:201-213` | Already has `assert()` guards |
| P2.3 | IConfig interface | **EXISTS** | `include/clevercoffee/IConfig.h` | Defined for testability |
| P2.4 | Handler tests | **PASSING** | All 234 tests | Brew, Steam, HotWater, Power all pass |
| P2.5 | Inject Config into handlers | **N/A** | Current design | Uses direct ParamDef access pattern |
| P2.6 | Deprecated methods | **FIXED** | Code review 2026-02-14 | Already removed |

**Recently Fixed:**
- Fixed unused return value warnings in `main.cpp:173` (finalizeMachineState)
- Fixed unused return value warnings in `WebServerManager.cpp:368,406` (set calls)

---

## Priority 3: Technical Debt (Deferred)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| P3.1 | Refactor SystemContext | **DEFERRED** | Large undertaking, not critical |
| P3.2 | Naming conventions | **DEFERRED** | Works as-is |
| P3.3 | Forward declarations | **DEFERRED** | Minor issue |
| P3.4 | Non-blocking logger | **DEFERRED** | Works adequately |

---

## Priority 4: TODOs in Code (Feature Gaps)

| ID | Task | Status | Location |
|----|------|--------|----------|
| P4.1 | User activity detection | **PENDING** | `MachineStateContext.cpp:381` |
| P4.2 | Scale integration | **PENDING** | `HardwareManager.cpp:324,329,695,701` |
| P4.3 | PWM/SSR heater control | **PENDING** | `HardwareManager.cpp:371` |
| P4.4 | Pump pressure control | **PENDING** | `HardwareManager.cpp:422` |
| P4.5 | Solenoid control | **PENDING** | `HardwareManager.cpp:557,568` |

---

## Test Coverage

| ID | Task | Status | Notes |
|----|------|--------|-------|
| T1 | Disabled tests | **2** | Only 2 disabled (NVS tests require hardware) |
| T2 | Test count | **234 PASSED** | All tests passing |

---

## Implementation Progress

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | Safety-Critical Fixes | **COMPLETE** |
| Phase 2 | Handler Ownership | **COMPLETE** |
| Phase 3 | Test Infrastructure | **COMPLETE** |
| Phase 4 | Enable Handler Tests | **COMPLETE** |
| Phase 5 | Config Injection | **N/A** - Current pattern works |
| Phase 6 | StateMachine Injection | **COMPLETE** |
| Phase 7 | Deprecated Removal | **COMPLETE** |
| Phase 8 | Enable Tests | **PARTIAL** - 2 remain |

---

## Verification Commands

```bash
# Build
mise exec python@3.13 -- pio run -e esp32_usb

# Tests
mise exec python@3.13 -- pio test -e native_test
```

---

## Current Status

- **Build:** ✅ SUCCESS (86% Flash, 13% RAM)
- **Tests:** ✅ 234/234 PASSED
- **Warnings:** Fixed unused return value warnings in main.cpp and WebServerManager.cpp