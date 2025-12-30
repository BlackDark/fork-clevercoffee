#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/network/MQTTManager.h"

#include <mutex>

/**
 * @brief System utility functions to replace legacy tight-coupled functions
 * These will be gradually moved to more appropriate locations
 */

inline void setRuntimePidState(const bool enabled) {
    static std::mutex           pid_mutex;
    std::lock_guard<std::mutex> lock(pid_mutex);

    g_state.process.pidEnabled = enabled;
    // TODO probably wrong
    Config::getInstance().pidEnabled.set(enabled);
}

inline void setSteamMode(const bool steamMode) {
     static std::mutex           steam_mutex;
     std::lock_guard<std::mutex> lock(steam_mutex);

     g_state.machine.steamON = steamMode;

     if (g_state.machine.steamON) {
         g_state.machine.steamFirstON = true;
     } else {
         g_state.machine.steamFirstON = false;
     }
}

// Helper function for timing debug
inline bool isMqttUpdateRunning() {
    return CleverCoffee::getGlobalSystemContext()->mqttManager() && CleverCoffee::getGlobalSystemContext()->mqttManager()->isUpdateRunning();
}

// MQTT discovery timer callback
inline void sendHASSIODiscoveryMsg() {
    if (CleverCoffee::getGlobalSystemContext()->mqttManager() && CleverCoffee::getGlobalSystemContext()->mqttManager()->isEnabled()) {
        CleverCoffee::getGlobalSystemContext()->mqttManager()->sendHASSIODiscoveryMsg();
    }
}

// Emergency stop if temp is too high
inline void testEmergencyStop() {
     static std::mutex           emergency_mutex;
     std::lock_guard<std::mutex> lock(emergency_mutex);

     if (g_state.process.temperature > EmergencyStopTemp && g_state.machine.emergencyStop == false) {
         g_state.machine.emergencyStop = true;
     } else if (g_state.process.temperature < (Config::getInstance().brewSetpoint.get() + 5) &&
                g_state.machine.emergencyStop == true) {
         g_state.machine.emergencyStop = false;
     }
}

/**
 * @brief Switch to offline mode if maxWifiReconnects were exceeded during boot
 */
inline void initOfflineMode() {
    static std::mutex           offline_mutex;
    std::lock_guard<std::mutex> lock(offline_mutex);

    if (Config::getInstance().hardwareOledEnabled.get()) {
        g_state.display.displayOffline = 1;
    }

    LOG(INFO, "Start offline mode with eeprom values, no wifi :(");
    g_state.network.offlineMode = true;
}
