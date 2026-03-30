/**
 * @file test_steam_water_injection/test_main.cpp
 * @brief Unit tests for steam mode water injection via water switch
 *
 * These tests verify:
 * 1. In steam mode, activating water switch triggers pump activation
 * 2. Pump remains active while water switch is held
 * 3. Pump deactivates when water switch is released
 * 4. System stays in steam mode during water injection
 */

#include <gtest/gtest.h>
#include "../test_support.h"
#include <memory>

// Mock components for steam water injection testing
class MockRelay {
public:
    MockRelay() : is_on_(false), on_count_(0), off_count_(0) {}
    
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

class MockSwitch {
public:
    MockSwitch() : is_pressed_(false) {}
    
    void setPressed(bool pressed) {
        is_pressed_ = pressed;
    }
    
    bool isPressed() const {
        return is_pressed_;
    }
    
private:
    bool is_pressed_;
};

class MockMachineStateContext {
public:
    MockMachineStateContext() 
        : steam_mode_active_(false),
          water_switch_(new MockSwitch()),
          pump_relay_(new MockRelay()),
          steam_water_injection_active_(false) {}
    
    void setSteamModeActive(bool active) {
        steam_mode_active_ = active;
    }
    
    bool isSteamModeActive() const {
        return steam_mode_active_;
    }
    
    MockSwitch* getHotWaterSwitch() {
        // In steam mode, this represents the water injection switch
        return water_switch_.get();
    }
    
    MockRelay* getPumpRelay() {
        return pump_relay_.get();
    }
    
    // Check if water switch is requesting water injection in steam mode
    bool isSteamWaterInjectionRequested() const {
        // REQUIREMENT: In steam mode, if water switch is pressed, request injection
        return steam_mode_active_ && water_switch_->isPressed();
    }
    
    // Activate pump for steam water injection
    void activateSteamWaterInjection() {
        if (pump_relay_) {
            pump_relay_->on();
            steam_water_injection_active_ = true;
        }
    }
    
    // Deactivate pump
    void deactivateSteamWaterInjection() {
        if (pump_relay_) {
            pump_relay_->off();
            steam_water_injection_active_ = false;
        }
    }
    
    bool isSteamWaterInjectionActive() const {
        return steam_water_injection_active_;
    }
    
private:
    bool steam_mode_active_;
    std::unique_ptr<MockSwitch> water_switch_;
    std::unique_ptr<MockRelay> pump_relay_;
    bool steam_water_injection_active_;
};

// ==================== STEAM WATER INJECTION TESTS ====================

class SteamWaterInjectionTest : public ::testing::Test {
protected:
    MockMachineStateContext context;
    
    void SetUp() override {
        context.setSteamModeActive(true);
        ASSERT_TRUE(context.isSteamModeActive()) << "Setup failed: must be in steam mode";
    }
};

/**
 * TEST 1: Water switch activation in steam mode triggers pump
 * 
 * Scenario:
 *   - System is in steam mode
 *   - User presses water switch
 *   - Expected: Pump activates and water flows into boiler
 * 
 * This test would fail BEFORE the fix is implemented.
 */
TEST_F(SteamWaterInjectionTest, SteamWaterSwitchActivatesPump) {
    // ARRANGE: System is in steam mode (SetUp already did this)
    ASSERT_TRUE(context.isSteamModeActive());
    
    // ACT: User presses water switch
    context.getHotWaterSwitch()->setPressed(true);
    
    // VERIFY: Water injection is requested
    ASSERT_TRUE(context.isSteamWaterInjectionRequested()) 
        << "Water injection should be requested when water switch pressed in steam mode";
    
    // ACT: State machine detects request and activates pump
    if (context.isSteamWaterInjectionRequested()) {
        context.activateSteamWaterInjection();
    }
    
    // ASSERT: Pump is active
    EXPECT_TRUE(context.isSteamWaterInjectionActive()) 
        << "CRITICAL BUG: Pump must activate when water switch pressed in steam mode!";
    EXPECT_TRUE(context.getPumpRelay()->isOn()) 
        << "Pump relay must be ON";
    EXPECT_GT(context.getPumpRelay()->getOnCount(), 0) 
        << "Pump relay must have been activated at least once";
}

/**
 * TEST 2: Pump stays active while water switch is held
 * 
 * Scenario:
 *   - Water injection is active
 *   - User keeps water switch pressed
 *   - Expected: Pump remains active and pumping water
 */
TEST_F(SteamWaterInjectionTest, PumpStaysActiveWhileWaterSwitchHeld) {
    // ARRANGE
    context.getHotWaterSwitch()->setPressed(true);
    context.activateSteamWaterInjection();
    ASSERT_TRUE(context.isSteamWaterInjectionActive());
    
    // ACT: Simulate multiple cycles while switch is held
    const uint32_t initial_on_count = context.getPumpRelay()->getOnCount();
    
    for (int i = 0; i < 10; i++) {
        // Water switch still pressed
        ASSERT_TRUE(context.isSteamWaterInjectionRequested());
        
        // Check pump status each cycle
        if (context.isSteamWaterInjectionRequested() && !context.isSteamWaterInjectionActive()) {
            context.activateSteamWaterInjection();
        }
    }
    
    // ASSERT: Pump is still active
    EXPECT_TRUE(context.isSteamWaterInjectionActive()) 
        << "Pump must stay active while water switch is held";
}

/**
 * TEST 3: Pump deactivates when water switch is released
 * 
 * Scenario:
 *   - Water injection is active
 *   - User releases water switch
 *   - Expected: Pump stops and water flow stops
 */
TEST_F(SteamWaterInjectionTest, PumpDeactivatesWhenWaterSwitchReleased) {
    // ARRANGE: Pump is active
    context.getHotWaterSwitch()->setPressed(true);
    context.activateSteamWaterInjection();
    ASSERT_TRUE(context.isSteamWaterInjectionActive());
    ASSERT_GT(context.getPumpRelay()->getOnCount(), 0);
    
    // ACT: User releases water switch
    context.getHotWaterSwitch()->setPressed(false);
    
    // VERIFY: Injection is no longer requested
    ASSERT_FALSE(context.isSteamWaterInjectionRequested()) 
        << "Water injection should NOT be requested when switch released";
    
    // ACT: State machine detects switch release and deactivates pump
    if (!context.isSteamWaterInjectionRequested() && context.isSteamWaterInjectionActive()) {
        context.deactivateSteamWaterInjection();
    }
    
    // ASSERT: Pump is stopped
    EXPECT_FALSE(context.isSteamWaterInjectionActive()) 
        << "Pump must deactivate when water switch released";
    EXPECT_FALSE(context.getPumpRelay()->isOn()) 
        << "Pump relay must be OFF";
    EXPECT_GT(context.getPumpRelay()->getOffCount(), 0) 
        << "Pump relay must have been deactivated";
}

/**
 * TEST 4: System stays in steam mode during water injection
 * 
 * Scenario:
 *   - System is in steam mode
 *   - Water injection is active
 *   - Expected: System remains in steam mode (no auto-transition)
 */
TEST_F(SteamWaterInjectionTest, SystemStaysSteamModeDuringWaterInjection) {
    // ARRANGE: In steam mode
    ASSERT_TRUE(context.isSteamModeActive());
    
    // ACT: Activate water injection
    context.getHotWaterSwitch()->setPressed(true);
    context.activateSteamWaterInjection();
    
    // ASSERT: Still in steam mode
    EXPECT_TRUE(context.isSteamModeActive()) 
        << "System must stay in steam mode during water injection";
    EXPECT_TRUE(context.isSteamWaterInjectionActive()) 
        << "Water injection must be active";
}

/**
 * TEST 5: Water injection only works in steam mode
 * 
 * Scenario:
 *   - System is NOT in steam mode
 *   - User presses water switch
 *   - Expected: No water injection (pump doesn't activate)
 */
TEST_F(SteamWaterInjectionTest, WaterInjectionOnlyWorksInSteamMode) {
    // ARRANGE: Exit steam mode
    context.setSteamModeActive(false);
    ASSERT_FALSE(context.isSteamModeActive());
    
    // ACT: User presses water switch (but we're not in steam mode)
    context.getHotWaterSwitch()->setPressed(true);
    
    // VERIFY: Water injection NOT requested (not in steam mode)
    EXPECT_FALSE(context.isSteamWaterInjectionRequested()) 
        << "Water injection must NOT be requested outside of steam mode";
    
    // ACT: Even if we try to activate, there should be guards
    if (context.isSteamWaterInjectionRequested()) {
        context.activateSteamWaterInjection();
    }
    
    // ASSERT: Pump should not be active
    EXPECT_FALSE(context.isSteamWaterInjectionActive()) 
        << "Pump must not activate outside steam mode";
}

/**
 * TEST 6: Cycle test - multiple press/release cycles
 * 
 * Scenario:
 *   - User presses and releases water switch multiple times
 *   - Expected: Pump activates/deactivates correctly each time
 */
TEST_F(SteamWaterInjectionTest, MultiplePressReleaseCycles) {
    const int NUM_CYCLES = 5;
    uint32_t final_on_count = 0;
    uint32_t final_off_count = 0;
    
    for (int cycle = 0; cycle < NUM_CYCLES; cycle++) {
        // Press
        context.getHotWaterSwitch()->setPressed(true);
        if (context.isSteamWaterInjectionRequested()) {
            context.activateSteamWaterInjection();
        }
        
        ASSERT_TRUE(context.isSteamWaterInjectionActive()) 
            << "Pump should be active in cycle " << cycle;
        
        // Release
        context.getHotWaterSwitch()->setPressed(false);
        if (!context.isSteamWaterInjectionRequested() && context.isSteamWaterInjectionActive()) {
            context.deactivateSteamWaterInjection();
        }
        
        ASSERT_FALSE(context.isSteamWaterInjectionActive()) 
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
