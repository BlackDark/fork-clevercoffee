# ESP32 Testing Quick Start Guide

## TL;DR - Start Here

**Desktop Testing (fast feedback):**
```bash
# Use GoogleTest on your laptop
pio test -e native_test
```

**Device Testing (real hardware):**
```bash
# Use Unity on ESP32
pio test -e esp32_usb
```

---

## Setup (30 minutes)

### Step 1: Add Test Environments to platformio.ini

```ini
[env:native_test]
platform = native
test_framework = googletest
lib_deps = google/googletest

[env:esp32_test]
extends = esp32_v1_base
test_framework = unity
```

### Step 2: Create Directory Structure

```
test/
├── mocks/
│   ├── MockTempSensor.h
│   ├── MockRelay.h
│   ├── MockSwitch.h
│   └── MockHardwareManager.h
├── desktop/
│   ├── test_emergency_temperature.cpp
│   ├── test_state_machines.cpp
│   └── test_process_controller.cpp
└── device/
    ├── test_emergency_temperature.cpp
    └── test_real_sensors.cpp
```

### Step 3: Create First Mock

**test/mocks/MockTempSensor.h**
```cpp
#pragma once
#include "clevercoffee/hardware/tempsensors/TempSensor.h"

class MockTempSensor : public TempSensor {
private:
    mutable double temperature_ = 20.0;
    mutable bool hasError_ = false;
    
public:
    void setTemperature(double temp) { temperature_ = temp; }
    void setError(bool error) { hasError_ = error; }
    
protected:
    bool sample_temperature(double& temperature) const override {
        if (hasError_) return false;
        temperature = temperature_;
        return true;
    }
};
```

---

## Desktop Testing Example (GoogleTest)

**test/desktop/test_emergency_temperature.cpp**

```cpp
#include <gtest/gtest.h>
#include "../mocks/MockTempSensor.h"
#include "clevercoffee/control/ProcessController.h"

class EmergencyTemperatureTest : public ::testing::Test {
protected:
    MockTempSensor sensor;
    ProcessController controller;
};

TEST_F(EmergencyTemperatureTest, TriggersAboveThreshold) {
    sensor.setTemperature(160.0);
    EXPECT_TRUE(controller.testEmergencyConditions());
}

TEST_F(EmergencyTemperatureTest, AllowsNormalTemperature) {
    sensor.setTemperature(92.0);
    EXPECT_FALSE(controller.testEmergencyConditions());
}

TEST_F(EmergencyTemperatureTest, DetectsSensorDisconnection) {
    sensor.setTemperature(-100.0);  // Out of valid range
    EXPECT_TRUE(controller.testEmergencyConditions());
}

TEST_F(EmergencyTemperatureTest, DebouncesHighTemp) {
    sensor.setTemperature(160.0);
    
    // First reading - not triggered
    EXPECT_FALSE(controller.testEmergencyConditions());
    
    controller.update();
    
    // Second reading - still not triggered
    EXPECT_FALSE(controller.testEmergencyConditions());
    
    controller.update();
    
    // Third reading - now triggered
    EXPECT_TRUE(controller.testEmergencyConditions());
}
```

**Run it:**
```bash
pio test -e native_test --filter "EmergencyTemperature"
```

---

## Device Testing Example (Unity)

**test/device/test_emergency_temperature.cpp**

```cpp
#include <unity.h>
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/sensors/SensorManager.h"

void test_emergency_temp_with_real_sensor(void) {
    SensorManager sensorManager;
    ProcessController controller;
    
    // Read actual temperature from device
    double actualTemp = sensorManager.getCurrentTemperature();
    
    // Should not trigger at room temperature
    TEST_ASSERT_FALSE(controller.testEmergencyConditions());
    TEST_ASSERT(actualTemp > -50.0 && actualTemp < 150.0);
}

void test_memory_stability(void) {
    // Run for 30 seconds to detect memory leaks
    unsigned long start = millis();
    unsigned int iterations = 0;
    
    while (millis() - start < 30000) {
        ProcessController::update();
        iterations++;
        delay(10);
    }
    
    TEST_ASSERT(iterations > 100);  // Should complete many cycles
}

void setup() {
    Serial.begin(115200);
    delay(2000);
    
    UNITY_BEGIN();
    RUN_TEST(test_emergency_temp_with_real_sensor);
    RUN_TEST(test_memory_stability);
    UNITY_END();
}

void loop() {}
```

**Run it:**
```bash
pio test -e esp32_usb --upload-port /dev/cu.usbserial-XXXX
```

---

## Common Test Patterns

### Pattern 1: Mock Sensor Input
```cpp
TEST(MyTest, SensorReading) {
    MockTempSensor sensor;
    sensor.setTemperature(95.5);
    
    // ... test code ...
    
    EXPECT_EQ(controller.getCurrentTemp(), 95.5);
}
```

### Pattern 2: Test State Transition
```cpp
TEST(StateTransitions, BrewStartStop) {
    MockTempSensor sensor;
    sensor.setTemperature(92.0);
    
    MachineStateContext context;
    BrewState brewState;
    
    brewState.onEntryImpl(context);
    EXPECT_EQ(context.currentState(), BREW_RUNNING);
    
    brewState.checkSpecificTransitions(context);
    // Verify transition occurred
}
```

### Pattern 3: Test Safety Logic
```cpp
TEST(SafetyLogic, EmergencyStop) {
    MockTempSensor sensor;
    MockRelay heater;
    
    sensor.setTemperature(165.0);  // Emergency condition
    
    ProcessController controller(&sensor, &heater);
    bool emergency = controller.testEmergencyConditions();
    
    EXPECT_TRUE(emergency);
    EXPECT_FALSE(heater.isOn());  // Heater should be off
}
```

### Pattern 4: Test Configuration
```cpp
TEST(Configuration, EmergencyThreshold) {
    Config::getInstance().emergencyStopTemp.set(140.0);
    
    MockTempSensor sensor;
    sensor.setTemperature(145.0);
    
    ProcessController controller;
    EXPECT_TRUE(controller.testEmergencyConditions());
}
```

---

## Running Tests

### Run All Tests
```bash
pio test -e native_test         # Desktop
pio test -e esp32_usb           # Device
```

### Run Specific Test Suite
```bash
pio test -e native_test --filter "*EmergencyTemperature*"
```

### List Available Tests
```bash
pio test -e native_test --list-tests
```

### Verbose Output
```bash
pio test -e native_test -vvv
```

### Generate Test Report
```bash
pio test -e native_test --junit-output-path test_results.xml
```

---

## Integration with Bug Fixes

Testing supports all 8 critical bug fixes:

| Bug | Test Type | Framework |
|-----|-----------|-----------|
| MEM-001: TempSensorTSIC leak | Memory | Valgrind (desktop) |
| MEM-002: TempSensorDallas leak | Memory | Extended device test |
| ISR-001: Race condition | Integration | Device test |
| STATE-001: Null returns | Unit | GoogleTest |
| STATE-002: Static variable | Unit | GoogleTest |
| HW-001: Const violation | Compile | clang-format |
| CTRL-001: Null check | Unit | GoogleTest |
| CTRL-002: Emergency temp | Unit + Device | Both |

---

## Next Steps

1. **Week 1:** Create mock layer (2 hours)
2. **Week 2:** Write unit tests for emergency temperature (3 hours)
3. **Week 3:** Write integration tests (3 hours)
4. **Week 4:** Add device tests (2 hours)

---

## Troubleshooting

### Tests Won't Compile
- Check that you're using correct GoogleTest syntax
- Ensure mock headers are in right path
- Verify includes are correct

### Tests Run Too Slow
- You're probably including too much
- Use mocks instead of real hardware
- Desktop tests should run in < 1 second

### Device Memory Issues
- Reduce number of tests per batch
- Use Unity's `TEST_IGNORE` for less critical tests
- Run device tests in separate batches

### ISR Timing Off
- This is normal on device
- Cannot be deterministic due to WiFi/BLE
- Use oscilloscope to measure real timing

---

## Resources

- Full documentation: `docs/ESP32_TESTING_CAPABILITIES.md`
- GoogleTest: https://google.github.io/googletest/
- Unity: https://github.com/ThrowTheSwitch/Unity
- PlatformIO: https://docs.platformio.org/en/latest/advanced/unit-testing/

---

## Quick Reference

```bash
# Setup
git clone <repo>
cd fork-clevercoffee

# First test run
mkdir -p test/mocks test/desktop test/device
# Copy mock files...
# Write first test...

# Run desktop tests
pio test -e native_test

# Run device tests
pio test -e esp32_usb

# Check coverage
pio test -e native_test --verbose --json-output test_output.json
```

---

**You're ready to test! Start with the mock layer, then write a single test.**
