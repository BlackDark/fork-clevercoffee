# Simplified g_state Elimination Plan - REVISED

> **For Claude:** Use superpowers:executing-plans or subagent-driven-development to implement this plan task-by-task.

**Goal:** Complete g_state elimination by routing ALL global state access through SystemContext, without over-abstraction or unnecessary service layers.

**Architecture:** SystemContext becomes the ONLY point of access to global state. It provides simple accessor methods that delegate to g_state members. This single abstraction layer improves testability and encapsulation without unnecessary indirection.

**Why This Approach:**
- Single responsibility: SystemContext is the "get state from globals" adapter
- Minimal abstraction: One layer, not five
- Proven pattern: Already working with g_state.process (245 refs eliminated)
- Low risk: Mechanical refactoring, not architectural changes
- Easy to remove: Just delete g_state and update SystemContext when done

---

## Phase 0: Analysis Complete ✅

Reference counts:
- ProcessController.cpp: 22 refs (mostly g_state.pid->Method() calls)
- SystemContext.cpp: 49 refs (delegation layer - intentional)
- WebServerManager.cpp: 7 refs
- Display templates: 12-20 refs total
- Other files: <10 refs each

**Key insight:** g_state.pid is NOT state, it's a pointer to a third-party PID controller object. This should stay in SystemContext as-is.

---

## Phase 1: Add Missing Accessors to SystemContext (1-2 hours)

### Task 1.1: Audit All g_state Member Access

**Files:**
- Read: All files with g_state references
- Reference: G_STATE_COMPLETE_ANALYSIS.md

**Steps:**
1. For each file with g_state refs, identify which struct members are accessed
2. Check if SystemContext already has accessors for those members
3. List any missing accessors needed

**Expected Output:** List of missing SystemContext accessor methods needed

---

## Phase 2: Add Missing Accessors to SystemContext (2-3 hours)

### Task 2.1: Add Sensor State Accessors

**Files:**
- Modify: `include/clevercoffee/context/SystemContext.h`
- Modify: `src/context/SystemContext.cpp`

Members to expose (currently missing):
- `sensors.currBrewWeight`
- `sensors.scaleTareOn`
- `sensors.scaleCalibrationOn`
- `sensors.inputPressure`
- `sensors.scaleFailure`
- `sensors.currReadingWeight`
- `sensors.preBrewWeight`
- `sensors.inputPressureFilter`

**Implementation:** Add getter/setter pairs for each, delegate to g_state

**Test:** Verify compilation, existing tests pass

**Commit:** `feat: add sensor state accessors to SystemContext`

---

### Task 2.2: Add Network State Accessors

**Files:**
- Modify: `include/clevercoffee/context/SystemContext.h`
- Modify: `src/context/SystemContext.cpp`

Members to expose:
- `network.offlineMode`
- `network.wifiReconnects`
- `network.hassioFailed`
- `network.cleverCoffeeWiFiManager`
- `network.webServerManager`

**Commit:** `feat: add network state accessors to SystemContext`

---

### Task 2.3: Add Coordination State Accessors

**Files:**
- Modify: `include/clevercoffee/context/SystemContext.h`
- Modify: `src/context/SystemContext.cpp`

Members to expose:
- `coordination.displayBufferReady`
- `coordination.hassioUpdateRunning`
- `coordination.temperatureUpdateRunning`
- `coordination.processController` (already exists)

**Commit:** `feat: add coordination state accessors to SystemContext`

---

## Phase 3: Replace g_state Access with SystemContext (3-4 hours)

### Task 3.1: Update ProcessController

**Files:**
- Modify: `src/control/ProcessController.cpp`

**Changes:**
- Replace `g_state.pid` references - KEEP as is (it's a third-party object pointer)
- Actually, ProcessController already uses this correctly - verify no changes needed

**Commit:** `refactor: verify ProcessController only uses g_state.pid correctly`

---

### Task 3.2: Update WebServerManager

**Files:**
- Modify: `src/network/WebServerManager.cpp`

**Changes:**
- Replace `g_state.sensors.scaleCalibrationOn` → `ctx.sensorScaleCalibrationOn()`
- Replace `g_state.sensors.scaleTareOn` → `ctx.sensorScaleTareOn()`
- Replace other sensor/network refs with SystemContext accessors

**Commit:** `refactor: WebServerManager uses SystemContext accessors`

---

### Task 3.3: Update Display Templates

**Files:**
- Modify: `include/clevercoffee/display/ModernDisplayTemplate.h`
- Modify: `include/clevercoffee/display/displayCommon.h`

**Changes:** Replace all g_state access with SystemContext accessors

**Commit:** `refactor: Display templates use SystemContext accessors`

---

### Task 3.4: Update embeddedWebserver.h

**Files:**
- Modify: `include/clevercoffee/embeddedWebserver.h`

**Changes:** Replace all g_state access with SystemContext accessors

**Commit:** `refactor: embeddedWebserver uses SystemContext accessors`

---

### Task 3.5: Update Config.h

**Files:**
- Modify: `include/clevercoffee/Config.h`

**Changes:** Replace all g_state access in callbacks with SystemContext accessors

**Commit:** `refactor: Config callbacks use SystemContext accessors`

---

### Task 3.6: Update isr.h

**Files:**
- Modify: `include/clevercoffee/isr.h`

**Changes:** Replace g_state access with SystemContext accessors (already done mostly)

**Commit:** `refactor: isr uses SystemContext accessors`

---

### Task 3.7: Update MQTTManager

**Files:**
- Modify: `src/network/MQTTManager.cpp`

**Changes:** Replace g_state access with SystemContext accessors

**Commit:** `refactor: MQTTManager uses SystemContext accessors`

---

### Task 3.8: Update Remaining Files

**Files:**
- `src/main.cpp`
- `src/core/SystemInitializer.cpp`
- `src/core/LoopManager.cpp`
- `include/clevercoffee/utils/SystemUtils.h`
- `include/clevercoffee/utils/helperUtils.h`
- Other files with g_state refs

**Changes:** Replace all g_state access with SystemContext accessors

**Commit:** `refactor: all remaining files use SystemContext accessors`

---

## Phase 4: Cleanup - Remove Dead Code (1 hour)

### Task 4.1: Remove Unused g_state Members

**Files:**
- Modify: `include/clevercoffee/GlobalState.h`

**Changes:** Remove all struct members identified as unused in analysis:
- HardwareRefs (all 18)
- StandbyState (all 5)
- 7 timing members (keep isrCounter)
- 15+ sensor state members (scale error recovery, etc.)
- 9 network members (MQTT, legacy timers)

**Expected:** ~43 unused members removed
**Commit:** `cleanup: remove 43 unused g_state members`

---

## Phase 5: Final Verification (1 hour)

### Task 5.1: Verify All g_state Access Eliminated

**Command:**
```bash
rg "g_state\." --type cpp --type h
```

**Expected:** Only results in:
- `include/clevercoffee/GlobalState.h` (struct definition)
- `src/context/SystemContext.cpp` (delegation implementation)
- Comments/examples only

**Commit:** `cleanup: verify all g_state access migrated to SystemContext`

---

## Phase 6: Remove GlobalState (OPTIONAL - future work)

At this point, we have a clean abstraction. The global g_state still exists but:
- ZERO direct access from application code
- ALL access flows through SystemContext
- Easy to test and mock
- Ready to completely remove later if desired

**Decision:** Keep g_state for now, make tests pass, deliver value. Complete removal can be Phase 7 in future session.

---

## Testing Strategy

- Run existing tests after each phase: `pio test -e esp32_usb`
- Build after each phase: `pio run -e esp32_usb -s`
- No new tests needed (refactoring only)
- Verify no regressions in functionality

---

## Risk Assessment

| Task | Risk | Mitigation |
|------|------|-----------|
| Add SystemContext accessors | Low | Simple delegation, no logic changes |
| Replace g_state refs | Low | Mechanical replacements, tests verify correctness |
| Remove unused members | Zero | No code uses them (0 refs verified) |
| Final cleanup | Low | Grep verify confirms all migrated |

---

## Estimated Timeline

| Phase | Tasks | Effort | Total |
|-------|-------|--------|-------|
| 1 | Audit refs | 30 min | 30 min |
| 2 | Add accessors | 3 tasks | 2-3 hrs |
| 3 | Replace refs | 8 tasks | 3-4 hrs |
| 4 | Remove unused | 1 task | 1 hr |
| 5 | Verify | 1 task | 1 hr |
| **TOTAL** | **13 tasks** | **7.5-9 hrs** | |

---

## Success Criteria

- ✅ Zero g_state direct access in application code
- ✅ All access flows through SystemContext
- ✅ Build passes cleanly
- ✅ All tests pass
- ✅ Memory/performance unchanged
- ✅ Code simpler and more testable

---

## Key Principles

1. **Don't over-abstract** - One SystemContext layer is enough
2. **Mechanical refactoring** - No algorithm changes, just redirecting access
3. **Test-driven** - Verify with builds and tests after each task
4. **Single responsibility** - SystemContext is just the "get from globals" adapter
5. **Keep what works** - g_state.pid is fine as-is, it's not our state

---

## Next Steps

Ready for execution. Choose:

1. **Subagent-Driven** (this session) - Fresh agent per task
2. **Manual** - Execute locally
3. **Parallel Session** - Executing-plans skill

Which approach?
