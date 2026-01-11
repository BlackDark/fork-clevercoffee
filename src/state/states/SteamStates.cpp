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
#include "clevercoffee/handlers/SteamHandler.h"
#include "clevercoffee/context/SystemContext.h"

// SteamStates Implementation
void SteamRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Steam mode activated - actively steaming");
    context.setSteamMode(true);
}

void SteamRunningState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting steam mode");
    context.setSteamMode(false);
    // Safety: Disable water injection pump when exiting steam mode
    context.disablePump();
    LOG(DEBUG, "Steam running exit - pump disabled");
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
    // Check for steam stop request
    if (context.isSteamStopRequested()) {
        context.setSteamStopRequested(false);
        return transitionToPidState(context, "Steam stop requested");
    }

    // Check switch state directly instead of handler flag
    auto& steamHandler = context.systemContext().steamHandler();
    const auto switchType = steamHandler.getSwitchType();

    // Check if switch state changed
    if (steamHandler.hasSwitchStateChanged()) {
        const bool wasReleased = steamHandler.wasSwitchReleased();
        steamHandler.clearSwitchStateChange();

        if (wasReleased) {
            return transitionToPidState(context, "Steam switch deactivated");
        }
    }

    // For toggle: if switch is OFF, transition back to PID
    if (switchType == Hardware::SwitchType::TOGGLE && !steamHandler.isSteamSwitchPressed()) {
        return transitionToPidState(context, "Steam toggle switch OFF");
    }

    return nullptr;
}