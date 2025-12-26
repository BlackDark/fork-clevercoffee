# Critical Bug Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix 8 critical bugs in the CleverCoffee ESP32 codebase to ensure memory safety, ISR correctness, and safety-critical functionality.

**Architecture:** Sequential fixes progressing from most critical (memory leaks, ISR safety) to important (error handling, null checks). Each fix is isolated and can be committed independently.

**Tech Stack:** C++17, ESP32 Arduino framework, modern C++ patterns (smart pointers, atomic)

**Estimated Total Time:** 5-6 hours

---

## Critical Bugs Overview

| Priority | Bug ID | File | Issue | Est. Time |
|----------|--------|------|-------|-----------|
| 1 | MEM-001 | TempSensorTSIC.cpp | Memory leak (missing destructor) | 15 min |
| 2 | MEM-002 | TempSensorDallas.cpp | Memory leak (missing destructors) | 15 min |
| 3 | ISR-001 | isr.h | Race condition (no volatile/atomic) | 30 min |
| 4 | STATE-001 | StateFactory.h | Null pointer returns unchecked | 30 min |
| 5 | STATE-002 | ErrorStates.cpp | Static variable persistence bug | 20 min |
| 6 | HW-001 | GPIOPin.cpp | Const violation in setType() | 5 min |
| 7 | CTRL-001 | ProcessController.cpp | Missing null check | 5 min |
| 8 | CTRL-002 | ProcessController.cpp | Emergency temp detection issues | 1 hour |

---

## Task 1: Fix TempSensorTSIC Memory Leak

**Files:**
- Modify: `include/clevercoffee/hardware/tempsensors/TempSensorTSIC.h:1-30`
- Modify: `src/hardware/tempsensors/TempSensorTSIC.cpp:1-25`

**Context:** TempSensorTSIC allocates `ZACwire` with `new` but has no destructor, causing memory leak every time the sensor is reinitialized. This is critical on ESP32's constrained heap (~200KB).

### Step 1: Examine current implementation

Run: `cat include/clevercoffee/hardware/tempsensors/TempSensorTSIC.h`

Expected output shows:
```cpp
class TempSensorTSIC final : public TempSensor {
  private:
    ZACwire* tsicSensor_;  // Raw pointer, no destructor
};
```

Run: `cat src/hardware/tempsensors/TempSensorTSIC.cpp`

Expected output shows no destructor definition.

### Step 2: Add destructor declaration to header

**File:** `include/clevercoffee/hardware/tempsensors/TempSensorTSIC.h`

Add after the constructor declaration (around line 18):

```cpp
class TempSensorTSIC final : public TempSensor {
  public:
    explicit TempSensorTSIC(int GPIOPin);
    ~TempSensorTSIC() override;  // ADD THIS LINE
    
  protected:
    bool sample_temperature(double& temperature) const override;
    
  private:
    ZACwire* tsicSensor_;
};
```

**Verification:** Confirm the destructor declaration is in the public section after constructor.

### Step 3: Implement destructor in cpp file

**File:** `src/hardware/tempsensors/TempSensorTSIC.cpp`

Add at the end of the file (after the existing constructor):

```cpp
TempSensorTSIC::~TempSensorTSIC() {
    if (tsicSensor_ != nullptr) {
        delete tsicSensor_;
        tsicSensor_ = nullptr;
    }
}
```

**Verification:** Check file ends with proper destructor implementation.

### Step 4: Compile to verify no syntax errors

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors, compilation succeeds or shows other unrelated warnings.

### Step 5: Commit this fix

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
git add include/clevercoffee/hardware/tempsensors/TempSensorTSIC.h
git add src/hardware/tempsensors/TempSensorTSIC.cpp
git commit -m "fix(hardware): add destructor to TempSensorTSIC to prevent memory leak

- Properly delete ZACwire instance in destructor
- Add null check before deletion
- Fixes memory leak on every sensor reinitialization"
```

Expected: Clean commit with message shown.

---

## Task 2: Fix TempSensorDallas Memory Leak

**Files:**
- Modify: `include/clevercoffee/hardware/tempsensors/TempSensorDallas.h:1-35`
- Modify: `src/hardware/tempsensors/TempSensorDallas.cpp:1-35`

**Context:** TempSensorDallas allocates TWO objects with `new` (OneWire and DallasTemperature) but has no destructor. Both are leaked. Must delete in reverse order of allocation (DallasTemperature first, then OneWire, since Dallas depends on OneWire).

### Step 1: Examine current implementation

Run: `cat include/clevercoffee/hardware/tempsensors/TempSensorDallas.h | grep -A 10 "private:"`

Expected output shows:
```cpp
  private:
    OneWire*           oneWire_;
    DallasTemperature* dallasSensor_;
```

Run: `cat src/hardware/tempsensors/TempSensorDallas.cpp | head -20`

Expected output shows constructor with allocations but no destructor.

### Step 2: Add destructor declaration to header

**File:** `include/clevercoffee/hardware/tempsensors/TempSensorDallas.h`

Add after the constructor declaration (around line 20):

```cpp
class TempSensorDallas final : public TempSensor {
  public:
    explicit TempSensorDallas(int GPIOPin);
    ~TempSensorDallas() override;  // ADD THIS LINE
    
  protected:
    bool sample_temperature(double& temperature) const override;
    
  private:
    OneWire*           oneWire_;
    DallasTemperature* dallasSensor_;
};
```

**Verification:** Destructor is declared after constructor in public section.

### Step 3: Implement destructor in cpp file

**File:** `src/hardware/tempsensors/TempSensorDallas.cpp`

Add at the end of the file (after the existing constructor):

```cpp
TempSensorDallas::~TempSensorDallas() {
    // Delete in reverse order of allocation
    // Dallas depends on OneWire, so delete Dallas first
    if (dallasSensor_ != nullptr) {
        delete dallasSensor_;
        dallasSensor_ = nullptr;
    }
    if (oneWire_ != nullptr) {
        delete oneWire_;
        oneWire_ = nullptr;
    }
}
```

**Verification:** Check destructor properly deletes both pointers in correct order.

### Step 4: Compile to verify no syntax errors

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors related to TempSensorDallas, compilation succeeds.

### Step 5: Commit this fix

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
git add include/clevercoffee/hardware/tempsensors/TempSensorDallas.h
git add src/hardware/tempsensors/TempSensorDallas.cpp
git commit -m "fix(hardware): add destructor to TempSensorDallas to prevent memory leak

- Properly delete DallasTemperature and OneWire instances in destructor
- Delete in correct order: Dallas first (depends on OneWire), then OneWire
- Add null checks before deletion
- Fixes double memory leak on every sensor reinitialization"
```

Expected: Clean commit with message shown.

---

## Task 3: Fix ISR Race Condition on PID Output

**Files:**
- Modify: `include/clevercoffee/GlobalState.h:80-120` (ProcessState struct)
- Modify: `include/clevercoffee/isr.h:1-50`

**Context:** The ISR (`onTimer`) reads `g_state.process.pidOutput` without synchronization. The main loop writes this value. On ESP32 (single core but with ISRs), this creates a race condition where the ISR might read a partially-written value. Solution: make pidOutput volatile (for compiler) and isrCounter atomic (for safe increment).

### Step 1: Examine current ISR code

Run: `cat include/clevercoffee/isr.h | head -35`

Expected output shows:
```cpp
void IRAM_ATTR onTimer() {
    if (g_state.process.pidOutput <= g_state.timing.isrCounter) {
        // ...
    }
}
```

### Step 2: Examine GlobalState ProcessState struct

Run: `grep -A 5 "struct ProcessState" include/clevercoffee/GlobalState.h`

Expected output shows member variables including `double pidOutput`.

### Step 3: Make pidOutput volatile in GlobalState

**File:** `include/clevercoffee/GlobalState.h`

Find the ProcessState struct (around line 85-120) and locate `double pidOutput`. Change it to:

```cpp
struct ProcessState {
    // ... other members ...
    volatile double pidOutput = 0.0;  // CHANGED: add volatile
    // ... other members ...
};
```

**Rationale:** The `volatile` keyword tells the compiler that this value might change unexpectedly (by ISR), so it must always read from memory, not cache in a register.

**Verification:** Confirm pidOutput is now `volatile double`.

### Step 4: Add atomic header to isr.h

**File:** `include/clevercoffee/isr.h`

Add at the top of the file with other includes:

```cpp
#ifndef ISR_H
#define ISR_H

#include <atomic>  // ADD THIS LINE
#include "clevercoffee/GlobalState.h"
// ... rest of includes ...
```

**Verification:** `#include <atomic>` is present at top of file.

### Step 5: Update ISR to read pidOutput safely

**File:** `include/clevercoffee/isr.h`

Replace the `onTimer()` function's pidOutput access. Find the section that looks like:

```cpp
void IRAM_ATTR onTimer() {
    timerAlarmWrite(g_state.machine.timer, 10000, true);
    
    if (g_state.process.pidOutput <= g_state.timing.isrCounter) {
        g_state.hardware.heaterRelay->off();
    } else {
        g_state.hardware.heaterRelay->on();
    }
    
    g_state.timing.isrCounter += 10;
    
    if (g_state.timing.isrCounter >= g_state.process.windowSize) {
        g_state.timing.isrCounter = 0;
    }
}
```

Replace with:

```cpp
void IRAM_ATTR onTimer() {
    timerAlarmWrite(g_state.machine.timer, 10000, true);
    
    // Read volatile pidOutput once for consistency
    const double currentPidOutput = g_state.process.pidOutput;
    const unsigned int currentCounter = g_state.timing.isrCounter;
    
    if (currentPidOutput <= currentCounter) {
        g_state.hardware.heaterRelay->off();
    } else {
        g_state.hardware.heaterRelay->on();
    }
    
    unsigned int newCounter = currentCounter + 10;
    if (newCounter >= g_state.process.windowSize) {
        newCounter = 0;
    }
    g_state.timing.isrCounter = newCounter;
}
```

**Rationale:** Reading the volatile variable once into a local variable ensures we use the same value for the comparison. This makes the PWM timing deterministic.

**Verification:** Confirm ISR reads pidOutput once, uses local variable for comparison.

### Step 6: Compile to verify no syntax errors

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors, compilation succeeds.

### Step 7: Commit this fix

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
git add include/clevercoffee/GlobalState.h
git add include/clevercoffee/isr.h
git commit -m "fix(core): add volatile qualifier and safe reading in ISR

- Make pidOutput volatile to prevent compiler caching
- Read volatile pidOutput once in ISR to ensure consistent PWM timing
- Prevents race condition between main loop and ISR
- Fixes potential heater on/off timing issues"
```

Expected: Clean commit with message shown.

---

## Task 4: Fix State Factory Null Pointer Returns

**Files:**
- Modify: `include/clevercoffee/state/StateFactory.h:15-30`
- Modify: `src/state/StateInfo.cpp:25-50` (verification)

**Context:** `StateFactory::getStateInstance()` returns `nullptr` if a state is not registered, but most callers don't check for null. This can cause crashes. Solution: Fail fast with an explicit ESP.restart() on invalid state.

### Step 1: Examine current StateFactory implementation

Run: `cat include/clevercoffee/state/StateFactory.h`

Expected output shows:
```cpp
inline MachineState* getStateInstance(MachineStateId id) {
    if (const auto* info = getStateInfo(id)) {
        if (info->getInstance) {
            return info->getInstance();
        }
    }
    return nullptr;  // PROBLEM: Unchecked by callers
}
```

### Step 2: Find one example of unchecked usage

Run: `grep -n "getStateInstance" src/state/states/BrewStates.cpp | head -3`

Expected output shows calls without null checks like:
```
31:    return getStateInstance(MachineStateId::BREW_PREINFUSION);
```

### Step 3: Add Logger include to StateFactory

**File:** `include/clevercoffee/state/StateFactory.h`

Add at the top with other includes:

```cpp
#ifndef STATE_FACTORY_H
#define STATE_FACTORY_H

#include "clevercoffee/Logger.h"  // ADD THIS
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/MachineState.h"
// ... rest ...
```

**Verification:** Logger.h is included.

### Step 4: Update getStateInstance to fail fast

**File:** `include/clevercoffee/state/StateFactory.h`

Replace the entire `getStateInstance` function:

```cpp
inline MachineState* getStateInstance(MachineStateId id) {
    if (const auto* info = getStateInfo(id)) {
        if (info->getInstance) {
            return info->getInstance();
        }
    }
    // CRITICAL: State not found or not registered
    // This indicates a fatal configuration error - restart immediately
    LOGF(FATAL, "CRITICAL: State not registered (ID=%d). System will restart.", static_cast<int>(id));
    ESP.restart();
    return nullptr;  // Unreachable, but satisfies return type
}
```

**Rationale:** By restarting instead of returning nullptr, we:
1. Prevent undefined behavior from null dereference
2. Create a clear failure signal
3. Force logs to show the fatal error
4. Device will restart and retry initialization

**Verification:** Function includes FATAL log and ESP.restart().

### Step 5: Compile to verify no syntax errors

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors, compilation succeeds.

### Step 6: Commit this fix

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
git add include/clevercoffee/state/StateFactory.h
git commit -m "fix(state): make StateFactory::getStateInstance fail-fast on invalid state

- Replace null return with ESP.restart() for unregistered states
- Log FATAL error with state ID before restart
- Prevents null pointer dereference crashes
- Ensures clear error signal in logs"
```

Expected: Clean commit with message shown.

---

## Task 5: Fix Static Variable Persistence Bug in EepromErrorState

**Files:**
- Modify: `include/clevercoffee/state/states/ErrorStates.h:45-70`
- Modify: `src/state/states/ErrorStates.cpp:110-130`

**Context:** `EepromErrorState` uses a static variable `eepromErrorStartTime` that never resets when the state is re-entered. If EEPROM error occurs twice, the second time uses the old timestamp, causing immediate timeout. Solution: Use instance variable with reset in `onEntry()`.

### Step 1: Examine current EepromErrorState implementation

Run: `grep -A 15 "EepromErrorState::checkSpecificTransitions" src/state/states/ErrorStates.cpp`

Expected output shows:
```cpp
static unsigned long eepromErrorStartTime = 0;  // PROBLEM: Never reset properly
```

### Step 2: Check if ErrorStates.h has instance variable

Run: `grep -A 5 "class EepromErrorState" include/clevercoffee/state/states/ErrorStates.h`

Expected output shows just public/protected sections, need to check for private members.

### Step 3: Add instance variable to EepromErrorState header

**File:** `include/clevercoffee/state/states/ErrorStates.h`

Find the `EepromErrorState` class (around line 60-75) and add a private member:

```cpp
class EepromErrorState final : public BaseState<MachineStateId::EEPROM_ERROR, EepromErrorState> {
  public:
    // ... existing public methods ...
    
  protected:
    void onEntryImpl(MachineStateContext& context) override;
    void updateImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
    
  private:
    unsigned long errorStartTime_ = 0;  // ADD THIS LINE
};
```

**Verification:** Private instance variable `errorStartTime_` is declared with initialization.

### Step 4: Update onEntry to reset the timer

**File:** `src/state/states/ErrorStates.cpp`

Find `EepromErrorState::onEntryImpl` method (around line 105-115) and ensure it looks like:

```cpp
void EepromErrorState::onEntryImpl(MachineStateContext& context) {
    context.logStateTransition(MachineStateId::INIT, MachineStateId::EEPROM_ERROR, "EEPROM Error detected");
    errorStartTime_ = millis();  // RESET timer on entry
    // ... rest of implementation ...
}
```

If `onEntryImpl` doesn't exist or doesn't reset the timer, add/update it:

```cpp
void EepromErrorState::onEntryImpl(MachineStateContext& context) {
    LOGF(ERROR, "Entering EEPROM error state");
    errorStartTime_ = millis();  // Initialize timer on state entry
    // ... rest of any existing logic ...
}
```

**Verification:** `onEntryImpl` resets `errorStartTime_ = millis()`.

### Step 5: Update checkSpecificTransitions to use instance variable

**File:** `src/state/states/ErrorStates.cpp`

Find `EepromErrorState::checkSpecificTransitions` (around line 114-130) and replace it:

```cpp
MachineState* EepromErrorState::checkSpecificTransitions(MachineStateContext& context) {
    constexpr unsigned long EEPROM_RECOVERY_TIMEOUT = 300000;  // 5 minutes
    
    if (millis() - errorStartTime_ > EEPROM_RECOVERY_TIMEOUT) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "EEPROM recovery timeout");
        return getStateInstance(MachineStateId::PID_NORMAL);
    }
    
    return nullptr;
}
```

**Key changes:**
- Use instance variable `errorStartTime_` instead of static
- Remove the static variable completely
- Timeout logic now works correctly on repeated errors

**Verification:** Function uses instance variable `errorStartTime_`, no static variable.

### Step 6: Remove the old static variable

**File:** `src/state/states/ErrorStates.cpp`

Search for any remaining `static unsigned long eepromErrorStartTime` and DELETE those lines.

Run: `grep -n "eepromErrorStartTime" src/state/states/ErrorStates.cpp`

Expected: No matches (all references removed).

### Step 7: Compile to verify no syntax errors

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors, compilation succeeds.

### Step 8: Commit this fix

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
git add include/clevercoffee/state/states/ErrorStates.h
git add src/state/states/ErrorStates.cpp
git commit -m "fix(state): use instance variable for EEPROM error timer instead of static

- Add errorStartTime_ as instance member variable
- Reset timer in onEntryImpl on every state entry
- Remove static variable that persisted across state entries
- Fixes timeout behavior on repeated EEPROM errors"
```

Expected: Clean commit with message shown.

---

## Task 6: Fix Const Violation in GPIOPin::setType()

**Files:**
- Modify: `include/clevercoffee/hardware/GPIOPin.h:65-70`
- Modify: `src/hardware/GPIOPin.cpp:28-40`

**Context:** `setType()` is marked `const` but modifies hardware state (calls `pinMode()`, `digitalWrite()`). This violates const semantics. Solution: Remove `const` qualifier.

### Step 1: Examine current implementation

Run: `grep -B 2 -A 5 "void.*setType" include/clevercoffee/hardware/GPIOPin.h`

Expected output shows:
```cpp
void setType(const Type pinType) const;  // PROBLEM: const method modifies hardware
```

Run: `grep -B 2 -A 8 "void GPIOPin::setType" src/hardware/GPIOPin.cpp`

Expected output shows:
```cpp
void GPIOPin::setType(const Type pinType) const {
    switch (pinType) {
        case OUT:
            pinMode(pin, OUTPUT);  // Hardware modification in const method
            break;
```

### Step 2: Remove const from header declaration

**File:** `include/clevercoffee/hardware/GPIOPin.h`

Find the `setType` method declaration (around line 68) and change from:

```cpp
void setType(const Type pinType) const;  // WRONG: const method
```

To:

```cpp
void setType(Type pinType);  // CORRECT: non-const method
```

**Note:** Remove both the `const Type` parameter (should just be `Type`) and the trailing `const` (const method qualifier).

**Verification:** Line shows `void setType(Type pinType);` with no `const` qualifiers.

### Step 3: Update the implementation in cpp file

**File:** `src/hardware/GPIOPin.cpp`

Find the `setType` method implementation (around line 31) and change the signature from:

```cpp
void GPIOPin::setType(const Type pinType) const {
```

To:

```cpp
void GPIOPin::setType(Type pinType) {
```

**Rationale:**
- Const method qualifier (`const` at end) means "this method doesn't modify member variables"
- But `setType` modifies Arduino hardware state
- Remove const to be semantically correct
- `const Type` parameter parameter is also unnecessary; just use `Type`

**Verification:** Function signature is now `void GPIOPin::setType(Type pinType) {`.

### Step 4: Compile to verify no syntax errors

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors, compilation succeeds.

### Step 5: Commit this fix

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
git add include/clevercoffee/hardware/GPIOPin.h
git add src/hardware/GPIOPin.cpp
git commit -m "fix(hardware): remove const from GPIOPin::setType() to match semantics

- Remove const method qualifier (method modifies hardware)
- Remove const parameter qualifier (unnecessary)
- Fixes const correctness violation
- Methods that modify hardware should not be const"
```

Expected: Clean commit with message shown.

---

## Task 7: Fix Missing Null Check in ProcessController

**Files:**
- Modify: `src/control/ProcessController.cpp:135-145`

**Context:** `updateTemperature()` has a fallback path that calls `hardwareManager_->getTempSensor()` without first checking if `hardwareManager_` is non-null. Solution: Add null check before dereferencing.

### Step 1: Examine current implementation

Run: `sed -n '130,150p' src/control/ProcessController.cpp`

Expected output shows:
```cpp
void ProcessController::updateTemperature() {
    if (sensorManager_ != nullptr) {
        temperature_ = sensorManager_->getCurrentTemperature();
        // ...
    } else if (hardwareManager_->getTempSensor() != nullptr) {  // PROBLEM: no check for hardwareManager_
        temperature_ = hardwareManager_->getTempSensor()->getCurrentTemperature();
        // ...
    }
}
```

### Step 2: Add null check for hardwareManager_

**File:** `src/control/ProcessController.cpp`

Find the fallback else-if statement (around line 136-140) and change from:

```cpp
} else if (hardwareManager_->getTempSensor() != nullptr) {
```

To:

```cpp
} else if (hardwareManager_ != nullptr && hardwareManager_->getTempSensor() != nullptr) {
```

This ensures `hardwareManager_` is valid before calling `getTempSensor()`.

**Verification:** The else-if condition now checks both `hardwareManager_ != nullptr` AND `getTempSensor() != nullptr`.

### Step 3: Compile to verify no syntax errors

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -50`

Expected: No errors, compilation succeeds.

### Step 4: Commit this fix

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
git add src/control/ProcessController.cpp
git commit -m "fix(control): add null check for hardwareManager before use

- Check hardwareManager_ is non-null before dereferencing
- Prevents potential null pointer dereference in temperature update fallback
- Maintains defensive programming practice"
```

Expected: Clean commit with message shown.

---

## Task 8: Fix Emergency Temperature Detection Issues

**Files:**
- Modify: `include/clevercoffee/Config.h` (add configuration parameter)
- Modify: `src/control/ProcessController.h:35-50` (add instance variables)
- Modify: `src/control/ProcessController.cpp:280-310`

**Context:** Emergency temperature detection is hardcoded without debouncing, hysteresis, or sensor validation. This safety-critical code needs improvement.

### Step 1: Add emergency temperature config parameter

**File:** `include/clevercoffee/Config.h`

Find the configuration definitions (around line 200-400) and add a new config parameter. Look for existing temperature configs like:

```cpp
ParamDef<double> brewTempOffset{"brewTempOffset", 0.0, -20.0, 20.0};
```

Add nearby:

```cpp
ParamDef<double> emergencyStopTemp{"emergencyStopTemp", 150.0, 120.0, 180.0};
ParamDef<double> emergencyStopHysteresis{"emergencyStopHysteresis", 5.0, 1.0, 15.0};
```

**Verification:** Both new config parameters are declared with reasonable defaults.

### Step 2: Add instance variables for debouncing

**File:** `include/clevercoffee/control/ProcessController.h`

Find the ProcessController class private section (around line 35-50) and add:

```cpp
  private:
    // ... existing members ...
    
    // Emergency temperature debouncing
    static constexpr int EMERGENCY_TEMP_DEBOUNCE_COUNT = 3;  // Require 3 consecutive readings
    int emergencyTempReadingCount_ = 0;
```

**Verification:** Two new private members are added for emergency temp debouncing.

### Step 3: Implement improved testEmergencyConditions method

**File:** `src/control/ProcessController.cpp`

Find the `testEmergencyConditions()` method (around line 286-297) and replace it completely:

```cpp
bool ProcessController::testEmergencyConditions() {
    const double emergencyTemp = Config::getInstance().emergencyStopTemp.get();
    const double hysteresis = Config::getInstance().emergencyStopHysteresis.get();
    const double sensorMinValid = -50.0;
    const double sensorMaxValid = 200.0;
    
    // STEP 1: Check for sensor disconnection or invalid reading
    if (temperature_ < sensorMinValid || temperature_ > sensorMaxValid) {
        LOGF(ERROR, "Emergency: Invalid temperature reading (%.1f°C outside valid range)", temperature_);
        emergencyStop();
        return true;
    }
    
    // STEP 2: Hysteresis-based emergency detection with debouncing
    if (temperature_ > emergencyTemp) {
        emergencyTempReadingCount_++;
        LOGF(WARNING, "High temperature detected: %.1f°C (reading %d/%d)",
             temperature_, emergencyTempReadingCount_, EMERGENCY_TEMP_DEBOUNCE_COUNT);
        
        // Require multiple consecutive high readings to trigger emergency
        if (emergencyTempReadingCount_ >= EMERGENCY_TEMP_DEBOUNCE_COUNT) {
            LOGF(ERROR, "Emergency: Temperature too high (%.1f°C > %.1f°C limit)!",
                 temperature_, emergencyTemp);
            emergencyStop();
            return true;
        }
    } else if (temperature_ < (emergencyTemp - hysteresis)) {
        // Reset counter only when temperature drops below threshold minus hysteresis
        if (emergencyTempReadingCount_ > 0) {
            LOGF(INFO, "Temperature normalized. Resetting emergency counter.");
            emergencyTempReadingCount_ = 0;
        }
    }
    // If temperature is between (threshold - hysteresis) and threshold, keep counter as-is
    // This implements hysteresis to prevent oscillation
    
    return false;
}
```

**Key improvements:**
1. **Debouncing:** Requires 3 consecutive readings above threshold
2. **Hysteresis:** Counter resets only below (threshold - hysteresis)
3. **Sensor validation:** Checks for disconnected/invalid readings
4. **Logging:** Clear warning messages with current values
5. **Configurable:** Uses Config values instead of hardcoded constants

**Verification:** Method includes sensor validation, debouncing, hysteresis, and configurable thresholds.

### Step 4: Reset emergency counter on state change (optional enhancement)

**File:** `src/control/ProcessController.cpp`

Optional: If there's an `onStateChange()` or similar method, reset the counter:

```cpp
void ProcessController::onStateChange(MachineStateId newState) {
    // Reset emergency temp counter when entering safe states
    if (newState == MachineStateId::INIT || newState == MachineStateId::STANDBY) {
        emergencyTempReadingCount_ = 0;
    }
}
```

**Note:** Only add this if a state change handler exists. If not, skip this step - the hysteresis logic is sufficient.

### Step 5: Compile to verify no syntax errors

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | head -80`

Expected: No errors related to ProcessController, compilation succeeds.

### Step 6: Commit this fix

```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
git add include/clevercoffee/Config.h
git add include/clevercoffee/control/ProcessController.h
git add src/control/ProcessController.cpp
git commit -m "fix(safety): improve emergency temperature detection with debouncing and hysteresis

- Add configurable emergencyStopTemp and emergencyStopHysteresis parameters
- Implement debouncing: require 3 consecutive high readings
- Add hysteresis: counter resets only below (threshold - hysteresis)
- Validate sensor readings (detect disconnection)
- Replace hardcoded threshold with config value
- Add detailed logging for temperature warnings
- Prevents false emergency triggers from noise/oscillation"
```

Expected: Clean commit with message shown.

---

## Verification Checklist

After completing all 8 tasks, verify the following:

### Compilation
```bash
cd /Users/marbaced/projects/forks/fork-clevercoffee
~/.platformio/penv/bin/pio run -e esp32_usb -s
```
Expected: ✅ Compiles successfully with no errors

### Git History
```bash
git log --oneline | head -8
```
Expected: All 8 commits present in order

### Memory Leaks Fixed
- ✅ TempSensorTSIC has destructor
- ✅ TempSensorDallas has destructor with proper deletion order

### ISR Safety
- ✅ pidOutput is volatile
- ✅ ISR reads pidOutput into local variable

### State Machine Safety
- ✅ getStateInstance calls ESP.restart() on invalid state
- ✅ EepromErrorState uses instance variable, not static
- ✅ No null pointer dereference risks

### Code Quality
- ✅ GPIOPin::setType() const violation removed
- ✅ ProcessController checks hardwareManager_ before use
- ✅ Emergency temp detection has debouncing, hysteresis, validation

---

## Summary

**Total bugs fixed:** 8
**Total commits:** 8
**Estimated total time:** 5-6 hours
**Overall impact:** All critical safety and memory issues resolved

The codebase will be significantly more robust after these fixes. All remaining work is technical debt and non-critical improvements covered in the original analysis report.

---

## Next Steps After This Plan

Once all 8 tasks are complete:

1. **Test on actual hardware** to verify:
   - No memory leaks over extended runtime
   - Emergency temp detection works correctly
   - ISR timing is stable

2. **Address high-priority fixes** from the original analysis:
   - ESP32 pin conflict documentation
   - HX711Scale timeout protection
   - Pressure sensor refactoring

3. **Technical debt reduction:**
   - Migrate more hardware classes to std::unique_ptr
   - Add sensor operation watchdog
   - Improve slow loop logging
