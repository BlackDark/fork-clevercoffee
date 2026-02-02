# ProcessController Interface Refactoring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor ProcessController to accept interface references (IHardwareContext, IDisplayManager, IMQTTManager) instead of concrete types, enabling integration testing with mocks.

**Architecture:** Extract interfaces for DisplayManager and MQTTManager following the existing IWiFiManager pattern. Update ProcessController to accept interface references. Create corresponding mocks for testing.

**Tech Stack:** C++17, GoogleTest/GMock, PlatformIO

---

## Task 1: Create IDisplayManager Interface

**Files:**
- Create: `include/clevercoffee/display/IDisplayManager.h`

**Step 1: Create IDisplayManager interface**

Create the file with this exact content:

```cpp
/**
 * @file IDisplayManager.h
 * @brief Interface for display management - enables testing with mock implementations
 */

#pragma once

#include <U8g2lib.h>

/**
 * @class IDisplayManager
 * @brief Abstract interface for display management operations
 *
 * This interface enables dependency injection and testing of components
 * that depend on display functionality without requiring actual hardware.
 */
class IDisplayManager {
  public:
    virtual ~IDisplayManager() = default;

    /**
     * @brief Get raw U8G2 pointer for compatibility with existing code
     * @return Pointer to U8G2 instance, or nullptr if not initialized
     */
    virtual U8G2* getDisplay() const noexcept = 0;

    /**
     * @brief Check if display is successfully initialized
     * @return true if display is ready for use
     */
    virtual bool isInitialized() const noexcept = 0;
};
```

**Step 2: Verify build**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | tail -10`

Expected: Build succeeds (new header compiles without errors)

**Step 3: Commit**

```bash
git add include/clevercoffee/display/IDisplayManager.h
git commit -m "feat: add IDisplayManager interface for testability"
```

---

## Task 2: Create IMQTTManager Interface

**Files:**
- Create: `include/clevercoffee/network/IMQTTManager.h`

**Step 1: Create IMQTTManager interface**

Create the file with this exact content:

```cpp
/**
 * @file IMQTTManager.h
 * @brief Interface for MQTT management - enables testing with mock implementations
 */

#pragma once

#include <functional>

// Forward declarations
namespace CleverCoffee {
class UICoordinator;
class SensorCoordinator;
class NetworkCoordinator;
class SystemContext;
} // namespace CleverCoffee

/**
 * @class IMQTTManager
 * @brief Abstract interface for MQTT management operations
 *
 * This interface enables dependency injection and testing of components
 * that depend on MQTT functionality without requiring actual network connections.
 */
class IMQTTManager {
  public:
    virtual ~IMQTTManager() = default;

    /**
     * @brief Check if MQTT is enabled
     * @return true if MQTT is enabled
     */
    virtual bool isEnabled() const noexcept = 0;

    /**
     * @brief Check if MQTT is connected
     * @return true if connected
     */
    virtual bool isConnected() const noexcept = 0;

    /**
     * @brief Check MQTT connection and reconnect if needed
     */
    virtual void checkConnection() = 0;

    /**
     * @brief Process MQTT loop and handle messages
     */
    virtual void loop() = 0;

    /**
     * @brief Publish system parameters to MQTT
     * @param continueOnError Whether to continue on errors
     * @return 0 on success, error code on failure
     */
    virtual int writeSysParamsToMQTT(bool continueOnError = true) = 0;

    /**
     * @brief Send Home Assistant discovery messages
     * @return 0 on success, error code on failure
     */
    virtual int sendHASSIODiscoveryMsg() = 0;

    /**
     * @brief Set UI coordinator for state management
     * @param coordinator Pointer to UICoordinator
     */
    virtual void setUICoordinator(CleverCoffee::UICoordinator* coordinator) noexcept = 0;

    /**
     * @brief Set Sensor coordinator for scale mode management
     * @param coordinator Pointer to SensorCoordinator
     */
    virtual void setSensorCoordinator(CleverCoffee::SensorCoordinator* coordinator) noexcept = 0;

    /**
     * @brief Set Network coordinator for connection state management
     * @param coordinator Pointer to NetworkCoordinator
     */
    virtual void setNetworkCoordinator(CleverCoffee::NetworkCoordinator* coordinator) noexcept = 0;

    /**
     * @brief Set system context for state management
     * @param context Pointer to SystemContext
     */
    virtual void setSystemContext(CleverCoffee::SystemContext* context) noexcept = 0;

    /**
     * @brief Set update running flag
     * @param running Whether update is running
     */
    virtual void setUpdateRunning(bool running) noexcept = 0;

    /**
     * @brief Check if update is running
     * @return true if update is running
     */
    virtual bool isUpdateRunning() const noexcept = 0;

    /**
     * @brief Check if was connected previously
     * @return true if was connected
     */
    virtual bool wasConnected() const noexcept = 0;

    /**
     * @brief Set was connected flag
     * @param connected Connection state
     */
    virtual void setWasConnected(bool connected) noexcept = 0;
};
```

**Step 2: Verify build**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | tail -10`

Expected: Build succeeds

**Step 3: Commit**

```bash
git add include/clevercoffee/network/IMQTTManager.h
git commit -m "feat: add IMQTTManager interface for testability"
```

---

## Task 3: Update DisplayManager to Implement IDisplayManager

**Files:**
- Modify: `include/clevercoffee/display/DisplayManager.h`

**Step 1: Add interface inheritance**

Read the file first, then update the class declaration to inherit from IDisplayManager and add override specifiers:

```cpp
#include "clevercoffee/display/IDisplayManager.h"  // Add this include

class DisplayManager : public IDisplayManager {    // Add inheritance
  public:
    // ... constructor unchanged ...

    /**
     * @brief Get raw U8G2 pointer for compatibility with existing code
     * @return Pointer to U8G2 instance, or nullptr if not initialized
     */
    U8G2* getDisplay() const noexcept override {    // Add override
        return display_.get();
    }

    /**
     * @brief Check if display is successfully initialized
     * @return true if display is ready for use
     */
    bool isInitialized() const noexcept override {  // Add override
        return display_ != nullptr;
    }

    // ... rest unchanged ...
};
```

**Step 2: Verify build**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | tail -10`

Expected: Build succeeds

**Step 3: Commit**

```bash
git add include/clevercoffee/display/DisplayManager.h
git commit -m "refactor: DisplayManager implements IDisplayManager interface"
```

---

## Task 4: Update MQTTManager to Implement IMQTTManager

**Files:**
- Modify: `include/clevercoffee/network/MQTTManager.h`

**Step 1: Add interface inheritance and override specifiers**

Add the include and update class declaration:

```cpp
#include "clevercoffee/network/IMQTTManager.h"  // Add this include

class MQTTManager : public IMQTTManager {       // Add inheritance
  public:
    // ... constructor unchanged ...

    // Add override to all interface methods:
    bool isEnabled() const noexcept override {
        return mqttEnabled_;
    }

    bool isConnected() const noexcept override {
        return const_cast<PubSubClient&>(mqttClient_).connected();
    }

    void checkConnection() override;  // Add override

    void loop() override;  // Add override

    int writeSysParamsToMQTT(bool continueOnError = true) override;  // Add override

    int sendHASSIODiscoveryMsg() override;  // Add override

    void setUICoordinator(CleverCoffee::UICoordinator* coordinator) noexcept override {
        uiCoordinator_ = coordinator;
    }

    void setSensorCoordinator(CleverCoffee::SensorCoordinator* coordinator) noexcept override {
        sensorCoordinator_ = coordinator;
    }

    void setNetworkCoordinator(CleverCoffee::NetworkCoordinator* coordinator) noexcept override {
        networkCoordinator_ = coordinator;
    }

    void setSystemContext(CleverCoffee::SystemContext* context) noexcept override {
        systemContext_ = context;
    }

    void setUpdateRunning(bool running) noexcept override {
        mqttUpdateRunning_ = running;
    }

    bool isUpdateRunning() const noexcept override {
        return mqttUpdateRunning_;
    }

    bool wasConnected() const noexcept override {
        return mqttWasConnected_;
    }

    void setWasConnected(bool connected) noexcept override {
        mqttWasConnected_ = connected;
    }

    // ... rest unchanged ...
};
```

**Step 2: Verify build**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | tail -10`

Expected: Build succeeds

**Step 3: Commit**

```bash
git add include/clevercoffee/network/MQTTManager.h
git commit -m "refactor: MQTTManager implements IMQTTManager interface"
```

---

## Task 5: Update ProcessController Header to Use Interfaces

**Files:**
- Modify: `include/clevercoffee/control/ProcessController.h`

**Step 1: Update forward declarations and constructor**

Update the forward declarations section:

```cpp
// Forward declarations
class IDisplayManager;   // Changed from DisplayManager
class IMQTTManager;      // Changed from MQTTManager

namespace CleverCoffee {
class IHardwareContext;  // Changed from HardwareManager
class SystemContext;
} // namespace CleverCoffee
```

Update the constructor signature (around line 54):

```cpp
ProcessController(const Config&                   config,
                  CleverCoffee::SystemContext&    systemContext,
                  CleverCoffee::IHardwareContext& hardwareManager,  // Changed
                  IDisplayManager&                displayManager,   // Changed
                  IMQTTManager&                   mqttManager);     // Changed
```

Update the member variables (around line 363):

```cpp
// Manager dependencies - ALL REQUIRED
CleverCoffee::IHardwareContext& hardwareManager_; // Changed - CRITICAL component
IDisplayManager&                displayManager_;  // Changed - always exists
IMQTTManager&                   mqttManager_;     // Changed - always exists
```

**Step 2: Verify build**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | tail -10`

Expected: Build fails with missing includes in ProcessController.cpp

**Step 3: Commit**

```bash
git add include/clevercoffee/control/ProcessController.h
git commit -m "refactor: ProcessController accepts interface references"
```

---

## Task 6: Update ProcessController Implementation to Use Interfaces

**Files:**
- Modify: `src/control/ProcessController.cpp`

**Step 1: Update includes**

Update the includes at the top of the file (around lines 6-20):

Replace:
```cpp
#include "clevercoffee/hardware/HardwareManager.h"
```

With:
```cpp
#include "clevercoffee/state/IHardwareContext.h"
#include "clevercoffee/display/IDisplayManager.h"
#include "clevercoffee/network/IMQTTManager.h"
```

**Step 2: Update constructor signature**

Update the constructor signature (around line 24):

```cpp
ProcessController::ProcessController(const Config&                   config,
                                     CleverCoffee::SystemContext&    systemContext,
                                     CleverCoffee::IHardwareContext& hardwareManager,  // Changed
                                     IDisplayManager&                displayManager,   // Changed
                                     IMQTTManager&                   mqttManager)      // Changed
    : config_(config), systemContext_(systemContext), hardwareManager_(hardwareManager),
      displayManager_(displayManager), mqttManager_(mqttManager), pidController_(nullptr), temperature_(0.0),
      // ... rest unchanged ...
```

**Step 3: Verify build**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | tail -10`

Expected: Build succeeds (production code still passes concrete types to interface references)

**Step 4: Commit**

```bash
git add src/control/ProcessController.cpp
git commit -m "refactor: ProcessController implementation uses interfaces"
```

---

## Task 7: Create MockDisplayManager

**Files:**
- Create: `test/mocks/MockDisplayManager.h`

**Step 1: Create mock**

Create the file with this exact content:

```cpp
/**
 * @file MockDisplayManager.h
 * @brief Mock implementation of IDisplayManager for testing
 */

#pragma once

#include <gmock/gmock.h>
#include "clevercoffee/display/IDisplayManager.h"

// Forward declaration for U8G2 stub
class U8G2;

/**
 * @class MockDisplayManager
 * @brief Google Mock implementation of IDisplayManager interface
 *
 * Provides a test double for DisplayManager that can be used with
 * GoogleTest expectations and behavior specifications.
 */
class MockDisplayManager : public IDisplayManager {
  public:
    MockDisplayManager()          = default;
    ~MockDisplayManager() override = default;

    MOCK_METHOD(U8G2*, getDisplay, (), (const, noexcept, override));
    MOCK_METHOD(bool, isInitialized, (), (const, noexcept, override));
};

/**
 * @brief Create a MockDisplayManager with default behavior for common scenarios
 * @return Unique pointer to NiceMock<MockDisplayManager> with sensible defaults
 *
 * Default behavior:
 * - getDisplay() returns nullptr
 * - isInitialized() returns true
 */
inline std::unique_ptr<testing::NiceMock<MockDisplayManager>> createDefaultMockDisplayManager() {
    auto mock = std::make_unique<testing::NiceMock<MockDisplayManager>>();
    ON_CALL(*mock, getDisplay()).WillByDefault(testing::Return(nullptr));
    ON_CALL(*mock, isInitialized()).WillByDefault(testing::Return(true));
    return mock;
}
```

**Step 2: Verify test build**

Run: `~/.platformio/penv/bin/pio test -e native_test --without-testing -v 2>&1 | grep -A5 "MockDisplayManager"`

Expected: Mock compiles without errors

**Step 3: Commit**

```bash
git add test/mocks/MockDisplayManager.h
git commit -m "test: add MockDisplayManager for testing"
```

---

## Task 8: Create MockMQTTManager

**Files:**
- Create: `test/mocks/MockMQTTManager.h`

**Step 1: Create mock**

Create the file with this exact content:

```cpp
/**
 * @file MockMQTTManager.h
 * @brief Mock implementation of IMQTTManager for testing
 */

#pragma once

#include <gmock/gmock.h>
#include "clevercoffee/network/IMQTTManager.h"

/**
 * @class MockMQTTManager
 * @brief Google Mock implementation of IMQTTManager interface
 *
 * Provides a test double for MQTTManager that can be used with
 * GoogleTest expectations and behavior specifications.
 */
class MockMQTTManager : public IMQTTManager {
  public:
    MockMQTTManager()          = default;
    ~MockMQTTManager() override = default;

    MOCK_METHOD(bool, isEnabled, (), (const, noexcept, override));
    MOCK_METHOD(bool, isConnected, (), (const, noexcept, override));
    MOCK_METHOD(void, checkConnection, (), (override));
    MOCK_METHOD(void, loop, (), (override));
    MOCK_METHOD(int, writeSysParamsToMQTT, (bool), (override));
    MOCK_METHOD(int, sendHASSIODiscoveryMsg, (), (override));
    MOCK_METHOD(void, setUICoordinator, (CleverCoffee::UICoordinator*), (noexcept, override));
    MOCK_METHOD(void, setSensorCoordinator, (CleverCoffee::SensorCoordinator*), (noexcept, override));
    MOCK_METHOD(void, setNetworkCoordinator, (CleverCoffee::NetworkCoordinator*), (noexcept, override));
    MOCK_METHOD(void, setSystemContext, (CleverCoffee::SystemContext*), (noexcept, override));
    MOCK_METHOD(void, setUpdateRunning, (bool), (noexcept, override));
    MOCK_METHOD(bool, isUpdateRunning, (), (const, noexcept, override));
    MOCK_METHOD(bool, wasConnected, (), (const, noexcept, override));
    MOCK_METHOD(void, setWasConnected, (bool), (noexcept, override));
};

/**
 * @brief Create a MockMQTTManager with default behavior for common scenarios
 * @return Unique pointer to NiceMock<MockMQTTManager> with sensible defaults
 *
 * Default behavior:
 * - isEnabled() returns false
 * - isConnected() returns false
 * - isUpdateRunning() returns false
 * - wasConnected() returns false
 */
inline std::unique_ptr<testing::NiceMock<MockMQTTManager>> createDefaultMockMQTTManager() {
    auto mock = std::make_unique<testing::NiceMock<MockMQTTManager>>();
    ON_CALL(*mock, isEnabled()).WillByDefault(testing::Return(false));
    ON_CALL(*mock, isConnected()).WillByDefault(testing::Return(false));
    ON_CALL(*mock, isUpdateRunning()).WillByDefault(testing::Return(false));
    ON_CALL(*mock, wasConnected()).WillByDefault(testing::Return(false));
    return mock;
}
```

**Step 2: Verify test build**

Run: `~/.platformio/penv/bin/pio test -e native_test --without-testing -v 2>&1 | grep -A5 "MockMQTTManager"`

Expected: Mock compiles without errors

**Step 3: Commit**

```bash
git add test/mocks/MockMQTTManager.h
git commit -m "test: add MockMQTTManager for testing"
```

---

## Task 9: Update ProcessController Integration Tests

**Files:**
- Modify: `test/test_process_controller/test_main.cpp`

**Step 1: Replace test file with working integration tests**

Replace the entire file with this content (the original plan's test code that now should work):

```cpp
/**
 * @file test_main.cpp
 * @brief Integration tests for ProcessController
 *
 * Tests safety-critical behavior:
 * - Emergency stop on overtemperature
 * - PID output clamping to safe bounds
 * - Setpoint management (brew/steam switching)
 * - Initialization from config
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>
#include "../test_support.h"
#include "../mocks/MockHardwareManager.h"
#include "../mocks/MockDisplayManager.h"
#include "../mocks/MockMQTTManager.h"

// Include implementations (PlatformIO native tests don't link src/)
#include "../mocks/ConfigStubs.cpp"
#include "../../src/Logger.cpp"
#include "../../src/control/EmergencyStopManager.cpp"
#include "../../src/control/ProcessController.cpp"
#include "../../src/context/SystemContext.cpp"
#include "../../src/context/ProcessState.cpp"
#include "../../src/context/SensorState.cpp"
#include "../../src/context/TimingState.cpp"
#include "../../src/coordinators/SensorCoordinator.cpp"
#include "../../src/coordinators/NetworkCoordinator.cpp"
#include "../../src/coordinators/UICoordinator.cpp"
#include "../../src/coordinators/StandbyCoordinator.cpp"

using namespace CleverCoffee;
using ::testing::Return;
using ::testing::NiceMock;
using ::testing::_;

// ============================================================================
// TEST FIXTURE
// ============================================================================

class ProcessControllerIntegrationTest : public ::testing::Test {
protected:
    std::unique_ptr<SystemContext> systemContext_;
    std::unique_ptr<ProcessController> controller_;

    NiceMock<MockHardwareManager> mockHardwareManager_;
    NiceMock<MockDisplayManager> mockDisplayManager_;
    NiceMock<MockMQTTManager> mockMqttManager_;

    void SetUp() override {
        // Create real SystemContext
        systemContext_ = std::make_unique<SystemContext>();
        systemContext_->markReady();

        // Configure safe default temperature
        ON_CALL(mockHardwareManager_, getCurrentTemperature())
            .WillByDefault(Return(25.0));
        ON_CALL(mockHardwareManager_, hasTemperatureError())
            .WillByDefault(Return(false));

        // Configure default config values
        Config& config = Config::getInstance();
        config.brewSetpoint.set(93.0);
        config.steamSetpoint.set(130.0);
        config.pidRegularKp.set(50.0);
        config.pidRegularTn.set(100.0);
        config.pidRegularTv.set(15.0);
        config.emergencyStopTemp.set(145.0);

        // Create real ProcessController
        controller_ = std::make_unique<ProcessController>(
            config,
            *systemContext_,
            mockHardwareManager_,
            mockDisplayManager_,
            mockMqttManager_
        );
    }

    void TearDown() override {
        controller_.reset();
        systemContext_.reset();
    }
};

// ============================================================================
// INITIALIZATION TESTS
// ============================================================================

/**
 * TEST: ProcessController initializes successfully
 */
TEST_F(ProcessControllerIntegrationTest, InitializesSuccessfully) {
    bool result = controller_->initialize();
    EXPECT_TRUE(result) << "ProcessController should initialize successfully";
}

/**
 * TEST: ProcessController loads setpoint from config
 */
TEST_F(ProcessControllerIntegrationTest, LoadsSetpointFromConfig) {
    controller_->initialize();

    double setpoint = controller_->getSetpoint();
    EXPECT_GT(setpoint, 0.0) << "Setpoint should be loaded from config";
    EXPECT_DOUBLE_EQ(93.0, setpoint) << "Setpoint should match config value";
}

// ============================================================================
// SAFETY-CRITICAL TESTS
// ============================================================================

/**
 * TEST: Emergency stop triggers on overtemperature
 *
 * CRITICAL SAFETY TEST: Verifies the machine shuts down when temperature
 * exceeds the emergency threshold (default 145°C)
 */
TEST_F(ProcessControllerIntegrationTest, EmergencyStopOnOvertemperature) {
    controller_->initialize();

    Config& config = Config::getInstance();
    const double emergencyThreshold = config.emergencyStopTemp.get();
    const double dangerousTemp = emergencyThreshold + 5.0;  // 150°C

    // Simulate dangerous temperature - need 3 consecutive readings for debounce
    ON_CALL(mockHardwareManager_, getCurrentTemperature())
        .WillByDefault(Return(dangerousTemp));

    // First two calls build up debounce counter
    controller_->testEmergencyConditions();
    controller_->testEmergencyConditions();

    // Third call should trigger emergency
    bool emergencyTriggered = controller_->testEmergencyConditions();

    EXPECT_TRUE(emergencyTriggered)
        << "Emergency should trigger after 3 consecutive overtemperature readings";
}

/**
 * TEST: PID output is clamped to safe bounds
 *
 * SAFETY TEST: Verifies PID output stays within 0-1000 range
 * regardless of input conditions
 */
TEST_F(ProcessControllerIntegrationTest, PIDOutputClampedToSafeBounds) {
    controller_->initialize();
    controller_->setPIDEnabled(true);

    // Set a large temperature error to push PID to extremes
    ON_CALL(mockHardwareManager_, getCurrentTemperature())
        .WillByDefault(Return(20.0));  // Far below setpoint

    controller_->updateTemperature();
    controller_->computePID();

    double output = controller_->getPIDOutput();
    EXPECT_GE(output, 0.0) << "PID output should not be negative";
    EXPECT_LE(output, 1000.0) << "PID output should not exceed 1000";
}

/**
 * TEST: PID output clamped when temperature above setpoint
 */
TEST_F(ProcessControllerIntegrationTest, PIDOutputClampedWhenHot) {
    controller_->initialize();
    controller_->setPIDEnabled(true);

    // Temperature well above setpoint
    ON_CALL(mockHardwareManager_, getCurrentTemperature())
        .WillByDefault(Return(100.0));  // Above 93°C setpoint

    controller_->updateTemperature();
    controller_->computePID();

    double output = controller_->getPIDOutput();
    EXPECT_GE(output, 0.0) << "PID output should not be negative";
    EXPECT_LE(output, 1000.0) << "PID output should not exceed 1000";
}

// ============================================================================
// SETPOINT MANAGEMENT TESTS
// ============================================================================

/**
 * TEST: Setpoint switches to steam temperature when steam active
 */
TEST_F(ProcessControllerIntegrationTest, SwitchesToSteamSetpoint) {
    controller_->initialize();

    double brewSetpoint = controller_->getSetpoint();
    EXPECT_DOUBLE_EQ(93.0, brewSetpoint);

    // Activate steam mode
    controller_->updateSetpoint(true);  // steamActive = true

    double steamSetpoint = controller_->getSetpoint();
    EXPECT_GT(steamSetpoint, brewSetpoint)
        << "Steam setpoint should be higher than brew setpoint";
    EXPECT_DOUBLE_EQ(130.0, steamSetpoint);
}

/**
 * TEST: Setpoint returns to brew temperature when steam deactivated
 */
TEST_F(ProcessControllerIntegrationTest, ReturnsToBrewSetpoint) {
    controller_->initialize();

    // Switch to steam
    controller_->updateSetpoint(true);
    EXPECT_DOUBLE_EQ(130.0, controller_->getSetpoint());

    // Switch back to brew
    controller_->updateSetpoint(false);
    EXPECT_DOUBLE_EQ(93.0, controller_->getSetpoint())
        << "Should return to brew setpoint when steam deactivated";
}

// ============================================================================
// PID STATE MANAGEMENT TESTS
// ============================================================================

/**
 * TEST: PID can be enabled and disabled
 */
TEST_F(ProcessControllerIntegrationTest, PIDCanBeEnabledAndDisabled) {
    controller_->initialize();

    controller_->setPIDEnabled(true);
    EXPECT_TRUE(controller_->isPIDEnabled());

    controller_->setPIDEnabled(false);
    EXPECT_FALSE(controller_->isPIDEnabled());
}

/**
 * TEST: PID should be enabled for normal brewing state
 */
TEST_F(ProcessControllerIntegrationTest, PIDEnabledForNormalState) {
    controller_->initialize();

    bool shouldEnable = controller_->shouldPIDBeEnabled(MachineStateId::PID_NORMAL);
    EXPECT_TRUE(shouldEnable) << "PID should be enabled in PID_NORMAL state";
}

/**
 * TEST: PID should be disabled for init state
 */
TEST_F(ProcessControllerIntegrationTest, PIDDisabledForInitState) {
    controller_->initialize();

    bool shouldEnable = controller_->shouldPIDBeEnabled(MachineStateId::INIT);
    EXPECT_FALSE(shouldEnable) << "PID should be disabled in INIT state";
}

// Note: main() is provided by test/main.cpp
```

**Step 2: Run tests**

Run: `~/.platformio/penv/bin/pio test -e native_test -f test_process_controller -v 2>&1`

Expected: All tests compile, may need stub additions

**Step 3: Fix any missing stubs/includes**

If compilation fails, add necessary stubs as minimal as possible.

**Step 4: Verify all tests pass**

Run: `~/.platformio/penv/bin/pio test -e native_test -f test_process_controller -v 2>&1 | tail -30`

Expected: All 10 tests PASS

**Step 5: Commit**

```bash
git add test/test_process_controller/test_main.cpp
git commit -m "test: implement ProcessController integration tests with real instantiation"
```

---

## Task 10: Run Full Test Suite Verification

**Step 1: Run all tests**

Run: `~/.platformio/penv/bin/pio test -e native_test 2>&1 | tail -30`

Expected: All tests pass, total count increased by 10

**Step 2: Verify production build**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | tail -5`

Expected: Build succeeds

**Step 3: Format code**

Run: `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s`

**Step 4: Final commit if formatting changes**

```bash
git status --short
# If changes:
git add -A && git commit -m "chore: format code"
```

---

## Summary

**Files Created:**
- `include/clevercoffee/display/IDisplayManager.h` - Display interface
- `include/clevercoffee/network/IMQTTManager.h` - MQTT interface
- `test/mocks/MockDisplayManager.h` - Display mock
- `test/mocks/MockMQTTManager.h` - MQTT mock

**Files Modified:**
- `include/clevercoffee/display/DisplayManager.h` - Implements IDisplayManager
- `include/clevercoffee/network/MQTTManager.h` - Implements IMQTTManager
- `include/clevercoffee/control/ProcessController.h` - Uses interfaces
- `src/control/ProcessController.cpp` - Uses interfaces
- `test/test_process_controller/test_main.cpp` - Real ProcessController tests

**Tests Added:**
1. `InitializesSuccessfully` - Basic initialization
2. `LoadsSetpointFromConfig` - Config integration
3. `EmergencyStopOnOvertemperature` - **SAFETY CRITICAL**
4. `PIDOutputClampedToSafeBounds` - **SAFETY CRITICAL**
5. `PIDOutputClampedWhenHot` - **SAFETY CRITICAL**
6. `SwitchesToSteamSetpoint` - Setpoint management
7. `ReturnsToBrewSetpoint` - Setpoint management
8. `PIDCanBeEnabledAndDisabled` - PID state
9. `PIDEnabledForNormalState` - State-based PID
10. `PIDDisabledForInitState` - State-based PID

**Architecture Benefits:**
- Consistent interface pattern across all managers
- ProcessController fully testable with mocks
- Safety-critical behavior verified through integration tests
- No changes required to production instantiation code
