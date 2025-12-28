# Phase 4: Architectural Cleanup - Separation of Concerns

> **Status:** Design Document
> **Created:** 2025-12-28
> **Author:** Claude (Sonnet 4.5)
> **Estimated Effort:** 2-3 weeks

## Executive Summary

This document outlines Phase 4 of the code quality refactoring: eliminating "spaghetti code" by enforcing strict separation of concerns between sensors, handlers, and the state machine. The goal is to create a clean, unidirectional data flow where **sensors observe**, **handlers coordinate**, and **the state machine decides and acts**.

## Problem Statement

### Current Anti-Patterns

While Phases 1-3 successfully addressed critical issues (global state, exception safety) and minor issues (constants, error handling), the user correctly identified that architectural concerns remain:

```
┌─────────────────────────────────────────────────────────────────┐
│ CURRENT STATE: Confused Responsibilities                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Sensors ────────┐                                               │
│  (scaleHandler)  │                                               │
│  ↓               │                                               │
│  [Reads Weight]  │    Should NOT happen:                         │
│  ↓               │    ❌ Sensors setting request flags            │
│  [Sets Flags?] ←─┘    ❌ Handlers making decisions               │
│  ↓                    ❌ SystemInitializer setting machineState  │
│  State Machine                                                         │
│  (Transitions)                                                        │
│                                                                       │
└───────────────────────────────────────────────────────────────────┘
```

### Specific Issues Found

1. **scaleHandler** (`include/clevercoffee/scaleHandler.h:151-152`):
   ```cpp
   // Line 151-152: Handler setting request flags
   g_state.machine.flags.requestBrewStop = true;     // ❌ Should NOT do this
   g_state.machine.flags.requestSensorError = true;  // ❌ Should NOT do this
   ```
   The scale handler should **only** read and report sensor values, not decide what to do about errors.

2. **SystemInitializer** (`src/core/SystemInitializer.cpp`):
   ```cpp
   g_state.machine.machineState = MachineStateId::PID_NORMAL;  // ❌ Direct assignment
   ```
   Direct state assignment bypasses the state machine's transition logic.

3. **Handlers** (BrewHandler, SteamHandler, HotWaterHandler):
   - Currently lightweight (good!), but the pattern isn't consistently documented
   - Some handlers set flags directly instead of just reading inputs

## Proposed Architecture

### The Three-Layer Model

```
┌─────────────────────────────────────────────────────────────────────┐
│ PROPOSED: Clean Unidirectional Data Flow                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌────────────────┐      ┌────────────────┐      ┌───────────────┐ │
│  │   LAYER 1:     │      │   LAYER 2:     │      │   LAYER 3:     │ │
│  │   SENSORS      │  →   │   HANDLERS     │  →   │   STATE        │ │
│  │   (OBSERVE)    │      │   (COORDINATE) │      │   MACHINE      │ │
│  └────────────────┘      └────────────────┘      │   (DECIDE)     │ │
│        │                        │               │   & ACT        │ │
│        │                        │               └───────────────┘ │
│        ↓                        ↓                      ↓           │
│  • Read sensors           • Read handlers         • Evaluate      │
│  • Update values          • Check permissions     • Transition    │
│  • Report errors          • Set request flags*    • Control       │
│  (ONLY)                   (COORDINATE ONLY)       hardware        │
│                                                  (DECIDE & ACT)   │
│                                                                     │
│  *Request flags are the ONLY way handlers influence the state       │
│   machine. Handlers NEVER decide, NEVER act directly.               │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Layer Responsibilities

#### Layer 1: Sensors (OBSERVE & REPORT)

**Responsibility:** Read hardware and update values. Nothing else.

**Examples:**
- `checkWeight()` → Read scale, update `g_state.sensors.currReadingWeight`
- `updateTemperatureSensor()` → Read temp, update `g_state.process.temperature`
- `updatePressureSensor()` → Read pressure, update `g_state.sensors.inputPressure`

**Rules:**
- ✅ Read from hardware
- ✅ Update sensor values in state
- ✅ Set error flags (e.g., `scaleFailure = true`)
- ❌ NEVER set request flags (e.g., `requestBrewStop`)
- ❌ NEVER control relays directly
- ❌ NEVER make decisions about what should happen

#### Layer 2: Handlers (COORDINATE)

**Responsibility:** Read switches/sensors, check permissions, set request flags.

**Examples:**
- `BrewHandler::process()` → Read brew switch, check permissions, set `requestBrewStart/Stop`
- `SteamHandler::process()` → Read steam switch, check temperature, set `requestSteamStart/Stop`
- `HotWaterHandler::process()` → Read hot water switch, check water tank, set `requestHotWaterStart/Stop`

**Rules:**
- ✅ Read switches and sensors
- ✅ Check permissions (temperature, water tank, etc.)
- ✅ Set request flags based on conditions
- ✅ Validate inputs (debouncing, validation)
- ❌ NEVER control relays directly
- ❌ NEVER set `machineState` directly
- ❌ NEVER make high-level decisions

#### Layer 3: State Machine (DECIDE & ACT)

**Responsibility:** Evaluate conditions, transition states, control hardware.

**Examples:**
- `BrewRunningState::checkSpecificTransitions()` → Check weight/time, transition to BREW_FINISHED
- `PidNormalState::onEntry()` → Turn on heater relay
- `SteamState::update()` → Control heater for steam temperature

**Rules:**
- ✅ Read request flags
- ✅ Evaluate business logic (brew-by-weight, timers, temperatures)
- ✅ Transition to new states
- ✅ Control relays and hardware directly
- ✅ Update UI based on state
- ❌ NEVER read raw sensor values (use context methods)

### Data Flow Example: Brew-by-Weight

```
TIMELINE: User presses brew switch, scale monitors weight

┌─────────────────────────────────────────────────────────────────────┐
│ T=0ms: User presses brew switch                                      │
├─────────────────────────────────────────────────────────────────────┤
│ Layer 1 (Sensors):                                                   │
│   checkWeight() → currReadingWeight = 0.0g                          │
│                                                                      │
│ Layer 2 (Handlers):                                                  │
│   BrewHandler::process() → requestBrewStart = true                  │
│                                                                      │
│ Layer 3 (State Machine):                                             │
│   BrewIdleState → checks requestBrewStart                           │
│   → transitions to BrewPreinfusionState                             │
│   → onEntry() turns on pump and valve relays                        │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│ T=100ms: Normal brewing loop                                         │
├─────────────────────────────────────────────────────────────────────┤
│ Layer 1 (Sensors):                                                   │
│   checkWeight() → currReadingWeight = 15.2g                         │
│   shotTimerScale() → currBrewWeight = 15.2g                         │
│                                                                      │
│ Layer 2 (Handlers):                                                  │
│   BrewHandler::process() → requestBrewStop = false (still pressing) │
│                                                                      │
│ Layer 3 (State Machine):                                             │
│   BrewRunningState::update() → logging                              │
│   → checkSpecificTransitions() → currBrewWeight < 30g target       │
│   → no transition, stays in BrewRunningState                        │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│ T=5000ms: Target weight reached                                      │
├─────────────────────────────────────────────────────────────────────┤
│ Layer 1 (Sensors):                                                   │
│   checkWeight() → currReadingWeight = 30.1g                         │
│   shotTimerScale() → currBrewWeight = 30.1g                         │
│                                                                      │
│ Layer 2 (Handlers):                                                  │
│   BrewHandler::process() → requestBrewStop = false (still pressing) │
│                                                                      │
│ Layer 3 (State Machine):                                             │
│   BrewRunningState::update() → logging                              │
│   → checkSpecificTransitions() → currBrewWeight >= 30g target      │
│   → transitions to BrewFinishedState                                │
│   → onEntry() turns off pump and valve relays                       │
└─────────────────────────────────────────────────────────────────────┘
```

## Implementation Plan

### Task 4.1: Remove Decision Logic from Sensors

**Problem:** scaleHandler sets request flags directly (lines 57-58, 151-152).

**Solution:** Move error handling decision logic into state machine.

**Files:**
- Modify: `include/clevercoffee/scaleHandler.h`
- Modify: `src/state/MachineStateContext.cpp`
- Test: `test/test_hardware_control/test_main.cpp`

**Changes:**

1. **scaleHandler.h** - Remove flag setting:
   ```cpp
   // BEFORE (lines 151-152):
   g_state.machine.flags.requestBrewStop = true;        // ❌ Remove
   g_state.machine.flags.requestSensorError = true;     // ❌ Remove

   // AFTER:
   g_state.sensors.scaleFailure = true;                 // ✅ Just report error
   // State machine will check this flag and decide what to do
   ```

2. **MachineStateContext** - Add method to check sensor health:
   ```cpp
   bool hasCriticalSensorFailure() const noexcept;
   ```

3. **State Machine** - Add sensor error checking:
   ```cpp
   // In each active state's checkSpecificTransitions():
   if (context.hasCriticalSensorFailure()) {
       return getStateInstance(MachineStateId::SENSOR_ERROR);
   }
   ```

**Estimated Time:** 1-2 days

### Task 4.2: Remove Direct State Assignment

**Problem:** SystemInitializer.cpp sets `g_state.machine.machineState` directly.

**Solution:** Use request flags for all state transitions.

**Files:**
- Modify: `src/core/SystemInitializer.cpp`
- Modify: `src/state/MachineStateContext.cpp`
- Test: `test/test_system_context/test_main.cpp`

**Changes:**

1. **SystemInitializer.cpp** - Replace direct assignment:
   ```cpp
   // BEFORE:
   g_state.machine.machineState = MachineStateId::PID_NORMAL;  // ❌

   // AFTER:
   g_state.machine.flags.requestNormalOperation = true;       // ✅
   ```

2. **Add missing request flags** to MachineStateFlags:
   ```cpp
   bool requestNormalOperation = false;
   bool requestPidNormal = false;
   bool requestInit = false;
   ```

3. **Add state transitions** to handle new flags:
   ```cpp
   // In InitState::checkSpecificTransitions():
   if (flags.requestNormalOperation) {
       return getStateInstance(MachineStateId::PID_NORMAL);
   }
   ```

**Estimated Time:** 1-2 days

### Task 4.3: Document and Enforce Handler Pattern

**Problem:** Handler responsibilities are not clearly documented or enforced.

**Solution:** Create handler base class with clear contracts, add documentation.

**Files:**
- Create: `include/clevercoffee/handlers/HandlerContract.h`
- Modify: `include/clevercoffee/handlers/BaseHandler.h`
- Modify: `include/clevercoffee/handlers/BrewHandler.h`
- Modify: `include/clevercoffee/handlers/SteamHandler.h`
- Modify: `include/clevercoffee/handlers/HotWaterHandler.h`
- Modify: `include/clevercoffee/handlers/PowerHandler.h`
- Create: `docs/ARCHITECTURE.md` (update)
- Test: `test/test_hardware_control/test_main.cpp`

**Changes:**

1. **Create HandlerContract.h** - Document handler rules:
   ```cpp
   /**
    * @brief Handler Contract - Rules All Handlers Must Follow
    *
    * LAYER 2: Handlers coordinate between sensors and state machine.
    *
    * RESPONSIBILITIES:
    * - Read switches and sensors
    * - Check permissions (temperature, water tank, etc.)
    * - Set request flags based on conditions
    * - Validate inputs (debouncing, validation)
    *
    * PROHIBITIONS:
    * - NEVER control relays directly (use request flags)
    * - NEVER set machineState directly (use request flags)
    * - NEVER make high-level decisions (state machine's job)
    * - NEVER read raw hardware without going through sensor manager
    *
    * DATA FLOW: Sensors → Handlers → Request Flags → State Machine
    */
   ```

2. **Update BaseHandler** - Add compile-time safeguards:
   ```cpp
   // Add methods that explicitly forbid direct hardware control
   class BaseHandler {
   protected:
       // Delete methods that should never be called
       BaseHandler() = delete;
       void setRelayState(...) = delete;  // Prevent direct relay control
   };
   ```

3. **Add handler documentation** to each handler class:
   ```cpp
   /**
    * @class BrewHandler
    * @brief Coordinates brew switch input with state machine
    *
    * LAYER 2: Handler
    * - Reads brew switch state
    * - Checks permissions (water tank, temperature)
    * - Sets requestBrewStart/requestBrewStop flags
    *
    * Does NOT control relays directly.
    * Does NOT transition states directly.
    */
   ```

**Estimated Time:** 2-3 days

### Task 4.4: Centralize Request Flag Management

**Problem:** Request flags scattered across g_state, no centralized management.

**Solution:** Create RequestCoordinator to manage all state transition requests.

**Files:**
- Create: `include/clevercoffee/coordinators/RequestCoordinator.h`
- Create: `src/coordinators/RequestCoordinator.cpp`
- Modify: `include/clevercoffee/context/SystemContext.h`
- Modify: `src/state/MachineStateContext.cpp`
- Test: `test/test_system_context/test_main.cpp`

**Changes:**

1. **Create RequestCoordinator.h**:
   ```cpp
   /**
    * @brief Centralized request flag management
    *
    * All state transition requests go through this coordinator.
    * Provides thread-safe request flag operations and logging.
    */
   class RequestCoordinator {
   public:
       // Request methods (called by handlers)
       void requestBrewStart() noexcept;
       void requestBrewStop() noexcept;
       void requestSteamStart() noexcept;
       void requestHotWaterStart() noexcept;
       void requestBackflushStart() noexcept;
       void requestSensorError() noexcept;

       // Check and consume methods (called by state machine)
       bool consumeRequestBrewStart() noexcept;
       bool consumeRequestBrewStop() noexcept;

       // Query methods
       bool hasPendingRequests() const noexcept;
   };
   ```

2. **Add to SystemContext**:
   ```cpp
   class SystemContext {
       RequestCoordinator& requestCoordinator() noexcept;
   private:
       RequestCoordinator requestCoordinator_;
   };
   ```

3. **Update handlers** to use RequestCoordinator:
   ```cpp
   // BEFORE:
   g_state.machine.flags.requestBrewStart = true;

   // AFTER:
   context.requestCoordinator().requestBrewStart();
   ```

**Estimated Time:** 3-4 days

### Task 4.5: Add Sensor Data Validation Layer

**Problem:** Sensors can return invalid values, validation scattered.

**Solution:** Create SensorValidator to centralize validation logic.

**Files:**
- Create: `include/clevercoffee/sensors/SensorValidator.h`
- Create: `src/sensors/SensorValidator.cpp`
- Modify: `src/sensors/SensorManager.cpp`
- Test: `test/test_sensor_manager/test_main.cpp`

**Changes:**

1. **Create SensorValidator.h**:
   ```cpp
   /**
    * @brief Centralized sensor data validation
    *
    * Validates sensor readings before they're used by the state machine.
    * LAYER 1.5: Sits between sensors and handlers.
    */
   class SensorValidator {
   public:
       static bool isValidTemperature(double temp) noexcept;
       static bool isValidPressure(float pressure) noexcept;
       static bool isValidWeight(float weight) noexcept;
       static bool isValidSwitchState(uint8_t state) noexcept;
   };
   ```

2. **Update sensors** to use validator:
   ```cpp
   // In checkWeight():
   const float weight = getScaleWeight();
   if (!SensorValidator::isValidWeight(weight)) {
       g_state.sensors.scaleFailure = true;
       return g_state.sensors.lastValidWeight;
   }
   ```

**Estimated Time:** 2-3 days

### Task 4.6: Remove scaleHandler Decision Logic (Part 2)

**Problem:** scaleHandler still makes decisions about brew-by-weight fallback.

**Solution:** Move fallback logic into BrewRunningState.

**Files:**
- Modify: `include/clevercoffee/scaleHandler.h`
- Modify: `src/state/states/BrewStates.cpp`
- Test: `test/test_emergency_transitions/test_main.cpp`

**Changes:**

1. **scaleHandler.h** - Remove fallback logic (lines 46-59):
   ```cpp
   // BEFORE (lines 46-59):
   if (brewByWeightEnabled && brewByTimeEnabled) {
       LOG(INFO, "Activating brew-by-time fallback...");
       g_state.sensors.brewByWeightFallbackActive = true;
   }
   // ❌ Handler deciding what to do

   // AFTER:
   if (!connected) {
       g_state.sensors.scaleConnectionLost = true;
   }
   // ✅ Just report connection status
   ```

2. **BrewRunningState** - Add fallback logic:
   ```cpp
   MachineState* BrewRunningState::checkSpecificTransitions(MachineStateContext& context) {
       // Check scale connection
       if (context.isScaleConnectionLost()) {
           const bool brewByWeightEnabled = Config::getInstance().brewByWeightEnabled.get();
           const bool brewByTimeEnabled = Config::getInstance().brewByTimeEnabled.get();

           if (brewByWeightEnabled && brewByTimeEnabled) {
               // Activate fallback - continue brewing by time
               context.setBrewByWeightFallback(true);
               LOG(INFO, "State machine: Activating brew-by-time fallback");
           } else if (brewByWeightEnabled) {
               // No fallback available - stop brew
               return getStateInstance(MachineStateId::SENSOR_ERROR);
           }
       }

       // ... rest of transition logic
   }
   ```

**Estimated Time:** 2-3 days

### Task 4.7: Add Comprehensive Testing

**Problem:** Need to verify separation of concerns is enforced.

**Solution:** Add tests that verify handlers don't overstep boundaries.

**Files:**
- Create: `test/test_separation_of_concerns/test_main.cpp`
- Modify: `test/test_hardware_control/test_main.cpp`

**Test Coverage:**

```cpp
TEST(SeparationOfConcerns, SensorOnlyObserves) {
    // Verify checkWeight() doesn't set request flags
    MockMachineStateContext mockContext;
    EXPECT_CALL(mockContext, setRequestBrewStop()).Times(0);  // Never called

    checkWeight();  // Should only read, not request
}

TEST(SeparationOfConcerns, HandlerOnlySetsFlags) {
    // Verify BrewHandler doesn't control relays
    MockRelay heaterRelay, pumpRelay, valveRelay;
    EXPECT_CALL(heaterRelay, turnOn()).Times(0);  // Never called
    EXPECT_CALL(pumpRelay, turnOn()).Times(0);    // Never called

    BrewHandler handler;
    handler.process();  // Should only set flags
}

TEST(SeparationOfConcerns, StateMachineControlsHardware) {
    // Verify state machine controls relays
    MockRelay pumpRelay;
    EXPECT_CALL(pumpRelay, turnOn()).Times(1);   // Called once
    EXPECT_CALL(pumpRelay, turnOff()).Times(1);  // Called once

    // Simulate brew cycle
    context.requestBrewStart();
    // ... run state machine
}
```

**Estimated Time:** 3-4 days

### Task 4.8: Documentation and Diagrams

**Problem:** Architecture needs to be clearly documented.

**Solution:** Create comprehensive documentation with diagrams.

**Files:**
- Create: `docs/ARCHITECTURE.md`
- Update: `docs/REFACTORING_SUMMARY.md`
- Create: `docs/SEPARATION_OF_CONCERNS.md`
- Update: `README.md`

**Content:**

1. **ARCHITECTURE.md**:
   - System overview with ASCII diagrams
   - Three-layer model explained
   - Data flow examples
   - Component interaction diagrams

2. **SEPARATION_OF_CONCERNS.md**:
   - Detailed explanation of each layer
   - Rules and anti-patterns
   - Migration guide for existing code
   - Examples of correct vs incorrect code

3. **REFACTORING_SUMMARY.md**:
   - Add Phase 4 section
   - Update metrics and progress

**Estimated Time:** 2-3 days

## Success Criteria

### Code Quality

- [ ] Zero instances of sensors setting request flags
- [ ] Zero instances of direct `machineState =` assignment (except state machine internals)
- [ ] Zero instances of handlers controlling relays directly
- [ ] All state transitions go through request flags
- [ ] All handlers documented with LAYER 2 contract
- [ ] All sensors documented with LAYER 1 contract

### Testing

- [ ] 100% test coverage for RequestCoordinator
- [ ] Tests verifying handler boundaries (handlers don't overstep)
- [ ] Tests verifying sensor boundaries (sensors only observe)
- [ ] Tests verifying state machine decisions (all logic in states)
- [ ] Integration tests for complete data flow

### Documentation

- [ ] ARCHITECTURE.md with comprehensive diagrams
- [ ] SEPARATION_OF_CONCERNS.md with examples
- [ ] All handlers have clear contract documentation
- [ ] Code comments explain data flow at decision points

## Risks and Mitigations

### Risk 1: Breaking Changes

**Risk:** Moving logic from handlers to state machine could break existing behavior.

**Mitigation:**
- Comprehensive tests before refactoring
- Feature flags for gradual rollout
- Extensive manual testing after each task
- Rollback plan (git revert)

### Risk 2: Performance Impact

**Risk:** Additional abstraction layers could impact performance.

**Mitigation:**
- Benchmark before/after
- Use `noexcept` and `inline` appropriately
- Profile hot paths
- Optimize after correctness verified

### Risk 3: Increased Complexity

**Risk:** More layers and coordinators could make code harder to understand.

**Mitigation:**
- Clear documentation with diagrams
- Consistent naming conventions
- Examples for each pattern
- Code reviews focusing on clarity

## Migration Strategy

### Phase 4A: Foundation (Week 1)
- Task 4.1: Remove sensor decision logic
- Task 4.3: Document handler pattern
- Task 4.5: Add sensor validation

### Phase 4B: Coordination (Week 1-2)
- Task 4.2: Remove direct state assignment
- Task 4.4: Centralize request flags
- Task 4.6: Remove scaleHandler logic (part 2)

### Phase 4C: Verification (Week 2-3)
- Task 4.7: Add comprehensive testing
- Task 4.8: Documentation and diagrams
- Final integration testing

## Rollback Plan

If issues arise:

1. **Per-task rollback**: Each task is committed separately, can revert individually
2. **Phase rollback**: Use `git revert <commit-range>` to revert entire Phase 4
3. **Document issues**: Create JIRA ticket for follow-up

## Open Questions

1. **Error handling granularity**: Should sensors set `scaleFailure` or report error codes?
   - **Recommendation**: Sensors set failure flags, state machine decides how to handle

2. **Request flag consumption**: Should flags auto-reset or explicit consume?
   - **Recommendation**: Explicit consume for better testability and logging

3. **Handler timing**: Should handlers run every loop or on timer?
   - **Recommendation**: Keep current timer-based approach (already working well)

## Next Steps

1. **Review this design** with user
2. **Prioritize tasks** based on user concerns
3. **Create detailed implementation plan** using superpowers:writing-plans
4. **Execute** using superpowers:subagent-driven-development

---

**Document Status:** Ready for Review
**Last Updated:** 2025-12-28
**Version:** 1.0
