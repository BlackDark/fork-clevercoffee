#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/network/MQTTManager.h"
#include "clevercoffee/context/SystemContext.h"

#include <mutex>

/**
 * @brief System utility functions to replace legacy tight-coupled functions
 * These will be gradually moved to more appropriate locations
 */

inline void setRuntimePidState(CleverCoffee::SystemContext& systemContext, const bool enabled) {
    static std::mutex           pid_mutex;
    std::lock_guard<std::mutex> lock(pid_mutex);

    systemContext.setProcessPidEnabled(enabled);
    // NOTE: Also updates Config to keep them in sync. This may need refactoring:
    // - Option 1: Config is source of truth, SystemContext reads from it
    // - Option 2: SystemContext is source of truth, Config is read-only runtime state
    // Current approach maintains backward compatibility but creates dual state
    Config::getInstance().pidEnabled.set(enabled);
}

inline void setSteamMode(CleverCoffee::SystemContext& systemContext, const bool steamMode) {
     static std::mutex           steam_mutex;
     std::lock_guard<std::mutex> lock(steam_mutex);

     systemContext.setSteamMode(steamMode);

     if (steamMode) {
         systemContext.setSteamFirstOn(true);
     } else {
         systemContext.setSteamFirstOn(false);
     }
}

// Helper function for timing debug
inline bool isMqttUpdateRunning(CleverCoffee::SystemContext& systemContext) {
    return systemContext.mqttManager() && systemContext.mqttManager()->isUpdateRunning();
}

// MQTT discovery timer callback
inline void sendHASSIODiscoveryMsg(CleverCoffee::SystemContext& systemContext) {
    if (systemContext.mqttManager() && systemContext.mqttManager()->isEnabled()) {
        systemContext.mqttManager()->sendHASSIODiscoveryMsg();
    }
}

// Emergency stop if temp is too high
inline void testEmergencyStop(CleverCoffee::SystemContext& systemContext) {
     static std::mutex           emergency_mutex;
     std::lock_guard<std::mutex> lock(emergency_mutex);

     double currentTemp = systemContext.processTemperature();
     
     if (currentTemp > EmergencyStopTemp && !systemContext.isEmergencyStopActive()) {
         systemContext.triggerEmergencyStop();
     } else if (currentTemp < (Config::getInstance().brewSetpoint.get() + 5) &&
                systemContext.isEmergencyStopActive()) {
         systemContext.setEmergencyStop(false);
     }
}

/**
 * @brief Switch to offline mode if maxWifiReconnects were exceeded during boot
 */
inline void initOfflineMode(CleverCoffee::SystemContext& systemContext) {
    static std::mutex           offline_mutex;
    std::lock_guard<std::mutex> lock(offline_mutex);

    if (Config::getInstance().hardwareOledEnabled.get()) {
        systemContext.uiCoordinator().setDisplayOffline(1);
    }

    LOG(INFO, "Start offline mode with eeprom values, no wifi :(");
    systemContext.setOfflineMode(true);
}
