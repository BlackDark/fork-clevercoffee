/**
 * @file CleverCoffeeWiFiManager.h
 * @brief RAII wrapper for WiFi management functionality
 */

#pragma once

#include <Arduino.h>
#include <functional>
#include <memory>

// Forward declarations
class WiFiManager;
class WiFiManagerParameter;

namespace CleverCoffee {
class NetworkCoordinator;
}

/**
 * @class CleverCoffeeWiFiManager
 * @brief RAII wrapper for WiFi connection and configuration management
 *
 * This class provides safe management of WiFi connections using RAII principles.
 * It encapsulates all WiFi setup, connection, and configuration portal functionality.
 */
class CleverCoffeeWiFiManager {
  public:
    /**
     * @brief Constructor - initializes WiFi manager
     * @param networkCoordinator Network coordinator for state tracking (optional)
     */
    explicit CleverCoffeeWiFiManager(CleverCoffee::NetworkCoordinator* networkCoordinator = nullptr);

    /**
     * @brief Destructor - automatically cleans up WiFi resources
     */
    ~CleverCoffeeWiFiManager();

    // Disable copy constructor and assignment operator
    CleverCoffeeWiFiManager(const CleverCoffeeWiFiManager&)            = delete;
    CleverCoffeeWiFiManager& operator=(const CleverCoffeeWiFiManager&) = delete;

    // Enable move constructor and assignment operator
    CleverCoffeeWiFiManager(CleverCoffeeWiFiManager&&)            = default;
    CleverCoffeeWiFiManager& operator=(CleverCoffeeWiFiManager&&) = default;

    /**
     * @brief Setup and connect to WiFi
     * @param hostname The device hostname
     * @param password The access point password
     * @param oledEnabled Whether OLED display is enabled for status messages
     * @param displayCallback Optional callback for display messages (line1, line2)
     * @return true if WiFi connected successfully, false if offline mode
     */
    bool setupAndConnect(const String&                                 hostname,
                         const String&                                 password,
                         bool                                          oledEnabled,
                         std::function<void(const char*, const char*)> displayCallback = nullptr);

    /**
     * @brief Reset WiFi settings and restart device
     */
    void resetSettings();

    /**
     * @brief Check if WiFi is connected
     * @return true if connected to WiFi
     */
    bool isConnected() const;

    /**
     * @brief Get the connected WiFi SSID
     * @return WiFi SSID string
     */
    String getSSID() const;

    /**
     * @brief Check if restart is required after AP configuration
     * @return true if restart is needed
     */
    bool requiresRestart() const noexcept {
        return restartAfterAP_;
    }

    /**
     * @brief Check WiFi connection and attempt reconnection if needed
     *
     * This method handles automatic WiFi reconnection logic including:
     * - Connection status monitoring
     * - Reconnection attempts with backoff
     * - Offline mode activation after max attempts
     * - Connection logging
     */
    void checkAndMaintainConnection();

    /**
     * @brief Get the current signal strength in dBm
     * @return Signal strength in dBm
     */
    int getSignalStrength();

  private:
    // ::WiFiManager wifiManager_; // Use global scope to avoid naming conflict
    std::unique_ptr<WiFiManager>          wifiManager_;
    std::unique_ptr<WiFiManagerParameter> customHostname_;
    bool                                  restartAfterAP_;
    CleverCoffee::NetworkCoordinator*     networkCoordinator_;

    /**
     * @brief Configure WiFi manager parameters
     */
    void configureWiFiManager(const String& hostname, const String& password);

    /**
     * @brief Attempt to connect to saved WiFi or start config portal
     * @param hostname The device hostname
     * @param password The access point password
     * @param displayCallback Optional callback for display messages
     * @return true if connected successfully
     */
    bool attemptConnection(const String&                                 hostname,
                           const String&                                 password,
                           std::function<void(const char*, const char*)> displayCallback);

    /**
     * @brief Handle successful WiFi connection
     * @param oledEnabled Whether to show OLED status messages
     * @param displayCallback Optional callback for display messages
     */
    void handleSuccessfulConnection(bool oledEnabled, std::function<void(const char*, const char*)> displayCallback);

    /**
     * @brief Handle WiFi connection failure
     * @param oledEnabled Whether to show OLED status messages
     * @param displayCallback Optional callback for display messages
     */
    void handleConnectionFailure(bool oledEnabled, std::function<void(const char*, const char*)> displayCallback);

    /**
     * @brief Update hostname from configuration portal
     */
    void updateHostnameFromPortal();
};
