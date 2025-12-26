# CleverCoffee Project - Testable Business Logic Components

## Executive Summary

The CleverCoffee project contains well-structured business logic that can be extensively unit tested without hardware dependencies. The architecture separates concerns into distinct layers: configuration, state machine, process control, sensor management, and handlers. Most business logic is hardware-agnostic and suitable for unit testing.

---

## 1. STATE MACHINE LOGIC

### Location
- Headers: `include/clevercoffee/state/`
- Implementation: `src/state/`

### Key Components

#### 1.1 Machine State Enumeration (NOT YET FULLY TESTABLE)
**File**: `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/state/MachineStateIds.h`

**Key Enums**:
- `MachineStateId` - 15+ distinct states for brew, steam, hot water, backflush, emergency, standby, PID modes
- Helper functions: `isBrewState()`, `isHotWaterState()`, `isSteamState()`, `isBackflushState()`, `isManualFlushState()`

**Testable Logic**:
- State classification functions (constexpr, pure functions)
- State transition validity checks

**Methods**:
- `isBrewState(MachineStateId)` - Pure function, testable
- `isHotWaterState(MachineStateId)` - Pure function, testable
- `isSteamState(MachineStateId)` - Pure function, testable
- `isBackflushState(MachineStateId)` - Pure function, testable
- `isManualFlushState(MachineStateId)` - Pure function, testable

**Logic**: State categorization using enum ranges

#### 1.2 Base State Class (TESTABLE)
**File**: `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/state/BaseState.h`

**Template Parameters**:
- `StateId`: MachineStateId enum value
- `DerivedState`: Concrete state implementation

**Key Methods**:
```cpp
void onEntry(MachineStateContext& context)           // Logs entry and delegates to onEntryImpl
void onExit(MachineStateContext& context)            // Logs exit and delegates to onExitImpl
MachineState* checkTransitions(MachineStateContext&) // Common safety checks + specific transitions
MachineStateId getStateId() const                    // Returns StateId template parameter
const char* getStateName() const                     // Returns human-readable name
```

**Override Points in Derived Classes**:
- `onEntryImpl()` - Custom entry behavior (default: no-op)
- `onExitImpl()` - Custom exit behavior (default: no-op)
- `checkSpecificTransitions()` - State-specific transition logic (pure virtual)
- `update()` - Per-frame update logic (from MachineState interface)

**Testable Logic**:
- Emergency stop prioritization (highest priority transition)
- Sensor error detection → SENSOR_ERROR state transition
- Water tank empty → WATER_TANK_EMPTY state transition
- Common safety checks before delegating to state-specific transitions
- State ID and name management

**Dependencies** (hardware-agnostic):
- `MachineStateContext` - provides access to sensors and controllers
- `StateFactory` - state creation

#### 1.3 State Implementations (TESTABLE)

**Brew States**:
- `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/state/states/BrewStates.h`
- `/Users/marbaced/projects/forks/fork-clevercoffee/src/state/states/BrewStates.cpp` (146 lines)

Classes:
1. `BrewIdleState` - Waiting for brew start
   - Checks for `requestBrewStart` flag
   - Checks for brew switch activation
   - Checks for backflush activation
   - Transitions: → BREW_PREINFUSION, → BACKFLUSH_IDLE

2. `BrewPreinfusionState` - Pre-infusion phase
   - Checks for `requestBrewStop` flag
   - Checks for brew switch deactivation
   - Transitions: → BREW_IDLE

3. `BrewPreinfusionPauseState` - Blooming/pause phase
   - Similar checks as preinfusion
   - Transitions: → BREW_IDLE

4. `BrewRunningState` - Active brewing
   - Brew weight/time detection
   - Pressure monitoring
   - Transitions: → BREW_FINISHED, → BREW_IDLE

5. `BrewFinishedState` - Post-brew state
   - Timer for display
   - Cleanup transitions

**Testable Logic**:
- Flag-based state transitions
- Switch activation/deactivation handling
- State timing and transitions
- Weight-based brew stop conditions
- Time-based brew stop conditions

**Other State Groups**:
- **Hot Water States** (`HotWaterStates.h`) - Similar structure to brew
  - `HotWaterIdleState`
  - `HotWaterRunningState`
  - `HotWaterStoppedState`

- **Steam States** (`SteamStates.h`) - Steam mode management
  - `SteamIdleState`
  - `SteamRunningState`
  - `SteamStoppedState`

- **Backflush States** (`BackflushStates.h`) - Backflush cycles
  - `BackflushIdleState`
  - `BackflushFillingState`
  - `BackflushFlushingState`
  - `BackflushFinishedState`
  - **Testable**: Fill/flush timing, cycle counting

- **Error States** (`ErrorStates.h`) - Safety states
  - `SensorErrorState` - Handles sensor failures
  - `WaterTankEmptyState` - Water tank empty safety
  - `EepromErrorState` - Configuration storage errors
  - **Testable**: Error detection, safe shutdown logic

- **Emergency Stop** (`EmergencyStopState.h`)
  - `performEmergencyShutdown()` - Safety shutdown logic
  - `isEmergencyCleared()` - Recovery condition checking
  - `getRecoveryState()` - Determining recovery path
  - **Testable**: Shutdown sequence, recovery conditions

#### 1.4 Machine State Context (TESTABLE FACADE)
**File**: `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/state/MachineStateContext.h`

**Purpose**: Unified interface for state implementations to access all resources

**Sensor Data Access Methods** (can be mocked):
```cpp
double getCurrentTemperature() const
bool hasTemperatureError() const
bool isWaterTankFull() const
float getCurrentPressure() const
float getFilteredPressure() const
float getCurrentWeight() const
float getCurrentBrewWeight() const
bool hasScaleError() const
bool hasSensorError() const
```

**Process State Checks** (can be mocked):
```cpp
bool isBrewActive() const
bool isManualFlushActive() const
bool isSteamActive() const
bool isHotWaterActive() const
bool isBackflushActive() const
bool isPidEnabled() const
bool isEmergencyStop() const
```

**Timing Functions** (can be mocked):
```cpp
unsigned long getCurrentTime() const
unsigned long getStateElapsedTimeMs() const
bool hasStateTimeoutElapsed(unsigned long timeoutMs) const
```

**Control Functions** (can be mocked):
```cpp
void setSteamMode(bool enabled) const
void setPidRuntimeState(bool enabled) const
void setManualFlushState(bool active) const
void setHotWaterState(bool active) const
void setSteamState(bool active) const
void setBackflushState(bool active) const
void disableWaterOperations() const
void enableWaterOperations() const
void enterSafeMode() const
void exitSafeMode() const
```

**Testable Pattern**: All methods can be mocked in tests to verify state transitions

---

## 2. PROCESS CONTROLLER LOGIC

### Location
**File**: `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/control/ProcessController.h`
**Implementation**: `/Users/marbaced/projects/forks/fork-clevercoffee/src/control/ProcessController.cpp` (431 lines)

### Key Responsibilities

**Temperature Management**:
- Method: `updateTemperature()`
- Logic: Reads from SensorManager, applies brew offset based on steam mode
- Testable: Temperature reading with offset application

**PID Control Computation**:
- Method: `computePID()`
- Logic: Runs PID algorithm and updates output
- Dependencies: PID_v1 library wrapper
- Testable: PID computation (needs PID mock)

**PID Tuning**:
- Method: `setPIDTunings(bool usePonM)`
- Logic: Sets Kp, Ki, Kd parameters for normal operation
- Parameters sourced from Config
- Method: `setBrewDetectionPIDTunings()`
- Method: `setSteamPIDTunings()`
- Testable: Parameter calculation from time constants

**Derived PID Parameters**:
- Method: `calculatePIDParameters()` (private)
- Logic: Compute Ki from Kp and Tn: `Ki = Kp / Tn`
- Logic: Compute Kd from Kp and Tv: `Kd = Kp * Tv`
- **Highly Testable**: Pure mathematical operations
- Member vars: `aggKp_`, `aggTn_`, `aggTv_` → `aggKi_`, `aggKd_`

**Emergency Temperature Detection**:
- Method: `testEmergencyConditions()`
- Logic: Debouncing with `EMERGENCY_TEMP_DEBOUNCE_COUNT = 3`
- Requires 3 consecutive high-temperature readings
- Triggers emergency stop if threshold exceeded
- **Testable**: State machine logic with counter

**Setpoint Management**:
- Method: `updateSetpoint(bool steamActive)`
- Logic: Switches between brew and steam setpoint
- Sources: `brewSetpoint_` vs `steamSetpoint_`
- **Testable**: Pure conditional logic

**PID Enable/Disable Logic**:
- Method: `shouldPIDBeEnabled(MachineStateId machineState) const`
- Logic: Determines PID state based on machine state
- States: BREW_*, STEAM_*, PID_NORMAL, etc.
- **Testable**: Pure state classification

**Process Control Cycle**:
- Method: `updateProcessControl(MachineStateId machineState)`
- Complete cycle: temp update → emergency check → PID compute → setpoint update → PID state update → brew delay handling
- **Testable**: Each sub-step can be tested independently

**Brew PID Delay Logic** (private):
- Method: `handleBrewPIDDelay(MachineStateId machineState)`
- Logic: Disables PID for configured duration after brew detection
- Uses timing: `brewPidDelay` from Config
- **Testable**: Timer-based state logic

**Safety Operations**:
- Method: `emergencyStop()` - Immediately disable PID and turn off heater
- Method: `performSafeShutdown()` - Complete shutdown
  - Disables PID
  - Turns off heater, pump, valve relays
  - Resets all brewing/flushing/steam states
  - **Testable**: State reset sequence verification

### Configuration Dependencies
- `brewSetpoint` (double, range 85-105°C)
- `steamSetpoint` (double, range 100-140°C)
- `brewTempOffset` (double, range -10 to +10°C)
- `pidRegularKp`, `pidRegularTn`, `pidRegularTv`, `pidRegularIMax`
- `pidBdKp`, `pidBdTn`, `pidBdTv`
- `pidSteamKp`
- `brewPidDelay` (seconds)
- `pidEnabled` (boolean)
- `emergencyStopTemp` (double, range 120-180°C)
- `emergencyStopHysteresis` (double, range 1-15°C)

### Testable Patterns
1. **Standalone calculation functions**: PID parameter derivation
2. **State-based logic**: Emergency detection, PID enable/disable
3. **Configuration reading**: All config access through Config singleton
4. **Flag-based control**: Using global state flags

---

## 3. CONFIGURATION SYSTEM

### Location
**File**: `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/Config.h` (1488 lines)

### Architecture

**Three Parameter Types**:

#### 3.1 ParamDef<T> - Editable Configuration
**Template**: `template <typename T> class ParamDef : public ConfigParamDef`

**Supported Types**: bool, int, double, float, String

**Key Methods**:
```cpp
T get() const noexcept                              // Type-safe getter
bool set(const T& value)                            // Type-safe setter with validation
bool isValid(const T& value) const                  // Range/type validation
void resetToDefault() override                      // Reset to default value
bool shouldShow() const                             // Conditional visibility
```

**Validation Logic** (TESTABLE):
```cpp
if constexpr (std::is_arithmetic_v<T>) {
    return value >= minValue_ && value <= maxValue_;  // Range check
}
```

**Storage**: NVS (Non-Volatile Storage) with hash-based keys

**Testable Logic**:
- Type-safe validation
- Range checking (min/max bounds)
- String-to-type conversion
- Default value management

#### 3.2 EnumParamDef<E> - Enum Configuration
**Template**: `template <typename E> class EnumParamDef : public ConfigParamDef`

**Key Methods**:
```cpp
E get() const                                       // Get enum value
bool set(const E& value)                            // Set with validation
bool isValid(const E& value) const                  // Check against options list
```

**Validation Logic** (TESTABLE):
- Validates enum against allowed options list
- Supports both integer and label-based parsing
- Type-safe enum handling

**Used For**:
- `BrewMode` (MANUAL_BREW vs AUTOMATIC_BREW)
- `DisplayTemplate`, `Language`, `LogLevel`
- `SwitchType`, `SwitchMode`, `RelayTriggerType`
- `OLEDType`, `OLEDAddress`
- `TemperatureSensorType`, `ScaleType`

#### 3.3 StateParamDef<T> - Read-Only State Values
**Template**: `template <typename T> class StateParamDef : public BaseParamDef`

**Purpose**: Expose runtime state values to web interface

**UpdateFrequency** (enum):
- REALTIME - Every loop (temperature, PID output)
- FREQUENT - Every few seconds (connection status)
- OCCASIONAL - When requested (system info)
- STATIC - Never changes (firmware version)

**Methods**:
```cpp
T get() const noexcept                              // Compute value on-the-fly
T getCached() const                                 // Get cached value
void updateCache() const                            // Force cache update
bool needsUpdate(unsigned long maxAge) const        // Check if update needed
```

**Testable Logic**:
- Cache invalidation logic
- Update frequency enforcement
- Value provider computation

#### 3.4 ComputedParamDef<T> - Derived Values
**Purpose**: Read-only computed values derived from config parameters

**Example**: 
- `Ki = Kp / Tn` (computed from Kp and Tn)
- Brew ratio (weight out / weight in)

**Testable Logic**:
- Computation function correctness
- Dependency tracking

### Configuration Parameters (DIRECTLY TESTABLE)

**PID Parameters**:
- `pidEnabled` (bool)
- `pidUsePonm` (bool) - Proportional on Measurement mode
- `pidEmaFactor` (double, 0.0-1.0) - EMA smoothing
- `pidRegularKp`, `pidRegularTn`, `pidRegularTv`, `pidRegularIMax`
- `pidBdKp`, `pidBdTn`, `pidBdTv` - Brew detection parameters
- `pidSteamKp`

**Brew Control**:
- `brewSetpoint` (double, 85-105°C)
- `brewTempOffset` (double, -10 to +10°C)
- `brewByTimeEnabled` (bool)
- `brewByTimeTargetTime` (double, 1-120s)
- `brewByWeightEnabled` (bool)
- `brewByWeightTargetWeight` (double, 1-500g)
- `brewByWeightAutoTare` (bool)
- `brewPreInfusionEnabled` (bool)
- `brewPreInfusionTime` (double, 0-30s)
- `brewPreInfusionPause` (double, 0-30s)
- `brewPidDelay` (double, 0-20s)
- `brewMode` (enum: MANUAL_BREW, AUTOMATIC_BREW)

**Safety**:
- `emergencyStopTemp` (double, 120-180°C)
- `emergencyStopHysteresis` (double, 1-15°C)
- `steamSetpoint` (double, 100-140°C)

**Backflush**:
- `backflushCycles` (int, 1-20)
- `backflushFillTime` (double, 0.5-5s)
- `backflushFlushTime` (double, 0.5-5s)

**Standby**:
- `standbyEnabled` (bool)
- `standbyTime` (double, 1-120 minutes)

**Hardware Configuration**:
- Switch enables/types/modes for brew, steam, power, hot water
- Relay trigger types
- LED enables/inversions
- Sensor types and calibrations

**Network**:
- `mqttEnabled`, `mqttBroker`, `mqttPort`, `mqttUsername`, `mqttPassword`, `mqttTopic`
- `mqttHassioEnabled`, `mqttHassioPrefix`

**System**:
- `systemHostname`, `systemOtaPassword`
- `systemOfflineMode`, `systemLogLevel`
- `systemAuthEnabled`, `systemAuthUsername`, `systemAuthPassword`

### Testable Logic
1. **Parameter Validation**: Range checking, type conversion
2. **Default Value Management**: Reset, initialization
3. **NVS Persistence**: Save/load logic (can mock NVS)
4. **JSON Serialization**: Config export/import
5. **Conditional Visibility**: Show/hide parameters based on conditions

---

## 4. SENSOR MANAGEMENT

### Location
**File**: `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/sensors/SensorManager.h` (229 lines)
**Implementation**: `/Users/marbaced/projects/forks/fork-clevercoffee/src/sensors/SensorManager.cpp` (436 lines)

### Key Responsibilities

**Temperature Sensor Interface**:
- Method: `getCurrentTemperature() const` → double
- Method: `hasTemperatureError() const` → bool
- Testable: Wrapper interface for temperature readings

**Water Tank Sensor**:
- Method: `isWaterTankFull() const` → bool
- Method: `updateWaterTankSensor()`
- **Testable Logic**: 
  - Debouncing with `waterTankCountsNeeded = 3`
  - Consecutive reads confirmation
  - State tracking variables: `waterTankFull_`, `waterTankCheckConsecutiveReads_`

**Pressure Sensor** (Analog input filtering):
- Method: `getCurrentPressure() const` → float
- Method: `getFilteredPressure() const` → float
- Method: `updatePressureSensor()`
- Method: `filterPressureValue(float input) -> float` (private)
- **Testable Logic**: 
  - Moving average filter implementation
  - History buffer management
  - Filter state variables: `inputPressure_`, `inputPressureFilter_`, `inX_`, `inY_`, `inOld_`, `inSum_`
  - History tracking: `pressureHistory_` (10-element array), `pressureHistoryIndex_`

**Scale Interface**:
- Method: `initializeScale() -> bool`
- Method: `updateScale()`
- Method: `getCurrentWeight() const -> float`
- Method: `getCurrentBrewWeight() const -> float`
- Method: `hasScaleError() const -> bool`
- Method: `getScale() const -> Scale*` (raw pointer access for calibration)

**Sensor Status**:
- Method: `areSensorsReady() const -> bool`
- Method: `hasSensorError() const -> bool`
- Checks all enabled sensors

**Modern C++23 Processing Methods** (TESTABLE):
```cpp
double processTemperatureReadings(const std::vector<double>& readings) const
  // Filters invalid readings, returns average
  
float processPressureReadings(const std::array<float, 10>& readings) const
  // Smooths pressure values using ranges-based filtering
```

**Testable Logic**:
1. **Debouncing**: Water tank sensor with 3-read confirmation
2. **Filtering**: Pressure sensor moving average filter
3. **History Management**: Circular buffers for readings
4. **Error Detection**: Sensor error state tracking
5. **Aggregation**: Processing multiple readings with filtering

### Internal State (TESTABLE)
- `temperature_` - Current temperature
- `sensorsInitialized_` - Initialization flag
- `waterTankFull_` - Current tank state
- `waterTankCheckConsecutiveReads_` - Debounce counter
- `inputPressure_` - Raw pressure
- `inputPressureFilter_` - Filtered pressure
- Filter state: `inX_`, `inY_`, `inOld_`, `inSum_`

---

## 5. HANDLER LOGIC (Switch-Based Event Processing)

### Location
**Files**: `include/clevercoffee/handlers/`
**Implementations**: `src/handlers/` (partial - most in global state system)

### 5.1 Base Handler
**File**: `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/handlers/BaseHandler.h`

**Purpose**: Template base for switch-based event handlers

**Key Methods**:
```cpp
void process()                                      // Main processing loop
bool isEnabled() const                              // Check if handler is enabled
bool hasPermission() const                          // Check if operation allowed
void processImpl()                                   // Derived implementation
```

**Testable Pattern**: Template method pattern
- Framework handles enable/permission checks
- Derived classes override `processImpl()`

### 5.2 Brew Handler
**File**: `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/handlers/BrewHandler.h` (109 lines)

**Class Hierarchy**: `BrewHandler : SwitchBasedHandler`

**Key State**:
- `PumpTimer pumpTimer_` - 5-minute max brew safety timeout
- `brewStartTime_` - Brew start timestamp
- `lastSwitchReading_` - Previous switch state

**Methods**:
```cpp
bool isBrewActive() const
  // Checks: isBrewState(machineState) && machineState != IDLE && != FINISHED

void valveSafetyShutdownCheck()
  // Safety: Closes valve if not actively brewing

bool isEnabled() const override
  // Checks: hardwareSwitchesBrewEnabled config

bool hasPermission() const override
  // Checks: Parent permission AND machineState != WATER_TANK_EMPTY AND != HOT_WATER_RUNNING

void processImpl() override
  // Calls: processSwitchInput() and checkPumpTimeout()
```

**Private Methods**:
```cpp
void processSwitchInput()
  // Handles toggle vs momentary switch logic
  // Updates g_state.sensors.brewSwitchReading

void checkPumpTimeout()
  // If timer expired and brew active: request stop
  // Sets g_state.machine.flags.requestBrewStop = true
```

**Testable Logic**:
1. **Permission Checks**: State-based authorization
2. **Pump Timeout**: Timer-based safety shutdown
3. **Switch Type Handling**: Toggle vs momentary logic
4. **Active State Classification**: Brew state validation

### 5.3 Other Handlers
Parallel structure:
- `HotWaterHandler` - Hot water switch processing
- `SteamHandler` - Steam switch processing
- `PowerHandler` - Power on/off switch
- `PumpTimer` - Timeout management utility

---

## 6. SAFETY AND EMERGENCY LOGIC

### Emergency Temperature Detection (TESTABLE)

**Location**: ProcessController::testEmergencyConditions()

**Logic**:
```
If current_temp > emergencyStopTemp:
    emergencyTempReadingCount++
    If emergencyTempReadingCount >= DEBOUNCE_COUNT (3):
        Return true (trigger emergency stop)
Else:
    emergencyTempReadingCount = 0 (reset counter)
```

**Testable**: 
- Debouncing mechanism
- Threshold comparison
- Counter reset on normal temperature
- Config-based temperature threshold
- Config-based hysteresis value

**Configuration Parameters**:
- `emergencyStopTemp` (120-180°C)
- `emergencyStopHysteresis` (1-15°C)

### Emergency Stop State Transitions (TESTABLE)

**Location**: BaseState::checkTransitions()

**Transition Priority** (highest to lowest):
1. Emergency stop active → EMERGENCY_STOP
2. Sensor error detected → SENSOR_ERROR
3. Water tank empty → WATER_TANK_EMPTY
4. State-specific transitions

**Testable**:
- Transition prioritization
- State error checking
- Safety interlock verification

### Safe Shutdown Sequence (TESTABLE)

**Location**: ProcessController::performSafeShutdown()

**Sequence**:
1. Disable PID control
2. Turn off heater relay
3. Turn off pump relay
4. Turn off valve relay
5. Reset brew state flags
6. Reset manual flush flags
7. Reset backflush state
8. Disable steam mode
9. Reset hot water state

**Testable**: 
- Shutdown sequence validation
- State reset verification
- Relay control ordering

---

## 7. TIMING AND STATE DURATION LOGIC (TESTABLE)

### State Timing
**Location**: MachineStateContext

**Methods**:
```cpp
unsigned long getStateElapsedTimeMs() const
  // Time since current state was entered

bool hasStateTimeoutElapsed(unsigned long timeoutMs) const
  // Check if specified timeout has elapsed since state entry

void updateStateEntryTime(std::chrono::steady_clock::time_point entryTime)
  // Update state entry timestamp (called on transitions)
```

**Testable Logic**:
1. Elapsed time calculation
2. Timeout comparison
3. State duration tracking

### Process Timing

**Configuration-Based Timeouts**:
- `brewPreInfusionTime` (0-30s)
- `brewPreInfusionPause` (0-30s)
- `brewByTimeTargetTime` (1-120s)
- `backflushFillTime` (0.5-5s)
- `backflushFlushTime` (0.5-5s)
- `standbyTime` (1-120 minutes)
- `brewPidDelay` (0-20s)

**Testable Pattern**: State machines with timeout transitions

---

## 8. BREW DETECTION AND CONTROL LOGIC (TESTABLE)

### Brew by Time
**Parameters**:
- `brewByTimeEnabled` (bool)
- `brewByTimeTargetTime` (double, 1-120s)

**Logic** (in BrewRunningState):
- Start timer on brew entry
- Compare elapsed time vs target time
- Trigger transition when time exceeded

**Testable**: Elapsed time comparison

### Brew by Weight
**Parameters**:
- `brewByWeightEnabled` (bool)
- `brewByWeightTargetWeight` (double, 1-500g)
- `brewByWeightAutoTare` (bool)

**Logic**:
- Track weight difference from tare
- Compare current brew weight vs target
- Stop when target weight reached

**Testable**: Weight comparison logic (if SensorManager can be mocked)

### Pre-Infusion Sequence
**Parameters**:
- `brewPreInfusionEnabled` (bool)
- `brewPreInfusionTime` (double, 0-30s) - Infusion duration
- `brewPreInfusionPause` (double, 0-30s) - Pause/blooming duration

**State Machine**:
1. BREW_IDLE → BREW_PREINFUSION (on brew request)
2. BREW_PREINFUSION → BREW_PREINFUSION_PAUSE (after infusion time)
3. BREW_PREINFUSION_PAUSE → BREW_RUNNING (after pause time)

**Testable**: Transition timing and sequencing

---

## 9. BACKFLUSH CYCLE LOGIC (TESTABLE)

**Parameters**:
- `backflushCycles` (int, 1-20) - Number of fill/flush pairs
- `backflushFillTime` (double, 0.5-5s) - Water fill duration
- `backflushFlushTime` (double, 0.5-5s) - Flush duration

**State Machine** (from BackflushStates):
1. BACKFLUSH_IDLE
   - Check for backflush request
   - → BACKFLUSH_FILLING

2. BACKFLUSH_FILLING
   - Fill portafilter for backflushFillTime
   - → BACKFLUSH_FLUSHING

3. BACKFLUSH_FLUSHING
   - Flush for backflushFlushTime
   - Increment cycle counter
   - If cycles < target: → BACKFLUSH_FILLING
   - If cycles >= target: → BACKFLUSH_FINISHED

4. BACKFLUSH_FINISHED
   - Cleanup and timing display
   - → BACKFLUSH_IDLE

**Testable Logic**:
1. Cycle counting (0 to N)
2. State transition timing
3. Fill/flush duration enforcement
4. Completion condition checking

---

## 10. STANDBY MODE LOGIC (TESTABLE)

**Parameters**:
- `standbyEnabled` (bool)
- `standbyTime` (double, 1-120 minutes)

**Logic**:
- Track last user activity (switch presses, state changes)
- After `standbyTime` minutes of inactivity → STANDBY state
- In standby: PID disabled, heater off, display off
- On user activity → exit standby, return to PID_NORMAL

**Testable**:
- Inactivity timer logic
- Activity detection
- State transitions
- Standby duration comparison

---

## SUMMARY TABLE: TESTABLE COMPONENTS

| Component | File | Type | Lines | Testability | Key Methods |
|-----------|------|------|-------|-------------|------------|
| **State Enums** | MachineStateIds.h | Pure Functions | 81 | HIGH | `isBrewState()`, `isHotWaterState()`, etc. |
| **BaseState** | BaseState.h | Template Class | 121 | HIGH | `checkTransitions()`, `onEntry()`, `onExit()` |
| **BrewStates** | BrewStates.h/cpp | State Classes | 146 | HIGH | `checkSpecificTransitions()`, flag checking |
| **EmergencyStop** | EmergencyStopState.h | State Class | 22 | HIGH | `performEmergencyShutdown()`, `isEmergencyCleared()` |
| **ProcessController** | ProcessController.h/cpp | Business Logic | 431 | HIGH | `calculatePIDParameters()`, `testEmergencyConditions()` |
| **Config Validation** | Config.h | Templates | 1488 | HIGH | `isValid()`, Range checking, Type conversion |
| **SensorManager** | SensorManager.h/cpp | Filters & Debouncing | 436 | HIGH | `filterPressureValue()`, Debounce logic |
| **BrewHandler** | BrewHandler.h | Event Handler | 109 | HIGH | `hasPermission()`, `checkPumpTimeout()` |
| **SafeShutdown** | ProcessController.h/cpp | Safety Logic | 431 | HIGH | `performSafeShutdown()` |
| **Timing Logic** | MachineStateContext.h | Time Management | 419 | HIGH | `hasStateTimeoutElapsed()`, Elapsed time |
| **Backflush FSM** | BackflushStates.h/cpp | State Machine | 105 | HIGH | State transitions, Cycle counting |

---

## TESTING STRATEGY RECOMMENDATIONS

### 1. **Immediate Candidates for Unit Tests** (No Hardware Mock Needed)
- State classification functions (constexpr pure functions)
- PID parameter calculations (Ki, Kd from Kp, Tn, Tv)
- Configuration validation (range checks, type conversion)
- Enum parameter validation
- Timeout/timing comparison logic
- Counter/debouncing logic

### 2. **Candidates with Mocking** (Mock Sensors/Config)
- State transition logic (mock MachineStateContext)
- Emergency temperature detection (mock sensor readings)
- Pump timeout logic (mock timer)
- Brew by weight/time logic (mock weight/time)
- Backflush cycle logic (mock relays)

### 3. **Candidates with Test Fixtures** (Mock Everything)
- Complete state machine transitions
- Process controller workflows
- Safe shutdown sequences
- Handler permission logic

### 4. **Pure Calculation Tests** (Easiest to Test)
```cpp
// PID Parameter Derivation
TEST(ProcessController, CalculateKiFromKpAndTn) {
    double kp = 50.0, tn = 600.0;
    double ki = kp / tn;  // 0.0833
    EXPECT_DOUBLE_EQ(ki, 50.0 / 600.0);
}

TEST(ProcessController, CalculateKdFromKpAndTv) {
    double kp = 50.0, tv = 60.0;
    double kd = kp * tv;  // 3000.0
    EXPECT_DOUBLE_EQ(kd, 50.0 * 60.0);
}
```

### 5. **State Machine Tests** (With Mocks)
```cpp
// Test state transitions
TEST(BrewStates, BrewIdleToPreinfusion) {
    MockMachineStateContext context;
    BrewIdleState state;
    
    // Set up: brew requested
    g_state.machine.flags.requestBrewStart = true;
    
    MachineState* nextState = state.checkSpecificTransitions(context);
    EXPECT_NE(nextState, nullptr);
    EXPECT_EQ(nextState->getStateId(), MachineStateId::BREW_PREINFUSION);
}
```

### 6. **Configuration Tests** (No Hardware)
```cpp
TEST(ConfigValidation, BrewSetpointRange) {
    auto& param = Config::getInstance().brewSetpoint;
    
    EXPECT_TRUE(param.isValid(95.0));   // Valid
    EXPECT_FALSE(param.isValid(50.0));  // Too low
    EXPECT_FALSE(param.isValid(150.0)); // Too high
}
```

---

## NOTES

1. **Hardware Dependencies**: 
   - Most testable logic can be isolated by mocking `MachineStateContext`
   - SensorManager reads can be mocked
   - Config reads are singleton-based, can be reset in tests

2. **Global State**: 
   - `g_state` global variable is problematic for testing
   - Can work around by resetting flags between tests
   - Recommend refactoring to inject state container

3. **ESP32 Limitations**:
   - No C++20 concepts support
   - Limited memory for mock objects
   - PID_v1 library is external dependency (may need wrapper tests)

4. **Test Framework Compatibility**:
   - Use Google Test (gtest) - compatible with PlatformIO
   - Avoid C++20+ features
   - Keep mocks lightweight for ESP32 memory constraints

5. **Configuration as "Values Object"**:
   - Config parameters are well-suited for property-based testing
   - Can generate test cases for parameter combinations
   - Range-based randomized testing

