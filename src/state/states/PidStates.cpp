/**
 * @file PidStates.cpp
 * @brief Implementation of PID-related state classes
 */

#include "clevercoffee/state/states/PidStates.h"

#include "clevercoffee/GlobalState.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"

// PidNormalState Implementation
void PidNormalState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "PID Normal mode active - ready for operation");
    resetStandbyTimerIfNeeded(context);
}

void PidNormalState::update(MachineStateContext& context) {
    resetStandbyTimerIfNeeded(context);
}

MachineState* PidNormalState::checkSpecificTransitions(MachineStateContext& context) {
    if (g_state.machine.flags.requestBrewStart) {
        g_state.machine.flags.requestBrewStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew start requested");
        return getStateInstance(MachineStateId::BREW_IDLE);
    }
    if (g_state.machine.flags.requestHotWaterStart) {
        g_state.machine.flags.requestHotWaterStart = false;
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_IDLE, "Hot water start requested");
        return getStateInstance(MachineStateId::HOT_WATER_IDLE);
    }
    if (g_state.machine.flags.requestSteamStart) {
        g_state.machine.flags.requestSteamStart = false;
        context.logStateTransition(getStateId(), MachineStateId::STEAM_IDLE, "Steam start requested");
        return getStateInstance(MachineStateId::STEAM_IDLE);
    }
    if (g_state.machine.flags.requestManualFlushStart) {
        g_state.machine.flags.requestManualFlushStart = false;
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_IDLE, "Manual flush start requested");
        return getStateInstance(MachineStateId::MANUAL_FLUSH_IDLE);
    }
    if (g_state.machine.flags.requestBackflushStart) {
        g_state.machine.flags.requestBackflushStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush start requested");
        return getStateInstance(MachineStateId::BACKFLUSH_IDLE);
    }
    if (g_state.machine.flags.requestStandby) {
        g_state.machine.flags.requestStandby = false;
        context.logStateTransition(getStateId(), MachineStateId::STANDBY, "Standby requested");
        return getStateInstance(MachineStateId::STANDBY);
    }
    if (shouldEnterStandby(context)) {
        context.logStateTransition(getStateId(), MachineStateId::STANDBY, "Entering standby mode");
        return getStateInstance(MachineStateId::STANDBY);
    }
    return nullptr;
}

bool PidNormalState::shouldEnterStandby(MachineStateContext& context) const {
    return context.shouldEnterStandby();
}

void PidNormalState::resetStandbyTimerIfNeeded(MachineStateContext& context) const {
    context.resetStandbyTimer(getStateId());
}

// PidDisabledState Implementation
void PidDisabledState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "PID disabled - operations without temperature control");
    context.setPidRuntimeState(false);
}

void PidDisabledState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "PID Disabled: Temp=%.1f°C, PidEnabled=%s",
         context.getCurrentTemperature(),
         context.isPidEnabled() ? "YES" : "NO");
}

MachineState* PidDisabledState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isPidEnabled()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "PID enabled");
        return getStateInstance(MachineStateId::PID_NORMAL);
    }
    return nullptr;
}