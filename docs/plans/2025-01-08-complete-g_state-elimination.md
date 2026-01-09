# Complete g_state Elimination Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Completely eliminate the global `g_state` variable and replace all state access with dependency-injected services and context objects, improving testability, encapsulation, and code clarity.

**Architecture:** Replace the monolithic global state structure with specialized, focused services (PIDService, ScaleService, NetworkService, DisplayService) injected through a central SystemContext. Each service owns its domain state and provides a clean API for access and modification.

**Tech Stack:** 
- C++17 with modern RAII patterns
- PlatformIO ESP32 build system
- Google Test for unit tests
- Dependency injection pattern
- State machine architecture

---

## Phase 0: Analysis and Documentation (COMPLETED)

**Status:** ✅ Complete - See `G_STATE_COMPLETE_ANALYSIS.md` for full breakdown

**Key Findings:**
- 75 total g_state members (11 nested structs)
- 431+ references across codebase
- ~50 members (67%) unused - dead code to remove
- ~25 members (33%) active - require migration
- Highest priority: `g_state.pid` (49 refs), `g_state.sensors` (40+ refs)

**Reference Distribution:**
- `SystemContext.cpp`: 54 refs (central adapter)
- `ProcessController.cpp`: 22 refs (PID control)
- `WebServerManager.cpp`: 10 refs (scale/web API)
- `embeddedWebserver.h`: 12 refs (API endpoints)
- `ModernDisplayTemplate.h`: 8 refs (UI rendering)

---

## Phase 1: Cleanup - Remove Dead Code (30 minutes, ZERO RISK)

### Task 1.1: Remove Unused Struct Members

**Files:**
- Modify: `include/clevercoffee/GlobalState.h:1-500`
- Test: Verify compilation only
- Reference: `G_STATE_COMPLETE_ANALYSIS.md` sections for Hardware (18 refs), Standby (5 refs), Legacy Timing (8 refs)

**Step 1: List members to remove**

Based on analysis, remove these entirely unused members:
```
HardwareRefs (all 18 members) - 0 references
StandbyState (all 5 members) - 0 references
TimingState (7 of 9 members) - leaving isrCounter
SensorState (15+ members) - scale calibration, error recovery, pressure filter
NetworkState (9 of 14 members) - MQTT, WiFi timers, legacy connection
```

**Step 2: Create backup branch**

```bash
git checkout -b cleanup/remove-dead-g_state-members
```

**Step 3: Edit GlobalState.h**

Remove struct definitions:
- Delete entire `HardwareRefs` struct (lines ~175-210)
- Delete entire `StandbyState` struct (lines ~220-235)
- Keep `TimingState` but remove: previousMillistemp, previousMillisMQTT, intervalPressure, previousMillisPressure, loopWaterTank, hassioDiscoveryTimer, printDisplayTimer, windowStartTime
- In `SensorState`: remove autoTareInProgress, autoTareStartTime, lastScaleConnectionCheck, scaleConnectionFailureTime, scaleConnectionLost, lastValidWeight, brewByWeightFallbackActive, scaleReadErrorCount, scaleMaxRetries, lastScaleErrorTime, scaleErrorCooldownMs, scaleInErrorRecovery, inX, inY, inOld, inSum, and all switch state members
- In `NetworkState`: remove lastWifiConnectionAttempt, lastTempEvent, tempEventInterval, mqttManager, mqttVars, mqttSensors, mqtt_was_connected, MQTTReCnctCount, lastMQTTConnectionAttempt

**Step 4: Remove global struct instances**

In `GlobalState` struct, remove initialization of deleted members.

**Step 5: Verify compilation**

```bash
~/.platformio/penv/bin/pio run -e esp32_usb -s
```

Expected: Clean build, no linker errors

**Step 6: Commit**

```bash
git add include/clevercoffee/GlobalState.h
git commit -m "cleanup: remove 43 unused g_state members (hardware, standby, legacy timing, mqtt)"
```

---

## Phase 2: Extract PID Service (2 hours, LOW RISK)

### Task 2.1: Create PIDService Class

**Files:**
- Create: `include/clevercoffee/services/PIDService.h`
- Create: `src/services/PIDService.cpp`
- Test: `test/test_pid_service/test_main.cpp`

**Purpose:** Encapsulate all PID-related state (temperature, setpoint, pidOutput, pidEnabled, aggKi, aggKd, previousInput, brewPidDisabled) and logic.

**Step 1: Write service header with failing test expectations**

```cpp
// include/clevercoffee/services/PIDService.h
#pragma once

namespace CleverCoffee {

class PIDService {
public:
    // Constructor
    explicit PIDService() = default;

    // Temperature management
    double getTemperature() const noexcept { return temperature_; }
    void setTemperature(double temp) noexcept { temperature_ = temp; }

    // Setpoint management
    double getSetpoint() const noexcept { return setpoint_; }
    void setSetpoint(double sp) noexcept { setpoint_ = sp; }

    // PID Output
    double getPidOutput() const noexcept { return pidOutput_; }
    void setPidOutput(double output) noexcept { pidOutput_ = output; }

    // PID Enabled
    bool isPidEnabled() const noexcept { return pidEnabled_; }
    void setPidEnabled(bool enabled) noexcept { pidEnabled_ = enabled; }

    // Brew PID Disabled
    bool isBrewPidDisabled() const noexcept { return brewPidDisabled_; }
    void setBrewPidDisabled(bool disabled) noexcept { brewPidDisabled_ = disabled; }

    // Previous Input (for derivative calculation)
    double getPreviousInput() const noexcept { return previousInput_; }
    void setPreviousInput(double input) noexcept { previousInput_ = input; }

    // Aggregator Gains
    double getAggKi() const noexcept { return aggKi_; }
    void setAggKi(double ki) noexcept { aggKi_ = ki; }

    double getAggKd() const noexcept { return aggKd_; }
    void setAggKd(double kd) noexcept { aggKd_ = kd; }

    // Pointer accessors for PID controller
    double* getTemperaturePtr() noexcept { return &temperature_; }
    double* getPidOutputPtr() noexcept { return &pidOutput_; }
    double* getSetpointPtr() noexcept { return &setpoint_; }

    // Reset to defaults
    void reset() noexcept;

private:
    // Core PID state
    double temperature_ = 0.0;
    double setpoint_ = 95.0;
    double pidOutput_ = 0.0;
    bool pidEnabled_ = true;

    // Brew control
    bool brewPidDisabled_ = false;

    // PID algorithm state
    double previousInput_ = 0.0;
    double aggKi_ = 0.0;
    double aggKd_ = 0.0;
};

} // namespace CleverCoffee
```

**Step 2: Create minimal test file**

```cpp
// test/test_pid_service/test_main.cpp
#include <gtest/gtest.h>
#include "clevercoffee/services/PIDService.h"

using namespace CleverCoffee;

class PIDServiceTest : public ::testing::Test {
protected:
    PIDService service;
};

TEST_F(PIDServiceTest, InitializesWithDefaults) {
    EXPECT_EQ(service.getTemperature(), 0.0);
    EXPECT_EQ(service.getSetpoint(), 95.0);
    EXPECT_EQ(service.getPidOutput(), 0.0);
    EXPECT_TRUE(service.isPidEnabled());
    EXPECT_FALSE(service.isBrewPidDisabled());
}

TEST_F(PIDServiceTest, CanSetAndGetTemperature) {
    service.setTemperature(75.5);
    EXPECT_EQ(service.getTemperature(), 75.5);
}

TEST_F(PIDServiceTest, CanSetAndGetSetpoint) {
    service.setSetpoint(90.0);
    EXPECT_EQ(service.getSetpoint(), 90.0);
}

TEST_F(PIDServiceTest, CanSetAndGetPidOutput) {
    service.setPidOutput(50.0);
    EXPECT_EQ(service.getPidOutput(), 50.0);
}

TEST_F(PIDServiceTest, CanSetAndGetAggregatorGains) {
    service.setAggKi(1.5);
    service.setAggKd(2.5);
    EXPECT_EQ(service.getAggKi(), 1.5);
    EXPECT_EQ(service.getAggKd(), 2.5);
}

TEST_F(PIDServiceTest, PointerAccessorsReturnValidPointers) {
    double* tempPtr = service.getTemperaturePtr();
    EXPECT_NE(tempPtr, nullptr);
    EXPECT_EQ(*tempPtr, 0.0);
    
    // Modify via pointer
    *tempPtr = 85.0;
    EXPECT_EQ(service.getTemperature(), 85.0);
}

TEST_F(PIDServiceTest, ResetRestoresDefaults) {
    service.setTemperature(100.0);
    service.setSetpoint(85.0);
    service.setPidEnabled(false);
    
    service.reset();
    
    EXPECT_EQ(service.getTemperature(), 0.0);
    EXPECT_EQ(service.getSetpoint(), 95.0);
    EXPECT_TRUE(service.isPidEnabled());
}
```

**Step 3: Implement PIDService.cpp**

```cpp
// src/services/PIDService.cpp
#include "clevercoffee/services/PIDService.h"

namespace CleverCoffee {

void PIDService::reset() noexcept {
    temperature_ = 0.0;
    setpoint_ = 95.0;
    pidOutput_ = 0.0;
    pidEnabled_ = true;
    brewPidDisabled_ = false;
    previousInput_ = 0.0;
    aggKi_ = 0.0;
    aggKd_ = 0.0;
}

} // namespace CleverCoffee
```

**Step 4: Run tests**

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
~/.platformio/penv/bin/pio test -e esp32_usb 2>&1 | grep -A 20 "test_pid_service"
```

Expected: All 7 tests PASS

**Step 5: Commit**

```bash
git add include/clevercoffee/services/PIDService.h src/services/PIDService.cpp test/test_pid_service/test_main.cpp
git commit -m "feat: create PIDService to encapsulate PID state (temperature, setpoint, pidOutput)"
```

---

### Task 2.2: Inject PIDService into SystemContext

**Files:**
- Modify: `include/clevercoffee/context/SystemContext.h:1-150`
- Modify: `src/context/SystemContext.cpp:1-200`

**Step 1: Add PIDService member to SystemContext**

In `SystemContext.h`, add:
```cpp
private:
    std::unique_ptr<PIDService> pidService_;

public:
    PIDService& pidService() noexcept { return *pidService_; }
    const PIDService& pidService() const noexcept { return *pidService_; }
```

**Step 2: Initialize in constructor**

In `SystemContext.cpp` constructor:
```cpp
SystemContext::SystemContext()
    : pidService_(std::make_unique<PIDService>())
{
    // ... other initialization
}
```

**Step 3: Update existing process accessors to delegate to PIDService**

Replace implementations:
```cpp
double SystemContext::processTemperature() const noexcept {
    return pidService_->getTemperature();
}

void SystemContext::setProcessTemperature(double temp) noexcept {
    pidService_->setTemperature(temp);
}

// ... and all other process accessors
```

**Step 4: Keep pointer accessors for PID controller compatibility**

```cpp
double* SystemContext::processTemperaturePtr() noexcept {
    return pidService_->getTemperaturePtr();
}
// ... etc
```

**Step 5: Verify compilation and existing tests**

```bash
~/.platformio/penv/bin/pio run -e esp32_usb -s
~/.platformio/penv/bin/pio test -e esp32_usb
```

Expected: All tests pass, firmware builds

**Step 6: Commit**

```bash
git add include/clevercoffee/context/SystemContext.h src/context/SystemContext.cpp
git commit -m "refactor: inject PIDService into SystemContext, maintain backward compatibility"
```

---

### Task 2.3: Update ProcessController to use SystemContext.pidService()

**Files:**
- Modify: `src/control/ProcessController.cpp:1-400`
- Test: `test/test_pid_mode_water_dispensing/test_main.cpp`, `test/test_pid_state_transitions/test_main.cpp`

**Step 1: Replace direct g_state.pid access with SystemContext**

Search for all `g_state.pid` references in ProcessController.cpp and replace:

```cpp
// Before
g_state.pid.mySetpoint = newSetpoint;
double temp = g_state.pid.input;

// After
auto& ctx = CleverCoffee::getGlobalSystemContext();
ctx.pidService().setSetpoint(newSetpoint);
double temp = ctx.pidService().getTemperature();
```

**Step 2: Update PID controller initialization**

In ProcessController initialization, use pointer accessors:
```cpp
// Initialize PID controller with pointers from service
auto& ctx = CleverCoffee::getGlobalSystemContext();
myPID = std::make_unique<PID>(
    ctx.pidService().getTemperaturePtr(),
    ctx.pidService().getPidOutputPtr(),
    ctx.pidService().getSetpointPtr(),
    // ... rest of PID constructor
);
```

**Step 3: Verify all 22 references replaced**

```bash
rg "g_state\.pid\." src/control/ProcessController.cpp
```

Expected: 0 results

**Step 4: Run ProcessController tests**

```bash
~/.platformio/penv/bin/pio test -e esp32_usb -f test_pid_mode_water_dispensing
~/.platformio/penv/bin/pio test -e esp32_usb -f test_pid_state_transitions
```

Expected: All tests pass

**Step 5: Commit**

```bash
git add src/control/ProcessController.cpp
git commit -m "refactor: ProcessController uses SystemContext.pidService() instead of g_state.pid"
```

---

## Phase 3: Extract Scale Service (2.5 hours, LOW-MEDIUM RISK)

### Task 3.1: Create ScaleService Class

**Files:**
- Create: `include/clevercoffee/services/ScaleService.h`
- Create: `src/services/ScaleService.cpp`
- Test: `test/test_scale_service/test_main.cpp`

**Purpose:** Encapsulate scale state (currBrewWeight, currReadingWeight, scaleTareOn, scaleCalibrationOn, scaleFailure, inputPressure, preBrewWeight).

**Step 1: Write service header**

```cpp
// include/clevercoffee/services/ScaleService.h
#pragma once

namespace CleverCoffee {

class ScaleService {
public:
    explicit ScaleService() = default;

    // Weight management
    double getCurrentBrewWeight() const noexcept { return currBrewWeight_; }
    void setCurrentBrewWeight(double weight) noexcept { currBrewWeight_ = weight; }

    double getCurrentReadingWeight() const noexcept { return currReadingWeight_; }
    void setCurrentReadingWeight(double weight) noexcept { currReadingWeight_ = weight; }

    double getPreBrewWeight() const noexcept { return preBrewWeight_; }
    void setPreBrewWeight(double weight) noexcept { preBrewWeight_ = weight; }

    // Tare control
    bool isTareOn() const noexcept { return scaleTareOn_; }
    void setTareOn(bool on) noexcept { scaleTareOn_ = on; }

    // Calibration control
    bool isCalibrationOn() const noexcept { return scaleCalibrationOn_; }
    void setCalibrationOn(bool on) noexcept { scaleCalibrationOn_ = on; }

    // Scale health
    bool hasScaleFailure() const noexcept { return scaleFailure_; }
    void setScaleFailure(bool failed) noexcept { scaleFailure_ = failed; }

    // Pressure input
    float getInputPressure() const noexcept { return inputPressure_; }
    void setInputPressure(float pressure) noexcept { inputPressure_ = pressure; }

    float getInputPressureFilter() const noexcept { return inputPressureFilter_; }
    void setInputPressureFilter(float filtered) noexcept { inputPressureFilter_ = filtered; }

    void reset() noexcept;

private:
    double currBrewWeight_ = 0.0;
    double currReadingWeight_ = 0.0;
    double preBrewWeight_ = 0.0;
    bool scaleTareOn_ = false;
    bool scaleCalibrationOn_ = false;
    bool scaleFailure_ = false;
    float inputPressure_ = 0.0;
    float inputPressureFilter_ = 0.0;
};

} // namespace CleverCoffee
```

**Step 2: Create comprehensive test file**

```cpp
// test/test_scale_service/test_main.cpp
#include <gtest/gtest.h>
#include "clevercoffee/services/ScaleService.h"

using namespace CleverCoffee;

class ScaleServiceTest : public ::testing::Test {
protected:
    ScaleService service;
};

TEST_F(ScaleServiceTest, InitializesWithDefaults) {
    EXPECT_EQ(service.getCurrentBrewWeight(), 0.0);
    EXPECT_EQ(service.getCurrentReadingWeight(), 0.0);
    EXPECT_FALSE(service.isTareOn());
    EXPECT_FALSE(service.isCalibrationOn());
    EXPECT_FALSE(service.hasScaleFailure());
    EXPECT_EQ(service.getInputPressure(), 0.0f);
}

TEST_F(ScaleServiceTest, CanManageBrewWeight) {
    service.setCurrentBrewWeight(18.5);
    EXPECT_EQ(service.getCurrentBrewWeight(), 18.5);
}

TEST_F(ScaleServiceTest, CanManageTareControl) {
    service.setTareOn(true);
    EXPECT_TRUE(service.isTareOn());
    service.setTareOn(false);
    EXPECT_FALSE(service.isTareOn());
}

TEST_F(ScaleServiceTest, CanManageCalibration) {
    service.setCalibrationOn(true);
    EXPECT_TRUE(service.isCalibrationOn());
}

TEST_F(ScaleServiceTest, CanReportScaleFailure) {
    EXPECT_FALSE(service.hasScaleFailure());
    service.setScaleFailure(true);
    EXPECT_TRUE(service.hasScaleFailure());
}

TEST_F(ScaleServiceTest, ResetRestoresDefaults) {
    service.setCurrentBrewWeight(25.0);
    service.setTareOn(true);
    service.setCalibrationOn(true);
    
    service.reset();
    
    EXPECT_EQ(service.getCurrentBrewWeight(), 0.0);
    EXPECT_FALSE(service.isTareOn());
    EXPECT_FALSE(service.isCalibrationOn());
}
```

**Step 3: Implement ScaleService.cpp**

```cpp
// src/services/ScaleService.cpp
#include "clevercoffee/services/ScaleService.h"

namespace CleverCoffee {

void ScaleService::reset() noexcept {
    currBrewWeight_ = 0.0;
    currReadingWeight_ = 0.0;
    preBrewWeight_ = 0.0;
    scaleTareOn_ = false;
    scaleCalibrationOn_ = false;
    scaleFailure_ = false;
    inputPressure_ = 0.0f;
    inputPressureFilter_ = 0.0f;
}

} // namespace CleverCoffee
```

**Step 4: Run tests**

```bash
~/.platformio/penv/bin/pio test -e esp32_usb -f test_scale_service
```

Expected: All tests PASS

**Step 5: Commit**

```bash
git add include/clevercoffee/services/ScaleService.h src/services/ScaleService.cpp test/test_scale_service/test_main.cpp
git commit -m "feat: create ScaleService to encapsulate scale and pressure sensor state"
```

---

### Task 3.2: Inject ScaleService into SystemContext

**Files:**
- Modify: `include/clevercoffee/context/SystemContext.h`
- Modify: `src/context/SystemContext.cpp`

**Step 1: Add ScaleService member**

```cpp
private:
    std::unique_ptr<ScaleService> scaleService_;

public:
    ScaleService& scaleService() noexcept { return *scaleService_; }
    const ScaleService& scaleService() const noexcept { return *scaleService_; }
```

**Step 2: Initialize in constructor**

```cpp
SystemContext::SystemContext()
    : pidService_(std::make_unique<PIDService>())
    , scaleService_(std::make_unique<ScaleService>())
{
    // ...
}
```

**Step 3: Maintain backward compatibility accessors**

Add methods that delegate to scaleService:
```cpp
double SystemContext::sensorsCurrBrewWeight() const noexcept {
    return scaleService_->getCurrentBrewWeight();
}

void SystemContext::setSensorsCurrBrewWeight(double weight) noexcept {
    scaleService_->setCurrentBrewWeight(weight);
}
// ... and all others
```

**Step 4: Build and test**

```bash
~/.platformio/penv/bin/pio run -e esp32_usb -s
~/.platformio/penv/bin/pio test -e esp32_usb
```

**Step 5: Commit**

```bash
git add include/clevercoffee/context/SystemContext.h src/context/SystemContext.cpp
git commit -m "refactor: inject ScaleService into SystemContext"
```

---

### Task 3.3: Update WebServerManager to use ScaleService

**Files:**
- Modify: `src/network/WebServerManager.cpp:1-400`

**Step 1: Replace g_state.sensors access**

Find all references:
```bash
rg "g_state\.sensors\." src/network/WebServerManager.cpp | head -20
```

Replace each with SystemContext accessor:
```cpp
// Before
g_state.sensors.scaleCalibrationOn = true;
bool tare = g_state.sensors.scaleTareOn;

// After
auto& ctx = CleverCoffee::getGlobalSystemContext();
ctx.scaleService().setCalibrationOn(true);
bool tare = ctx.scaleService().isTareOn();
```

**Step 2: Verify all replaced**

```bash
rg "g_state\.sensors\." src/network/WebServerManager.cpp
```

Expected: 0 results

**Step 3: Build and test**

```bash
~/.platformio/penv/bin/pio run -e esp32_usb -s
~/.platformio/penv/bin/pio test -e esp32_usb -f test_network
```

**Step 4: Commit**

```bash
git add src/network/WebServerManager.cpp
git commit -m "refactor: WebServerManager uses ScaleService instead of g_state.sensors"
```

---

## Phase 4: Extract Network Service (1.5 hours, LOW RISK)

### Task 4.1: Create NetworkService

**Files:**
- Create: `include/clevercoffee/services/NetworkService.h`
- Create: `src/services/NetworkService.cpp`

**Step 1: Write NetworkService header**

```cpp
// include/clevercoffee/services/NetworkService.h
#pragma once

namespace CleverCoffee {

class CleverCoffeeWiFiManager;
class WebServerManager;

class NetworkService {
public:
    explicit NetworkService() = default;

    // WiFi manager
    CleverCoffeeWiFiManager* getWiFiManager() const noexcept { return wifiManager_; }
    void setWiFiManager(CleverCoffeeWiFiManager* manager) noexcept { wifiManager_ = manager; }

    // Web server manager
    WebServerManager* getWebServerManager() const noexcept { return webServerManager_; }
    void setWebServerManager(WebServerManager* manager) noexcept { webServerManager_ = manager; }

    // Offline mode
    bool isOfflineMode() const noexcept { return offlineMode_; }
    void setOfflineMode(bool offline) noexcept { offlineMode_ = offline; }

    // WiFi reconnection tracking
    unsigned int getWiFiReconnects() const noexcept { return wifiReconnects_; }
    void setWiFiReconnects(unsigned int count) noexcept { wifiReconnects_ = count; }
    void incrementWiFiReconnects() noexcept { ++wifiReconnects_; }

    // Hassio connection
    bool hasHassioFailed() const noexcept { return hassioFailed_; }
    void setHassioFailed(bool failed) noexcept { hassioFailed_ = failed; }

    void reset() noexcept;

private:
    CleverCoffeeWiFiManager* wifiManager_ = nullptr;
    WebServerManager* webServerManager_ = nullptr;
    bool offlineMode_ = false;
    unsigned int wifiReconnects_ = 0;
    bool hassioFailed_ = false;
};

} // namespace CleverCoffee
```

**Step 2: Create tests and implementation**

```cpp
// test/test_network_service/test_main.cpp
#include <gtest/gtest.h>
#include "clevercoffee/services/NetworkService.h"

using namespace CleverCoffee;

class NetworkServiceTest : public ::testing::Test {
protected:
    NetworkService service;
};

TEST_F(NetworkServiceTest, InitializesWithDefaults) {
    EXPECT_EQ(service.getWiFiManager(), nullptr);
    EXPECT_EQ(service.getWebServerManager(), nullptr);
    EXPECT_FALSE(service.isOfflineMode());
    EXPECT_EQ(service.getWiFiReconnects(), 0);
    EXPECT_FALSE(service.hasHassioFailed());
}

TEST_F(NetworkServiceTest, CanTrackOfflineMode) {
    service.setOfflineMode(true);
    EXPECT_TRUE(service.isOfflineMode());
}

TEST_F(NetworkServiceTest, CanTrackWiFiReconnects) {
    service.incrementWiFiReconnects();
    service.incrementWiFiReconnects();
    EXPECT_EQ(service.getWiFiReconnects(), 2);
}
```

**Step 3: Implement and test**

```bash
~/.platformio/penv/bin/pio test -e esp32_usb -f test_network_service
```

Expected: All tests PASS

**Step 4: Commit**

```bash
git add include/clevercoffee/services/NetworkService.h src/services/NetworkService.cpp test/test_network_service/test_main.cpp
git commit -m "feat: create NetworkService to encapsulate network state"
```

---

### Task 4.2: Inject NetworkService into SystemContext

**Files:**
- Modify: `include/clevercoffee/context/SystemContext.h`
- Modify: `src/context/SystemContext.cpp`

**Step 1-5: Follow same pattern as PIDService and ScaleService**

**Step 6: Commit**

```bash
git commit -m "refactor: inject NetworkService into SystemContext"
```

---

## Phase 5: Extract Sensor/Coordination Service (2 hours, MEDIUM RISK)

### Task 5.1: Create CoordinationService

**Files:**
- Create: `include/clevercoffee/services/CoordinationService.h`
- Create: `src/services/CoordinationService.cpp`

**Purpose:** Encapsulate coordination flags (displayBufferReady, hassioUpdateRunning, temperatureUpdateRunning, processController reference).

**Step 1-6: Follow established pattern from previous services**

---

## Phase 6: Update All Remaining References (3-4 hours, MEDIUM RISK)

### Task 6.1: Update Display Templates

**Files:**
- Modify: `include/clevercoffee/display/ModernDisplayTemplate.h`
- Modify: `include/clevercoffee/display/displayCommon.h`

Replace all `g_state` access with SystemContext service accessors.

### Task 6.2: Update embeddedWebserver.h

**Files:**
- Modify: `include/clevercoffee/embeddedWebserver.h`

### Task 6.3: Update UIManager

**Files:**
- Modify: `src/ui/UIManager.cpp`

### Task 6.4: Update Config.h callbacks

**Files:**
- Modify: `include/clevercoffee/Config.h`

### Task 6.5: Update isr.h

**Files:**
- Modify: `include/clevercoffee/isr.h`

---

## Phase 7: Final Cleanup - Remove GlobalState (1 hour, HIGH RISK)

### Task 7.1: Verify all g_state references removed

```bash
rg "g_state\." --type cpp --type h
```

Expected: 0 results (excluding comments)

### Task 7.2: Remove GlobalState struct and instance

**Files:**
- Delete: `include/clevercoffee/GlobalState.h`
- Modify: `src/GlobalState.cpp` - Remove instantiation

### Task 7.3: Update includes

Remove all `#include "clevercoffee/GlobalState.h"` statements

### Task 7.4: Final build and test

```bash
~/.platformio/penv/bin/pio run -e esp32_usb
~/.platformio/penv/bin/pio test -e esp32_usb
```

Expected: Everything builds and tests pass

---

## Testing Strategy

### Unit Tests Required
- Each service has comprehensive unit tests (in Phase 2-5)
- SystemContext integration tests verify service injection
- Existing component tests (ProcessController, WebServer, etc.) should pass unchanged

### Integration Tests
- Full system initialization test
- State flow through multiple services
- Display rendering with services
- Network communication with services

### Build Verification
```bash
# Clean build
~/.platformio/penv/bin/pio run -e esp32_usb

# Run all tests
~/.platformio/penv/bin/pio test -e esp32_usb

# Verify no g_state references
rg "g_state\." --type cpp --type h
```

---

## Risk Mitigation

| Phase | Risk Level | Mitigation |
|-------|-----------|-----------|
| 0 (Cleanup) | Zero | No functional code changes, only removal of dead code |
| 1 (PID Service) | Low | PID is well-tested, SystemContext provides adapter layer |
| 2 (Scale Service) | Low | Scale is relatively isolated, API is simple |
| 3 (Network Service) | Low | Mostly moved to managers already |
| 4 (Sensor Service) | Medium | Some display coupling, tested with existing suites |
| 5 (Remaining refs) | Medium | Must verify all web/mqtt endpoints still work |
| 6 (Remove GlobalState) | High | Point of no return, must have all references migrated |

---

## Rollback Plan

If any phase causes issues:
1. Return to previous commit: `git checkout <commit-sha>`
2. Identify root cause in that phase's changes
3. Fix in new branch: `git checkout -b fix/<phase-name>`
4. Add test case to prevent regression
5. Retry phase with fix

---

## Success Criteria

- ✅ Zero g_state references in application code
- ✅ All services have unit tests (>90% coverage)
- ✅ SystemContext provides clean dependency injection API
- ✅ Firmware builds cleanly, tests pass
- ✅ Memory usage unchanged or improved
- ✅ No functional regressions
- ✅ Code is more testable and maintainable

---

## Estimated Timeline

| Phase | Effort | Prerequisite |
|-------|--------|-------------|
| 0 | 30 min | None |
| 1 (PID) | 2 hrs | Phase 0 complete |
| 2 (Scale) | 2.5 hrs | Phase 1 complete |
| 3 (Network) | 1.5 hrs | Phase 2 complete |
| 4 (Coordination) | 2 hrs | Phase 3 complete |
| 5 (Remaining) | 3-4 hrs | Phase 4 complete |
| 6 (Cleanup) | 1 hr | Phase 5 complete |
| **TOTAL** | **12-14 hours** | - |

**Parallel work possible:** Phases can be partially parallelized once SystemContext structure is in place, but testing must be sequential to verify no regressions.

---

## Documentation

All changes maintain:
- Doxygen-compatible function documentation
- Inline code comments for complex logic
- Architecture documentation in context/
- Test file naming conventions
- Commit message clarity (feat:, refactor:, test:)

---

## Next Steps

Choose execution method:

1. **Subagent-Driven (this session)** - Fresh agent per task, code review between tasks
2. **Parallel Session (separate)** - New session with executing-plans, batch execution

Which approach?
