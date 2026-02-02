# ProcessController Interface Refactoring Design

**Date:** 2026-02-02
**Status:** Approved
**Goal:** Refactor ProcessController to accept interfaces instead of concrete types, enabling integration testing with mocks

## Problem Statement

ProcessController integration tests cannot be implemented because ProcessController's constructor requires concrete types:
- `HardwareManager&` (concrete class)
- `DisplayManager&` (concrete class)
- `MQTTManager&` (concrete class)

Test mocks (MockHardwareManager, MockDisplayManager, MockMQTTManager) cannot be passed to ProcessController because there's no type conversion from mocks to concrete types.

## Solution: Interface Extraction

Extract interfaces for all three dependencies and update ProcessController to accept interface references instead of concrete class references. This follows the pattern already established with `IWiFiManager` and `IHardwareContext`.

---

## Design Section 1: Interface Hierarchy

**IHardwareContext** (already exists)
- Used by: HardwareManager
- Methods needed by ProcessController:
  - `safeShutdown()`
  - `getHeaterRelay()` → returns Relay pointer

**IDisplayManager** (new)
- Used by: DisplayManager
- Methods to extract:
  - `getDisplay()` → returns U8G2 pointer
  - `isInitialized()` → returns bool
- Minimal interface since ProcessController doesn't currently use it
- Future-proof for when ProcessController might need display access

**IMQTTManager** (new)
- Used by: MQTTManager
- Methods to extract:
  - Core methods: `isEnabled()`, `isConnected()`, `checkConnection()`, `loop()`
  - Publishing: `writeSysParamsToMQTT()`, `sendHASSIODiscoveryMsg()`
  - Coordinators: `setUICoordinator()`, `setSensorCoordinator()`, `setNetworkCoordinator()`, `setSystemContext()`
- Comprehensive interface since MQTTManager has many public methods

**ProcessController changes:**
```cpp
ProcessController(const Config&         config,
                  SystemContext&        systemContext,
                  IHardwareContext&     hardwareManager,  // Changed
                  IDisplayManager&      displayManager,   // Changed
                  IMQTTManager&         mqttManager);     // Changed
```

---

## Design Section 2: Interface Implementations

**IDisplayManager Interface:**
```cpp
// include/clevercoffee/display/IDisplayManager.h
#pragma once
#include <U8g2lib.h>

class IDisplayManager {
public:
    virtual ~IDisplayManager() = default;

    virtual U8G2* getDisplay() const noexcept = 0;
    virtual bool isInitialized() const noexcept = 0;
};
```

**DisplayManager changes:**
```cpp
// include/clevercoffee/display/DisplayManager.h
#include "clevercoffee/display/IDisplayManager.h"

class DisplayManager : public IDisplayManager {
public:
    // Add override specifiers to existing methods
    U8G2* getDisplay() const noexcept override;
    bool isInitialized() const noexcept override;

    // Rest unchanged
};
```

**IMQTTManager Interface:**
```cpp
// include/clevercoffee/network/IMQTTManager.h
#pragma once
#include <functional>

// Forward declarations
namespace CleverCoffee {
class UICoordinator;
class SensorCoordinator;
class NetworkCoordinator;
class SystemContext;
}

class IMQTTManager {
public:
    virtual ~IMQTTManager() = default;

    // Connection management
    virtual bool isEnabled() const noexcept = 0;
    virtual bool isConnected() const noexcept = 0;
    virtual void checkConnection() = 0;
    virtual void loop() = 0;

    // Publishing
    virtual int writeSysParamsToMQTT(bool continueOnError = true) = 0;
    virtual int sendHASSIODiscoveryMsg() = 0;

    // Coordinator setters
    virtual void setUICoordinator(CleverCoffee::UICoordinator* coordinator) noexcept = 0;
    virtual void setSensorCoordinator(CleverCoffee::SensorCoordinator* coordinator) noexcept = 0;
    virtual void setNetworkCoordinator(CleverCoffee::NetworkCoordinator* coordinator) noexcept = 0;
    virtual void setSystemContext(CleverCoffee::SystemContext* context) noexcept = 0;

    // Update management
    virtual void setUpdateRunning(bool running) noexcept = 0;
    virtual bool isUpdateRunning() const noexcept = 0;
    virtual bool wasConnected() const noexcept = 0;
    virtual void setWasConnected(bool connected) noexcept = 0;
};
```

**MQTTManager changes:**
```cpp
// include/clevercoffee/network/MQTTManager.h
#include "clevercoffee/network/IMQTTManager.h"

class MQTTManager : public IMQTTManager {
public:
    // Add override specifiers to all interface methods
    bool isEnabled() const noexcept override;
    bool isConnected() const noexcept override;
    // ... etc for all IMQTTManager methods

    // Keep existing methods that aren't in interface
};
```

---

## Design Section 3: Consumer Changes

**ProcessController.h changes:**
```cpp
// Forward declarations
class IDisplayManager;
class IMQTTManager;

namespace CleverCoffee {
class IHardwareContext;  // Changed from HardwareManager
class SystemContext;
}

class ProcessController {
public:
    ProcessController(const Config&                config,
                      CleverCoffee::SystemContext& systemContext,
                      CleverCoffee::IHardwareContext& hardwareManager,  // Changed
                      IDisplayManager&             displayManager,      // Changed
                      IMQTTManager&                mqttManager);        // Changed

private:
    // Member variables
    CleverCoffee::IHardwareContext& hardwareManager_;  // Changed
    IDisplayManager&                displayManager_;   // Changed
    IMQTTManager&                   mqttManager_;      // Changed
};
```

**ProcessController.cpp changes:**
```cpp
#include "clevercoffee/state/IHardwareContext.h"  // Changed
#include "clevercoffee/display/IDisplayManager.h"  // Added
#include "clevercoffee/network/IMQTTManager.h"     // Added

ProcessController::ProcessController(const Config&                   config,
                                     CleverCoffee::SystemContext&    systemContext,
                                     CleverCoffee::IHardwareContext& hardwareManager,  // Changed
                                     IDisplayManager&                displayManager,   // Changed
                                     IMQTTManager&                   mqttManager)      // Changed
    : config_(config), systemContext_(systemContext),
      hardwareManager_(hardwareManager),  // Works with interface
      displayManager_(displayManager),     // Works with interface
      mqttManager_(mqttManager)            // Works with interface
```

**Other consumers** (main.cpp, state files, etc.):
- Production code continues passing concrete types (HardwareManager, DisplayManager, MQTTManager)
- C++ allows passing derived classes to base class references
- No changes needed in production instantiation code

**Test code:**
- Can now pass MockHardwareManager (implements IHardwareContext)
- Can now pass MockDisplayManager (implements IDisplayManager)
- Can now pass MockMQTTManager (implements IMQTTManager)

---

## Design Section 4: Mock Implementations

**MockDisplayManager** (new):
```cpp
// test/mocks/MockDisplayManager.h
#pragma once
#include <gmock/gmock.h>
#include "clevercoffee/display/IDisplayManager.h"
#include "U8g2lib.h"

class MockDisplayManager : public IDisplayManager {
public:
    MockDisplayManager() = default;
    ~MockDisplayManager() override = default;

    MOCK_METHOD(U8G2*, getDisplay, (), (const, noexcept, override));
    MOCK_METHOD(bool, isInitialized, (), (const, noexcept, override));
};

// Helper function for common test setup
inline std::unique_ptr<testing::NiceMock<MockDisplayManager>> createDefaultMockDisplayManager() {
    auto mock = std::make_unique<testing::NiceMock<MockDisplayManager>>();
    ON_CALL(*mock, getDisplay()).WillByDefault(testing::Return(nullptr));
    ON_CALL(*mock, isInitialized()).WillByDefault(testing::Return(true));
    return mock;
}
```

**MockMQTTManager** (new):
```cpp
// test/mocks/MockMQTTManager.h
#pragma once
#include <gmock/gmock.h>
#include "clevercoffee/network/IMQTTManager.h"

class MockMQTTManager : public IMQTTManager {
public:
    MockMQTTManager() = default;
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

// Helper function for common test setup
inline std::unique_ptr<testing::NiceMock<MockMQTTManager>> createDefaultMockMQTTManager() {
    auto mock = std::make_unique<testing::NiceMock<MockMQTTManager>>();
    ON_CALL(*mock, isEnabled()).WillByDefault(testing::Return(false));
    ON_CALL(*mock, isConnected()).WillByDefault(testing::Return(false));
    ON_CALL(*mock, isUpdateRunning()).WillByDefault(testing::Return(false));
    ON_CALL(*mock, wasConnected()).WillByDefault(testing::Return(false));
    return mock;
}
```

**MockHardwareManager** (update existing):
```cpp
// test/mocks/MockHardwareManager.h - already exists, just verify it implements IHardwareContext
// Should already be good since it was created following this pattern
```

---

## Design Section 5: Implementation Phases

**Phase 1: Create Interfaces**
1. Create `include/clevercoffee/display/IDisplayManager.h`
2. Create `include/clevercoffee/network/IMQTTManager.h`
3. Verify build: `~/.platformio/penv/bin/pio run -e esp32_usb -s`

**Phase 2: Update Production Classes**
1. Update `DisplayManager.h` to inherit from `IDisplayManager`
2. Add `override` specifiers to DisplayManager methods
3. Update `MQTTManager.h` to inherit from `IMQTTManager`
4. Add `override` specifiers to MQTTManager methods
5. Verify build: `~/.platformio/penv/bin/pio run -e esp32_usb -s`

**Phase 3: Update ProcessController**
1. Update `ProcessController.h` constructor signature and forward declarations
2. Update `ProcessController.h` member variable types
3. Update `ProcessController.cpp` constructor signature and includes
4. Verify build: `~/.platformio/penv/bin/pio run -e esp32_usb -s`

**Phase 4: Create Mocks**
1. Create `test/mocks/MockDisplayManager.h`
2. Create `test/mocks/MockMQTTManager.h`
3. Update existing MockHardwareManager if needed
4. Verify test build: `~/.platformio/penv/bin/pio test -e native_test --without-testing -v`

**Phase 5: Update ProcessController Integration Tests**
1. Update `test/test_process_controller/test_main.cpp` to use all three mocks
2. Verify the 10 integration tests compile and run
3. Fix any test failures
4. Verify all tests pass: `~/.platformio/penv/bin/pio test -e native_test -f test_process_controller -v`

**Phase 6: Verification**
1. Run full test suite: `~/.platformio/penv/bin/pio test -e native_test`
2. Verify production build: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
3. Format code: `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s`
4. Commit all changes

**Testing Strategy:**
- Build verification after each phase prevents late-stage breakage
- Test-only build before full test run catches mock issues early
- Integration tests validate the refactoring enables actual testing

---

## Files Changed

| File | Change |
|------|--------|
| `include/clevercoffee/display/IDisplayManager.h` | NEW |
| `include/clevercoffee/network/IMQTTManager.h` | NEW |
| `include/clevercoffee/display/DisplayManager.h` | Add `: public IDisplayManager`, add overrides |
| `include/clevercoffee/network/MQTTManager.h` | Add `: public IMQTTManager`, add overrides |
| `include/clevercoffee/control/ProcessController.h` | Change param types to interfaces |
| `src/control/ProcessController.cpp` | Change param types, update includes |
| `test/mocks/MockDisplayManager.h` | NEW |
| `test/mocks/MockMQTTManager.h` | NEW |
| `test/test_process_controller/test_main.cpp` | Update to use real ProcessController with mocks |

---

## Expected Outcomes

### Immediate Benefits
- ProcessController can be instantiated in tests with mocks
- All 10 integration tests from original plan can be implemented
- Safety-critical PID and emergency stop behavior can be verified

### Architectural Benefits
- Consistent interface pattern across all managers (WiFi, Hardware, Display, MQTT)
- Better dependency inversion principle adherence
- Future managers will follow this pattern

### Test Coverage
After implementation, we can test:
- ProcessController initialization
- PID output clamping (safety-critical)
- Emergency stop on overtemperature (safety-critical)
- Setpoint switching (brew/steam)
- PID state management

---

## Design Rationale

**Why Full Interface Extraction vs. Minimal Change?**
- Consistency: All managers use interfaces
- Future-proofing: Pattern established for when ProcessController needs DisplayManager/MQTTManager
- Clean architecture: No mixing of interface and concrete dependencies
- Maintainability: Clear pattern for future developers

**Why Not Remove Unused Dependencies?**
- DisplayManager and MQTTManager might be used in future ProcessController features
- Breaking change would require updating all production code instantiations
- Keeping them with interfaces is more flexible
