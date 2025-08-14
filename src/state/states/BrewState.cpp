/**
 * @file BrewState.cpp
 * @brief Implementation of BrewIdleState for brew idle operations
 */

#include "BrewState.h"
#include "../MachineStateContext.h"
#include "../GlobalState.h"
#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "BrewPreinfusionState.h"
#include "BackflushState.h"
#include "PidNormalState.h"
#include "Logger.h"


void BrewIdleState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Brew idle - ready to start brewing");
}

void BrewIdleState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
}

void BrewIdleState::update(MachineStateContext& context) {
    // Monitor brew conditions while idle
    LOGF(DEBUG, "Brew Idle: Temp=%.1f°C, Weight=%.1fg, Tank=%s",
         context.getCurrentTemperature(),
         context.getCurrentBrewWeight(),
         context.isWaterTankFull() ? "OK" : "EMPTY");
}

std::unique_ptr<MachineState> BrewIdleState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency conditions (highest priority)
    // 2. Sensor errors
    // 3. Brew start request (condition flag)
    // 4. Manual brew switch activation
    // 5. Backflush operations
    // 6. Return to normal operation

    // Check for emergency stop
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop activated");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError()) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error detected");
        return std::make_unique<SensorErrorState>();
    }

    // Check for condition flags
    auto& flags = g_state.machine.flags;
    // Note: requestBrewStart is already handled in PidNormalState - this is for manual switch activation

    // Check if brew switch was activated
    if (context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION, "Brew switch activated");
        return std::make_unique<BrewPreinfusionState>();
    }

    // Check if backflush was activated
    if (context.isBackflushActive() || flags.requestBackflushStart) {
        if (flags.requestBackflushStart) {
            flags.requestBackflushStart = false; // Reset flag
        }
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush activated");
        return std::make_unique<BackflushState>();
    }

    // Check if we should return to normal operation
    if (!context.isBrewActive() && !context.isBackflushActive()) {
        // No active brew operations - return to normal PID state
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "No active brew operations - returning to normal");
        return std::make_unique<PidNormalState>();
    }

    // Stay in brew idle state
    return nullptr;
}
