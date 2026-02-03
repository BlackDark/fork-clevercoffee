# Enable Disabled ProcessController Tests Design

**Date:** 2026-02-03
**Status:** Approved
**Goal:** Enable the 3 disabled ProcessController integration tests by fixing SystemContext and PID initialization issues

## Problem Statement

Three ProcessController integration tests are currently disabled:

1. **EmergencyStopOnOvertemperature** - Segfaults when calling `SystemContext::triggerEmergencyStop()`
2. **PIDOutputClampedToSafeBounds** - PID output clamping test when temperature is low
3. **PIDOutputClampedWhenHot** - PID output clamping test when temperature is high

### Root Causes

**Emergency Stop Test Segfault:**
- `ProcessController::emergencyStop()` calls `systemContext_.triggerEmergencyStop()`
- `SystemContext::triggerEmergencyStop()` dereferences `machineStateContext_` pointer
- In tests, `machineStateContext_` is NULL → segfault

**PID Tests:**
- Tests should work once SystemContext is properly initialized
- May need multiple update cycles for PID to respond

## Solution Design

### Design Section 1: Test Fixture Enhancement

**Architecture Changes:**

Add MachineStateContext stub to test fixture to prevent NULL pointer dereference:

```cpp
class ProcessControllerIntegrationTest : public ::testing::Test {
protected:
    std::unique_ptr<SystemContext> systemContext_;
    std::unique_ptr<ProcessController> controller_;

    // Mock managers
    NiceMock<MockHardwareManager> mockHardwareManager_;
    NiceMock<MockDisplayManager> mockDisplayManager_;
    NiceMock<MockMQTTManager> mockMqttManager_;

    void SetUp() override {
        // Create real SystemContext
        systemContext_ = std::make_unique<SystemContext>();
        systemContext_->markReady();

        // Configure mocks...

        // Create real ProcessController
        controller_ = std::make_unique<ProcessController>(
            config,
            *systemContext_,
            mockHardwareManager_,
            mockDisplayManager_,
            mockMqttManager_
        );
    }
};
```

**Key Insight:** Use minimal stubbing (Option B) rather than full MachineStateContext implementation.

---

### Design Section 2: Specific Test Fixes

**Fix 1: EmergencyStopOnOvertemperature**

Enable the test with proper verification:

```cpp
TEST_F(ProcessControllerIntegrationTest, EmergencyStopOnOvertemperature) {
    controller_->initialize();

    Config& config = Config::getInstance();
    const double emergencyThreshold = config.emergencyStopTemp.get();
    const double dangerousTemp = emergencyThreshold + 5.0;  // 150°C

    // Simulate dangerous temperature
    ON_CALL(mockHardwareManager_, getCurrentTemperature())
        .WillByDefault(Return(dangerousTemp));

    // Update temperature and test emergency conditions 3 times (debounce)
    for (int i = 0; i < 3; i++) {
        controller_->updateTemperature();
        controller_->testEmergencyConditions();
    }

    // Verify emergency was triggered (check via emergencyStopActive flag)
    // Since we're using stubs, we verify the method was called
    // Alternative: check that PID was disabled and output zeroed
    EXPECT_FALSE(controller_->isPIDEnabled());
    EXPECT_EQ(0.0, controller_->getPIDOutput());
}
```

**Fix 2: PID Output Clamping Tests**

Enable tests with multiple update cycles:

```cpp
TEST_F(ProcessControllerIntegrationTest, PIDOutputClampedToSafeBounds) {
    controller_->initialize();
    controller_->setPIDEnabled(true);

    // Set temperature far below setpoint
    ON_CALL(mockHardwareManager_, getCurrentTemperature())
        .WillByDefault(Return(20.0));

    // Run several update cycles to let PID respond
    for (int i = 0; i < 5; i++) {
        controller_->updateTemperature();
        controller_->computePID();
    }

    double output = controller_->getPIDOutput();
    EXPECT_GE(output, 0.0) << "PID output should not be negative";
    EXPECT_LE(output, 1000.0) << "PID output should not exceed 1000";
}

TEST_F(ProcessControllerIntegrationTest, PIDOutputClampedWhenHot) {
    controller_->initialize();
    controller_->setPIDEnabled(true);

    // Set temperature above setpoint
    ON_CALL(mockHardwareManager_, getCurrentTemperature())
        .WillByDefault(Return(100.0));  // Above 95°C setpoint

    // Run several update cycles
    for (int i = 0; i < 5; i++) {
        controller_->updateTemperature();
        controller_->computePID();
    }

    double output = controller_->getPIDOutput();
    EXPECT_GE(output, 0.0) << "PID output should not be negative";
    EXPECT_LE(output, 1000.0) << "PID output should not exceed 1000";
}
```

---

### Design Section 3: Implementation Details

**Approach: Option B - Minimal Stubbing**

Add only the necessary stub implementations without including full MachineStateContext.cpp:

```cpp
// Global state for MachineStateContext stub
namespace {
    bool emergencyStopState = false;
}

// Stub implementations
void MachineStateContext::setEmergencyStop(bool state) noexcept {
    emergencyStopState = state;
}

bool MachineStateContext::isEmergencyStop() const {
    return emergencyStopState;
}

// Reset state in test TearDown
void resetEmergencyStopState() {
    emergencyStopState = false;
}
```

**Why Option B (Stubbing)?**
- ✅ Minimal code changes
- ✅ No additional dependencies
- ✅ Fast test execution
- ✅ Focuses on ProcessController behavior
- ❌ Doesn't test MachineStateContext integration (acceptable for unit tests)

**Alternative Option A (Full Implementation):**
- Requires including MachineStateContext.cpp
- Requires mocking StateMachine, WiFiManager, etc.
- More complex but tests full integration
- Overkill for these specific tests

---

### Design Section 4: Testing Strategy & Success Criteria

**Testing Approach:**

1. **Enable Tests Incrementally**:
   - First: Fix EmergencyStopOnOvertemperature (add MachineStateContext stubs)
   - Then: Enable both PID tests (should work once SystemContext is stable)
   - Run each test individually to verify

2. **Test Execution Commands**:
   ```bash
   # Test individual test
   ~/.platformio/penv/bin/pio test -e native_test -f test_process_controller --gtest_filter="*EmergencyStop*"

   # Test all ProcessController tests
   ~/.platformio/penv/bin/pio test -e native_test -f test_process_controller -v

   # Full test suite
   ~/.platformio/penv/bin/pio test -e native_test
   ```

3. **Expected Test Results**:
   - EmergencyStopOnOvertemperature: Verifies PID disabled and output zeroed after emergency
   - PIDOutputClampedToSafeBounds: Output stays in [0, 1000] range when cold
   - PIDOutputClampedWhenHot: Output stays in [0, 1000] range when hot

**Success Criteria:**

- ✅ All 10 ProcessController tests enabled and passing
- ✅ Total test count increases: 179 → 182 tests
- ✅ No segfaults or crashes
- ✅ Production build still succeeds
- ✅ Zero production code changes (tests only)

**Minimal Changes Required:**
1. Add 2 MachineStateContext stub methods (setEmergencyStop, isEmergencyStop)
2. Remove `DISABLED_` prefix from 3 test names
3. Possibly adjust test logic for proper verification

---

## Files Affected

| File | Change |
|------|--------|
| `test/test_process_controller/test_main.cpp` | Add MachineStateContext stubs, enable 3 tests |

---

## Implementation Phases

**Phase 1: Add Stubs**
- Add MachineStateContext stub methods (setEmergencyStop, isEmergencyStop)
- Add global state variable for emergency stop tracking

**Phase 2: Enable Emergency Test**
- Remove `DISABLED_` from EmergencyStopOnOvertemperature
- Verify test passes
- Commit

**Phase 3: Enable PID Tests**
- Remove `DISABLED_` from both PID output tests
- Verify tests pass
- Commit

**Phase 4: Verification**
- Run full test suite
- Verify production build
- Confirm 182 tests passing

---

## Expected Outcomes

### Test Coverage Enhancement
- **Before**: 7/10 ProcessController tests passing (3 disabled)
- **After**: 10/10 ProcessController tests passing (0 disabled)
- **Total Tests**: 179 → 182

### Safety-Critical Coverage
- Emergency stop on overtemperature (debounced 3 readings)
- PID output bounds enforcement (0-1000 range)
- Temperature regulation safety

### Benefits
- Validates ProcessController safety-critical behavior
- Proves interface refactoring enables comprehensive testing
- No production code changes required
- Maintains test isolation and fast execution

---

## Design Rationale

**Why Minimal Stubbing?**
- ProcessController tests should focus on ProcessController behavior
- MachineStateContext integration can be tested separately
- Faster test execution
- Simpler maintenance
- Follows single responsibility principle for tests

**Why Not Full MachineStateContext?**
- Would require StateMachine, WiFiManager mocks
- Adds complexity without proportional value
- Slows down test execution
- Makes tests harder to understand and maintain

**Why Enable All 3 Tests?**
- Safety-critical behavior must be tested
- Validates the entire refactoring effort
- Provides confidence in ProcessController reliability
- Completes the originally planned test suite
