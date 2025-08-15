/**
 * @file StateFactory.h
 * @brief Direct mapping between MachineStateId and State classes
 */

#pragma once

#include <memory>
#include <unordered_map>
#include <functional>
#include "MachineStateIds.h"
#include "MachineState.h"

// Include all state headers
#include "states/InitState.h"
#include "states/PidNormalState.h"
#include "states/EmergencyStopState.h"
#include "states/BrewStates.h"
#include "states/BackflushStates.h"
#include "states/WaterSteamStates.h"
#include "states/SystemStates.h"
#include "states/ErrorStates.h"

/**
 * @brief Factory function type for creating state instances
 */
using StateFactory = std::function<std::unique_ptr<MachineState>()>;
using StateFactoryMap = std::unordered_map<MachineStateId, StateFactory>;

/**
 * @brief Get the factory map that links MachineStateId to actual state classes
 * This ensures direct linkage between state IDs and their implementations
 */
inline const StateFactoryMap& getStateFactories() {
    static const StateFactoryMap factories = {
        // Core system states
        {MachineStateId::INIT, []() { return std::make_unique<InitState>(); }},
        {MachineStateId::PID_NORMAL, []() { return std::make_unique<PidNormalState>(); }},
        {MachineStateId::PID_DISABLED, []() { return std::make_unique<PidDisabledState>(); }},
        {MachineStateId::STANDBY, []() { return std::make_unique<StandbyState>(); }},
        
        // Brew states
        {MachineStateId::BREW_IDLE, []() { return std::make_unique<BrewIdleState>(); }},
        {MachineStateId::BREW_PREINFUSION, []() { return std::make_unique<BrewPreinfusionState>(); }},
        {MachineStateId::BREW_PREINFUSION_PAUSE, []() { return std::make_unique<BrewPreinfusionPauseState>(); }},
        {MachineStateId::BREW_RUNNING, []() { return std::make_unique<BrewRunningState>(); }},
        {MachineStateId::BREW_FINISHED, []() { return std::make_unique<BrewFinishedState>(); }},
        
        // Hot water states
        {MachineStateId::HOT_WATER_IDLE, []() { return std::make_unique<HotWaterIdleState>(); }},
        {MachineStateId::HOT_WATER_RUNNING, []() { return std::make_unique<HotWaterRunningState>(); }},
        {MachineStateId::HOT_WATER_STOPPED, []() { return std::make_unique<HotWaterStoppedState>(); }},
        
        // Steam states
        {MachineStateId::STEAM_IDLE, []() { return std::make_unique<SteamIdleState>(); }},
        {MachineStateId::STEAM_RUNNING, []() { return std::make_unique<SteamRunningState>(); }},
        {MachineStateId::STEAM_STOPPED, []() { return std::make_unique<SteamStoppedState>(); }},
        
        // Manual flush states
        {MachineStateId::MANUAL_FLUSH_IDLE, []() { return std::make_unique<ManualFlushIdleState>(); }},
        {MachineStateId::MANUAL_FLUSH_RUNNING, []() { return std::make_unique<ManualFlushRunningState>(); }},
        
        // Backflush states
        {MachineStateId::BACKFLUSH_IDLE, []() { return std::make_unique<BackflushState>(); }},
        {MachineStateId::BACKFLUSH_FILLING, []() { return std::make_unique<BackflushFillingState>(); }},
        {MachineStateId::BACKFLUSH_FLUSHING, []() { return std::make_unique<BackflushFlushingState>(); }},
        {MachineStateId::BACKFLUSH_FINISHED, []() { return std::make_unique<BackflushFinishedState>(); }},
        
        // Error/safety states
        {MachineStateId::EMERGENCY_STOP, []() { return std::make_unique<EmergencyStopState>(); }},
        {MachineStateId::SENSOR_ERROR, []() { return std::make_unique<SensorErrorState>(); }},
        {MachineStateId::WATER_TANK_EMPTY, []() { return std::make_unique<WaterTankEmptyState>(); }},
        {MachineStateId::EEPROM_ERROR, []() { return std::make_unique<EepromErrorState>(); }}
    };
    return factories;
}

/**
 * @brief Create a state instance by ID with direct linkage guarantee
 * @param id The state ID to create
 * @return Unique pointer to the state, or nullptr if ID not found
 */
inline std::unique_ptr<MachineState> createState(MachineStateId id) {
    const auto& factories = getStateFactories();
    auto it = factories.find(id);
    return (it != factories.end()) ? it->second() : nullptr;
}

/**
 * @brief Validate that all MachineStateId values have corresponding factory entries
 * This should be called during system initialization to ensure completeness
 * @return true if all state IDs have factories, false otherwise
 */
inline bool validateStateFactoryCompleteness() {
    const auto& factories = getStateFactories();
    
    // Check that all commonly used state IDs have factories
    const std::vector<MachineStateId> requiredStates = {
        MachineStateId::INIT,
        MachineStateId::PID_NORMAL,
        MachineStateId::PID_DISABLED,
        MachineStateId::STANDBY,
        MachineStateId::BREW_IDLE,
        MachineStateId::HOT_WATER_IDLE,
        MachineStateId::STEAM_IDLE,
        MachineStateId::EMERGENCY_STOP,
        MachineStateId::SENSOR_ERROR,
        MachineStateId::WATER_TANK_EMPTY,
        MachineStateId::EEPROM_ERROR
    };
    
    for (const auto& stateId : requiredStates) {
        if (factories.find(stateId) == factories.end()) {
            return false;
        }
    }
    
    return true;
}

/**
 * @brief Template helper to ensure compile-time linkage between state IDs and classes
 * Usage: static_assert(StateIdClassMapping<MachineStateId::BREW_IDLE, BrewIdleState>::valid);
 */
template<MachineStateId StateId, typename StateClass>
struct StateIdClassMapping {
    static constexpr bool valid = (StateClass::template BaseState<StateId, StateClass>::getStateId() == StateId);
};