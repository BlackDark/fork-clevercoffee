/**
 * @file test_main.cpp
 * @brief Comprehensive unit tests for SteamHandler
 *
 * Tests steam handler functionality including:
 * - Construction and initialization
 * - Switch press detection (null and valid switch)
 * - Switch state change detection (pressed/released)
 * - Handler activation through process() with/without MachineStateContext
 * - Toggle vs momentary switch behavior
 * - Edge cases (null pointers, repeated calls)
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>
#include "../test_support.h"
#include "../ConfigTestHelper.h"
#include "../mocks/MockSwitch.h"
#include "../mocks/MockWiFiManager.h"

// Include shared handler test stubs
#include "../mocks/HandlerTestStubs.cpp"

#include "clevercoffee/handlers/SteamHandler.h"

#include <memory>

using namespace CleverCoffee;
using ::testing::NiceMock;
using ::testing::Return;

// ============================================================================
// TEST FIXTURE
// ============================================================================

class SteamHandlerTest : public ::testing::Test {
  protected:
    void SetUp() override {
        systemContext_ = std::make_unique<SystemContext>();
        systemContext_->markReady();
        handler_ = std::make_unique<SteamHandler>(*systemContext_);

        Config::getInstance().hardwareSwitchesSteamEnabled.set(true);
        Config::getInstance().hardwareSwitchesSteamType.set(::Hardware::SwitchType::MOMENTARY);
    }

    void TearDown() override {
        handler_.reset();
        systemContext_->setMachineStateContext(nullptr);
        systemContext_.reset();
        resetConfigDefaults();
    }

    void setupMachineStateContext(MachineStateId initialState = MachineStateId::PID_NORMAL) {
        dummyHwManager_      = std::make_unique<HardwareManager>(Config::getInstance());
        dummyDisplayManager_ = std::make_unique<DisplayManager>(::Hardware::OLEDType::SSD1306, ::Hardware::OLEDAddress::ADDR_3C);
        dummyMqttManager_    = std::make_unique<MQTTManager>();
        dummyWiFiManager_    = std::make_unique<NiceMock<MockWiFiManager>>();

        machineStateContext_ = std::make_unique<MachineStateContext>(
            *systemContext_, *dummyHwManager_, *dummyDisplayManager_, *dummyWiFiManager_, *dummyMqttManager_);
        machineStateContext_->setCurrentStateId(initialState);
        systemContext_->setMachineStateContext(machineStateContext_.get());
    }

    std::unique_ptr<SystemContext>              systemContext_;
    std::unique_ptr<SteamHandler>               handler_;
    std::unique_ptr<MachineStateContext>         machineStateContext_;
    std::unique_ptr<HardwareManager>            dummyHwManager_;
    std::unique_ptr<DisplayManager>             dummyDisplayManager_;
    std::unique_ptr<MQTTManager>                dummyMqttManager_;
    std::unique_ptr<NiceMock<MockWiFiManager>>  dummyWiFiManager_;
};

// ============================================================================
// CONSTRUCTION TESTS
// ============================================================================

TEST_F(SteamHandlerTest, ConstructsWithSystemContext) {
    EXPECT_FALSE(handler_->isSteamSwitchPressed())
        << "Newly constructed handler should not report switch pressed";
}

TEST_F(SteamHandlerTest, SetHardwareSetsSwitch) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    EXPECT_CALL(mockSwitch, isPressed()).WillOnce(Return(false));
    EXPECT_FALSE(handler_->isSteamSwitchPressed());
}

// ============================================================================
// NULL SWITCH SAFETY TESTS
// ============================================================================

TEST_F(SteamHandlerTest, NullSwitchReturnsFalseForAllQueries) {
    EXPECT_FALSE(handler_->isSteamSwitchPressed());
    EXPECT_FALSE(handler_->hasSwitchStateChanged());
    EXPECT_FALSE(handler_->wasSwitchPressed());
    EXPECT_FALSE(handler_->wasSwitchReleased());
}

// ============================================================================
// SWITCH DETECTION TESTS
// ============================================================================

TEST_F(SteamHandlerTest, DetectsSwitchPress) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    EXPECT_CALL(mockSwitch, isPressed()).WillOnce(Return(true));
    EXPECT_TRUE(handler_->isSteamSwitchPressed());
}

TEST_F(SteamHandlerTest, DetectsSwitchRelease) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    EXPECT_CALL(mockSwitch, isPressed()).WillOnce(Return(false));
    EXPECT_FALSE(handler_->isSteamSwitchPressed());
}

TEST_F(SteamHandlerTest, ClearsSwitchStateChange) {
    EXPECT_FALSE(handler_->hasSwitchStateChanged());
    handler_->clearSwitchStateChange();
    EXPECT_FALSE(handler_->hasSwitchStateChanged());
}

TEST_F(SteamHandlerTest, GetsSwitchTypeFromConfig) {
    Config::getInstance().hardwareSwitchesSteamType.set(::Hardware::SwitchType::MOMENTARY);
    EXPECT_EQ(::Hardware::SwitchType::MOMENTARY, handler_->getSwitchType());

    Config::getInstance().hardwareSwitchesSteamType.set(::Hardware::SwitchType::TOGGLE);
    EXPECT_EQ(::Hardware::SwitchType::TOGGLE, handler_->getSwitchType());
}

// ============================================================================
// PROCESS FLOW TESTS
// ============================================================================

TEST_F(SteamHandlerTest, ProcessReturnsEarlyWhenDisabled) {
    Config::getInstance().hardwareSwitchesSteamEnabled.set(false);
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    handler_->process();
    // No crash — returns early because isEnabled() is false
}

TEST_F(SteamHandlerTest, ProcessReturnsEarlyWithNullSwitch) {
    handler_->process();
    // isHardwareValid() returns false — returns early
}

TEST_F(SteamHandlerTest, ProcessDetectsSwitchChangeWithContext) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::PID_NORMAL);

    // First: switch stays LOW (no change)
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    handler_->process();
    EXPECT_FALSE(handler_->hasSwitchStateChanged());

    // Second: switch goes HIGH (state change)
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();
    EXPECT_TRUE(handler_->hasSwitchStateChanged());
    EXPECT_TRUE(handler_->wasSwitchPressed());
    EXPECT_TRUE(machineStateContext_->isSteamStartRequested())
        << "Steam start should be requested on momentary press in PID_NORMAL state";

    handler_->clearSwitchStateChange();
    EXPECT_FALSE(handler_->hasSwitchStateChanged());
}

TEST_F(SteamHandlerTest, MomentarySecondPressStopsSteam) {
    Config::getInstance().hardwareSwitchesSteamType.set(::Hardware::SwitchType::MOMENTARY);
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::STEAM_RUNNING);

    // Press while steaming → stop request
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();
    EXPECT_TRUE(machineStateContext_->isSteamStopRequested())
        << "Second momentary press while steaming should request stop";
}

// ============================================================================
// TOGGLE SWITCH TESTS
// ============================================================================

TEST_F(SteamHandlerTest, ToggleSwitchActivationAndDeactivation) {
    Config::getInstance().hardwareSwitchesSteamType.set(::Hardware::SwitchType::TOGGLE);
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::PID_NORMAL);

    // Toggle ON
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();
    EXPECT_TRUE(handler_->wasSwitchPressed());
    EXPECT_TRUE(machineStateContext_->isSteamStartRequested());

    handler_->clearSwitchStateChange();
    machineStateContext_->setSteamStartRequested(false);

    // Toggle OFF while steaming
    machineStateContext_->setCurrentStateId(MachineStateId::STEAM_RUNNING);
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    handler_->process();
    EXPECT_TRUE(handler_->wasSwitchReleased());
    EXPECT_TRUE(machineStateContext_->isSteamStopRequested())
        << "Toggle off should request steam stop";
}

// Note: main() is provided by test/main.cpp
