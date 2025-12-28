# Phase 5 Integration Requirements & Analysis

**Research Date:** 2025-12-28  
**Status:** Complete Integration Plan Ready

---

## Executive Summary

Phase 5 requires integrating the new **SensorCoordinator** into the main execution loop. The codebase currently has a hybrid architecture where:
- **SensorManager** (legacy) exists and handles sensor read operations
- **SensorCoordinator** (new) exists but is not yet called from the main loop
- **SystemContext** owns the SensorCoordinator but it remains dormant

This document provides the exact integration points, current flow diagrams, and a step-by-step migration path.

---

## 1. Main Loop Architecture Overview

### Current Loop Execution (main.cpp → loop())

```
loop()
  └── loopManager->update()  [src/core/LoopManager.cpp:80]
        ├── 1. Logger::update()
        ├── 2. updateWaterTank()
        ├── 3. updateProcessControl()
        ├── 4. updateLEDs()
        ├── 5. updateNetwork()
        ├── 6. updateWebsite()
        ├── 7. updateCentralizedSensorTimers()  [THIS IS WHERE SENSORS UPDATE]
        ├── 8. updateSwitchesAndStandby()
        ├── 9. updateStateMachine()
        ├── 10. updateDisplay()
        └── 11. updateDebugTiming()
```

**Key File:** `src/main.cpp:177-182`
- The entire loop is coordinated by `LoopManager::update()`
- Called from Arduino's built-in `loop()` function

---

## 2. Current SensorManager Flow

### 2.1 SensorManager Initialization

**Location:** `src/core/SystemInitializer.cpp:402-435` (`initializeSensors()`)

```cpp
// Line 405: Create SensorManager
sensorManager_ = std::make_unique<SensorManager>();

// Lines 408-409: Get hardware references
TempSensor* tempSensorRef = hardwareManager_->getTempSensor();
Switch* waterTankSensorRef = hardwareManager_->getWaterTankSensor();

// Line 412: Get coordinator from SystemContext
CleverCoffee::SensorCoordinator* coord = systemContext_->sensorCoordinator();

// Line 414: Initialize SensorManager with coordinator reference
if (sensorManager_->initialize(tempSensorRef, waterTankSensorRef, coord))
```

**Current State:**
- SensorManager is created as a unique_ptr in SystemInitializer
- It receives a non-owning pointer to SensorCoordinator
- SensorManager stores this pointer but doesn't use it (it's prepared for Phase 5)

### 2.2 SensorManager Global Access

**Location:** `src/main.cpp:70`
```cpp
SensorManager* sensorManager = nullptr;
```

**Populated:** `src/main.cpp:131` (during setup)
```cpp
SensorManager* sensorManager = systemInitializer->getSensorManager();
```

### 2.3 Sensor Reading Methods

SensorManager provides these methods (include/clevercoffee/sensors/SensorManager.h):

| Method | Line | Purpose | Called By |
|--------|------|---------|-----------|
| `update()` | 65 | Update temperature with caching & timeout protection | LoopManager::updateTemperatureSensor() |
| `updateWaterTankSensor()` | 102 | Check water tank level | LoopManager::checkWaterTankLevel() |
| `updatePressureSensor()` | 120 | Read pressure sensor | LoopManager::updatePressureSensor() |
| `updateScale()` | 132 | Scale reading (legacy) | Not called in modern loop |
| `getCurrentTemperature()` | 84 | Get cached temp | ProcessController, StateMachine |
| `isWaterTankFull()` | 97 | Get tank state | LoopManager, ProcessController |

### 2.4 Current Sensor Update Locations in Loop

**Temperature sensor:** `src/core/LoopManager.cpp:338-350` (updateTemperatureSensor)
- Calls `sensorManager_->update()` via timer (400ms interval)
- Updates temperature and handles timeouts

**Pressure sensor:** `src/core/LoopManager.cpp:352-367` (updatePressureSensor)
- Calls `sensorManager_->updatePressureSensor()` via timer (50ms interval)
- Caches result in `g_state.sensors`

**Water tank:** `src/core/LoopManager.cpp:331-336` (checkWaterTankLevel)
- Calls `sensorManager_->updateWaterTankSensor()` via timer (200ms interval)
- Updates `g_state.machine.waterTankFull`

**Scale:** `src/core/LoopManager.cpp:369-387` (updateScaleSensor)
- Calls external functions (not SensorManager)
- Uses global state variables

---

## 3. SensorCoordinator Architecture

### 3.1 SensorCoordinator Location & Ownership

**Header:** `include/clevercoffee/coordinators/SensorCoordinator.h`  
**Implementation:** `src/coordinators/SensorCoordinator.cpp`

**Ownership Chain:**
```
SystemContext (owned by SystemInitializer)
  └── SensorCoordinator (member variable, created on stack)
      ├── tempSensor_ (non-owning pointer to ISensor)
      └── scaleSensor_ (non-owning pointer to ISensor)
```

**Access in SystemInitializer:** `src/core/SystemInitializer.cpp:412`
```cpp
CleverCoffee::SensorCoordinator* coord = systemContext_ ? &systemContext_->sensorCoordinator() : nullptr;
```

### 3.2 SensorCoordinator Interface

**Key Method:** `update()` - Called from main loop
- `src/coordinators/SensorCoordinator.cpp:23-26`
- Non-blocking, always returns immediately
- Polls sensors at configurable intervals (TEMP_UPDATE_INTERVAL_MS=400ms, SCALE_UPDATE_INTERVAL_MS=100ms)

**Temperature Methods:**
- `getTemperature()` - Returns cached value
- `hasTemperatureSensorError()` - Atomic read of error flag
- `startTemperatureUpdate() / stopTemperatureUpdate()` (legacy)

**Scale Methods:**
- `getWeight()` - Returns cached weight
- `hasScaleSensorError()` - Atomic read of error flag
- `startScaleUpdate() / stopScaleUpdate()` (legacy)

**General Methods:**
- `hasSensorError()` - Returns true if any sensor has error

### 3.3 SensorCoordinator Dependencies

**Requires ISensor Interface (NOT TempSensor or Switch):**
- `include/clevercoffee/sensors/ISensor.h`
- Provides `startRead()` and `tryGetValue()` methods
- Uses Result<T> type for error handling

**Current Status:**
- Temperature and Scale sensors do NOT implement ISensor yet (Phase 5 work)
- SensorCoordinator accepts `nullptr` for sensors and gracefully skips them

---

## 4. SystemContext Setup

### 4.1 SystemContext Creation

**Location:** `src/core/SystemInitializer.cpp:55`
```cpp
systemContext_ = std::make_unique<CleverCoffee::SystemContext>();
```

**Called In:** `initialize()` at the very start (Phase 1)

### 4.2 SystemContext Lifecycle

1. **Created:** SystemInitializer::initialize() line 55
2. **Populated:** Automatically - SensorCoordinator is created as member
3. **Made Ready:** `systemContext_->markReady()` is called in SystemInitializer (NOT YET IMPLEMENTED)
4. **Used:** Passed to StateMachine constructor in main.cpp:149
5. **Accessed:** `systemContext_->sensorCoordinator()` in SystemInitializer:412

### 4.3 Current SystemContext Issues

**Problem 1:** SensorCoordinator never receives sensor pointers
- `systemContext_->sensorCoordinator()` is created but ISensor references not injected
- Constructor of SensorCoordinator: `src/coordinators/SensorCoordinator.cpp:12-21`
- Currently called with `nullptr` for both sensors

**Problem 2:** SensorCoordinator.update() is never called
- SystemContext exists but no code calls `sensorCoordinator().update()`
- Sensor data is never cached in SensorCoordinator
- State machine has no way to access cached sensor values

**Problem 3:** SystemContext is never marked ready
- `markReady()` is defined but never called
- Components can't check `isReady()` for initialization guard

---

## 5. Current Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                     INITIALIZATION (setup)                       │
└─────────────────────────────────────────────────────────────────┘

SystemInitializer::initialize()
  ├─► Create SystemContext
  ├─► Create HardwareManager
  │    ├─► Create TempSensor (Dallas)
  │    ├─► Create WaterTankSwitch
  │    └─► Create ScaleManager
  │
  ├─► Create SensorManager
  │    └─► SensorManager::initialize(tempSensor, waterTankSensor, &sensorCoordinator)
  │        └─► Stored in sensorManager_ (but coordinator not used yet)
  │
  └─► Return sensorManager to main.cpp as global

main.cpp setup()
  └─► Store sensorManager as global pointer

┌─────────────────────────────────────────────────────────────────┐
│                  MAIN LOOP EXECUTION (loop)                      │
└─────────────────────────────────────────────────────────────────┘

Arduino loop()
  └─► LoopManager::update()
      │
      ├─► Timer-based updateTemperatureSensor() [every 400ms]
      │    └─► sensorManager->update()
      │         └─► tempSensor->updateTemperature()
      │         └─► temperature_ = cached value
      │
      ├─► Timer-based updatePressureSensor() [every 50ms]
      │    └─► sensorManager->updatePressureSensor()
      │         └─► measurePressure() from hardware
      │
      ├─► Timer-based checkWaterTankLevel() [every 200ms]
      │    └─► sensorManager->updateWaterTankSensor()
      │         └─► waterTankSensor->read()
      │
      ├─► updateProcessControl()
      │    └─► processController->updateProcessControl()
      │         └─► Uses g_state.process.temperature (from SensorManager)
      │
      ├─► updateStateMachine()
      │    └─► stateMachine->update()
      │         └─► Uses SystemContext (but NOT sensorCoordinator data)
      │
      └─► Other updates...

┌─────────────────────────────────────────────────────────────────┐
│            SENSOR COORDINATOR (CURRENTLY UNUSED)                 │
└─────────────────────────────────────────────────────────────────┘

SystemContext::sensorCoordinator()
  └─► Returns reference to SensorCoordinator
      ├─► Never receives ISensor implementations
      ├─► Never has update() called
      ├─► Never cached values accessed
      └─► Pure DEAD CODE currently
```

---

## 6. Integration Points Needed

### 6.1 Step 1: Provide ISensor Implementations to SensorCoordinator

**Current Problem:**
- HardwareManager creates TempSensor and Scale objects
- These do NOT implement ISensor interface
- SensorCoordinator cannot work with them

**Where to Implement:**
1. **Option A:** Make TempSensor implement ISensor
   - Add to: `include/clevercoffee/hardware/tempsensors/TempSensor.h`
   - Implement startRead() and tryGetValue()
   
2. **Option B:** Create adapter classes
   - Create: `include/clevercoffee/sensors/TempSensorAdapter.h`
   - Wraps TempSensor and implements ISensor
   - Similar for Scale

**Recommendation:** Option A is cleaner - implement ISensor in TempSensor itself

**Location to Inject:**
- `src/core/SystemInitializer.cpp` in `initializeSensors()`
- After getting sensors from HardwareManager, cast or wrap them as ISensor*
- Pass to SensorCoordinator constructor (or new initialization method)

### 6.2 Step 2: Call SensorCoordinator.update() from Main Loop

**Where:**
- `src/core/LoopManager.cpp:80` in `update()` method
- Should be called early (before sensor timers execute)
- Non-blocking, so minimal performance impact

**Current sensor timer locations:** Lines 389-403
- Temperature timer: `updateTemperatureSensor()` (line 338-350)
- Pressure timer: `updatePressureSensor()` (line 352-367)
- Water tank timer: `checkWaterTankLevel()` (line 331-336)
- Scale timer: `updateScaleSensor()` (line 369-387)

**Change Required:**
- Add new method in LoopManager: `updateSensorCoordinator()`
- Call it from main `update()` method around line 122 (before other sensor updates)
- This replaces the timer-based individual sensor calls gradually

### 6.3 Step 3: Inject SensorCoordinator Reference into LoopManager

**Current State:**
- LoopManager has `sensorManager_` pointer but no `sensorCoordinator_`
- LoopManager constructor: `src/core/LoopManager.cpp:46-55`
- Current parameters: ProcessController*, SensorManager*, UIManager*, HotWaterHandler*

**Change Required:**
1. Add new parameter or setter method to LoopManager
2. Option: Pass SystemContext to LoopManager
   - Most elegant - one dependency instead of many
   - Allows access to all coordinators
   
3. Option: Add SensorCoordinator* parameter
   - Minimal change
   - Explicit dependency

**Implementation Location:**
- `include/clevercoffee/core/LoopManager.h`: Add parameter/setter
- `src/core/LoopManager.cpp`: Store reference
- `src/main.cpp:160`: Pass coordinator when creating LoopManager

### 6.4 Step 4: Mark SystemContext as Ready

**Location:**
- `src/core/SystemInitializer.cpp` in `initialize()`
- Call `systemContext_->markReady()` at end (before returning true)
- Currently line 154 is good spot

**Current:**
```cpp
systemInitialized_ = true;
return true;
```

**Proposed:**
```cpp
systemContext_->markReady();  // Add this
systemInitialized_ = true;
return true;
```

### 6.5 Step 5: Migrate Sensor Value Access

**Current:** State machine and other components use `g_state.process.temperature`

**Target:** State machine uses `systemContext_.sensorCoordinator().getTemperature()`

**Locations Affected:**
- `src/state/StateMachine.cpp` - dozens of references to `g_state.process.temperature`
- `src/control/ProcessController.cpp` - temperature reading
- Any component that reads sensor values

**Migration Strategy:**
- Phase 5a: Keep both paths (gradual migration)
- Phase 5b: SensorManager reads from SensorCoordinator
- Phase 6: Remove SensorManager, use only SensorCoordinator

---

## 7. Proposed Integration Flow (Target State)

```
┌─────────────────────────────────────────────────────────────────┐
│                     INITIALIZATION (setup)                       │
└─────────────────────────────────────────────────────────────────┘

SystemInitializer::initialize()
  ├─► Create SystemContext
  │    └─► SensorCoordinator created (empty)
  │
  ├─► Create HardwareManager
  │    ├─► TempSensor (now implements ISensor)
  │    ├─► WaterTankSwitch
  │    └─► ScaleManager (now implements ISensor)
  │
  ├─► INJECT SENSORS: Populate SensorCoordinator
  │    └─► systemContext_->sensorCoordinator().setSensors(tempSensor, scaleSensor)
  │        [NEW METHOD NEEDED]
  │
  ├─► systemContext_->markReady()  [NEW CALL]
  │
  └─► Create SensorManager (legacy, for backward compat)
       └─► Still get pointer but now it's optional

main.cpp setup()
  └─► Store references from SystemInitializer

┌─────────────────────────────────────────────────────────────────┐
│                  MAIN LOOP EXECUTION (loop)                      │
└─────────────────────────────────────────────────────────────────┘

Arduino loop()
  └─► LoopManager::update()
      │
      ├─► NEW: updateSensorCoordinator()  [FIRST]
      │    └─► systemContext_.sensorCoordinator().update()
      │         ├─► updateTemperature() [checks interval]
      │         │    └─► tempSensor->startRead() / tryGetValue()
      │         │         └─► Caches result in cachedTemperature_
      │         │
      │         └─► updateScale() [checks interval]
      │              └─► scaleSensor->tryGetValue()
      │                   └─► Caches result in cachedWeight_
      │
      ├─► Timer-based updateTemperatureSensor() [STILL RUNS, calls SensorManager]
      │    └─► sensorManager->update() [NOW READS FROM COORDINATOR]
      │         └─► temperature_ = coordinator.getTemperature()
      │
      ├─► Timer-based updatePressureSensor() [UNCHANGED]
      │
      ├─► Timer-based checkWaterTankLevel() [UNCHANGED]
      │
      ├─► updateProcessControl()
      │    └─► Uses g_state.process.temperature [still works]
      │
      ├─► updateStateMachine()
      │    └─► stateMachine->update()
      │         └─► Can access systemContext_.sensorCoordinator().getTemperature()
      │
      └─► Other updates...

┌─────────────────────────────────────────────────────────────────┐
│           SENSOR COORDINATOR (NOW ACTIVE & CACHED)               │
└─────────────────────────────────────────────────────────────────┘

SystemContext::sensorCoordinator()
  └─► Returns reference to SensorCoordinator
      ├─► tempSensor_ = TempSensor implementing ISensor
      ├─► scaleSensor_ = ScaleManager implementing ISensor
      ├─► cachedTemperature_ [updated every 400ms]
      ├─► cachedWeight_ [updated every 100ms]
      └─► Error flags (atomic, for thread safety)
```

---

## 8. Backward Compatibility Strategy

### 8.1 Can SensorManager and SensorCoordinator Coexist?

**YES** - This is the recommended Phase 5 approach.

**Current:**
- SensorManager is the active sensor reader
- SensorCoordinator is dormant (never called)

**Phase 5 Target:**
- SensorCoordinator becomes active (update() called in loop)
- SensorManager can:
  - Option A: Forward to SensorCoordinator (read cached values)
  - Option B: Continue independently for pressure sensor
  - Option C: Become a wrapper/bridge

**Migration Path:**
1. **Phase 5a:** Both active, using different data paths
2. **Phase 5b:** SensorManager reads from SensorCoordinator for temperature
3. **Phase 6:** Remove SensorManager, use only SensorCoordinator

### 8.2 Global State Compatibility

**Current code expects:**
```cpp
double temp = sensorManager->getCurrentTemperature();
// or
double temp = g_state.process.temperature;
```

**Both must keep working during Phase 5:**
- SensorManager still provides `getCurrentTemperature()`
- `g_state.process.temperature` still updated
- SensorCoordinator provides cached values as backup

**After Phase 6 (optional):**
- Remove SensorManager entirely
- Use only `systemContext.sensorCoordinator().getTemperature()`

---

## 9. Order of Changes to Minimize Breakage

### Stage 1: Preparation (No Breaking Changes)

1. **Make TempSensor implement ISensor**
   - Add virtual methods startRead() / tryGetValue() to TempSensor
   - Implement adapter if necessary
   - Add scale implementation of ISensor

2. **Add setter method to SensorCoordinator**
   - New method: `setSensors(ISensor* temp, ISensor* scale)`
   - Allows post-construction initialization
   - File: `include/clevercoffee/coordinators/SensorCoordinator.h`

3. **Add SensorCoordinator reference to LoopManager**
   - New parameter or setter method
   - Store non-owning pointer
   - File: `include/clevercoffee/core/LoopManager.h`

### Stage 2: Injection (Low Risk)

4. **Inject sensors into SensorCoordinator**
   - In `SystemInitializer::initializeSensors()`
   - After getting sensors from HardwareManager
   - Call new `setSensors()` method
   - File: `src/core/SystemInitializer.cpp:402-435`

5. **Inject SensorCoordinator into LoopManager**
   - In `main.cpp:160` when creating LoopManager
   - Pass systemContext reference or coordinator pointer
   - Files: `src/main.cpp` + `src/core/LoopManager.cpp`

6. **Mark SystemContext as ready**
   - In `SystemInitializer::initialize()`
   - Before returning true
   - File: `src/core/SystemInitializer.cpp:154`

### Stage 3: Integration (Moderate Risk)

7. **Call SensorCoordinator.update() from LoopManager**
   - New method: `LoopManager::updateSensorCoordinator()`
   - Call at beginning of `update()` loop (line ~122)
   - Non-blocking, safe to call frequently
   - File: `src/core/LoopManager.cpp`

8. **Keep existing timer-based sensor reads**
   - Don't remove yet - let both run in parallel
   - Allows transition period
   - Files: `src/core/LoopManager.cpp:338-387`

### Stage 4: Gradual Migration (Optional, Phase 5b)

9. **Have SensorManager read from SensorCoordinator**
   - In `SensorManager::update()`
   - Read cached temperature from coordinator
   - Still updates `g_state.process.temperature`
   - File: `src/sensors/SensorManager.cpp:59-128`

10. **Update State Machine to use SystemContext**
    - Add SystemContext& parameter to StateMachine
    - Gradually replace `g_state.process.temperature` reads
    - Use `systemContext.sensorCoordinator().getTemperature()`
    - File: `src/state/StateMachine.cpp`

### Stage 5: Cleanup (Phase 6, Future)

11. **Remove SensorManager.update() timer calls**
12. **Remove global g_state sensor reads where possible**
13. **Remove legacy SensorManager if no longer needed**

---

## 10. File Changes Summary

### New/Modified Files Required for Phase 5

| File | Change | Lines | Complexity |
|------|--------|-------|------------|
| `include/clevercoffee/hardware/tempsensors/TempSensor.h` | Add ISensor methods | 5-10 | Low |
| `include/clevercoffee/hardware/scales/Scale.h` | Add ISensor methods | 5-10 | Low |
| `include/clevercoffee/coordinators/SensorCoordinator.h` | Add setSensors() method | 10-15 | Low |
| `src/coordinators/SensorCoordinator.cpp` | Implement setSensors() | 5-10 | Low |
| `src/core/SystemInitializer.cpp` | Call setSensors(), markReady() | 10-15 | Low |
| `include/clevercoffee/core/LoopManager.h` | Add sensorCoordinator_ param/setter | 5-10 | Low |
| `src/core/LoopManager.cpp` | Add updateSensorCoordinator(), call it | 20-30 | Medium |
| `src/main.cpp` | Pass coordinator to LoopManager | 3-5 | Low |

### No Changes Needed

- `src/sensors/SensorManager.cpp` - Works as-is for Phase 5
- `src/state/StateMachine.cpp` - Works as-is during Phase 5
- `src/control/ProcessController.cpp` - Works as-is during Phase 5
- `src/core/LoopManager.cpp` - Sensor timers keep running

---

## 11. Specific Line Numbers & Code Locations

### Key Function Entry Points

| Component | File | Function | Lines | Purpose |
|-----------|------|----------|-------|---------|
| Main Loop | `src/main.cpp` | `loop()` | 177-182 | Arduino entry point, calls loopManager.update() |
| Loop Coordinator | `src/core/LoopManager.cpp` | `update()` | 80-179 | Orchestrates all subsystem updates |
| Sensor Timers | `src/core/LoopManager.cpp` | `updateCentralizedSensorTimers()` | 389-403 | Invokes all sensor timers |
| Temp Update | `src/core/LoopManager.cpp` | `updateTemperatureSensor()` | 338-350 | Calls sensorManager.update() |
| Init System | `src/core/SystemInitializer.cpp` | `initialize()` | 51-155 | Main initialization orchestrator |
| Init Sensors | `src/core/SystemInitializer.cpp` | `initializeSensors()` | 402-435 | Creates and initializes SensorManager |

### Critical Variables

| Variable | File | Line | Type | Purpose |
|----------|------|------|------|---------|
| `systemInitializer` | `src/main.cpp` | 59 | unique_ptr<SystemInitializer> | Global initializer instance |
| `loopManager` | `src/main.cpp` | 82 | unique_ptr<LoopManager> | Global loop coordinator |
| `sensorManager` | `src/main.cpp` | 70 | SensorManager* | Global sensor manager pointer |
| `systemContext_` | SystemInitializer | member | unique_ptr<SystemContext> | Owns all coordinators |
| `sensorCoordinator_` | SystemContext | member | SensorCoordinator | Manages sensor polling |

---

## 12. Integration Error Handling

### Defensive Checks Needed

1. **Null pointer checks**
   ```cpp
   if (sensorCoordinator_ && tempSensor_) {
       sensorCoordinator_->update();
   }
   ```

2. **Sensor availability checks**
   ```cpp
   if (systemContext_->isReady() && coordinator.hasTemperatureSensor()) {
       temp = coordinator.getTemperature();
   }
   ```

3. **Error state handling**
   ```cpp
   if (coordinator.hasSensorError()) {
       LOGF(WARNING, "Sensor error detected: %d", error_code);
   }
   ```

4. **Fallback paths**
   - If SensorCoordinator unavailable, fall back to SensorManager
   - If ISensor conversion fails, use legacy TempSensor directly
   - Always keep `g_state.process.temperature` updated for compatibility

---

## 13. Testing Strategy

### Unit Tests Required

1. **SensorCoordinator.update()** when called from LoopManager
   - Verify non-blocking behavior
   - Verify caching of values
   - Verify error tracking

2. **ISensor interface implementation**
   - TempSensor implements startRead() / tryGetValue()
   - Scale implements ISensor

3. **SystemInitializer injection**
   - Verify sensors injected into coordinator
   - Verify SystemContext marked ready
   - Verify LoopManager receives coordinator

### Integration Tests

1. **Full loop execution**
   - Call LoopManager::update() 1000 times
   - Verify SensorCoordinator.update() called
   - Verify temperature caching works
   - Verify no memory leaks

2. **Temperature reading path**
   - Old: Timer calls SensorManager.update() → tempSensor.read()
   - New: Timer calls LoopManager.updateSensorCoordinator() → SensorCoordinator.updateTemperature()
   - Both should produce same cached value

3. **Backward compatibility**
   - Verify g_state.process.temperature still updated
   - Verify sensorManager->getCurrentTemperature() still works
   - Verify both paths return same value

---

## 14. Conclusion

Phase 5 integration is straightforward because:

1. **Infrastructure exists**: SystemContext, SensorCoordinator already built
2. **Low coupling**: SensorCoordinator has no dependencies on system state
3. **Coexistence strategy**: Can run both systems in parallel during transition
4. **Clear injection points**: 4 specific locations for dependency injection
5. **Non-breaking changes**: Can implement Stage 1-3 without breaking anything

**Recommended Approach:**
- Implement all Stage 1-3 changes (preparation + injection)
- Let both sensor systems run in parallel for Phase 5a
- Optionally migrate sensor value access in Phase 5b
- Save cleanup/removal for Phase 6

**Risk Level: LOW** - All changes are additive, no removals of working code needed.

