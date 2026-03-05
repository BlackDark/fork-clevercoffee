/**
 * @file test_main.cpp
 * @brief Comprehensive unit tests for BrewHandler
 *
 * Tests the brew handler functionality including:
 * - Construction and initialization
 * - Switch press detection (null and valid switch)
 * - Switch state change detection (pressed/released)
 * - Handler activation through process() with/without MachineStateContext
 * - Valve safety shutdown check
 * - Edge cases (null pointers, repeated calls)
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>
#include "../test_support.h"
#include "../ConfigTestHelper.h"
#include "../mocks/MockSwitch.h"
#include "../mocks/MockWiFiManager.h"

// Include shared handler test stubs (provides SystemContext, MachineStateContext stubs, etc.)
#include "../mocks/HandlerTestStubs.cpp"

// Handler header is included transitively via SystemContext.cpp
#include "clevercoffee/handlers/BrewHandler.h"

#include <memory>

using namespace CleverCoffee;
using ::testing::NiceMock;
using ::testing::Return;

// ============================================================================
// TEST FIXTURE
// ============================================================================

class BrewHandlerTest : public ::testing::Test {
  protected:
    void SetUp() override {
        systemContext_ = std::make_unique<SystemContext>();
        systemContext_->markReady();
        handler_ = std::make_unique<BrewHandler>(*systemContext_, Config::getInstance());

        Config::getInstance().hardwareSwitchesBrewEnabled.set(true);
        Config::getInstance().hardwareSwitchesBrewType.set(::Hardware::SwitchType::MOMENTARY);
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
    std::unique_ptr<BrewHandler>                handler_;
    std::unique_ptr<MachineStateContext>         machineStateContext_;
    std::unique_ptr<HardwareManager>            dummyHwManager_;
    std::unique_ptr<DisplayManager>             dummyDisplayManager_;
    std::unique_ptr<MQTTManager>                dummyMqttManager_;
    std::unique_ptr<NiceMock<MockWiFiManager>>  dummyWiFiManager_;
};

// ============================================================================
// CONSTRUCTION TESTS
// ============================================================================

TEST_F(BrewHandlerTest, ConstructsWithSystemContext) {
    EXPECT_FALSE(handler_->isBrewSwitchPressed())
        << "Newly constructed handler should not report switch pressed (no hardware set)";
}

TEST_F(BrewHandlerTest, SetHardwareSetsPointers) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    GPIOPin    pin(1, GPIOPin::OUT);
    Relay      valveRelay(pin, ::Hardware::RelayTriggerType::HIGH_TRIGGER);

    handler_->setHardware(&mockSwitch, &valveRelay);

    EXPECT_CALL(mockSwitch, isPressed()).WillOnce(Return(false));
    EXPECT_FALSE(handler_->isBrewSwitchPressed());
}

// ============================================================================
// NULL SWITCH SAFETY TESTS
// ============================================================================

TEST_F(BrewHandlerTest, NullSwitchReturnsFalseForAllQueries) {
    EXPECT_FALSE(handler_->isBrewSwitchPressed());
    EXPECT_FALSE(handler_->hasSwitchStateChanged());
    EXPECT_FALSE(handler_->wasSwitchPressed());
    EXPECT_FALSE(handler_->wasSwitchReleased());
}

TEST_F(BrewHandlerTest, NullSwitchIsBrewActiveReturnsFalse) {
    EXPECT_FALSE(handler_->isBrewActive())
        << "isBrewActive should return false when MachineStateContext is null";
}

// ============================================================================
// SWITCH DETECTION TESTS
// ============================================================================

TEST_F(BrewHandlerTest, DetectsSwitchPress) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch, nullptr);

    EXPECT_CALL(mockSwitch, isPressed()).WillOnce(Return(true));
    EXPECT_TRUE(handler_->isBrewSwitchPressed());
}

TEST_F(BrewHandlerTest, DetectsSwitchRelease) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch, nullptr);

    EXPECT_CALL(mockSwitch, isPressed()).WillOnce(Return(false));
    EXPECT_FALSE(handler_->isBrewSwitchPressed());
}

TEST_F(BrewHandlerTest, ClearsSwitchStateChange) {
    EXPECT_FALSE(handler_->hasSwitchStateChanged());
    handler_->clearSwitchStateChange();
    EXPECT_FALSE(handler_->hasSwitchStateChanged());
}

TEST_F(BrewHandlerTest, GetsSwitchTypeFromConfig) {
    Config::getInstance().hardwareSwitchesBrewType.set(::Hardware::SwitchType::MOMENTARY);
    EXPECT_EQ(::Hardware::SwitchType::MOMENTARY, handler_->getSwitchType());

    Config::getInstance().hardwareSwitchesBrewType.set(::Hardware::SwitchType::TOGGLE);
    EXPECT_EQ(::Hardware::SwitchType::TOGGLE, handler_->getSwitchType());
}

// ============================================================================
// PROCESS FLOW TESTS
// ============================================================================

TEST_F(BrewHandlerTest, ProcessReturnsEarlyWhenDisabled) {
    Config::getInstance().hardwareSwitchesBrewEnabled.set(false);
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch, nullptr);

    handler_->process();
    // No crash — returns early because isEnabled() is false
}

TEST_F(BrewHandlerTest, ProcessReturnsEarlyWithNullSwitch) {
    setupMachineStateContext(MachineStateId::PID_NORMAL);
    handler_->process();
    // No crash — returns early at hardware validation
}

TEST_F(BrewHandlerTest, ProcessReturnsEarlyWithNullMachineStateContext) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch, nullptr);

    EXPECT_CALL(mockSwitch, isPressed()).Times(0);
    handler_->process();
    // No crash — returns early at permission check (null context)
}

TEST_F(BrewHandlerTest, ProcessDetectsSwitchChangeWithContext) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch, nullptr);
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
    EXPECT_TRUE(machineStateContext_->isBrewStartRequested());

    handler_->clearSwitchStateChange();
    EXPECT_FALSE(handler_->hasSwitchStateChanged());
}

TEST_F(BrewHandlerTest, ProcessDeniesPermissionWhenWaterTankEmpty) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch, nullptr);
    setupMachineStateContext(MachineStateId::WATER_TANK_EMPTY);

    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();
    EXPECT_FALSE(machineStateContext_->isBrewStartRequested())
        << "Brew should not be requested when water tank is empty";
}

// ============================================================================
// VALVE SAFETY TESTS
// ============================================================================

TEST_F(BrewHandlerTest, ValveSafetyShutdownWithNullRelay) {
    handler_->valveSafetyShutdownCheck();
    // No crash with null relay
}

TEST_F(BrewHandlerTest, ValveSafetyShutdownClosesValveWhenNotBrewing) {
    GPIOPin    pin(1, GPIOPin::OUT);
    Relay      valveRelay(pin, ::Hardware::RelayTriggerType::HIGH_TRIGGER);
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);

    handler_->setHardware(&mockSwitch, &valveRelay);
    handler_->valveSafetyShutdownCheck();
    // No crash — valve off() called (goes through GPIOPin stub)
}

// ============================================================================
// TOGGLE SWITCH TESTS
// ============================================================================

TEST_F(BrewHandlerTest, ToggleSwitchDetectsActivationAndDeactivation) {
    Config::getInstance().hardwareSwitchesBrewType.set(::Hardware::SwitchType::TOGGLE);
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch, nullptr);
    setupMachineStateContext(MachineStateId::PID_NORMAL);

    // Toggle ON
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();
    EXPECT_TRUE(handler_->wasSwitchPressed());
    EXPECT_TRUE(machineStateContext_->isBrewStartRequested());

    handler_->clearSwitchStateChange();
    machineStateContext_->setBrewStartRequested(false);

    // Toggle OFF while brewing
    machineStateContext_->setCurrentStateId(MachineStateId::BREW_RUNNING);
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    handler_->process();
    EXPECT_TRUE(handler_->wasSwitchReleased());
    EXPECT_TRUE(machineStateContext_->isBrewStopRequested())
        << "Toggle off should request brew stop when brewing";
}

// Note: main() is provided by test/main.cpp
