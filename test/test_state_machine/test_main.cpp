/**
 * @file test_main.cpp
 * @brief Unit tests for StateMachine
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>
#include "../test_support.h"
#include "../mocks/MockHardwareManager.h"
#include "../mocks/MockDisplayManager.h"
#include "../mocks/MockMQTTManager.h"
#include "../mocks/MockWiFiManager.h"

// Include implementations
#include "../mocks/ConfigStubs.cpp"
#include "../../src/Logger.cpp"

// For now, test the mock infrastructure works
// Full StateMachine tests require additional setup

using namespace CleverCoffee;
using ::testing::Return;
using ::testing::NiceMock;
using ::testing::_;

class StateMachineInfrastructureTest : public ::testing::Test {
protected:
    NiceMock<MockHardwareManager> mockHardwareManager_;
    NiceMock<MockDisplayManager> mockDisplayManager_;
    NiceMock<MockWiFiManager> mockWiFiManager_;
    NiceMock<MockMQTTManager> mockMqttManager_;

    void SetUp() override {
        // Configure default mock behaviors
        ON_CALL(mockWiFiManager_, isConnected()).WillByDefault(Return(true));
        ON_CALL(mockWiFiManager_, requiresRestart()).WillByDefault(Return(false));
        ON_CALL(mockHardwareManager_, getCurrentTemperature()).WillByDefault(Return(25.0));
        ON_CALL(mockHardwareManager_, hasTemperatureError()).WillByDefault(Return(false));
    }
};

/**
 * TEST: Mock infrastructure is properly configured
 */
TEST_F(StateMachineInfrastructureTest, MockWiFiManagerWorks) {
    EXPECT_TRUE(mockWiFiManager_.isConnected());
    EXPECT_FALSE(mockWiFiManager_.requiresRestart());
}

/**
 * TEST: MockHardwareManager provides temperature
 */
TEST_F(StateMachineInfrastructureTest, MockHardwareManagerWorks) {
    EXPECT_DOUBLE_EQ(25.0, mockHardwareManager_.getCurrentTemperature());
    EXPECT_FALSE(mockHardwareManager_.hasTemperatureError());
}

/**
 * TEST: WiFi manager mock can be configured for different scenarios
 */
TEST_F(StateMachineInfrastructureTest, WiFiManagerCanBeConfiguredForOffline) {
    NiceMock<MockWiFiManager> offlineWifi;
    ON_CALL(offlineWifi, isConnected()).WillByDefault(Return(false));

    EXPECT_FALSE(offlineWifi.isConnected());
}

/**
 * TEST: WiFi manager mock tracks method calls
 */
TEST_F(StateMachineInfrastructureTest, WiFiManagerTracksConnectionChecks) {
    MockWiFiManager strictWifi;
    ON_CALL(strictWifi, isConnected()).WillByDefault(Return(true));

    EXPECT_CALL(strictWifi, checkAndMaintainConnection()).Times(2);

    strictWifi.checkAndMaintainConnection();
    strictWifi.checkAndMaintainConnection();
}

/**
 * TEST: Hardware manager emergency shutdown can be verified
 */
TEST_F(StateMachineInfrastructureTest, HardwareManagerEmergencyShutdown) {
    MockHardwareManager hw;
    EXPECT_CALL(hw, emergencyShutdown()).Times(1);

    hw.emergencyShutdown();
}

// Note: main() is provided by test/main.cpp
