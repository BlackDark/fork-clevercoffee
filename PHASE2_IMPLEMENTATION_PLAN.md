# Phase 2 - SystemContext Accessor Implementation Plan
## g_state Elimination: Add Missing Accessors

**Date**: 2026-01-08  
**Status**: Planning → Implementation  
**Branch**: ai/claude-c23-refactor  

---

## IMPLEMENTATION STRATEGY

### Approach: Gradual Migration with Backward Compatibility

1. **Add new SystemContext accessors** (no code changes to consumers)
2. **Migrate files in Tier order** (highest impact first)
3. **Verify each tier** before moving to next
4. **Deprecate direct g_state access** gradually
5. **Final cleanup** when all files migrated

---

## PHASE 2A: CRITICAL ACCESSORS (Week 1)

### 1. Timer Management (machine.timer)

**Current Pattern:**
```cpp
// isr.h - Direct timer manipulation
if (!g_state.machine.timer || (uintptr_t)g_state.machine.timer < 0x1000) {
    g_state.machine.timer = timerBegin(0, 80, true);
    timerAttachInterrupt(g_state.machine.timer, &onTimer, true);
    timerAlarmWrite(g_state.machine.timer, 10000, true);
    timerAlarmEnable(g_state.machine.timer);
}
```

**New SystemContext Methods:**
```cpp
// In SystemContext.h
// Timer operations (wrapper around MachineStateContext)
hw_timer_t* machineTimer() noexcept;
void setMachineTimer(hw_timer_t* timer) noexcept;
bool isMachineTimerInitialized() const noexcept;

// ISR counter for animation timing
unsigned int isrCounter() const noexcept;
void setIsrCounter(unsigned int value) noexcept;
void incrementIsrCounter() noexcept;
```

**Implementation in SystemContext.cpp:**
```cpp
hw_timer_t* SystemContext::machineTimer() noexcept {
    return machineStateContext_->getTimer();
}

void SystemContext::setMachineTimer(hw_timer_t* timer) noexcept {
    machineStateContext_->setTimer(timer);
}

bool SystemContext::isMachineTimerInitialized() const noexcept {
    return machineStateContext_->getTimer() != nullptr;
}

unsigned int SystemContext::isrCounter() const noexcept {
    // Access from global state (will migrate later)
    return g_state.timing.isrCounter;
}

void SystemContext::setIsrCounter(unsigned int value) noexcept {
    g_state.timing.isrCounter = value;
}

void SystemContext::incrementIsrCounter() noexcept {
    g_state.timing.isrCounter++;
}
```

**Why coordinating with MachineStateContext:**
- MachineStateContext already owns the timer pointer
- Timer lifecycle tied to machine state
- Cleaner separation of concerns

---

### 2. Emergency Stop (machine.emergencyStop)

**Current Pattern:**
```cpp
// SystemUtils.h - Direct flag access
if (currentTemp > EmergencyStopTemp && g_state.machine.emergencyStop == false) {
    g_state.machine.emergencyStop = true;
    // ... trigger emergency stop
}
```

**New SystemContext Methods:**
```cpp
// In SystemContext.h
bool isEmergencyStopActive() const noexcept;
void setEmergencyStop(bool active) noexcept;
void triggerEmergencyStop() noexcept;
```

**Implementation:**
```cpp
// In SystemContext.cpp
bool SystemContext::isEmergencyStopActive() const noexcept {
    return g_state.machine.emergencyStop;
}

void SystemContext::setEmergencyStop(bool active) noexcept {
    g_state.machine.emergencyStop = active;
}

void SystemContext::triggerEmergencyStop() noexcept {
    g_state.machine.emergencyStop = true;
    // Could add logging or notifications here
}
```

**Thread Safety Note:**
- Consider using std::atomic<bool> in future refactor
- For now, access via SystemContext (centralized)
- All emergency stop calls go through one method

---

### 3. PID Abstraction Layer

**Current Pattern:**
```cpp
// ProcessController.cpp (22 accesses)
g_state.pid->SetTunings(kp, ki, kd, P_ON_M);
g_state.pid->Compute();
g_state.pid->SetMode(AUTOMATIC);
g_state.pid->GetKp();
// ... many more direct calls
```

**Strategy: Create wrapper methods for common PID operations**

```cpp
// In SystemContext.h
// PID Computation
void computePid() noexcept;

// PID Configuration
void setPidTunings(double kp, double ki, double kd, int ponM = 1) noexcept;
void setPidMode(int mode) noexcept;  // AUTOMATIC or MANUAL
void setPidOutputLimits(double min, double max) noexcept;
void setPidIntegratorLimits(double min, double max) noexcept;
void setPidSampleTime(int sampleTime) noexcept;
void setPidSmoothingFactor(double factor) noexcept;

// PID Getters
int pidMode() const noexcept;
double pidKp() const noexcept;
double pidKi() const noexcept;
double pidKd() const noexcept;
double pidLastPPart() const noexcept;
double pidLastIPart() const noexcept;
double pidLastDPart() const noexcept;
double pidInputError() const noexcept;
double pidDeltaInput() const noexcept;

// Direct access (for backward compat during migration)
PID* pidController() noexcept;
const PID* pidController() const noexcept;
```

**Implementation:**
```cpp
// In SystemContext.cpp
void SystemContext::computePid() noexcept {
    if (g_state.pid) {
        g_state.pid->Compute();
    }
}

void SystemContext::setPidTunings(double kp, double ki, double kd, int ponM) noexcept {
    if (g_state.pid) {
        g_state.pid->SetTunings(kp, ki, kd, ponM);
    }
}

void SystemContext::setPidMode(int mode) noexcept {
    if (g_state.pid) {
        g_state.pid->SetMode(mode);
    }
}

int SystemContext::pidMode() const noexcept {
    return g_state.pid ? g_state.pid->GetMode() : MANUAL;
}

double SystemContext::pidKp() const noexcept {
    return g_state.pid ? g_state.pid->GetKp() : 0.0;
}

// ... etc for all other getters

PID* SystemContext::pidController() noexcept {
    return g_state.pid;
}
```

**Why keep direct pointer access:**
- Some code needs full PID object access
- Gradual migration - not replacing everything at once
- Direct pointer allows legacy code to continue
- Will remove in Phase 3 after all code migrated

---

## PHASE 2B: HIGH PRIORITY ACCESSORS (Week 1-2)

### 4. Sensor Scale Operations

**Members to add:**
```cpp
// Scale operation flags (read/write)
bool scaleCalibrationOn() const noexcept;
void setScaleCalibrationOn(bool on) noexcept;
void requestScaleCalibration() noexcept;  // Already exists

bool scaleTareOn() const noexcept;
void setScaleTareOn(bool on) noexcept;
void requestScaleTare() noexcept;  // Already exists
```

**Pattern:**
```cpp
// In SystemContext.h - Add to existing sensor accessor section
bool scaleCalibrationOn() const noexcept;
void setScaleCalibrationOn(bool on) noexcept;

bool scaleTareOn() const noexcept;
void setScaleTareOn(bool on) noexcept;

// In SystemContext.cpp
bool SystemContext::scaleCalibrationOn() const noexcept {
    return g_state.sensors.scaleCalibrationOn;
}

void SystemContext::setScaleCalibrationOn(bool on) noexcept {
    g_state.sensors.scaleCalibrationOn = on;
}

bool SystemContext::scaleTareOn() const noexcept {
    return g_state.sensors.scaleTareOn;
}

void SystemContext::setScaleTareOn(bool on) noexcept {
    g_state.sensors.scaleTareOn = on;
}
```

---

### 5. Sensor Data Access

**Members to add:**
```cpp
double currBrewWeight() const noexcept;
void setCurrBrewWeight(double weight) noexcept;

double currReadingWeight() const noexcept;
void setCurrReadingWeight(double weight) noexcept;

double currPumpOnTime() const noexcept;
void setCurrPumpOnTime(double time) noexcept;

float inputPressure() const noexcept;
void setInputPressure(float pressure) noexcept;

bool scaleFailure() const noexcept;
void setScaleFailure(bool failed) noexcept;
```

**Consider expanding DisplaySnapshot:**
Instead of individual accessors for display-only data, consolidate into:
```cpp
struct DisplaySnapshot {
    // Process data (already present)
    double currentTemperature = 0.0;
    double setpointTemperature = 0.0;
    double pidOutputPercent = 0.0;
    double currentBrewTime = 0.0;
    double targetBrewTime = 0.0;
    bool brewPidDisabled = false;
    
    // ADD: Sensor data
    double brewWeight = 0.0;
    double readingWeight = 0.0;
    double pumpOnTime = 0.0;
    float pressure = 0.0f;
    bool scaleFailure = false;
    
    // ADD: Machine state
    bool steamMode = false;
    bool backflushMode = false;
    
    // ADD: Timing
    unsigned int isrCounter = 0;
};

DisplaySnapshot getDisplaySnapshot() const noexcept;
```

---

### 6. Network Manager References

**Members to add:**
```cpp
CleverCoffeeWiFiManager* wifiManager() noexcept;
void setWifiManager(CleverCoffeeWiFiManager* manager) noexcept;

WebServerManager* webServerManager() noexcept;
void setWebServerManager(WebServerManager* manager) noexcept;

bool offlineMode() const noexcept;
void setOfflineMode(bool offline) noexcept;

bool hassioDiscoveryRunning() const noexcept;
void setHassioDiscoveryRunning(bool running) noexcept;

bool hassioFailed() const noexcept;
void setHassioFailed(bool failed) noexcept;

unsigned int wifiReconnects() const noexcept;
void setWifiReconnects(unsigned int count) noexcept;
```

**Note:** Some already exist as setters. Need to add getters.

---

### 7. Machine Mode Flags

**Members to add:**
```cpp
bool steamMode() const noexcept;
void setSteamMode(bool on) noexcept;

bool steamFirstOn() const noexcept;
void setSteamFirstOn(bool on) noexcept;

bool backflushMode() const noexcept;
void setBackflushMode(bool on) noexcept;

MachineStateId machineState() const noexcept;
```

---

## PHASE 2C: MEDIUM PRIORITY ACCESSORS (Week 2)

### 8. Display Coordination

```cpp
bool displayBufferReady() const noexcept;
void setDisplayBufferReady(bool ready) noexcept;  // Already exists as markDisplayBufferReady
```

### 9. Pressure Filter Variables

```cpp
float inX() const noexcept;
void setInX(float value) noexcept;

float inY() const noexcept;
void setInY(float value) noexcept;

float inOld() const noexcept;
void setInOld(float value) noexcept;

float inSum() const noexcept;
void setInSum(float value) noexcept;
```

---

## IMPLEMENTATION CHECKLIST

### Step 1: Extend SystemContext Header
- [ ] Add CRITICAL accessor declarations
- [ ] Add HIGH priority accessor declarations
- [ ] Update DisplaySnapshot struct
- [ ] Verify no conflicts with existing accessors

### Step 2: Implement SystemContext Methods
- [ ] Implement CRITICAL accessors in SystemContext.cpp
- [ ] Implement HIGH priority accessors
- [ ] Implement MEDIUM priority accessors (as needed)
- [ ] Verify all implementations delegate properly

### Step 3: Verify Compilation
- [ ] Run build with new accessors
- [ ] Check for any new warnings
- [ ] Verify no breaking changes to existing code

### Step 4: Begin Tier 1 Migration
- [ ] Update ProcessController.cpp to use PID accessors
- [ ] Update WebServerManager.cpp to use sensor accessors
- [ ] Update embeddedWebserver.h to use new accessors
- [ ] Update ModernDisplayTemplate.h to use DisplaySnapshot
- [ ] Update SystemInitializer.cpp

### Step 5: Testing
- [ ] Compile and verify no new warnings
- [ ] Run existing unit tests
- [ ] Test display rendering
- [ ] Test web endpoints
- [ ] Test MQTT operations
- [ ] Verify emergency stop works

---

## ESTIMATED EFFORT

- **CRITICAL accessors**: 2-3 hours (timer, emergencyStop, PID abstraction)
- **HIGH priority accessors**: 2-3 hours (scale, sensors, network, machine modes)
- **MEDIUM priority accessors**: 1-2 hours (display, filtering)
- **Tier 1 file migration**: 4-6 hours (ProcessController, WebServerManager, etc.)
- **Testing & verification**: 2-3 hours

**Total**: ~13-17 hours

---

## RISK MITIGATION

1. **Backward Compatibility**: Keep g_state.pid pointer accessible
2. **Gradual Migration**: Migrate one file at a time
3. **Testing**: Build and test after each major change
4. **Code Review**: Have accessors reviewed before Tier 1 migration
5. **Monitoring**: Track build size and RAM usage

---

## SUCCESS CRITERIA

✅ All CRITICAL accessors implemented and working  
✅ All HIGH priority accessors implemented  
✅ MEDIUM priority accessors available as needed  
✅ Tier 1 files migrated without issues  
✅ Project compiles cleanly  
✅ All existing tests pass  
✅ No new compiler warnings  
✅ Memory usage stable or improved  
✅ All functionality verified (display, web, MQTT)  

---

## DELIVERABLES FOR PHASE 2

1. Extended SystemContext.h with all accessor declarations
2. SystemContext.cpp implementation of all accessors
3. Migrated Tier 1 files using new accessors
4. Verification report showing successful migration
5. Updated DisplaySnapshot with additional data
6. Test results confirming all functionality works

---

## NEXT PHASE (Phase 3)

- Migrate Tier 2 files
- Migrate Tier 3 files
- Deprecate remaining direct g_state access
- Final cleanup

