/**
 * @file PidNormalState.cpp
 * @brief Implementation of PidNormalState for normal operation
 */

#include "PidNormalState.h"
#include "../MachineStateContext.h"
#include "Logger.h"
#include "BrewState.h"
#include "HotWaterState.h"
#include "SteamState.h"
#include "BackflushState.h"
#include "ManualFlushState.h"
#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "StandbyState.h"

void PidNormalState::onEntry(MachineStateContext& context) {
    context.logStateEntry(static_cast<int>(getStateId()), getStateName());
    LOG(INFO, "PID Normal mode active - ready for operation");
    resetStandbyTimerIfNeeded(context);
}

void PidNormalState::onExit(MachineStateContext& context) {
    context.logStateExit(static_cast<int>(getStateId()), getStateName());
}

void PidNormalState::update(MachineStateContext& context) {
    // Monitor system status and user inputs
    // Actual control logic is handled by other components
    resetStandbyTimerIfNeeded(context);
}

std::unique_ptr<MachineState> PidNormalState::checkTransitions(MachineStateContext& context) {
    // Check for emergency conditions first
    if (checkEmergencyConditions(context)) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::EMERGENCY_STOP), "Emergency condition detected");
        return std::make_unique<EmergencyStopState>();
    }
    
    // Check for sensor errors
    if (checkSystemErrors(context)) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::SENSOR_ERROR), "System error detected");
        return std::make_unique<SensorErrorState>();
    }
    
    // Check for standby conditions
    if (shouldEnterStandby(context)) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::STANDBY), "Entering standby mode");
        return std::make_unique<StandbyState>();
    }
    
    // Continue in normal mode
    return nullptr;
}

bool PidNormalState::checkEmergencyConditions(MachineStateContext& context) const {
    return context.isEmergencyStop();
}

bool PidNormalState::shouldEnterStandby(MachineStateContext& context) const {
    return context.shouldEnterStandby();
}

bool PidNormalState::checkSystemErrors(MachineStateContext& context) const {
    return context.hasSensorError();
}

void PidNormalState::resetStandbyTimerIfNeeded(MachineStateContext& context) const {
    // Reset standby timer to prevent automatic sleep during active operation
    context.resetStandbyTimer(static_cast<int>(getStateId()));
}