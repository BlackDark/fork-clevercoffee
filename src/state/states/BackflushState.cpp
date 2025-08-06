/**
 * @file BackflushState.cpp
 * @brief Implementation of BackflushState for machine cleaning operations
 */

#include "BackflushState.h"
#include "../MachineStateContext.h"
#include "EmergencyStopState.h"
#include "PidNormalState.h"
#include "SensorErrorState.h"
#include "WaterTankEmptyState.h"
#include "Logger.h"

void BackflushState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Starting backflush cleaning operation");
    
    // Start backflush operation - activate cleaning cycle
    context.setBackflushState(true);
}

void BackflushState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Stopping backflush cleaning operation");
    
    // Stop backflush operation - deactivate cleaning hardware
    context.setBackflushState(false);
}

void BackflushState::update(MachineStateContext& context) {
    // Monitor backflush operation
    LOGF(DEBUG, "Backflush: Active, Temp=%.1f°C, Tank=%s, Sensors=%s", 
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY", 
         context.hasSensorError() ? "ERROR" : "OK");
}

std::unique_ptr<MachineState> BackflushState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency stop (highest priority - immediate safety)
    // 2. System errors (sensors, water tank)
    // 3. Backflush completion (normal operation)

    // Check emergency conditions first
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateIds::EMERGENCY_STOP, 
                                   "Emergency condition detected during backflush");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for water tank empty
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateIds::WATER_TANK_EMPTY, 
                                   "Water tank empty during backflush");
        return std::make_unique<WaterTankEmptyState>();
    }

    // Check for sensor errors
    if (context.hasSensorError() || context.hasTemperatureError()) {
        context.logStateTransition(getStateId(), MachineStateIds::SENSOR_ERROR, 
                                   "Sensor error detected during backflush");
        return std::make_unique<SensorErrorState>();
    }

    // Check if backflush is no longer active (cleaning cycle completed)
    if (!context.isBackflushActive()) {
        context.logStateTransition(getStateId(), MachineStateIds::PID_NORMAL, 
                                   "Backflush cleaning completed");
        return std::make_unique<PidNormalState>();
    }

    // Continue backflush operation
    return nullptr;
}