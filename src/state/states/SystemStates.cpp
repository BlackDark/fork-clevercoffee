/**
 * @file SystemStates.cpp
 * @brief Implementation of system-related state classes
 */

#include "clevercoffee/state/states/SystemStates.h"

#include "clevercoffee/GlobalState.h"
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

void ManualFlushIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Manual flush mode activated");
    context.setManualFlushState(true);
}

void ManualFlushIdleState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting manual flush mode");
    context.setManualFlushState(false);
}

void ManualFlushIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Manual Flush: Temp=%.1f°C, Tank=%s, FlushActive=%s",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.isManualFlushActive() ? "YES" : "NO");
}

MachineState* ManualFlushIdleState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isManualFlushStopRequested()) {
        context.setManualFlushStopRequested(false);
        if (context.isPidEnabled()) {
            return getStateInstance(MachineStateId::PID_NORMAL);
        } else {
            return getStateInstance(MachineStateId::PID_DISABLED);
        }
    }
    if (!context.isManualFlushActive()) {
        if (context.isPidEnabled()) {
            return getStateInstance(MachineStateId::PID_NORMAL);
        } else {
            return getStateInstance(MachineStateId::PID_DISABLED);
        }
    }
    return nullptr;
}

void ManualFlushRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Manual flush running - hardware controlled by handlers");
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
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_IDLE, "Manual flush stop requested");
        return getStateInstance(MachineStateId::MANUAL_FLUSH_IDLE);
    }
    if (!context.isManualFlushActive()) {
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_IDLE, "Manual flush deactivated");
        return getStateInstance(MachineStateId::MANUAL_FLUSH_IDLE);
    }
    return nullptr;
}