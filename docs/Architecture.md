# CleverCoffee System Architecture

## Overview

CleverCoffee is an embedded coffee machine control system built for ESP32 using the Arduino framework. The system manages temperature control, brewing processes, user interface, network connectivity, and safety monitoring through a modular, state-machine-based architecture.

## Design Principles

The architecture follows these core principles:

1. **Dependency Injection**: Explicit dependencies instead of global state
2. **Interface Segregation**: Depend on abstractions, not concrete implementations
3. **Testability**: Mock-friendly design for unit testing
4. **Thread Safety**: Atomic operations for shared state
5. **Type Safety**: Strong typing with enums and Expected<T> for error handling
6. **Separation of Concerns**: Clear boundaries between subsystems

## System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         Application                          │
│                    (State Machine Logic)                     │
└────────────────────────┬────────────────────────────────────┘
                         │
                         │ uses
                         │
┌────────────────────────▼────────────────────────────────────┐
│                     SystemContext                            │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │   Sensor     │  │   Network    │  │      UI      │      │
│  │ Coordinator  │  │ Coordinator  │  │ Coordinator  │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
                         │
                         │ implements
                         │
┌────────────────────────▼────────────────────────────────────┐
│                    Context Interfaces                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │  IHardware   │  │   IConfig    │  │ IState       │      │
│  │   Context    │  │   Context    │  │  Manager     │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
                         │
                         │ provides
                         │
┌────────────────────────▼────────────────────────────────────┐
│                      Hardware Layer                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │  Temp    │  │  Scale   │  │  Relays  │  │   LED    │   │
│  │  Sensor  │  │          │  │          │  │          │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## Dependency Injection Pattern

### Problem: Global State

Previously, the system used a global `g_state` variable that contained all shared state:

```cpp
// OLD APPROACH (anti-pattern)
struct GlobalState {
    bool temperatureUpdateRunning;
    bool scaleUpdateRunning;
    bool mqttConnected;
    bool wifiConnected;
    // ... many more fields ...
};

extern GlobalState g_state;

void someFunction() {
    g_state.temperatureUpdateRunning = true;  // Hidden dependency!
}
```

**Problems:**
- Hidden dependencies (hard to see what functions use)
- Difficult to test (can't inject mocks)
- Tight coupling between components
- Thread safety issues

### Solution: Explicit Dependencies

The new architecture uses explicit dependency injection:

```cpp
// NEW APPROACH (dependency injection)
class SystemContext {
public:
    SensorCoordinator& sensorCoordinator() noexcept {
        return sensorCoordinator_;
    }

    NetworkCoordinator& networkCoordinator() noexcept {
        return networkCoordinator_;
    }

    // ... explicit accessors ...
};

class TemperatureController {
    SensorCoordinator& sensorCoord_;  // Explicit dependency

public:
    TemperatureController(SensorCoordinator& coord)
        : sensorCoord_(coord) {}  // Injected in constructor

    void startReading() {
        sensorCoord_.startTemperatureUpdate();  // Clear dependency
    }
};
```

**Benefits:**
- Dependencies are visible in function signatures
- Easy to inject test doubles/mocks
- Clear ownership and lifetime
- Better encapsulation

## Coordinator Pattern

Coordinators manage cross-cutting concerns that previously lived in global state. They provide:

1. **Thread-safe state management** using `std::atomic`
2. **Clear separation of concerns** (sensor, network, UI)
3. **Testable interfaces** (easy to mock)

### SensorCoordinator

Manages sensor operation state:

```cpp
SystemContext& ctx = ...;

// Start sensor operation
ctx.sensorCoordinator().startTemperatureUpdate();

// Check if operation in progress
if (ctx.sensorCoordinator().isTemperatureUpdateRunning()) {
    // Handle concurrent access
}

// Complete operation
ctx.sensorCoordinator().stopTemperatureUpdate();
```

### NetworkCoordinator

Tracks network connection state:

```cpp
// Update connection state
ctx.networkCoordinator().setWifiConnected(true);
ctx.networkCoordinator().setMqttConnected(true);

// Track connection attempts
ctx.networkCoordinator().incrementMqttConnectionAttempts();
if (success) {
    ctx.networkCoordinator().resetMqttConnectionAttempts();
}

// Check state
if (ctx.networkCoordinator().isMqttConnected()) {
    // Send MQTT message
}
```

### UICoordinator

Manages display refresh and sleep:

```cpp
// Request refresh from interrupt/timer
ctx.uiCoordinator().requestRefresh();

// Main loop checks and updates
if (ctx.uiCoordinator().needsRefresh()) {
    updateDisplay();
    ctx.uiCoordinator().clearRefresh();
}

// Control sleep behavior
ctx.uiCoordinator().setAutoSleep(true);
```

## Interface-Based Design

Interfaces (abstract classes) break circular dependencies and improve testability.

### IHardwareContext

Provides hardware abstraction for states:

**Problem**: Circular dependency between State and Context
- State needs to access hardware → needs Context
- Context contains State instances → needs State
- Circular dependency!

**Solution**: IHardwareContext interface
- State depends only on interface (not concrete Context)
- Concrete Context implements interface
- Dependency points in one direction only

```cpp
class IHardwareContext {
public:
    virtual double getCurrentTemperature() const noexcept = 0;
    virtual Relay* getHeaterRelay() noexcept = 0;
    virtual void updateHardware() noexcept = 0;
    // ... pure virtual methods ...
};

// State uses interface
class HeatState : public MachineState {
    void execute(IHardwareContext& hw) override {
        double temp = hw.getCurrentTemperature();  // Abstract interface
        // ... use temperature ...
    }
};

// Concrete context implements interface
class MachineContext : public IHardwareContext {
    double getCurrentTemperature() const noexcept override {
        return tempSensor_->read();  // Actual hardware access
    }
};
```

**Benefits:**
- No circular dependencies
- Easy to mock for testing:
  ```cpp
  class MockHardwareContext : public IHardwareContext {
      double getCurrentTemperature() const noexcept override {
          return 95.0;  // Test value
      }
  };
  ```

### IConfigContext

Abstracts configuration access:

```cpp
class IConfigContext {
public:
    virtual double getBrewSetpoint() const noexcept = 0;
    virtual double getPidKp() const noexcept = 0;
    // ... configuration methods ...
};

// Usage in state
class BrewState : public MachineState {
    void execute(IConfigContext& config) override {
        double setpoint = config.getBrewSetpoint();
        // ... use setpoint ...
    }
};
```

### IStateManager

Abstracts state machine operations:

```cpp
class IStateManager {
public:
    virtual void transitionTo(MachineState& newState) = 0;
    virtual bool hasStateTimeoutElapsed(unsigned long timeoutMs) const noexcept = 0;
    // ... state management methods ...
};

// Usage in state
class PreheatState : public MachineState {
    void execute(IStateManager& manager) override {
        if (manager.hasStateTimeoutElapsed(5000)) {
            manager.transitionTo(brewState);
        }
    }
};
```

## State Machine Overview

The system uses a hierarchical state machine to manage the coffee brewing process:

### State Hierarchy

```
MachineState (base)
├── InitState
├── IdleState
├── HeatState
├── BrewState
│   ├── PreInfusionState
│   └── ExtractionState
├── SteamState
└── ErrorState
```

### State Transitions

States transition based on:
1. **User input** (button presses, commands)
2. **Timeouts** (brew time, pre-infusion time)
3. **Sensor readings** (temperature, weight)
4. **Error conditions** (sensor failure, empty tank)

### State Implementation Pattern

```cpp
class BrewState : public MachineState {
public:
    // Called when entering the state
    void onEnter(IStateManager& manager) override {
        startTime_ = millis();
        hw_.getPumpRelay()->turnOn();
    }

    // Called repeatedly while in state
    void execute(IStateManager& manager) override {
        double weight = hw_.getWeight();
        double time = (millis() - startTime_) / 1000.0;

        if (time >= config_.getTargetBrewTime()) {
            manager.transitionTo(extractState);
        }
    }

    // Called when exiting the state
    void onExit(IStateManager& manager) override {
        hw_.getPumpRelay()->turnOff();
    }

private:
    unsigned long startTime_;
};
```

## Error Handling

The system uses type-safe error handling without exceptions:

### Expected<T> Type

Represents success or failure:

```cpp
Expected<double, Error> readTemperature() {
    if (sensorFailed) {
        return Error(ErrorCode::SENSOR_READ_FAILED,
                     "Temperature sensor timeout");
    }
    return 95.0;  // Success value
}

// Usage
auto result = readTemperature();
if (result) {
    double temp = result.value();
    Serial.println(temp);
} else {
    Error err = result.error();
    Serial.printf("Error: %s\n", err.message().c_str());
}
```

### Benefits over Exceptions

- **Zero overhead**: No stack unwinding cost
- **Explicit**: Error handling visible in type signature
- **ESP32 compatible**: Limited exception support on ESP32
- **Predictable**: No surprise control flow

## Component Interaction

### Initialization Flow

```
1. Create SystemContext
2. Initialize hardware components
3. Create and configure states
4. Initialize state machine with initial state
5. Mark system ready
6. Enter main loop
```

### Main Loop

```cpp
void loop() {
    // 1. Read sensors
    sensorManager.update();

    // 2. Update hardware outputs
    hardwareContext.updateHardware();

    // 3. Execute current state
    stateMachine.execute();

    // 4. Handle UI refresh
    if (uiContext.needsRefresh()) {
        display.update();
        uiContext.clearRefresh();
    }

    // 5. Handle network
    networkManager.update();
}
```

### Brewing Process Example

```
1. User presses BREW button
   └─> Transition from IdleState to HeatState

2. HeatState executes
   ├─> Check temperature via IHardwareContext
   ├─> Control heater relay via IHardwareContext
   └─> When temp reached, transition to BrewState

3. BrewState executes
   ├─> Start pre-infusion (low pressure)
   ├─> Wait for pre-infusion time (from IConfigContext)
   ├─> Start full extraction
   ├─> Monitor weight from scale
   ├─> Monitor brew time
   └─> When complete, transition to IdleState
```

## Testing Strategy

The architecture enables comprehensive testing:

### Unit Testing with Mocks

```cpp
// Mock interface implementation
class MockHardwareContext : public IHardwareContext {
public:
    double tempToReturn_{95.0};
    bool tankEmpty_{false};

    double getCurrentTemperature() const noexcept override {
        return tempToReturn_;
    }

    bool isWaterTankEmpty() const noexcept override {
        return tankEmpty_;
    }
};

// Test state behavior
TEST(BrewState, HandlesEmptyTank) {
    MockHardwareContext mockHW;
    mockHW.tankEmpty_ = true;

    BrewState state;
    state.execute(mockHW);

    EXPECT_TRUE(state.transitionedToError());
}
```

### Integration Testing

Real hardware contexts can test component integration:
- Test state transitions with actual sensors
- Test timing with real millis()
- Test PID controller with heater element

## Thread Safety

The system uses atomic operations for shared state:

```cpp
class SensorCoordinator {
    std::atomic<bool> temperatureUpdateRunning_{false};

public:
    void startTemperatureUpdate() noexcept {
        temperatureUpdateRunning_.store(true, std::memory_order_relaxed);
    }

    bool isTemperatureUpdateRunning() const noexcept {
        return temperatureUpdateRunning_.load(std::memory_order_relaxed);
    }
};
```

**Note**: ESP32 Arduino framework is single-threaded (one core for app, one for WiFi).
The atomic operations provide:
- Signal fencing (interrupt safety)
- Future-proofing for potential multi-threading
- Clear documentation of shared mutable state

## Memory Management

The architecture minimizes dynamic memory allocation:

- **Stack allocation preferred**: Contexts and coordinators on stack
- **Smart pointers**: Used where heap allocation is necessary
- **No raw new/delete**: Use std::unique_ptr, std::shared_ptr
- **Reference passing**: Avoid copies, use const& where appropriate

## Extensibility

The architecture supports easy extension:

### Adding a New Coordinator

```cpp
// 1. Define coordinator
class BrewingCoordinator {
    std::atomic<double> currentWeight_;
    // ... brewing state ...
};

// 2. Add to SystemContext
class SystemContext {
    BrewingCoordinator brewingCoordinator_;
    // ... accessor ...
};

// 3. Use in components
class BrewController {
    BrewingCoordinator& brewCoord_;
    // ... use brewing coordinator ...
};
```

### Adding a New State

```cpp
class CustomBrewState : public MachineState {
public:
    CustomBrewState(IHardwareContext& hw, IConfigContext& config)
        : hw_(hw), config_(config) {}

    void execute(IStateManager& manager) override {
        // Custom brewing logic
        if (customCondition) {
            manager.transitionTo(nextState);
        }
    }

private:
    IHardwareContext& hw_;
    IConfigContext& config_;
};
```

## Design Decision Records

### Why Dependency Injection Over Singletons?

**Decision**: Use dependency injection instead of singleton pattern

**Rationale**:
- Singletons hide dependencies
- Difficult to test (global state)
- Unclear lifetime management
- Violates Single Responsibility Principle

### Why Interfaces Over Concrete Classes?

**Decision**: Depend on interfaces (IHardwareContext, IConfigContext, IStateManager)

**Rationale**:
- Breaks circular dependencies
- Enables mocking for testing
- Allows implementation changes without affecting clients
- Follows Dependency Inversion Principle

### Why Expected<T> Over Exceptions?

**Decision**: Use Expected<T, Error> for error handling instead of exceptions

**Rationale**:
- ESP32 has limited exception support
- Zero runtime overhead
- Explicit error handling in type signature
- Better for embedded systems
- More predictable control flow

### Why Coordinators Over Global State?

**Decision**: Use coordinator classes instead of global state struct

**Rationale**:
- Thread safety through atomics
- Clear separation of concerns
- Testable interfaces
- Explicit dependencies
- Better encapsulation

## Future Improvements

### Potential Enhancements

1. **Add monadic operations to Expected<T>**:
   ```cpp
   Expected<double, Error> result = readSensor()
       .and_then([](double temp) { return validateTemp(temp); })
       .transform([](double temp) { return convertToFahrenheit(temp); });
   ```

2. **Add state history tracking**:
   ```cpp
   std::vector<MachineStateId> history = manager.getStateHistory();
   ```

3. **Add state transition guards**:
   ```cpp
   bool canTransition(MachineStateId from, MachineStateId to) const;
   ```

4. **Add configuration validation**:
   ```cpp
   Expected<void, Error> validateConfig(const Config& config);
   ```

5. **Add performance monitoring**:
   ```cpp
   class PerformanceMonitor {
       void recordStateTime(MachineStateId state, unsigned long duration);
       void recordSensorReadTime(const char* sensor, unsigned long duration);
   };
   ```

## References

- [C++ Core Guidelines](https://isocpp.github.io/CppCoreGuidelines/)
- [Dependency Injection in C++](https://herbsutter.com/2016/10/30/know-your-std-dependency-injection/)
- [State Machine Design Pattern](https://www.boost.org/doc/libs/release/libs/statechart/)
- [Expected<T, E> Proposal (P0323)](https://wg21.link/P0323)
