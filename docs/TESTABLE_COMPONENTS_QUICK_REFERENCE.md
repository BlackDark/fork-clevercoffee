# Quick Reference: Testable Components Summary

## By Priority for Testing

### 🔴 Tier 1: Immediate Test Candidates (Pure Logic, No Hardware)

| Component | File | Testability | Why Test |
|-----------|------|------------|----------|
| **State Classification** | `MachineStateIds.h` | ★★★★★ | Pure constexpr functions: `isBrewState()`, `isHotWaterState()`, etc. |
| **Config Validation** | `Config.h` | ★★★★★ | Range checking, type conversion for all parameter types |
| **PID Parameter Math** | `ProcessController.cpp` | ★★★★★ | `Ki = Kp/Tn`, `Kd = Kp*Tv` calculations |
| **Counter Logic** | `SensorManager.cpp` | ★★★★★ | Debouncing: 3-read confirmation for water tank |

### 🟡 Tier 2: Testable with Mocks (Sensor/Config Mocking)

| Component | File | Testability | What to Mock |
|-----------|------|------------|--------------|
| **State Transitions** | `BrewStates.cpp` | ★★★★☆ | `MachineStateContext` (all 9+ state pairs) |
| **Emergency Detection** | `ProcessController.cpp` | ★★★★☆ | Temperature readings, counter state |
| **Filter Logic** | `SensorManager.cpp` | ★★★★☆ | Pressure history, moving average filter |
| **Brew Handler** | `BrewHandler.h` | ★★★★☆ | Pump timer, switch state, machine state |

### 🟢 Tier 3: Integration Tests (Multiple Components)

| Component | File | Testability | Integration |
|-----------|------|------------|-------------|
| **Backflush FSM** | `BackflushStates.cpp` | ★★★☆☆ | 4-state cycle with timing |
| **Brew Sequence** | `BrewStates.cpp` | ★★★☆☆ | 5-state sequence with timing |
| **Process Control** | `ProcessController.cpp` | ★★★☆☆ | Temperature → PID → Setpoint → Relay |
| **Safe Shutdown** | `ProcessController.cpp` | ★★★☆☆ | Multi-step relay shutdown sequence |

---

## By Component Category

### State Machine (11 State Classes)

```
BaseState (template) ──┬─→ BrewStates (5 classes)
                       ├─→ HotWaterStates (3 classes)
                       ├─→ SteamStates (3 classes)
                       ├─→ BackflushStates (4 classes)
                       ├─→ ErrorStates (3 classes)
                       └─→ EmergencyStopState (1 class)
```

**Key Testable Methods**: `checkSpecificTransitions()`, `onEntryImpl()`, `onExitImpl()`
**Test Approach**: Mock `MachineStateContext`, test flag-based transitions

### Process Control (1 Main Class)

```
ProcessController
├─ Temperature Management → `updateTemperature()`
├─ PID Computation → `computePID()`
├─ PID Tuning → `setPIDTunings()`, `setBrewDetectionPIDTunings()`
├─ Emergency Detection → `testEmergencyConditions()`
├─ Setpoint Management → `updateSetpoint()`
├─ Safety Operations → `emergencyStop()`, `performSafeShutdown()`
└─ Brew PID Delay → `handleBrewPIDDelay()`
```

**Key Testable Methods**: All listed above
**Test Approach**: Mock sensors, config, and PID controller

### Configuration (4 Template Classes)

```
Config (Singleton)
├─ ParamDef<T> (40+ instances)
├─ EnumParamDef<E> (10+ instances)
├─ StateParamDef<T> (commented out - future)
└─ ComputedParamDef<T> (commented out - future)
```

**Key Testable Logic**: Validation, type conversion, range checking
**Test Approach**: Direct instantiation, no mocking needed

### Sensor Management (1 Main Class)

```
SensorManager
├─ Temperature Interface
├─ Water Tank (debouncing)
├─ Pressure (filtering)
├─ Scale (weight reading)
└─ Processing (ranges-based filtering)
```

**Key Testable Logic**: Debouncing counter, filter state, error flags
**Test Approach**: Mock hardware sensors, test filter algorithms

### Handlers (4 Main Classes)

```
BrewHandler ──┬─ Permission checks
              ├─ Pump timeout logic
              └─ Switch processing

HotWaterHandler, SteamHandler, PowerHandler (similar structure)
```

**Key Testable Logic**: Permission checks, timeout triggering
**Test Approach**: Mock hardware, test flag transitions

---

## Top 10 Highest-Impact Tests to Write First

### Tier 1: Easiest, Most Value (Write These First!)

1. **State Classification Tests** (5 tests, <5 mins each)
   - `isBrewState()` for all 5 brew states
   - `isHotWaterState()` for all 3 hot water states
   - Boundary cases

2. **Config Range Validation** (5 tests, <5 mins each)
   - `brewSetpoint` range (85-105°C)
   - `emergencyStopTemp` range (120-180°C)
   - Out-of-range rejection
   - Boundary conditions

3. **PID Parameter Calculations** (3 tests, <10 mins each)
   - `Ki = Kp / Tn`
   - `Kd = Kp * Tv`
   - Zero handling (Tn=0 case)

4. **Water Tank Debouncing** (3 tests, <15 mins each)
   - Require 3 consecutive reads
   - Reset counter on state change
   - State tracking

### Tier 2: High Value, Moderate Effort (Write These Next!)

5. **State Transition Prioritization** (1 test, <20 mins)
   - Emergency stop > Sensor error > Tank empty
   - Verify priority ordering

6. **Emergency Temperature Detection** (2 tests, <20 mins each)
   - 3-read debouncing
   - Threshold comparison
   - Counter reset

7. **Brew Handler Permissions** (3 tests, <15 mins each)
   - Disabled check
   - Water tank empty check
   - Hot water active check

### Tier 3: Integration Tests (Write After Tier 1&2!)

8. **Backflush Cycle FSM** (1 test, <30 mins)
   - 4-state sequence with timing
   - Cycle counting
   - Completion condition

9. **Brew Sequence FSM** (1 test, <30 mins)
   - 5-state sequence (idle → preinfusion → pause → running → finished)
   - Flag-based transitions

10. **Safe Shutdown Sequence** (1 test, <30 mins)
    - Step-by-step shutdown verification
    - State flag reset validation
    - Relay disable ordering

---

## Test File Organization

```
test/
├─ unit/
│  ├─ state/
│  │  ├─ test_machine_state_ids.cpp          (State classification)
│  │  ├─ test_base_state.cpp                 (Common transitions)
│  │  ├─ test_brew_states.cpp                (Brew FSM)
│  │  ├─ test_emergency_stop_state.cpp       (Emergency logic)
│  │  └─ test_backflush_states.cpp           (Backflush FSM)
│  │
│  ├─ control/
│  │  ├─ test_process_controller_math.cpp    (PID calculations)
│  │  ├─ test_process_controller_safety.cpp  (Emergency detection)
│  │  └─ test_process_controller_shutdown.cpp (Shutdown sequence)
│  │
│  ├─ config/
│  │  ├─ test_config_validation.cpp          (Range checking)
│  │  ├─ test_config_enum.cpp                (Enum validation)
│  │  └─ test_config_persistence.cpp         (Save/load)
│  │
│  ├─ sensors/
│  │  ├─ test_sensor_manager_debouncing.cpp  (Water tank logic)
│  │  ├─ test_sensor_manager_filtering.cpp   (Pressure filter)
│  │  └─ test_sensor_manager_aggregation.cpp (Reading processing)
│  │
│  └─ handlers/
│     ├─ test_brew_handler.cpp               (Brew handler)
│     └─ test_handler_permissions.cpp        (All handlers)
│
└─ integration/
   ├─ test_brew_flow.cpp                     (Complete brew sequence)
   ├─ test_emergency_flow.cpp                (Emergency to recovery)
   └─ test_backflush_flow.cpp                (Backflush sequence)
```

---

## Mocking Strategy

### Level 1: No Mocks Needed
- State classification (`isBrewState()`)
- Config validation
- PID math calculations
- Counter logic

### Level 2: Single Component Mock
- State transitions (mock `MachineStateContext`)
- Config reading (reset singleton, or inject)
- Timer logic (mock `millis()`)

### Level 3: Multiple Mocks
- Sensor manager (mock hardware interface)
- Process controller (mock sensors + config)
- Handler logic (mock relays + switches)

### Level 4: Full Integration
- Complete state machine (mock entire hardware layer)
- Process control workflow (mock all managers)
- Safe shutdown sequence (mock relays + flags)

---

## Expected Test Coverage After Phase 1

**Easy Tests** (Tier 1-2):
- State classification: 100% coverage
- Config validation: 95% coverage
- PID math: 100% coverage
- Debouncing: 85% coverage
- Emergency detection: 80% coverage
- Handlers: 70% coverage

**Estimated Time**: 20-30 hours for comprehensive Tier 1+2 coverage

**Files to Start With**:
1. `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/state/MachineStateIds.h`
2. `/Users/marbaced/projects/forks/fork-clevercoffee/include/clevercoffee/Config.h`
3. `/Users/marbaced/projects/forks/fork-clevercoffee/src/control/ProcessController.cpp`

