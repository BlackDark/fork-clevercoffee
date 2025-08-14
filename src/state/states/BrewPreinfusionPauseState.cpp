/**
 * @file BrewPreinfusionPauseState.cpp
 * @brief Implementation of BrewPreinfusionPauseState for brew preinfusion pause
 */

#include "BrewPreinfusionPauseState.h"
#include "../MachineStateContext.h"
#include "../GlobalState.h"
#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "BrewRunningState.h"
#include "BrewState.h"
#include "Logger.h"


void BrewPreinfusionPauseState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Brew preinfusion pause - blooming phase");
}

void BrewPreinfusionPauseState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
}

void BrewPreinfusionPauseState::update(MachineStateContext& context) {
    // Monitor pause conditions
    LOGF(DEBUG, "Brew Preinfusion Pause: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

std::unique_ptr<MachineState> BrewPreinfusionPauseState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency conditions (highest priority)
    // 2. Sensor errors
    // 3. Brew stop request (condition flag)
    // 4. Manual brew switch deactivation
    // 5. Pause completion -> brew running
    // 6. Water tank empty

    // Check for emergency stop
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during preinfusion pause");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError()) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error during preinfusion pause");
        return std::make_unique<SensorErrorState>();
    }

    // Check for condition flags - brew stop request
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false; // Reset flag
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew stop requested during preinfusion pause");
        return std::make_unique<BrewState>();
    }

    // Check if brew switch was deactivated manually
    if (!context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew switch deactivated during preinfusion pause");
        return std::make_unique<BrewState>();
    }

    // Check water tank
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateId::WATER_TANK_EMPTY, "Water tank empty during preinfusion pause");
        // For now, stop the brew
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Water tank empty - stopping brew");
        return std::make_unique<BrewState>();
    }

    // TODO: Add logic to determine when pause is complete and brewing should resume
    // This would typically be based on time or user configuration
    // For now, we'll add placeholder logic

    // Check if pause time is complete and we should transition to brew running
    // (This would be based on brewing algorithm/timer)

    // Continue pause
    return nullptr;
}
