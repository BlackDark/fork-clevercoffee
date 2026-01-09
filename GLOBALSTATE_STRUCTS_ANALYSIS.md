# GlobalState.h Nested Structs Architecture Analysis

## Summary of Findings

The GlobalState.h file contains **11 nested struct types** within the `GlobalStateNamespace` namespace. These structs were originally designed to hold all global state that has since been **completely decomposed into individual private members** in the `SystemContext` class.

## Current State

**Status**: ALL NESTED STRUCTS ARE NOW UNUSED - THEY ARE DEAD CODE

The structs are defined in GlobalState.h but:
- **NOT instantiated anywhere** in the codebase (except as type definitions)
- **NOT used in any method signatures** outside GlobalState.h
- **NOT referenced in tests**
- **Completely replaced** by SystemContext's private member variables with prefixes

Only **3 items from GlobalState.h are actually used**:
1. `cmp_str` - String comparator for std::map (used in MQTT maps)
2. `MachineStateFlags` - State machine request flags
3. Constants like `TIME_TO_DISPLAY_OFF_MILLIS`, `wifiConnectionDelay`, etc.

---

## List of All GlobalStateNamespace Structs

### 1. ProcessState
**Location**: GlobalState.h lines 98-120
**Contains**: Process control state (temperature, setpoint, PID values, brew timing)
**Current Usage**: NONE - decomposed into SystemContext members with `process_` prefix
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - it's completely replaced by SystemContext

### 2. CoordinationState
**Location**: GlobalState.h lines 125-134
**Contains**: System coordination flags (temperatureUpdateRunning, websiteUpdateRunning, etc.)
**Current Usage**: NONE - decomposed into SystemContext members with `coordination_` prefix
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - coordination is handled by individual Coordinators now

### 3. HandlerRefs
**Location**: GlobalState.h lines 139-144
**Contains**: References to brew/water/power/steam handlers
**Current Usage**: NONE - handlers are registered directly in SystemContext
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - SystemContext manages handlers directly

### 4. HardwareRefs
**Location**: GlobalState.h lines 149-173
**Contains**: References to all hardware components (display, relays, sensors, switches, LEDs)
**Current Usage**: NONE - hardware is managed through HardwareContext
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - HardwareContext owns all hardware references

### 5. NetworkState
**Location**: GlobalState.h lines 178-196
**Contains**: WiFi and MQTT state (manager pointers, connection status, MQTT variable maps)
**Current Usage**: NONE - decomposed into SystemContext members with `network_` prefix
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - replaced by SystemContext network members

### 6. TimingState
**Location**: GlobalState.h lines 201-216
**Contains**: Timing variables and millisecond timers
**Current Usage**: NONE - timing is handled by individual MillisecondTimer instances
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - timers are managed individually now

### 7. StandbyState
**Location**: GlobalState.h lines 221-232
**Contains**: Standby mode timing (standbyModeRemainingTimeMillis, etc.)
**Current Usage**: NONE - decomposed into SystemContext members with `standby_` prefix
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - replaced by SystemContext standby members

### 8. SensorState
**Location**: GlobalState.h lines 237-297
**Contains**: Sensor/scale/pressure data and switch states
**Current Usage**: NONE - decomposed into SystemContext members with `sensors_` prefix
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - largest struct, completely decomposed

### 9. MachineStateFlags
**Location**: GlobalState.h lines 299-314
**Contains**: Request flags for state machine transitions (requestBrewStart, requestBrewStop, etc.)
**Current Usage**: 
  - Used in SystemContext.h as `GlobalStateNamespace::MachineStateFlags machine_flags_` member
  - Used as a return type annotation in comments
**Instantiation Count**: 1 (in SystemContext)
**Recommended Action**: **KEEP but MOVE** - Move to `clevercoffee/state/StateFlags.h` or similar
  - This is the only struct actively used
  - Should have its own dedicated header

### 10. MachineStateData
**Location**: GlobalState.h lines 319-334
**Contains**: Machine state info (current state, steam status, backflush info, flags)
**Current Usage**: NONE - decomposed into SystemContext members with `machine_` prefix
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - completely replaced by SystemContext

### 11. DisplayState
**Location**: GlobalState.h lines 339-341
**Contains**: Display offline flag
**Current Usage**: NONE - single member decomposed into SystemContext
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - single flag can be managed directly

### 12. DebugState
**Location**: GlobalState.h lines 346-349
**Contains**: Debug state strings (hotWaterStateDebug, etc.)
**Current Usage**: NONE - decomposed into SystemContext members with `debug_` prefix
**Instantiation Count**: 0
**Recommended Action**: **DELETE** - replaced by SystemContext

---

## Items That SHOULD Stay in GlobalState.h

### 1. cmp_str
**Status**: KEEP - Used in MQTT variable maps
**Current Usage**: 
  - SystemContext.h uses it for `std::map<const char*, const char*, cmp_str>`
  - SystemContext.h uses it for `std::map<const char*, std::function<double()>, cmp_str>`
  - MQTTManager.h defines its own copy (duplicate!)

**Recommended Action**: KEEP in GlobalState.h (or better yet, move to `clevercoffee/utils/StringComparator.h`)

### 2. MachineStateFlags
**Status**: KEEP but MOVE
**Current Usage**: SystemContext machine_flags_ member

**Recommended Action**: Move to dedicated header `clevercoffee/state/StateFlags.h`

### 3. Constants
**Status**: KEEP
**Constants Used**:
  - `TIME_TO_DISPLAY_OFF` / `TIME_TO_DISPLAY_OFF_MILLIS` - used for standby timeout
  - `wifiConnectionDelay` - used in CleverCoffeeWiFiManager
  - `maxWifiReconnects` - used in CleverCoffeeWiFiManager
  - `EmergencyStopTemp` - used in SystemUtils
  - `waterTankCountsNeeded` - for water tank detection
  - `SCALE_CONNECTION_*` - scale connection management

**Recommended Action**: KEEP in GlobalState.h or move to appropriate domain headers

---

## Architecture Recommendations

### Phase 1: Immediate Cleanup
1. **DELETE all 10 unused nested structs** (everything except MachineStateFlags):
   - ProcessState
   - CoordinationState
   - HandlerRefs
   - HardwareRefs
   - NetworkState
   - TimingState
   - StandbyState
   - SensorState
   - MachineStateData
   - DisplayState
   - DebugState

2. **CREATE new header files**:
   ```
   include/clevercoffee/state/StateFlags.h
   include/clevercoffee/utils/StringComparator.h
   ```

### Phase 2: Refactor GlobalState.h
The file should contain ONLY:
- Forward declarations
- `cmp_str` struct (or moved to StringComparator.h)
- `MachineStateFlags` struct (or moved to StateFlags.h)
- Constants (wifiConnectionDelay, maxWifiReconnects, etc.)
- `initializeHandlers()` function
- `g_systemContext` extern declaration

### Phase 3: Update Includes
- Files that need MachineStateFlags should include StateFlags.h
- Files that need cmp_str should include StringComparator.h
- Remove most other GlobalState.h includes (they're unnecessary)

---

## Files Currently Including GlobalState.h

| File | Why | Should Keep | Recommendation |
|------|-----|-----------|-----------------|
| SystemContext.h | MachineStateFlags, cmp_str | YES | Keep, will import from dedicated headers |
| ProcessController.cpp | Unknown/legacy | Investigate | Remove if not needed |
| main.cpp | Likely for initializeHandlers | YES | Keep for handler init |
| Config.cpp | Unknown | Investigate | Remove if not needed |
| State machine files (7 files) | Unknown/legacy | Investigate | Remove if not needed |
| Network files | Constants (wifiConnectionDelay, etc.) | YES | Keep or move to network headers |
| Display files | Constants | Maybe | Move constants to display headers |
| Utils files | Various | YES | Keep as needed |
| embeddedWebserver.h | Unknown | Investigate | Remove if not needed |
| TempSensor.h | Unknown | Investigate | Remove if not needed |
| isr.h | Unknown | Investigate | Remove if not needed |

---

## Rightful Owner Mapping

| Struct | Original Use Case | Rightful Owner After Refactor | Status |
|--------|------------------|--------------------------------|--------|
| ProcessState | Process/PID control | SystemContext (already done) | DELETE |
| CoordinationState | Cross-system coordination | Coordinator classes (not monolithic) | DELETE |
| HandlerRefs | Handler management | SystemContext or HandlerRegistry | DELETE |
| HardwareRefs | Hardware management | HardwareContext | DELETE |
| NetworkState | Network/WiFi/MQTT state | SystemContext (already done) | DELETE |
| TimingState | Timing management | Individual timer instances | DELETE |
| StandbyState | Standby mode control | StandbyCoordinator or SystemContext | DELETE |
| SensorState | Sensor/scale data | SensorCoordinator or SystemContext | DELETE |
| MachineStateFlags | State machine requests | StateFlags.h (dedicated header) | MOVE |
| MachineStateData | Machine state tracking | SystemContext (already done) | DELETE |
| DisplayState | Display management | SystemContext or DisplayManager | DELETE |
| DebugState | Debugging info | SystemContext or Logger | DELETE |

---

## Code Quality Impact

### Benefits of Deletion:
1. **Removes Dead Code**: Eliminates confusing struct definitions that are never used
2. **Clarifies Architecture**: Makes it clear how SystemContext actually manages state
3. **Reduces Coupling**: Global structs create implicit dependencies
4. **Improves Maintainability**: No confusion about where state should be modified
5. **Type Safety**: Explicit getter/setter methods in SystemContext vs implicit struct access

### Risk Level: LOW
- No code is currently using these structs
- SystemContext members are the source of truth
- No behavioral changes required

---

## Compilation Impact

**Current Build Status**: 
- Project compiles successfully
- GlobalState.h is included in 24 files but mostly for constants and MachineStateFlags

**After Cleanup**:
- Will compile cleanly
- No changes to functionality
- Some includes will become unnecessary (can be removed)
- New headers for StateFlags and StringComparator will be needed

