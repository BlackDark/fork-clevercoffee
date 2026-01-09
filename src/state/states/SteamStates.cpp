/**
 * @file SteamStates.cpp
 * @brief Implementation of steam-related state classes
 */

#include "clevercoffee/state/states/SteamStates.h"

#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"
#include "clevercoffee/hardware/Switch.h"

// SteamStates Implementation
void SteamIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Steam mode activated");
    context.setSteamMode(true);
}

void SteamIdleState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting steam mode");
    context.setSteamMode(false);
    // Safety: Disable water injection pump when exiting steam mode
    context.disablePump();
}

void SteamIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Steam: Temp=%.1f°C, Tank=%s, SteamActive=%s",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.isSteamActive() ? "YES" : "NO");
}

MachineState* SteamIdleState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isSteamStopRequested()) {
        context.setSteamStopRequested(false);
        if (context.isPidEnabled()) {
            return getStateInstance(MachineStateId::PID_NORMAL);
        } else {
            return getStateInstance(MachineStateId::PID_DISABLED);
        }
    }
    if (!context.isSteamActive()) {
        if (context.isPidEnabled()) {
            return getStateInstance(MachineStateId::PID_NORMAL);
        } else {
            return getStateInstance(MachineStateId::PID_DISABLED);
        }
    }
    return nullptr;
}

void SteamRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Steam running - actively steaming");
}

void SteamRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Steam Running: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
    
    // FEATURE: Handle water injection during steam mode
    // When user presses water switch (hot water switch repurposed as water injection switch),
    // activate pump to inject water into boiler
    auto* waterSwitch = context.getHotWaterSwitch();
    if (waterSwitch) {
        if (waterSwitch->isPressed()) {
            // User pressed water injection switch - activate pump
            context.enablePump();
            LOGF(DEBUG, "Steam: Water injection active");
        } else {
            // User released water injection switch - deactivate pump
            context.disablePump();
        }
    }
}

MachineState* SteamRunningState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isSteamStopRequested()) {
        context.setSteamStopRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::STEAM_STOPPED, "Steam stop requested");
        return getStateInstance(MachineStateId::STEAM_STOPPED);
    }
    if (!context.isSteamActive()) {
        context.logStateTransition(getStateId(), MachineStateId::STEAM_STOPPED, "Steam deactivated");
        return getStateInstance(MachineStateId::STEAM_STOPPED);
    }
    return nullptr;
}

void SteamStoppedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Steam stopped - steaming complete");
    // Safety: Disable water injection pump when stopping steam
    context.disablePump();
}

void SteamStoppedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Steam Stopped: Temp=%.1f°C", context.getCurrentTemperature());
}

MachineState* SteamStoppedState::checkSpecificTransitions(MachineStateContext& context) {
    // Use a hardcoded 2 second timeout for the stopped state (display time)
    if (context.hasStateTimeoutElapsed(2000)) {
        context.logStateTransition(getStateId(), MachineStateId::STEAM_IDLE, "Steam stopped display timeout");
        return getStateInstance(MachineStateId::STEAM_IDLE);
    }

    return nullptr;
}