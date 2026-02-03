# Enable Disabled ProcessController Tests Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable the 3 disabled ProcessController integration tests by adding minimal MachineStateContext stubs to prevent segfaults.

**Architecture:** Add global state tracking for emergency stop and stub MachineStateContext methods. Enable tests incrementally, verifying each passes before moving to the next.

**Tech Stack:** C++17, GoogleTest, PlatformIO native tests

---

## Task 1: Add MachineStateContext Emergency Stop Stubs

**Files:**
- Modify: `test/test_process_controller/test_main.cpp`

**Step 1: Add global emergency stop state**

Add after the existing stubs (around line 38):

```cpp
// Stub for MachineStateContext emergency stop tracking
namespace {
    bool emergencyStopState = false;
}

void MachineStateContext::setEmergencyStop(bool state) noexcept {
    emergencyStopState = state;
}

bool MachineStateContext::isEmergencyStop() const {
    return emergencyStopState;
}
```

**Step 2: Add reset function to TearDown**

Update the test fixture TearDown method to reset emergency state:

```cpp
void TearDown() override {
    controller_.reset();
    systemContext_.reset();
    emergencyStopState = false;  // Reset for next test
}
```

**Step 3: Verify compilation**

Run: `~/.platformio/penv/bin/pio test -e native_test -f test_process_controller --without-testing 2>&1 | tail -10`

Expected: Compiles successfully

**Step 4: Commit**

```bash
git add test/test_process_controller/test_main.cpp
git commit -m "test: add MachineStateContext emergency stop stubs"
```

---

## Task 2: Enable EmergencyStopOnOvertemperature Test

**Files:**
- Modify: `test/test_process_controller/test_main.cpp`

**Step 1: Remove DISABLED_ prefix**

Find the test (around line 135) and change:

```cpp
// Before:
TEST_F(ProcessControllerIntegrationTest, DISABLED_EmergencyStopOnOvertemperature) {

// After:
TEST_F(ProcessControllerIntegrationTest, EmergencyStopOnOvertemperature) {
```

**Step 2: Update test verification**

The test currently expects `emergencyTriggered` to be true. Since we're using stubs, verify the effects instead:

Replace the last part of the test (lines 153-158) with:

```cpp
// Third call should trigger emergency
controller_->updateTemperature();
bool emergencyTriggered = controller_->testEmergencyConditions();

EXPECT_TRUE(emergencyTriggered)
    << "Emergency should trigger after 3 consecutive overtemperature readings";

// Verify emergency actions were taken
EXPECT_FALSE(controller_->isPIDEnabled())
    << "PID should be disabled after emergency";
EXPECT_EQ(0.0, controller_->getPIDOutput())
    << "PID output should be zeroed after emergency";
```

**Step 3: Run the test**

Run: `~/.platformio/penv/bin/pio test -e native_test -f test_process_controller --gtest_filter="*EmergencyStop*" -v 2>&1 | tail -20`

Expected: Test PASSES

**Step 4: Commit**

```bash
git add test/test_process_controller/test_main.cpp
git commit -m "test: enable EmergencyStopOnOvertemperature test"
```

---

## Task 3: Enable PID Output Clamping Tests

**Files:**
- Modify: `test/test_process_controller/test_main.cpp`

**Step 1: Remove DISABLED_ prefix from both PID tests**

Find the tests (around lines 168 and 189) and change:

```cpp
// Test 1 - Before:
TEST_F(ProcessControllerIntegrationTest, DISABLED_PIDOutputClampedToSafeBounds) {

// Test 1 - After:
TEST_F(ProcessControllerIntegrationTest, PIDOutputClampedToSafeBounds) {

// Test 2 - Before:
TEST_F(ProcessControllerIntegrationTest, DISABLED_PIDOutputClampedWhenHot) {

// Test 2 - After:
TEST_F(ProcessControllerIntegrationTest, PIDOutputClampedWhenHot) {
```

**Step 2: Update PIDOutputClampedToSafeBounds test**

Add multiple update cycles to allow PID to respond. Replace the test body (lines 169-182):

```cpp
TEST_F(ProcessControllerIntegrationTest, PIDOutputClampedToSafeBounds) {
    controller_->initialize();
    controller_->setPIDEnabled(true);

    // Set a large temperature error to push PID to extremes
    ON_CALL(mockHardwareManager_, getCurrentTemperature())
        .WillByDefault(Return(20.0));  // Far below setpoint

    // Run multiple update cycles to let PID respond
    for (int i = 0; i < 5; i++) {
        controller_->updateTemperature();
        controller_->computePID();
    }

    double output = controller_->getPIDOutput();
    EXPECT_GE(output, 0.0) << "PID output should not be negative";
    EXPECT_LE(output, 1000.0) << "PID output should not exceed 1000";
}
```

**Step 3: Update PIDOutputClampedWhenHot test**

Replace the test body (lines 189-203):

```cpp
TEST_F(ProcessControllerIntegrationTest, PIDOutputClampedWhenHot) {
    controller_->initialize();
    controller_->setPIDEnabled(true);

    // Temperature well above setpoint
    ON_CALL(mockHardwareManager_, getCurrentTemperature())
        .WillByDefault(Return(100.0));  // Above 95°C setpoint

    // Run multiple update cycles
    for (int i = 0; i < 5; i++) {
        controller_->updateTemperature();
        controller_->computePID();
    }

    double output = controller_->getPIDOutput();
    EXPECT_GE(output, 0.0) << "PID output should not be negative";
    EXPECT_LE(output, 1000.0) << "PID output should not exceed 1000";
}
```

**Step 4: Run both PID tests**

Run: `~/.platformio/penv/bin/pio test -e native_test -f test_process_controller --gtest_filter="*PIDOutput*" -v 2>&1 | tail -30`

Expected: Both tests PASS

**Step 5: Commit**

```bash
git add test/test_process_controller/test_main.cpp
git commit -m "test: enable PID output clamping tests"
```

---

## Task 4: Verify Full Test Suite

**Step 1: Run all ProcessController tests**

Run: `~/.platformio/penv/bin/pio test -e native_test -f test_process_controller -v 2>&1 | tail -30`

Expected: All 10 tests PASS (0 disabled)

**Step 2: Run full test suite**

Run: `~/.platformio/penv/bin/pio test -e native_test 2>&1 | tail -40`

Expected: All 182 tests PASS (increased from 179)

**Step 3: Verify production build**

Run: `~/.platformio/penv/bin/pio run -e esp32_usb -s 2>&1 | tail -5`

Expected: Build succeeds

**Step 4: Format code**

Run: `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s`

**Step 5: Final commit if formatting changes**

```bash
git status --short
# If changes:
git add -A && git commit -m "chore: format code"
```

---

## Summary

**Tests Enabled:**
1. `EmergencyStopOnOvertemperature` - Safety-critical emergency shutdown verification
2. `PIDOutputClampedToSafeBounds` - PID output bounds when cold
3. `PIDOutputClampedWhenHot` - PID output bounds when hot

**Test Coverage:**
- **Before**: 7/10 ProcessController tests passing (3 disabled)
- **After**: 10/10 ProcessController tests passing (0 disabled)
- **Total**: 179 → 182 tests

**Changes:**
- Added 2 MachineStateContext stub methods (12 lines of code)
- Removed `DISABLED_` from 3 tests
- Enhanced test verification logic
- Zero production code changes

**Safety Coverage:**
- Emergency stop triggers on 3 consecutive overtemperature readings
- PID output constrained to [0, 1000] range
- Temperature safety thresholds enforced
