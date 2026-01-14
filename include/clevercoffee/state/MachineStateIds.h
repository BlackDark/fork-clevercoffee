/**
 * @file MachineStateIds.h
 * @brief Typed enums for machine states
 */

#pragma once

/**
 * @brief All machine state IDs - comprehensive enum with unique values for each state
 */
enum class MachineStateId {
    INIT       = 0,
    PID_NORMAL = 20,

    // Brew states
    BREW_PREINFUSION       = 31,
    BREW_PREINFUSION_PAUSE = 32,
    BREW_RUNNING           = 33,
    BREW_FINISHED          = 34,

    // Manual flush states
    MANUAL_FLUSH_RUNNING = 36,

    // Steam states
    STEAM_RUNNING = 51,

    // Backflush states
    BACKFLUSH_IDLE     = 60,
    BACKFLUSH_FILLING  = 61,
    BACKFLUSH_FLUSHING = 62,
    BACKFLUSH_FINISHED = 63,

    WATER_TANK_EMPTY = 70,
    EMERGENCY_STOP   = 80,
    PID_DISABLED     = 90,
    STANDBY          = 95,
    SENSOR_ERROR     = 100,
    EEPROM_ERROR     = 110,
};

// Helper functions to check state categories
inline constexpr bool isBrewState(MachineStateId state) {
    return state >= MachineStateId::BREW_PREINFUSION && state <= MachineStateId::BREW_FINISHED;
}

inline constexpr bool isSteamState(MachineStateId state) {
    return state == MachineStateId::STEAM_RUNNING;
}

inline constexpr bool isBackflushState(MachineStateId state) {
    return state >= MachineStateId::BACKFLUSH_IDLE && state <= MachineStateId::BACKFLUSH_FINISHED;
}

inline constexpr bool isManualFlushState(MachineStateId state) {
    return state == MachineStateId::MANUAL_FLUSH_RUNNING;
}

/**
 * @brief Switch states for user input
 */
enum class SwitchState {
    IDLE             = 200,
    PRESSED          = 201,
    SHORT_PRESSED    = 202,
    LONG_PRESSED     = 203,
    WAIT_FOR_RELEASE = 204
};
