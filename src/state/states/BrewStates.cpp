/**
 * @file BrewStates.cpp
 * @brief Implementation of brew-related state classes
 */

#include "clevercoffee/state/states/BrewStates.h"

#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"
#include "clevercoffee/Config.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/handlers/BrewHandler.h"

// BrewStates Implementation
void BrewPreinfusionState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew preinfusion started");
}

void BrewPreinfusionState::onExitImpl(MachineStateContext& context) {
    // Safety: Ensure pump and valve are disabled when exiting preinfusion
    cleanupPumpAndValve(context);
    LOG(DEBUG, "Brew preinfusion exit - hardware cleaned up");
}

void BrewPreinfusionState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Brew Preinfusion: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

MachineState* BrewPreinfusionState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for brew stop request (common pattern)
    if (auto* stopState = checkBrewStopRequest(context)) {
        return stopState;
    }
    
    auto& brewHandler = context.systemContext().brewHandler();
    const auto switchType = brewHandler.getSwitchType();
    
    // Check if switch state changed
    if (brewHandler.hasSwitchStateChanged()) {
        const bool wasReleased = brewHandler.wasSwitchReleased();
        brewHandler.clearSwitchStateChange();
        
                    if (wasReleased) {
                        return transitionToPidState(context, "Brew switch deactivated during preinfusion");
                    }
    }
    
    // Check if brew should continue based on switch type
    bool shouldContinue = false;
    if (switchType == Hardware::SwitchType::TOGGLE) {
        // Toggle: continue as long as switch is ON
        shouldContinue = brewHandler.isBrewSwitchPressed();
    } else {  // MOMENTARY
        // Momentary: continue if brew is active (in a brew state)
        shouldContinue = context.isBrewActive();
    }
    
                if (!shouldContinue) {
                    return transitionToPidState(context, "Brew switch deactivated during preinfusion");
                }
    return nullptr;
}

void BrewPreinfusionPauseState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew preinfusion pause - blooming phase");
}

void BrewPreinfusionPauseState::onExitImpl(MachineStateContext& context) {
    // Safety: Ensure pump and valve are disabled when exiting preinfusion pause
    cleanupPumpAndValve(context);
    LOG(DEBUG, "Brew preinfusion pause exit - hardware cleaned up");
}

void BrewPreinfusionPauseState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Brew Preinfusion Pause: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

MachineState* BrewPreinfusionPauseState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for brew stop request (common pattern)
    if (auto* stopState = checkBrewStopRequest(context)) {
        return stopState;
    }
    
    auto& brewHandler = context.systemContext().brewHandler();
    const auto switchType = brewHandler.getSwitchType();
    
    // Check if switch state changed
    if (brewHandler.hasSwitchStateChanged()) {
        const bool wasReleased = brewHandler.wasSwitchReleased();
        brewHandler.clearSwitchStateChange();
        
        if (wasReleased) {
            return transitionToPidState(context, "Brew switch deactivated during preinfusion pause");
        }
    }
    
    // Check if brew should continue based on switch type
    bool shouldContinue = false;
    if (switchType == Hardware::SwitchType::TOGGLE) {
        // Toggle: continue as long as switch is ON
        shouldContinue = brewHandler.isBrewSwitchPressed();
    } else {  // MOMENTARY
        // Momentary: continue if brew is active (in a brew state)
        shouldContinue = context.isBrewActive();
    }
    
        if (!shouldContinue) {
            return transitionToPidState(context, "Brew switch deactivated during preinfusion pause");
        }
    return nullptr;
}

void BrewRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew running - active brewing in progress");
}

void BrewRunningState::onExitImpl(MachineStateContext& context) {
    // Safety: Ensure pump and valve are disabled when exiting brew running state
    context.disablePump();
    context.closeWaterValve();
    LOG(DEBUG, "Brew running exit - hardware cleaned up");
}

void BrewRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Brew Running: Weight=%.1fg, Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentBrewWeight(),
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

MachineState* BrewRunningState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for brew stop request - transition to finished state
    if (context.isBrewStopRequested()) {
        context.setBrewStopRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew stop requested");
        return getStateInstance(MachineStateId::BREW_FINISHED);
    }
    
    auto& brewHandler = context.systemContext().brewHandler();
    const auto switchType = brewHandler.getSwitchType();
    
    // Check if switch state changed
    if (brewHandler.hasSwitchStateChanged()) {
        const bool wasPressed = brewHandler.wasSwitchPressed();
        const bool wasReleased = brewHandler.wasSwitchReleased();
        brewHandler.clearSwitchStateChange();
        
        if (switchType == Hardware::SwitchType::TOGGLE && wasReleased) {
            // Toggle: switch released - stop brew
            context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew toggle switch deactivated");
            return getStateInstance(MachineStateId::BREW_FINISHED);
        } else if (switchType == Hardware::SwitchType::MOMENTARY && wasPressed) {
            // Momentary: switch pressed again - stop brew
            context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew momentary switch - stopping brew");
            return getStateInstance(MachineStateId::BREW_FINISHED);
        }
    }
    
    // Check if brew should continue based on switch type
    bool shouldContinue = false;
    if (switchType == Hardware::SwitchType::TOGGLE) {
        // Toggle: continue as long as switch is ON
        shouldContinue = brewHandler.isBrewSwitchPressed();
    } else {  // MOMENTARY
        // Momentary: continue if brew is active (in a brew state)
        shouldContinue = context.isBrewActive();
    }
    
    if (!shouldContinue) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew completed");
        return getStateInstance(MachineStateId::BREW_FINISHED);
    }
    return nullptr;
}

void BrewFinishedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew cycle completed");
}

void BrewFinishedState::onExitImpl(MachineStateContext& context) {
    // Safety: Ensure pump and valve are disabled when exiting brew finished state
    context.disablePump();
    context.closeWaterValve();
    LOG(DEBUG, "Brew finished exit - hardware cleaned up");
}

void BrewFinishedState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Brew Finished: Weight=%.1fg, Temp=%.1f°C",
         context.getCurrentBrewWeight(),
         context.getCurrentTemperature());
}

MachineState* BrewFinishedState::checkSpecificTransitions(MachineStateContext& context) {
    auto& brewHandler = context.systemContext().brewHandler();
    const auto switchType = brewHandler.getSwitchType();
    
    // For toggle switch: also allow transition if switch state changed
    if (switchType == Hardware::SwitchType::TOGGLE && brewHandler.hasSwitchStateChanged()) {
        brewHandler.clearSwitchStateChange();
        // If toggle is ON, start new brew; if OFF, return to PID
        if (brewHandler.isBrewSwitchPressed()) {
            context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION, "Brew finished - toggle switch ON, starting new brew");
            return getStateInstance(MachineStateId::BREW_PREINFUSION);
        } else {
            return transitionToPidState(context, "Brew finished - toggle switch OFF, returning to PID");
        }
    }
    
    // Use a hardcoded 3 second timeout for the finished state (display time)
    if (context.hasStateTimeoutElapsed(CleverCoffee::BrewTiming::FINISHED_DISPLAY_TIMEOUT_MS)) {
        // After timeout, check switch state: if toggle ON, start new brew; otherwise return to PID
        if (switchType == Hardware::SwitchType::TOGGLE && brewHandler.isBrewSwitchPressed()) {
            context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION, "Brew finished timeout - toggle switch ON, starting new brew");
            return getStateInstance(MachineStateId::BREW_PREINFUSION);
        } else {
            return transitionToPidState(context, "Brew finished display timeout");
        }
    }

    return nullptr;
}