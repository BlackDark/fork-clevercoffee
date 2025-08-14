/**
 * @file ManualFlushState.cpp
 * @brief Implementation of ManualFlushIdleState for manual flush operations
 */

#include "ManualFlushState.h"
#include "../MachineStateContext.h"
#include "../GlobalState.h"
#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "PidNormalState.h"
#include "Logger.h"


void ManualFlushIdleState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Manual flush mode activated");

    // Enable manual flush mode
    context.setManualFlushState(true);
}

void ManualFlushIdleState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Exiting manual flush mode");

    // Disable manual flush mode
    context.setManualFlushState(false);
}

void ManualFlushIdleState::update(MachineStateContext& context) {
    // Monitor manual flush conditions
    LOGF(DEBUG, "Manual Flush: Temp=%.1f°C, Tank=%s, FlushActive=%s",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.isManualFlushActive() ? "YES" : "NO");
}

std::unique_ptr<MachineState> ManualFlushIdleState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency conditions (highest priority)
    // 2. Sensor errors
    // 3. Manual flush stop request (condition flag)
    // 4. Manual flush switch deactivation
    // 5. Water tank empty
    // 6. Return to normal operation

    // Check for emergency stop
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during manual flush");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError()) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error during manual flush");
        return std::make_unique<SensorErrorState>();
    }

    // Check for condition flags - manual flush stop request
    auto& flags = g_state.machine.flags;
    if (flags.requestManualFlushStop) {
        flags.requestManualFlushStop = false; // Reset flag
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Manual flush stop requested");
        return std::make_unique<PidNormalState>();
    }

    // Check if manual flush switch was deactivated
    if (!context.isManualFlushActive()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Manual flush switch deactivated");
        return std::make_unique<PidNormalState>();
    }

    // Check water tank
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateId::WATER_TANK_EMPTY, "Water tank empty during manual flush");
        // For now, exit manual flush mode
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Water tank empty - exiting manual flush mode");
        return std::make_unique<PidNormalState>();
    }

    // Note: Manual flush start request would be handled in PidNormalState or similar
    // This state represents the active manual flush mode

    // Continue in manual flush mode
    return nullptr;
}
