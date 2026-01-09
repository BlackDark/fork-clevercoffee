#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/network/MQTTManager.h"
#include "clevercoffee/context/SystemContext.h"

#include <mutex>

/**
 * @brief System utility functions to replace legacy tight-coupled functions
 * These will be gradually moved to more appropriate locations
 */

inline void setRuntimePidState(const bool enabled) {
    static std::mutex           pid_mutex;
    std::lock_guard<std::mutex> lock(pid_mutex);

    CleverCoffee::getGlobalSystemContext()->setProcessPidEnabled(enabled);
    // TODO probably wrong
    Config::getInstance().pidEnabled.set(enabled);
}

inline void setSteamMode(const bool steamMode) {
     static std::mutex           steam_mutex;
     std::lock_guard<std::mutex> lock(steam_mutex);

     auto* ctx = CleverCoffee::getGlobalSystemContext();
     if (!ctx) {
         return;
     }

     ctx->setSteamMode(steamMode);

     if (steamMode) {
         ctx->setSteamFirstOn(true);
     } else {
         ctx->setSteamFirstOn(false);
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

     auto* ctx = CleverCoffee::getGlobalSystemContext();
     if (!ctx) {
         return;
     }

     double currentTemp = ctx->processTemperature();
     
     if (currentTemp > EmergencyStopTemp && !ctx->isEmergencyStopActive()) {
         ctx->triggerEmergencyStop();
     } else if (currentTemp < (Config::getInstance().brewSetpoint.get() + 5) &&
                ctx->isEmergencyStopActive()) {
         ctx->setEmergencyStop(false);
     }
}

/**
 * @brief Switch to offline mode if maxWifiReconnects were exceeded during boot
 */
inline void initOfflineMode() {
    static std::mutex           offline_mutex;
    std::lock_guard<std::mutex> lock(offline_mutex);

    auto* ctx = CleverCoffee::getGlobalSystemContext();
    if (!ctx) {
        return;
    }

    if (Config::getInstance().hardwareOledEnabled.get()) {
        ctx->uiCoordinator().setDisplayOffline(1);
    }

    LOG(INFO, "Start offline mode with eeprom values, no wifi :(");
    ctx->setOfflineMode(true);
}
