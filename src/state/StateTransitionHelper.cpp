/**
 * @file StateTransitionHelper.cpp
 * @brief Implementation of state transition helper utilities
 */

#include "StateTransitionHelper.h"
#include "MachineStateContext.h"
#include "states/EmergencyStopState.h"
#include "states/ErrorStates.h"
#include "states/PidNormalState.h"
#include "states/SystemStates.h"

namespace StateTransitionHelper {

std::unique_ptr<MachineState> checkEmergencyStop(MachineStateContext& context, MachineStateId currentStateId) {
    if (context.isEmergencyStop()) {
        context.logStateTransition(currentStateId, MachineStateId::EMERGENCY_STOP, "Emergency stop activated");
        return std::make_unique<EmergencyStopState>();
    }
    return nullptr;
}

std::unique_ptr<MachineState> checkSensorError(MachineStateContext& context, MachineStateId currentStateId) {
    if (context.hasSensorError()) {
        context.logStateTransition(currentStateId, MachineStateId::SENSOR_ERROR, "Sensor error detected");
        return std::make_unique<SensorErrorState>();
    }
    return nullptr;
}

std::unique_ptr<MachineState> checkWaterTankEmpty(MachineStateContext& context, MachineStateId currentStateId) {
    // Only transition to water tank empty if:
    // 1. Tank is not full AND
    // 2. We're not already in water tank empty state AND  
    // 3. We're not in an error state (which has higher priority)
    if (!context.isWaterTankFull() && 
        currentStateId != MachineStateId::WATER_TANK_EMPTY &&
        currentStateId != MachineStateId::EMERGENCY_STOP &&
        currentStateId != MachineStateId::SENSOR_ERROR &&
        currentStateId != MachineStateId::EEPROM_ERROR) {
        
        context.logStateTransition(currentStateId, MachineStateId::WATER_TANK_EMPTY, "Water tank empty detected");
        return std::make_unique<WaterTankEmptyState>();
    }
    return nullptr;
}

std::unique_ptr<MachineState> checkCommonSafetyTransitions(MachineStateContext& context, MachineStateId currentStateId) {
    // Check in priority order
    
    // 1. Emergency stop has highest priority
    if (auto state = checkEmergencyStop(context, currentStateId)) {
        return state;
    }
    
    // 2. Sensor errors
    if (auto state = checkSensorError(context, currentStateId)) {
        return state;
    }
    
    // 3. Water tank empty (unless we're already in an error state)
    if (auto state = checkWaterTankEmpty(context, currentStateId)) {
        return state;
    }
    
    return nullptr;
}

std::unique_ptr<MachineState> getNormalOperationState(MachineStateContext& context, MachineStateId currentStateId) {
    if (context.isPidEnabled()) {
        context.logStateTransition(currentStateId, MachineStateId::PID_NORMAL, "Returning to normal operation");
        return std::make_unique<PidNormalState>();
    } else {
        context.logStateTransition(currentStateId, MachineStateId::PID_DISABLED, "Returning to disabled PID operation");
        return std::make_unique<PidDisabledState>();
    }
}

} // namespace StateTransitionHelper