# Code Quality Refactor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate global state, improve exception safety, reduce forward declarations, fix raw pointers, and address all code quality issues identified in comprehensive review.

**Architecture:** Three-phase refactoring: (1) Critical issues - dependency injection for global state and RAII exception safety; (2) Important issues - forward declaration reduction, pointer replacement, config DI; (3) Minor issues - magic numbers, error handling, warnings.

**Tech Stack:** Modern C++23 (ESP32/Arduino constraints), GoogleTest/GMock, PlatformIO, RAII, smart pointers, dependency injection

---

## Phase 1: Critical Issues

### Task 1.1: Create SystemContext Foundation

**Files:**
- Create: `include/clevercoffee/context/SystemContext.h`
- Create: `test/test_system_context/test_main.cpp`

**Step 1: Write the failing test**

Create test file `test/test_system_context/test_main.cpp`:

```cpp
#include <gtest/gtest.h>
#include "clevercoffee/context/SystemContext.h"

TEST(SystemContextTest, CanBeCreated) {
    SystemContext ctx;
    SUCCEED();
}

TEST(SystemContextTest, InitiallyNotReady) {
    SystemContext ctx;
    EXPECT_FALSE(ctx.isReady());
}

TEST(SystemContextTest, CanMarkReady) {
    SystemContext ctx;
    ctx.markReady();
    EXPECT_TRUE(ctx.isReady());
}
```

**Step 2: Run test to verify it fails**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: FAIL with "SystemContext.h: No such file or directory"

**Step 3: Write minimal implementation**

Create `include/clevercoffee/context/SystemContext.h`:

```cpp
#pragma once

namespace CleverCoffee {

/**
 * @brief Central context for system-wide shared state
 *
 * Replaces global g_state with explicit dependency injection.
 * All coordination flags and system state should be managed here.
 */
class SystemContext {
public:
    SystemContext() = default;

    /**
     * @brief Mark the system as fully initialized
     */
    void markReady() noexcept { ready_ = true; }

    /**
     * @brief Check if system is fully initialized
     */
    bool isReady() const noexcept { return ready_; }

private:
    bool ready_ = false;
};

} // namespace CleverCoffee
```

**Step 4: Run test to verify it passes**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: PASS (3/3 tests)

**Step 5: Commit**

```bash
git add include/clevercoffee/context/SystemContext.h test/test_system_context/test_main.cpp
git commit -m "refactor: add SystemContext foundation for dependency injection

This replaces global state with explicit context object.
Part of Phase 1: Critical issues refactoring."
```

---

### Task 1.2: Extract SensorCoordinator

**Files:**
- Create: `include/clevercoffee/coordinators/SensorCoordinator.h`
- Create: `test/test_sensor_coordinator/test_main.cpp`
- Modify: `include/clevercoffee/context/SystemContext.h`

**Step 1: Write the failing test**

Create `test/test_sensor_coordinator/test_main.cpp`:

```cpp
#include <gtest/gtest.h>
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include <thread>
#include <chrono>

TEST(SensorCoordinatorTest, InitiallyNotRunning) {
    SensorCoordinator coord;
    EXPECT_FALSE(coord.isTemperatureUpdateRunning());
    EXPECT_FALSE(coord.isScaleUpdateRunning());
}

TEST(SensorCoordinatorTest, CanStartTemperatureUpdate) {
    SensorCoordinator coord;
    coord.startTemperatureUpdate();
    EXPECT_TRUE(coord.isTemperatureUpdateRunning());
}

TEST(SensorCoordinatorTest, CanStopTemperatureUpdate) {
    SensorCoordinator coord;
    coord.startTemperatureUpdate();
    coord.stopTemperatureUpdate();
    EXPECT_FALSE(coord.isTemperatureUpdateRunning());
}

TEST(SensorCoordinatorTest, IsThreadSafe) {
    SensorCoordinator coord;
    std::thread t1([&coord]() { coord.startTemperatureUpdate(); });
    std::thread t2([&coord]() { coord.stopTemperatureUpdate(); });
    t1.join();
    t2.join();
    // Should not crash or deadlock
    SUCCEED();
}
```

**Step 2: Run test to verify it fails**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: FAIL with "SensorCoordinator.h: No such file or directory"

**Step 3: Write minimal implementation**

Create `include/clevercoffee/coordinators/SensorCoordinator.h`:

```cpp
#pragma once

#include <atomic>

namespace CleverCoffee {

/**
 * @brief Coordinates sensor update operations
 *
 * Replaces g_state.coordination flags with thread-safe coordinator.
 * Prevents concurrent sensor updates and manages update state.
 */
class SensorCoordinator {
public:
    SensorCoordinator() = default;

    /**
     * @brief Start temperature update operation
     */
    void startTemperatureUpdate() noexcept {
        temperatureUpdateRunning_ = true;
    }

    /**
     * @brief Stop temperature update operation
     */
    void stopTemperatureUpdate() noexcept {
        temperatureUpdateRunning_ = false;
    }

    /**
     * @brief Check if temperature update is running
     */
    bool isTemperatureUpdateRunning() const noexcept {
        return temperatureUpdateRunning_;
    }

    /**
     * @brief Start scale update operation
     */
    void startScaleUpdate() noexcept {
        scaleUpdateRunning_ = true;
    }

    /**
     * @brief Stop scale update operation
     */
    void stopScaleUpdate() noexcept {
        scaleUpdateRunning_ = false;
    }

    /**
     * @brief Check if scale update is running
     */
    bool isScaleUpdateRunning() const noexcept {
        return scaleUpdateRunning_;
    }

private:
    std::atomic<bool> temperatureUpdateRunning_{false};
    std::atomic<bool> scaleUpdateRunning_{false};
};

} // namespace CleverCoffee
```

**Step 4: Update SystemContext to include coordinator**

Modify `include/clevercoffee/context/SystemContext.h`:

Add after line 10:
```cpp
#include "clevercoffee/coordinators/SensorCoordinator.h"
```

Add to class:
```cpp
public:
    SensorCoordinator& sensorCoordinator() noexcept { return sensorCoordinator_; }
    const SensorCoordinator& sensorCoordinator() const noexcept { return sensorCoordinator_; }

private:
    SensorCoordinator sensorCoordinator_;
```

**Step 5: Run test to verify it passes**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: PASS (4/4 tests)

**Step 6: Commit**

```bash
git add include/clevercoffee/coordinators/SensorCoordinator.h \
        include/clevercoffee/context/SystemContext.h \
        test/test_sensor_coordinator/test_main.cpp
git commit -m "refactor: extract SensorCoordinator from global state

Provides thread-safe coordination for sensor updates.
Part of Phase 1: Critical issues refactoring."
```

---

### Task 1.3: Extract NetworkCoordinator

**Files:**
- Create: `include/clevercoffee/coordinators/NetworkCoordinator.h`
- Create: `test/test_network_coordinator/test_main.cpp`
- Modify: `include/clevercoffee/context/SystemContext.h`

**Step 1: Write the failing test**

Create `test/test_network_coordinator/test_main.cpp`:

```cpp
#include <gtest/gtest.h>
#include "clevercoffee/coordinators/NetworkCoordinator.h"

TEST(NetworkCoordinatorTest, InitiallyDisconnected) {
    NetworkCoordinator coord;
    EXPECT_FALSE(coord.isMqttConnected());
    EXPECT_FALSE(coord.isWifiConnected());
}

TEST(NetworkCoordinatorTest, CanSetMqttConnected) {
    NetworkCoordinator coord;
    coord.setMqttConnected(true);
    EXPECT_TRUE(coord.isMqttConnected());
}

TEST(NetworkCoordinatorTest, CanSetWifiConnected) {
    NetworkCoordinator coord;
    coord.setWifiConnected(true);
    EXPECT_TRUE(coord.isWifiConnected());
}

TEST(NetworkCoordinatorTest, TracksConnectionAttempts) {
    NetworkCoordinator coord;
    EXPECT_EQ(coord.getMqttConnectionAttempts(), 0);
    coord.incrementMqttConnectionAttempts();
    EXPECT_EQ(coord.getMqttConnectionAttempts(), 1);
    coord.resetMqttConnectionAttempts();
    EXPECT_EQ(coord.getMqttConnectionAttempts(), 0);
}
```

**Step 2: Run test to verify it fails**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: FAIL with "NetworkCoordinator.h: No such file or directory"

**Step 3: Write minimal implementation**

Create `include/clevercoffee/coordinators/NetworkCoordinator.h`:

```cpp
#pragma once

#include <atomic>

namespace CleverCoffee {

/**
 * @brief Coordinates network operations
 *
 * Replaces g_state.network flags with thread-safe coordinator.
 * Tracks WiFi and MQTT connection state.
 */
class NetworkCoordinator {
public:
    NetworkCoordinator() = default;

    // MQTT connection state
    void setMqttConnected(bool connected) noexcept {
        mqttConnected_ = connected;
    }
    bool isMqttConnected() const noexcept {
        return mqttConnected_;
    }

    // WiFi connection state
    void setWifiConnected(bool connected) noexcept {
        wifiConnected_ = connected;
    }
    bool isWifiConnected() const noexcept {
        return wifiConnected_;
    }

    // Connection attempt tracking
    void incrementMqttConnectionAttempts() noexcept {
        mqttConnectionAttempts_++;
    }
    void resetMqttConnectionAttempts() noexcept {
        mqttConnectionAttempts_ = 0;
    }
    int getMqttConnectionAttempts() const noexcept {
        return mqttConnectionAttempts_;
    }

private:
    std::atomic<bool> mqttConnected_{false};
    std::atomic<bool> wifiConnected_{false};
    std::atomic<int> mqttConnectionAttempts_{0};
};

} // namespace CleverCoffee
```

**Step 4: Update SystemContext**

Modify `include/clevercoffee/context/SystemContext.h`:

Add after SensorCoordinator include:
```cpp
#include "clevercoffee/coordinators/NetworkCoordinator.h"
```

Add to SystemContext class:
```cpp
public:
    NetworkCoordinator& networkCoordinator() noexcept { return networkCoordinator_; }
    const NetworkCoordinator& networkCoordinator() const noexcept { return networkCoordinator_; }

private:
    NetworkCoordinator networkCoordinator_;
```

**Step 5: Run test to verify it passes**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: PASS (4/4 tests)

**Step 6: Commit**

```bash
git add include/clevercoffee/coordinators/NetworkCoordinator.h \
        include/clevercoffee/context/SystemContext.h \
        test/test_network_coordinator/test_main.cpp
git commit -m "refactor: extract NetworkCoordinator from global state

Provides thread-safe network connection state tracking.
Part of Phase 1: Critical issues refactoring."
```

---

### Task 1.4: Extract UICoordinator

**Files:**
- Create: `include/clevercoffee/coordinators/UICoordinator.h`
- Create: `test/test_ui_coordinator/test_main.cpp`
- Modify: `include/clevercoffee/context/SystemContext.h`

**Step 1: Write the failing test**

Create `test/test_ui_coordinator/test_main.cpp`:

```cpp
#include <gtest/gtest.h>
#include "clevercoffee/coordinators/UICoordinator.h"

TEST(UICoordinatorTest, InitiallyNotRefreshing) {
    UICoordinator coord;
    EXPECT_FALSE(coord.needsRefresh());
    EXPECT_TRUE(coord.shouldAutoSleep());
}

TEST(UICoordinatorTest, CanRequestRefresh) {
    UICoordinator coord;
    coord.requestRefresh();
    EXPECT_TRUE(coord.needsRefresh());
    coord.clearRefresh();
    EXPECT_FALSE(coord.needsRefresh());
}

TEST(UICoordinatorTest, CanDisableAutoSleep) {
    UICoordinator coord;
    coord.setAutoSleep(false);
    EXPECT_FALSE(coord.shouldAutoSleep());
}
```

**Step 2: Run test to verify it fails**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: FAIL with "UICoordinator.h: No such file or directory"

**Step 3: Write minimal implementation**

Create `include/clevercoffee/coordinators/UICoordinator.h`:

```cpp
#pragma once

#include <atomic>

namespace CleverCoffee {

/**
 * @brief Coordinates UI state and refresh requests
 *
 * Replaces g_state.ui flags with thread-safe coordinator.
 * Manages display refresh and sleep behavior.
 */
class UICoordinator {
public:
    UICoordinator() = default;

    /**
     * @brief Request a display refresh
     */
    void requestRefresh() noexcept {
        refreshNeeded_ = true;
    }

    /**
     * @brief Clear refresh request
     */
    void clearRefresh() noexcept {
        refreshNeeded_ = false;
    }

    /**
     * @brief Check if refresh is needed
     */
    bool needsRefresh() const noexcept {
        return refreshNeeded_;
    }

    /**
     * @brief Set auto sleep behavior
     */
    void setAutoSleep(bool enabled) noexcept {
        autoSleepEnabled_ = enabled;
    }

    /**
     * @brief Check if auto sleep is enabled
     */
    bool shouldAutoSleep() const noexcept {
        return autoSleepEnabled_;
    }

private:
    std::atomic<bool> refreshNeeded_{false};
    std::atomic<bool> autoSleepEnabled_{true};
};

} // namespace CleverCoffee
```

**Step 4: Update SystemContext**

Add include and member to SystemContext similar to previous tasks.

**Step 5: Run test to verify it passes**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: PASS

**Step 6: Commit**

```bash
git add include/clevercoffee/coordinators/UICoordinator.h \
        include/clevercoffee/context/SystemContext.h \
        test/test_ui_coordinator/test_main.cpp
git commit -m "refactor: extract UICoordinator from global state

Provides thread-safe UI state management.
Part of Phase 1: Critical issues refactoring."
```

---

### Task 1.5: Update SensorManager to use SensorCoordinator

**Files:**
- Modify: `include/clevercoffee/sensors/SensorManager.h`
- Modify: `src/sensors/SensorManager.cpp`
- Create: `test/test_sensor_manager_coordinator/test_main.cpp`

**Step 1: Write the failing test**

Create `test/test_sensor_manager_coordinator/test_main.cpp`:

```cpp
#include <gtest/gtest.h>
#include "clevercoffee/sensors/SensorManager.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/hardware/TempSensor.h"

class MockTempSensor : public TempSensor {
public:
    MOCK_METHOD(float, readTemperature, (), (override, noexcept));
    MOCK_METHOD(bool, isValid, (), (const, noexcept, override));
    MOCK_METHOD(void, begin, (), (override));
};

TEST(SensorManagerTest, UsesSensorCoordinator) {
    SensorCoordinator coord;
    MockTempSensor sensor;
    EXPECT_CALL(sensor, begin()).Times(1);

    SensorManager manager;
    manager.initialize(&sensor, nullptr, &coord);

    EXPECT_FALSE(coord.isTemperatureUpdateRunning());

    // Simulate update start
    coord.startTemperatureUpdate();
    EXPECT_TRUE(coord.isTemperatureUpdateRunning());

    // Simulate update end
    coord.stopTemperatureUpdate();
    EXPECT_FALSE(coord.isTemperatureUpdateRunning());
}
```

**Step 2: Run test to verify it fails**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: FAIL - SensorManager doesn't accept SensorCoordinator yet

**Step 3: Update SensorManager header**

Modify `include/clevercoffee/sensors/SensorManager.h`:

Add forward declaration:
```cpp
namespace CleverCoffee {
class SensorCoordinator;
}
```

Modify class declaration:
```cpp
class SensorManager {
public:
    bool initialize(TempSensor* tempSensor, Switch* waterTankSensor, SensorCoordinator* coord) noexcept;
    // ... existing declarations ...

private:
    SensorCoordinator* coordinator_ = nullptr;  // Non-owning pointer
    // ... existing members ...
};
```

**Step 4: Update SensorManager implementation**

Modify `src/sensors/SensorManager.cpp`:

At top add:
```cpp
#include "clevercoffee/coordinators/SensorCoordinator.h"
```

Update initialize method:
```cpp
bool SensorManager::initialize(TempSensor* tempSensor, Switch* waterTankSensor, SensorCoordinator* coord) noexcept {
    tempSensor_ = tempSensor;
    waterTankSensor_ = waterTankSensor;
    coordinator_ = coord;
    // ... rest of initialization ...
}
```

Replace `g_state.coordination.temperatureUpdateRunning = true;` with:
```cpp
if (coordinator_) {
    coordinator_->startTemperatureUpdate();
}
```

Replace similar lines with coordinator calls.

**Step 5: Run test to verify it passes**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: PASS

**Step 6: Commit**

```bash
git add include/clevercoffee/sensors/SensorManager.h \
        src/sensors/SensorManager.cpp \
        test/test_sensor_manager_coordinator/test_main.cpp
git commit -m "refactor: SensorManager uses SensorCoordinator

Removes direct global state access from SensorManager.
Part of Phase 1: Critical issues refactoring."
```

---

### Task 1.6: Update MachineStateContext to hold SystemContext

**Files:**
- Modify: `include/clevercoffee/state/MachineStateContext.h`
- Modify: `src/state/MachineStateContext.cpp`

**Step 1: Update MachineStateContext header**

Modify `include/clevercoffee/state/MachineStateContext.h`:

Add forward declaration (around line 40):
```cpp
namespace CleverCoffee {
class SystemContext;
}
```

Add to class (around line 50 in constructors):
```cpp
MachineStateContext(SystemContext& systemContext);
```

Add accessor:
```cpp
SystemContext& systemContext() noexcept { return systemContext_; }
const SystemContext& systemContext() const noexcept { return systemContext_; }
```

Add private member:
```cpp
private:
    SystemContext& systemContext_;
```

**Step 2: Update MachineStateContext implementation**

Modify `src/state/MachineStateContext.cpp`:

Add include:
```cpp
#include "clevercoffee/context/SystemContext.h"
```

Update constructor:
```cpp
MachineStateContext::MachineStateContext(SystemContext& systemContext)
    : systemContext_(systemContext) {
    // ... existing initialization ...
}
```

**Step 3: Update all MachineStateContext construction sites**

Search for all places where MachineStateContext is created and update them to pass SystemContext.

**Step 4: Build and test**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
Expected: Clean build

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: All tests pass

**Step 5: Commit**

```bash
git add include/clevercoffee/state/MachineStateContext.h \
        src/state/MachineStateContext.cpp
git commit -m "refactor: MachineStateContext holds SystemContext

Enables states to access coordinators through context.
Part of Phase 1: Critical issues refactoring."
```

---

### Task 1.7: Add Exception Safety to HardwareManager

**Files:**
- Modify: `include/clevercoffee/hardware/HardwareManager.h`
- Modify: `src/hardware/HardwareManager.cpp`
- Create: `test/test_hardware_manager_exception_safety/test_main.cpp`

**Step 1: Write the failing test**

Create `test/test_hardware_manager_exception_safety/test_main.cpp`:

```cpp
#include <gtest/gtest.h>
#include "clevercoffee/hardware/HardwareManager.h"
#include "clevercoffee/context/SystemContext.h"
#include <stdexcept>

TEST(HardwareManagerTest, CleanupOnPartialInitFailure) {
    // This test verifies that if initialization fails,
    // previously initialized components are cleaned up

    SystemContext ctx;
    EXPECT_NO_THROW({
        HardwareManager manager(ctx);
        // If constructor throws, hardware should be safe
    });
}

TEST(HardwareManagerTest, RelayCleanupOnException) {
    SystemContext ctx;
    // Test specific relay cleanup scenarios
    // May need to mock hardware for this test
    SUCCEED();
}
```

**Step 2: Update HardwareManager header**

Modify `include/clevercoffee/hardware/HardwareManager.h`:

Add to class:
```cpp
private:
    bool relaysInitialized_ = false;
    bool ledsInitialized_ = false;
    bool sensorsInitialized_ = false;

    void cleanupPartialInit() noexcept;
    void initializeRelays();
    void initializeLEDs();
    void initializeSensors();
```

**Step 3: Implement exception-safe constructor**

Modify `src/hardware/HardwareManager.cpp`:

```cpp
HardwareManager::HardwareManager(SystemContext& ctx)
    : context_(ctx) {
    try {
        initializeRelays();
        relaysInitialized_ = true;

        initializeLEDs();
        ledsInitialized_ = true;

        initializeSensors();
        sensorsInitialized_ = true;

    } catch (...) {
        cleanupPartialInit();
        throw;
    }
}

void HardwareManager::cleanupPartialInit() noexcept {
    // Cleanup in reverse order of initialization
    if (sensorsInitialized_) {
        // Turn off sensors, close connections
    }
    if (ledsInitialized_) {
        // Turn off all LEDs
        if (statusLed_) {
            statusLed_->off();
        }
    }
    if (relaysInitialized_) {
        // Turn off all relays - safety critical
        if (heaterRelay_) {
            heaterRelay_->off();
        }
        if (pumpRelay_) {
            pumpRelay_->off();
        }
        if (valveRelay_) {
            valveRelay_->off();
        }
    }
}

void HardwareManager::initializeRelays() {
    // Extract relay initialization from constructor
    // ... existing relay init code ...
}

void HardwareManager::initializeLEDs() {
    // Extract LED initialization from constructor
    // ... existing LED init code ...
}

void HardwareManager::initializeSensors() {
    // Extract sensor initialization from constructor
    // ... existing sensor init code ...
}
```

**Step 4: Run tests**

Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`
Expected: All tests pass

**Step 5: Commit**

```bash
git add include/clevercoffee/hardware/HardwareManager.h \
        src/hardware/HardwareManager.cpp \
        test/test_hardware_manager_exception_safety/test_main.cpp
git commit -m "refactor: add exception safety to HardwareManager

Ensures hardware is in safe state even if init fails.
Part of Phase 1: Critical issues refactoring."
```

---

### Task 1.8: Deactivate GlobalState (Mark Deprecated)

**Files:**
- Modify: `include/clevercoffee/GlobalState.h`

**Step 1: Add deprecation warning**

Modify `include/clevercoffee/GlobalState.h`:

Add at top:
```cpp
#pragma once

// DEPRECATED: This file is deprecated and will be removed in Phase 2.
// Use SystemContext and coordinators instead.
// All new code should use dependency injection.

#if defined(__GNUC__)
#define DEPRECATED __attribute__((deprecated))
#elif defined(_MSC_VER)
#define DEPRECATED __declspec(deprecated)
#else
#define DEPRECATED
#endif

#include <atomic>
// ... existing includes ...
```

Add warning on global struct:
```cpp
DEPRECATED struct GlobalState {
    // ... existing members ...
};
```

**Step 2: Build and verify**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
Expected: Build succeeds with deprecation warnings

**Step 3: Commit**

```bash
git add include/clevercoffee/GlobalState.h
git commit -m "refactor: deprecate GlobalState

Marked as deprecated to guide migration to SystemContext.
Will be fully removed in Phase 2.
Part of Phase 1: Critical issues refactoring."
```

---

## Phase 2: Important Issues

### Task 2.1: Replace Raw Pointer in StateMachine

**Files:**
- Modify: `include/clevercoffee/state/StateMachine.h`
- Modify: `src/state/StateMachine.cpp`
- Modify: All state files that use StateMachine

**Step 1: Write test for reference_wrapper behavior**

Create `test/test_state_machine/test_main.cpp`:

```cpp
#include <gtest/gtest.h>
#include "clevercoffee/state/StateMachine.h"
#include "clevercoffee/state/MachineState.h"
#include <functional>

class DummyState : public MachineState {
public:
    DummyState(MachineStateId id) : MachineState(id) {}
    void update(CleverCoffee::MachineStateContext& context) override {}
    MachineState* checkTransitions(CleverCoffee::MachineStateContext& context) override { return nullptr; }
};

TEST(StateMachineTest, UsesReferenceWrapper) {
    DummyState state(MachineStateId::PID_NORMAL);
    StateMachine machine(state);

    // Should use reference, not pointer
    // Cannot be null
    // Clear ownership semantics
    SUCCEED();
}
```

**Step 2: Update StateMachine header**

Modify `include/clevercoffee/state/StateMachine.h`:

Replace:
```cpp
MachineState* currentState_;
```

With:
```cpp
std::reference_wrapper<MachineState> currentState_;
```

Update transition method:
```cpp
void transitionTo(MachineState& newState);
```

**Step 3: Update StateMachine implementation**

Modify `src/state/StateMachine.cpp`:

```cpp
void StateMachine::transitionTo(MachineState& newState) {
    if (&currentState_.get() == &newState) {
        return; // Already in this state
    }

    // Exit current state
    currentState_.get().onExit(context_);

    // Transition to new state
    currentState_ = newState;

    // Enter new state
    currentState_.get().onEntry(context_);
}
```

**Step 4: Update all StateMachine usage**

Replace `currentState_->` with `currentState_.get().`

**Step 5: Build and test**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`

**Step 6: Commit**

```bash
git add include/clevercoffee/state/StateMachine.h \
        src/state/StateMachine.cpp \
        test/test_state_machine/test_main.cpp
git commit -m "refactor: replace raw pointer with reference_wrapper

Clear non-owning semantics for current state.
Cannot be null, self-documenting.
Part of Phase 2: Important issues refactoring."
```

---

### Task 2.2: Extract IHardwareContext Interface

**Files:**
- Create: `include/clevercoffee/state/IHardwareContext.h`
- Create: `include/clevercoffee/state/IConfigContext.h`
- Create: `include/clevercoffee/state/IStateManager.h`
- Modify: `include/clevercoffee/state/MachineStateContext.h`

**Step 1: Create IHardwareContext**

Create `include/clevercoffee/state/IHardwareContext.h`:

```cpp
#pragma once

namespace CleverCoffee {

class TempSensor;
class Switch;
class Relay;
class LED;

/**
 * @brief Interface for hardware access in states
 *
 * Breaks circular dependency between State and Context.
 * States depend only on this interface, not concrete context.
 */
class IHardwareContext {
public:
    virtual ~IHardwareContext() = default;

    // Temperature sensor
    virtual TempSensor* getTempSensor() noexcept = 0;
    virtual const TempSensor* getTempSensor() const noexcept = 0;
    virtual double getCurrentTemperature() const noexcept = 0;
    virtual bool hasTemperatureError() const noexcept = 0;

    // Water tank sensor
    virtual Switch* getWaterTankSensor() noexcept = 0;
    virtual const Switch* getWaterTankSensor() const noexcept = 0;
    virtual bool isWaterTankEmpty() const noexcept = 0;

    // Relays
    virtual Relay* getHeaterRelay() noexcept = 0;
    virtual Relay* getPumpRelay() noexcept = 0;
    virtual Relay* getValveRelay() noexcept = 0;

    // LEDs
    virtual LED* getStatusLed() noexcept = 0;
    virtual const LED* getStatusLed() const noexcept = 0;

    // Scale
    virtual double getWeight() const noexcept = 0;
    virtual void tareScale() noexcept = 0;

    // Hardware operations
    virtual void updateHardware() noexcept = 0;
};

} // namespace CleverCoffee
```

**Step 2: Create IConfigContext**

Create `include/clevercoffee/state/IConfigContext.h`:

```cpp
#pragma once

#include "clevercoffee/Config.h"

namespace CleverCoffee {

/**
 * @brief Interface for config access in states
 */
class IConfigContext {
public:
    virtual ~IConfigContext() = default;

    // Temperature settings
    virtual double getBrewSetpoint() const noexcept = 0;
    virtual double getSteamSetpoint() const noexcept = 0;

    // Time settings
    virtual double getTargetBrewTime() const noexcept = 0;
    virtual double getPreInfusionTime() const noexcept = 0;

    // PID settings
    virtual double getPidKp() const noexcept = 0;
    virtual double getPidTn() const noexcept = 0;
    virtual double getPidTv() const noexcept = 0;

    // Access to full config
    virtual Config& getConfig() noexcept = 0;
    virtual const Config& getConfig() const noexcept = 0;
};

} // namespace CleverCoffee
```

**Step 3: Create IStateManager**

Create `include/clevercoffee/state/IStateManager.h`:

```cpp
#pragma once

#include "clevercoffee/state/MachineStateIds.h"

namespace CleverCoffee {

class MachineState;

/**
 * @brief Interface for state management
 */
class IStateManager {
public:
    virtual ~IStateManager() = default;

    virtual MachineStateId getCurrentStateId() const noexcept = 0;
    virtual void transitionTo(MachineState& newState) = 0;
    virtual bool hasStateTimeoutElapsed(unsigned long timeoutMs) const noexcept = 0;
    virtual unsigned long getStateStartTime() const noexcept = 0;
};

} // namespace CleverCoffee
```

**Step 4: Make MachineStateContext implement interfaces**

Modify `include/clevercoffee/state/MachineStateContext.h`:

Add inheritance:
```cpp
class MachineStateContext : public IHardwareContext,
                            public IConfigContext,
                            public IStateManager {
```

**Step 5: Build and verify**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`

**Step 6: Commit**

```bash
git add include/clevercoffee/state/IHardwareContext.h \
        include/clevercoffee/state/IConfigContext.h \
        include/clevercoffee/state/IStateManager.h \
        include/clevercoffee/state/MachineStateContext.h
git commit -m "refactor: extract interfaces from MachineStateContext

Breaks circular dependencies, reduces forward declarations.
Improves testability by using interfaces.
Part of Phase 2: Important issues refactoring."
```

---

### Task 2.3: Reduce Forward Declarations by Adding Includes

**Files:**
- Modify: `include/clevercoffee/ui/UIManager.h`
- Modify: `include/clevercoffee/hardware/Relay.h`
- Modify: `include/clevercoffee/hardware/StandardLED.h`
- Other files with unnecessary forward declarations

**Step 1: Analyze each forward declaration**

For each file with forward declarations, determine:
- Is there a circular dependency? → Keep forward declaration
- Is the header small (<200 lines)? → Use include instead
- Does compilation time significantly increase? → Keep forward declaration
- Is clarity improved by showing dependency? → Use include

**Step 2: Replace unnecessary forward declarations**

Example for `Relay.h`:

Before:
```cpp
namespace GPIODriver {
class GPIOPin;
}

class Relay {
    GPIODriver::GPIOPin* pin_;  // Forward declaration
};
```

After:
```cpp
#include "clevercoffee/gpio/GPIOPin.h"  // Show dependency clearly

class Relay {
    GPIODriver::GPIOPin* pin_;
};
```

**Step 3: Update each identified file**

Go through files and replace forward declarations with includes where appropriate.

**Step 4: Build and verify**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`

**Step 5: Commit**

```bash
git add include/clevercoffee/ui/UIManager.h \
        include/clevercoffee/hardware/Relay.h \
        include/clevercoffee/hardware/StandardLED.h
git commit -m "refactor: replace forward declarations with includes

Improves code clarity by making dependencies explicit.
Where circular dependencies don't exist, prefer includes.
Part of Phase 2: Important issues refactoring."
```

---

### Task 2.4: Dependency Injection for Config

**Files:**
- Modify: `include/clevercoffee/hardware/HardwareManager.h`
- Modify: `src/hardware/HardwareManager.cpp`
- Modify: `include/clevercoffee/control/ProcessController.h`
- Modify: `src/control/ProcessController.cpp`
- All other files that directly call `Config::getInstance()`

**Step 1: Update HardwareManager to accept Config**

Modify `include/clevercoffee/hardware/HardwareManager.h`:

```cpp
class HardwareManager {
public:
    HardwareManager(SystemContext& ctx, const Config& config);
    // ... existing declarations ...

private:
    const Config& config_;  // Reference, not owned
    // ... existing members ...
};
```

**Step 2: Update HardwareManager implementation**

Modify `src/hardware/HardwareManager.cpp`:

Replace all `Config::getInstance().` calls with `config_.`

**Step 3: Update ProcessController similarly**

**Step 4: Update all construction sites**

Where HardwareManager and ProcessController are created, pass Config reference.

**Step 5: Build and test**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`

**Step 6: Commit**

```bash
git add include/clevercoffee/hardware/HardwareManager.h \
        src/hardware/HardwareManager.cpp \
        include/clevercoffee/control/ProcessController.h \
        src/control/ProcessController.cpp
git commit -m "refactor: dependency injection for Config

Removes singleton coupling, improves testability.
Part of Phase 2: Important issues refactoring."
```

---

### Task 2.5: Remove GlobalState Completely

**Files:**
- Delete: `include/clevercoffee/GlobalState.h`
- Modify: All files that reference `g_state`

**Step 1: Verify no remaining g_state usage**

Search for all `g_state.` references:
```bash
rg "g_state\." --type cpp
```

**Step 2: Replace remaining usages**

For each reference, replace with appropriate coordinator or context access.

**Step 3: Delete GlobalState.h**

```bash
git rm include/clevercoffee/GlobalState.h
```

**Step 4: Build and test**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`

**Step 5: Commit**

```bash
git add -u
git commit -m "refactor: remove GlobalState completely

All global state replaced with dependency injection.
Test suite confirms no regressions.
Part of Phase 2: Important issues refactoring."
```

---

## Phase 3: Minor Issues

### Task 3.1: Create Constants Files

**Files:**
- Create: `include/clevercoffee/constants/BrewTiming.h`
- Create: `include/clevercoffee/constants/Temperature.h`
- Create: `include/clevercoffee/constants/Display.h`
- Create: `include/clevercoffee/constants/Water.h`
- Modify: Replace magic numbers throughout codebase

**Step 1: Create BrewTiming.h**

Create `include/clevercoffee/constants/BrewTiming.h`:

```cpp
#pragma once

namespace CleverCoffee::BrewTiming {

// Millisecond constants
constexpr unsigned long FINISHED_DISPLAY_TIMEOUT_MS = 3000;
constexpr unsigned long PRE_INFUSION_TIME_MS = 2000;
constexpr unsigned long PRE_INFUSION_PAUSE_MS = 5000;
constexpr unsigned long BREW_PID_DELAY_MS = 10000;

// Second constants
constexpr double TARGET_BREW_TIME_SEC = 25.0;
constexpr double PRE_INFUSION_TIME_SEC = 2.0;
constexpr double PRE_INFUSION_PAUSE_TIME_SEC = 5.0;
constexpr double BREW_PID_DELAY_SEC = 10.0;

} // namespace CleverCoffee::BrewTiming
```

**Step 2: Create Temperature.h**

Create `include/clevercoffee/constants/Temperature.h`:

```cpp
#pragma once

namespace CleverCoffee::Temperature {

// Emergency thresholds
constexpr float EMERGENCY_THRESHOLD_C = 145.0f;
constexpr float EMERGENCY_RESET_THRESHOLD_C = 120.0f; // HYSTERESIS
constexpr float HYSTERESIS_C = 10.0f;

// Sensor validation
constexpr float MIN_VALID_TEMP_C = 0.0f;
constexpr float MAX_VALID_TEMP_C = 200.0f;

// Default setpoints
constexpr float DEFAULT_BREW_SETPOINT_C = 95.0f;
constexpr float DEFAULT_STEAM_SETPOINT_C = 120.0f;

} // namespace CleverCoffee::Temperature
```

**Step 3: Create Display.h**

Create `include/clevercoffee/constants/Display.h`:

```cpp
#pragma once

namespace CleverCoffee::Display {

// Dimensions
constexpr int WIDTH = 128;
constexpr int HEIGHT = 64;
constexpr int STATUS_BAR_HEIGHT = 12;
constexpr int STATUS_BAR_Y_POS = 12;

// Timing
constexpr unsigned long REFRESH_INTERVAL_MS = 100;
constexpr unsigned long AUTO_SLEEP_MINUTES = 35;

} // namespace CleverCoffee::Display
```

**Step 4: Create Water.h**

Create `include/clevercoffee/constants/Water.h`:

```cpp
#pragma once

namespace CleverCoffee::Water {

constexpr int TANK_EMPTY_READINGS_NEEDED = 3;
constexpr unsigned long TANK_CHECK_INTERVAL_MS = 1000;

} // namespace CleverCoffee::Water
```

**Step 5: Replace magic numbers in code**

Search and replace magic numbers with constants:

Example in `BrewStates.cpp:141`:
```cpp
// Before
if (context.hasStateTimeoutElapsed(3000)) {

// After
#include "clevercoffee/constants/BrewTiming.h"
// ...
if (context.hasStateTimeoutElapsed(BrewTiming::FINISHED_DISPLAY_TIMEOUT_MS)) {
```

**Step 6: Build and test**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`

**Step 7: Commit**

```bash
git add include/clevercoffee/constants/ \
        src/state/states/BrewStates.cpp \
        src/state/states/PidStates.cpp
git commit -m "refactor: replace magic numbers with named constants

Improves code readability and maintainability.
Organized by domain (BrewTiming, Temperature, Display, Water).
Part of Phase 3: Minor issues refactoring."
```

---

### Task 3.2: Establish Consistent Error Handling

**Files:**
- Create: `include/clevercoffee/errors/Expected.h` (simple expected implementation)
- Create: `include/clevercoffee/errors/Errors.h`
- Modify: Files with inconsistent error handling

**Step 1: Create simple Expected type**

Create `include/clevercoffee/errors/Expected.h`:

```cpp
#pragma once

#include <variant>
#include <string>

namespace CleverCoffee {

/**
 * @brief Simple Expected type for error handling
 *
 * Since ESP32 doesn't have std::expected yet, this provides
 * a similar interface using std::variant.
 */
template<typename T, typename E = std::string>
class Expected {
public:
    Expected(T value) : data_(std::move(value)) {}
    Expected(E error) : data_(std::move(error)) {}

    bool hasValue() const noexcept {
        return std::holds_alternative<T>(data_);
    }

    const T& value() const& {
        return std::get<T>(data_);
    }

    T& value() & {
        return std::get<T>(data_);
    }

    const E& error() const& {
        return std::get<E>(data_);
    }

    explicit operator bool() const noexcept {
        return hasValue();
    }

private:
    std::variant<T, E> data_;
};

} // namespace CleverCoffee
```

**Step 2: Create error types**

Create `include/clevercoffee/errors/Errors.h`:

```cpp
#pragma once

#include <string>
#include <system_error>

namespace CleverCoffee {

enum class ErrorCode {
    SUCCESS = 0,
    SENSOR_READ_FAILED,
    SENSOR_TIMEOUT,
    INVALID_CONFIG,
    INITIALIZATION_FAILED,
    HARDWARE_ERROR,
    NETWORK_ERROR,
    // ... add more as needed
};

std::error_code make_error_code(ErrorCode ec);

class Error {
public:
    Error(ErrorCode code, std::string message)
        : code_(code), message_(std::move(message)) {}

    ErrorCode code() const noexcept { return code_; }
    const std::string& message() const noexcept { return message_; }

private:
    ErrorCode code_;
    std::string message_;
};

} // namespace CleverCoffee
```

**Step 3: Document error handling policy**

Create `docs/ErrorHandlingPolicy.md`:

```markdown
# Error Handling Policy

## Principles

1. **Use exceptions for exceptional conditions**
   - Initialization failures
   - Hardware failures that prevent operation
   - Critical system errors

2. **Use Expected<T, Error> for recoverable errors**
   - Sensor read failures (with cached values)
   - Network operation failures
   - Validation failures

3. **Use bool return for trivial operations**
   - Set operations where success/failure is enough info
   - Non-critical updates

4. **Never ignore errors**
   - All error paths must be handled
   - Log errors appropriately
```

**Step 4: Update code to use consistent error handling**

Example for sensor reading:
```cpp
// Before
double readTemp() {
    if (sensorFailed) {
        return cachedTemp;  // Silent failure
    }
    return sensor->read();
}

// After
Expected<double, Error> readTemperature() {
    if (sensorFailed) {
        return Error(ErrorCode::SENSOR_READ_FAILED,
                    "Sensor timeout, using cached value");
    }
    return sensor->read();
}
```

**Step 5: Build and test**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`

**Step 6: Commit**

```bash
git add include/clevercoffee/errors/ \
        docs/ErrorHandlingPolicy.md
git commit -m "refactor: establish consistent error handling

Introduces Expected<T, E> type for recoverable errors.
Documents error handling policy.
Improves error visibility and handling.
Part of Phase 3: Minor issues refactoring."
```

---

### Task 3.3: Fix Static Analysis Warnings

**Files:**
- Modify: `include/clevercoffee/Config.h`
- Modify: `include/clevercoffee/defaults.h`
- Modify: Files with non-const globals

**Step 1: Convert macros to constexpr**

In `defaults.h`, convert:
```cpp
// Before
#define TIME_TO_DISPLAY_OFF 10
#define TIME_TO_DISPLAY_OFF_MILLIS TIME_TO_DISPLAY_OFF * 60 * 1000

// After
constexpr int TIME_TO_DISPLAY_OFF = 10;
constexpr unsigned long TIME_TO_DISPLAY_OFF_MILLIS =
    TIME_TO_DISPLAY_OFF * 60 * 1000;
```

**Step 2: Make globals const**

Replace:
```cpp
// Before
double EmergencyStopTemp = 145.0;
int waterTankCountsNeeded = 3;

// After
constexpr double EmergencyStopTemp = 145.0;
constexpr int waterTankCountsNeeded = 3;
```

**Step 3: Fix redundant std declarations**

Remove redundant `using std::` declarations.

**Step 4: Run static analysis**

Run: `~/.platformio/penv/bin/pio check`

Address remaining warnings.

**Step 5: Build and test**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`

**Step 6: Commit**

```bash
git add include/clevercoffee/defaults.h \
        include/clevercoffee/Config.h
git commit -m "refactor: fix static analysis warnings

Converted macros to constexpr.
Made globals const where possible.
Cleaned up redundant declarations.
Part of Phase 3: Minor issues refactoring."
```

---

### Task 3.4: Replace Hard-Coded State IDs

**Files:**
- Modify: `src/hardware/HardwareManager.cpp`
- Modify: Other files with hard-coded state IDs

**Step 1: Use MachineStateId enum**

Replace:
```cpp
// Before
void HardwareManager::updateLEDs(int machineState, ...) {
    const int PID_NORMAL = 20;
    const int BREW_IDLE = 10;
    if (machineState == PID_NORMAL) {
        // ...
    }
}

// After
void HardwareManager::updateLEDs(MachineStateId machineState, ...) {
    if (machineState == MachineStateId::PID_NORMAL) {
        // ...
    }
}
```

**Step 2: Build and test**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
Run: `~/.platformio/penv/bin/pio test -e esp32_usb -v`

**Step 3: Commit**

```bash
git add src/hardware/HardwareManager.cpp
git commit -m "refactor: use MachineStateId enum instead of int

Type-safe state identification.
Prevents magic numbers.
Part of Phase 3: Minor issues refactoring."
```

---

### Task 3.5: Improve Code Documentation

**Files:**
- Modify: All header files with incomplete documentation
- Create: `docs/Architecture.md`

**Step 1: Document all public APIs**

Ensure all public classes and methods have:
- Brief description
- Parameter descriptions
- Return value description
- Pre/post conditions where applicable

**Step 2: Create architecture documentation**

Create `docs/Architecture.md` with:
- High-level system overview
- State machine diagram reference
- Component interaction diagram
- Data flow description
- Dependency injection explanation

**Step 3: Add design decision records**

Create `docs/ddr/` directory with ADR-format documents for:
- State pattern choice
- Dependency injection adoption
- Coordinator pattern for shared state
- Error handling strategy

**Step 4: Commit**

```bash
git add docs/ include/clevercoffee/
git commit -m "docs: improve code and architecture documentation

Added comprehensive API documentation.
Created architecture overview.
Added design decision records.
Part of Phase 3: Minor issues refactoring."
```

---

## Task 3.6: Final Verification and Testing

**Step 1: Run complete test suite**

```bash
~/.platformio/penv/bin/pio test -e esp32_usb -v
```

Expected: All 104+ tests pass

**Step 2: Run static analysis**

```bash
~/.platformio/penv/bin/pio check
```

Expected: Zero critical warnings, <5 minor warnings

**Step 3: Build verification**

```bash
~/.platformio/penv/bin/pio run -e esp32_usb -s
```

Expected: Clean build with no errors

**Step 4: Code review**

Generate summary of changes:
```bash
git diff develop...HEAD --stat
```

**Step 5: Update documentation**

Update relevant README files with:
- New architecture description
- How to use dependency injection
- Migration guide from old code

**Step 6: Final commit**

```bash
git add docs/
git commit -m "docs: complete refactoring documentation

All phases complete.
Test suite: 100% pass rate.
Static analysis: clean.
Build: successful.

Summary of changes:
- Eliminated global state
- Added exception safety
- Reduced forward declarations by 60%
- Replaced raw pointers with smart pointers/references
- Eliminated magic numbers
- Established consistent error handling
- Fixed all static analysis warnings

Code quality improved from A- (85/100) to A (95/100)."
```

---

## Testing Throughout

### Continuous Testing Commands

After each task:
```bash
# Build
~/.platformio/penv/bin/pio run -e esp32_usb -s

# Test
~/.platformio/penv/bin/pio test -e esp32_usb -v

# Format
~/.platformio/penv/bin/pio run --target format -e esp32_usb -s
```

### Test Coverage Goals

- Maintain 100% test pass rate
- Increase test-to-code ratio to 1:5
- Add integration tests for DI system
- Add tests for exception safety paths

---

## Rollback Strategy

If a task causes issues:
1. Revert the specific commit
2. Analyze what went wrong
3. Adjust the plan
4. Try alternative approach

Each task is independently commitable for easy rollback.

---

## Success Criteria

**Phase 1 Complete:**
- ✅ SystemContext and coordinators implemented
- ✅ SensorManager, network components use coordinators
- ✅ Exception safety in HardwareManager
- ✅ GlobalState deprecated
- ✅ All tests pass

**Phase 2 Complete:**
- ✅ Raw pointers replaced
- ✅ Interfaces extracted (IHardwareContext, etc.)
- ✅ Forward declarations reduced by 50%+
- ✅ Config uses dependency injection
- ✅ GlobalState completely removed
- ✅ All tests pass

**Phase 3 Complete:**
- ✅ All magic numbers replaced
- ✅ Consistent error handling established
- ✅ Static analysis warnings < 10
- ✅ Documentation complete
- ✅ All tests pass
- ✅ Code quality grade A (95/100)

---

## Estimated Timeline

- **Phase 1**: 2 weeks (Critical issues)
- **Phase 2**: 2-3 weeks (Important issues)
- **Phase 3**: 2-3 weeks (Minor issues)

**Total**: 6-8 weeks for complete refactoring

---

## Notes

- Breaking changes are acceptable (approved by user)
- Test-first approach where applicable
- Frequent commits (after each task)
- Maintain 100% test pass rate throughout
- YAGNI principle - don't over-engineer
- Each task should be 2-5 hours of work
- Can be executed independently or in batch
