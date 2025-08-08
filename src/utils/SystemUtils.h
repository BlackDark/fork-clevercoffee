#pragma once

#include "../Config.h"
#include "../network/MQTTManager.h"
#include "../state/GlobalState.h"

/**
 * @brief System utility functions to replace legacy tight-coupled functions
 * These will be gradually moved to more appropriate locations
 */

inline void setRuntimePidState(const bool enabled) {
    g_state.process.pidEnabled = enabled;
    // TODO probably wrong
    Config::getInstance().pidEnabled.set(enabled);
}

inline void setSteamMode(const bool steamMode) {
    g_state.machine.steamON = steamMode;

    if (g_state.machine.steamON) {
        g_state.machine.steamFirstON = true;
    } else {
        g_state.machine.steamFirstON = false;
    }
}

// Helper function for timing debug
inline bool isMqttUpdateRunning() {
    return g_state.network.mqttManager && g_state.network.mqttManager->isUpdateRunning();
}

// MQTT discovery timer callback
inline void sendHASSIODiscoveryMsg() {
    if (g_state.network.mqttManager && g_state.network.mqttManager->isEnabled()) {
        g_state.network.mqttManager->sendHASSIODiscoveryMsg();
    }
}

// Emergency stop if temp is too high
inline void testEmergencyStop() {
    if (g_state.process.temperature > EmergencyStopTemp && g_state.machine.emergencyStop == false) {
        g_state.machine.emergencyStop = true;
    }
    else if (g_state.process.temperature < (Config::getInstance().brewSetpoint.get() + 5) && g_state.machine.emergencyStop == true) {
        g_state.machine.emergencyStop = false;
    }
}

/**
 * @brief Switch to offline mode if maxWifiReconnects were exceeded during boot
 */
inline void initOfflineMode() {
    if (Config::getInstance().hardwareOledEnabled.get()) {
        g_state.display.displayOffline = 1;
    }

    LOG(INFO, "Start offline mode with eeprom values, no wifi :(");
    g_state.network.offlineMode = true;
}
