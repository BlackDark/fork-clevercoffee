/**
 * @file BackflushStates.cpp
 * @brief All backflush-related states implementation
 */

#include "clevercoffee/state/states/BackflushStates.h"
#include "clevercoffee/state/states/clevercoffee/MachineStateContext.h"
#include "clevercoffee/Logger.h"

// BackflushState Implementation
void BackflushState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Backflush idle - ready for backflush operation");
}

void BackflushState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Idle: Temp=%.1f°C", context.getCurrentTemperature());
}

std::unique_ptr<MachineState> BackflushState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBackflushStart) {
        flags.requestBackflushStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FILLING, "Backflush start requested");
        return std::make_unique<BackflushFillingState>();
    }

    return nullptr;
}

// BackflushFillingState Implementation
void BackflushFillingState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Backflush filling - filling group with water");
}

void BackflushFillingState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Filling: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> BackflushFillingState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBackflushStop) {
        flags.requestBackflushStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush stop requested");
        return std::make_unique<BackflushState>();
    }

    // Check if filling time elapsed or pressure reached
    static unsigned long fillStartTime = 0;
    if (fillStartTime == 0) {
        fillStartTime = millis();
    }

    constexpr unsigned long FILL_TIME = 5000; // 5 seconds
    constexpr float FILL_PRESSURE = 8.0f; // 8 bar

    if (millis() - fillStartTime > FILL_TIME || context.getFilteredPressure() >= FILL_PRESSURE) {
        fillStartTime = 0;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FLUSHING, "Filling complete");
        return std::make_unique<BackflushFlushingState>();
    }

    return nullptr;
}

// BackflushFlushingState Implementation
void BackflushFlushingState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Backflush flushing - actively flushing system");
}

void BackflushFlushingState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Flushing: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> BackflushFlushingState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBackflushStop) {
        flags.requestBackflushStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush stop requested");
        return std::make_unique<BackflushState>();
    }

    // Check if flushing time elapsed
    static unsigned long flushStartTime = 0;
    if (flushStartTime == 0) {
        flushStartTime = millis();
    }

    constexpr unsigned long FLUSH_TIME = 10000; // 10 seconds

    if (millis() - flushStartTime > FLUSH_TIME) {
        flushStartTime = 0;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FINISHED, "Flushing complete");
        return std::make_unique<BackflushFinishedState>();
    }

    return nullptr;
}

// BackflushFinishedState Implementation
void BackflushFinishedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Backflush finished - backflush cycle complete");
}

void BackflushFinishedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Finished: Temp=%.1f°C", context.getCurrentTemperature());
}

std::unique_ptr<MachineState> BackflushFinishedState::checkSpecificTransitions(MachineStateContext& context) {
    // Automatically transition back to backflush idle after brief delay
    static unsigned long finishTime = 0;
    if (finishTime == 0) {
        finishTime = millis();
    }

    constexpr unsigned long FINISH_DISPLAY_TIME = 3000; // 3 seconds
    if (millis() - finishTime > FINISH_DISPLAY_TIME) {
        finishTime = 0;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush finish timeout");
        return std::make_unique<BackflushState>();
    }

    return nullptr;
}
