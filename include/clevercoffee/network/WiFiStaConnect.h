/**
 * @file WiFiStaConnect.h
 * @brief Apply STA hostname before WiFi.begin so DHCP client ID matches config
 */

#pragma once

#include <Arduino.h>
#include <WiFi.h>

namespace CleverCoffee::Network {

inline void applyStaHostname(const String& hostname) {
    if (hostname.isEmpty()) {
        return;
    }
    WiFi.setHostname(hostname.c_str());
}

/**
 * @brief Set DHCP/STA hostname then start connection to an explicit SSID
 */
inline void beginStaWithHostname(const String& hostname, const String& ssid, const String& password) {
    applyStaHostname(hostname);
    if (password.isEmpty()) {
        WiFi.begin(ssid.c_str());
    } else {
        WiFi.begin(ssid.c_str(), password.c_str());
    }
}

/**
 * @brief Reconnect using saved credentials, re-applying hostname first
 */
inline void reconnectStaWithHostname(const String& hostname) {
    WiFi.disconnect();
    applyStaHostname(hostname);
    WiFi.begin();
}

} // namespace CleverCoffee::Network
