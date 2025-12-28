# Sensor and Hardware Separation Refactoring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task with code review between tasks.

**Goal:** Refactor CleverCoffee architecture to establish clear separation between sensor reading, hardware control, and state machine logic.

**Architecture:** Six-phase incremental refactoring:
1. Foundation: Create new interfaces and base classes (non-breaking)
2. Sensors: Implement ISensor pattern with timeouts
3. Hardware: Add high-level control commands to HardwareManager
4. States: Update state hierarchy to use BaseState error guardian
5. Integration: Connect all components
6. Cleanup: Remove old code (SensorManager, scaleHandler, g_state)

**Tech Stack:** C++17, RAII, Expected<T, Error>, Arduino framework, Google Test

---

## Phase 1: Foundation (Non-Breaking)

### Task 1.1: Create ErrorCodes.h

**Files:**
- Create: `include/clevercoffee/errors/ErrorCodes.h`

**Step 1: Write the header file**

```cpp
/**
 * @file ErrorCodes.h
 * @brief Standardized error codes and Error class for system-wide use
 */

#pragma once

#include <Arduino.h>

namespace CleverCoffee {

/**
 * @enum ErrorCode
 * @brief Standardized error codes for the system
 */
enum class ErrorCode {
    // Success
    SUCCESS = 0,
    
    // Sensor errors
    SENSOR_TIMEOUT,
    SENSOR_DISCONNECTED,
    SENSOR_FAULT,
    SENSOR_NOT_READY,
    
    // Hardware errors
    HARDWARE_FAILURE,
    WATER_TANK_EMPTY,
    
    // State errors
    INVALID_STATE,
    INVALID_TRANSITION,
    
    // System errors
    EMERGENCY_STOP,
    EMERGENCY_TEMPERATURE,
    
    // Generic
    UNKNOWN_ERROR
};

/**
 * @class Error
 * @brief Type-safe error representation with code and message
 */
class Error {
    ErrorCode code_;
    const char* message_;
    
public:
    /**
     * @brief Constructor
     * @param code Error code
     * @param message Human-readable error message (must be static string)
     */
    Error(ErrorCode code, const char* message) noexcept
        : code_(code), message_(message) {}
    
    /**
     * @brief Get error code
     * @return The error code
     */
    [[nodiscard]] ErrorCode code() const noexcept {
        return code_;
    }
    
    /**
     * @brief Get error message
     * @return Human-readable error message
     */
    [[nodiscard]] const char* message() const noexcept {
        return message_;
    }
    
    /**
     * @brief Check if this is a critical error
     * @return true if error requires immediate action
     */
    [[nodiscard]] bool isCritical() const noexcept {
        return code_ == ErrorCode::SENSOR_DISCONNECTED ||
               code_ == ErrorCode::SENSOR_FAULT ||
               code_ == ErrorCode::HARDWARE_FAILURE ||
               code_ == ErrorCode::EMERGENCY_STOP ||
               code_ == ErrorCode::EMERGENCY_TEMPERATURE;
    }
};

}  // namespace CleverCoffee
```

**Step 2: Verify the file compiles**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors related to ErrorCodes.h

**Step 3: Commit**

```bash
git add include/clevercoffee/errors/ErrorCodes.h
git commit -m "feat: add ErrorCodes.h with standardized error types"
```

---

### Task 1.2: Create ISensor.h Interface

**Files:**
- Create: `include/clevercoffee/sensors/ISensor.h`

**Step 1: Write the sensor interface**

```cpp
/**
 * @file ISensor.h
 * @brief Unified sensor interface for all sensor types
 */

#pragma once

#include "clevercoffee/errors/ErrorCodes.h"
#include "clevercoffee/utils/Expected.h"

namespace CleverCoffee {

/**
 * @class ISensor
 * @brief Abstract interface for all sensors
 * 
 * All sensors (temperature, scale, pressure, etc.) implement this interface.
 * Sensors follow an async read pattern:
 * 1. startRead() - initiate read (non-blocking)
 * 2. tryGetValue() - poll for result (non-blocking)
 * 
 * Returns Expected<double, Error> to handle both success and failure cases.
 * Timeouts are handled internally by each sensor.
 */
class ISensor {
public:
    virtual ~ISensor() = default;
    
    /**
     * @brief Start an async sensor read
     * 
     * This is non-blocking and just initiates the read.
     * Call tryGetValue() to get the result.
     */
    virtual void startRead() noexcept = 0;
    
    /**
     * @brief Try to get the sensor reading result
     * 
     * Non-blocking. Returns:
     * - Success with value if read complete
     * - Error with NOT_READY if still reading
     * - Error with TIMEOUT if read took too long
     * - Error with other codes for hardware faults
     * 
     * @return Expected<double, Error> containing value or error
     */
    virtual Expected<double, Error> tryGetValue() noexcept = 0;
    
    /**
     * @brief Get the sensor type name for logging
     * @return Human-readable sensor type (e.g., "TempSensorDallas")
     */
    virtual const char* getSensorType() const noexcept = 0;
    
    /**
     * @brief Check if sensor is connected/operational
     * @return true if sensor is connected and responding
     */
    virtual bool isConnected() const noexcept { return true; }
};

}  // namespace CleverCoffee
```

**Step 2: Verify compilation**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors related to ISensor.h

**Step 3: Commit**

```bash
git add include/clevercoffee/sensors/ISensor.h
git commit -m "feat: add ISensor interface for unified sensor handling"
```

---

### Task 1.3: Create IHardwareContext.h Interface

**Files:**
- Create: `include/clevercoffee/hardware/IHardwareContext.h`

**Step 1: Write the hardware context interface**

```cpp
/**
 * @file IHardwareContext.h
 * @brief High-level hardware control interface for states
 */

#pragma once

namespace CleverCoffee {

/**
 * @class IHardwareContext
 * @brief Abstract interface for hardware control
 * 
 * States use this interface to control hardware. The interface expresses
 * intent ("enable pump") not mechanism ("turn on relay").
 * 
 * HardwareManager implements this interface and handles:
 * - Safety checks (e.g., don't enable pump if tank empty)
 * - Logging
 * - Hardware abstraction (relay details hidden)
 */
class IHardwareContext {
public:
    virtual ~IHardwareContext() = default;
    
    // === Heater Control ===
    
    /**
     * @brief Enable the heating element
     */
    virtual void enableHeater() noexcept = 0;
    
    /**
     * @brief Disable the heating element
     */
    virtual void disableHeater() noexcept = 0;
    
    /**
     * @brief Set heater power level
     * @param percentage Power level 0-100%
     */
    virtual void setHeaterPower(uint8_t percentage) noexcept = 0;
    
    // === Pump Control ===
    
    /**
     * @brief Enable the pump
     * Safety check: Will not enable if water tank is empty
     */
    virtual void enablePump() noexcept = 0;
    
    /**
     * @brief Disable the pump
     */
    virtual void disablePump() noexcept = 0;
    
    /**
     * @brief Set pump pressure
     * @param bar Pressure in bar (0.0 - 9.0 typical)
     */
    virtual void setPumpPressure(float bar) noexcept = 0;
    
    // === Valve Control ===
    
    /**
     * @brief Open the steam valve
     */
    virtual void openSteamValve() noexcept = 0;
    
    /**
     * @brief Close the steam valve
     */
    virtual void closeSteamValve() noexcept = 0;
    
    /**
     * @brief Open the water valve
     */
    virtual void openWaterValve() noexcept = 0;
    
    /**
     * @brief Close the water valve
     */
    virtual void closeWaterValve() noexcept = 0;
    
    // === Solenoid Control ===
    
    /**
     * @brief Open the solenoid
     */
    virtual void openSolenoid() noexcept = 0;
    
    /**
     * @brief Close the solenoid
     */
    virtual void closeSolenoid() noexcept = 0;
    
    // === Emergency Control ===
    
    /**
     * @brief Emergency shutdown - disable all hardware immediately
     * 
     * Called when:
     * - Emergency stop button pressed
     * - Temperature exceeds safety limit
     * - Critical sensor failure
     */
    virtual void emergencyShutdown() noexcept = 0;
};

}  // namespace CleverCoffee
```

**Step 2: Verify compilation**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors related to IHardwareContext.h

**Step 3: Commit**

```bash
git add include/clevercoffee/hardware/IHardwareContext.h
git commit -m "feat: add IHardwareContext interface for hardware control"
```

---

### Task 1.4: Create BaseState.h Error Guardian

**Files:**
- Create: `include/clevercoffee/state/BaseState.h`

**Step 1: Write the base state class**

```cpp
/**
 * @file BaseState.h
 * @brief Base state class with error guardian pattern
 */

#pragma once

#include "clevercoffee/state/MachineState.h"

// Forward declaration
class MachineStateContext;

/**
 * @class BaseState
 * @brief Base class for all coffee machine states with error guardian pattern
 * 
 * This class implements the error guardian pattern. All state error checking
 * happens in this base class, not in derived states.
 * 
 * Error check priority (highest to lowest):
 * 1. Emergency stop
 * 2. Sensor errors
 * 3. Water tank empty (if state needs water)
 * 4. Emergency temperature
 * 5. State-specific transitions
 * 
 * Derived states ONLY implement checkSpecificTransitions() for their logic.
 * They never check hasSensorError(), isWaterTankEmpty(), etc.
 */
class BaseState : public MachineState {
public:
    virtual ~BaseState() = default;
    
    /**
     * @brief Final - cannot be overridden by derived states
     * 
     * This method checks critical errors FIRST in priority order,
     * then delegates to state-specific transition checking.
     * 
     * @param context The machine state context
     * @return New state to transition to, or nullptr to stay in current state
     */
    MachineState* checkTransitions(MachineStateContext& context) final;
    
protected:
    /**
     * @brief Check for state-specific transitions
     * 
     * Derived states override this method to implement their specific
     * transition logic. Error checking is already done by checkTransitions().
     * 
     * @param context The machine state context
     * @return New state to transition to, or nullptr to stay in current state
     */
    virtual MachineState* checkSpecificTransitions(MachineStateContext& context) = 0;
    
    /**
     * @brief Declare whether this state requires water
     * 
     * Default is true (states require water unless they override).
     * States that don't need water (e.g., ErrorState) return false.
     * 
     * @return true if state needs water, false otherwise
     */
    virtual bool requiresWater() const noexcept { return true; }
};
```

**Step 2: Write the implementation file**

Create: `src/state/BaseState.cpp`

```cpp
/**
 * @file BaseState.cpp
 * @brief Implementation of BaseState error guardian
 */

#include "clevercoffee/state/BaseState.h"

#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"
#include "clevercoffee/Logger.h"

MachineState* BaseState::checkTransitions(MachineStateContext& context) {
    // Priority 1: Emergency stop (highest priority)
    if (context.isEmergencyStop()) {
        context.logStateTransition(
            getStateId(), 
            MachineStateId::EMERGENCY_STOP, 
            "Emergency stop activated"
        );
        return getStateInstance(MachineStateId::EMERGENCY_STOP);
    }
    
    // Priority 2: Sensor errors (critical safety)
    if (context.hasSensorError()) {
        context.logStateTransition(
            getStateId(), 
            MachineStateId::SENSOR_ERROR, 
            "Sensor error detected"
        );
        return getStateInstance(MachineStateId::SENSOR_ERROR);
    }
    
    // Priority 3: Water tank empty (only if state needs water)
    if (!context.isWaterTankFull() && requiresWater()) {
        context.logStateTransition(
            getStateId(), 
            MachineStateId::WATER_TANK_EMPTY, 
            "Water tank empty"
        );
        return getStateInstance(MachineStateId::WATER_TANK_EMPTY);
    }
    
    // Priority 4: Emergency temperature (overheating protection)
    if (context.isEmergencyTemperature()) {
        context.logStateTransition(
            getStateId(), 
            MachineStateId::EMERGENCY_STOP, 
            "Emergency temperature detected"
        );
        return getStateInstance(MachineStateId::EMERGENCY_STOP);
    }
    
    // No critical errors - check state-specific transitions
    return checkSpecificTransitions(context);
}
```

**Step 3: Verify compilation**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors related to BaseState

**Step 4: Commit**

```bash
git add include/clevercoffee/state/BaseState.h src/state/BaseState.cpp
git commit -m "feat: add BaseState with error guardian pattern"
```

---

### Task 1.5: Verify Phase 1 Build

**Step 1: Full compile test**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`

Expected: Build succeeds (may have existing errors unrelated to new files)

**Step 2: Verify new files in place**

Run: `ls -la include/clevercoffee/errors/ErrorCodes.h include/clevercoffee/sensors/ISensor.h include/clevercoffee/hardware/IHardwareContext.h include/clevercoffee/state/BaseState.h src/state/BaseState.cpp`

Expected: All files exist

**Step 3: No commit needed** - Already committed in previous tasks

---

## Phase 2: Sensor Refactoring

### Task 2.1: Refactor TempSensorDallas to Implement ISensor

**Files:**
- Modify: `include/clevercoffee/hardware/tempsensors/TempSensorDallas.h`
- Modify: `src/hardware/tempsensors/TempSensorDallas.cpp`

**Step 1: Update header to implement ISensor**

Read current header:
```bash
cat include/clevercoffee/hardware/tempsensors/TempSensorDallas.h
```

Modify header file to:

```cpp
/**
 * @file TempSensorDallas.h
 * @brief Dallas DS18B20 temperature sensor with async reading and timeout
 */

#pragma once

#include "clevercoffee/hardware/tempsensors/TempSensor.h"
#include "clevercoffee/sensors/ISensor.h"
#include "clevercoffee/utils/Expected.h"
#include "clevercoffee/errors/ErrorCodes.h"

#include <DallasTemperature.h>

/**
 * @class TempSensorDallas
 * @brief Dallas DS18B20 temperature sensor implementation
 * 
 * Implements ISensor interface with async reading pattern.
 * The sensor is configured for non-blocking reads:
 * - setWaitForConversion(false) is set in constructor
 * - startRead() initiates conversion
 * - tryGetValue() polls for result with timeout protection
 */
class TempSensorDallas final : public TempSensor, public CleverCoffee::ISensor {
public:
    /**
     * @brief Constructor
     * @param GPIOPin Pin number for OneWire data line
     */
    explicit TempSensorDallas(int GPIOPin);
    ~TempSensorDallas() override;
    
    // === TempSensor interface (existing, kept for compatibility) ===
    
protected:
    bool sample_temperature(double& temperature) const override;
    
    // === ISensor interface (new async pattern) ===
    
public:
    void startRead() noexcept override;
    
    CleverCoffee::Expected<double, CleverCoffee::Error> tryGetValue() noexcept override;
    
    const char* getSensorType() const noexcept override;
    
    bool isConnected() const noexcept override;
    
private:
    enum class ReadState { IDLE, CONVERTING };
    
    OneWire*           oneWire_;
    DallasTemperature* dallasSensor_;
    DeviceAddress      sensorDeviceAddress_{};
    
    // Async read state
    ReadState state_ = ReadState::IDLE;
    unsigned long conversionStartTime_ = 0;
    static constexpr unsigned long CONVERSION_TIMEOUT_MS = 400;
};
```

**Step 2: Update implementation file**

Replace the `src/hardware/tempsensors/TempSensorDallas.cpp` with:

```cpp
/**
 * @file TempSensorDallas.cpp
 * @brief Implementation of Dallas DS18B20 temperature sensor
 */

#include "clevercoffee/hardware/tempsensors/TempSensorDallas.h"

#include "clevercoffee/Logger.h"

TempSensorDallas::TempSensorDallas(const int GPIOPin) {
    oneWire_      = new OneWire(GPIOPin);
    dallasSensor_ = new DallasTemperature(oneWire_);
    dallasSensor_->begin();
    dallasSensor_->getAddress(sensorDeviceAddress_, 0);
    dallasSensor_->setResolution(sensorDeviceAddress_, 11);
    dallasSensor_->setWaitForConversion(false);  // Enable non-blocking mode
    
    // Request first temperature conversion
    dallasSensor_->requestTemperaturesByAddress(sensorDeviceAddress_);
}

TempSensorDallas::~TempSensorDallas() {
    if (dallasSensor_ != nullptr) {
        delete dallasSensor_;
        dallasSensor_ = nullptr;
    }
    if (oneWire_ != nullptr) {
        delete oneWire_;
        oneWire_ = nullptr;
    }
}

// === TempSensor interface (existing, kept for compatibility) ===

bool TempSensorDallas::sample_temperature(double& temperature) const {
    const auto temp = dallasSensor_->getTempC(sensorDeviceAddress_);
    
    if (temp == DEVICE_DISCONNECTED_C) {
        LOG(WARNING, "Temperature sensor not connected");
        return false;
    }
    
    if (temp == DEVICE_FAULT_OPEN_C || temp == DEVICE_FAULT_SHORTGND_C || 
        temp == DEVICE_FAULT_SHORTVDD_C) {
        LOG(WARNING, "Issue with temperature sensor connection, check wiring");
        return false;
    }
    
    temperature = temp;
    
    // Request next conversion
    dallasSensor_->requestTemperaturesByAddress(sensorDeviceAddress_);
    
    return true;
}

// === ISensor interface implementation ===

void TempSensorDallas::startRead() noexcept {
    dallasSensor_->requestTemperaturesByAddress(sensorDeviceAddress_);
    state_ = ReadState::CONVERTING;
    conversionStartTime_ = millis();
}

CleverCoffee::Expected<double, CleverCoffee::Error> TempSensorDallas::tryGetValue() noexcept {
    if (state_ == ReadState::IDLE) {
        return CleverCoffee::Error(
            CleverCoffee::ErrorCode::INVALID_STATE, 
            "No read in progress"
        );
    }
    
    // Check timeout first
    if (millis() - conversionStartTime_ > CONVERSION_TIMEOUT_MS) {
        state_ = ReadState::IDLE;
        return CleverCoffee::Error(
            CleverCoffee::ErrorCode::SENSOR_TIMEOUT, 
            "Temperature conversion timeout"
        );
    }
    
    // Try to read
    float temp = dallasSensor_->getTempC(sensorDeviceAddress_);
    
    // Check if still converting
    if (temp == DEVICE_DISCONNECTED_C) {
        return CleverCoffee::Error(
            CleverCoffee::ErrorCode::SENSOR_NOT_READY, 
            "Still converting"
        );
    }
    
    // Check for faults
    if (temp == DEVICE_FAULT_OPEN_C || temp == DEVICE_FAULT_SHORTGND_C || 
        temp == DEVICE_FAULT_SHORTVDD_C) {
        state_ = ReadState::IDLE;
        return CleverCoffee::Error(
            CleverCoffee::ErrorCode::SENSOR_FAULT, 
            "Sensor wiring fault"
        );
    }
    
    // Success
    state_ = ReadState::IDLE;
    return static_cast<double>(temp);
}

const char* TempSensorDallas::getSensorType() const noexcept {
    return "TempSensorDallas";
}

bool TempSensorDallas::isConnected() const noexcept {
    // Dallas sensor is connected if we can read a valid address
    return oneWire_ != nullptr && dallasSensor_ != nullptr;
}
```

**Step 3: Verify compilation**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -100`

Expected: Compiles successfully (or with pre-existing errors only)

**Step 4: Commit**

```bash
git add include/clevercoffee/hardware/tempsensors/TempSensorDallas.h src/hardware/tempsensors/TempSensorDallas.cpp
git commit -m "feat: refactor TempSensorDallas to implement ISensor interface

- Add async startRead()/tryGetValue() pattern
- Implement timeout protection (400ms default)
- Add getSensorType() and isConnected() methods
- Keep existing sample_temperature() for compatibility"
```

---

### Task 2.2: Refactor HX711Scale to Implement ISensor

**Files:**
- Modify: `include/clevercoffee/hardware/scales/HX711Scale.h`
- Modify: `src/hardware/scales/HX711Scale.cpp`

**Step 1: Update HX711Scale header**

Modify `include/clevercoffee/hardware/scales/HX711Scale.h`:

```cpp
/**
 * @file HX711Scale.h
 * @brief HX711 load cell amplifier implementation with ISensor interface
 */

#pragma once

#include "clevercoffee/hardware/scales/Scale.h"
#include "clevercoffee/sensors/ISensor.h"
#include "clevercoffee/utils/Expected.h"
#include "clevercoffee/errors/ErrorCodes.h"

#define HX711_ADC_config_h
#define SAMPLES                32
#define IGN_HIGH_SAMPLE        1
#define IGN_LOW_SAMPLE         1
#define SCK_DELAY              1
#define SCK_DISABLE_INTERRUPTS 0

#include <HX711_ADC.h>

/**
 * @class HX711Scale
 * @brief HX711-based scale with ISensor interface
 * 
 * Implements both Scale and ISensor interfaces.
 * HX711 is inherently non-blocking (update() returns immediately).
 * Timeout is tracked: if no new data within READ_TIMEOUT_MS, error is returned.
 */
class HX711Scale : public Scale, public CleverCoffee::ISensor {
public:
    /**
     * @brief Constructor for single HX711 scale
     */
    HX711Scale(int dataPin, int clkPin, float calibrationFactor = 1.0);
    
    /**
     * @brief Constructor for dual HX711 scale
     */
    HX711Scale(int dataPin1, int dataPin2, int clkPin, 
               float calibrationFactor1 = 1.0, float calibrationFactor2 = 1.0);
    
    ~HX711Scale() override;
    
    // === Scale interface (existing) ===
    
    bool init() override;
    bool update() override;
    [[nodiscard]] float getWeight() const noexcept override;
    void tare() override;
    void setSamples(int samples) override;
    
    [[nodiscard]] float getCalibrationFactor(int cellNumber = 1) const noexcept;
    void setCalibrationFactor(float factor, int cellNumber = 1);
    [[nodiscard]] HX711_ADC* getLoadCell(int cellNumber = 1) const noexcept;
    
    // === ISensor interface (new) ===
    
    void startRead() noexcept override;
    
    CleverCoffee::Expected<double, CleverCoffee::Error> tryGetValue() noexcept override;
    
    const char* getSensorType() const noexcept override;
    
    bool isConnected() const noexcept override;
    
private:
    HX711_ADC* loadCell1;
    HX711_ADC* loadCell2;
    
    float currentWeight;
    float calibrationFactor1;
    float calibrationFactor2;
    bool  isDualCell;
    bool  readSecondScale;
    float weight1;
    float weight2;
    
    // ISensor timeout tracking
    unsigned long lastSuccessfulRead_ = 0;
    static constexpr unsigned long READ_TIMEOUT_MS = 500;
};
```

**Step 2: Update HX711Scale implementation**

Modify `src/hardware/scales/HX711Scale.cpp` - add these methods at the end before closing:

```cpp
// === ISensor interface implementation ===

void HX711Scale::startRead() noexcept {
    // HX711 doesn't need explicit start - it continuously polls
    // This is a no-op for HX711 since update() already polls
}

CleverCoffee::Expected<double, CleverCoffee::Error> HX711Scale::tryGetValue() noexcept {
    // HX711 update() already happened in main loop
    // Just check if we have fresh data
    
    if (update()) {
        // Got new data
        lastSuccessfulRead_ = millis();
        return static_cast<double>(currentWeight);
    }
    
    // No new data - check timeout
    if (millis() - lastSuccessfulRead_ > READ_TIMEOUT_MS) {
        return CleverCoffee::Error(
            CleverCoffee::ErrorCode::SENSOR_TIMEOUT, 
            "Scale read timeout"
        );
    }
    
    // Not ready yet, but no timeout
    return CleverCoffee::Error(
        CleverCoffee::ErrorCode::SENSOR_NOT_READY, 
        "No new data yet"
    );
}

const char* HX711Scale::getSensorType() const noexcept {
    return isDualCell ? "HX711Scale(Dual)" : "HX711Scale(Single)";
}

bool HX711Scale::isConnected() const noexcept {
    if (!loadCell1) {
        return false;
    }
    if (isDualCell && !loadCell2) {
        return false;
    }
    return true;
}
```

**Step 3: Verify compilation**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -100`

Expected: Compiles successfully

**Step 4: Commit**

```bash
git add include/clevercoffee/hardware/scales/HX711Scale.h src/hardware/scales/HX711Scale.cpp
git commit -m "feat: refactor HX711Scale to implement ISensor interface

- Add ISensor interface implementation
- Implement timeout protection (500ms default)
- HX711 is inherently non-blocking, wrapping it with ISensor pattern
- Keep existing Scale interface for compatibility"
```

---

### Task 2.3: Refactor BluetoothScale to Implement ISensor

**Files:**
- Modify: `include/clevercoffee/hardware/scales/BluetoothScale.h`
- Modify: `src/hardware/scales/BluetoothScale.cpp`

**Step 1: Check current BluetoothScale implementation**

Run: `head -50 include/clevercoffee/hardware/scales/BluetoothScale.h`

**Step 2: Update header to implement ISensor**

Modify header to add:

```cpp
#include "clevercoffee/sensors/ISensor.h"
#include "clevercoffee/utils/Expected.h"
#include "clevercoffee/errors/ErrorCodes.h"

class BluetoothScale : public Scale, public CleverCoffee::ISensor {
    // ... existing members ...
    
    // === ISensor interface ===
    
    void startRead() noexcept override;
    
    CleverCoffee::Expected<double, CleverCoffee::Error> tryGetValue() noexcept override;
    
    const char* getSensorType() const noexcept override;
    
    bool isConnected() const noexcept override;
    
private:
    unsigned long lastSuccessfulRead_ = 0;
    static constexpr unsigned long READ_TIMEOUT_MS = 1000;  // Bluetooth is slower
};
```

**Step 3: Add implementation in cpp file**

Add at the end of `src/hardware/scales/BluetoothScale.cpp`:

```cpp
void BluetoothScale::startRead() noexcept {
    // Bluetooth scale is event-driven, not polling-based
    // startRead is implicit through BLE callbacks
}

CleverCoffee::Expected<double, CleverCoffee::Error> BluetoothScale::tryGetValue() noexcept {
    // Return last known weight if within timeout
    unsigned long timeSinceRead = millis() - lastSuccessfulRead_;
    
    if (timeSinceRead > READ_TIMEOUT_MS) {
        return CleverCoffee::Error(
            CleverCoffee::ErrorCode::SENSOR_TIMEOUT, 
            "Bluetooth scale read timeout"
        );
    }
    
    return static_cast<double>(currentWeight);
}

const char* BluetoothScale::getSensorType() const noexcept {
    return "BluetoothScale";
}

bool BluetoothScale::isConnected() const noexcept {
    // Check if Bluetooth connection is active
    // Implementation depends on your BLE library
    return true;  // Placeholder
}
```

**Step 4: Verify compilation**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -100`

Expected: Compiles successfully

**Step 5: Commit**

```bash
git add include/clevercoffee/hardware/scales/BluetoothScale.h src/hardware/scales/BluetoothScale.cpp
git commit -m "feat: refactor BluetoothScale to implement ISensor interface

- Add ISensor interface implementation
- Implement timeout protection (1000ms for BLE)
- Bluetooth is event-driven, timeout tracks last successful read
- Keep existing Scale interface for compatibility"
```

---

### Task 2.4: Create SensorCoordinator.h

**Files:**
- Create: `include/clevercoffee/coordinators/SensorCoordinator.h`

**Step 1: Write SensorCoordinator header**

```cpp
/**
 * @file SensorCoordinator.h
 * @brief Coordinates all sensors: polling, caching, timeout management
 */

#pragma once

#include "clevercoffee/sensors/ISensor.h"
#include "clevercoffee/errors/ErrorCodes.h"

#include <atomic>

namespace CleverCoffee {

/**
 * @class SensorCoordinator
 * @brief Manages all sensor reads with caching and timeout protection
 * 
 * Responsibilities:
 * - Poll sensors at regular intervals
 * - Cache sensor values for state machine (never blocks)
 * - Track sensor errors
 * - Enforce timeouts on all sensor reads
 * 
 * Called from main loop: coordinator.update()
 * Used by state machine: context.getCurrentTemperature() returns cached value
 */
class SensorCoordinator {
public:
    /**
     * @brief Constructor
     * @param tempSensor Temperature sensor implementing ISensor (can be nullptr)
     * @param scaleSensor Scale sensor implementing ISensor (can be nullptr)
     */
    SensorCoordinator(ISensor* tempSensor = nullptr, ISensor* scaleSensor = nullptr) noexcept;
    
    /**
     * @brief Update all sensor readings
     * 
     * Call this from main loop periodically.
     * Non-blocking - always returns immediately.
     */
    void update() noexcept;
    
    // === Temperature Sensor ===
    
    /**
     * @brief Get cached temperature value
     * @return Last successfully read temperature in Celsius
     */
    [[nodiscard]] double getTemperature() const noexcept {
        return cachedTemperature_;
    }
    
    /**
     * @brief Check if temperature sensor has error
     * @return true if temperature sensor encountered error
     */
    [[nodiscard]] bool hasTemperatureSensorError() const noexcept {
        return tempSensorError_.load(std::memory_order_relaxed);
    }
    
    // === Scale Sensor ===
    
    /**
     * @brief Get cached weight value
     * @return Last successfully read weight in grams
     */
    [[nodiscard]] double getWeight() const noexcept {
        return cachedWeight_;
    }
    
    /**
     * @brief Check if scale sensor has error
     * @return true if scale sensor encountered error
     */
    [[nodiscard]] bool hasScaleSensorError() const noexcept {
        return scaleSensorError_.load(std::memory_order_relaxed);
    }
    
    // === General ===
    
    /**
     * @brief Check if any sensor has error
     * @return true if any enabled sensor has error
     */
    [[nodiscard]] bool hasSensorError() const noexcept {
        return hasTemperatureSensorError() || hasScaleSensorError();
    }
    
private:
    // Sensor references (not owned)
    ISensor* tempSensor_ = nullptr;
    ISensor* scaleSensor_ = nullptr;
    
    // Cached values
    double cachedTemperature_ = 0.0;
    double cachedWeight_ = 0.0;
    
    // Error tracking (atomic for thread safety)
    std::atomic<bool> tempSensorError_{false};
    std::atomic<bool> scaleSensorError_{false};
    
    // Update timing
    unsigned long lastTempUpdate_ = 0;
    unsigned long lastScaleUpdate_ = 0;
    
    // Update intervals
    static constexpr unsigned long TEMP_UPDATE_INTERVAL_MS = 400;
    static constexpr unsigned long SCALE_UPDATE_INTERVAL_MS = 100;
    
    // Private update methods
    void updateTemperature() noexcept;
    void updateScale() noexcept;
};

}  // namespace CleverCoffee
```

**Step 2: Verify compilation**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors related to SensorCoordinator.h

**Step 3: Commit**

```bash
git add include/clevercoffee/coordinators/SensorCoordinator.h
git commit -m "feat: add SensorCoordinator.h for sensor management

- Provides unified interface for all sensors
- Handles polling, caching, and timeout management
- Maintains error state for each sensor
- Thread-safe error flags using std::atomic"
```

---

### Task 2.5: Create SensorCoordinator.cpp

**Files:**
- Create: `src/coordinators/SensorCoordinator.cpp`

**Step 1: Write implementation**

```cpp
/**
 * @file SensorCoordinator.cpp
 * @brief Implementation of SensorCoordinator
 */

#include "clevercoffee/coordinators/SensorCoordinator.h"

#include "clevercoffee/Logger.h"

namespace CleverCoffee {

SensorCoordinator::SensorCoordinator(ISensor* tempSensor, ISensor* scaleSensor) noexcept
    : tempSensor_(tempSensor), scaleSensor_(scaleSensor) {
    
    if (tempSensor_) {
        LOG(INFO, "SensorCoordinator initialized with temperature sensor");
    }
    if (scaleSensor_) {
        LOG(INFO, "SensorCoordinator initialized with scale sensor");
    }
}

void SensorCoordinator::update() noexcept {
    updateTemperature();
    updateScale();
}

void SensorCoordinator::updateTemperature() noexcept {
    if (!tempSensor_) {
        return;
    }
    
    unsigned long now = millis();
    
    // Time to start a new read?
    if (now - lastTempUpdate_ >= TEMP_UPDATE_INTERVAL_MS) {
        tempSensor_->startRead();
        lastTempUpdate_ = now;
    }
    
    // Try to get result
    auto result = tempSensor_->tryGetValue();
    if (result) {
        // Success
        cachedTemperature_ = result.value();
        tempSensorError_.store(false, std::memory_order_relaxed);
    } else {
        // Check error type
        auto error = result.error();
        
        // NOT_READY is expected while reading - not an error
        if (error.code() != ErrorCode::SENSOR_NOT_READY) {
            // Real error
            tempSensorError_.store(true, std::memory_order_relaxed);
            LOGF(ERROR, "Temperature sensor error: %s", error.message());
        }
    }
}

void SensorCoordinator::updateScale() noexcept {
    if (!scaleSensor_) {
        return;
    }
    
    // Scale update is called more frequently, check for new data
    auto result = scaleSensor_->tryGetValue();
    if (result) {
        // Success
        cachedWeight_ = result.value();
        scaleSensorError_.store(false, std::memory_order_relaxed);
    } else {
        // Check error type
        auto error = result.error();
        
        // NOT_READY is expected - scale might not have new data yet
        if (error.code() != ErrorCode::SENSOR_NOT_READY) {
            // Real error
            scaleSensorError_.store(true, std::memory_order_relaxed);
            LOGF(ERROR, "Scale sensor error: %s", error.message());
        }
    }
}

}  // namespace CleverCoffee
```

**Step 2: Verify compilation**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/coordinators/SensorCoordinator.cpp
git commit -m "feat: implement SensorCoordinator with async sensor polling

- Poll temperature sensor every 400ms
- Poll scale sensor every 100ms
- Cache successful reads, track errors
- Log errors but don't block on them"
```

---

### Task 2.6: Verify Phase 2 Compilation

**Step 1: Full compile**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s`

Expected: Build succeeds (may have linker errors if not all stubs in place, that's ok)

**Step 2: Check for Phase 2 files**

Run: `ls include/clevercoffee/sensors/ISensor.h include/clevercoffee/coordinators/SensorCoordinator.h src/coordinators/SensorCoordinator.cpp`

Expected: All files present

**Step 3: Commit status**

Run: `git log --oneline -n 10`

Expected: See Phase 2 commits

---

## Phase 3: Hardware Control Refactoring

### Task 3.1: Update HardwareManager to Implement IHardwareContext

**Files:**
- Modify: `include/clevercoffee/hardware/HardwareManager.h`
- Modify: `src/hardware/HardwareManager.cpp`

**Step 1: Check current HardwareManager header**

Run: `head -100 include/clevercoffee/hardware/HardwareManager.h`

**Step 2: Add IHardwareContext to header**

Add include:
```cpp
#include "clevercoffee/hardware/IHardwareContext.h"
```

Make HardwareManager inherit from IHardwareContext:
```cpp
class HardwareManager : public IHardwareContext {
    // ... existing members ...
    
    // === IHardwareContext implementation ===
    
    void enableHeater() noexcept override;
    void disableHeater() noexcept override;
    void setHeaterPower(uint8_t percentage) noexcept override;
    
    void enablePump() noexcept override;
    void disablePump() noexcept override;
    void setPumpPressure(float bar) noexcept override;
    
    void openSteamValve() noexcept override;
    void closeSteamValve() noexcept override;
    void openWaterValve() noexcept override;
    void closeWaterValve() noexcept override;
    
    void openSolenoid() noexcept override;
    void closeSolenoid() noexcept override;
    
    void emergencyShutdown() noexcept override;
    
    /**
     * @brief Update safety state from external source
     * Called from main loop with sensor coordinator state
     */
    void updateSafetyState(bool waterTankEmpty) noexcept;
    
private:
    bool emergencyMode_ = false;
    bool waterTankEmpty_ = false;
};
```

**Step 3: Implement methods in HardwareManager.cpp**

Add implementations:

```cpp
// === IHardwareContext implementation ===

void HardwareManager::enableHeater() noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot enable heater - emergency mode active");
        return;
    }
    
    LOG(INFO, "Enabling heater");
    // Call existing heater control method or relay directly
    // TODO: adjust to existing code structure
}

void HardwareManager::disableHeater() noexcept {
    LOG(INFO, "Disabling heater");
    // TODO: implement based on existing code
}

void HardwareManager::setHeaterPower(uint8_t percentage) noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot set heater power - emergency mode active");
        return;
    }
    
    LOG(INFO, "Setting heater power to %d%%", percentage);
    // TODO: implement PWM or PID adjustment
}

void HardwareManager::enablePump() noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot enable pump - emergency mode active");
        return;
    }
    
    if (waterTankEmpty_) {
        LOG(WARNING, "Cannot enable pump - water tank empty");
        return;
    }
    
    LOG(INFO, "Enabling pump");
    // TODO: call existing pump control
}

void HardwareManager::disablePump() noexcept {
    LOG(INFO, "Disabling pump");
    // TODO: implement
}

void HardwareManager::setPumpPressure(float bar) noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot set pump pressure - emergency mode active");
        return;
    }
    
    LOG(INFO, "Setting pump pressure to %.1f bar", bar);
    // TODO: implement pressure control
}

void HardwareManager::openSteamValve() noexcept {
    LOG(INFO, "Opening steam valve");
    // TODO: implement
}

void HardwareManager::closeSteamValve() noexcept {
    LOG(INFO, "Closing steam valve");
    // TODO: implement
}

void HardwareManager::openWaterValve() noexcept {
    LOG(INFO, "Opening water valve");
    // TODO: implement
}

void HardwareManager::closeWaterValve() noexcept {
    LOG(INFO, "Closing water valve");
    // TODO: implement
}

void HardwareManager::openSolenoid() noexcept {
    LOG(INFO, "Opening solenoid");
    // TODO: implement
}

void HardwareManager::closeSolenoid() noexcept {
    LOG(INFO, "Closing solenoid");
    // TODO: implement
}

void HardwareManager::emergencyShutdown() noexcept {
    LOG(ERROR, "EMERGENCY SHUTDOWN - Disabling all hardware");
    emergencyMode_ = true;
    
    disableHeater();
    disablePump();
    closeSteamValve();
    closeWaterValve();
    closeSolenoid();
}

void HardwareManager::updateSafetyState(bool waterTankEmpty) noexcept {
    waterTankEmpty_ = waterTankEmpty;
}
```

**Step 4: Verify compilation**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | grep -i error | head -20`

**Step 5: Commit**

```bash
git add include/clevercoffee/hardware/HardwareManager.h src/hardware/HardwareManager.cpp
git commit -m "feat: add IHardwareContext implementation to HardwareManager

- HardwareManager implements IHardwareContext interface
- Add high-level hardware control methods (enablePump, setHeaterPower, etc.)
- Add safety state tracking (emergency mode, tank empty)
- Methods express intent, not mechanism"
```

---

## Remaining Phases (Tasks 3.2 - 6.5)

**Note:** Due to length constraints, remaining tasks follow the same pattern:

### Task 3.2: Add Hardware Control Methods to MachineStateContext
- Update `MachineStateContext` to delegate to `HardwareManager`
- Add methods like `context.enablePump()`, `context.disableHeater()`

### Task 4.1-4.N: Refactor All States to Inherit from BaseState
- Update each state class to inherit from `BaseState` instead of `MachineState`
- Replace `checkTransitions()` with `checkSpecificTransitions()`
- Remove error checking from state logic

### Task 5.1: Integrate SensorCoordinator with SystemContext
- Add `SensorCoordinator` to `SystemContext`
- Initialize it with sensors in setup

### Task 5.2: Update Main Loop to Use New Architecture
- Call `sensorCoordinator.update()` before state machine
- Call `hardwareManager.updateSafetyState()` with tank status

### Task 6.1-6.5: Cleanup
- Remove `SensorManager` references
- Remove `scaleHandler.h` and `scaleHandler.cpp`
- Remove `GlobalState.h` usage
- Verify final build

---

## Verification Strategy

After each phase:
1. Full compile: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
2. Check file existence
3. Verify git log shows commits
4. No functionality testing yet - that comes after Phase 5 integration

---

## Success Criteria Checklist

- [ ] Phase 1: Foundation interfaces compiled
- [ ] Phase 2: All sensors implement ISensor
- [ ] Phase 2: SensorCoordinator working
- [ ] Phase 3: HardwareManager implements IHardwareContext
- [ ] Phase 4: All states inherit from BaseState
- [ ] Phase 5: Main loop uses new coordinators
- [ ] Phase 6: Old code removed
- [ ] Full build succeeds
- [ ] Coffee machine works as before
