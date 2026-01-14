/**
 * @file MockMQTTManager.h
 * @brief Mock implementation of MQTTManager for testing
 *
 * Allows testing code that depends on MQTTManager without actual network connections.
 */

#pragma once

#include <gmock/gmock.h>
#include <String.h>
#include <PubSubClient.h>

/**
 * @class MockMQTTManager
 * @brief Mock implementation of MQTTManager for testing
 *
 * Provides EXPECT_CALL() support for MQTT operations.
 * 
 * Example usage:
 * @code
 * MockMQTTManager mockMQTT;
 * EXPECT_CALL(mockMQTT, setup(_)).WillOnce(Return(true));
 * EXPECT_CALL(mockMQTT, isEnabled()).WillRepeatedly(Return(true));
 * EXPECT_CALL(mockMQTT, isConnected()).WillRepeatedly(Return(true));
 * EXPECT_CALL(mockMQTT, loop()).Times(AtLeast(1));
 * @endcode
 */
class MockMQTTManager {
public:
    MockMQTTManager() = default;
    virtual ~MockMQTTManager() = default;

    /**
     * @brief Mock method to setup MQTT
     */
    MOCK_METHOD(bool, setup, (const String&), ());

    /**
     * @brief Mock method to check MQTT connection
     */
    MOCK_METHOD(void, checkConnection, (), ());

    /**
     * @brief Mock method to process MQTT loop
     */
    MOCK_METHOD(void, loop, (), ());

    /**
     * @brief Mock method to publish system parameters
     */
    MOCK_METHOD(int, writeSysParamsToMQTT, (bool), ());

    /**
     * @brief Mock method to send Home Assistant discovery
     */
    MOCK_METHOD(int, sendHASSIODiscoveryMsg, (), ());

    /**
     * @brief Mock method to check if MQTT is enabled
     */
    MOCK_METHOD(bool, isEnabled, (), (const, noexcept));

    /**
     * @brief Mock method to check if MQTT is connected
     */
    MOCK_METHOD(bool, isConnected, (), (const, noexcept));

    /**
     * @brief Mock method to register parameter
     */
    MOCK_METHOD(void, registerParameter, (const char*, const char*), ());

    /**
     * @brief Mock method to register sensor
     */
    MOCK_METHOD(void, registerSensor, (const char*, std::function<double()>), ());

    /**
     * @brief Mock method to set update running flag
     */
    MOCK_METHOD(void, setUpdateRunning, (bool), (noexcept));

    /**
     * @brief Mock method to check if update is running
     */
    MOCK_METHOD(bool, isUpdateRunning, (), (const, noexcept));

    /**
     * @brief Mock method to check if was connected
     */
    MOCK_METHOD(bool, wasConnected, (), (const, noexcept));

    /**
     * @brief Mock method to set was connected flag
     */
    MOCK_METHOD(void, setWasConnected, (bool), (noexcept));

    /**
     * @brief Mock method to get MQTT client
     */
    MOCK_METHOD(PubSubClient&, getClient, (), (noexcept));

    /**
     * @brief Mock method to set UI coordinator
     */
    MOCK_METHOD(void, setUICoordinator, (void*), (noexcept));

    /**
     * @brief Mock method to set sensor coordinator
     */
    MOCK_METHOD(void, setSensorCoordinator, (void*), (noexcept));

    /**
     * @brief Mock method to set network coordinator
     */
    MOCK_METHOD(void, setNetworkCoordinator, (void*), (noexcept));

    /**
     * @brief Mock method to set system context
     */
    MOCK_METHOD(void, setSystemContext, (void*), (noexcept));
};
