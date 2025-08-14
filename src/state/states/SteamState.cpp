/**
 * @file SteamState.cpp
 * @brief Implementation of SteamIdleState for steam operations
 */

#include "SteamState.h"
#include "../MachineStateContext.h"
#include "../GlobalState.h"
#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "PidNormalState.h"
#include "Logger.h"


void SteamIdleState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Steam mode activated");

    // Enable steam mode
    context.setSteamMode(true);
}

void SteamIdleState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Exiting steam mode");

    // Disable steam mode
    context.setSteamMode(false);
}

void SteamIdleState::update(MachineStateContext& context) {
    // Monitor steam conditions
    LOGF(DEBUG, "Steam: Temp=%.1f°C, Tank=%s, SteamActive=%s",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.isSteamActive() ? "YES" : "NO");
}

std::unique_ptr<MachineState> SteamIdleState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency conditions (highest priority)
    // 2. Sensor errors
    // 3. Steam stop request (condition flag)
    // 4. Manual steam switch deactivation
    // 5. Water tank empty
    // 6. Steam completion conditions

    // Check for emergency stop
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during steam");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError()) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error during steam");
        return std::make_unique<SensorErrorState>();
    }

    // Check for condition flags - steam stop request
    auto& flags = g_state.machine.flags;
    if (flags.requestSteamStop) {
        flags.requestSteamStop = false; // Reset flag
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Steam stop requested");
        return std::make_unique<PidNormalState>();
    }

    // Check if steam switch was deactivated manually
    if (!context.isSteamActive()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Steam switch deactivated");
        return std::make_unique<PidNormalState>();
    }

    // Check water tank
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateId::WATER_TANK_EMPTY, "Water tank empty during steam");
        // For now, exit steam mode
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Water tank empty - exiting steam mode");
        return std::make_unique<PidNormalState>();
    }

    // Note: Steam start request is handled in PidNormalState
    // This state represents the active steam mode where the user can steam milk

    // Continue in steam mode
    return nullptr;
}
