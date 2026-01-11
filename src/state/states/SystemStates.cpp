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
        context.resetMqttReconnectCount();
        return transitionToPidState(context, "Normal operation requested");
    }
    if (context.hasUserActivity() || context.shouldExitStandby()) {
        context.resetMqttReconnectCount();
        return transitionToPidState(context, "User activity detected - exiting standby");
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
        return transitionToPidState(context, "Manual flush stop requested");
    }
    if (!context.isManualFlushActive()) {
        return transitionToPidState(context, "Manual flush deactivated");
    }
    return nullptr;
}