/**
 * @file CleverCoffeeWiFiManager.cpp
 * @brief Implementation of RAII wrapper for WiFi management
 */

#include "CleverCoffeeWiFiManager.h"
#include "../Config.h"
#include "../display/languages.h"
#include "../utils/brewUtils.h"
#include "Logger.h"
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

CleverCoffeeWiFiManager::CleverCoffeeWiFiManager() :
    wifiManager_(std::make_unique<WiFiManager>()), restartAfterAP_(false) {

    // Create custom hostname parameter
    const String hostname = Config::getInstance().systemHostname.get();
    customHostname_ = std::make_unique<WiFiManagerParameter>("hostname", "Hostname", hostname.c_str(), 30);
}

CleverCoffeeWiFiManager::~CleverCoffeeWiFiManager() {
    // Destructor implementation - unique_ptr automatically cleans up WiFiManager and customHostname_
    // This needs to be defined in the .cpp file where WiFiManager is fully defined
}

bool CleverCoffeeWiFiManager::setupAndConnect(const String& hostname, const String& password, bool oledEnabled, std::function<void(const char*, const char*)> displayCallback) {
    configureWiFiManager(hostname, password);

    if (attemptConnection(hostname, password, displayCallback)) {
        handleSuccessfulConnection(oledEnabled, displayCallback);
        return true;
    }
    else {
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

bool CleverCoffeeWiFiManager::attemptConnection(const String& hostname, const String& password, std::function<void(const char*, const char*)> displayCallback) {
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

void CleverCoffeeWiFiManager::handleSuccessfulConnection(bool oledEnabled, std::function<void(const char*, const char*)> displayCallback) {
    LOGF(INFO, "WiFi connected - IP = %i.%i.%i.%i", WiFi.localIP()[0], WiFi.localIP()[1], WiFi.localIP()[2], WiFi.localIP()[3]);

    byte mac[6];
    WiFi.macAddress(mac);
    char completemac[18]; // XX:XX:XX:XX:XX:XX + null terminator
    snprintf(completemac, sizeof(completemac), "%02X%02X%02X%02X%02X%02X", 
             mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

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

void CleverCoffeeWiFiManager::handleConnectionFailure(bool oledEnabled, std::function<void(const char*, const char*)> displayCallback) {
    LOG(INFO, "WiFi connection timed out...");

    if (oledEnabled && displayCallback) {
        displayCallback("No WiFi", "Offline Mode");
    }

    wifiManager_->disconnect();
    delay(1000);
}

void CleverCoffeeWiFiManager::updateHostnameFromPortal() {
    // Read hostname from portal and store in config
    String newHostname = String(customHostname_->getValue());
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
    static int connectionAttemptCounter = 1;
    static bool wifiConnectedHandled = false;

    // Don't attempt reconnection if in offline mode or brewing is active
    if (g_state.network.offlineMode || checkBrewActive()) return;

    // Try to connect and if it does not succeed, enter offline mode
    if ((millis() - g_state.network.lastWifiConnectionAttempt >= wifiConnectionDelay) && (g_state.network.wifiReconnects <= maxWifiReconnects)) {

        if (WiFi.status() != WL_CONNECTED) { // check WiFi connection status
            wifiConnectedHandled = false;

            if (connectionAttemptCounter == 1) {
                g_state.network.wifiReconnects++;
                LOGF(INFO, "Attempting WIFI (re-)connection: %i", g_state.network.wifiReconnects);
                WiFi.disconnect();
                WiFi.begin();
            }

            delay(20);                      // give WIFI some time to connect

            if (WiFi.status() != WL_CONNECTED && connectionAttemptCounter < 100) {
                connectionAttemptCounter++; // reconnect counter, maximum waiting time = 20*100ms plus loop times
            }
            else {
                if (connectionAttemptCounter == 100) {
                    LOGF(INFO, "Wifi Reconnection failed - %i loops", connectionAttemptCounter);
                    g_state.network.lastWifiConnectionAttempt = millis();
                    connectionAttemptCounter = 1;
                }
            }
        }
        else {
            if (wifiConnectedHandled == false) {
                LOGF(INFO, "Wifi Reconnected - %i loops", connectionAttemptCounter);
                wifiConnectedHandled = true;
                connectionAttemptCounter = 1;
            }
        }
    }

    // Enter offline mode if maximum reconnection attempts reached
    if (g_state.network.wifiReconnects >= maxWifiReconnects && WiFi.status() != WL_CONNECTED) {
        // no wifi connection after trying connection, initiate offline mode
        g_state.network.offlineMode = true;
        LOG(INFO, "Entered offline mode after maximum WiFi reconnection attempts");
    }
    else {
        if (WiFi.status() == WL_CONNECTED) {
            g_state.network.wifiReconnects = 0;
        }
    }
}

int CleverCoffeeWiFiManager::getSignalStrength() {
    if (g_state.network.offlineMode) return 0;

    long rssi;

    if (WiFi.status() == WL_CONNECTED) {
        rssi = WiFi.RSSI();
    }
    else {
        rssi = -100;
    }

    if (rssi >= -50) {
        return 4;
    }
    else if (rssi < -50 && rssi >= -65) {
        return 3;
    }
    else if (rssi < -65 && rssi >= -75) {
        return 2;
    }
    else if (rssi < -75 && rssi >= -80) {
        return 1;
    }

    return 0;
}
