/**
 * @file WiFiManager.cpp
 * @brief Implementation of RAII wrapper for WiFi management
 */

#include "WiFiManager.h"
#include "../Config.h"
#include "Logger.h"
#include <ESP.h>
#include <WiFi.h>

// Forward declaration for helper function
extern String number2string(int value);

CleverCoffeeWiFiManager::CleverCoffeeWiFiManager() :
    customHostname_(nullptr), restartAfterAP_(false) {

    // Create custom hostname parameter
    const String hostname = Config::getInstance().get<String>("system.hostname");
    customHostname_ = new WiFiManagerParameter("hostname", "Hostname", hostname.c_str(), 30);
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
    wifiManager_.addParameter(customHostname_);
    wifiManager_.setCleanConnect(true);
    wifiManager_.setConnectTimeout(10); // using 10s to connect to WLAN, 5s is sometimes too short!
    wifiManager_.setBreakAfterConfig(true);
    wifiManager_.setConnectRetries(3);
    wifiManager_.setHostname(hostname.c_str());
}

bool CleverCoffeeWiFiManager::attemptConnection(const String& hostname, const String& password, std::function<void(const char*, const char*)> displayCallback) {
    if (wifiManager_.getWiFiIsSaved()) {
        LOG(INFO, "Connecting to WiFi");
    }

    wifiManager_.setEnableConfigPortal(false); // doesn't start config portal within autoconnect
    wifiManager_.setDisableConfigPortal(true); // disables config portal on wifi save
    bool wifiConnected = wifiManager_.autoConnect(hostname.c_str(), password.c_str());

    if (!wifiConnected) {
        wifiManager_.setConfigPortalTimeout(1);  // prompt config portal to update password
        wifiConnected = wifiManager_.startConfigPortal(hostname.c_str(), password.c_str());
        wifiManager_.setConfigPortalTimeout(60); // sec timeout for captive portal

        if (Config::getInstance().get<bool>("hardware.oled.enabled") && displayCallback) {
            displayCallback("Starting Portal AP", hostname.c_str());
        }

        wifiConnected = wifiManager_.startConfigPortal(hostname.c_str(), password.c_str());
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
    const String macaddr0 = number2string(mac[0]);
    const String macaddr1 = number2string(mac[1]);
    const String macaddr2 = number2string(mac[2]);
    const String macaddr3 = number2string(mac[3]);
    const String macaddr4 = number2string(mac[4]);
    const String macaddr5 = number2string(mac[5]);
    const String completemac = macaddr0 + macaddr1 + macaddr2 + macaddr3 + macaddr4 + macaddr5;

    LOGF(DEBUG, "MAC-ADDRESS: %s", completemac.c_str());

    if (oledEnabled && displayCallback) {
        displayCallback("WiFi Connected", wifiManager_.getWiFiSSID(true).c_str());
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

    wifiManager_.disconnect();
    delay(1000);
}

void CleverCoffeeWiFiManager::updateHostnameFromPortal() {
    // Read hostname from portal and store in config
    String newHostname = String(customHostname_->getValue());
    const String currentHostname = Config::getInstance().get<String>("system.hostname");

    if (newHostname.length() > 0 && newHostname != currentHostname) {
        // Update the config system - this will automatically save to NVS
        Config::getInstance().set<String>("system.hostname", newHostname);
        LOG(INFO, "Hostname updated from configuration portal");
    }
}

void CleverCoffeeWiFiManager::resetSettings() {
    wifiManager_.resetSettings();
    delay(500);
    ESP.restart();
}

bool CleverCoffeeWiFiManager::isConnected() const {
    return WiFi.status() == WL_CONNECTED;
}

String CleverCoffeeWiFiManager::getSSID() const {
    return const_cast<::WiFiManager&>(wifiManager_).getWiFiSSID(true);
}