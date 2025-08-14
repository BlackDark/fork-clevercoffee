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
#include "../GlobalState.h"


void PidDisabledState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "PID disabled - heater control off");

    // Ensure PID is disabled and heater is off
    context.setPidRuntimeState(false);
}

void PidDisabledState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
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
    // 3. Normal operation request (user/external request)
    // 4. PID re-enabled (return to normal operation)

    // Check for emergency stop (highest priority)
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop activated");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError()) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error detected");
        return std::make_unique<SensorErrorState>();
    }

    // Check for normal operation request
    if (g_state.machine.flags.requestNormalOperation) {
        g_state.machine.flags.requestNormalOperation = false;
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Normal operation requested");
        return std::make_unique<PidNormalState>();
    }

    // Check if PID was re-enabled
    if (context.isPidEnabled()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "PID re-enabled");
        return std::make_unique<PidNormalState>();
    }

    // Stay in PID disabled state
    return nullptr;
}
