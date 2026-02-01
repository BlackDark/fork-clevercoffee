/**
 * @file IWiFiManager.h
 * @brief Interface for WiFi management - enables testing with mock implementations
 */

#pragma once

#include <Arduino.h>
#include <functional>

/**
 * @class IWiFiManager
 * @brief Abstract interface for WiFi management operations
 *
 * This interface enables dependency injection and testing of components
 * that depend on WiFi functionality without requiring actual hardware.
 */
class IWiFiManager {
public:
    virtual ~IWiFiManager() = default;

    /**
     * @brief Setup and connect to WiFi
     * @param hostname The device hostname
     * @param password The access point password
     * @param oledEnabled Whether OLED display is enabled for status messages
     * @param displayCallback Optional callback for display messages (line1, line2)
     * @return true if WiFi connected successfully, false if offline mode
     */
    virtual bool setupAndConnect(
        const String& hostname,
        const String& password,
        bool oledEnabled,
        std::function<void(const char*, const char*)> displayCallback = nullptr) = 0;

    /**
     * @brief Reset WiFi settings and restart device
     */
    virtual void resetSettings() = 0;

    /**
     * @brief Check if WiFi is connected
     * @return true if connected to WiFi
     */
    virtual bool isConnected() const = 0;

    /**
     * @brief Get the connected WiFi SSID
     * @return WiFi SSID string
     */
    virtual String getSSID() const = 0;

    /**
     * @brief Check if restart is required after AP configuration
     * @return true if restart is needed
     */
    virtual bool requiresRestart() const noexcept = 0;

    /**
     * @brief Check WiFi connection and attempt reconnection if needed
     */
    virtual void checkAndMaintainConnection() = 0;

    /**
     * @brief Get the current signal strength in dBm
     * @return Signal strength in dBm
     */
    virtual int getSignalStrength() = 0;
};
