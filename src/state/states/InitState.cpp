/**
 * @file InitState.cpp
 * @brief Implementation of InitState for system startup and validation
 */

#include "InitState.h"
#include "../MachineStateContext.h"
#include "Logger.h"
#include "PidDisabledState.h"
#include "PidNormalState.h"
#include "SensorErrorState.h"
#include "WaterTankEmptyState.h"

void InitState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "System initializing - performing startup checks");
}

void InitState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "System initialization complete");
}

void InitState::update(MachineStateContext& context) {
    // Perform continuous system validation during init
    // The actual transition logic is handled in checkTransitions()

    // Log current system status for debugging
    LOGF(DEBUG, "Init state: Water tank: %s, Sensors: %s, PID: %s", checkWaterTank(context) ? "OK" : "EMPTY", checkSensors(context) ? "OK" : "ERROR", checkPidEnabled(context) ? "ENABLED" : "DISABLED");
}

std::unique_ptr<MachineState> InitState::checkTransitions(MachineStateContext& context) {
    // Priority order for state transitions:
    // 1. Water tank empty (highest priority - safety)
    // 2. Sensor errors (critical for operation)
    // 3. PID disabled/enabled (normal operation states)

    // Check water tank first - critical for safety
    if (!checkWaterTank(context)) {
        context.logStateTransition(getStateId(), MachineStateId::WATER_TANK_EMPTY, "Water tank empty");
        return std::make_unique<WaterTankEmptyState>();
    }

    // Check sensors - critical for temperature control
    if (!checkSensors(context)) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error detected");
        return std::make_unique<SensorErrorState>();
    }

    // Determine normal operation state based on PID setting
    if (!checkPidEnabled(context)) {
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, "PID disabled");
        return std::make_unique<PidDisabledState>();
    }
    else {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "PID enabled - entering normal operation");
        return std::make_unique<PidNormalState>();
    }
}

bool InitState::checkWaterTank(MachineStateContext& context) const {
    return context.isWaterTankFull();
}

bool InitState::checkSensors(MachineStateContext& context) const {
    // Check if sensor manager reports any errors
    if (context.hasSensorError()) {
        return false;
    }

    // Check temperature sensor specifically
    if (context.hasTemperatureError()) {
        return false;
    }

    return true;
}

bool InitState::checkPidEnabled(MachineStateContext& context) const {
    return context.isPidEnabled();
}