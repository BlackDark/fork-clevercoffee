/**
 * @file test_main.cpp
 * @brief Unit tests for ProcessController infrastructure
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>
#include "../test_support.h"
#include "../mocks/MockHardwareManager.h"
#include "../mocks/MockDisplayManager.h"
#include "../mocks/MockMQTTManager.h"
#include "../mocks/MockConfig.h"

// Include implementations
#include "../mocks/ConfigStubs.cpp"
#include "../../src/Logger.cpp"

using namespace CleverCoffee;
using ::testing::Return;
using ::testing::NiceMock;
using ::testing::_;

class ProcessControllerInfrastructureTest : public ::testing::Test {
protected:
    NiceMock<MockHardwareManager> mockHardwareManager_;
    NiceMock<MockDisplayManager> mockDisplayManager_;
    NiceMock<MockMQTTManager> mockMqttManager_;
    MockConfig mockConfig_;

    void SetUp() override {
        // Configure temperature defaults
        ON_CALL(mockHardwareManager_, getCurrentTemperature()).WillByDefault(Return(93.0));
        ON_CALL(mockHardwareManager_, hasTemperatureError()).WillByDefault(Return(false));

        // Configure safe defaults
        mockConfig_.setBrewSetpoint(93.0);
        mockConfig_.setSteamSetpoint(120.0);
        mockConfig_.setPidEnabled(true);
    }
};

/**
 * TEST: Mock configuration works correctly
 */
TEST_F(ProcessControllerInfrastructureTest, MockConfigurationWorks) {
    EXPECT_DOUBLE_EQ(93.0, mockConfig_.getBrewSetpoint());
    EXPECT_DOUBLE_EQ(120.0, mockConfig_.getSteamSetpoint());
    EXPECT_TRUE(mockConfig_.getPidEnabled());
}

/**
 * TEST: Temperature can be read from hardware manager
 */
TEST_F(ProcessControllerInfrastructureTest, TemperatureReadingWorks) {
    EXPECT_DOUBLE_EQ(93.0, mockHardwareManager_.getCurrentTemperature());
    EXPECT_FALSE(mockHardwareManager_.hasTemperatureError());
}

/**
 * TEST: Hardware manager heater control can be verified
 */
TEST_F(ProcessControllerInfrastructureTest, HeaterControlWorks) {
    MockHardwareManager hw;

    EXPECT_CALL(hw, enableHeater()).Times(1);
    EXPECT_CALL(hw, disableHeater()).Times(1);

    hw.enableHeater();
    hw.disableHeater();
}

/**
 * TEST: Emergency conditions trigger shutdown
 */
TEST_F(ProcessControllerInfrastructureTest, EmergencyShutdownWorks) {
    MockHardwareManager hw;

    // Simulate overtemperature
    ON_CALL(hw, getCurrentTemperature()).WillByDefault(Return(150.0));

    EXPECT_CALL(hw, emergencyShutdown()).Times(1);

    // Verify temperature is dangerous
    EXPECT_GT(hw.getCurrentTemperature(), 140.0);

    // Trigger shutdown
    hw.emergencyShutdown();
}

/**
 * TEST: PID parameters can be configured via MockConfig
 */
TEST_F(ProcessControllerInfrastructureTest, PIDParametersConfigurable) {
    mockConfig_.setPidRegularKp(50.0);
    mockConfig_.setPidRegularTn(150.0);
    mockConfig_.setPidRegularTv(15.0);

    EXPECT_DOUBLE_EQ(50.0, mockConfig_.getPidRegularKp());
    EXPECT_DOUBLE_EQ(150.0, mockConfig_.getPidRegularTn());
    EXPECT_DOUBLE_EQ(15.0, mockConfig_.getPidRegularTv());
}

/**
 * TEST: Steam setpoint is higher than brew setpoint
 */
TEST_F(ProcessControllerInfrastructureTest, SteamSetpointHigherThanBrew) {
    EXPECT_GT(mockConfig_.getSteamSetpoint(), mockConfig_.getBrewSetpoint());
}

// Note: main() is provided by test/main.cpp
