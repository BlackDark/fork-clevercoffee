/**
 * @file BackflushStates.cpp
 * @brief Implementation of backflush-related state classes
 */

#include "clevercoffee/state/states/BackflushStates.h"

#include "clevercoffee/GlobalState.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"

// BackflushStates Implementation
void BackflushState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Backflush idle - ready for backflush operation");
}

void BackflushState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Idle: Temp=%.1f°C", context.getCurrentTemperature());
}

MachineState* BackflushState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBackflushStart) {
        flags.requestBackflushStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FILLING, "Backflush start requested");
        return getStateInstance(MachineStateId::BACKFLUSH_FILLING);
    }
    return nullptr;
}

void BackflushFillingState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Backflush Filling: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

MachineState* BackflushFillingState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for manual stop request
    auto& flags = g_state.machine.flags;
    if (flags.requestBackflushStop) {
        flags.requestBackflushStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush stop requested");
        return getStateInstance(MachineStateId::BACKFLUSH_IDLE);
    }

    // Check timeout using config-based fill time
    if (context.hasStateTimeoutElapsed(context.getBackflushFillTimeMs())) {
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FLUSHING, "Fill time completed");
        return getStateInstance(MachineStateId::BACKFLUSH_FLUSHING);
    }

    return nullptr;
}

void BackflushFlushingState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Backflush Flushing: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

MachineState* BackflushFlushingState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for manual stop request
    auto& flags = g_state.machine.flags;
    if (flags.requestBackflushStop) {
        flags.requestBackflushStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush stop requested");
        return getStateInstance(MachineStateId::BACKFLUSH_IDLE);
    }

    // Check timeout using config-based flush time
    if (context.hasStateTimeoutElapsed(context.getBackflushFlushTimeMs())) {
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FINISHED, "Flush time completed");
        return getStateInstance(MachineStateId::BACKFLUSH_FINISHED);
    }

    return nullptr;
}

void BackflushFinishedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Backflush finished - backflush cycle complete");
}

void BackflushFinishedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Finished: Temp=%.1f°C", context.getCurrentTemperature());
}

MachineState* BackflushFinishedState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for manual stop request
    auto& flags = g_state.machine.flags;
    if (flags.requestBackflushStop) {
        flags.requestBackflushStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush stop requested");
        return getStateInstance(MachineStateId::BACKFLUSH_IDLE);
    }

    // Use a hardcoded 3 second timeout for the finished state (display time)
    if (context.hasStateTimeoutElapsed(3000)) {
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Finished display timeout");
        return getStateInstance(MachineStateId::BACKFLUSH_IDLE);
    }

    return nullptr;
}