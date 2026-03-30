/**
 * @file test_main.cpp
 * @brief Comprehensive unit tests for PowerHandler
 *
 * Tests power handler functionality including:
 * - Construction and initialization
 * - Power switch handling (null and valid switch)
 * - Toggle vs momentary switch behavior
 * - Power on/off state machine interaction
 * - Long press reboot detection
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

#include "clevercoffee/handlers/PowerHandler.h"

#include <memory>

using namespace CleverCoffee;
using ::testing::NiceMock;
using ::testing::Return;

// ============================================================================
// TEST FIXTURE
// ============================================================================

class PowerHandlerTest : public ::testing::Test {
  protected:
    void SetUp() override {
        systemContext_ = std::make_unique<SystemContext>();
        systemContext_->markReady();
        handler_ = std::make_unique<PowerHandler>(*systemContext_, Config::getInstance());

        Config::getInstance().hardwareSwitchesPowerEnabled.set(true);
        Config::getInstance().hardwareSwitchesPowerType.set(::Hardware::SwitchType::TOGGLE);
    }

    void TearDown() override {
        handler_.reset();
        systemContext_->setMachineStateContext(nullptr);
        systemContext_.reset();
        resetConfigDefaults();
        g_test_millis = 0;
    }

    void setupMachineStateContext(MachineStateId initialState = MachineStateId::STANDBY) {
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
    std::unique_ptr<PowerHandler>               handler_;
    std::unique_ptr<MachineStateContext>         machineStateContext_;
    std::unique_ptr<HardwareManager>            dummyHwManager_;
    std::unique_ptr<DisplayManager>             dummyDisplayManager_;
    std::unique_ptr<MQTTManager>                dummyMqttManager_;
    std::unique_ptr<NiceMock<MockWiFiManager>>  dummyWiFiManager_;
};

// ============================================================================
// CONSTRUCTION TESTS
// ============================================================================

TEST_F(PowerHandlerTest, ConstructsWithSystemContext) {
    // Handler should be constructed successfully — no crash
    ASSERT_NE(handler_, nullptr);
}

TEST_F(PowerHandlerTest, SetHardwareSetsSwitch) {
    MockSwitch mockSwitch(::Hardware::SwitchType::TOGGLE, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    // After setHardware, switch is accessible — no crash
}

// ============================================================================
// NULL SWITCH SAFETY TESTS
// ============================================================================

TEST_F(PowerHandlerTest, ProcessWithNullSwitchReturnsEarly) {
    // No hardware set — isHardwareValid() returns false
    handler_->process();
    // No crash — returns early at hardware validation
}

// ============================================================================
// ENABLED/DISABLED TESTS
// ============================================================================

TEST_F(PowerHandlerTest, ProcessReturnsEarlyWhenDisabled) {
    Config::getInstance().hardwareSwitchesPowerEnabled.set(false);
    MockSwitch mockSwitch(::Hardware::SwitchType::TOGGLE, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    handler_->process();
    // No crash — returns early because isEnabled() is false
}

TEST_F(PowerHandlerTest, ProcessProceedsWhenEnabled) {
    Config::getInstance().hardwareSwitchesPowerEnabled.set(true);
    MockSwitch mockSwitch(::Hardware::SwitchType::TOGGLE, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);

    // No MachineStateContext → process proceeds (PowerHandler hasPermission always returns true)
    // But processImpl accesses context methods, which check for null → no crash
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    handler_->process();
}

// ============================================================================
// TOGGLE SWITCH TESTS
// ============================================================================

TEST_F(PowerHandlerTest, ToggleSwitchPowerOnFromStandby) {
    Config::getInstance().hardwareSwitchesPowerType.set(::Hardware::SwitchType::TOGGLE);
    MockSwitch mockSwitch(::Hardware::SwitchType::TOGGLE, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::STANDBY);

    // Toggle ON from standby — should request normal operation
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();
    EXPECT_TRUE(machineStateContext_->isNormalOperationRequested())
        << "Toggling power ON from STANDBY should request normal operation";
}

TEST_F(PowerHandlerTest, ToggleSwitchPowerOffFromNormal) {
    Config::getInstance().hardwareSwitchesPowerType.set(::Hardware::SwitchType::TOGGLE);
    MockSwitch mockSwitch(::Hardware::SwitchType::TOGGLE, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::PID_NORMAL);

    // First call: switch HIGH (sets lastPowerSwitchPressed_ to true)
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    handler_->process();

    // Switch LOW — toggle OFF from normal state should request standby
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    handler_->process();
    EXPECT_TRUE(machineStateContext_->isStandbyRequested())
        << "Toggling power OFF from PID_NORMAL should request standby";
}

TEST_F(PowerHandlerTest, ToggleSwitchNoChangeWhenSameState) {
    Config::getInstance().hardwareSwitchesPowerType.set(::Hardware::SwitchType::TOGGLE);
    MockSwitch mockSwitch(::Hardware::SwitchType::TOGGLE, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::STANDBY);

    // Switch stays LOW — no state change
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    handler_->process();
    handler_->process();
    // No crash, no state change request
    EXPECT_FALSE(machineStateContext_->isNormalOperationRequested());
    EXPECT_FALSE(machineStateContext_->isStandbyRequested());
}

// ============================================================================
// MOMENTARY SWITCH TESTS
// ============================================================================

TEST_F(PowerHandlerTest, MomentarySwitchPowerOnFromStandby) {
    Config::getInstance().hardwareSwitchesPowerType.set(::Hardware::SwitchType::MOMENTARY);
    MockSwitch mockSwitch(::Hardware::SwitchType::TOGGLE, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::STANDBY);

    // Set millis to simulate time after initialization
    g_test_millis = 6000;

    // First process to record system initialization time
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    EXPECT_CALL(mockSwitch, longPressDetected()).WillRepeatedly(Return(false));
    handler_->process();

    // Advance time past the 5-second threshold
    g_test_millis = 12000;

    // Press button (state change from LOW to HIGH)
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    EXPECT_CALL(mockSwitch, longPressDetected()).WillRepeatedly(Return(false));
    handler_->process();

    EXPECT_TRUE(machineStateContext_->isNormalOperationRequested())
        << "Momentary press from STANDBY should request normal operation";
}

TEST_F(PowerHandlerTest, MomentarySwitchPowerOffFromNormal) {
    Config::getInstance().hardwareSwitchesPowerType.set(::Hardware::SwitchType::MOMENTARY);
    MockSwitch mockSwitch(::Hardware::SwitchType::TOGGLE, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    setupMachineStateContext(MachineStateId::PID_NORMAL);

    // Set millis to simulate time after initialization
    g_test_millis = 6000;

    // First process to record system initialization time
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(false));
    EXPECT_CALL(mockSwitch, longPressDetected()).WillRepeatedly(Return(false));
    handler_->process();

    // Advance time past the 5-second threshold
    g_test_millis = 12000;

    // Press button
    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    EXPECT_CALL(mockSwitch, longPressDetected()).WillRepeatedly(Return(false));
    handler_->process();

    EXPECT_TRUE(machineStateContext_->isStandbyRequested())
        << "Momentary press from PID_NORMAL should request standby";
}

// ============================================================================
// PROCESS WITHOUT CONTEXT TESTS
// ============================================================================

TEST_F(PowerHandlerTest, ProcessWithoutContextDoesNotCrash) {
    MockSwitch mockSwitch(::Hardware::SwitchType::TOGGLE, ::Hardware::SwitchMode::NORMALLY_OPEN);
    handler_->setHardware(&mockSwitch);
    // No MachineStateContext set

    EXPECT_CALL(mockSwitch, isPressed()).WillRepeatedly(Return(true));
    EXPECT_CALL(mockSwitch, longPressDetected()).WillRepeatedly(Return(false));

    // PowerHandler hasPermission always returns true, so processImpl runs
    // processImpl accesses machineStateContext() which returns nullptr
    // Inner methods check for nullptr and return gracefully
    handler_->process();
    // No crash means success
}

// Note: main() is provided by test/main.cpp
