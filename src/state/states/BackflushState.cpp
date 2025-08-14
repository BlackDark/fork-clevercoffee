/**
 * @file BackflushState.cpp
 * @brief Implementation of BackflushIdleState for backflush operations
 */

#include "BackflushState.h"
#include "../MachineStateContext.h"
#include "../GlobalState.h"
#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "PidNormalState.h"
#include "BrewState.h"
#include "Logger.h"


void BackflushIdleState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Backflush mode activated");

    // Enable backflush mode
    context.setBackflushState(true);
}

void BackflushIdleState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Exiting backflush mode");

    // Disable backflush mode
    context.setBackflushState(false);
}

void BackflushIdleState::update(MachineStateContext& context) {
    // Monitor backflush conditions
    LOGF(DEBUG, "Backflush: Temp=%.1f°C, Tank=%s, BackflushActive=%s",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.isBackflushActive() ? "YES" : "NO");
}

std::unique_ptr<MachineState> BackflushIdleState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency conditions (highest priority)
    // 2. Sensor errors
    // 3. Backflush stop request (condition flag)
    // 4. Backflush completion
    // 5. Water tank empty
    // 6. Manual deactivation

    // Check for emergency stop
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during backflush");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError()) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error during backflush");
        return std::make_unique<SensorErrorState>();
    }

    // Check for condition flags - backflush stop request
    auto& flags = g_state.machine.flags;
    if (flags.requestBackflushStop) {
        flags.requestBackflushStop = false; // Reset flag
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Backflush stop requested");
        return std::make_unique<BrewState>();
    }

    // Check if backflush was deactivated
    if (!context.isBackflushActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Backflush deactivated");
        return std::make_unique<BrewState>();
    }

    // Check water tank
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateId::WATER_TANK_EMPTY, "Water tank empty during backflush");
        // For now, exit backflush mode
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Water tank empty - exiting backflush mode");
        return std::make_unique<BrewState>();
    }

    // TODO: Add logic for backflush completion based on cycles, time, or other criteria
    // This would typically involve multiple cycles of pumping and pausing

    // Note: Backflush start request is handled in BrewState
    // This state represents the active backflush cleaning mode

    // Continue in backflush mode
    return nullptr;
}
