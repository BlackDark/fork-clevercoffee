#pragma once
#include "../Config.h"
#include "../network/MQTTManager.h" // Required for MQTTManager methods
#include "../state/GlobalState.h"
#include "Logger.h"

// This file should be removed in the future. Tight coupled functions

inline void setRuntimePidState(const bool enabled) {
    g_state.process.pidEnabled = enabled;
    Config::getInstance().pidEnabled.set(enabled);
}

inline void setSteamMode(const bool steamMode) {
    g_state.machine.steamON = steamMode;

    if (g_state.machine.steamON) {
        g_state.machine.steamFirstON = true;
    }

    if (!g_state.machine.steamON) {
        g_state.machine.steamFirstON = false;
    }
}

inline char const* machinestateEnumToString(const LegacyMachineState machineState) {
    switch (machineState) {
        case kInit:
            return "Init";
        case kPidNormal:
            return "PID Normal";
        case kBrew:
            return "Brew";
        case kManualFlush:
            return "Manual Flush";
        case kHotWater:
            return "Hot Water";
        case kSteam:
            return "Steam";
        case kBackflush:
            return "Backflush";
        case kWaterTankEmpty:
            return "Water Tank Empty";
        case kEmergencyStop:
            return "Emergency Stop";
        case kPidDisabled:
            return "PID Disabled";
        case kStandby:
            return "Standby Mode";
        case kSensorError:
            return "Sensor Error";
        case kEepromError:
            return "EEPROM Error";
    }

    return "Unknown";
}

inline void printMachineState() {
    LOGF(DEBUG, "new machineState: %s -> %s", machinestateEnumToString(g_state.machine.lastmachinestate), machinestateEnumToString(g_state.machine.machineState));
}

// Helper function for timing debug
inline bool isMqttUpdateRunning() {
    return g_state.network.mqttManager && g_state.network.mqttManager->isUpdateRunning();
}

// Compatibility wrapper function
// inline int writeSysParamsToMQTT(bool continueOnError = true) {
//     if (g_state.network.mqttManager && g_state.network.mqttManager->isEnabled()) {
//         return g_state.network.mqttManager->writeSysParamsToMQTT(continueOnError);
//     }
//     return 0;
// }

// MQTT functionality is now managed by MQTTManager
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
