/**
 * @file CleverCoffeeWiFiManager.cpp
 * @brief Implementation of RAII wrapper for WiFi management
 */

#include "clevercoffee/network/CleverCoffeeWiFiManager.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/coordinators/NetworkCoordinator.h"
#include "clevercoffee/display/languages.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/utils/Resilience.h"

#include <ESP.h>
#include <WiFi.h>
#include <WiFiManager.h>

// Helper function to convert byte to hex string
static String byteToHex(byte value) {
    String result = String(value, HEX);
    if (result.length() == 1) {
        result = "0" + result;
    }
    result.toUpperCase();
    return result;
}

CleverCoffeeWiFiManager::CleverCoffeeWiFiManager(CleverCoffee::NetworkCoordinator* networkCoordinator)
    : wifiManager_(std::make_unique<WiFiManager>()), restartAfterAP_(false), networkCoordinator_(networkCoordinator) {
    // Create custom hostname parameter
    const String hostname = Config::getInstance().systemHostname.get();
    customHostname_       = std::make_unique<WiFiManagerParameter>("hostname", "Hostname", hostname.c_str(), 30);

    // Initialize retry policy with exponential backoff: 10s initial, 5min max, 2x multiplier, 5 max attempts
    retryPolicy_ = std::make_unique<CleverCoffee::Utils::RetryPolicy>(10000,  // Initial delay: 10 seconds
                                                                      300000, // Max delay: 5 minutes
                                                                      2.0,    // Backoff multiplier: 2x
                                                                      5 // Max attempts: 5 (matches maxWifiReconnects)
    );

    // Initialize circuit breaker: 5 failures, 60s open timeout, 30s half-open timeout
    circuitBreaker_ = std::make_unique<CleverCoffee::Utils::CircuitBreaker>(5,     // Failure threshold: 5 failures
                                                                            60000, // Open timeout: 60 seconds
                                                                            30000  // Half-open timeout: 30 seconds
    );
}

CleverCoffeeWiFiManager::~CleverCoffeeWiFiManager() {
    // Destructor implementation - unique_ptr automatically cleans up WiFiManager and customHostname_
    // This needs to be defined in the .cpp file where WiFiManager is fully defined
}

bool CleverCoffeeWiFiManager::setupAndConnect(const String&                                 hostname,
                                              const String&                                 password,
                                              bool                                          oledEnabled,
                                              std::function<void(const char*, const char*)> displayCallback) {
    configureWiFiManager(hostname, password);

    if (attemptConnection(hostname, password, displayCallback)) {
        handleSuccessfulConnection(oledEnabled, displayCallback);
        return true;
    } else {
        handleConnectionFailure(oledEnabled, displayCallback);
        return false;
    }
}

void CleverCoffeeWiFiManager::configureWiFiManager(const String& hostname, const String& password) {
    wifiManager_->addParameter(customHostname_.get());
    wifiManager_->setCleanConnect(true);
    wifiManager_->setConnectTimeout(10); // using 10s to connect to WLAN, 5s is sometimes too short!
    wifiManager_->setBreakAfterConfig(true);
    wifiManager_->setConnectRetries(3);
    wifiManager_->setHostname(hostname.c_str());
}

bool CleverCoffeeWiFiManager::attemptConnection(const String&                                 hostname,
                                                const String&                                 password,
                                                std::function<void(const char*, const char*)> displayCallback) {
    if (wifiManager_->getWiFiIsSaved()) {
        LOG(INFO, "Connecting to WiFi");
    }

    wifiManager_->setEnableConfigPortal(false); // doesn't start config portal within autoconnect
    wifiManager_->setDisableConfigPortal(true); // disables config portal on wifi save
    bool wifiConnected = wifiManager_->autoConnect(hostname.c_str(), password.c_str());

    if (!wifiConnected) {
        wifiManager_->setConfigPortalTimeout(1);  // prompt config portal to update password
        wifiConnected = wifiManager_->startConfigPortal(hostname.c_str(), password.c_str());
        wifiManager_->setConfigPortalTimeout(60); // sec timeout for captive portal

        if (Config::getInstance().hardwareOledEnabled.get() && displayCallback) {
            displayCallback("Starting Portal AP", hostname.c_str());
        }

        wifiConnected = wifiManager_->startConfigPortal(hostname.c_str(), password.c_str());
        if (wifiConnected) {
            restartAfterAP_ = true;
            updateHostnameFromPortal();
        }
    }

    return wifiConnected;
}

void CleverCoffeeWiFiManager::handleSuccessfulConnection(
    bool oledEnabled, std::function<void(const char*, const char*)> displayCallback) {
    LOGF(INFO,
         "WiFi connected - IP = %i.%i.%i.%i",
         WiFi.localIP()[0],
         WiFi.localIP()[1],
         WiFi.localIP()[2],
         WiFi.localIP()[3]);

    byte mac[6];
    WiFi.macAddress(mac);
    char completemac[18]; // XX:XX:XX:XX:XX:XX + null terminator
    snprintf(
        completemac, sizeof(completemac), "%02X%02X%02X%02X%02X%02X", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

    LOGF(DEBUG, "MAC-ADDRESS: %s", completemac);

    if (oledEnabled && displayCallback) {
        displayCallback(langstring_connectwifi1, wifiManager_->getWiFiSSID(true).c_str());
    }

    if (restartAfterAP_) {
        LOG(INFO, "Restarting after successful Wifi configuration");
        delay(1000);
        ESP.restart();
    }
}

void CleverCoffeeWiFiManager::handleConnectionFailure(bool                                          oledEnabled,
                                                      std::function<void(const char*, const char*)> displayCallback) {
    LOG(INFO, "WiFi connection timed out...");

    if (oledEnabled && displayCallback) {
        displayCallback("No WiFi", "Offline Mode");
    }

    wifiManager_->disconnect();
    delay(1000);
}

void CleverCoffeeWiFiManager::updateHostnameFromPortal() {
    // Read hostname from portal and store in config
    String       newHostname     = String(customHostname_->getValue());
    const String currentHostname = Config::getInstance().systemHostname.get();

    if (newHostname.length() > 0 && newHostname != currentHostname) {
        // Update the config system - this will automatically save to NVS
        Config::getInstance().systemHostname.set(newHostname);
        LOG(INFO, "Hostname updated from configuration portal");
    }
}

void CleverCoffeeWiFiManager::resetSettings() {
    wifiManager_->resetSettings();
    delay(500);
    ESP.restart();
}

bool CleverCoffeeWiFiManager::isConnected() const {
    return WiFi.status() == WL_CONNECTED;
}

String CleverCoffeeWiFiManager::getSSID() const {
    return wifiManager_->getWiFiSSID(true);
}

void CleverCoffeeWiFiManager::checkAndMaintainConnection() {
    static bool wifiConnectedHandled = false;

    // Check offline mode from NetworkCoordinator
    if (!networkCoordinator_) {
        LOG(ERROR, "CleverCoffeeWiFiManager: NetworkCoordinator is null");
        return;
    }

    bool isOfflineMode = networkCoordinator_->isOfflineMode();

    // Don't attempt reconnection if in offline mode
    if (isOfflineMode) return;

    const unsigned long currentTime = millis();

    // Check circuit breaker - fail fast if circuit is open
    if (!circuitBreaker_->canAttempt(currentTime)) {
        if (!wifiConnectedHandled) {
            LOGF(WARNING, "WiFi circuit breaker OPEN - skipping reconnection attempt (fail fast)");
            wifiConnectedHandled = true;
        }
        return;
    }

    // Check if WiFi is connected
    if (WiFi.status() == WL_CONNECTED) {
        // WiFi is connected - record success and reset retry policy
        if (!wifiConnectedHandled) {
            LOGF(INFO, "WiFi connected successfully (attempt %u)", retryPolicy_->getCurrentAttempt() + 1);
            wifiConnectedHandled = true;
        }
        circuitBreaker_->recordSuccess(currentTime);
        retryPolicy_->reset();
        networkCoordinator_->resetWifiReconnects();
        networkCoordinator_->setWifiConnected(true);
        return;
    }

    // WiFi is not connected - check if we should retry
    if (!retryPolicy_->shouldRetry()) {
        // Max attempts reached - enter offline mode
        if (!isOfflineMode) {
            networkCoordinator_->setOfflineMode(true);
            LOGF(WARNING,
                 "WiFi max reconnection attempts reached (%u) - entering offline mode",
                 retryPolicy_->getCurrentAttempt());
        }
        return;
    }

    // Check if enough time has passed for next retry (exponential backoff)
    if (!retryPolicy_->canRetryNow(currentTime)) {
        return; // Wait for backoff period
    }

    // Attempt reconnection
    wifiConnectedHandled = false;
    retryPolicy_->incrementAttempt(currentTime);
    networkCoordinator_->incrementWifiReconnects();
    networkCoordinator_->setLastWifiConnectionAttempt(currentTime);

    LOGF(INFO,
         "Attempting WiFi reconnection: attempt %u/%u (delay: %lums)",
         retryPolicy_->getCurrentAttempt(),
         retryPolicy_->isMaxAttemptsReached() ? retryPolicy_->getCurrentAttempt() : 0,
         retryPolicy_->getNextDelay());

    WiFi.disconnect();
    WiFi.begin();

    // Use yield() instead of delay() to avoid blocking other tasks
    yield();

    // Check connection status after brief delay
    // Note: WiFi.begin() is asynchronous, so we check status in next loop iteration
    const unsigned int attemptNumber = retryPolicy_->getCurrentAttempt();
    if (WiFi.status() == WL_CONNECTED) {
        // Connection successful
        circuitBreaker_->recordSuccess(currentTime);
        retryPolicy_->reset();
        networkCoordinator_->resetWifiReconnects();
        networkCoordinator_->setWifiConnected(true);
        LOGF(INFO, "WiFi reconnected successfully on attempt %u", attemptNumber);
    } else {
        // Connection failed - record failure
        circuitBreaker_->recordFailure(currentTime);
        LOGF(DEBUG,
             "WiFi reconnection attempt %u failed, next retry in %lums",
             retryPolicy_->getCurrentAttempt(),
             retryPolicy_->getNextDelay());
    }

    // Enter offline mode if circuit breaker is open and max attempts reached
    if (circuitBreaker_->isOpen() && retryPolicy_->isMaxAttemptsReached()) {
        if (!isOfflineMode) {
            networkCoordinator_->setOfflineMode(true);
            LOG(WARNING, "WiFi circuit breaker OPEN and max attempts reached - entering offline mode");
        }
    }
}

int CleverCoffeeWiFiManager::getSignalStrength() {
    // Check offline mode from NetworkCoordinator
    if (!networkCoordinator_) return 0;
    if (networkCoordinator_->isOfflineMode()) return 0;

    long rssi;

    if (WiFi.status() == WL_CONNECTED) {
        rssi = WiFi.RSSI();
    } else {
        rssi = -100;
    }

    if (rssi >= -50) {
        return 4;
    } else if (rssi < -50 && rssi >= -65) {
        return 3;
    } else if (rssi < -65 && rssi >= -75) {
        return 2;
    } else if (rssi < -75 && rssi >= -80) {
        return 1;
    }

    return 0;
}
