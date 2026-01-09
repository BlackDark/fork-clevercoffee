/**
 * @file GlobalState.h
 * @brief Essential type definitions for state machine and system coordination
 *
 * MIGRATION HISTORY:
 * This file originally contained a global g_state variable and GlobalState struct.
 * Phase 4 eliminated the global variable and moved all state to SystemContext.
 * All dead code struct types have been removed in this phase.
 *
 * CURRENT PURPOSE:
 * Contains only type definitions essential for the state machine and system:
 * - cmp_str: String comparator for MQTT variable/sensor maps
 * - MachineStateFlags: Request flags for state machine transitions
 * - MachineStateData: Machine state and brewing control data
 * - TIME_TO_DISPLAY_OFF_MILLIS: Display timeout constant
 *
 * All global state has been eliminated. State is managed entirely through
 * SystemContext with dependency injection. See SystemContext.h for architecture.
 */

#pragma once

#include "clevercoffee/defaults.h"
#include "clevercoffee/state/MachineStateIds.h"

#include <Arduino.h>
#include <cstring>
#include <functional>
#include <map>
#include <memory>

// Display timeout constants
constexpr int TIME_TO_DISPLAY_OFF = 10;
constexpr unsigned long TIME_TO_DISPLAY_OFF_MILLIS = TIME_TO_DISPLAY_OFF * 60 * 1000;

/**
 * @brief String comparator for C-string keys in std::map
 * Used for MQTT variable and sensor mappings
 */
struct cmp_str {
    bool operator()(char const* a, char const* b) const {
        return std::strcmp(a, b) < 0;
    }
};

#include "clevercoffee/hardware/scales/Scale.h"

// Forward declarations for type definitions
class U8G2;
class Relay;
class TempSensor;
class MQTTManager;
class ProcessController;
class CleverCoffeeWiFiManager;
class WebServerManager;
class PID;
class Config;

namespace CleverCoffee {
class SystemContext;
}

class Switch;
class LED;
class GPIOPin;
class BrewHandler;
class HotWaterHandler;
class PowerHandler;
class SteamHandler;

extern const char*      WIFI_PASSWORD;
constexpr unsigned long wifiConnectionDelay   = WIFICONNECTIONDELAY;
constexpr unsigned int  maxWifiReconnects     = MAXWIFIRECONNECTS;
constexpr double        EmergencyStopTemp     = 145;
constexpr int           waterTankCountsNeeded = 3; // Number of same readings to change water tank sensing

// Scale connection constants
constexpr unsigned long SCALE_CONNECTION_CHECK_INTERVAL = 500;   // Check every 500 milliseconds
constexpr unsigned long SCALE_CONNECTION_TIMEOUT        = 5000;  // 5 seconds timeout
constexpr unsigned long SCALE_RECONNECTION_TIMEOUT      = 30000; // 30 seconds before giving up

namespace GlobalStateNamespace {

/**
 * @brief State machine transition request flags
 *
 * These flags are used to request transitions between machine states.
 * The state machine processes these flags and transitions accordingly.
 */
struct MachineStateFlags {
    bool requestBrewStart        = false;
    bool requestBrewStop         = false;
    bool requestHotWaterStart    = false;
    bool requestHotWaterStop     = false;
    bool requestManualFlushStart = false;
    bool requestManualFlushStop  = false;
    bool requestBackflushStart   = false;
    bool requestBackflushStop    = false;
    bool requestSteamStart       = false;
    bool requestSteamStop        = false;
    bool requestShutdown         = false;
    bool requestStandby          = false;
    bool requestNormalOperation  = false;
    bool requestSensorError      = false;
};

/**
 * @brief Machine state and brewing control data
 *
 * Contains the current machine state, emergency stop status, and brewing flags.
 * Managed by MachineStateContext in the state machine implementation.
 */
struct MachineStateData {
    MachineStateId machineState        = MachineStateId::INIT;
    MachineStateId lastmachinestate    = MachineStateId::INIT;
    int            lastmachinestatepid = -1;
    bool           emergencyStop       = false;
    bool           steamON             = false;
    bool           steamFirstON        = false;
    bool           backflushOn         = false;
    int            currBackflushCycles = 1;
    bool           waterTankFull       = true;
    bool           systemInitialized   = false;

    MachineStateFlags flags = MachineStateFlags();

    hw_timer_t* timer = nullptr;
};

} // namespace GlobalStateNamespace

// Handler initialization function
void initializeHandlers(CleverCoffee::SystemContext* systemContext = nullptr);
