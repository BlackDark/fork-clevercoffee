/**
 * @file ManualFlushState.cpp
 * @brief Implementation of ManualFlushState for manual flushing operations
 */

#include "ManualFlushState.h"
#include "../MachineStateContext.h"
#include "EmergencyStopState.h"
#include "PidNormalState.h"
#include "SensorErrorState.h"
#include "WaterTankEmptyState.h"
#include "Logger.h"

void ManualFlushState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Starting manual flush operation");
    
    // Start manual flush - activate pump/valve as needed
    context.setManualFlushState(true);
}

void ManualFlushState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Stopping manual flush operation");
    
    // Stop manual flush - deactivate pump/valve
    context.setManualFlushState(false);
}

void ManualFlushState::update(MachineStateContext& context) {
    // Monitor manual flush operation
    // The actual transition logic is handled in checkTransitions()
    
    LOGF(DEBUG, "Manual Flush: Active, Tank=%s, Sensors=%s", 
         context.isWaterTankFull() ? "OK" : "EMPTY", 
         context.hasSensorError() ? "ERROR" : "OK");
}

std::unique_ptr<MachineState> ManualFlushState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency stop (highest priority - immediate safety)
    // 2. System errors (sensors, water tank)
    // 3. Manual flush completion (normal operation)

    // Check emergency conditions first
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateIds::EMERGENCY_STOP, 
                                   "Emergency condition detected during manual flush");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for water tank empty
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateIds::WATER_TANK_EMPTY, 
                                   "Water tank empty during manual flush");
        return std::make_unique<WaterTankEmptyState>();
    }

    // Check for sensor errors
    if (context.hasSensorError() || context.hasTemperatureError()) {
        context.logStateTransition(getStateId(), MachineStateIds::SENSOR_ERROR, 
                                   "Sensor error detected during manual flush");
        return std::make_unique<SensorErrorState>();
    }

    // Check if manual flush is no longer active (user stopped it)
    if (!context.isManualFlushActive()) {
        context.logStateTransition(getStateId(), MachineStateIds::PID_NORMAL, 
                                   "Manual flush completed");
        return std::make_unique<PidNormalState>();
    }

    // Continue manual flush operation
    return nullptr;
}