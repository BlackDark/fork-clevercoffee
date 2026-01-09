# g_state Access Audit Report - Phase 1 Analysis
## CleverCoffee ESP32 Firmware Refactoring

Generated: 2026-01-08
Analysis Focus: All g_state member access patterns across the codebase

---

## EXECUTIVE SUMMARY

- **Total Files Accessing g_state**: 19 files
- **Total Unique g_state Members**: 38 distinct members
- **Total Access Instances**: 166 direct accesses
- **Priority Categories**: HIGH (critical), MEDIUM (important), LOW (nice-to-have)

---

## DETAILED FINDINGS BY CATEGORY

### CATEGORY 1: PID CONTROLLER ACCESS (CRITICAL)
**Total Accesses**: 44 (highest priority)
**Files**: ProcessController.cpp (22x), SystemInitializer.cpp (10x), CustomFormattersDemo.cpp (4x), ModernDisplayTemplate.h (6x), constexpr_demo.cpp (2x)

Current Pattern: `g_state.pid` → Direct PID object pointer access
Issue: PID is a complex object with many methods being called directly

Members Affected:
- g_state.pid (44x) - The PID controller object itself
  - Used for: SetTunings(), SetIntegratorLimits(), SetSampleTime(), SetOutputLimits(),
              SetSmoothingFactor(), SetMode(), Compute(), GetKp(), GetKi(), GetKd(),
              GetMode(), GetPonE(), GetInputError(), GetLastPPart(), GetLastIPart(),
              GetLastDPart(), GetDeltaInput()

Status in SystemContext: ⚠️ PARTIALLY ADDRESSED
- SystemContext has accessors for PID output/input pointers
- SystemContext has getters/setters for individual PID parameters
- BUT: Direct g_state.pid access for PID object methods is NOT replaced

Recommended Approach: 
1. Create abstraction layer in SystemContext for PID operations
2. Keep direct PID pointer for now (backward compatibility)
3. Gradually migrate PID method calls through SystemContext


### CATEGORY 2: SENSOR DATA - SCALE & WEIGHT (HIGH PRIORITY)
**Total Accesses**: 30+
**Files**: embeddedWebserver.h (10x), WebServerManager.cpp (10x), ModernDisplayTemplate.h (3x), displayCommon.h (2x), Config.h (2x), CustomFormattersDemo.cpp (2x)

Members Affected:
1. **g_state.sensors.scaleCalibrationOn** (13x)
   - Files: embeddedWebserver.h (5x), MQTTManager.cpp (2x), WebServerManager.cpp (6x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: HIGH - Web endpoints and MQTT need control

2. **g_state.sensors.scaleTareOn** (10x)
   - Files: embeddedWebserver.h (5x), MQTTManager.cpp (2x), WebServerManager.cpp (3x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: HIGH - Web endpoints and MQTT need control

3. **g_state.sensors.currBrewWeight** (10x)
   - Files: CustomFormattersDemo.cpp (2x), Config.h (2x), ModernDisplayTemplate.h (2x), displayCommon.h (2x), embeddedWebserver.h (1x), WebServerManager.cpp (1x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: HIGH - Display and web endpoints need read access

4. **g_state.sensors.currReadingWeight** (3x)
   - Files: Config.h (1x), ModernDisplayTemplate.h (1x), embeddedWebserver.h (1x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: MEDIUM

5. **g_state.sensors.currPumpOnTime** (3x)
   - Files: ModernDisplayTemplate.h (2x), displayCommon.h (1x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: MEDIUM - Display-only

6. **g_state.sensors.inputPressure** (2x)
   - Files: CustomFormattersDemo.cpp (1x), ModernDisplayTemplate.h (1x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: MEDIUM

7. **g_state.sensors.scaleFailure** (3x)
   - Files: ModernDisplayTemplate.h (3x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: MEDIUM - Display-only

8. **g_state.sensors.preBrewWeight** (1x)
   - Files: Config.h (1x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: LOW

9. **g_state.sensors.inputPressureFilter** (1x)
   - Files: Config.h (1x)
   - Status: PARTIALLY ADDRESSED in SystemContext
   - Methods: updatePressureFilter(), getPressureFilterOutput()
   - Priority: LOW

Pressure Filter Variables (used for IIR filtering):
- g_state.sensors.inX (2x), g_state.sensors.inY (2x), g_state.sensors.inOld (2x), g_state.sensors.inSum (3x)
- Files: helperUtils.h (9 total)
- Status: MISSING ACCESSORS in SystemContext
- Priority: MEDIUM - Helper function needs these for filtering


### CATEGORY 3: NETWORK & COMMUNICATION (HIGH PRIORITY)
**Total Accesses**: 16+
**Files**: LoopManager.cpp (5x), SystemInitializer.cpp (3x), MQTTManager.cpp (4x)

Members Affected:
1. **g_state.network.cleverCoffeeWiFiManager** (4x)
   - Files: CustomFormattersDemo.cpp (2x), LoopManager.cpp (1x), SystemInitializer.cpp (1x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: HIGH - Manager initialization and usage

2. **g_state.network.offlineMode** (3x)
   - Files: SystemUtils.h (1x), LoopManager.cpp (2x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: HIGH - Core fallback logic depends on this

3. **g_state.network.webServerManager** (2x)
   - Files: LoopManager.cpp (1x), SystemInitializer.cpp (1x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: HIGH - Manager initialization

4. **g_state.network.hassioFailed** (3x)
   - Files: MQTTManager.cpp (3x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: MEDIUM

5. **g_state.network.wifiReconnects** (1x)
   - Files: LoopManager.cpp (1x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: LOW


### CATEGORY 4: MACHINE STATE & CONTROL (HIGH PRIORITY)
**Total Accesses**: 20+
**Files**: embeddedWebserver.h (10x), SystemUtils.h (7x), CustomFormattersDemo.cpp (3x)

Members Affected:
1. **g_state.machine.timer** (9x)
   - Files: isr.h (9x)
   - Status: PARTIALLY ADDRESSED - MachineStateContext owns timer
   - Priority: CRITICAL - ISR initialization and management
   - Note: Timer ownership was moved to MachineStateContext

2. **g_state.machine.steamON** (5x)
   - Files: embeddedWebserver.h (3x), SystemUtils.h (2x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: HIGH - Steam mode toggle and emergency stop logic

3. **g_state.machine.backflushOn** (5x)
   - Files: embeddedWebserver.h (5x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: HIGH - Backflush mode toggle for web interface

4. **g_state.machine.machineState** (3x)
   - Files: CustomFormattersDemo.cpp (3x)
   - Status: PARTIALLY ADDRESSED in MachineStateContext
   - Priority: MEDIUM - Display/logging only

5. **g_state.machine.emergencyStop** (4x)
   - Files: SystemUtils.h (4x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: CRITICAL - Safety-critical feature


### CATEGORY 5: DISPLAY & UI COORDINATION (MEDIUM PRIORITY)
**Total Accesses**: 5+
**Files**: ModernDisplayTemplate.h (4x), displayCommon.h (4x)

Members Affected:
1. **g_state.coordination.displayBufferReady** (4x)
   - Files: ModernDisplayTemplate.h (1x), displayCommon.h (3x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: MEDIUM - Display update synchronization
   - Note: SystemContext.markDisplayBufferReady() exists but no read accessor

2. **g_state.timing.isrCounter** (5x)
   - Files: ModernDisplayTemplate.h (2x), displayCommon.h (1x), isr.h (2x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: MEDIUM - Animation timing sync


### CATEGORY 6: COORDINATION FLAGS (LOW PRIORITY)
**Total Accesses**: 3
**Files**: MQTTManager.cpp (1x), main.cpp (1x)

Members Affected:
1. **g_state.coordination.processController** (1x)
   - Files: main.cpp (1x)
   - Status: ADDRESSED via SystemContext.setProcessController()
   - Priority: LOW

2. **g_state.coordination.hassioUpdateRunning** (1x)
   - Files: MQTTManager.cpp (1x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: LOW

3. **g_state.machine.steamFirstON** (2x)
   - Files: SystemUtils.h (2x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: LOW


### CATEGORY 7: SYSTEM INFO (LOW PRIORITY)
**Total Accesses**: 1
**Files**: SystemInitializer.cpp (1x)

Members Affected:
1. **g_state.sysVersion** (1x)
   - Files: SystemInitializer.cpp (1x)
   - Status: MISSING ACCESSOR in SystemContext
   - Priority: LOW - Version display only


---

## MISSING ACCESSOR SUMMARY

### CRITICAL (Must implement for Phase 2)
1. ✋ g_state.machine.timer - Timer initialization/management
2. ✋ g_state.machine.emergencyStop - Safety feature
3. ✋ g_state.pid - PID object abstraction layer

### HIGH PRIORITY (Should implement for Phase 2)
1. g_state.sensors.scaleCalibrationOn - Web/MQTT endpoints
2. g_state.sensors.scaleTareOn - Web/MQTT endpoints
3. g_state.sensors.currBrewWeight - Display/Web/MQTT
4. g_state.network.cleverCoffeeWiFiManager - Manager initialization
5. g_state.network.offlineMode - Fallback logic
6. g_state.network.webServerManager - Manager initialization
7. g_state.machine.steamON - Mode toggle/Emergency stop
8. g_state.machine.backflushOn - Mode toggle

### MEDIUM PRIORITY (Nice to have)
1. g_state.sensors.currReadingWeight - Display/Web
2. g_state.sensors.currPumpOnTime - Display
3. g_state.sensors.scaleFailure - Display error indication
4. g_state.sensors.inX, inY, inOld, inSum - Pressure filtering
5. g_state.network.hassioFailed - Status tracking
6. g_state.coordination.displayBufferReady - Display sync
7. g_state.timing.isrCounter - Animation timing
8. g_state.machine.machineState - Display/Logging

### LOW PRIORITY (Can defer)
1. g_state.sensors.preBrewWeight - Config
2. g_state.sensors.inputPressureFilter - Already has accessors
3. g_state.network.wifiReconnects - Status tracking
4. g_state.machine.steamFirstON - Initialization flag
5. g_state.coordination.hassioUpdateRunning - Update coordination
6. g_state.sysVersion - Version display


---

## FILES NEEDING MIGRATION - ORDERED BY IMPACT

### TIER 1 - HIGHEST PRIORITY (Most g_state dependencies)
1. **src/control/ProcessController.cpp** (22 accesses)
   - Depends on: g_state.pid
   - Status: Partially using SystemContext accessors but still direct g_state.pid calls
   - Action: Create PID abstraction layer in SystemContext

2. **src/network/WebServerManager.cpp** (10 accesses)
   - Depends on: scaleTareOn, scaleCalibrationOn, currBrewWeight
   - Status: Web endpoints directly manipulating g_state
   - Action: Create web endpoint handlers in SystemContext

3. **include/clevercoffee/embeddedWebserver.h** (20 accesses)
   - Depends on: scaleTareOn, scaleCalibrationOn, backflushOn, steamON, currBrewWeight, currReadingWeight
   - Status: HTTP handler code directly accessing g_state
   - Action: Migrate HTTP handlers to use SystemContext accessors

4. **include/clevercoffee/display/ModernDisplayTemplate.h** (17 accesses)
   - Depends on: pid, scaleFailure, sensors (multiple), timing.isrCounter, coordination.displayBufferReady
   - Status: Display rendering code has heavy g_state coupling
   - Action: Create display data accessor methods in SystemContext

5. **src/core/SystemInitializer.cpp** (13 accesses)
   - Depends on: pid (setup), sysVersion, network managers
   - Status: Initialization code setting up g_state references
   - Action: Refactor initialization to use SystemContext API

### TIER 2 - SECOND PRIORITY
1. **include/clevercoffee/utils/SystemUtils.h** (9 accesses)
   - Depends on: emergencyStop, steamON, steamFirstON, offlineMode
   - Status: Utility functions using g_state directly
   - Action: Create utility accessor methods in SystemContext

2. **include/clevercoffee/utils/helperUtils.h** (9 accesses)
   - Depends on: Pressure filter variables (inX, inY, inOld, inSum)
   - Status: Inline filter implementation
   - Action: Refactor pressure filter into SystemContext or separate class

3. **src/core/LoopManager.cpp** (5 accesses)
   - Depends on: offlineMode, wifiReconnects, cleverCoffeeWiFiManager, webServerManager
   - Status: Main loop checking network state
   - Action: Create network state query methods in SystemContext

### TIER 3 - THIRD PRIORITY
1. **include/clevercoffee/display/displayCommon.h** (8 accesses)
   - Depends on: displayBufferReady, sensors (multiple), isrCounter
   - Status: Display helper functions
   - Action: Migrate display helpers to use SystemContext

2. **src/network/MQTTManager.cpp** (8 accesses)
   - Depends on: scaleTareOn, scaleCalibrationOn, hassioFailed, hassioUpdateRunning
   - Status: MQTT sensor registration and callbacks
   - Action: Create MQTT accessor methods in SystemContext

3. **examples/CustomFormattersDemo.cpp** (22 accesses)
   - Status: Example/demo code - can be deprecated
   - Action: Not critical for migration (example code)

### TIER 4 - LOWER PRIORITY
1. **include/clevercoffee/isr.h** (11 accesses)
   - Depends on: machine.timer, timing.isrCounter
   - Status: ISR setup code (mostly one-time)
   - Action: Refactor timer initialization to use MachineStateContext

2. **include/clevercoffee/Config.h** (5 accesses)
   - Status: Config callbacks reading sensor state
   - Action: Refactor to pass SystemContext to callbacks

3. **include/clevercoffee/state/MachineStateContext.h** (1 access)
4. **include/clevercoffee/context/HardwareContext.h** (1 access)
5. **include/clevercoffee/coordinators/NetworkCoordinator.h** (1 access)
6. **include/clevercoffee/coordinators/UICoordinator.h** (1 access)
   - Status: Reference in comments/docs only
   - Action: No code changes needed

---

## IMPLEMENTATION ROADMAP FOR PHASE 2

### Step 1: Create Missing Accessors in SystemContext (High Priority)
Priority: CRITICAL → HIGH → MEDIUM

Create accessor pairs (getter/setter) for:
- Sensor scale operations: scaleTareOn(), setScaleTareOn(), scaleCalibrationOn(), setScaleCalibrationOn()
- Sensor data: currBrewWeight(), setCurrBrewWeight(), currReadingWeight(), setCurrReadingWeight()
- Sensor flags: scaleFailure(), setScaleFailure()
- Machine control: steamON(), setSteamON(), backflushOn(), setBackflushOn(), emergencyStop(), setEmergencyStop()
- Network: cleverCoffeeWiFiManager(), setCleverCoffeeWiFiManager(), webServerManager(), setWebServerManager()
- Network flags: offlineMode(), setOfflineMode(), hassioFailed(), setHassioFailed(), wifiReconnects(), setWifiReconnects()
- Display: displayBufferReady(), setDisplayBufferReady()
- Timing: isrCounter(), setIsrCounter()
- Pressure filter: inX(), setInX(), inY(), setInY(), inOld(), setInOld(), inSum(), setInSum()

### Step 2: Create PID Abstraction Layer
- Create methods in SystemContext for common PID operations
- Wrapper methods: computePID(), setPidTunings(), setPidMode(), etc.
- Keep g_state.pid pointer for backward compatibility during migration

### Step 3: Migrate Files (in order of Tier 1-4)
1. ProcessController.cpp - Use PID abstraction layer
2. WebServerManager.cpp - Use sensor/machine accessors
3. embeddedWebserver.h - Use sensor/machine accessors
4. ModernDisplayTemplate.h - Use display snapshot + accessors
5. SystemInitializer.cpp - Use initialization accessors
6. Continue with Tier 2-4 files...

### Step 4: Verify & Test
- Compile test
- Run existing unit tests
- Verify display rendering works
- Verify web endpoints work
- Verify MQTT works


---

## NOTES FOR IMPLEMENTATION

1. **DisplaySnapshot Pattern**: SystemContext already has getDisplaySnapshot() method which provides atomic read of display data. Consider expanding this pattern.

2. **Coordinator Abstraction**: SensorCoordinator and NetworkCoordinator already exist but may not have all needed accessors yet.

3. **Thread Safety**: When creating new accessors for mutable state (like scale calibration flags), consider thread-safe access patterns.

4. **Backward Compatibility**: Some files may need gradual migration. Direct g_state access can be deprecated gradually.

5. **ISR Handling**: Timer initialization in isr.h is special - coordinate with MachineStateContext ownership.

6. **Example Files**: CustomFormattersDemo.cpp and constexpr_demo.cpp are examples - can be updated later or marked deprecated.

---

## COMPLETION CRITERIA FOR PHASE 1

✅ All files accessing g_state identified (19 files)
✅ All unique g_state members documented (38 members)
✅ Access patterns categorized by type
✅ SystemContext coverage identified (existing vs missing)
✅ Priority matrix created for Phase 2
✅ Implementation roadmap provided

**Phase 1 Status: COMPLETE**
Ready for Phase 2 implementation.
