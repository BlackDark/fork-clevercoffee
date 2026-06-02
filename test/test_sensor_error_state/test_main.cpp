/**
 * @file test_main.cpp
 * @brief Unit tests for SensorErrorState transitions
 *
 * Regression tests for Bug #1:
 * - SensorErrorState must NOT spontaneously transition to PID_DISABLED after a timeout.
 * - SensorErrorState recovers back to the appropriate PID state when the error clears.
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>

#include <memory>

#include "../ConfigTestHelper.h"
#include "../test_support.h"
#include "../mocks/MockWiFiManager.h"
#include "../mocks/HandlerTestStubs.cpp"

#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/states/ErrorStates.h"

const char* getStateName(MachineStateId) {
    return "test";
}

#include "../../src/state/states/ErrorStates.cpp"

using ::testing::NiceMock;

// ============================================================================
// TEST FIXTURE
// ============================================================================

class SensorErrorStateTest : public ::testing::Test {
  protected:
    void SetUp() override {
        g_test_millis = 0;
        Preferences::resetTestStore();

        systemContext_ = std::make_unique<CleverCoffee::SystemContext>();
        systemContext_->markReady();

        hwManager_      = std::make_unique<CleverCoffee::HardwareManager>(Config::getInstance());
        displayManager_ = std::make_unique<DisplayManager>(::Hardware::OLEDType::SSD1306,
                                                            ::Hardware::OLEDAddress::ADDR_3C);
        mqttManager_    = std::make_unique<MQTTManager>();
        wifiManager_    = std::make_unique<NiceMock<MockWiFiManager>>();

        context_ = std::make_unique<MachineStateContext>(
            *systemContext_, *hwManager_, *displayManager_, *wifiManager_, *mqttManager_);
        systemContext_->setMachineStateContext(context_.get());

        context_->updateStateEntryTime(std::chrono::steady_clock::now());
    }

    void TearDown() override {
        systemContext_->setMachineStateContext(nullptr);
        context_.reset();
        hwManager_.reset();
        displayManager_.reset();
        mqttManager_.reset();
        wifiManager_.reset();
        systemContext_.reset();
        g_test_millis = 0;
    }

    std::unique_ptr<CleverCoffee::SystemContext>  systemContext_;
    std::unique_ptr<CleverCoffee::HardwareManager> hwManager_;
    std::unique_ptr<DisplayManager>               displayManager_;
    std::unique_ptr<MQTTManager>                  mqttManager_;
    std::unique_ptr<NiceMock<MockWiFiManager>>    wifiManager_;
    std::unique_ptr<MachineStateContext>           context_;
};

// ============================================================================
// REGRESSION: Bug #1 — Spontaneous PID_DISABLED after sensor error
// ============================================================================

/**
 * SensorErrorState must never transition to PID_DISABLED due to a timeout.
 *
 * The original bug: a 60-second timeout silently transitioned the machine to
 * PID_DISABLED without any user action. This caused confusion because the
 * display showed "PID disabled" after an unrelated sensor glitch.
 *
 * With the fix and PID enabled in config: after any amount of time spent in
 * SENSOR_ERROR, the state either stays (if error still present) or recovers
 * to PID_NORMAL. It must NEVER end up in PID_DISABLED unless the user
 * had explicitly disabled PID.
 */
TEST_F(SensorErrorStateTest, NeverTransitionsToPidDisabledOnTimeout) {
    // User has PID enabled — no reason for PID_DISABLED to appear
    ASSERT_TRUE(Config::getInstance().pidEnabled.set(true));

    SensorErrorState state;
    state.onEntryImpl(*context_);

    // Simulate far past the old 60-second timeout and past ERROR_RECOVERY_DELAY_MS
    g_test_millis = 120'000UL;

    auto next = state.checkSpecificTransitions(*context_);

    // Must recover to PID_NORMAL, NOT PID_DISABLED
    ASSERT_TRUE(next.has_value()) << "Should have transitioned after recovery delay";
    EXPECT_NE(*next, MachineStateId::PID_DISABLED)
        << "SensorErrorState must not transition to PID_DISABLED when user has PID enabled";
    EXPECT_EQ(*next, MachineStateId::PID_NORMAL);
}

/**
 * SensorErrorState returns nullopt (stays put) while the error is still active.
 *
 * The stubs return hasSensorError()=false and hasTemperatureError()=false which
 * triggers the recovery path. We test the persisting-error scenario by verifying
 * that within the recovery delay window the state returns nullopt.
 */
TEST_F(SensorErrorStateTest, StaysInSensorErrorWithinRecoveryDelay) {
    SensorErrorState state;
    state.onEntryImpl(*context_);

    // Less than ERROR_RECOVERY_DELAY_MS (2000ms) — still in recovery delay window
    g_test_millis = 500UL;

    auto next = state.checkSpecificTransitions(*context_);

    EXPECT_FALSE(next.has_value())
        << "SensorErrorState should not transition before recovery delay expires";
}

/**
 * SensorErrorState recovers to PID_NORMAL after the error clears and the
 * recovery delay elapses.
 *
 * Stubs: hasSensorError()=false, hasTemperatureError()=false, pidEnabled=true
 */
TEST_F(SensorErrorStateTest, RecoversToPidNormalAfterDelayWhenErrorClears) {
    // Ensure PID is enabled in config so getPidState() returns PID_NORMAL
    ASSERT_TRUE(Config::getInstance().pidEnabled.set(true));

    SensorErrorState state;
    state.onEntryImpl(*context_);

    // Jump past ERROR_RECOVERY_DELAY_MS (5000ms)
    g_test_millis = 6000UL;

    auto next = state.checkSpecificTransitions(*context_);

    ASSERT_TRUE(next.has_value()) << "SensorErrorState should transition when error cleared and delay elapsed";
    EXPECT_NE(*next, MachineStateId::PID_DISABLED)
        << "SensorErrorState must not recover into PID_DISABLED";
    EXPECT_EQ(*next, MachineStateId::PID_NORMAL)
        << "SensorErrorState should recover to PID_NORMAL when PID is enabled";
}

/**
 * SensorErrorState recovers to PID_DISABLED (not PID_NORMAL) when the user
 * had explicitly disabled PID before the sensor error occurred.
 */
TEST_F(SensorErrorStateTest, RecoversToPidDisabledWhenUserHadDisabledPid) {
    // User had disabled PID
    ASSERT_TRUE(Config::getInstance().pidEnabled.set(false));

    SensorErrorState state;
    state.onEntryImpl(*context_);

    g_test_millis = 6000UL;

    auto next = state.checkSpecificTransitions(*context_);

    ASSERT_TRUE(next.has_value());
    EXPECT_EQ(*next, MachineStateId::PID_DISABLED)
        << "Should recover to PID_DISABLED when user had disabled PID";
}

// Note: main() is provided by test/main.cpp
