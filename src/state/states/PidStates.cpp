/**
 * @file PidStates.cpp
 * @brief Implementation of PID-related state classes
 */

#include "clevercoffee/state/states/PidStates.h"

#include "clevercoffee/GlobalState.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"
#include "clevercoffee/hardware/Switch.h"

// PidNormalState Implementation
void PidNormalState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "PID Normal mode active - ready for operation");
    resetStandbyTimerIfNeeded(context);
}

void PidNormalState::update(MachineStateContext& context) {
    // Only reset standby timer occasionally to avoid spam
    static unsigned long lastStandbyReset = 0;
    const unsigned long currentTime = context.getCurrentTime();
    
    // Reset standby timer at most once every 30 seconds to avoid log spam
    if (currentTime - lastStandbyReset >= 30000) {
        resetStandbyTimerIfNeeded(context);
        lastStandbyReset = currentTime;
    }
    
    // FEATURE: Handle water dispensing from water switch in PID mode
    // User can dispense hot water directly without entering HOT_WATER mode
    auto* waterSwitch = context.getHotWaterSwitch();
    if (waterSwitch) {
        if (waterSwitch->isPressed()) {
            // User pressed water switch - activate pump to dispense hot water
            context.enablePump();
            LOGF(DEBUG, "PID: Water dispensing active");
        } else {
            // User released water switch - deactivate pump
            context.disablePump();
        }
    }
}

MachineState* PidNormalState::checkSpecificTransitions(MachineStateContext& context) {
    // CRITICAL: Check if PID was disabled while in this state
    if (!context.isPidEnabled()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, "PID disabled");
        return getStateInstance(MachineStateId::PID_DISABLED);
    }
    
    if (context.isBrewStartRequested()) {
        context.setBrewStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew start requested");
        return getStateInstance(MachineStateId::BREW_IDLE);
    }
    if (context.isHotWaterStartRequested()) {
        context.setHotWaterStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_IDLE, "Hot water start requested");
        return getStateInstance(MachineStateId::HOT_WATER_IDLE);
    }
    if (context.isSteamStartRequested()) {
        context.setSteamStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::STEAM_IDLE, "Steam start requested");
        return getStateInstance(MachineStateId::STEAM_IDLE);
    }
    if (context.isManualFlushStartRequested()) {
        context.setManualFlushStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_IDLE, "Manual flush start requested");
        return getStateInstance(MachineStateId::MANUAL_FLUSH_IDLE);
    }
    if (context.isBackflushStartRequested()) {
        context.setBackflushStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush start requested");
        return getStateInstance(MachineStateId::BACKFLUSH_IDLE);
    }
    if (context.isStandbyRequested()) {
        context.setStandbyRequested(false);
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
    // Safety: Disable pump when PID is disabled to prevent runaway water dispensing
    context.disablePump();
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