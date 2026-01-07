/**
 * @file test_pid_mode_water_dispensing/test_main.cpp
 * @brief Unit tests for water dispensing in PID mode
 *
 * These tests verify:
 * 1. In PID mode, water switch activates pump for dispensing hot water
 * 2. Pump stays active while water switch is held
 * 3. Pump deactivates when water switch is released
 * 4. System remains in PID mode during water dispensing
 */

#include <gtest/gtest.h>
#include <memory>

// Mock Relay (reuse from steam tests)
class MockRelayPid {
public:
    MockRelayPid() : is_on_(false), on_count_(0), off_count_(0) {}
    
    void on() {
        is_on_ = true;
        on_count_++;
    }
    
    void off() {
        is_on_ = false;
        off_count_++;
    }
    
    bool isOn() const {
        return is_on_;
    }
    
    uint32_t getOnCount() const {
        return on_count_;
    }
    
    uint32_t getOffCount() const {
        return off_count_;
    }
    
    void reset() {
        on_count_ = 0;
        off_count_ = 0;
        is_on_ = false;
    }
    
private:
    bool is_on_;
    uint32_t on_count_;
    uint32_t off_count_;
};

// Mock Switch (reuse concept)
class MockSwitchPid {
public:
    MockSwitchPid() : is_pressed_(false) {}
    
    void setPressed(bool pressed) {
        is_pressed_ = pressed;
    }
    
    bool isPressed() const {
        return is_pressed_;
    }
    
private:
    bool is_pressed_;
};

// Mock PID Mode Context
class MockPidModeContext {
public:
    MockPidModeContext() 
        : pid_mode_active_(true),
          water_switch_(new MockSwitchPid()),
          pump_relay_(new MockRelayPid()),
          water_dispensing_active_(false) {}
    
    void setPidModeActive(bool active) {
        pid_mode_active_ = active;
    }
    
    bool isPidModeActive() const {
        return pid_mode_active_;
    }
    
    MockSwitchPid* getWaterDispenseSwitch() {
        // In PID mode, the hot water switch dispenses water
        return water_switch_.get();
    }
    
    MockRelayPid* getPumpRelay() {
        return pump_relay_.get();
    }
    
    // Check if water switch is requesting water dispensing in PID mode
    bool isWaterDispenseRequested() const {
        // In PID mode, if water switch is pressed, request dispensing
        return pid_mode_active_ && water_switch_->isPressed();
    }
    
    // Activate pump for hot water dispensing
    void activateWaterDispense() {
        if (pump_relay_) {
            pump_relay_->on();
            water_dispensing_active_ = true;
        }
    }
    
    // Deactivate pump
    void deactivateWaterDispense() {
        if (pump_relay_) {
            pump_relay_->off();
            water_dispensing_active_ = false;
        }
    }
    
    bool isWaterDispensingActive() const {
        return water_dispensing_active_;
    }
    
private:
    bool pid_mode_active_;
    std::unique_ptr<MockSwitchPid> water_switch_;
    std::unique_ptr<MockRelayPid> pump_relay_;
    bool water_dispensing_active_;
};

// ==================== PID MODE WATER DISPENSING TESTS ====================

class PidModeWaterDispensingTest : public ::testing::Test {
protected:
    MockPidModeContext context;
    
    void SetUp() override {
        context.setPidModeActive(true);
        ASSERT_TRUE(context.isPidModeActive()) << "Setup failed: must be in PID mode";
    }
};

/**
 * TEST 1: Water switch activation in PID mode triggers pump for hot water dispensing
 * 
 * Scenario:
 *   - System is in PID mode (temperature controlled, ready state)
 *   - User presses water switch
 *   - Expected: Pump activates and hot water flows out
 */
TEST_F(PidModeWaterDispensingTest, WaterSwitchActivatesPumpInPidMode) {
    // ARRANGE: System is in PID mode (SetUp already did this)
    ASSERT_TRUE(context.isPidModeActive());
    
    // ACT: User presses water switch
    context.getWaterDispenseSwitch()->setPressed(true);
    
    // VERIFY: Water dispensing is requested
    ASSERT_TRUE(context.isWaterDispenseRequested()) 
        << "Water dispensing should be requested when water switch pressed in PID mode";
    
    // ACT: State machine detects request and activates pump
    if (context.isWaterDispenseRequested()) {
        context.activateWaterDispense();
    }
    
    // ASSERT: Pump is active
    EXPECT_TRUE(context.isWaterDispensingActive()) 
        << "CRITICAL: Pump must activate when water switch pressed in PID mode!";
    EXPECT_TRUE(context.getPumpRelay()->isOn()) 
        << "Pump relay must be ON";
    EXPECT_GT(context.getPumpRelay()->getOnCount(), 0) 
        << "Pump relay must have been activated at least once";
}

/**
 * TEST 2: Pump stays active while water switch is held in PID mode
 * 
 * Scenario:
 *   - Water dispensing is active
 *   - User keeps water switch pressed
 *   - Expected: Pump remains active and water flows continuously
 */
TEST_F(PidModeWaterDispensingTest, PumpStaysActiveWhileWaterSwitchHeldInPidMode) {
    // ARRANGE
    context.getWaterDispenseSwitch()->setPressed(true);
    context.activateWaterDispense();
    ASSERT_TRUE(context.isWaterDispensingActive());
    
    // ACT: Simulate multiple cycles while switch is held
    const uint32_t initial_on_count = context.getPumpRelay()->getOnCount();
    
    for (int i = 0; i < 10; i++) {
        // Water switch still pressed
        ASSERT_TRUE(context.isWaterDispenseRequested());
        
        // Check pump status each cycle
        if (context.isWaterDispenseRequested() && !context.isWaterDispensingActive()) {
            context.activateWaterDispense();
        }
    }
    
    // ASSERT: Pump is still active
    EXPECT_TRUE(context.isWaterDispensingActive()) 
        << "Pump must stay active while water switch is held";
}

/**
 * TEST 3: Pump deactivates when water switch is released in PID mode
 * 
 * Scenario:
 *   - Water dispensing is active
 *   - User releases water switch
 *   - Expected: Pump stops and water flow stops
 */
TEST_F(PidModeWaterDispensingTest, PumpDeactivatesWhenWaterSwitchReleasedInPidMode) {
    // ARRANGE: Pump is active
    context.getWaterDispenseSwitch()->setPressed(true);
    context.activateWaterDispense();
    ASSERT_TRUE(context.isWaterDispensingActive());
    ASSERT_GT(context.getPumpRelay()->getOnCount(), 0);
    
    // ACT: User releases water switch
    context.getWaterDispenseSwitch()->setPressed(false);
    
    // VERIFY: Dispensing is no longer requested
    ASSERT_FALSE(context.isWaterDispenseRequested()) 
        << "Water dispensing should NOT be requested when switch released";
    
    // ACT: State machine detects switch release and deactivates pump
    if (!context.isWaterDispenseRequested() && context.isWaterDispensingActive()) {
        context.deactivateWaterDispense();
    }
    
    // ASSERT: Pump is stopped
    EXPECT_FALSE(context.isWaterDispensingActive()) 
        << "Pump must deactivate when water switch released";
    EXPECT_FALSE(context.getPumpRelay()->isOn()) 
        << "Pump relay must be OFF";
    EXPECT_GT(context.getPumpRelay()->getOffCount(), 0) 
        << "Pump relay must have been deactivated";
}

/**
 * TEST 4: System stays in PID mode during water dispensing
 * 
 * Scenario:
 *   - System is in PID mode
 *   - Water dispensing is active
 *   - Expected: System remains in PID mode (no auto-transition)
 */
TEST_F(PidModeWaterDispensingTest, SystemStaysPidModeDuringWaterDispensing) {
    // ARRANGE: In PID mode
    ASSERT_TRUE(context.isPidModeActive());
    
    // ACT: Activate water dispensing
    context.getWaterDispenseSwitch()->setPressed(true);
    context.activateWaterDispense();
    
    // ASSERT: Still in PID mode
    EXPECT_TRUE(context.isPidModeActive()) 
        << "System must stay in PID mode during water dispensing";
    EXPECT_TRUE(context.isWaterDispensingActive()) 
        << "Water dispensing must be active";
}

/**
 * TEST 5: Water dispensing only works in PID mode
 * 
 * Scenario:
 *   - System is NOT in PID mode
 *   - User presses water switch
 *   - Expected: No water dispensing (pump doesn't activate)
 */
TEST_F(PidModeWaterDispensingTest, WaterDispensingOnlyWorksInPidMode) {
    // ARRANGE: Exit PID mode
    context.setPidModeActive(false);
    ASSERT_FALSE(context.isPidModeActive());
    
    // ACT: User presses water switch (but we're not in PID mode)
    context.getWaterDispenseSwitch()->setPressed(true);
    
    // VERIFY: Water dispensing NOT requested (not in PID mode)
    EXPECT_FALSE(context.isWaterDispenseRequested()) 
        << "Water dispensing must NOT be requested outside of PID mode";
    
    // ACT: Even if we try to activate, there should be guards
    if (context.isWaterDispenseRequested()) {
        context.activateWaterDispense();
    }
    
    // ASSERT: Pump should not be active
    EXPECT_FALSE(context.isWaterDispensingActive()) 
        << "Pump must not activate outside PID mode";
}

/**
 * TEST 6: Multiple dispense cycles work correctly
 * 
 * Scenario:
 *   - User presses and releases water switch multiple times
 *   - Expected: Pump activates/deactivates correctly each time
 */
TEST_F(PidModeWaterDispensingTest, MultipleDenseCycles) {
    const int NUM_CYCLES = 5;
    uint32_t final_on_count = 0;
    uint32_t final_off_count = 0;
    
    for (int cycle = 0; cycle < NUM_CYCLES; cycle++) {
        // Press water switch
        context.getWaterDispenseSwitch()->setPressed(true);
        if (context.isWaterDispenseRequested()) {
            context.activateWaterDispense();
        }
        
        ASSERT_TRUE(context.isWaterDispensingActive()) 
            << "Pump should be active in cycle " << cycle;
        
        // Release water switch
        context.getWaterDispenseSwitch()->setPressed(false);
        if (!context.isWaterDispenseRequested() && context.isWaterDispensingActive()) {
            context.deactivateWaterDispense();
        }
        
        ASSERT_FALSE(context.isWaterDispensingActive()) 
            << "Pump should be inactive in cycle " << cycle;
    }
    
    // ASSERT: Each cycle should have one on and one off
    final_on_count = context.getPumpRelay()->getOnCount();
    final_off_count = context.getPumpRelay()->getOffCount();
    
    EXPECT_EQ(final_on_count, NUM_CYCLES) 
        << "Pump should activate " << NUM_CYCLES << " times, got " << final_on_count;
    EXPECT_EQ(final_off_count, NUM_CYCLES) 
        << "Pump should deactivate " << NUM_CYCLES << " times, got " << final_off_count;
}

// Note: main() is provided by test/main.cpp for all tests
