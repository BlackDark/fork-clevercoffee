/**
 * @file BrewStates.cpp
 * @brief Implementation of brew-related state classes
 */

#include "clevercoffee/state/states/BrewStates.h"

#include "clevercoffee/GlobalState.h"
#include "clevercoffee/constants/BrewTiming.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"

// BrewStates Implementation
void BrewIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew idle - ready to start brewing");
}

void BrewIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Brew Idle: Temp=%.1f°C, Weight=%.1fg, Tank=%s",
         context.getCurrentTemperature(),
         context.getCurrentBrewWeight(),
         context.isWaterTankFull() ? "OK" : "EMPTY");
}

MachineState* BrewIdleState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStart) {
        flags.requestBrewStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION, "Brew start requested");
        return getStateInstance(MachineStateId::BREW_PREINFUSION);
    }
    if (context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION, "Brew switch activated");
        return getStateInstance(MachineStateId::BREW_PREINFUSION);
    }
    if (context.isBackflushActive() || flags.requestBackflushStart) {
        if (flags.requestBackflushStart) {
            flags.requestBackflushStart = false;
        }
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush activated");
        return getStateInstance(MachineStateId::BACKFLUSH_IDLE);
    }
    return nullptr;
}

void BrewPreinfusionState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew preinfusion started");
}

void BrewPreinfusionState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Brew Preinfusion: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

MachineState* BrewPreinfusionState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew stop requested during preinfusion");
        return getStateInstance(MachineStateId::BREW_IDLE);
    }
    if (!context.isBrewActive()) {
        context.logStateTransition(
            getStateId(), MachineStateId::BREW_IDLE, "Brew switch deactivated during preinfusion");
        return getStateInstance(MachineStateId::BREW_IDLE);
    }
    return nullptr;
}

void BrewPreinfusionPauseState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew preinfusion pause - blooming phase");
}

void BrewPreinfusionPauseState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Brew Preinfusion Pause: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

MachineState* BrewPreinfusionPauseState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false;
        context.logStateTransition(
            getStateId(), MachineStateId::BREW_IDLE, "Brew stop requested during preinfusion pause");
        return getStateInstance(MachineStateId::BREW_IDLE);
    }
    if (!context.isBrewActive()) {
        context.logStateTransition(
            getStateId(), MachineStateId::BREW_IDLE, "Brew switch deactivated during preinfusion pause");
        return getStateInstance(MachineStateId::BREW_IDLE);
    }
    return nullptr;
}

void BrewRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew running - active brewing in progress");
}

void BrewRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Brew Running: Weight=%.1fg, Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentBrewWeight(),
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

MachineState* BrewRunningState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew stop requested");
        return getStateInstance(MachineStateId::BREW_FINISHED);
    }
    if (!context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew completed");
        return getStateInstance(MachineStateId::BREW_FINISHED);
    }
    return nullptr;
}

void BrewFinishedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew cycle completed");
}

void BrewFinishedState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Brew Finished: Weight=%.1fg, Temp=%.1f°C",
         context.getCurrentBrewWeight(),
         context.getCurrentTemperature());
}

MachineState* BrewFinishedState::checkSpecificTransitions(MachineStateContext& context) {
    // Use a hardcoded 3 second timeout for the finished state (display time)
    if (context.hasStateTimeoutElapsed(CleverCoffee::BrewTiming::FINISHED_DISPLAY_TIMEOUT_MS)) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew finished display timeout");
        return getStateInstance(MachineStateId::BREW_IDLE);
    }

    return nullptr;
}