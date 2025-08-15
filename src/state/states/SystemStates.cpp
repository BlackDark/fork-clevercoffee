/**
 * @file SystemStates.cpp
 * @brief System states implementation
 */

#include "clevercoffee/state/states/SystemStates.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateTransitionHelper.h"
#include "clevercoffee/Logger.h"

// Forward declaration for states that will be in other files
class PidNormalState;

// PidDisabledState Implementation
void PidDisabledState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "PID disabled - operations without temperature control");
    context.setPidRuntimeState(false);
}

void PidDisabledState::update(MachineStateContext& context) {
    LOGF(DEBUG, "PID Disabled: Temp=%.1f°C, PidEnabled=%s",
         context.getCurrentTemperature(),
         context.isPidEnabled() ? "YES" : "NO");
}

std::unique_ptr<MachineState> PidDisabledState::checkSpecificTransitions(MachineStateContext& context) {
    // Check if PID should be enabled
    if (context.isPidEnabled()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "PID enabled");
        // Will need to return PidNormalState - placeholder for now
        return nullptr;
    }

    return nullptr;
}

// StandbyState Implementation
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
    LOGF(DEBUG, "Standby: Power saving active, UserActivity=%s, Sensors=%s",
         context.hasUserActivity() ? "DETECTED" : "IDLE",
         context.hasSensorError() ? "ERROR" : "OK");
}

std::unique_ptr<MachineState> StandbyState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestNormalOperation) {
        flags.requestNormalOperation = false;
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Normal operation requested");
        context.resetMqttReconnectCount();
        // Will need to return PidNormalState - placeholder for now
        return nullptr;
    }

    // Check for user activity to wake up from standby
    if (context.hasUserActivity() || context.shouldExitStandby()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "User activity detected - exiting standby");
        context.resetMqttReconnectCount();
        // Will need to return PidNormalState - placeholder for now
        return nullptr;
    }

    return nullptr;
}

// ManualFlushIdleState Implementation
void ManualFlushIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Manual flush mode activated");
    context.setManualFlushState(true);
}

void ManualFlushIdleState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting manual flush mode");
    context.setManualFlushState(false);
}

void ManualFlushIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Manual Flush: Temp=%.1f°C, Tank=%s, FlushActive=%s",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.isManualFlushActive() ? "YES" : "NO");
}

std::unique_ptr<MachineState> ManualFlushIdleState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestManualFlushStop) {
        flags.requestManualFlushStop = false;
        return StateTransitionHelper::getNormalOperationState(context, getStateId());
    }

    // Check if manual flush switch was deactivated
    if (!context.isManualFlushActive()) {
        return StateTransitionHelper::getNormalOperationState(context, getStateId());
    }

    return nullptr;
}

// ManualFlushRunningState Implementation
void ManualFlushRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Manual flush running - hardware controlled by handlers");
}

void ManualFlushRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Manual Flush Running: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> ManualFlushRunningState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestManualFlushStop) {
        flags.requestManualFlushStop = false;
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_IDLE, "Manual flush stop requested");
        return std::make_unique<ManualFlushIdleState>();
    }

    // Check if manual flush was deactivated
    if (!context.isManualFlushActive()) {
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_IDLE, "Manual flush deactivated");
        return std::make_unique<ManualFlushIdleState>();
    }

    return nullptr;
}
