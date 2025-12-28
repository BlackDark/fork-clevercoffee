# Global State Elimination Plan

**Date**: 2025-12-28  
**Status**: Planning - Ready for Implementation  
**Priority**: High (803 references remaining)

## Executive Summary

Despite significant progress in architectural refactoring, the codebase still contains **803 references to `g_state`** across the system. This global mutable state creates tight coupling, makes testing difficult, and violates modern C++ design principles.

This plan outlines a phased approach to eliminate `g_state` and replace it with proper dependency injection through SystemContext and coordinator patterns.

---

## Current State Analysis

### g_state Usage Breakdown

| Category | References | Primary Usage |
|----------|-----------|---------------|
| `g_state.hardware.*` | 286 | Display, LEDs, Relays, Switches, Scale |
| `g_state.process.*` | 62 | PID values, temperature, timers |
| `g_state.sensors.*` | 62 | Weight, pressure, temperature readings |
| `g_state.machine.*` | 85 | State machine flags, current state |
| `g_state.network.*` | 59 | WiFi status, MQTT, hostname |
| `g_state.handlers.*` | 22 | BrewHandler, SteamHandler, etc. |
| `g_state.coordination.*` | 19 | Coordinator references |
| `g_state.standby.*` | 16 | Standby mode state |
| `g_state.timing.*` | 13 | Timing variables |
| `g_state.display.*` | 4 | Display state |

**Total: 803 references**

### Top Offending Files

| File | References | Category |
|------|-----------|----------|
| `display/displayCommon.h` | 272 | Hardware, Process, Sensors |
| `core/LoopManager.cpp` | 79 | All categories |
| `display/ModernDisplayTemplate.h` | 76 | Hardware, Process, Machine |
| `core/SystemInitializer.cpp` | 67 | All categories |
| `control/ProcessController.cpp` | 45 | Process, Machine |

---

## Missing Functionality from scaleHandler Removal

### 1. Scale Calibration Workflow
**Status**: ❌ Removed  
**Impact**: Users cannot calibrate scale from UI  

**Original Functionality:**
- Multi-step non-blocking calibration process
- Display feedback during calibration
- Known weight input
- Calibration factor calculation
- Persistent storage of calibration values

**Restoration Plan:**
- Add `SensorCoordinator::startCalibration(float knownWeight)` method
- Add `SensorCoordinator::isCalibrating()` state check
- Add `SensorCoordinator::getCalibrationProgress()` for UI feedback
- Store calibration factors in Config system
- Implement state machine for calibration workflow

### 2. Scale Tare Workflow
**Status**: ⚠️ Basic functionality exists, UI workflow removed  
**Impact**: No visual feedback during tare operation  

**Original Functionality:**
- 2-second warning display
- "Remove any load" message
- Visual confirmation when complete
- Non-blocking execution

**Restoration Plan:**
- Add `UIManager::showTareWarning()` method
- Add `UIManager::showTareComplete()` method
- Implement tare workflow in UIManager
- Trigger from Config change or button press

### 3. Bluetooth Scale Connection Management
**Status**: ⚠️ Basic functionality exists in BluetoothScale, but monitoring removed  
**Impact**: No brew-by-weight fallback during connection loss  

**Original Functionality:**
- Periodic connection status checks (5 second interval)
- Automatic brew-by-time fallback during active brew
- Connection timeout detection (30 seconds)
- Reconnection attempts with exponential backoff
- Connection restored notifications

**Restoration Plan:**
- Move connection monitoring to SensorCoordinator
- Add `SensorCoordinator::isScaleConnectionHealthy()` method
- Implement fallback logic in BrewRunningState
- Add connection status to SystemContext for state machine access
- Log connection state changes

### 4. Scale Error Recovery
**Status**: ❌ Removed  
**Impact**: No automatic recovery from transient scale errors  

**Original Functionality:**
- Error count tracking (max 3 consecutive failures)
- Error cooldown period (5 seconds)
- Return last valid weight during errors
- Automatic recovery after cooldown

**Restoration Plan:**
- Add error recovery to SensorCoordinator
- Track consecutive error count per sensor
- Implement cooldown mechanism
- Cache last valid readings
- Expose error recovery state to state machine

### 5. Auto-Tare for Bluetooth Scales
**Status**: ❌ Removed  
**Impact**: Bluetooth scales don't auto-tare at brew start  

**Original Functionality:**
- Automatic tare when brew starts
- 2-second wait for tare completion
- Pre-brew weight capture after tare

**Restoration Plan:**
- Add auto-tare configuration option
- Trigger from BrewIdleState when transitioning to brew
- Implement in BluetoothScale.autoTare() method
- Wait for completion before capturing pre-brew weight

---

## Phase 9: Eliminate g_state.hardware (286 references)

**Priority**: High  
**Estimated Effort**: 3-4 days  

### Goal
Replace direct hardware access through g_state with HardwareManager and SystemContext.

### Current Problem
```cpp
// Current (bad)
if (g_state.hardware.statusLed) {
    g_state.hardware.statusLed->turnOn();
}

// Display accessing hardware directly
U8G2* display = g_state.hardware.display;
display->drawStr(0, 10, "Hello");
```

### Target Architecture
```cpp
// Target (good)
hardwareManager.getStatusLED().turnOn();

// Or through coordinator
uiCoordinator.updateDisplay([](DisplayContext& ctx) {
    ctx.drawStr(0, 10, "Hello");
});
```

### Implementation Tasks

#### Task 9.1: Extend HardwareManager with Accessors
**Files**: `include/clevercoffee/hardware/HardwareManager.h`

Add methods:
```cpp
class HardwareManager {
public:
    // LED access
    LED& getStatusLED() noexcept;
    LED& getBrewLED() noexcept;
    LED& getSteamLED() noexcept;
    
    // Display access
    U8G2& getDisplay() noexcept;
    
    // Switch access
    Switch& getBrewSwitch() noexcept;
    Switch& getSteamSwitch() noexcept;
    Switch& getHotWaterSwitch() noexcept;
    Switch& getPowerSwitch() noexcept;
    
    // Relay access
    Relay& getHeaterRelay() noexcept;
    Relay& getPumpRelay() noexcept;
    Relay& getValveRelay() noexcept;
    Relay& getSolenoidRelay() noexcept;
    
    // Scale access
    Scale* getScale() noexcept;  // Can be null
};
```

#### Task 9.2: Update LoopManager to Use HardwareManager
**Files**: `src/core/LoopManager.cpp`

Replace:
```cpp
// Old
g_state.hardware.statusLed->turnOn();

// New
hardwareManager_->getStatusLED().turnOn();
```

**Estimated**: 79 references in LoopManager.cpp

#### Task 9.3: Update Display Code to Use DisplayManager
**Files**: 
- `include/clevercoffee/display/displayCommon.h` (234 references)
- `include/clevercoffee/display/ModernDisplayTemplate.h` (52 references)

**Strategy**: 
- All display operations should go through DisplayManager
- DisplayManager owns the U8G2 instance
- Remove direct hardware.display access

#### Task 9.4: Update Handlers to Use HardwareManager
**Files**: 
- `include/clevercoffee/handlers/PowerHandler.h` (3 references)
- `include/clevercoffee/handlers/BrewHandler.h` (2 references)
- `include/clevercoffee/handlers/SteamHandler.h` (1 reference)
- `include/clevercoffee/handlers/HotWaterHandler.h` (1 reference)

**Pattern**:
```cpp
class BrewHandler {
    HardwareManager& hardwareManager_;
public:
    BrewHandler(HardwareManager& hw) : hardwareManager_(hw) {}
    
    void process() {
        if (hardwareManager_.getBrewSwitch().isPressed()) {
            // Handle brew
        }
    }
};
```

---

## Phase 10: Eliminate g_state.sensors (62 references)

**Priority**: High  
**Estimated Effort**: 2-3 days  

### Goal
All sensor data should be accessed through SensorCoordinator via SystemContext.

### Current Problem
```cpp
// Current (bad)
float weight = g_state.sensors.currReadingWeight;
float brewWeight = g_state.sensors.currBrewWeight;
float temp = g_state.sensors.temperature;
```

### Target Architecture
```cpp
// Target (good)
float weight = sensorCoordinator.getWeight();
float brewWeight = context.getCurrentBrewWeight();
float temp = sensorCoordinator.getTemperature();
```

### Implementation Tasks

#### Task 10.1: Move Brew Weight Tracking to SensorCoordinator
**Current**: LoopManager manages brew weight calculation  
**Target**: SensorCoordinator owns brew weight state machine

Add to SensorCoordinator:
```cpp
class SensorCoordinator {
    enum class BrewWeightState { IDLE, BREWING };
    BrewWeightState brewWeightState_ = BrewWeightState::IDLE;
    double preBrewWeight_ = 0.0;
    double currBrewWeight_ = 0.0;
    
public:
    void startBrewWeightTracking() noexcept;
    void stopBrewWeightTracking() noexcept;
    double getBrewWeight() const noexcept { return currBrewWeight_; }
    double getPreBrewWeight() const noexcept { return preBrewWeight_; }
};
```

#### Task 10.2: Update WebServerManager and MQTTManager
**Files**: 
- `src/network/WebServerManager.cpp` (16 references)
- `src/network/MQTTManager.cpp` (4 references)

Replace sensor reads with SensorCoordinator access:
```cpp
// Old
doc["weight"] = g_state.sensors.currReadingWeight;
doc["brewWeight"] = g_state.sensors.currBrewWeight;

// New
doc["weight"] = sensorCoordinator_->getWeight();
doc["brewWeight"] = sensorCoordinator_->getBrewWeight();
```

#### Task 10.3: Remove Sensor State Variables from GlobalState
**File**: `include/clevercoffee/GlobalState.h`

Delete entire `sensors` struct:
```cpp
struct {
    float currReadingWeight;
    float currBrewWeight;
    float preBrewWeight;
    float temperature;
    float inputPressure;
    float inputPressureFilter;
    // ... etc
} sensors;
```

Move these to appropriate owners (SensorCoordinator, ProcessController).

---

## Phase 11: Eliminate g_state.process (62 references)

**Priority**: Medium  
**Estimated Effort**: 2-3 days  

### Goal
Process state should be owned by ProcessController and accessed through SystemContext.

### Current Problem
```cpp
// Current (bad)
g_state.process.temperature = 95.0;
g_state.process.setPoint = 93.0;
g_state.process.pidOutput = 0.75;
```

### Target Architecture
```cpp
// Target (good)
processController.getTemperature();
processController.getSetPoint();
processController.getPIDOutput();
```

### Implementation Tasks

#### Task 11.1: Move Process State to ProcessController
Add to ProcessController:
```cpp
class ProcessController {
    double currentTemperature_ = 0.0;
    double setPoint_ = 0.0;
    double pidOutput_ = 0.0;
    double brewSetPoint_ = 0.0;
    double steamSetPoint_ = 0.0;
    
    // PID coefficients
    double aggKp_, aggKi_, aggKd_;
    double aggbKp_, aggbKi_, aggbKd_;
    
public:
    double getTemperature() const noexcept { return currentTemperature_; }
    double getSetPoint() const noexcept { return setPoint_; }
    double getPIDOutput() const noexcept { return pidOutput_; }
    // ... etc
};
```

#### Task 11.2: Update Display and Network Access
**Files**: 
- `display/displayCommon.h` (18 references)
- `display/ModernDisplayTemplate.h` (17 references)
- `control/ProcessController.cpp` (22 references)

Pass ProcessController to display functions instead of reading g_state.

---

## Phase 12: Eliminate g_state.machine (85 references)

**Priority**: High  
**Estimated Effort**: 3-4 days  

### Goal
Machine state should be fully owned by StateMachine, accessed through MachineStateContext.

### Current Problem
```cpp
// Current (bad)
if (g_state.machine.machineState == MachineStateId::BREW_RUNNING) {
    // ...
}
g_state.machine.flags.requestBrewStart = true;
```

### Target Architecture
```cpp
// Target (good)
if (context.getCurrentState() == MachineStateId::BREW_RUNNING) {
    // ...
}
context.setFlag(MachineStateFlag::REQUEST_BREW_START);
```

### Implementation Tasks

#### Task 12.1: Add State Query Methods to MachineStateContext
```cpp
class MachineStateContext {
public:
    MachineStateId getCurrentState() const noexcept;
    bool isBrewState() const noexcept;
    bool isSteamState() const noexcept;
    
    // Flag management
    void setFlag(MachineStateFlag flag) noexcept;
    void clearFlag(MachineStateFlag flag) noexcept;
    bool isFlagSet(MachineStateFlag flag) const noexcept;
};
```

#### Task 12.2: Update All State Checks
Replace direct g_state.machine access with context methods throughout:
- Handlers (26 references)
- Display (21 references)
- States (12 references)
- LoopManager (10 references)

---

## Phase 13: Eliminate g_state.network (59 references)

**Priority**: Low  
**Estimated Effort**: 1-2 days  

### Goal
Network state should be owned by NetworkCoordinator.

### Implementation

Add to NetworkCoordinator:
```cpp
class NetworkCoordinator {
    bool wifiConnected_ = false;
    bool mqttConnected_ = false;
    String hostname_;
    String ipAddress_;
    
public:
    bool isWiFiConnected() const noexcept { return wifiConnected_; }
    bool isMQTTConnected() const noexcept { return mqttConnected_; }
    const String& getHostname() const noexcept { return hostname_; }
    const String& getIPAddress() const noexcept { return ipAddress_; }
};
```

Update files:
- `core/LoopManager.cpp` (26 references)
- `network/MQTTManager.cpp` (15 references)
- `network/CleverCoffeeWiFiManager.cpp` (10 references)

---

## Phase 14: Eliminate Remaining Categories

### g_state.handlers (22 references)
**Solution**: Store handlers in SystemContext, access through getters

### g_state.coordination (19 references)
**Solution**: Already have coordinators in SystemContext, replace direct access

### g_state.standby (16 references)
**Solution**: Create StandbyManager, add to SystemContext

### g_state.timing (13 references)
**Solution**: Move timing to LoopManager or individual components

### g_state.display (4 references)
**Solution**: Already have DisplayManager, replace remaining references

---

## Phase 15: Delete GlobalState

**Final Step**: Once all references eliminated:

1. Remove `#include "clevercoffee/GlobalState.h"` from all files
2. Delete `include/clevercoffee/GlobalState.h`
3. Delete `src/state/GlobalState.cpp`
4. Remove `extern GlobalState g_state;` declarations
5. Verify build succeeds with 0 g_state references

---

## Migration Strategy

### Recommended Order
1. **Phase 9**: Hardware (high impact, foundational)
2. **Phase 10**: Sensors (builds on Phase 9)
3. **Phase 12**: Machine state (critical for state machine refactor)
4. **Phase 11**: Process (depends on ProcessController refactor)
5. **Phase 13**: Network (low coupling, easy wins)
6. **Phase 14**: Remaining categories (cleanup)
7. **Phase 15**: Delete GlobalState (celebration!)

### Per-Phase Process
1. Create feature branch: `git checkout -b remove-g-state-{category}`
2. Identify owner class for state (e.g., HardwareManager, SensorCoordinator)
3. Add accessor methods to owner class
4. Update all references file-by-file
5. Remove state from GlobalState struct
6. Verify build and tests
7. Commit and merge

### Risk Mitigation
- **Break changes into small PRs** (one phase at a time)
- **Keep tests passing** after each file update
- **Add integration tests** for critical paths
- **Document breaking changes** in commit messages
- **Maintain backwards compatibility** where feasible during transition

---

## Success Criteria

- ✅ Zero `g_state` references in codebase
- ✅ GlobalState.h deleted
- ✅ All state owned by appropriate managers/coordinators
- ✅ Dependency injection throughout
- ✅ Full test coverage maintained
- ✅ Build succeeds with no errors
- ✅ Firmware runs successfully on hardware

---

## Estimated Total Effort

| Phase | Effort | Priority |
|-------|--------|----------|
| Phase 9: Hardware | 3-4 days | High |
| Phase 10: Sensors | 2-3 days | High |
| Phase 11: Process | 2-3 days | Medium |
| Phase 12: Machine | 3-4 days | High |
| Phase 13: Network | 1-2 days | Low |
| Phase 14: Remaining | 2-3 days | Low |
| Phase 15: Delete | 1 day | Final |

**Total**: 14-22 days (3-4 weeks)

---

## Benefits

Once complete:
- ✅ **Zero global mutable state**
- ✅ **Fully testable components** (dependency injection)
- ✅ **Thread-safe by design** (no shared mutable state)
- ✅ **Clear ownership** (each manager owns its state)
- ✅ **Better encapsulation** (state hidden behind interfaces)
- ✅ **Modern C++ best practices** (RAII, const correctness)
- ✅ **Easier to reason about** (no action at a distance)

---

**Document Status:** Ready for Implementation  
**Last Updated:** 2025-12-28  
**Version:** 1.0
