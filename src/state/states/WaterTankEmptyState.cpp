/**
 * @file WaterTankEmptyState.cpp
 * @brief Implementation of WaterTankEmptyState for water tank empty condition
 */

#include "WaterTankEmptyState.h"
#include "../MachineStateContext.h"
#include "EmergencyStopState.h"
#include "Logger.h"
#include "PidNormalState.h"
#include "SensorErrorState.h"

void WaterTankEmptyState::onEntry(MachineStateContext& context) {
    context.logStateEntry(static_cast<int>(getStateId()), getStateName());
    LOG(WARNING, "Water tank is empty - operations suspended");

    // Disable operations that require water
    context.disableWaterOperations();
}

void WaterTankEmptyState::onExit(MachineStateContext& context) {
    context.logStateExit(static_cast<int>(getStateId()), getStateName());
    LOG(INFO, "Water tank refilled - operations resuming");

    // Re-enable water operations
    context.enableWaterOperations();
}

void WaterTankEmptyState::update(MachineStateContext& context) {
    // Monitor water tank status
    LOGF(DEBUG, "Water Tank Empty: Monitoring tank level, Sensors=%s", context.hasSensorError() ? "ERROR" : "OK");
}

std::unique_ptr<MachineState> WaterTankEmptyState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency stop (highest priority - immediate safety)
    // 2. Sensor errors (system integrity)
    // 3. Water tank refilled (normal operation resumes)

    // Check emergency conditions first
    if (context.isEmergencyStop()) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::EMERGENCY_STOP), "Emergency condition detected with empty water tank");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError() || context.hasTemperatureError()) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::SENSOR_ERROR), "Sensor error detected with empty water tank");
        return std::make_unique<SensorErrorState>();
    }

    // Check if water tank has been refilled
    if (context.isWaterTankFull()) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::PID_NORMAL), "Water tank refilled - resuming normal operation");
        return std::make_unique<PidNormalState>();
    }

    // Continue waiting for water tank refill
    return nullptr;
}