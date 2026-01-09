# Global State Complete Elimination Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete elimination of all remaining g_state references (226 active references) except intentional 24 in isr.h/SystemUtils.h/embeddedWebserver.h/demo code.

**Architecture:** 
- Replace all g_state accesses with dependency injection through SystemContext
- Use coordinator pattern for cross-cutting concerns (sensors, network, display)
- Implement value-object pattern for immutable state snapshots
- Create specialized interfaces for data access (read-only vs read-write)

**Tech Stack:** Modern C++17, SystemContext pattern, Coordinators, Dependency Injection

---

## EXECUTIVE SUMMARY

### Current Status
- **Total g_state references analyzed**: 247 (excluding .pio/)
- **Actionable references**: 226 (including display files which have 96 refs)
- **Intentional references**: 24 (isr.h, embeddedWebserver.h, SystemUtils.h, examples)
- **Refactoring target**: 226 → 0 (complete elimination)

### Reference Breakdown by Category
| Category | Count | Refactoring Strategy |
|----------|-------|----------------------|
| **process** | 62 | Inject ProcessController or create ProcessDataProvider |
| **hardware** | 286 | Inject HardwareContext references |
| **sensors** | 62 | Inject SensorCoordinator |
| **machine** | 85 | Pass MachineStateContext explicitly |
| **network** | 59 | Inject NetworkCoordinator |
| **handlers** | 22 | Inject handler references via context |
| **coordination** | 19 | Use coordinator methods directly |
| **timing** | 13 | Create TimingProvider or inject timing state |
| **standby** | 16 | Inject StandbyCoordinator |
| **display** | 4 | Use DisplayManager methods |

### Expected Impact
✅ **Testability**: All components can be tested in isolation with mock dependencies  
✅ **Maintainability**: Clear dependency graph with explicit parameter passing  
✅ **Thread Safety**: Eliminate global mutable state races  
✅ **Type Safety**: Compiler enforces correct object lifetime management  
✅ **Performance**: No runtime overhead, potential for better optimization

---

## FILE-BY-FILE ANALYSIS & REFACTORING APPROACH

### Priority 1: CRITICAL - Foundation Files (15 files)
These files establish patterns and are dependencies for others.

---

### File 1: `include/clevercoffee/isr.h` (13 refs - INTENTIONAL, NO CHANGE)
**Status**: ✅ Keep all references (ISR context, cannot use DI)  
**Justification**: Interrupt service routines cannot use dependency injection. These 13 references are legitimate and expected.

---

### File 2: `include/clevercoffee/embeddedWebserver.h` (16 refs - INTENTIONAL, NO CHANGE)
**Status**: ✅ Keep all references (legacy webserver compatibility)  
**Justification**: Legacy webserver code with permanent g_state dependency. Safe to keep isolated.

---

### File 3: `include/clevercoffee/utils/SystemUtils.h` (11 refs - INTENTIONAL, REVIEW)
**Current**: Mostly display coordination and timing  
**Action**: Review which refs are truly necessary; refactor display/timing to use injected services

---

### File 4: `src/control/ProcessController.cpp` (46 refs)

**Category Breakdown**:
- `g_state.process.*`: 30 refs (temperature, pidOutput, setpoint, currBrewTime, brewPidDisabled, etc.)
- `g_state.pid->*`: 14 refs (PID object access, method calls)
- `g_state.machine.*`: 2 refs (state machine checks)

**Current Pattern Analysis**:
```cpp
// LINE 59-61: Read from global to local
temperature_ = g_state.process.temperature;
pidOutput_ = g_state.process.pidOutput;
setpoint_ = g_state.process.setpoint;

// LINE 80: Write to global
g_state.process.temperature = temperature_;

// LINE 143: Call PID object via global pointer
g_state.pid->Compute();

// LINE 252: Check state in conditional
if (...|| g_state.process.brewPidDisabled)
```

**Refactoring Approach**:

**Phase 1**: Create ProcessDataProvider interface
```cpp
// NEW FILE: include/clevercoffee/context/ProcessDataProvider.h
class ProcessDataProvider {
public:
    virtual ~ProcessDataProvider() = default;
    
    // Read-only accessors
    virtual double getTemperature() const = 0;
    virtual double getSetpoint() const = 0;
    virtual double getPidOutput() const = 0;
    virtual double getCurrentBrewTime() const = 0;
    virtual bool isBrewPidDisabled() const = 0;
    virtual double getTotalTargetBrewTime() const = 0;
    
    // Write accessors
    virtual void setTemperature(double temp) = 0;
    virtual void setPidOutput(double output) = 0;
    virtual void setSetpoint(double setpoint) = 0;
    virtual void setCurrentBrewTime(double time) = 0;
    virtual void setBrewPidDisabled(bool disabled) = 0;
    virtual void setTotalTargetBrewTime(double time) = 0;
};
```

**Phase 2**: Create implementation that wraps GlobalState temporarily
```cpp
// NEW FILE: src/context/GlobalStateProcessProvider.cpp
class GlobalStateProcessProvider : public ProcessDataProvider {
private:
    GlobalState& state_;
public:
    explicit GlobalStateProcessProvider(GlobalState& state) : state_(state) {}
    
    double getTemperature() const override { return state_.process.temperature; }
    void setTemperature(double temp) override { state_.process.temperature = temp; }
    // ... other methods
};
```

**Phase 3**: Inject ProcessDataProvider into ProcessController
```cpp
// MODIFIED: ProcessController constructor
class ProcessController {
private:
    ProcessDataProvider& processData_;
    PID* pidController_;
    
public:
    ProcessController(ProcessDataProvider& data, PID* pid, ...)
        : processData_(data), pidController_(pid), ... {}
    
    void updateTemperature(double temp) {
        // BEFORE: g_state.process.temperature = temperature_;
        // AFTER:
        processData_.setTemperature(temp);
    }
};
```

**Impact Analysis**:
- ✅ Removes 30 direct g_state.process references
- ⚠️ Keeps PID pointer injection (will be handled in separate refactoring)
- ✅ Maintains backward compatibility during transition
- ✅ Enables unit testing with mock ProcessDataProvider

**Estimated Effort**: 2 hours

**Dependencies**: 
- PID controller refactoring (separate phase)
- SystemInitializer changes (to inject provider)

---

### File 5: `src/core/SystemInitializer.cpp` (29 refs)

**Category Breakdown**:
- `g_state.process.*`: 18 refs (temperature, PID params aggKi/aggKd, setpoint)
- `g_state.network.*`: 3 refs (WiFi/WebServer manager pointers)
- `g_state.pid->*`: 5 refs (PID object setup)
- `g_state.coordination.*`: 2 refs (processController registration)

**Current Pattern**:
```cpp
// LINE 193-203: PID initialization with g_state fields
pidController_ = std::make_unique<PID>(
    &g_state.process.temperature,
    &g_state.process.pidOutput,
    &g_state.process.setpoint,
    ...
);
g_state.pid = pidController_.get();  // LINE 203

// LINE 309: Register manager pointers
g_state.network.cleverCoffeeWiFiManager = cleverCoffeeWiFiManager_.get();
g_state.network.webServerManager = webServerManager_.get();

// LINE 508-515: Calculate derived PID parameters
g_state.process.aggKi = ...;
g_state.process.aggKd = ...;
```

**Refactoring Approach**:

**Phase 1**: Create PIDInputProvider (wraps process state for PID input)
```cpp
// NEW FILE: include/clevercoffee/context/PIDInputProvider.h
class PIDInputProvider {
public:
    virtual ~PIDInputProvider() = default;
    virtual double* getInputPtr() = 0;
    virtual double* getOutputPtr() = 0;
    virtual double* getSetpointPtr() = 0;
};

// Implementation using ProcessDataProvider
class ProcessPIDInputProvider : public PIDInputProvider {
private:
    ProcessDataProvider& provider_;
    // Storage for pointers PID needs
    double input_, output_, setpoint_;
public:
    double* getInputPtr() override { /* sync from provider */ return &input_; }
    // ...
};
```

**Phase 2**: Create NetworkComponentRegistry
```cpp
// NEW FILE: include/clevercoffee/context/NetworkComponentRegistry.h
class NetworkComponentRegistry {
public:
    void registerWiFiManager(CleverCoffeeWiFiManager* mgr) { wifiMgr_ = mgr; }
    void registerWebServerManager(WebServerManager* mgr) { webMgr_ = mgr; }
    // ... getters
private:
    CleverCoffeeWiFiManager* wifiMgr_ = nullptr;
    WebServerManager* webMgr_ = nullptr;
};
```

**Phase 3**: Modify SystemInitializer
```cpp
// Instead of:
// g_state.network.cleverCoffeeWiFiManager = cleverCoffeeWiFiManager_.get();

// Use:
systemContext_->networkComponentRegistry().registerWiFiManager(
    cleverCoffeeWiFiManager_.get());
```

**Impact Analysis**:
- ✅ Removes 3 direct manager pointer registrations
- ✅ Removes 18 process state direct writes
- ⚠️ Keeps PID object pointer injection (Phase 2 refactoring)
- ✅ Better initialization ordering

**Estimated Effort**: 2.5 hours

**Dependencies**: 
- ProcessDataProvider implementation (File 4)
- NetworkComponentRegistry creation
- PID refactoring (Phase 2)

---

### File 6: `include/clevercoffee/display/ModernDisplayTemplate.h` (27 refs)

**Category Breakdown**:
- `g_state.process.*`: 10 refs (temperature, setpoint, pidOutput, currBrewTime)
- `g_state.pid->Get*()`: 3 refs (GetKp, GetKi, GetKd)
- `g_state.sensors.*`: 1 ref (currPumpOnTime)
- `g_state.coordination.*`: 1 ref (displayBufferReady flag)
- `g_state.timing.*`: 1 ref (isrCounter for animation)

**Current Pattern**:
```cpp
// LINE 40: Set coordination flag
g_state.coordination.displayBufferReady = true;

// LINE 78: Read temperature for display
CleverCoffee::getGlobalSystemContext()->hardwareContext().display()
    ->print(g_state.process.temperature, 1);

// LINE 101-111: Read PID values for display
CleverCoffee::getGlobalSystemContext()->hardwareContext().display()
    ->print(g_state.pid->GetKp(), 0);
```

**Refactoring Approach**:

**Phase 1**: Create DisplayDataSnapshot (immutable value object)
```cpp
// NEW FILE: include/clevercoffee/display/DisplayDataSnapshot.h
struct DisplayDataSnapshot {
    // Process data
    double currentTemperature = 0.0;
    double setpointTemperature = 0.0;
    double pidOutputPercent = 0.0;
    double currentBrewTime = 0.0;
    double targetBrewTime = 0.0;
    
    // PID tuning values
    double pidKp = 0.0;
    double pidKi = 0.0;
    double pidKd = 0.0;
    
    // Sensor data
    double pumpOnTime = 0.0;
    float inputPressure = 0.0;
    double brewWeight = 0.0;
    
    // Timing/animation
    unsigned int isrCounter = 0;
    
    // Flags
    bool displayBufferReady = false;
};
```

**Phase 2**: Create DisplayDataProvider interface
```cpp
// NEW FILE: include/clevercoffee/display/DisplayDataProvider.h
class DisplayDataProvider {
public:
    virtual ~DisplayDataProvider() = default;
    
    // Get immutable snapshot of all display data
    virtual DisplayDataSnapshot getSnapshot() const = 0;
    
    // Coordinate with display system
    virtual void markDisplayBufferReady() = 0;
};
```

**Phase 3**: Modify ModernDisplayTemplate to use snapshots
```cpp
// BEFORE:
void printPage() {
    g_state.coordination.displayBufferReady = true;
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()
        ->print(g_state.process.temperature, 1);
}

// AFTER:
void printPage(DisplayDataProvider& displayData) {
    displayData.markDisplayBufferReady();
    auto snapshot = displayData.getSnapshot();
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()
        ->print(snapshot.currentTemperature, 1);
}
```

**Impact Analysis**:
- ✅ Removes all 27 g_state references from header (critical for display perf)
- ✅ Snapshot pattern enables batch updates
- ✅ Immutable data improves thread safety
- ✅ Testable with mock snapshots

**Estimated Effort**: 2 hours

**Dependencies**:
- DisplayManager refactoring (to provide DisplayDataProvider)
- SensorCoordinator updates (to track pump times)

---

### File 7: `include/clevercoffee/display/displayCommon.h` (25 refs)

**Category Breakdown**:
- `g_state.process.*`: 12 refs (temperature, setpoint, currBrewTime, pidOutput)
- `g_state.sensors.*`: 6 refs (currBrewWeight, inputPressure, currPumpOnTime)
- `g_state.coordination.*`: 3 refs (displayBufferReady)
- `g_state.timing.*`: 1 ref (isrCounter)
- `g_state.hardware.display`: Implicit (accessed via context already)

**Current Pattern** (SIMILAR TO FILE 6):
```cpp
// Multiple display functions that read g_state directly
void displayTemperatureBar(int x, int y, int width) {
    // Read from g_state
    if (g_state.process.temperature > 50) { ... }
}
```

**Refactoring Approach**:

**SAME AS FILE 6** - Use DisplayDataSnapshot pattern

Since displayCommon.h and ModernDisplayTemplate.h are tightly coupled display files, they should be refactored together using the same snapshot pattern.

**Additional Pattern for displayCommon**:
```cpp
// Create display function overloads that accept snapshot
void displayTemperatureBar(int x, int y, int width, 
                          const DisplayDataSnapshot& snapshot) {
    if (snapshot.currentTemperature > 50) { ... }
}

// Deprecate old versions (gradually remove)
// void displayTemperatureBar(int x, int y, int width);
```

**Impact Analysis**:
- ✅ Removes 25 g_state references
- ✅ Parallel to File 6 refactoring
- ✅ Can be done simultaneously
- ✅ Enables batch display updates

**Estimated Effort**: 1.5 hours (parallel with File 6)

**Dependencies**:
- Same as File 6
- DisplayDataSnapshot (shared)

---

### File 8: `src/core/LoopManager.cpp` (10 refs)

**Category Breakdown**:
- `g_state.process.*`: 2 refs (temperature, setpoint for LED logic)
- `g_state.network.*`: 3 refs (wifiReconnects reset, offlineMode check)
- `g_state.pid->Get*()`: 2 refs (Kp, Ki, Kd in logging)
- `g_state.coordination.*`: 1 ref (unclear from initial scan)
- General logging: 2 refs

**Current Pattern**:
```cpp
// LINE 195-196: LED status based on temperature
if (g_state.process.temperature < g_state.process.setpoint - 1) {
    // LED control logic
}

// LINE 484: Reset WiFi counter
g_state.network.wifiReconnects = 0;

// LINE 509: Check offline mode
if (g_state.network.offlineMode) {
    // Handle offline
}
```

**Refactoring Approach**:

**Phase 1**: Extract LED control logic
```cpp
// NEW FILE: include/clevercoffee/ui/LEDController.h
class LEDController {
public:
    void updateBrewLEDStatus(const ProcessDataSnapshot& data) {
        bool nearSetpoint = fabs(data.currentTemperature - data.setpointTemperature) < 1.0;
        // Control LED
    }
};
```

**Phase 2**: Inject NetworkCoordinator instead of reading g_state
```cpp
// BEFORE:
if (g_state.network.offlineMode) { ... }

// AFTER:
if (systemContext_.networkCoordinator().isOfflineMode()) { ... }
```

**Phase 3**: Pass DisplayDataSnapshot for LED updates
```cpp
// MODIFIED LoopManager
class LoopManager {
private:
    LEDController ledController_;
    SystemContext& systemContext_;
    
public:
    void update() {
        auto snapshot = systemContext_.displayDataProvider().getSnapshot();
        ledController_.updateBrewLEDStatus(snapshot);
    }
};
```

**Impact Analysis**:
- ✅ Removes 3 network state direct reads
- ✅ Removes 2 temperature direct reads
- ✅ Delegates LED logic to specialized controller
- ✅ Better separation of concerns

**Estimated Effort**: 1.5 hours

**Dependencies**:
- LEDController new class
- NetworkCoordinator usage
- DisplayDataSnapshot (from Files 6-7)

---

### File 9: `src/network/WebServerManager.cpp` (9 refs)

**Category Breakdown**:
- `g_state.process.*`: 2 refs (setpoint write, pidEnabled write)
- `g_state.sensors.*`: 4 refs (scaleTareOn, scaleCalibrationOn - both read/write)
- General web API endpoints: 9 total

**Current Pattern**:
```cpp
// LINE 360: API endpoint sets setpoint
g_state.process.setpoint = newSetpoint;

// LINE 396: API endpoint toggles PID
g_state.process.pidEnabled = newPidState;

// LINE 430: Scale tare command
g_state.sensors.scaleTareOn = true;

// LINE 444: Scale calibration command
g_state.sensors.scaleCalibrationOn = true;
```

**Refactoring Approach**:

**Phase 1**: Create CommandHandler interface
```cpp
// NEW FILE: include/clevercoffee/network/CommandHandler.h
class CommandHandler {
public:
    virtual ~CommandHandler() = default;
    
    // Process commands
    virtual void setSetpoint(double newSetpoint) = 0;
    virtual void setPidEnabled(bool enabled) = 0;
    virtual void requestScaleTare() = 0;
    virtual void requestScaleCalibration() = 0;
};
```

**Phase 2**: Create WebServerCommandHandler implementation
```cpp
// NEW FILE: src/network/WebServerCommandHandler.cpp
class WebServerCommandHandler : public CommandHandler {
private:
    ProcessController& processCtrl_;
    SensorCoordinator& sensorCoord_;
    
public:
    void setSetpoint(double newSetpoint) override {
        // Validate and set
        processCtrl_.setSetpoint(newSetpoint);
    }
    
    void requestScaleTare() override {
        sensorCoord_.requestScaleTare();
    }
};
```

**Phase 3**: Modify WebServerManager endpoints
```cpp
// BEFORE:
request->onSimpleData(httpEndpointSetSetpoint);
static void httpEndpointSetSetpoint(AsyncWebServerRequest* request, ...) {
    g_state.process.setpoint = newSetpoint;
}

// AFTER:
WebServerCommandHandler cmdHandler(processCtrl_, sensorCoord_);
request->onSimpleData([&](AsyncWebServerRequest* req, ...) {
    cmdHandler.setSetpoint(newSetpoint);
    req->send(200);
});
```

**Impact Analysis**:
- ✅ Removes 2 direct process state writes
- ✅ Removes 4 direct sensor state writes
- ✅ Centralizes all web API commands
- ✅ Enables validation/logging of commands
- ✅ Improves thread safety (command queuing possible)

**Estimated Effort**: 2 hours

**Dependencies**:
- CommandHandler interface
- ProcessController refactoring (File 4)
- SensorCoordinator enhancements

---

### File 10: `src/network/MQTTManager.cpp` (8 refs)

**Category Breakdown**:
- `g_state.sensors.*`: 4 refs (scaleTareOn, scaleCalibrationOn - read/write)
- `g_state.coordination.*`: 1 ref (hassioUpdateRunning flag)
- `g_state.network.*`: 2 refs (hassioFailed flag)

**Current Pattern** (SIMILAR TO FILE 9):
```cpp
// LINE 200: MQTT command sets scale tare
g_state.sensors.scaleTareOn = static_cast<bool>(value);

// LINE 208: MQTT command sets scale calibration
g_state.sensors.scaleCalibrationOn = static_cast<bool>(value);

// LINE 623: Set coordination flag
g_state.coordination.hassioUpdateRunning = true;

// LINE 631: Set network flag
g_state.network.hassioFailed = true;
```

**Refactoring Approach**:

**SAME AS FILE 9** - Create MQTTCommandHandler extending CommandHandler

**Phase 1**: Extend CommandHandler interface for MQTT commands
```cpp
// ADD TO: include/clevercoffee/network/CommandHandler.h
class CommandHandler {
    // ... existing interface
    
    // MQTT specific
    virtual void notifyHassioDiscoveryRunning(bool running) = 0;
    virtual void notifyHassioFailed(bool failed) = 0;
};
```

**Phase 2**: Implement MQTTCommandHandler
```cpp
// MODIFIED: WebServerCommandHandler → GenericCommandHandler
class GenericCommandHandler : public CommandHandler {
private:
    ProcessController& processCtrl_;
    SensorCoordinator& sensorCoord_;
    NetworkCoordinator& networkCoord_;
    UICoordinator& uiCoord_;
    
public:
    void notifyHassioDiscoveryRunning(bool running) override {
        uiCoord_.setHassioDiscoveryRunning(running);
    }
    
    void notifyHassioFailed(bool failed) override {
        networkCoord_.setHassioFailed(failed);
    }
};
```

**Phase 3**: Inject into MQTTManager
```cpp
// MODIFIED MQTTManager constructor
class MQTTManager {
private:
    CommandHandler& cmdHandler_;
    
public:
    MQTTManager(CommandHandler& cmd) : cmdHandler_(cmd) {}
    
    void onMqttMessage(const char* topic, ...) {
        if (strcmp(topic, "scaleTare") == 0) {
            cmdHandler_.requestScaleTare();
        }
    }
};
```

**Impact Analysis**:
- ✅ Removes 4 sensor state direct writes
- ✅ Removes 3 coordination/network state writes
- ✅ Centralizes all MQTT commands
- ✅ Reuses CommandHandler from File 9
- ✅ Better separation between MQTT logic and state mutations

**Estimated Effort**: 1.5 hours

**Dependencies**:
- Extended CommandHandler interface
- NetworkCoordinator enhancements
- UICoordinator enhancements

---

### File 11: `include/clevercoffee/Config.h` (8 refs - ALL COMMENTED)

**Status**: ✅ No action needed (all references are in commented-out example code)

These are documentation/examples showing how StateParamDef would access g_state. They're not executed.

---

### File 12: `src/main.cpp` (4 refs)

**Category Breakdown**:
- `g_state.coordination.processController`: 1 ref (registration)
- `g_state.process.*`: 3 refs (logging)

**Current Pattern**:
```cpp
// LINE 163: Register processor controller
g_state.coordination.processController = processController.get();

// LINE 211-219: Debug logging
LOGF(INFO, "Temp: %.2f, Setpoint: %.2f, PID: %.0f",
     g_state.process.temperature,
     g_state.process.setpoint,
     g_state.process.pidOutput);
```

**Refactoring Approach**:

**Phase 1**: Remove ProcessController registration (use systemContext)
```cpp
// BEFORE:
g_state.coordination.processController = processController.get();

// AFTER:
systemContext_->setProcessController(processController.get());
// (add this method to SystemContext if not present)
```

**Phase 2**: Replace logging with snapshot
```cpp
// BEFORE:
LOGF(INFO, "Temp: %.2f, Setpoint: %.2f, PID: %.0f",
     g_state.process.temperature,
     g_state.process.setpoint,
     g_state.process.pidOutput);

// AFTER:
auto snapshot = systemContext_->displayDataProvider().getSnapshot();
LOGF(INFO, "Temp: %.2f, Setpoint: %.2f, PID: %.0f",
     snapshot.currentTemperature,
     snapshot.setpointTemperature,
     snapshot.pidOutputPercent);
```

**Impact Analysis**:
- ✅ Removes 1 coordination registration
- ✅ Removes 3 process logging references
- ✅ Uses DisplayDataSnapshot (from Files 6-7)
- ✅ Minimal changes required

**Estimated Effort**: 0.5 hours

**Dependencies**:
- SystemContext enhancements
- DisplayDataSnapshot (Files 6-7)

---

### File 13: `include/clevercoffee/utils/helperUtils.h` (5 refs)

**Category Breakdown**:
- `g_state.sensors.*`: 5 refs (pressure filter state: inX, inOld, inSum)

**Current Pattern**:
```cpp
// LINE 99-102: Pressure filter calculations
g_state.sensors.inX   = static_cast<float>(input * 0.3f);
// Use old value
g_state.sensors.inSum = g_state.sensors.inX + g_state.sensors.inY;
g_state.sensors.inOld = g_state.sensors.inSum;
```

**Refactoring Approach**:

**Phase 1**: Create PressureFilter class (encapsulate state)
```cpp
// NEW FILE: include/clevercoffee/sensors/PressureFilter.h
class PressureFilter {
private:
    float inX_   = 0.0f;
    float inOld_ = 0.0f;
    float inSum_ = 0.0f;
    
public:
    float update(float input) {
        inX_ = input * 0.3f;
        inSum_ = inX_ + inOld_;  // inY would be calculated elsewhere
        inOld_ = inSum_;
        return inSum_;
    }
};
```

**Phase 2**: Inject filter into SensorCoordinator
```cpp
// MODIFIED: SensorCoordinator
class SensorCoordinator {
private:
    PressureFilter pressureFilter_;
    
public:
    void updatePressure(float rawPressure) {
        float filtered = pressureFilter_.update(rawPressure);
        // Store filtered value
    }
};
```

**Phase 3**: Remove direct g_state access from helperUtils.h
```cpp
// BEFORE:
void updatePressureFilter(float input) {
    g_state.sensors.inX = input * 0.3f;
    // ...
}

// AFTER:
// Function becomes part of PressureFilter class
// helperUtils becomes pure utilities without state access
```

**Impact Analysis**:
- ✅ Removes 5 sensor state direct writes
- ✅ Encapsulates filter logic
- ✅ Improves reusability
- ✅ Enables filter testing in isolation

**Estimated Effort**: 1 hour

**Dependencies**:
- PressureFilter class creation
- SensorCoordinator integration

---

### File 14: `include/clevercoffee/state/MachineStateContext.h` (1 ref)

**Status**: Minimal impact, likely just a single read check. Verify during implementation.

**Refactoring Approach**: If reading machine state, pass MachineStateContext explicitly instead of g_state.

**Estimated Effort**: 0.25 hours

---

### File 15: `include/clevercoffee/coordinators/UICoordinator.h` (1 ref)

**Status**: Review reference type (likely a registration or single access).

**Estimated Effort**: 0.25 hours

---

### File 16: `include/clevercoffee/coordinators/NetworkCoordinator.h` (1 ref)

**Status**: Review reference type.

**Estimated Effort**: 0.25 hours

---

### File 17: `include/clevercoffee/context/HardwareContext.h` (1 ref)

**Status**: Review reference type (likely initialization or verification).

**Estimated Effort**: 0.25 hours

---

## EXECUTION ORDER & DEPENDENCY GRAPH

### Phase 1: Foundation - Create New Abstractions (4 hours)
**Duration**: 1-2 days  
**Can run in parallel**: Yes (files are independent)

```
1.1 Create ProcessDataProvider interface
    └─ inputs: ProcessState struct analysis
    └─ outputs: ProcessDataProvider.h

1.2 Create DisplayDataSnapshot value object
    └─ inputs: All display file analysis
    └─ outputs: DisplayDataSnapshot.h

1.3 Create DisplayDataProvider interface
    └─ inputs: ModernDisplayTemplate.h, displayCommon.h analysis
    └─ outputs: DisplayDataProvider.h

1.4 Create CommandHandler interface
    └─ inputs: WebServer/MQTT endpoint analysis
    └─ outputs: CommandHandler.h

1.5 Create PressureFilter class
    └─ inputs: helperUtils.h analysis
    └─ outputs: PressureFilter.h, PressureFilter.cpp
```

**Parallel Execution**: All tasks can run simultaneously.

**Verification**:
- Code compiles with new interfaces
- No circular dependencies
- All interfaces are properly documented

---

### Phase 2: Core Component Refactoring (6 hours)
**Duration**: 2-3 days  
**Must run sequentially** (interdependencies)

```
2.1 Refactor ProcessController.cpp
    ├─ requires: ProcessDataProvider (Phase 1.1)
    ├─ requires: PID refactoring (later phase)
    └─ output: ProcessController uses dependency injection
    
2.2 Refactor SystemInitializer.cpp
    ├─ requires: ProcessDataProvider implementation (Phase 2.1)
    ├─ requires: NetworkComponentRegistry creation
    ├─ requires: PID refactoring
    └─ output: Manager registration via context
    
2.3 Refactor WebServerManager.cpp
    ├─ requires: CommandHandler interface (Phase 1.4)
    ├─ requires: ProcessController changes (Phase 2.1)
    └─ output: Web endpoints use command handler
    
2.4 Refactor MQTTManager.cpp
    ├─ requires: CommandHandler interface (Phase 1.4)
    ├─ requires: Extended handler for MQTT (Phase 2.3)
    └─ output: MQTT commands use command handler
```

**Sequential Dependency Chain**:
- 2.1 must complete before 2.2 (ProcessController injection needed)
- 2.1, 2.3 can run in parallel after Phase 1
- 2.4 depends on 2.3 completion

---

### Phase 3: Display Layer Refactoring (3 hours)
**Duration**: 1-2 days  
**Can run in parallel after Phase 1**

```
3.1 Refactor ModernDisplayTemplate.h
    ├─ requires: DisplayDataSnapshot (Phase 1.2)
    ├─ requires: DisplayDataProvider (Phase 1.3)
    └─ output: Template uses snapshot instead of g_state
    
3.2 Refactor displayCommon.h
    ├─ requires: DisplayDataSnapshot (Phase 1.2)
    ├─ requires: DisplayDataProvider (Phase 1.3)
    ├─ parallel: Can run with 3.1
    └─ output: All display functions use snapshot
    
3.3 Refactor LoopManager.cpp
    ├─ requires: LEDController creation
    ├─ requires: DisplayDataSnapshot
    ├─ requires: NetworkCoordinator usage
    └─ output: LED control delegated to LEDController
```

**Parallel Execution**: 3.1 and 3.2 can run simultaneously.

---

### Phase 4: Sensor & Utility Refactoring (2 hours)
**Duration**: 1 day

```
4.1 Refactor helperUtils.h
    ├─ requires: PressureFilter creation (Phase 1.5)
    ├─ requires: SensorCoordinator integration
    └─ output: helperUtils becomes pure utilities
    
4.2 Integrate PressureFilter into SensorCoordinator
    ├─ requires: PressureFilter class (Phase 1.5)
    └─ output: Pressure filtering encapsulated
```

**Sequential**: 4.1 should complete before 4.2, or run as single task.

---

### Phase 5: Integration & Cleanup (2 hours)
**Duration**: 1 day

```
5.1 Update main.cpp
    ├─ requires: ProcessController injection (Phase 2.1)
    ├─ requires: DisplayDataSnapshot (Phase 1.2)
    └─ output: main.cpp uses SystemContext
    
5.2 Verify remaining single-ref files
    ├─ files: MachineStateContext.h, UICoordinator.h, etc.
    └─ output: All minor files refactored
    
5.3 System-wide build test
    ├─ requires: All phases complete
    └─ output: Full compilation without warnings
    
5.4 Execute all unit tests
    ├─ requires: Test updates for new interfaces
    └─ output: All tests passing
```

---

## COMPREHENSIVE TASK BREAKDOWN (30+ SUBTASKS)

### Task 1: Create ProcessDataProvider Interface
**Files**:
- Create: `include/clevercoffee/context/ProcessDataProvider.h`

**Step 1: Write interface header**

```cpp
#pragma once

/**
 * @brief Read-write interface for process control data
 *
 * Provides abstraction over process state (temperature, PID output, setpoint, etc.)
 * allowing ProcessController and other components to work without direct g_state access.
 *
 * Implementation can delegate to GlobalState temporarily during migration,
 * or to proper state manager after refactoring.
 */
class ProcessDataProvider {
public:
    virtual ~ProcessDataProvider() = default;

    // ===== READ ACCESSORS =====
    /// @brief Get current measured temperature in Celsius
    virtual double getTemperature() const = 0;

    /// @brief Get target setpoint temperature in Celsius
    virtual double getSetpoint() const = 0;

    /// @brief Get current PID output (0-1000 scale typically)
    virtual double getPidOutput() const = 0;

    /// @brief Get elapsed brew time in milliseconds
    virtual double getCurrentBrewTime() const = 0;

    /// @brief Get target brew time in milliseconds
    virtual double getTotalTargetBrewTime() const = 0;

    /// @brief Check if brew PID is disabled
    virtual bool isBrewPidDisabled() const = 0;

    /// @brief Get previous input value for PID derivative calculation
    virtual double getPreviousInput() const = 0;

    // ===== WRITE ACCESSORS =====
    /// @brief Update temperature (typically called by sensor readings)
    virtual void setTemperature(double temp) = 0;

    /// @brief Update PID output value (called after PID computation)
    virtual void setPidOutput(double output) = 0;

    /// @brief Set target setpoint (called by user or controller)
    virtual void setSetpoint(double setpoint) = 0;

    /// @brief Update current brew time (called by timer)
    virtual void setCurrentBrewTime(double time) = 0;

    /// @brief Update target brew time
    virtual void setTotalTargetBrewTime(double time) = 0;

    /// @brief Set brew PID disabled state
    virtual void setBrewPidDisabled(bool disabled) = 0;

    /// @brief Update previous input for derivative calculations
    virtual void setPreviousInput(double input) = 0;

    // ===== BATCH OPERATIONS =====
    /// @brief Get snapshot of all process data (atomic read)
    virtual struct ProcessDataSnapshot getSnapshot() const = 0;
};

struct ProcessDataSnapshot {
    double temperature        = 0.0;
    double setpoint           = 0.0;
    double pidOutput          = 0.0;
    double currentBrewTime    = 0.0;
    double targetBrewTime     = 0.0;
    bool   brewPidDisabled    = false;
    double previousInput      = 0.0;
};
```

**Step 2: Create implementation wrapper**

Create: `src/context/GlobalStateProcessProvider.cpp`

```cpp
#include "clevercoffee/context/ProcessDataProvider.h"
#include "clevercoffee/GlobalState.h"

class GlobalStateProcessProvider : public ProcessDataProvider {
private:
    GlobalState& state_;

public:
    explicit GlobalStateProcessProvider(GlobalState& state) : state_(state) {}

    double getTemperature() const override {
        return state_.process.temperature;
    }

    double getSetpoint() const override {
        return state_.process.setpoint;
    }

    double getPidOutput() const override {
        return state_.process.pidOutput;
    }

    double getCurrentBrewTime() const override {
        return state_.process.currBrewTime;
    }

    double getTotalTargetBrewTime() const override {
        return state_.process.totalTargetBrewTime;
    }

    bool isBrewPidDisabled() const override {
        return state_.process.brewPidDisabled;
    }

    double getPreviousInput() const override {
        return state_.process.previousInput;
    }

    void setTemperature(double temp) override {
        state_.process.temperature = temp;
    }

    void setPidOutput(double output) override {
        state_.process.pidOutput = output;
    }

    void setSetpoint(double setpoint) override {
        state_.process.setpoint = setpoint;
    }

    void setCurrentBrewTime(double time) override {
        state_.process.currBrewTime = time;
    }

    void setTotalTargetBrewTime(double time) override {
        state_.process.totalTargetBrewTime = time;
    }

    void setBrewPidDisabled(bool disabled) override {
        state_.process.brewPidDisabled = disabled;
    }

    void setPreviousInput(double input) override {
        state_.process.previousInput = input;
    }

    ProcessDataSnapshot getSnapshot() const override {
        return ProcessDataSnapshot{
            .temperature = state_.process.temperature,
            .setpoint = state_.process.setpoint,
            .pidOutput = state_.process.pidOutput,
            .currentBrewTime = state_.process.currBrewTime,
            .targetBrewTime = state_.process.totalTargetBrewTime,
            .brewPidDisabled = state_.process.brewPidDisabled,
            .previousInput = state_.process.previousInput,
        };
    }
};
```

**Step 3: Verify compilation**

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50
```

**Expected**: No errors related to ProcessDataProvider

**Step 4: Commit**

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
git add include/clevercoffee/context/ProcessDataProvider.h src/context/GlobalStateProcessProvider.cpp
git commit -m "feat: create ProcessDataProvider interface for process state access"
```

---

### Task 2: Create DisplayDataSnapshot Value Object
**Files**:
- Create: `include/clevercoffee/display/DisplayDataSnapshot.h`

**Step 1: Write snapshot header with comprehensive documentation**

```cpp
#pragma once

#include <cstddef>

/**
 * @brief Immutable snapshot of all display-related data
 *
 * This value object captures the current state of all data needed for rendering
 * the display at a specific point in time. Using snapshots instead of accessing
 * g_state directly provides:
 *
 * 1. **Thread Safety**: Atomic read of all display data
 * 2. **Performance**: Single structure copy instead of multiple g_state accesses
 * 3. **Immutability**: Renderers cannot accidentally modify state
 * 4. **Testability**: Easy to mock display data in tests
 *
 * @note This is a data transfer object, not a model. It should be passed by const ref.
 */
struct DisplayDataSnapshot {
    // ===== PROCESS DATA =====
    double currentTemperature        = 0.0;   ///< Current measured temperature (°C)
    double setpointTemperature       = 0.0;   ///< Target setpoint (°C)
    double pidOutputPercent          = 0.0;   ///< Heater power (0-1000)
    double currentBrewTime           = 0.0;   ///< Elapsed brew time (ms)
    double targetBrewTime            = 0.0;   ///< Target brew duration (ms)
    bool   brewPidDisabled           = false; ///< Brew PID active state

    // ===== PID TUNING VALUES =====
    double pidKp                     = 0.0;   ///< Proportional gain
    double pidKi                     = 0.0;   ///< Integral gain
    double pidKd                     = 0.0;   ///< Derivative gain

    // ===== SENSOR DATA =====
    double pumpOnTime                = 0.0;   ///< Hot water pump runtime (ms)
    float  inputPressure             = 0.0f;  ///< Current pump pressure (bar)
    double brewWeight                = 0.0;   ///< Current shot weight (grams)

    // ===== TIMING/ANIMATION =====
    unsigned int isrCounter          = 0;     ///< ISR counter for animation sync

    // ===== COORDINATION FLAGS =====
    bool displayBufferReady          = false; ///< Display update ready flag
};
```

**Step 2: Verify header-only compilation**

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | grep -i "error" | head -5
```

**Expected**: No errors

**Step 3: Commit**

```bash
git add include/clevercoffee/display/DisplayDataSnapshot.h
git commit -m "feat: create DisplayDataSnapshot immutable value object"
```

---

### Task 3: Create DisplayDataProvider Interface
**Files**:
- Create: `include/clevercoffee/display/DisplayDataProvider.h`

**Implementation similar to Task 1**

---

### Task 4: Create CommandHandler Interface (for Web/MQTT)
**Files**:
- Create: `include/clevercoffee/network/CommandHandler.h`

**Key methods**:
```cpp
virtual void setSetpoint(double newSetpoint) = 0;
virtual void setPidEnabled(bool enabled) = 0;
virtual void requestScaleTare() = 0;
virtual void requestScaleCalibration() = 0;
virtual void notifyHassioDiscoveryRunning(bool running) = 0;
virtual void notifyHassioFailed(bool failed) = 0;
```

---

### Task 5: Create PressureFilter Class
**Files**:
- Create: `include/clevercoffee/sensors/PressureFilter.h`
- Create: `src/sensors/PressureFilter.cpp`

---

### Task 6: Refactor ProcessController.cpp (46 refs)

**Step 1: Update constructor to accept ProcessDataProvider**

```cpp
// BEFORE:
class ProcessController {
public:
    ProcessController(const Config& config) : config_(config) {}

// AFTER:
class ProcessController {
private:
    ProcessDataProvider& processData_;
    
public:
    ProcessController(ProcessDataProvider& data, const Config& config)
        : processData_(data), config_(config) {}
```

**Step 2: Replace all g_state.process reads/writes**

```cpp
// BEFORE (line 59-61):
temperature_ = g_state.process.temperature;
pidOutput_ = g_state.process.pidOutput;
setpoint_ = g_state.process.setpoint;

// AFTER:
auto snapshot = processData_.getSnapshot();
temperature_ = snapshot.temperature;
pidOutput_ = snapshot.pidOutput;
setpoint_ = snapshot.setpoint;
```

**Step 3: Replace all g_state.process writes**

```cpp
// BEFORE (line 80):
g_state.process.temperature = temperature_;

// AFTER:
processData_.setTemperature(temperature_);
```

**Step 4: Replace all g_state.process member reads in logic**

```cpp
// BEFORE (line 252):
if (...|| g_state.process.brewPidDisabled)

// AFTER:
if (... || processData_.isBrewPidDisabled())
```

**Step 5: Run tests to verify changes**

```bash
~/.platformio/penv/bin/pio run -e esp32_usb -s
```

**Step 6: Commit**

```bash
git add src/control/ProcessController.cpp include/clevercoffee/control/ProcessController.h
git commit -m "refactor: inject ProcessDataProvider into ProcessController"
```

---

### Task 7: Refactor SystemInitializer.cpp (29 refs)

Similar pattern to Task 6 - replace g_state accesses with injected dependencies.

---

### Task 8-9: Refactor Display Files (ModernDisplayTemplate.h, displayCommon.h - 52 refs total)

**For each display function:**

```cpp
// BEFORE:
void printTemperature(int x, int y) {
    display->print(g_state.process.temperature, 1);
}

// AFTER:
void printTemperature(int x, int y, const DisplayDataSnapshot& snapshot) {
    display->print(snapshot.currentTemperature, 1);
}
```

---

### Task 10: Refactor WebServerManager.cpp (9 refs)

Create GenericCommandHandler that implements CommandHandler interface, inject into WebServerManager.

---

### Task 11: Refactor MQTTManager.cpp (8 refs)

Reuse GenericCommandHandler from Task 10.

---

### Task 12: Refactor LoopManager.cpp (10 refs)

Create LEDController, update LoopManager to use it with DisplayDataSnapshot.

---

### Task 13: Refactor helperUtils.h (5 refs)

Integrate PressureFilter into SensorCoordinator, remove direct g_state access from helperUtils.

---

### Task 14: Refactor main.cpp (4 refs)

Update to use SystemContext instead of g_state for registration.

---

### Task 15-18: Remaining Single-Ref Files

Verify and refactor each single-reference file in the remaining list.

---

### Task 19: System Build Test

```bash
~/.platformio/penv/bin/pio run -e esp32_usb -s
```

Verify no build errors or warnings.

---

### Task 20: Unit Tests Verification

```bash
pytest test/ -v 2>&1 | tail -20
```

All tests should pass.

---

### Task 21: Create Transition Documentation

Document how old g_state was used and how new patterns replace it.

---

### Task 22: Final g_state Reference Count Verification

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
rg "g_state\." --type cpp --type h -n | grep -v ".pio/" | grep -v "isr.h" | grep -v "embeddedWebserver.h" | grep -v "SystemUtils.h" | grep -v "examples/" | wc -l
```

Expected output: <= 5 (only intentional references)

---

## RISK ASSESSMENT & MITIGATION

### Risk 1: Breaking Display Rendering
**Impact**: High (users see broken display)  
**Probability**: Medium (display logic is complex)

**Mitigation**:
1. Refactor display files last (after all data providers stable)
2. Create comprehensive display unit tests
3. Test rendering with actual display hardware
4. Use snapshot pattern for atomic data reads

---

### Risk 2: PID Controller Instability
**Impact**: High (system cannot reach setpoint)  
**Probability**: Low (PID is well-tested)

**Mitigation**:
1. Keep PID refactoring minimal
2. Test with existing PID test suite
3. Validate PID output values match previous behavior
4. Monitor temperature curves during testing

---

### Risk 3: Race Conditions in Coordinator Access
**Impact**: Medium (occasional incorrect reads)  
**Probability**: Low (coordinators use atomic ops)

**Mitigation**:
1. Use DisplayDataSnapshot for batch reads
2. Document thread-safe accessors in coordinators
3. Run ThreadSanitizer on test suite
4. Test under high-frequency updates

---

### Risk 4: Increased Binary Size
**Impact**: Low (might exceed ESP32 flash)  
**Probability**: Very Low (abstractions are efficient)

**Mitigation**:
1. Monitor binary size at each phase
2. Use inline virtual functions where possible
3. Leverage compiler optimizations
4. Test on actual ESP32 with OTA enabled

---

## SUCCESS CRITERIA & VALIDATION

### Acceptance Criteria

✅ **All g_state references eliminated** (except 24 intentional)
- Target: `rg "g_state\." --count | tail -1` returns 24

✅ **Build succeeds without warnings**
```bash
~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | grep -i warning
# Expected: No output (or warnings unrelated to g_state)
```

✅ **All unit tests pass**
```bash
pytest test/ --tb=short
# Expected: All tests PASSED
```

✅ **Display rendering works correctly**
- Temperature displays correctly
- PID values show accurate tuning info
- Brew timer updates properly
- Animations (blinking, progress bars) work

✅ **Web API endpoints respond**
```bash
curl -X POST http://espresso.local/api/setpoint \
  -H "Content-Type: application/json" \
  -d '{"value": 92.5}'
# Expected: 200 OK
```

✅ **MQTT commands execute**
- Scale tare from Home Assistant
- Scale calibration from Home Assistant
- PID tuning updates

✅ **Performance metrics maintained**
- Loop time < 100ms
- Display updates < 50ms
- No memory leaks (valgrind)
- No thread safety issues (ThreadSanitizer)

---

## ESTIMATED TIMELINE

| Phase | Description | Effort | Duration |
|-------|-------------|--------|----------|
| **1** | Create abstractions | 4 hrs | 1-2 days |
| **2** | Core component refactoring | 6 hrs | 2-3 days |
| **3** | Display layer refactoring | 3 hrs | 1-2 days |
| **4** | Sensor/utility refactoring | 2 hrs | 1 day |
| **5** | Integration & testing | 2 hrs | 1 day |
| **6** | Documentation & commit | 1 hr | 0.5 day |
| **TOTAL** | | **18 hrs** | **~6-8 days** |

**Critical Path**: Phase 1 → Phase 2.1 → Phase 2.2 → Phase 3 → Phase 5

---

## DEPENDENCY RESOLUTION ORDER

```
Phase 1 (Parallel):
  ├─ ProcessDataProvider interface ──→ Phase 2.1 (ProcessController)
  ├─ DisplayDataSnapshot ────────────┐
  ├─ DisplayDataProvider interface ──┼→ Phase 3 (Display refactoring)
  ├─ CommandHandler interface ───────→ Phase 2.3, 2.4 (Web/MQTT)
  └─ PressureFilter class ───────────→ Phase 4.1, 4.2 (Sensors)

Phase 2 (Sequential):
  2.1: ProcessController (ProcessDataProvider)
       ↓
  2.2: SystemInitializer (ProcessController + network registry)
       ↓
  2.3: WebServerManager (CommandHandler)
       ↓
  2.4: MQTTManager (CommandHandler from 2.3)

Phase 3 (Parallel, after Phase 1):
  ├─ ModernDisplayTemplate (DisplayDataSnapshot)
  └─ displayCommon (DisplayDataSnapshot)

Phase 4 (Sequential):
  4.1: helperUtils (PressureFilter)
       ↓
  4.2: SensorCoordinator integration

Phase 5 (Sequential):
  5.1: main.cpp (ProcessController from 2.1)
  5.2: Remaining single-ref files
  5.3: System build test
  5.4: Unit test execution
```

---

## IMPLEMENTATION NOTES

### Guidelines for Reviewers

1. **Each refactored file must**:
   - Remove all g_state direct references
   - Accept injected dependencies in constructor
   - Maintain identical behavior before/after
   - Include Doxygen comments on new methods

2. **Testing must verify**:
   - Unit tests pass for modified components
   - Integration tests pass for coordinator interactions
   - Display renders correctly (visual inspection)
   - Web API responds (curl tests)
   - MQTT publishes/subscribes (broker inspection)

3. **Code review should check**:
   - No new g_state references introduced
   - Thread safety maintained
   - Memory leaks absent (valgrind clean)
   - Performance not degraded

### Architecture Evolution

```
BEFORE (Monolithic):
┌─────────────────────────────────────┐
│         Global State (g_state)      │
│  ├─ process                         │
│  ├─ hardware                        │
│  ├─ sensors                         │
│  ├─ network                         │
│  └─ machine                         │
└─────────────────────────────────────┘
  ▲              ▲             ▲
  │              │             │
  └──────────┬───┴─────────┬───┘
             │             │
        [Component A] [Component B]


AFTER (Modular with DI):
┌──────────────────────────────────────────────────────┐
│              SystemContext                           │
│  ├─ SensorCoordinator ──┐                           │
│  ├─ NetworkCoordinator  ├─→ [ProcessDataProvider]   │
│  ├─ UICoordinator ──────┤                           │
│  └─ HardwareContext ────┤                           │
│                         └─→ [DisplayDataProvider]   │
└──────────────────────────────────────────────────────┘
                    ▲
                    │
      ┌─────────────┼─────────────┐
      │             │             │
 [Component A] [Component B] [Component C]
 (with injected (with injected (with injected
  dependencies) dependencies) dependencies)
```

This architecture enables:
- **Testing**: Mock providers for unit tests
- **Evolution**: Swap implementations without changing callers
- **Clarity**: Clear dependency graph
- **Performance**: No runtime overhead (all virtual calls inlined)

---

## NEXT STEPS

1. **Create a feature branch** for this work (already in fork)
2. **Review this plan** for feedback and adjustments
3. **Begin Phase 1** (create abstractions in parallel)
4. **Commit frequently** (each task is one commit)
5. **Test continuously** (build after each phase)
6. **Validate acceptance criteria** before considering complete

---

## REFERENCES & RELATED DOCUMENTS

- [2025-12-28-global-state-elimination-plan.md](./2025-12-28-global-state-elimination-plan.md) - Previous context analysis
- [PHASE_30_COMPLETION_FINAL.md](../PHASE_30_COMPLETION_FINAL.md) - Architectural decisions
- [GlobalState.h](../../include/clevercoffee/GlobalState.h) - Current g_state structure
- [SystemContext.h](../../include/clevercoffee/context/SystemContext.h) - Target architecture
