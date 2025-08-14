/**
 * @file PidDisabledState.cpp
 * @brief Implementation of PidDisabledState when PID is disabled
 */

#include "PidDisabledState.h"
#include "../MachineStateContext.h"
#include "EmergencyStopState.h"
#include "Logger.h"
#include "PidNormalState.h"
#include "SensorErrorState.h"

void PidDisabledState::onEntry(MachineStateContext& context) {
    context.logStateEntry(static_cast<int>(getStateId()), getStateName());
    LOG(INFO, "PID disabled - heater control off");

    // Ensure PID is disabled and heater is off
    context.setPidRuntimeState(false);
}

void PidDisabledState::onExit(MachineStateContext& context) {
    context.logStateExit(static_cast<int>(getStateId()), getStateName());
    LOG(INFO, "Exiting PID disabled state");
}

void PidDisabledState::update(MachineStateContext& context) {
    // Monitor system status while PID is disabled
    // The heater should remain off in this state

    LOGF(DEBUG, "PID Disabled: Temp=%.1f°C, Tank=%s, Sensors=%s", context.getCurrentTemperature(), context.isWaterTankFull() ? "OK" : "EMPTY", context.hasSensorError() ? "ERROR" : "OK");
}

std::unique_ptr<MachineState> PidDisabledState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency stop (highest priority - immediate safety)
    // 2. System errors (sensors)
    // 3. PID re-enabled (return to normal operation)

    // Check for emergency stop (highest priority)
    if (context.isEmergencyStop()) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::EMERGENCY_STOP), "Emergency stop activated");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError()) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::SENSOR_ERROR), "Sensor error detected");
        return std::make_unique<SensorErrorState>();
    }

    // Check if PID was re-enabled
    if (context.isPidEnabled()) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::PID_NORMAL), "PID re-enabled");
        return std::make_unique<PidNormalState>();
    }

    // Stay in PID disabled state
    return nullptr;
}