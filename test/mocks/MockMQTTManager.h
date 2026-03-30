/**
 * @file MockMQTTManager.h
 * @brief Mock implementation of IMQTTManager for testing
 */

#pragma once

#include <gmock/gmock.h>
#include "clevercoffee/network/IMQTTManager.h"

/**
 * @class MockMQTTManager
 * @brief Google Mock implementation of IMQTTManager interface
 *
 * Provides a test double for MQTTManager that can be used with
 * GoogleTest expectations and behavior specifications.
 */
class MockMQTTManager : public IMQTTManager {
  public:
    MockMQTTManager()          = default;
    ~MockMQTTManager() override = default;

    MOCK_METHOD(bool, isEnabled, (), (const, noexcept, override));
    MOCK_METHOD(bool, isConnected, (), (const, noexcept, override));
    MOCK_METHOD(void, checkConnection, (), (override));
    MOCK_METHOD(void, loop, (), (override));
    MOCK_METHOD(int, writeSysParamsToMQTT, (bool), (override));
    MOCK_METHOD(int, sendHASSIODiscoveryMsg, (), (override));
    MOCK_METHOD(void, setUICoordinator, (CleverCoffee::UICoordinator*), (noexcept, override));
    MOCK_METHOD(void, setSensorCoordinator, (CleverCoffee::SensorCoordinator*), (noexcept, override));
    MOCK_METHOD(void, setNetworkCoordinator, (CleverCoffee::NetworkCoordinator*), (noexcept, override));
    MOCK_METHOD(void, setSystemContext, (CleverCoffee::SystemContext*), (noexcept, override));
    MOCK_METHOD(void, setUpdateRunning, (bool), (noexcept, override));
    MOCK_METHOD(bool, isUpdateRunning, (), (const, noexcept, override));
    MOCK_METHOD(bool, wasConnected, (), (const, noexcept, override));
    MOCK_METHOD(void, setWasConnected, (bool), (noexcept, override));
};

/**
 * @brief Create a MockMQTTManager with default behavior for common scenarios
 * @return Unique pointer to NiceMock<MockMQTTManager> with sensible defaults
 *
 * Default behavior:
 * - isEnabled() returns false
 * - isConnected() returns false
 * - isUpdateRunning() returns false
 * - wasConnected() returns false
 */
inline std::unique_ptr<testing::NiceMock<MockMQTTManager>> createDefaultMockMQTTManager() {
    auto mock = std::make_unique<testing::NiceMock<MockMQTTManager>>();
    ON_CALL(*mock, isEnabled()).WillByDefault(testing::Return(false));
    ON_CALL(*mock, isConnected()).WillByDefault(testing::Return(false));
    ON_CALL(*mock, isUpdateRunning()).WillByDefault(testing::Return(false));
    ON_CALL(*mock, wasConnected()).WillByDefault(testing::Return(false));
    return mock;
}
