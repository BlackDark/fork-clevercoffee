/**
 * @file HotWaterState.cpp
 * @brief Implementation of HotWaterIdleState for hot water idle operations
 */

#include "HotWaterState.h"
#include "../MachineStateContext.h"

#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "HotWaterRunningState.h"
#include "PidNormalState.h"
#include "Logger.h"

void HotWaterIdleState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Hot water idle - ready to dispense hot water");
}

void HotWaterIdleState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
}

void HotWaterIdleState::update(MachineStateContext& context) {
    // Monitor hot water conditions while idle
    LOGF(DEBUG, "Hot Water Idle: Temp=%.1f°C, Tank=%s, Pressure=%.1fbar", 
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> HotWaterIdleState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency conditions (highest priority)
    // 2. Sensor errors
    // 3. Hot water start request (condition flag - already handled in PidNormalState)
    // 4. Manual hot water switch activation
    // 5. Return to normal operation

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

    // Check if hot water switch was activated manually
    if (context.isHotWaterActive()) {
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_RUNNING, "Hot water switch activated");
        return std::make_unique<HotWaterRunningState>();
    }

    // Check if we should return to normal operation
    if (!context.isHotWaterActive()) {
        // No active hot water operations - return to normal PID state
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "No active hot water operations - returning to normal");
        return std::make_unique<PidNormalState>();
    }

    // Stay in hot water idle state
    return nullptr;
}