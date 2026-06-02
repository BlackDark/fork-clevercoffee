/**
 * @file SystemStates.cpp
 * @brief Implementation of system-related state classes
 */

#include "clevercoffee/state/states/SystemStates.h"

#include "clevercoffee/Logger.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/types/GlobalTypes.h"

// SystemStates Implementation
void StandbyState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Entering standby mode - reducing power consumption");
    context.enterStandbyMode();
    context.setSteamMode(false);
    // Safety: Ensure pump and valve are off when entering standby
    context.disablePump();
    context.closeWaterValve();
    // Disable runtime PID (does not modify config - config remains source of truth)
    context.setPidRuntimeState(false);
    // Drain stale stop flags (preserve start flags as wake triggers)
    context.clearStaleStopRequests();
}

void StandbyState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting standby mode - resuming normal operation");
    context.exitStandbyMode();
    // Restore runtime PID state based on config (config is source of truth)
    // getPidState() will check config and return correct state (PID_NORMAL or PID_DISABLED)
    const bool configPidEnabled = context.isPidConfigEnabled();
    context.setPidRuntimeState(configPidEnabled);
}

void StandbyState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Standby: Power saving active, UserActivity=%s, Sensors=%s",
         context.hasUserActivity() ? "DETECTED" : "IDLE",
         context.hasSensorError() ? "ERROR" : "OK");
}

std::optional<MachineStateId> StandbyState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isNormalOperationRequested()) {
        context.setNormalOperationRequested(false);
        context.resetMqttReconnectCount();
        return transitionToPidState(context, "Normal operation requested");
    }
    // Exit standby on any user activity: brew start, steam start, or hot water activity
    if (context.isBrewStartRequested() || context.isSteamStartRequested() || context.hasUserActivity() ||
        context.shouldExitStandby()) {
        context.resetMqttReconnectCount();
        return transitionToPidState(context, "User activity detected - exiting standby");
    }
    return std::nullopt;
}

void ManualFlushRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Manual flush mode activated");
    context.setManualFlushState(true);
    context.enablePump();
    context.openWaterValve();
}

void ManualFlushRunningState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting manual flush mode");
    context.disablePump();
    context.closeWaterValve();
    context.setManualFlushState(false);
}

void ManualFlushRunningState::update(MachineStateContext& context) {
    // Keep pump and valve active during manual flush
    context.enablePump();
    context.openWaterValve();

    LOGF(DEBUG,
         "Manual Flush Running: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::optional<MachineStateId> ManualFlushRunningState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isManualFlushStopRequested()) {
        context.setManualFlushStopRequested(false);
        // Return to backflush idle since manual flush is only available from backflush mode
        if (context.isBackflushModeActive()) {
            context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Manual flush stop requested");
            return MachineStateId::BACKFLUSH_IDLE;
        }
        return transitionToPidState(context, "Manual flush stop requested");
    }
    return std::nullopt;
}