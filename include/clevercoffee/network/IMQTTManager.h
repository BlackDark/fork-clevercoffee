/**
 * @file IMQTTManager.h
 * @brief Interface for MQTT management - enables testing with mock implementations
 */

#pragma once

#include <functional>

// Forward declarations
namespace CleverCoffee {
class UICoordinator;
class SensorCoordinator;
class NetworkCoordinator;
class SystemContext;
} // namespace CleverCoffee

/**
 * @class IMQTTManager
 * @brief Abstract interface for MQTT management operations
 *
 * This interface enables dependency injection and testing of components
 * that depend on MQTT functionality without requiring actual network connections.
 */
class IMQTTManager {
  public:
    virtual ~IMQTTManager() = default;

    /**
     * @brief Check if MQTT is enabled
     * @return true if MQTT is enabled
     */
    virtual bool isEnabled() const noexcept = 0;

    /**
     * @brief Check if MQTT is connected
     * @return true if connected
     */
    virtual bool isConnected() const noexcept = 0;

    /**
     * @brief Check MQTT connection and reconnect if needed
     */
    virtual void checkConnection() = 0;

    /**
     * @brief Process MQTT loop and handle messages
     */
    virtual void loop() = 0;

    /**
     * @brief Publish system parameters to MQTT
     * @param continueOnError Whether to continue on errors
     * @return 0 on success, error code on failure
     */
    virtual int writeSysParamsToMQTT(bool continueOnError = true) = 0;

    /**
     * @brief Send Home Assistant discovery messages
     * @return 0 on success, error code on failure
     */
    virtual int sendHASSIODiscoveryMsg() = 0;

    /**
     * @brief Set UI coordinator for state management
     * @param coordinator Pointer to UICoordinator
     */
    virtual void setUICoordinator(CleverCoffee::UICoordinator* coordinator) noexcept = 0;

    /**
     * @brief Set Sensor coordinator for scale mode management
     * @param coordinator Pointer to SensorCoordinator
     */
    virtual void setSensorCoordinator(CleverCoffee::SensorCoordinator* coordinator) noexcept = 0;

    /**
     * @brief Set Network coordinator for connection state management
     * @param coordinator Pointer to NetworkCoordinator
     */
    virtual void setNetworkCoordinator(CleverCoffee::NetworkCoordinator* coordinator) noexcept = 0;

    /**
     * @brief Set system context for state management
     * @param context Pointer to SystemContext
     */
    virtual void setSystemContext(CleverCoffee::SystemContext* context) noexcept = 0;

    /**
     * @brief Set update running flag
     * @param running Whether update is running
     */
    virtual void setUpdateRunning(bool running) noexcept = 0;

    /**
     * @brief Check if update is running
     * @return true if update is running
     */
    virtual bool isUpdateRunning() const noexcept = 0;

    /**
     * @brief Check if was connected previously
     * @return true if was connected
     */
    virtual bool wasConnected() const noexcept = 0;

    /**
     * @brief Set was connected flag
     * @param connected Connection state
     */
    virtual void setWasConnected(bool connected) noexcept = 0;
};
