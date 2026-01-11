# Testing Guide
## CleverCoffee ESP32 Coffee Machine Controller

**Purpose:** Guide for writing and running unit tests for the CleverCoffee codebase.

---

## Test Infrastructure

### Available Mocks

The following mock implementations are available in `test/mocks/`:

1. **MockISensor** (`MockISensor.h`)
   - Implements `ISensor` interface
   - Supports async read pattern
   - Can simulate connection failures
   - Usage:
     ```cpp
     MockISensor mockTemp(95.5);
     SensorCoordinator coord(&mockTemp, nullptr, nullptr);
     ```

2. **MockConfig** (`MockConfig.h`)
   - Provides test configuration values
   - Can be used where Config dependency injection is possible
   - Note: Most code uses `Config::getInstance()` directly
   - Usage:
     ```cpp
     MockConfig mockConfig;
     mockConfig.setPidEnabled(true);
     mockConfig.setBrewSetpoint(95.0);
     ```

3. **MockRelay** (`MockRelay.h`)
   - GMock-based mock for Relay interface
   - Supports EXPECT_CALL() for verification
   - Usage:
     ```cpp
     MockRelay mockRelay(gpioPin);
     EXPECT_CALL(mockRelay, on()).Times(1);
     ```

4. **MockSwitch** (`MockSwitch.h`)
   - GMock-based mock for Switch interface
   - Usage:
     ```cpp
     MockSwitch mockSwitch(Hardware::SwitchType::MOMENTARY, Hardware::SwitchMode::NORMALLY_OPEN);
     EXPECT_CALL(mockSwitch, isPressed()).WillOnce(::testing::Return(true));
     ```

5. **MockLED** (`MockLED.h`)
   - GMock-based mock for LED interface
   - Usage:
     ```cpp
     MockLED mockLED;
     EXPECT_CALL(mockLED, turnOn()).Times(1);
     ```

6. **MockSensorManager** (`MockSensorManager.h`)
   - Legacy mock for sensor manager
   - Consider using MockISensor with SensorCoordinator instead

### Test Helpers

Located in `test/test_utils/TestHelpers.h`:

- **TestFixtureBase**: Base test fixture with SystemContext
- **TestConfigBuilder**: Fluent builder for test configuration
- **createTestSystemContext()**: Helper to create test SystemContext

Usage:
```cpp
class MyTest : public TestFixtureBase {
    void SetUp() override {
        TestFixtureBase::SetUp();
        // Additional setup
    }
};
```

---

## Testing Patterns

### Testing Components with Config Singleton

Many components use `Config::getInstance()` directly. To test these:

**Option 1: Test with Real Config (Recommended for Integration Tests)**
```cpp
TEST(MyTest, TestWithRealConfig) {
    // Initialize real Config
    Config::getInstance().begin();
    Config::getInstance().pidEnabled.set(true);
    
    // Test component
    MyComponent component;
    // ... test ...
    
    // Cleanup
    Config::getInstance().resetAllToDefaults();
}
```

**Option 2: Refactor for Dependency Injection (For New Code)**
```cpp
// Instead of:
void myFunction() {
    bool enabled = Config::getInstance().pidEnabled.get();
}

// Use:
void myFunction(const Config& config) {
    bool enabled = config.pidEnabled.get();
}

// Then test with:
TEST(MyTest, TestWithMockConfig) {
    MockConfig mockConfig;
    mockConfig.setPidEnabled(true);
    myFunction(mockConfig);
}
```

### Testing State Machine

State machine tests should use:
- Mock hardware (relays, switches, sensors)
- Test SystemContext
- State transition verification

Example:
```cpp
TEST(StateMachineTest, TransitionsToBrewState) {
    auto systemContext = createTestSystemContext();
    MockSwitch brewSwitch(Hardware::SwitchType::MOMENTARY, ...);
    
    EXPECT_CALL(brewSwitch, isPressed()).WillOnce(Return(true));
    
    // Test state transition
    // ...
}
```

### Testing Coordinators

Coordinators are designed to be testable:
- They accept dependencies via constructor
- They use dependency injection
- They can be tested in isolation

Example:
```cpp
TEST(SensorCoordinatorTest, ReadsTemperature) {
    MockISensor mockTemp(95.5);
    SensorCoordinator coord(&mockTemp, nullptr, nullptr);
    
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    EXPECT_DOUBLE_EQ(95.5, coord.getTemperature());
}
```

---

## Running Tests

### Prerequisites
- Google Test (GTest) framework
- PlatformIO test environment configured

### Running All Tests
```bash
pio test
```

### Running Specific Test Suite
```bash
pio test -f test_system_context
```

### Running with Verbose Output
```bash
pio test --verbose
```

---

## Test Coverage Goals

### Current Coverage
- ✅ SystemContext: 20 tests
- ✅ SensorCoordinator: 5 tests
- ✅ State Machine: Basic tests
- ⚠️ ProcessController: Test structure created
- ⚠️ LoopManager: Test structure created

### Target Coverage
- **Core Components:** 80%+ coverage
  - SystemContext
  - ProcessController
  - LoopManager
  - State Machine
  - Coordinators

- **Handlers:** 70%+ coverage
  - BrewHandler
  - SteamHandler
  - PowerHandler
  - HotWaterHandler

- **Hardware:** 60%+ coverage
  - HardwareManager
  - Sensor implementations

---

## Best Practices

### 1. Use Dependency Injection
Prefer constructor injection over singletons:
```cpp
// Good
class MyComponent {
    MyComponent(const Config& config) : config_(config) {}
};

// Avoid
class MyComponent {
    void doSomething() {
        Config::getInstance().pidEnabled.get(); // Hard to test
    }
};
```

### 2. Mock External Dependencies
Mock hardware, network, and storage:
- Use MockISensor for sensors
- Use MockRelay for relays
- Use MockSwitch for switches
- Use MockConfig where possible

### 3. Test State Transitions
Verify state machine behavior:
- State entry/exit
- Transition conditions
- Error handling

### 4. Test Error Cases
Don't just test happy paths:
- Null pointer handling
- Invalid input
- Hardware failures
- Network errors

### 5. Keep Tests Fast
- Avoid real hardware delays
- Use mocks instead of real sensors
- Minimize sleep/wait calls

---

## Known Limitations

### Config Singleton
- `Config::getInstance()` is used extensively
- Hard to mock without refactoring
- **Workaround:** Test with real Config instance, reset between tests

### Hardware Dependencies
- Some components require actual hardware initialization
- **Workaround:** Use mocks or test in integration test environment

### ISR Code
- Interrupt service routines are hard to test
- **Workaround:** Test ISR logic separately, use controlled accessors

---

## Future Improvements

1. **IConfig Interface**
   - Create IConfig interface
   - Make Config implement it
   - Allow dependency injection

2. **More Comprehensive Mocks**
   - Mock HardwareManager
   - Mock DisplayManager
   - Mock MQTTManager

3. **Test Fixtures**
   - Standard test fixtures for common scenarios
   - Test data builders

4. **Integration Tests**
   - Full system integration tests
   - Hardware-in-the-loop tests

---

## Examples

See existing tests for examples:
- `test/test_system_context/test_main.cpp` - SystemContext tests
- `test/test_sensor_coordinator/test_main.cpp` - SensorCoordinator tests
- `test/test_state_machine/test_main.cpp` - State machine tests

---

**Last Updated:** Current Session  
**Status:** Active development
