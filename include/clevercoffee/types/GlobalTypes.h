/**
 * @file GlobalTypes.h
 * @brief Essential type definitions and constants for the system
 *
 * This file contains type definitions, constants, and utilities that were
 * previously in GlobalState.h. It's organized by category for clarity.
 */

#pragma once

#include <Arduino.h>
#include <cstring>
#include <functional>
#include <map>
#include <memory>

#include "clevercoffee/defaults.h"
#include "clevercoffee/state/MachineStateIds.h"

// ============================================================================
// Display Constants
// ============================================================================

constexpr int TIME_TO_DISPLAY_OFF = 10;
constexpr unsigned long TIME_TO_DISPLAY_OFF_MILLIS = TIME_TO_DISPLAY_OFF * 60 * 1000;

// ============================================================================
// String Utilities
// ============================================================================

/**
 * @brief String comparator for C-string keys in std::map
 * Used for MQTT variable and sensor mappings
 * @deprecated Use unordered_map with hash_cstr instead
 */
struct cmp_str {
    bool operator()(char const* a, char const* b) const {
        return std::strcmp(a, b) < 0;
    }
};

/**
 * @brief Hash function for C-string keys in std::unordered_map
 * Used for MQTT variable and sensor mappings
 */
struct hash_cstr {
    std::size_t operator()(const char* str) const noexcept {
        // djb2 hash algorithm
        std::size_t hash = 5381;
        int c;
        while ((c = *str++)) {
            hash = ((hash << 5) + hash) + c; // hash * 33 + c
        }
        return hash;
    }
};

/**
 * @brief Equality comparator for C-string keys in std::unordered_map
 */
struct equal_cstr {
    bool operator()(const char* a, const char* b) const noexcept {
        return std::strcmp(a, b) == 0;
    }
};

// ============================================================================
// State Machine Types
// ============================================================================

/**
 * @brief State machine transition request flags
 *
 * These flags are used to request transitions between machine states.
 * The state machine processes these flags and transitions accordingly.
 */
struct MachineStateFlags {
    bool requestBrewStart        = false;
    bool requestBrewStop         = false;
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

// ============================================================================
// System Constants
// ============================================================================

// WiFi and network constants
extern const char*      WIFI_PASSWORD;
constexpr unsigned long wifiConnectionDelay   = 10000;  // delay between reconnects in ms (from defaults.h)
constexpr unsigned int  maxWifiReconnects     = 5;      // maximum reconnection attempts (from defaults.h)

// Temperature safety
constexpr double EmergencyStopTemp     = 145;

// Water tank sensing
constexpr int waterTankCountsNeeded = 3; // Number of same readings to change water tank sensing

// Scale connection constants
constexpr unsigned long SCALE_CONNECTION_CHECK_INTERVAL = 500;   // Check every 500 milliseconds
constexpr unsigned long SCALE_CONNECTION_TIMEOUT        = 5000;  // 5 seconds timeout
constexpr unsigned long SCALE_RECONNECTION_TIMEOUT      = 30000; // 30 seconds before giving up

// ============================================================================
// Hardware Types
// ============================================================================

#include "clevercoffee/hardware/scales/Scale.h"

// Forward declarations
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

// ============================================================================
// Initialization
// ============================================================================

// Handler initialization (formerly in GlobalState.h)
void initializeHandlers(CleverCoffee::SystemContext* systemContext = nullptr);
