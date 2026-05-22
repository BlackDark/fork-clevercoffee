/**
 * @file BackflushStates.cpp
 * @brief Implementation of backflush-related state classes
 */

#include "clevercoffee/state/states/BackflushStates.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/backflush/BackflushModeLogic.h"
#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/types/GlobalTypes.h"

namespace {

std::optional<MachineStateId> checkBackflushModeDisabled(MachineStateContext& context, MachineStateId fromStateId) {
    if (context.isBackflushModeActive()) {
        return std::nullopt;
    }
    context.setBackflushStopRequested(false);
    const MachineStateId pidState = context.getPidState();
    context.logStateTransition(fromStateId, pidState, "Backflush mode disabled");
    return pidState;
}

} // namespace

// BackflushStates Implementation
void BackflushState::onEntryImpl(MachineStateContext& context) {
    cleanupPumpAndValve(context);
    LOG(INFO, "Backflush idle - ready for backflush operation");
}

void BackflushState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Idle: Temp=%.1f°C", context.getCurrentTemperature());
}

std::optional<MachineStateId> BackflushState::checkSpecificTransitions(MachineStateContext& context) {
    if (auto disabled = checkBackflushModeDisabled(context, getStateId())) {
        return disabled;
    }

    if (context.isManualFlushStartRequested()) {
        context.setManualFlushStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_RUNNING, "Manual flush start requested");
        return MachineStateId::MANUAL_FLUSH_RUNNING;
    }

    if (context.isBackflushCycleStartRequested()) {
        context.setBackflushCycleStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FILLING, "Backflush cycle start requested");
        return MachineStateId::BACKFLUSH_FILLING;
    }
    return std::nullopt;
}

void BackflushFillingState::onEntryImpl(MachineStateContext& context) {
    context.enablePump();
    context.openWaterValve();
    LOGF(INFO, "Backflush: filling portafilter (cycle %d)", context.getBackflushCycleCount());
}

void BackflushFillingState::onExitImpl(MachineStateContext& context) {
    cleanupPumpAndValve(context);
}

void BackflushFillingState::update(MachineStateContext& context) {
    context.enablePump();
    context.openWaterValve();
    LOGF(DEBUG,
         "Backflush Filling: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::optional<MachineStateId> BackflushFillingState::checkSpecificTransitions(MachineStateContext& context) {
    if (auto disabled = checkBackflushModeDisabled(context, getStateId())) {
        return disabled;
    }

    if (context.isBackflushStopRequested()) {
        context.setBackflushStopRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush stop requested");
        return MachineStateId::BACKFLUSH_IDLE;
    }

    if (context.hasStateTimeoutElapsed(context.getBackflushFillTimeMs())) {
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FLUSHING, "Fill time completed");
        return MachineStateId::BACKFLUSH_FLUSHING;
    }

    return std::nullopt;
}

void BackflushFlushingState::onEntryImpl(MachineStateContext& context) {
    cleanupPumpAndValve(context);
    LOG(INFO, "Backflush: flushing into drip tray");
}

void BackflushFlushingState::onExitImpl(MachineStateContext& context) {
    cleanupPumpAndValve(context);
}

void BackflushFlushingState::update(MachineStateContext& context) {
    cleanupPumpAndValve(context);
    LOGF(DEBUG,
         "Backflush Flushing: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::optional<MachineStateId> BackflushFlushingState::checkSpecificTransitions(MachineStateContext& context) {
    if (auto disabled = checkBackflushModeDisabled(context, getStateId())) {
        return disabled;
    }

    if (context.isBackflushStopRequested()) {
        context.setBackflushStopRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush stop requested");
        return MachineStateId::BACKFLUSH_IDLE;
    }

    if (context.hasStateTimeoutElapsed(context.getBackflushFlushTimeMs())) {
        const int configuredCycles = context.getBackflushCycles();
        if (CleverCoffee::Backflush::resolveCycleAdvance(context.getBackflushCycleCount(), configuredCycles) ==
            CleverCoffee::Backflush::CycleAdvanceEffect::StartNextCycle) {
            context.setBackflushCycleCount(context.getBackflushCycleCount() + 1);
            context.logStateTransition(
                getStateId(), MachineStateId::BACKFLUSH_FILLING, "Starting next backflush cycle");
            return MachineStateId::BACKFLUSH_FILLING;
        }

        context.setBackflushCycleCount(1);
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FINISHED, "All backflush cycles completed");
        return MachineStateId::BACKFLUSH_FINISHED;
    }

    return std::nullopt;
}

void BackflushFinishedState::onEntryImpl(MachineStateContext& context) {
    cleanupPumpAndValve(context);
    context.systemContext().maintenanceCoordinator().resetSinceBackflush();
    LOG(INFO, "Backflush finished - backflush cycle complete");
}

void BackflushFinishedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Finished: Temp=%.1f°C", context.getCurrentTemperature());
}

std::optional<MachineStateId> BackflushFinishedState::checkSpecificTransitions(MachineStateContext& context) {
    if (auto disabled = checkBackflushModeDisabled(context, getStateId())) {
        return disabled;
    }

    if (context.isBackflushStopRequested()) {
        context.setBackflushStopRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush stop requested");
        return MachineStateId::BACKFLUSH_IDLE;
    }

    if (context.isBackflushCycleStartRequested()) {
        context.setBackflushCycleStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FILLING, "Backflush cycle start requested");
        return MachineStateId::BACKFLUSH_FILLING;
    }

    if (context.hasStateTimeoutElapsed(CleverCoffee::BackflushTiming::FINISHED_DISPLAY_TIMEOUT_MS)) {
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Finished display timeout");
        return MachineStateId::BACKFLUSH_IDLE;
    }

    return std::nullopt;
}
