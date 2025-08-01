# CleverCoffee Modernization TODO List

## Implementation Strategy

This TODO list breaks down the migration plan into small, incremental steps that maintain compilation and functionality at each stage. Each item should be completed and tested before moving to the next.

---

## Phase 1: Foundation & Cleanup (Weeks 1-2)

### 1.1 Basic Modernization (Days 1-3)

#### **TODO-001: Replace NULL with nullptr** [DONE]
- **Files**: All `.cpp` and `.h` files
- **Action**: Search and replace all `NULL` occurrences with `nullptr`
- **Test**: Compile and verify basic functionality
- **Risk**: Low - Direct replacement

#### **TODO-002: Add const correctness to utility functions** [DONE]
- **Files**: `src/main.cpp` (lines 344-372), `src/utils/helperUtils.h`
- **Action**:
  - Make `number2string()` functions take `const` parameters
  - Mark functions as `const` where appropriate
  - Add `const` to string parameters in utility functions
- **Test**: Compile and verify no functionality changes
- **Risk**: Low - Read-only operations

#### **TODO-003: Replace C-style casts with C++ casts** [DONE]
- **Files**: `src/main.cpp`, `src/Config.cpp`
- **Action**:
  - Replace `(type)value` with `static_cast<type>(value)`
  - Replace dangerous casts with `reinterpret_cast` where necessary
- **Test**: Compile and run basic operations
- **Risk**: Low - Type safety improvement

#### **TODO-004: Modernize for loops where possible** [DONE]
- **Files**: `src/main.cpp`, display template files
- **Action**:
  - Replace traditional for loops with range-based where applicable
  - Use `auto` for iterator types
- **Test**: Compile and verify display functionality
- **Risk**: Low - Syntax modernization

### 1.2 Memory Management Foundation (Days 4-6)

#### **TODO-005: Create RAII wrapper for U8G2 display** [DONE]
- **Files**: Create `src/display/DisplayManager.h/.cpp`
- **Action**:
  ```cpp
  class DisplayManager {
      std::unique_ptr<U8G2> display_;
  public:
      DisplayManager(int type, int address);
      ~DisplayManager() = default;
      U8G2* get() const { return display_.get(); }
  };
  ```
- **Test**: Initialize display through wrapper
- **Risk**: Medium - Critical display functionality

#### **TODO-006: Replace U8G2 raw pointer with DisplayManager** [DONE]
- **Files**: `src/main.cpp` (lines 894-918)
- **Action**:
  - Replace global `U8G2* u8g2` with `std::unique_ptr<DisplayManager>`
  - Update all `u8g2->` calls to use manager
- **Test**: Full display functionality testing
- **Risk**: Medium - Core display system

#### **TODO-007: Create RAII wrappers for hardware components** [DONE]
- **Files**: Create `src/hardware/HardwareManager.h/.cpp`
- **Action**:
  ```cpp
  class HardwareManager {
      std::unique_ptr<Relay> heaterRelay_;
      std::unique_ptr<Relay> pumpRelay_;
      std::unique_ptr<Relay> valveRelay_;
      // ... other components
  public:
      void initializeHardware();
      Relay& getHeaterRelay() const;
  };
  ```
- **Test**: Hardware initialization and basic control
- **Risk**: High - Core hardware functionality

#### **TODO-008: Replace hardware raw pointers with HardwareManager** [DONE]
- **Files**: `src/main.cpp` (lines 929-988)
- **Action**:
  - Move hardware initialization to HardwareManager
  - Replace global pointers with manager access
- **Test**: Complete hardware functionality test
- **Risk**: High - Critical system components

### 1.3 Code Organization (Days 7-10)

#### **TODO-009: Extract WiFi management to separate class** [DONE]
- **Files**: Create `src/network/WiFiManager.h/.cpp`
- **Action**:
  - Move WiFi setup and connection logic from `main.cpp` (lines 767-851)
  - Create `WiFiManager` class with clean interface
  - Handle offline mode gracefully
- **Test**: WiFi connection, portal setup, offline mode
- **Risk**: Medium - Network functionality

#### **TODO-010: Extract MQTT functionality to separate class** [DONE]
- **Files**: Create `src/network/MQTTManager.h/.cpp`
- **Action**:
  - Move MQTT setup and handling from `main.cpp` (lines 1002-1074)
  - Create clean MQTT interface with callbacks
  - Handle reconnection logic
- **Test**: MQTT publishing, subscription, Home Assistant integration
- **Risk**: Medium - External integrations

#### **TODO-011: Create SystemInitializer class** [DONE]
- **Files**: Create `src/core/SystemInitializer.h/.cpp`
- **Action**:
  - Extract `setup()` function logic into organized class
  - Break down into logical initialization phases
  - Add proper error handling and logging
- **Test**: Complete system startup sequence
- **Risk**: High - System initialization

#### **TODO-012: Extract sensor management to separate class** [DONE]
- **Files**: Create `src/sensors/SensorManager.h/.cpp`
- **Action**:
  - Move temperature sensor initialization and reading
  - Move pressure sensor handling
  - Move water tank sensor logic
  - Create unified sensor interface
- **Test**: All sensor readings and calibration
- **Risk**: Medium - Sensor functionality

---

## Phase 2: Architectural Refactoring (Weeks 3-4)

### 2.1 State Machine Refactoring (Days 11-15)

#### **TODO-013: Define state machine interfaces** [DONE]
- **Files**: Create `src/state/MachineState.h`
- **Action**:
  ```cpp
  class MachineStateContext; // Forward declaration

  class MachineState {
  public:
      virtual ~MachineState() = default;
      virtual void onEntry(MachineStateContext& context) {}
      virtual void onExit(MachineStateContext& context) {}
      virtual void update(MachineStateContext& context) = 0;
      virtual std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) = 0;
  };
  ```
- **Test**: Compile interface definitions
- **Risk**: Low - Interface definition only

#### **TODO-014: Create MachineStateContext** [DONE]
- **Files**: Create `src/state/MachineStateContext.h/.cpp`
- **Action**:
  - Define context class with all necessary state data
  - Include references to hardware, sensors, configuration
  - Provide clean API for state implementations
- **Test**: Context creation and data access
- **Risk**: Medium - Central state management

#### **TODO-015: Implement individual state classes** [DONE]
- **Files**: Create `src/state/states/` directory with individual state files
- **Action**:
  - `InitState.cpp` - System initialization state
  - `PidNormalState.cpp` - Normal PID operation
  - `BrewState.cpp` - Brewing operation
  - `EmergencyStopState.cpp` - Emergency safety handling
  - Placeholder states for all other machine states
  - Extract logic from current `handleMachineState()` switch
- **Test**: Each state individually
- **Risk**: High - Core business logic

#### **TODO-016: Implement StateMachine controller** [DONE]
- **Files**: Create `src/state/StateMachine.h/.cpp`
- **Action**:
  ```cpp
  class StateMachine {
      std::unique_ptr<MachineState> currentState_;
      MachineStateContext context_;
  public:
      void update();
      void transitionTo(std::unique_ptr<MachineState> newState);
  };
  ```
- **Test**: State transitions and logic
- **Risk**: High - State management system

#### **TODO-017: Replace handleMachineState() with StateMachine**
- **Files**: `src/main.cpp` (lines 391-725)
- **Action**:
  - Replace switch statement with StateMachine instance
  - Update main loop to use StateMachine::update()
  - Remove old state handling code
- **Test**: Complete state machine functionality
- **Risk**: Very High - Core system behavior

### 2.2 Main Function Decomposition (Days 16-20)

#### **TODO-018: Create ProcessController class**
- **Files**: Create `src/control/ProcessController.h/.cpp`
- **Action**:
  - Move PID controller logic and tuning
  - Move brewing process control
  - Move steam control logic
  - Create clean interfaces for process control
- **Test**: PID control, brewing, steaming processes
- **Risk**: Very High - Core control algorithms

#### **TODO-019: Extract loopPID() to ProcessController**
- **Files**: `src/main.cpp` (lines 1165-1405)
- **Action**:
  - Move entire PID loop logic to ProcessController
  - Handle temperature updates, PID computation
  - Manage brewing and steam PID modes
- **Test**: Temperature control, PID stability
- **Risk**: Very High - Temperature control system

#### **TODO-020: Create UIManager class**
- **Files**: Create `src/ui/UIManager.h/.cpp`
- **Action**:
  - Move display update logic
  - Move LED control
  - Handle user interface state
  - Manage display templates
- **Test**: Display updates, LED indicators
- **Risk**: Medium - User interface

#### **TODO-021: Extract main loop components**
- **Files**: `src/main.cpp` (lines 1148-1163)
- **Action**:
  - Break down main loop into manager calls
  - Coordinate between ProcessController, UIManager, etc.
  - Maintain timing requirements
- **Test**: Complete system operation
- **Risk**: High - System coordination

#### **TODO-022: Minimize main.cpp to coordination only**
- **Files**: `src/main.cpp`
- **Action**:
  - Reduce main.cpp to < 200 lines
  - Only setup, coordination, and cleanup
  - All business logic in appropriate managers
- **Test**: Full system functionality
- **Risk**: High - System architecture change

---

## Phase 3: Feature Enhancement (Weeks 5-6)

### 3.1 Template Usage & Type Safety (Days 21-23)

#### **TODO-023: Enhance Config system with type-safe enums**
- **Files**: `src/Config.h`, `src/defaults.h`
- **Action**:
  - Define enum classes for hardware types, modes
  - Add template specializations for enum access
  - Improve type safety in configuration
- **Test**: Configuration loading and validation
- **Risk**: Medium - Configuration system

#### **TODO-024: Add constexpr validation to Config**
- **Files**: `src/Config.h`
- **Action**:
  - Add compile-time range validation
  - Use constexpr for parameter validation
  - Improve configuration error messages
- **Test**: Configuration validation edge cases
- **Risk**: Low - Validation improvement

#### **TODO-025: Implement std::variant for parameter storage**
- **Files**: `src/Config.cpp`
- **Action**:
  - Replace void* storage with std::variant
  - Improve type safety for parameter access
  - Add visitor patterns for parameter handling
- **Test**: Configuration parameter access
- **Risk**: Medium - Parameter storage system

### 3.2 Error Handling (Days 24-26)

#### **TODO-026: Define system exception hierarchy**
- **Files**: Create `src/core/Exceptions.h`
- **Action**:
  ```cpp
  class SystemException : public std::runtime_error {};
  class HardwareException : public SystemException {};
  class NetworkException : public SystemException {};
  class ConfigurationException : public SystemException {};
  ```
- **Test**: Exception creation and handling
- **Risk**: Low - Exception definitions

#### **TODO-027: Add exception handling to critical paths**
- **Files**: SystemInitializer, HardwareManager, etc.
- **Action**:
  - Add try-catch blocks around critical operations
  - Implement graceful degradation strategies
  - Add comprehensive error logging
- **Test**: Error conditions and recovery
- **Risk**: Medium - Error handling behavior

#### **TODO-028: Implement safe mode operation**
- **Files**: Create `src/core/SafeMode.h/.cpp`
- **Action**:
  - Define minimal safe operation mode
  - Handle critical failures gracefully
  - Provide diagnostic information
- **Test**: System behavior under failures
- **Risk**: Medium - Failure handling

### 3.3 Performance Optimization (Days 27-28)

#### **TODO-029: Add move semantics to heavy objects**
- **Files**: Configuration classes, String handling
- **Action**:
  - Add move constructors and assignment operators
  - Use std::move for object transfers
  - Optimize string parameter passing
- **Test**: Performance measurement, functionality
- **Risk**: Low - Performance optimization

#### **TODO-030: Optimize memory allocations**
- **Files**: Display buffers, sensor data structures
- **Action**:
  - Use stack allocation where possible
  - Implement object pooling for frequent allocations
  - Profile memory usage patterns
- **Test**: Memory usage monitoring
- **Risk**: Medium - Memory management

---

## Phase 4: Testing & Quality (Weeks 7-8)

### 4.1 Testing Infrastructure (Days 29-32)

#### **TODO-031: Add Google Test framework**
- **Files**: `platformio.ini`, create `test/` structure
- **Action**:
  - Configure PlatformIO for unit testing
  - Add Google Test dependency
  - Create basic test structure
- **Test**: Sample test compilation and execution
- **Risk**: Low - Testing framework setup

#### **TODO-032: Create hardware mocks**
- **Files**: Create `test/mocks/` directory
- **Action**:
  - Mock GPIO operations
  - Mock sensor readings
  - Mock display operations
  - Enable testing without hardware
- **Test**: Mock functionality and test execution
- **Risk**: Medium - Testing infrastructure

#### **TODO-033: Add unit tests for core classes**
- **Files**: Create tests for each major class
- **Action**:
  - Test Config system thoroughly
  - Test state machine transitions
  - Test PID controller logic
  - Test hardware abstractions
- **Test**: Unit test coverage and correctness
- **Risk**: Medium - Test implementation

#### **TODO-034: Add integration tests**
- **Files**: Create `test/integration/` directory
- **Action**:
  - Test complete brewing process
  - Test system initialization
  - Test error handling scenarios
- **Test**: Integration test execution
- **Risk**: Medium - Integration testing

### 4.2 Quality Assurance (Days 33-35)

#### **TODO-035: Configure static analysis**
- **Files**: `.clang-tidy`, `platformio.ini`
- **Action**:
  - Configure clang-tidy for C++ checks
  - Add static analysis to build process
  - Fix identified issues
- **Test**: Static analysis clean build
- **Risk**: Low - Code quality tools

#### **TODO-036: Add code formatting**
- **Files**: `.clang-format`, format scripts
- **Action**:
  - Configure consistent code formatting
  - Format existing codebase
  - Add format validation to build
- **Test**: Consistent code formatting
- **Risk**: Low - Code formatting

#### **TODO-037: Improve documentation**
- **Files**: All public interfaces
- **Action**:
  - Add Doxygen comments to public APIs
  - Create architecture documentation
  - Document configuration parameters
- **Test**: Documentation generation
- **Risk**: Low - Documentation improvement

#### **TODO-038: Restore WiFi connection status display during system initialization**
- **Files**: `src/core/SystemInitializer.cpp`, `src/main.cpp`, `src/network/WiFiManager.cpp`
- **Action**:
  - Create a safe, early-stage display message function that doesn't depend on full display initialization
  - Implement basic WiFi status messages during connection ("Connecting...", "Connected", etc.)
  - Ensure display messages work before u8g2_prepare() and language initialization
  - Restore user feedback during WiFi connection process without causing crashes
- **Test**: Verify WiFi status appears on display during boot without crashes
- **Risk**: Medium - Display system integration

---

## Implementation Guidelines

### Before Each TODO Item:
1. **Create branch**: `git checkout -b todo-XXX-description`
2. **Backup**: Ensure current state compiles and works
3. **Small changes**: Keep changes minimal and focused
4. **Test early**: Compile and test after each logical step

### After Each TODO Item:
1. **Compile**: `~/.platformio/penv/bin/pio run -e esp32_usb`
2. **Format**: `~/.platformio/penv/bin/pio run --target format -e esp32_usb`
3. **Test**: Verify specific functionality works
4. **Commit**: `git commit -m "TODO-XXX: Description of changes"`

### Critical Testing Points:
- **TODO-005, 006**: Display functionality
- **TODO-007, 008**: Hardware control (heater, pump, valve)
- **TODO-015, 016, 017**: State machine behavior
- **TODO-018, 019**: PID control and temperature regulation
- **TODO-021, 022**: Complete system integration

### Rollback Strategy:
If any TODO item breaks functionality:
1. **Document**: Note what broke and why
2. **Revert**: `git checkout previous-working-commit`
3. **Analyze**: Understand the issue
4. **Retry**: Attempt with smaller changes or different approach

---

## Success Criteria

Each TODO item is considered complete when:
- ✅ Code compiles without warnings
- ✅ Specific functionality tested and working
- ✅ No regression in existing features
- ✅ Code formatted and documented
- ✅ Changes committed to version control

The entire migration is successful when:
- ✅ Full system functionality maintained
- ✅ Code quality metrics improved (complexity, maintainability)
- ✅ Unit test coverage > 80%
- ✅ Documentation complete and accurate
- ✅ Performance within acceptable limits

---

*This TODO list represents approximately 8 weeks of focused development work. Each item should be approached methodically with proper testing and validation to ensure system stability throughout the modernization process.*
