#pragma once
#include "../Config.h"
#include "../state/GlobalState.h"
#include "Logger.h"



inline void setRuntimePidState(const bool enabled) {
    g_state.process.pidEnabled = enabled;
    Config::getInstance().set<bool>("pid.enabled", enabled);
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
