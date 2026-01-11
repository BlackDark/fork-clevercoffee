/**
 * @file SteamStates.cpp
 * @brief Implementation of steam-related state classes
 */

#include "clevercoffee/state/states/SteamStates.h"

#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/hardware/Switch.h"

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

std::optional<MachineStateId> SteamRunningState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for steam stop request (flag-based - handlers set this flag)
    // Handlers detect switch changes and set flags, fixing timing issues
    if (context.isSteamStopRequested()) {
        context.setSteamStopRequested(false);
        return transitionToPidState(context, "Steam stop requested");
    }

    // No direct handler checks - handlers set flags, states only check flags
    // This fixes timing issues with direct hardware checks
    return std::nullopt;
}