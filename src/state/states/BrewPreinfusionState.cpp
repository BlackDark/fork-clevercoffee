/**
 * @file BrewPreinfusionState.cpp
 * @brief Implementation of BrewPreinfusionState for brew preinfusion
 */

#include "BrewPreinfusionState.h"
#include "../MachineStateContext.h"
#include "../GlobalState.h"
#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "BrewPreinfusionPauseState.h"
#include "BrewRunningState.h"
#include "BrewState.h"
#include "Logger.h"


void BrewPreinfusionState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Brew preinfusion started");
}

void BrewPreinfusionState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
}

void BrewPreinfusionState::update(MachineStateContext& context) {
    // Monitor preinfusion progress
    LOGF(DEBUG, "Brew Preinfusion: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

std::unique_ptr<MachineState> BrewPreinfusionState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency conditions (highest priority)
    // 2. Sensor errors
    // 3. Brew stop request (condition flag)
    // 4. Manual brew switch deactivation
    // 5. Preinfusion completion -> pause or full brew
    // 6. Water tank empty

    // Check for emergency stop
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during preinfusion");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError()) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error during preinfusion");
        return std::make_unique<SensorErrorState>();
    }

    // Check for condition flags - brew stop request
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false; // Reset flag
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew stop requested during preinfusion");
        return std::make_unique<BrewState>();
    }

    // Check if brew switch was deactivated manually
    if (!context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew switch deactivated during preinfusion");
        return std::make_unique<BrewState>();
    }

    // Check water tank
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateId::WATER_TANK_EMPTY, "Water tank empty during preinfusion");
        // Note: WaterTankEmptyState would need to be included
        // return std::make_unique<WaterTankEmptyState>();
        // For now, stop the brew
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Water tank empty - stopping brew");
        return std::make_unique<BrewState>();
    }

    // TODO: Add logic to determine when preinfusion is complete
    // This would typically be based on time, pressure, or other brewing parameters
    // For now, we'll add placeholder transitions

    // Check if we should transition to preinfusion pause
    // (This would be based on brewing algorithm/timer)

    // Check if we should transition directly to brew running
    // (This would be for brewing profiles without pause)

    // Continue preinfusion
    return nullptr;
}
