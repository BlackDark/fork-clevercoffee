/**
 * @file test_main.cpp
 * @brief Comprehensive unit tests for HotWaterHandler
 *
 * Tests hot water handler functionality including:
 * - Construction and initialization
 * - Water switch handling (null and valid switch)
 * - Permission checks (water tank empty, null context)
 * - Handler activation through process() with/without MachineStateContext
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

#include "clevercoffee/handlers/HotWaterHandler.h"

#include <memory>

using namespace CleverCoffee;
using ::testing::NiceMock;
using ::testing::Return;

// ============================================================================
// TEST FIXTURE
// ============================================================================

class HotWaterHandlerTest : public ::testing::Test {
  protected:
    void SetUp() override {
        systemContext_ = std::make_unique<SystemContext>();
        systemContext_->markReady();
        handler_ = std::make_unique<HotWaterHandler>(*systemContext_);

        Config::getInstance().hardwareSwitchesHotWaterEnabled.set(true);
        Config::getInstance().hardwareSwitchesHotWaterType.set(::Hardware::SwitchType::MOMENTARY);
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
    std::unique_ptr<HotWaterHandler>            handler_;
    std::unique_ptr<MachineStateContext>         machineStateContext_;
    std::unique_ptr<HardwareManager>            dummyHwManager_;
    std::unique_ptr<DisplayManager>             dummyDisplayManager_;
    std::unique_ptr<MQTTManager>                dummyMqttManager_;
    std::unique_ptr<NiceMock<MockWiFiManager>>  dummyWiFiManager_;
};

// ============================================================================
// CONSTRUCTION TESTS
// ============================================================================

TEST_F(HotWaterHandlerTest, ConstructsWithSystemContext) {
    EXPECT_FALSE(handler_->isHotWaterActive())
        << "Newly constructed handler should not report hot water active (no hardware set)";
}

TEST_F(HotWaterHandlerTest, SetHardwareSetsSwitch) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    EXPECT_CALL(mockSwitch, isPressed()).WillOnce(Return(false));
    EXPECT_FALSE(handler_->isHotWaterActive());
}

// ============================================================================
// NULL SWITCH SAFETY TESTS
// ============================================================================

TEST_F(HotWaterHandlerTest, NullSwitchReturnsFalse) {
    EXPECT_FALSE(handler_->isHotWaterActive())
        << "isHotWaterActive should return false when switch is null";
}

// ============================================================================
// SWITCH DETECTION TESTS
// ============================================================================

TEST_F(HotWaterHandlerTest, DetectsHotWaterActive) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    EXPECT_CALL(mockSwitch, isPressed()).WillOnce(Return(true));
    EXPECT_TRUE(handler_->isHotWaterActive());
}

TEST_F(HotWaterHandlerTest, DetectsHotWaterInactive) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    EXPECT_CALL(mockSwitch, isPressed()).WillOnce(Return(false));
    EXPECT_FALSE(handler_->isHotWaterActive());
}

// ============================================================================
// PROCESS FLOW TESTS
// ============================================================================

TEST_F(HotWaterHandlerTest, ProcessReturnsEarlyWhenDisabled) {
    Config::getInstance().hardwareSwitchesHotWaterEnabled.set(false);
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    handler_->process();
    // No crash — returns early because isEnabled() is false
}

TEST_F(HotWaterHandlerTest, ProcessReturnsEarlyWithNullSwitch) {
    setupMachineStateContext(MachineStateId::PID_NORMAL);
    handler_->process();
    // No crash — returns early at hardware validation
}

TEST_F(HotWaterHandlerTest, ProcessReturnsEarlyWithNullMachineStateContext) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    handler_->process();
    // No crash — returns early at permission check (null context)
}

TEST_F(HotWaterHandlerTest, ProcessDeniesPermissionWhenWaterTankEmpty) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::WATER_TANK_EMPTY);

    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();
    // Permission denied due to water tank empty — should not crash
}

TEST_F(HotWaterHandlerTest, ProcessGrantsPermissionInNormalState) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::PID_NORMAL);

    // Switch stays LOW (no change)
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    handler_->process();
    // Permission granted, processImpl runs — no crash
}

TEST_F(HotWaterHandlerTest, ProcessDetectsSwitchActivation) {
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::PID_NORMAL);

    // First: switch LOW
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    handler_->process();

    // Second: switch goes HIGH (toggle activated)
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();
    // Switch state change detected and logged — no crash
}

// ============================================================================
// TOGGLE VS MOMENTARY TESTS
// ============================================================================

TEST_F(HotWaterHandlerTest, ToggleSwitchLogsSwitchActivation) {
    Config::getInstance().hardwareSwitchesHotWaterType.set(::Hardware::SwitchType::TOGGLE);
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::PID_NORMAL);

    // Activate toggle
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();
    // Toggle activated — logs message, no crash

    // Deactivate toggle
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    handler_->process();
    // Toggle deactivated — logs message, no crash
}

TEST_F(HotWaterHandlerTest, MomentarySwitchLogsSwitchPress) {
    Config::getInstance().hardwareSwitchesHotWaterType.set(::Hardware::SwitchType::MOMENTARY);
    MockSwitch mockSwitch(::Hardware::SwitchType::MOMENTARY, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::PID_NORMAL);

    // Press momentary switch
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();
    // Momentary pressed — logs message, no crash

    // Release momentary switch
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    handler_->process();
    // Momentary released — logs message, no crash
}

// Note: main() is provided by test/main.cpp
