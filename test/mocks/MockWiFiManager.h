/**
 * @file MockWiFiManager.h
 * @brief GMock implementation of IWiFiManager for testing
 */

#pragma once

#include <gmock/gmock.h>
#include "clevercoffee/network/IWiFiManager.h"

/**
 * @class MockWiFiManager
 * @brief Mock implementation of IWiFiManager for unit testing
 *
 * Provides EXPECT_CALL() support for all WiFi operations.
 *
 * Example usage:
 * @code
 * MockWiFiManager mockWifi;
 * ON_CALL(mockWifi, isConnected()).WillByDefault(Return(true));
 * EXPECT_CALL(mockWifi, checkAndMaintainConnection()).Times(1);
 * @endcode
 */
class MockWiFiManager : public IWiFiManager {
public:
    MockWiFiManager() = default;
    ~MockWiFiManager() override = default;

    MOCK_METHOD(bool, setupAndConnect,
        (const String& hostname,
         const String& password,
         bool oledEnabled,
         std::function<void(const char*, const char*)> displayCallback),
        (override));

    MOCK_METHOD(void, resetSettings, (), (override));

    MOCK_METHOD(bool, isConnected, (), (const, override));

    MOCK_METHOD(String, getSSID, (), (const, override));

    MOCK_METHOD(bool, requiresRestart, (), (const, noexcept, override));

    MOCK_METHOD(void, checkAndMaintainConnection, (), (override));

    MOCK_METHOD(int, getSignalStrength, (), (override));
};

/**
 * @brief Create a NiceMock WiFiManager with sensible defaults
 *
 * Returns a mock that won't complain about unexpected calls and
 * has reasonable default return values configured.
 */
inline ::testing::NiceMock<MockWiFiManager> createDefaultMockWiFiManager() {
    ::testing::NiceMock<MockWiFiManager> mock;
    ON_CALL(mock, isConnected()).WillByDefault(::testing::Return(true));
    ON_CALL(mock, getSSID()).WillByDefault(::testing::Return(String("TestNetwork")));
    ON_CALL(mock, requiresRestart()).WillByDefault(::testing::Return(false));
    ON_CALL(mock, getSignalStrength()).WillByDefault(::testing::Return(-50));
    ON_CALL(mock, setupAndConnect(::testing::_, ::testing::_, ::testing::_, ::testing::_))
        .WillByDefault(::testing::Return(true));
    return mock;
}
