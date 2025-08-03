# Global State Optimization Plan

## Current Situation Analysis

### Problem Scale
- **258 extern declarations** across the codebase
- Global variables accessed from nearly every file
- No clear ownership or lifecycle management
- Hidden dependencies making testing and maintenance difficult
- Potential race conditions and data corruption

### Most Frequently Used Globals
```cpp
// System State (9+ usages each)
extern Config& config;           // 8 usages
extern double temperature;       // 7 usages
extern bool offlineMode;         // 5 usages
extern double pidOutput;         // 4 usages
extern bool steamON;             // 4 usages

// Hardware References
extern U8G2* u8g2;              // 3 usages
extern Relay* heaterRelay;      // 3 usages
extern TempSensor* tempSensor;  // Multiple files

// Coordination Flags
extern bool temperatureUpdateRunning;
extern bool hassioUpdateRunning;
extern bool websiteUpdateRunning;
extern bool displayUpdateRunning;
```

## Global Variable Categories

### 1. Process Control State (High Priority)
**Variables:**
- `temperature`, `setpoint`, `pidOutput`
- `steamON`, `pidON`, `brewPidDisabled`
- `currBrewTime`, `brewSetpoint`, `steamSetpoint`

**Current Problems:**
- Modified from multiple classes
- No validation or bounds checking
- Unclear ownership and lifecycle

### 2. System Coordination (High Priority)
**Variables:**
- `temperatureUpdateRunning`, `websiteUpdateRunning`
- `hassioUpdateRunning`, `displayUpdateRunning`
- `standbyModeRemainingTimeMillis`

**Current Problems:**
- Used for inter-component coordination
- Race condition potential
- Scattered across different managers

### 3. Hardware References (Medium Priority)
**Variables:**
- `u8g2`, `heaterRelay`, `pumpRelay`, `valveRelay`
- `tempSensor`, `brewSwitch`, `steamSwitch`

**Current Problems:**
- Direct hardware access from business logic
- No abstraction layer
- Difficult to mock for testing

### 4. Configuration & Constants (Low Priority)
**Variables:**
- `config` (read-only access mostly)
- `hostname`, `offlineMode`
- System settings and flags

**Current Problems:**
- Less problematic as mostly read-only
- Could benefit from better encapsulation

## Solution Approaches (Worst to Best)

### ❌ Option 1: Global State Object
```cpp
class GlobalState {
public:
    double temperature;
    bool steamON;
    double pidOutput;
    // ... all globals here
};
extern GlobalState* g_state; // Still global access!
```

**Problems:**
- Just moves the problem to a different location
- Still global access patterns
- No real encapsulation benefits
- Same testing and dependency issues

**Verdict:** Not recommended - cosmetic change only

### ⚠️ Option 2: Singleton Pattern
```cpp
class SystemState {
public:
    static SystemState& instance() {
        static SystemState s;
        return s;
    }

    double getTemperature() const { return temperature_; }
    void setTemperature(double temp) { temperature_ = temp; }

private:
    double temperature_ = 0.0;
};

// Usage: SystemState::instance().getTemperature()
```

**Benefits:**
- Controlled access through methods
- Can add validation and logging
- Better than raw globals

**Problems:**
- Hidden global dependency
- Difficult to test (can't inject mock)
- Tight coupling to singleton

**Verdict:** Marginal improvement, but creates new problems

### ✅ Option 3: Data Structures + Dependency Injection
```cpp
// Grouped data structures
struct ProcessState {
    double temperature = 0.0;
    double setpoint = brewSetpoint;
    double pidOutput = 0.0;
    bool steamMode = false;
    bool pidEnabled = true;
};

struct SystemCoordination {
    bool temperatureUpdateRunning = false;
    bool websiteUpdateRunning = false;
    bool hassioUpdateRunning = false;
    bool displayUpdateRunning = false;
};

// Managers receive data through constructor
class ProcessController {
public:
    ProcessController(ProcessState& state, SystemCoordination& coord)
        : state_(state), coordination_(coord) {}

    void updateTemperature() {
        coordination_.temperatureUpdateRunning = true;
        state_.temperature = readSensor();
        coordination_.temperatureUpdateRunning = false;
    }

private:
    ProcessState& state_;
    SystemCoordination& coordination_;
};
```

**Benefits:**
- Clear data grouping and relationships
- Dependency injection enables testing
- Gradual migration path
- 258 externs → ~10 data structures

**Problems:**
- Still shared mutable state
- Requires careful coordination
- Not fully object-oriented

**Verdict:** Good intermediate step

### 🏆 Option 4: Manager Ownership (Full OOP)
```cpp
class ProcessController {
public:
    // Clean interface - no external state access
    double getTemperature() const { return temperature_; }
    double getPidOutput() const { return pidOutput_; }
    bool isUpdateRunning() const { return updateRunning_; }

    void update() {
        updateRunning_ = true;
        temperature_ = readSensor();
        pidOutput_ = calculatePID();
        updateRunning_ = false;
    }

private:
    // Owned data - no external access
    double temperature_ = 0.0;
    double pidOutput_ = 0.0;
    bool updateRunning_ = false;
};

class DisplayCoordinator {
public:
    DisplayCoordinator(ProcessController& process, NetworkManager& network)
        : process_(process), network_(network) {}

    void update() {
        // Clean interface access
        if (!process_.isUpdateRunning() && !network_.isUpdateRunning()) {
            refreshDisplay();
        }
    }

private:
    ProcessController& process_;
    NetworkManager& network_;
};
```

**Benefits:**
- Zero global variable access
- Clear ownership and lifecycle
- Fully testable (can inject mocks)
- Thread-safe by design
- Maintainable and extensible

**Problems:**
- Requires significant refactoring
- May need careful sequencing of changes
- Higher initial development cost

**Verdict:** Best long-term solution

## Recommended Migration Plan

### Phase 1: Data Consolidation (Immediate - Low Risk)

**Goal:** Reduce 258 extern declarations to ~10 data structure references

**Step 1.1: Create Data Structures**
```cpp
// src/state/SystemData.h
struct ProcessState {
    double temperature = 0.0;
    double setpoint = brewSetpoint;
    double pidOutput = 0.0;
    bool steamMode = false;
    bool pidEnabled = true;
    double currBrewTime = 0.0;
};

struct CoordinationState {
    bool temperatureUpdateRunning = false;
    bool websiteUpdateRunning = false;
    bool hassioUpdateRunning = false;
    bool displayUpdateRunning = false;
};

struct HardwareRefs {
    U8G2* display = nullptr;
    Relay* heaterRelay = nullptr;
    Relay* pumpRelay = nullptr;
    Relay* valveRelay = nullptr;
    TempSensor* tempSensor = nullptr;
};

struct SystemSettings {
    Config& config;
    bool offlineMode = false;
    String hostname = HOSTNAME;
};
```

**Step 1.2: Create Global Data Instances**
```cpp
// src/state/SystemData.cpp
ProcessState g_processState;
CoordinationState g_coordinationState;
HardwareRefs g_hardwareRefs;
SystemSettings g_systemSettings{Config::getInstance()};
```

**Step 1.3: Replace Individual Globals**
Replace scattered externs with grouped access:
```cpp
// OLD: extern double temperature;
// NEW: extern ProcessState g_processState;
//      Usage: g_processState.temperature
```

**Benefits:**
- Immediate ~95% reduction in extern declarations
- Clear data relationships
- No logic changes (low risk)
- Enables Phase 2

**Effort:** 2-3 days, moderate complexity

### Phase 2: Dependency Injection (Medium-term)

**Goal:** Pass data structures to managers instead of global access

**Step 2.1: Update Manager Constructors**
```cpp
class ProcessController {
public:
    ProcessController(ProcessState& state,
                     CoordinationState& coordination,
                     HardwareRefs& hardware)
        : state_(state), coordination_(coordination), hardware_(hardware) {}

private:
    ProcessState& state_;
    CoordinationState& coordination_;
    HardwareRefs& hardware_;
};
```

**Step 2.2: Update SystemInitializer**
```cpp
void SystemInitializer::initialize() {
    // Initialize hardware references
    g_hardwareRefs.display = displayManager_->get();
    g_hardwareRefs.heaterRelay = &hardwareManager_->getHeaterRelay();

    // Create managers with data injection
    processController_ = std::make_unique<ProcessController>(
        g_processState, g_coordinationState, g_hardwareRefs);
}
```

**Benefits:**
- Clear dependencies visible in constructors
- Testable (can inject mock data structures)
- Still shared state but controlled access
- Foundation for Phase 3

**Effort:** 1-2 weeks, moderate complexity

### Phase 3: Manager Ownership (Long-term)

**Goal:** Each manager owns its data, clean interfaces between components

**Step 3.1: Move Data Into Managers**
```cpp
class ProcessController {
public:
    // Public interface - no direct data access
    double getTemperature() const { return temperature_; }
    void setTargetTemperature(double target) { setpoint_ = target; }
    bool isUpdateRunning() const { return updateRunning_; }

private:
    // Owned data
    double temperature_ = 0.0;
    double setpoint_ = brewSetpoint;
    double pidOutput_ = 0.0;
    bool updateRunning_ = false;
};
```

**Step 3.2: Update Inter-Manager Communication**
```cpp
class LoopCoordinator {
public:
    LoopCoordinator(ProcessController& process, UIManager& ui, NetworkManager& network)
        : process_(process), ui_(ui), network_(network) {}

    void update() {
        process_.update();

        // Clean interface-based coordination
        if (!process_.isUpdateRunning() && !network_.isUpdateRunning()) {
            ui_.refresh();
        }
    }

private:
    ProcessController& process_;
    UIManager& ui_;
    NetworkManager& network_;
};
```

**Benefits:**
- Zero global variables
- Full encapsulation
- Completely testable
- Professional C++ architecture
- Maintainable and extensible

**Effort:** 3-4 weeks, high complexity

## Detailed Implementation Steps

### Phase 1 Implementation Details

#### Week 1: Data Structure Creation
1. **Day 1-2:** Create `src/state/SystemData.h` with all data structures
2. **Day 3:** Create `src/state/SystemData.cpp` with global instances
3. **Day 4-5:** Update `GlobalVariables.h` to include new structures

#### Week 2: Global Replacement
1. **Day 1-2:** Replace process control globals (temperature, pidOutput, etc.)
2. **Day 3:** Replace coordination flags (updateRunning variables)
3. **Day 4:** Replace hardware references
4. **Day 5:** Testing and compilation fixes

#### Benefits After Phase 1:
- **Complexity Reduction:** 258 externs → ~10 structure references
- **Maintainability:** Clear data grouping
- **Documentation:** Self-documenting data relationships
- **Foundation:** Enables dependency injection

### Phase 2 Implementation Details

#### Week 1-2: Manager Updates
1. Update each manager to accept data structures in constructor
2. Replace direct global access with member references
3. Update SystemInitializer to inject dependencies

#### Week 3: Testing & Integration
1. Comprehensive testing of all managers
2. Integration testing of manager interactions
3. Performance verification

### Phase 3 Implementation Details

#### Week 1-2: Data Migration
1. Move data from global structures into appropriate managers
2. Create clean public interfaces for each manager
3. Update inter-manager dependencies

#### Week 3-4: Interface Design
1. Design clean communication patterns between managers
2. Implement event-based or callback systems where needed
3. Remove all remaining global data access

## Risk Assessment

### Phase 1 Risks: LOW
- **Compilation Issues:** Medium - many files to update
- **Logic Errors:** Low - same data, different access pattern
- **Testing Impact:** Low - behavior unchanged

**Mitigation:** Incremental replacement, comprehensive compilation testing

### Phase 2 Risks: MEDIUM
- **Constructor Complexity:** Medium - many parameters
- **Integration Issues:** Medium - manager interdependencies
- **Testing Complexity:** Medium - need to inject test data

**Mitigation:** Start with least dependent managers, thorough integration testing

### Phase 3 Risks: HIGH
- **Architecture Changes:** High - fundamental design changes
- **Communication Patterns:** High - need new inter-manager protocols
- **Performance Impact:** Medium - may need optimization

**Mitigation:** Prototype critical paths, performance benchmarking, gradual rollout

## Measurable Success Criteria

### Phase 1 Success Metrics:
- [ ] Extern declarations reduced from 258 to < 20
- [ ] All globals grouped into logical data structures
- [ ] Compilation successful with no behavior changes
- [ ] Code review shows clear data relationships

### Phase 2 Success Metrics:
- [ ] All managers use dependency injection
- [ ] No direct global access from manager implementations
- [ ] Unit tests can inject mock data structures
- [ ] Manager dependencies clearly visible in constructors

### Phase 3 Success Metrics:
- [ ] Zero global variable access
- [ ] Each manager owns its data completely
- [ ] Full unit test coverage of all managers
- [ ] Clean interface contracts between components
- [ ] Performance equivalent to original implementation

## Recommendation

**Start with Phase 1** for the following reasons:

1. **Immediate Benefits:** Massive reduction in complexity (258 → ~10 references)
2. **Low Risk:** Same logic, just organized differently
3. **Clear Progress:** Measurable improvement that enables future phases
4. **Foundation Building:** Essential groundwork for proper OOP architecture
5. **Team Learning:** Introduces concepts gradually without overwhelming changes

**Timeline Estimate:**
- **Phase 1:** 2-3 weeks (immediate value)
- **Phase 2:** 4-6 weeks (significant improvement)
- **Phase 3:** 6-8 weeks (professional architecture)

The key insight: **Organizing globals is much better than eliminating them all at once** in legacy embedded code. Phase 1 provides 80% of the benefits with 20% of the effort.
