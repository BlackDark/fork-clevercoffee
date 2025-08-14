/**
 * @file BrewRunningState.cpp
 * @brief Implementation of BrewRunningState for active brewing
 */

#include "BrewRunningState.h"
#include "../MachineStateContext.h"
#include "../GlobalState.h"
#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "BrewState.h"
#include "PidNormalState.h"
#include "Logger.h"


void BrewRunningState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Brew running - coffee extraction in progress");
}

void BrewRunningState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Brew run completed");
}

void BrewRunningState::update(MachineStateContext& context) {
    // Monitor brewing progress
    LOGF(DEBUG, "Brew Running: Temp=%.1f°C, Weight=%.1fg, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getCurrentBrewWeight(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> BrewRunningState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency conditions (highest priority)
    // 2. Sensor errors
    // 3. Brew stop request (condition flag)
    // 4. Manual brew switch deactivation
    // 5. Brew completion conditions

    // Check for emergency stop
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during brew");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError()) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error during brew");
        return std::make_unique<SensorErrorState>();
    }

    // Check for condition flags - brew stop request
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false; // Reset flag
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew stop requested");
        return std::make_unique<BrewState>();
    }

    // Check if brew switch was deactivated manually
    if (!context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew switch deactivated");
        return std::make_unique<BrewState>();
    }

    // TODO: Add brew completion conditions based on time, weight, or other criteria
    // This would be handled by the brew control logic and set appropriate flags

    // Continue brewing
    return nullptr;
}
