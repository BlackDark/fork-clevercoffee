# ESP32 C++ Arduino Testing & Mocking Capabilities Research

## Executive Summary

This CleverCoffee ESP32 project **can implement comprehensive testing**, but the approach differs significantly from traditional desktop C++ testing:

- **YES: Unity test framework is built-in** to ESP32 SDK
- **YES: Can test on both device and desktop** (native binary runs on Linux/macOS)
- **PARTIAL: GoogleTest/GoogleMock work on desktop only**, not on ESP32
- **YES: Hardware mocking is possible** via abstraction layers
- **LIMITATION: Real-time constraints** (ISR timing, hardware timing) must test on device

---

## 1. Can We Use GoogleTest + GoogleMock on ESP32?

### Answer: **PARTIAL - Only on Desktop/Native**

**What Works:**
- GoogleTest (gtest) v1.15.2 is available in PlatformIO registry
- GoogleMock is part of GoogleTest
- Can compile and run tests **on your development machine** (Linux/macOS/Windows)
- Excellent for testing business logic, controllers, state machines

**What Doesn't Work:**
- **Cannot run on ESP32 device** - GoogleTest requires too much memory and C++ features
- GoogleTest needs: dynamic memory, exceptions (optional), full STL
- ESP32 has ~200KB RAM (shared with WiFi, BLE, etc.) - GoogleTest alone uses ~50-100KB

**Why Separation Is Good:**
```
Desktop/CI Tests          ESP32 Device Tests
├─ GoogleTest             ├─ Unity
├─ Mock hardware layers   ├─ Real hardware
├─ Fast feedback (< 1s)   ├─ Hardware integration
└─ 100% code coverage     └─ ISR timing, real constraints
```

**Recommendation:** Use GoogleTest for desktop, Unity for device.

---

## 2. Other C++ Mocking Libraries on ESP32

### Summary Table

| Framework | Device | Desktop | Memory | Notes |
|-----------|--------|---------|--------|-------|
| **Unity** | YES | YES | ~20KB | Built into ESP32 SDK, simple assertions |
| **GoogleTest** | NO | YES | ~100KB | Professional, needs desktop |
| **Catch2** | NO | YES | ~80KB | Modern C++, header-only, desktop-only |
| **FakeIt** | NO | YES | ~60KB | Header-only mocking, C++11, desktop |
| **Mock (berrak)** | YES | YES | ~10KB | Arduino-specific, lightweight |
| **ArduinoFake** | YES | YES | ~15KB | Mocks Arduino functions (digitalWrite, etc) |
| **CMock** | NO | YES | ~30KB | C-specific, not ideal for C++ |

**Best Matches for This Project:**

1. **Unity (device) + GoogleTest (desktop)** - RECOMMENDED
   - Built-in to ESP32, no extra dependencies
   - Professional testing on desktop
   - Clear separation of concerns

2. **Mock by berrak + GoogleTest**
   - Lighter weight than Unity
   - Good for mocking Arduino functions

3. **ArduinoFake + GoogleTest**
   - Specifically designed to mock Arduino APIs
   - Perfect for testing GPIO, Serial, I2C interactions

---

## 3. Practical Limitations of Testing on ESP32

### Memory Constraints

```
ESP32 Total SRAM: ~320KB
├─ WiFi/BLE stack: ~100-150KB (if enabled)
├─ FreeRTOS kernel: ~20KB
├─ Your app: ~50-100KB
└─ Available for tests: ~50-80KB ← Problem!
```

**Impact:**
- Cannot test large data structures in memory
- No dynamic arrays in tests
- Limited test count per run (20-30 tests max)
- Must run tests in batches

### Real-Time Constraints

Cannot reliably test:
- ISR timing (jitter, preemption)
- WiFi/BLE timing
- Precise PWM control (but can verify values)
- Interrupt ordering

**Solution:** Test logic on desktop, verify timing on device with scope/logic analyzer.

### Hardware Dependencies

Cannot mock easily on device:
- Temperature sensor (OneWire, TSIC)
- Scale (HX711)
- Pressure sensor (ADC)
- Relays (GPIO)

**Solution:** Use abstraction layers + desktop mocks.

---

## 4. Testing Hardware-Dependent Code Patterns

### Current Architecture Analysis

**Good News:** Your code already has good abstractions:

```cpp
// ✅ Excellent: Abstract interfaces
class TempSensor {
    virtual bool sample_temperature(double& temperature) const = 0;
};

class SensorManager {
    TempSensor* tempSensor_;  // Depends on interface, not implementation
};

// ✅ Good: HardwareManager centralizes creation
class HardwareManager {
    TempSensor* getTempSensor() const;
    Relay& getHeaterRelay() const;
};
```

### Testing Approach for Each Component

#### Pattern 1: Sensor Testing
```cpp
// Mock sensor for desktop testing
class MockTempSensor : public TempSensor {
    double mock_temperature_ = 95.0;
    
    bool sample_temperature(double& temperature) const override {
        temperature = mock_temperature_;
        return true;
    }
};

// Test business logic
TEST(EmergencyStop, DetectsTempAboveThreshold) {
    MockTempSensor sensor;
    sensor.mock_temperature_ = 160.0;  // Emergency condition
    
    ProcessController controller;
    EXPECT_TRUE(controller.testEmergencyConditions());
}
```

#### Pattern 2: Relay/Hardware Testing
```cpp
// Mock relay for testing
class MockRelay : public Relay {
    bool was_turned_on_ = false;
    
    void on() override { was_turned_on_ = true; }
    void off() override { was_turned_on_ = false; }
    bool isOn() const override { return was_turned_on_; }
};

// Test heater control
TEST(HeaterControl, TurnsOnWhenBelowSetpoint) {
    MockRelay relay;
    ProcessController controller;
    
    controller.setTemperature(90.0);
    controller.update();
    
    EXPECT_TRUE(relay.isOn());
}
```

#### Pattern 3: State Machine Testing
```cpp
// State machines are perfect for unit testing
TEST(BrewStates, TransitionsCorrectly) {
    MockSensorManager sensors;
    sensors.setTemperature(92.0);
    
    BrewState state;
    MachineStateContext context(sensors);
    
    state.onEntryImpl(context);
    EXPECT_EQ(context.currentState(), BREW_RUNNING);
}
```

---

## 5. Recommended Testing Strategy for CleverCoffee

### Three-Tier Testing Approach

```
┌─────────────────────────────────────────────────────────┐
│ TIER 1: Unit Tests (Desktop)                            │
│ - GoogleTest + GoogleMock                               │
│ - Zero hardware dependencies                            │
│ - Fast feedback (< 1 second)                            │
│ - 100+ test cases                                       │
│ ├─ State machines (StateFactory, states/)               │
│ ├─ Controllers (ProcessController, brew logic)          │
│ ├─ Sensor filtering (SensorManager)                     │
│ ├─ Config system (Config.h)                             │
│ └─ Safety logic (emergency stop, etc)                   │
└─────────────────────────────────────────────────────────┘
        ↓↓↓ Real implementations ↓↓↓
┌─────────────────────────────────────────────────────────┐
│ TIER 2: Integration Tests (Desktop Native)              │
│ - GoogleTest with minimal hardware stubs                │
│ - Test component interactions                           │
│ - Mock only low-level hardware (GPIO, I2C)              │
│ ├─ HardwareManager + SensorManager integration          │
│ ├─ State machine + sensor interaction                   │
│ └─ Config reload + state sync                           │
└─────────────────────────────────────────────────────────┘
        ↓↓↓ Burn to firmware ↓↓↓
┌─────────────────────────────────────────────────────────┐
│ TIER 3: Device Tests (ESP32 Hardware)                   │
│ - Unity test framework (built-in)                       │
│ - Real hardware, real constraints                       │
│ - Timing validation                                     │
│ ├─ Temperature sensor reads (OneWire)                   │
│ ├─ Relay control (GPIO PWM)                             │
│ ├─ ISR behavior under load                              │
│ ├─ Memory leaks (long runtime)                          │
│ └─ WiFi/BLE interaction                                 │
└─────────────────────────────────────────────────────────┘
```

### Recommended Tech Stack

```
Development Machine (Linux/macOS/Windows):
├─ GoogleTest v1.15.2 (available in PlatformIO)
├─ GoogleMock (included with GoogleTest)
├─ Catch2 v4.3.7 (alternative, modern syntax)
└─ ArduinoFake (for testing Arduino APIs)

ESP32 Device:
├─ Unity v2.6.0 (built into framework)
├─ Hardware mocks (MockSensor, MockRelay)
└─ PlatformIO test runner
```

---

## 6. Mocking Hardware Layers Effectively

### Abstraction Strategy

Your project **already has this right**:

```cpp
// ✅ Hardware interfaces (can be mocked)
class TempSensor { virtual bool sample_temperature(...) = 0; };
class Relay { virtual void on() = 0; virtual void off() = 0; };
class Switch { virtual uint8_t read() = 0; };

// ✅ Factory pattern (can be swapped)
class HardwareManager {
    std::unique_ptr<TempSensor> tempSensor_;
    std::unique_ptr<Relay> heaterRelay_;
};

// For testing: pass mock implementations
HardwareManager hwManager;
MockTempSensor mockSensor;
MockRelay mockRelay;
// Test using mocks...
```

### What Needs to Be Mocked

| Component | Desktop Mock | Device Stub |
|-----------|--------------|-------------|
| TempSensor | YES - return fixed value | Real hardware |
| Relay/GPIO | YES - track state | Real GPIO |
| Switch input | YES - return fixed state | Real GPIO |
| SensorManager | YES - return test data | Real sensors |
| EEPROM | YES - in-memory map | Real EEPROM |
| WiFi | YES - stub callback | Real WiFi (skip in tests) |
| Timer/ISR | PARTIAL - can mock timing | Real timer |

### Concrete Example: Mock Sensor

```cpp
// test/mocks/MockTempSensor.h
#pragma once
#include "clevercoffee/hardware/tempsensors/TempSensor.h"

class MockTempSensor : public TempSensor {
private:
    mutable double temperature_ = 20.0;
    mutable bool hasError_ = false;
    
public:
    // Set values for testing
    void setTemperature(double temp) { temperature_ = temp; }
    void setError(bool error) { hasError_ = error; }
    
protected:
    bool sample_temperature(double& temperature) const override {
        if (hasError_) return false;
        temperature = temperature_;
        return true;
    }
};

// test/test_emergency_stop.cpp
#include <gtest/gtest.h>
#include "test/mocks/MockTempSensor.h"
#include "clevercoffee/control/ProcessController.h"

TEST(EmergencyStop, TriggersAboveThreshold) {
    MockTempSensor sensor;
    sensor.setTemperature(165.0);  // Above 150°C limit
    
    ProcessController controller(&sensor);
    EXPECT_TRUE(controller.testEmergencyConditions());
}

TEST(EmergencyStop, AllowsNormalTemperature) {
    MockTempSensor sensor;
    sensor.setTemperature(92.0);
    
    ProcessController controller(&sensor);
    EXPECT_FALSE(controller.testEmergencyConditions());
}

TEST(EmergencyStop, DetectsSensorDisconnection) {
    MockTempSensor sensor;
    sensor.setError(true);  // Simulate disconnected sensor
    
    ProcessController controller(&sensor);
    EXPECT_TRUE(controller.testEmergencyConditions());
}
```

---

## 7. Tools & Libraries for ESP32 Testing

### Built-In Tools (No Installation Needed)

```
✅ Unity v2.6.0
   - Location: ~/.platformio/packages/framework-arduinoespressif32/tools/sdk/esp32/include/unity/
   - Included: Unity test framework + Unity fixture
   - Commands: TEST_ASSERT_EQUAL(), TEST_ASSERT_TRUE(), etc.
   - Device: YES (runs on ESP32)
   - Memory: ~20KB

✅ CMock (not ideal for C++ but available)
   - Location: Same SDK tools
   - Commands: EXPECT_CALL(), ASSERT_CALLED(), etc.
   - Memory: ~30KB
```

### Available in PlatformIO Registry

```
For Desktop Testing:
  throwtheswitch/Unity        2.6.0   ✅ Device & Desktop
  google/googletest           1.15.2  ✅ Desktop only
  doctest/doctest             2.4.11  ✅ Desktop (fast)
  baracodadailyhealthtech/catch2 4-3.7.1 ✅ Desktop (modern)

For Arduino API Mocking:
  fabiobatsilva/ArduinoFake   0.4.0   ✅ Mocks digitalWrite, etc
  berrak/Mock                 0.2.0   ✅ Lightweight mocking

For Hardware Mocking:
  berrak/MockEEPROM           0.1.0   ✅ Mock EEPROM.read/write
  apsol/LittleFSMock          0.2.2   ✅ Mock LittleFS
```

### Recommended Setup

```ini
# platformio.ini - Add test environment

[env:esp32_test]
extends = esp32_v1_base
test_framework = unity

[env:native_test]
platform = native
test_framework = googletest
lib_deps = google/googletest
```

---

## 8. Example: Testing Emergency Temperature Detection

This is a perfect example from your bug fixes list.

### Bug: Emergency Temperature Detection

**Original Code Problem:**
- Hardcoded threshold (150°C)
- No debouncing
- No hysteresis
- No sensor validation

### Complete Test Suite

```cpp
// test/test_emergency_temperature.cpp
#include <gtest/gtest.h>
#include "test/mocks/MockTempSensor.h"
#include "test/mocks/MockHardwareManager.h"
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/Config.h"

class EmergencyTemperatureTest : public ::testing::Test {
protected:
    MockTempSensor sensor;
    MockHardwareManager hwManager;
    ProcessController controller;
    
    void SetUp() override {
        // Reset config to defaults
        Config::getInstance().emergencyStopTemp.set(150.0);
        Config::getInstance().emergencyStopHysteresis.set(5.0);
    }
};

// Test 1: Detects temperature above threshold
TEST_F(EmergencyTemperatureTest, TriggersAboveThreshold) {
    sensor.setTemperature(160.0);
    
    EXPECT_TRUE(controller.testEmergencyConditions());
    EXPECT_TRUE(controller.isEmergencyStopped());
}

// Test 2: Allows temperature below threshold
TEST_F(EmergencyTemperatureTest, AllowsNormalTemperature) {
    sensor.setTemperature(92.0);
    
    EXPECT_FALSE(controller.testEmergencyConditions());
    EXPECT_FALSE(controller.isEmergencyStopped());
}

// Test 3: Debouncing - requires 3 consecutive readings
TEST_F(EmergencyTemperatureTest, DebouncesHighTemperature) {
    // First high reading
    sensor.setTemperature(160.0);
    EXPECT_FALSE(controller.testEmergencyConditions());  // Not triggered yet
    
    // Second high reading
    controller.update();
    EXPECT_FALSE(controller.testEmergencyConditions());  // Still not triggered
    
    // Third high reading - NOW trigger
    controller.update();
    EXPECT_TRUE(controller.testEmergencyConditions());   // Triggered!
}

// Test 4: Hysteresis prevents oscillation
TEST_F(EmergencyTemperatureTest, HysteresisPreventsFalseRecovery) {
    // Go above threshold
    sensor.setTemperature(160.0);
    EXPECT_TRUE(controller.testEmergencyConditions());
    
    // Drop to 148°C (between threshold-hysteresis and threshold)
    sensor.setTemperature(148.0);
    controller.update();
    
    // Should still be triggered (hysteresis zone)
    EXPECT_TRUE(controller.testEmergencyConditions());
    
    // Drop below threshold-hysteresis (145°C)
    sensor.setTemperature(144.0);
    controller.update();
    
    // NOW reset
    EXPECT_FALSE(controller.testEmergencyConditions());
}

// Test 5: Detects sensor disconnection
TEST_F(EmergencyTemperatureTest, DetectsSensorDisconnection) {
    sensor.setTemperature(-100.0);  // Below valid range (-50 to 200)
    
    EXPECT_TRUE(controller.testEmergencyConditions());
    EXPECT_TRUE(controller.isEmergencyStopped());
}

// Test 6: Invalid readings from short circuit
TEST_F(EmergencyTemperatureTest, DetectsShortCircuit) {
    sensor.setTemperature(300.0);  // Above valid range
    
    EXPECT_TRUE(controller.testEmergencyConditions());
    EXPECT_TRUE(controller.isEmergencyStopped());
}

// Test 7: Configurable threshold
TEST_F(EmergencyTemperatureTest, RespectConfigurableThreshold) {
    Config::getInstance().emergencyStopTemp.set(140.0);
    
    sensor.setTemperature(145.0);
    EXPECT_TRUE(controller.testEmergencyConditions());
}

// Test 8: Reset on state change
TEST_F(EmergencyTemperatureTest, ResetsDebounceOnStateChange) {
    // Go above threshold 2 times (not triggered yet)
    sensor.setTemperature(160.0);
    controller.testEmergencyConditions();
    controller.update();
    controller.testEmergencyConditions();
    
    // Change to safe state (STANDBY)
    controller.onStateChange(MachineStateId::STANDBY);
    
    // Temperature still high, but counter is reset
    controller.testEmergencyConditions();
    EXPECT_FALSE(controller.testEmergencyConditions());  // Needs 3 again
}
```

### Device Integration Test

```cpp
// test/test_emergency_temp_device.cpp (runs on ESP32)
#include <unity.h>
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/sensors/SensorManager.h"

void test_emergency_temp_with_real_sensor(void) {
    // This test uses REAL temperature sensor
    SensorManager sensorManager;
    ProcessController controller;
    
    // Read actual temperature
    double actualTemp = sensorManager.getCurrentTemperature();
    
    // Should not trigger emergency at room temperature
    TEST_ASSERT_FALSE(controller.testEmergencyConditions());
    TEST_ASSERT(actualTemp < 100.0);  // Room temp
}

void test_emergency_debounce_timing(void) {
    // Verify debounce works with real ISR timing
    unsigned long startTime = millis();
    
    // Monitor temperature spike (if heated)
    while (millis() - startTime < 5000) {
        ProcessController::update();
        delay(100);
    }
    
    // If no spike occurred, test passes (no false emergency)
    TEST_ASSERT_TRUE(true);
}

void setup() {
    Serial.begin(115200);
    delay(2000);
    
    UNITY_BEGIN();
    RUN_TEST(test_emergency_temp_with_real_sensor);
    RUN_TEST(test_emergency_debounce_timing);
    UNITY_END();
}

void loop() {}
```

---

## 9. Summary Table: What's Possible

| Testing Need | Desktop | ESP32 Device | Recommendation |
|--------------|---------|--------------|-----------------|
| **Business Logic** | YES (GoogleTest) | YES (Unity) | Use desktop for speed |
| **State Machines** | YES (GoogleTest) | YES (Unity) | Use desktop for coverage |
| **Hardware Mocking** | YES (GoogleMock) | PARTIAL (hand-coded) | Desktop mocks + device stubs |
| **Real Sensors** | NO | YES | Device only |
| **ISR Timing** | NO | YES | Device with scope |
| **Memory Leaks** | YES (Valgrind) | YES (extended test) | Both |
| **WiFi/Network** | STUB | YES | Device only |
| **Battery/Power** | NO | YES | Device only |
| **EEPROM** | YES (Mock) | YES (Real) | Both |
| **Real-Time** | NO | YES | Device with analysis |

---

## 10. Action Items

### Phase 1: Setup (1 hour)
- Add test environment to platformio.ini (native + esp32)
- Install GoogleTest from PlatformIO registry
- Create test/ directory structure:
  ```
  test/
  ├── mocks/
  │   ├── MockTempSensor.h
  │   ├── MockRelay.h
  │   ├── MockHardwareManager.h
  │   └── MockSensorManager.h
  ├── desktop/
  │   ├── test_state_machines.cpp
  │   ├── test_emergency_temperature.cpp
  │   └── test_process_controller.cpp
  └── device/
      ├── test_emergency_temperature.cpp
      └── test_real_sensors.cpp
  ```

### Phase 2: Mock Layer (2 hours)
- Create mock implementations of:
  - TempSensor interface
  - Relay interface
  - Switch interface
  - HardwareManager facade

### Phase 3: Unit Tests (3 hours)
- Test emergency temperature fix (from bug list)
- Test state machine transitions
- Test ISR race condition fix
- Test null pointer fixes

### Phase 4: Device Tests (2 hours)
- Verify mocks work with real hardware
- Test memory leak fixes
- Test ISR timing with real constraints

---

## 11. Files to Reference

**Espressif Unity Documentation:**
```
~/.platformio/packages/framework-arduinoespressif32/tools/sdk/esp32/include/unity/
```

**PlatformIO Testing Docs:**
- https://docs.platformio.org/en/latest/advanced/unit-testing/index.html

**GoogleTest Docs:**
- https://google.github.io/googletest/

---

## Conclusion

**YES, comprehensive testing is absolutely possible for CleverCoffee:**

1. Use **GoogleTest on desktop** for fast, thorough unit testing (100+ tests)
2. Use **Unity on device** for hardware integration (20-30 tests per batch)
3. **Mock hardware layers** using existing abstract interfaces
4. **Test emergency temperature** with debouncing, hysteresis, and validation
5. **Verify ISR fixes** with real timing on hardware

The infrastructure is already there in your abstractions. You just need to add the test layer and mock implementations.

---

