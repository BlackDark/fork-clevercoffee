/**
 * @file StateFactory.cpp
 * @brief Implementation of state factory functions.
 */

#include "clevercoffee/state/StateFactory.h"

#include "clevercoffee/Logger.h"
#include "clevercoffee/state/states/BackflushStates.h"
#include "clevercoffee/state/states/BrewStates.h"
#include "clevercoffee/state/states/EmergencyStopState.h"
#include "clevercoffee/state/states/ErrorStates.h"
#include "clevercoffee/state/states/InitState.h"
#include "clevercoffee/state/states/PidStates.h"
#include "clevercoffee/state/states/SteamStates.h"
#include "clevercoffee/state/states/SystemStates.h"

#include <Arduino.h>

/**
 * @brief Create a new state instance by ID.
 * @param id The state ID to create.
 * @return Unique pointer to new state instance (never nullptr).
 * @note All states are instance-based (created on demand). Each transition creates a fresh instance.
 * @note If state ID is not recognized, system will restart immediately with error log.
 */
std::unique_ptr<MachineState> createStateInstance(MachineStateId id) {
    switch (id) {
        case MachineStateId::BACKFLUSH_IDLE:
            return std::make_unique<BackflushState>();
        case MachineStateId::BACKFLUSH_FILLING:
            return std::make_unique<BackflushFillingState>();
        case MachineStateId::BACKFLUSH_FLUSHING:
            return std::make_unique<BackflushFlushingState>();
        case MachineStateId::BACKFLUSH_FINISHED:
            return std::make_unique<BackflushFinishedState>();
        case MachineStateId::BREW_PREINFUSION:
            return std::make_unique<BrewPreinfusionState>();
        case MachineStateId::BREW_PREINFUSION_PAUSE:
            return std::make_unique<BrewPreinfusionPauseState>();
        case MachineStateId::BREW_RUNNING:
            return std::make_unique<BrewRunningState>();
        case MachineStateId::BREW_FINISHED:
            return std::make_unique<BrewFinishedState>();
        case MachineStateId::EMERGENCY_STOP:
            return std::make_unique<EmergencyStopState>();
        case MachineStateId::SENSOR_ERROR:
            return std::make_unique<SensorErrorState>();
        case MachineStateId::WATER_TANK_EMPTY:
            return std::make_unique<WaterTankEmptyState>();
        case MachineStateId::EEPROM_ERROR:
            return std::make_unique<EepromErrorState>();
        case MachineStateId::INIT:
            return std::make_unique<InitState>();
        case MachineStateId::PID_NORMAL:
            return std::make_unique<PidNormalState>();
        case MachineStateId::PID_DISABLED:
            return std::make_unique<PidDisabledState>();
        case MachineStateId::STANDBY:
            return std::make_unique<StandbyState>();
        case MachineStateId::MANUAL_FLUSH_RUNNING:
            return std::make_unique<ManualFlushRunningState>();
        case MachineStateId::STEAM_RUNNING:
            return std::make_unique<SteamRunningState>();
        default:
            // CRITICAL: Unknown state ID
            LOGF(FATAL, "CRITICAL: Unknown state ID=%d. System will restart.", static_cast<int>(id));
            ESP.restart();
            return nullptr; // Unreachable, but satisfies return type
    }
}

/**
 * @brief Get state name by ID.
 * @param id The state ID.
 * @return State name string.
 */
const char* getStateName(MachineStateId id) {
    switch (id) {
        case MachineStateId::BACKFLUSH_IDLE:
            return "Backflush Idle";
        case MachineStateId::BACKFLUSH_FILLING:
            return "Backflush Filling";
        case MachineStateId::BACKFLUSH_FLUSHING:
            return "Backflush Flushing";
        case MachineStateId::BACKFLUSH_FINISHED:
            return "Backflush Finished";
        case MachineStateId::BREW_PREINFUSION:
            return "Brew Preinfusion";
        case MachineStateId::BREW_PREINFUSION_PAUSE:
            return "Brew Preinfusion Pause";
        case MachineStateId::BREW_RUNNING:
            return "Brew Running";
        case MachineStateId::BREW_FINISHED:
            return "Brew Finished";
        case MachineStateId::EMERGENCY_STOP:
            return "Emergency Stop";
        case MachineStateId::SENSOR_ERROR:
            return "Sensor Error";
        case MachineStateId::WATER_TANK_EMPTY:
            return "Water Tank Empty";
        case MachineStateId::EEPROM_ERROR:
            return "EEPROM Error";
        case MachineStateId::INIT:
            return "Init";
        case MachineStateId::PID_NORMAL:
            return "PID Normal";
        case MachineStateId::PID_DISABLED:
            return "PID Disabled";
        case MachineStateId::STANDBY:
            return "Standby";
        case MachineStateId::MANUAL_FLUSH_RUNNING:
            return "Manual Flush Running";
        case MachineStateId::STEAM_RUNNING:
            return "Steam Running";
        default:
            return "Unknown";
    }
}
