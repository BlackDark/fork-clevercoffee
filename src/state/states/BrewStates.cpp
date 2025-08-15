/**
 * @file BrewStates.cpp
 * @brief All brew-related states implementation
 */

#include "BrewStates.h"
#include "../MachineStateContext.h"
#include "Logger.h"

// BrewIdleState Implementation
void BrewIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew idle - ready to start brewing");
}

void BrewIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Brew Idle: Temp=%.1f°C, Weight=%.1fg, Tank=%s",
         context.getCurrentTemperature(),
         context.getCurrentBrewWeight(),
         context.isWaterTankFull() ? "OK" : "EMPTY");
}

std::unique_ptr<MachineState> BrewIdleState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    
    // Check for brew start request
    if (flags.requestBrewStart) {
        flags.requestBrewStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION, "Brew start requested");
        return std::make_unique<BrewPreinfusionState>();
    }

    // Check if brew switch was activated
    if (context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION, "Brew switch activated");
        return std::make_unique<BrewPreinfusionState>();
    }

    // Check if backflush was activated
    if (context.isBackflushActive() || flags.requestBackflushStart) {
        if (flags.requestBackflushStart) {
            flags.requestBackflushStart = false;
        }
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush activated");
        // Note: BackflushState will be in BackflushStates.h
        return nullptr; // Will be implemented when BackflushStates is created
    }

    return nullptr;
}

// BrewPreinfusionState Implementation
void BrewPreinfusionState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew preinfusion started");
}

void BrewPreinfusionState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Brew Preinfusion: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

std::unique_ptr<MachineState> BrewPreinfusionState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew stop requested during preinfusion");
        return std::make_unique<BrewIdleState>();
    }

    if (!context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew switch deactivated during preinfusion");
        return std::make_unique<BrewIdleState>();
    }

    // TODO: Add preinfusion completion logic based on time/pressure
    return nullptr;
}

// BrewPreinfusionPauseState Implementation
void BrewPreinfusionPauseState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew preinfusion pause - blooming phase");
}

void BrewPreinfusionPauseState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Brew Preinfusion Pause: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

std::unique_ptr<MachineState> BrewPreinfusionPauseState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew stop requested during preinfusion pause");
        return std::make_unique<BrewIdleState>();
    }

    if (!context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew switch deactivated during preinfusion pause");
        return std::make_unique<BrewIdleState>();
    }

    // TODO: Add pause completion logic
    return nullptr;
}

// BrewRunningState Implementation
void BrewRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew running - active brewing in progress");
}

void BrewRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Brew Running: Weight=%.1fg, Temp=%.1f°C, Pressure=%.1fbar", 
         context.getCurrentBrewWeight(),
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> BrewRunningState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew stop requested");
        return std::make_unique<BrewFinishedState>();
    }

    if (!context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew completed");
        return std::make_unique<BrewFinishedState>();
    }

    return nullptr;
}

// BrewFinishedState Implementation
void BrewFinishedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew cycle completed");
}

void BrewFinishedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Brew Finished: Weight=%.1fg, Temp=%.1f°C", 
         context.getCurrentBrewWeight(),
         context.getCurrentTemperature());
}

std::unique_ptr<MachineState> BrewFinishedState::checkSpecificTransitions(MachineStateContext& context) {
    // Automatically transition back to brew idle after a short delay
    static unsigned long finishTime = 0;
    if (finishTime == 0) {
        finishTime = millis();
    }
    
    constexpr unsigned long FINISH_DISPLAY_TIME = 3000; // 3 seconds
    if (millis() - finishTime > FINISH_DISPLAY_TIME) {
        finishTime = 0;
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew finished timeout");
        return std::make_unique<BrewIdleState>();
    }

    return nullptr;
}