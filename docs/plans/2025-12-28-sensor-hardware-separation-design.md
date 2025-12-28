# Sensor and Hardware Separation Refactoring Design

**Date**: 2025-12-28  
**Status**: Design Complete - Ready for Implementation  
**Breaking Changes**: Yes (intentional - modernization)

## Overview

Complete architectural refactoring to establish clear separation of concerns between sensors, hardware control, and state machine logic. This design eliminates mixed responsibilities and establishes a clean, testable architecture.

## Design Principles

1. **Clear Separation of Concerns**: Sensors read, states decide, hardware executes
2. **No Mixed Responsibilities**: Each component has one job
3. **Timeout Protection**: All sensor reads have configurable timeouts
4. **Type-Safe Errors**: Use `Expected<T, Error>` for all fallible operations
5. **State Machine Controls Everything**: Only state machine triggers hardware actions
6. **Cached Sensor Values**: State machine never waits on sensor reads
7. **Safety First**: Hardware manager enforces safety checks

## Core Architecture

### Component Responsibilities

**Sensors** (Hardware Layer)
- **Responsibility**: Read hardware, return values or errors
- **Does NOT**: Trigger state changes, control other hardware, manage application logic
- **Interface**: Simple read operations that return `Expected<T, Error>`
- **Timeout**: Internal async pattern with configurable timeouts

**SensorCoordinator** (Coordination Layer)
- **Responsibility**: Poll sensors periodically, cache values, enforce timeouts, track error state
- **Does NOT**: Decide what to do with errors, control hardware actuators
- **Interface**: Provide cached sensor values + error flags to state machine
- **Update frequency**: Called from main loop at regular intervals

**HardwareManager** (Hardware Control Layer)
- **Responsibility**: Control actuators (pump, heater, valves, solenoid)
- **Does NOT**: Read sensors, make decisions about when to activate
- **Interface**: High-level commands like `enablePump()`, `setHeaterPower()`, `openValve()`
- **Safety**: Enforces safety checks (e.g., don't enable pump if tank empty)

**State Machine** (Control Logic Layer)
- **Responsibility**: Execute brewing logic, make all control decisions
- **Does NOT**: Directly manipulate relays/pins
- **Interface**: Calls `HardwareManager` to control hardware, reads from `SensorCoordinator`

**BaseState** (Error Guardian)
- **Responsibility**: Check critical errors BEFORE state-specific logic
- **Does NOT**: Handle state-specific logic
- **Interface**: `checkTransitions()` checks critical errors first, then calls `checkSpecificTransitions()`

### Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│                        Main Loop                             │
└────────┬────────────────────────────────────────────────────┘
         │
         ├─► SensorCoordinator.update()
         │   ├─► TempSensor.tryGetValue() ──► Cache temperature
         │   ├─► ScaleSensor.tryGetValue() ──► Cache weight  
         │   ├─► PressureSensor.tryGetValue() ──► Cache pressure
         │   └─► WaterTankSensor.read() ──► Cache tank status
         │
         ├─► HardwareManager.updateSafetyState(tankStatus)
         │
         ├─► StateMachine.update()
         │   │
         │   ├─► currentState.update(context)
         │   │   └─► BrewRunningState.update()
         │   │       ├─► Read: context.getCurrentTemperature()
         │   │       │         └─► Returns cached value
         │   │       ├─► Read: context.getCurrentBrewWeight()
         │   │       │         └─► Returns cached value
         │   │       └─► Control: context.setPumpPressure(9.0)
         │   │                 └─► HardwareManager.setPumpPressure()
         │   │                     └─► Check safety → Control relay
         │   │
         │   └─► currentState.checkTransitions(context)
         │       │
         │       └─► BaseState.checkTransitions() [GUARDIAN]
         │           ├─► if (isEmergencyStop()) → EmergencyStopState
         │           ├─► if (hasSensorError()) → SensorErrorState
         │           ├─► if (!waterTankFull) → WaterTankEmptyState
         │           └─► No errors? → checkSpecificTransitions()
         │               └─► BrewRunningState checks brew-specific logic
         │
         ├─► DisplayManager.update()
         │
         └─► NetworkManager.update()
```

## Detailed Component Design

### 1. Sensor Interface & Timeout Pattern

#### ISensor Interface

```cpp
class ISensor {
public:
    virtual ~ISensor() = default;
    
    // Start async read (non-blocking)
    virtual void startRead() noexcept = 0;
    
    // Try to get result (non-blocking)
    // Returns: Success with value, or Error (NOT_READY, TIMEOUT, DISCONNECTED, etc.)
    virtual Expected<double, Error> tryGetValue() noexcept = 0;
    
    // Get sensor type for logging/debugging
    virtual const char* getSensorType() const noexcept = 0;
    
    // Optional: Get sensor-specific info
    virtual bool isConnected() const noexcept { return true; }
};
```

#### Temperature Sensor Implementation

```cpp
class TempSensorDallas : public ISensor {
    enum class ReadState { IDLE, CONVERTING };
    ReadState state_ = ReadState::IDLE;
    unsigned long conversionStartTime_ = 0;
    static constexpr unsigned long CONVERSION_TIMEOUT_MS = 400;
    
public:
    void startRead() noexcept override {
        dallasSensor_->requestTemperaturesByAddress(sensorDeviceAddress_);
        state_ = ReadState::CONVERTING;
        conversionStartTime_ = millis();
    }
    
    Expected<double, Error> tryGetValue() noexcept override {
        if (state_ == ReadState::IDLE) {
            return Error(ErrorCode::INVALID_STATE, "No read in progress");
        }
        
        // Check timeout FIRST
        if (millis() - conversionStartTime_ > CONVERSION_TIMEOUT_MS) {
            state_ = ReadState::IDLE;
            return Error(ErrorCode::SENSOR_TIMEOUT, "Temperature conversion timeout");
        }
        
        // Try to read (non-blocking)
        float temp = dallasSensor_->getTempC(sensorDeviceAddress_);
        
        // Check if still converting or disconnected
        if (temp == DEVICE_DISCONNECTED_C) {
            return Error(ErrorCode::NOT_READY, "Still converting");
        }
        
        if (temp == DEVICE_FAULT_OPEN_C || temp == DEVICE_FAULT_SHORTGND_C) {
            state_ = ReadState::IDLE;
            return Error(ErrorCode::SENSOR_FAULT, "Sensor wiring fault");
        }
        
        // Success
        state_ = ReadState::IDLE;
        return static_cast<double>(temp);
    }
    
    const char* getSensorType() const noexcept override {
        return "TempSensorDallas";
    }
};
```

#### Scale Sensor Implementation

```cpp
class HX711Scale : public ISensor {
    HX711_ADC* loadCell1_;
    HX711_ADC* loadCell2_;
    float currentWeight_ = 0.0f;
    unsigned long lastSuccessfulRead_ = 0;
    static constexpr unsigned long READ_TIMEOUT_MS = 500;
    bool isDualCell_ = false;
    
public:
    void startRead() noexcept override {
        // HX711 doesn't need explicit start - just polls
        // This is a no-op for HX711
    }
    
    Expected<double, Error> tryGetValue() noexcept override {
        // Try to update (non-blocking)
        bool updated = false;
        
        if (!isDualCell_) {
            updated = loadCell1_->update();
            if (updated) {
                currentWeight_ = loadCell1_->getData();
            }
        } else {
            // Dual cell logic
            if (loadCell1_->update() || loadCell2_->update()) {
                currentWeight_ = loadCell1_->getData() + loadCell2_->getData();
                updated = true;
            }
        }
        
        if (updated) {
            lastSuccessfulRead_ = millis();
            return static_cast<double>(currentWeight_);
        }
        
        // Check timeout
        if (millis() - lastSuccessfulRead_ > READ_TIMEOUT_MS) {
            return Error(ErrorCode::SENSOR_TIMEOUT, "Scale read timeout");
        }
        
        return Error(ErrorCode::NOT_READY, "No new data yet");
    }
    
    const char* getSensorType() const noexcept override {
        return "HX711Scale";
    }
};
```

#### SensorCoordinator (Caching & Timeout Management)

```cpp
class SensorCoordinator {
    // Sensor references
    ISensor* tempSensor_ = nullptr;
    ISensor* scaleSensor_ = nullptr;
    
    // Cached values
    double cachedTemperature_ = 0.0;
    double cachedWeight_ = 0.0;
    
    // Error tracking
    bool tempSensorError_ = false;
    bool scaleSensorError_ = false;
    unsigned long lastTempUpdate_ = 0;
    unsigned long lastScaleUpdate_ = 0;
    
    // Update intervals
    static constexpr unsigned long TEMP_UPDATE_INTERVAL_MS = 400;
    static constexpr unsigned long SCALE_UPDATE_INTERVAL_MS = 100;
    
public:
    void update() noexcept {
        updateTemperature();
        updateScale();
    }
    
    void updateTemperature() noexcept {
        unsigned long now = millis();
        
        // Time to start a new read?
        if (now - lastTempUpdate_ >= TEMP_UPDATE_INTERVAL_MS) {
            tempSensor_->startRead();
            lastTempUpdate_ = now;
        }
        
        // Try to get result
        auto result = tempSensor_->tryGetValue();
        if (result) {
            cachedTemperature_ = result.value();
            tempSensorError_ = false;
        } else if (result.error().code() != ErrorCode::NOT_READY) {
            // Real error (not just "still reading")
            tempSensorError_ = true;
            LOGF(ERROR, "Temperature sensor error: %s", result.error().message());
        }
    }
    
    void updateScale() noexcept {
        auto result = scaleSensor_->tryGetValue();
        if (result) {
            cachedWeight_ = result.value();
            scaleSensorError_ = false;
        } else if (result.error().code() != ErrorCode::NOT_READY) {
            scaleSensorError_ = true;
            LOGF(ERROR, "Scale sensor error: %s", result.error().message());
        }
    }
    
    // Simple getters for cached values
    double getTemperature() const noexcept { return cachedTemperature_; }
    double getWeight() const noexcept { return cachedWeight_; }
    bool hasTemperatureSensorError() const noexcept { return tempSensorError_; }
    bool hasScaleSensorError() const noexcept { return scaleSensorError_; }
};
```

### 2. Hardware Control Interface

#### IHardwareContext Interface

```cpp
class IHardwareContext {
public:
    virtual ~IHardwareContext() = default;
    
    // Heater control
    virtual void enableHeater() noexcept = 0;
    virtual void disableHeater() noexcept = 0;
    virtual void setHeaterPower(uint8_t percentage) noexcept = 0;
    
    // Pump control
    virtual void enablePump() noexcept = 0;
    virtual void disablePump() noexcept = 0;
    virtual void setPumpPressure(float bar) noexcept = 0;
    
    // Valve control
    virtual void openSteamValve() noexcept = 0;
    virtual void closeSteamValve() noexcept = 0;
    virtual void openWaterValve() noexcept = 0;
    virtual void closeWaterValve() noexcept = 0;
    
    // Solenoid control
    virtual void openSolenoid() noexcept = 0;
    virtual void closeSolenoid() noexcept = 0;
    
    // Emergency shutdown
    virtual void emergencyShutdown() noexcept = 0;
};
```

#### HardwareManager Implementation

```cpp
class HardwareManager : public IHardwareContext {
    // Hardware components
    Relay* heaterRelay_ = nullptr;
    Relay* pumpRelay_ = nullptr;
    Relay* valveRelay_ = nullptr;
    Relay* solenoidRelay_ = nullptr;
    
    // Safety state
    bool emergencyMode_ = false;
    bool waterTankEmpty_ = false;
    
public:
    void enableHeater() noexcept override {
        if (emergencyMode_) {
            LOG(WARNING, "Cannot enable heater - emergency mode active");
            return;
        }
        
        LOG(INFO, "Enabling heater");
        heaterRelay_->turnOn();
    }
    
    void disableHeater() noexcept override {
        LOG(INFO, "Disabling heater");
        heaterRelay_->turnOff();
    }
    
    void enablePump() noexcept override {
        if (emergencyMode_) {
            LOG(WARNING, "Cannot enable pump - emergency mode active");
            return;
        }
        
        if (waterTankEmpty_) {
            LOG(WARNING, "Cannot enable pump - water tank empty");
            return;
        }
        
        LOG(INFO, "Enabling pump");
        pumpRelay_->turnOn();
    }
    
    void disablePump() noexcept override {
        LOG(INFO, "Disabling pump");
        pumpRelay_->turnOff();
    }
    
    void emergencyShutdown() noexcept override {
        LOG(ERROR, "EMERGENCY SHUTDOWN - Disabling all hardware");
        emergencyMode_ = true;
        
        disableHeater();
        disablePump();
        closeSteamValve();
        closeWaterValve();
        closeSolenoid();
    }
    
    void updateSafetyState(bool tankEmpty) noexcept {
        waterTankEmpty_ = tankEmpty;
    }
};
```

### 3. State Machine Error Handling

#### BaseState Error Guardian

```cpp
class BaseState : public MachineState {
public:
    // Final - cannot be overridden by derived states
    MachineState* checkTransitions(MachineStateContext& context) final {
        // Check critical errors FIRST (in priority order)
        
        // 1. Emergency stop - highest priority
        if (context.isEmergencyStop()) {
            context.logStateTransition(
                getStateId(), 
                MachineStateId::EMERGENCY_STOP, 
                "Emergency stop activated"
            );
            return getStateInstance(MachineStateId::EMERGENCY_STOP);
        }
        
        // 2. Sensor errors - critical safety issue
        if (context.hasSensorError()) {
            context.logStateTransition(
                getStateId(), 
                MachineStateId::SENSOR_ERROR, 
                "Sensor error detected"
            );
            return getStateInstance(MachineStateId::SENSOR_ERROR);
        }
        
        // 3. Water tank empty - only if state needs water
        if (!context.isWaterTankFull() && requiresWater()) {
            context.logStateTransition(
                getStateId(), 
                MachineStateId::WATER_TANK_EMPTY, 
                "Water tank empty"
            );
            return getStateInstance(MachineStateId::WATER_TANK_EMPTY);
        }
        
        // 4. Emergency temperature - overheating protection
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
    
protected:
    // Derived states override this for their specific transition logic
    virtual MachineState* checkSpecificTransitions(MachineStateContext& context) = 0;
    
    // States declare if they need water (default: true for safety)
    virtual bool requiresWater() const noexcept { return true; }
};
```

#### Example State Implementation

```cpp
class BrewRunningState : public BaseState {
public:
    void onEntry(MachineStateContext& context) override {
        LOG(INFO, "Brew running - extraction started");
        context.enablePump();
        context.setPumpPressure(9.0);
        brewStartTime_ = millis();
    }
    
    void update(MachineStateContext& context) override {
        // No error checking needed - BaseState handles it!
        // Just focus on brewing logic
        
        double temp = context.getCurrentTemperature();
        float weight = context.getCurrentBrewWeight();
        float pressure = context.getCurrentPressure();
        
        LOGF(DEBUG, 
             "Brewing: temp=%.1f°C, weight=%.1fg, pressure=%.1fbar", 
             temp, weight, pressure);
    }
    
    void onExit(MachineStateContext& context) override {
        LOG(INFO, "Brew extraction finished");
        context.disablePump();
        context.closeSolenoid();
    }
    
protected:
    MachineState* checkSpecificTransitions(MachineStateContext& context) override {
        // Only check brew-specific transitions
        
        if (!context.isBrewActive()) {
            return getStateInstance(MachineStateId::BREW_FINISHED);
        }
        
        float targetWeight = context.getTargetBrewWeight();
        float currentWeight = context.getCurrentBrewWeight();
        if (currentWeight >= targetWeight) {
            return getStateInstance(MachineStateId::BREW_FINISHED);
        }
        
        if (millis() - brewStartTime_ > 60000) {
            context.logStateTransition(
                getStateId(), 
                MachineStateId::BREW_FINISHED, 
                "Brew timeout - safety limit"
            );
            return getStateInstance(MachineStateId::BREW_FINISHED);
        }
        
        return nullptr;
    }
    
    bool requiresWater() const noexcept override {
        return true;
    }
    
private:
    unsigned long brewStartTime_ = 0;
};
```

### 4. Error Codes

```cpp
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

class Error {
    ErrorCode code_;
    const char* message_;
    
public:
    Error(ErrorCode code, const char* message)
        : code_(code), message_(message) {}
        
    ErrorCode code() const noexcept { return code_; }
    const char* message() const noexcept { return message_; }
    
    bool isCritical() const noexcept {
        return code_ == ErrorCode::SENSOR_DISCONNECTED ||
               code_ == ErrorCode::SENSOR_FAULT ||
               code_ == ErrorCode::HARDWARE_FAILURE ||
               code_ == ErrorCode::EMERGENCY_STOP ||
               code_ == ErrorCode::EMERGENCY_TEMPERATURE;
    }
};
```

## File Structure

```
include/clevercoffee/
├── sensors/
│   ├── ISensor.h                    [NEW] Common sensor interface
│   ├── SensorCoordinator.h          [REFACTORED] Manages all sensors
│   └── SensorManager.h              [DEPRECATED] Will be removed
├── hardware/
│   ├── tempsensors/
│   │   ├── TempSensor.h            [REFACTORED] Implement ISensor
│   │   └── TempSensorDallas.h      [REFACTORED] Add async pattern
│   ├── scales/
│   │   ├── Scale.h                 [REFACTORED] Implement ISensor
│   │   ├── HX711Scale.h            [REFACTORED] Add async pattern
│   │   └── BluetoothScale.h        [REFACTORED] Add async pattern
│   ├── HardwareManager.h           [REFACTORED] Add high-level control
│   └── IHardwareContext.h          [NEW] Hardware control interface
├── state/
│   ├── BaseState.h                 [NEW] Error guardian base class
│   ├── MachineState.h              [REFACTORED] Simplified interface
│   ├── MachineStateContext.h       [REFACTORED] Add hardware delegation
│   └── states/
│       ├── BrewStates.h            [REFACTORED] Inherit from BaseState
│       ├── ErrorStates.h           [REFACTORED] Inherit from BaseState
│       └── ...                     [REFACTORED] All states updated
├── errors/
│   ├── ErrorCodes.h                [NEW] Standardized error codes
│   └── Expected.h                  [EXISTS] Already implemented
├── scaleHandler.h                  [DEPRECATED] Will be removed
└── GlobalState.h                   [DEPRECATED] Will be removed
```

## Migration Strategy

### Phase 1: Foundation (Non-Breaking)
- Create `ISensor.h` interface
- Create `ErrorCodes.h` with all error codes
- Create `BaseState.h` base class
- Create `IHardwareContext.h` interface
- **Verify**: Code still compiles and works

### Phase 2: Sensor Refactoring
- Refactor `TempSensorDallas` to implement `ISensor`
- Refactor `HX711Scale` to implement `ISensor`
- Refactor `BluetoothScale` to implement `ISensor`
- Create new `SensorCoordinator` with caching/timeout logic
- Migrate sensor update logic from `SensorManager` to `SensorCoordinator`
- **Verify**: Sensors read correctly, timeouts work

### Phase 3: Hardware Control Refactoring
- Add high-level methods to `HardwareManager` (enablePump, etc.)
- Make `HardwareManager` implement `IHardwareContext`
- Add safety checks to `HardwareManager` methods
- **Verify**: Hardware control works, safety checks active

### Phase 4: State Machine Refactoring
- Update `MachineStateContext` to delegate hardware control
- Update all states to inherit from `BaseState`
- Update all states to use high-level hardware methods
- Remove direct relay access from states
- **Verify**: State transitions work, error handling active

### Phase 5: Cleanup
- Remove `scaleHandler.cpp` and `scaleHandler.h`
- Remove `SensorManager` (replaced by `SensorCoordinator`)
- Remove `GlobalState.h` and `g_state`
- Update all remaining references
- **Verify**: Complete system works without old code

### Phase 6: Testing & Documentation
- Add unit tests for new components
- Add integration tests for sensor timeout handling
- Update architecture documentation
- Add migration guide for future changes
- **Verify**: All tests pass

## Breaking Changes (Intentional)

These changes are acceptable for clean modern architecture:

1. **States can no longer directly access relays**
   - Old: `context.getPumpRelay()->turnOn()`
   - New: `context.enablePump()`

2. **Sensor reads are now async**
   - Old: `bool success = tempSensor->updateTemperature()`
   - New: `auto result = tempSensor->tryGetValue()`

3. **No more global g_state**
   - Old: `g_state.sensors.temperature`
   - New: `context.getCurrentTemperature()`

4. **scaleHandler functions removed**
   - Old: `initScale()` global function
   - New: `sensorCoordinator.initializeScale()`

## Success Criteria

The refactoring is complete when:

1. ✅ Sensors return `Expected<T, Error>` with timeout handling
2. ✅ States use high-level hardware commands (no direct relay access)
3. ✅ BaseState checks critical errors before state logic
4. ✅ SensorCoordinator manages all sensor caching/timeouts
5. ✅ HardwareManager enforces safety checks
6. ✅ No global `g_state` variable
7. ✅ No `scaleHandler.cpp` file
8. ✅ Code compiles and coffee machine works correctly
9. ✅ Clear separation: sensors read, states decide, hardware executes

## Benefits

1. **Clear responsibilities**: Each component has one job
2. **Testability**: Easy to mock sensors, hardware, states independently
3. **Maintainability**: Changes to sensors don't affect states
4. **Safety**: Centralized error checking and hardware safety checks
5. **Predictability**: State machine never blocks on sensor reads
6. **Debuggability**: Clear flow from sensor read → cache → state decision → hardware action
7. **Modern C++**: Use of Expected<T>, interfaces, RAII, no globals

## Next Steps

1. Commit this design document
2. Create implementation plan with detailed tasks
3. Execute phases 1-6 in order
4. Test thoroughly after each phase
5. Update documentation as we go
