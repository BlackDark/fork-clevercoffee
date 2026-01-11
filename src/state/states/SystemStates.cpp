/**
 * @file SystemStates.cpp
 * @brief Implementation of system-related state classes
 */

#include "clevercoffee/state/states/SystemStates.h"

#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"

// SystemStates Implementation
void StandbyState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Entering standby mode - reducing power consumption");
    context.enterStandbyMode();
    context.setPidRuntimeState(false);
}

void StandbyState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting standby mode - resuming normal operation");
    context.exitStandbyMode();
    context.setPidRuntimeState(true);
}

void StandbyState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Standby: Power saving active, UserActivity=%s, Sensors=%s",
         context.hasUserActivity() ? "DETECTED" : "IDLE",
         context.hasSensorError() ? "ERROR" : "OK");
}

MachineState* StandbyState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isNormalOperationRequested()) {
        context.setNormalOperationRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Normal operation requested");
        context.resetMqttReconnectCount();
        return getStateInstance(MachineStateId::PID_NORMAL);
    }
    if (context.hasUserActivity() || context.shouldExitStandby()) {
        context.logStateTransition(
            getStateId(), MachineStateId::PID_NORMAL, "User activity detected - exiting standby");
        context.resetMqttReconnectCount();
        return getStateInstance(MachineStateId::PID_NORMAL);
    }
    return nullptr;
}

void ManualFlushRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Manual flush mode activated - hardware controlled by handlers");
    context.setManualFlushState(true);
}

void ManualFlushRunningState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting manual flush mode");
    context.setManualFlushState(false);
}

void ManualFlushRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Manual Flush Running: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

MachineState* ManualFlushRunningState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isManualFlushStopRequested()) {
        context.setManualFlushStopRequested(false);
        if (context.isPidEnabled()) {
            context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Manual flush stop requested");
            return getStateInstance(MachineStateId::PID_NORMAL);
        } else {
            context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, "Manual flush stop requested");
            return getStateInstance(MachineStateId::PID_DISABLED);
        }
    }
    if (!context.isManualFlushActive()) {
        if (context.isPidEnabled()) {
            context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Manual flush deactivated");
            return getStateInstance(MachineStateId::PID_NORMAL);
        } else {
            context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, "Manual flush deactivated");
            return getStateInstance(MachineStateId::PID_DISABLED);
        }
    }
    return nullptr;
}