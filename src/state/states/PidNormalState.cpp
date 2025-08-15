/**
 * @file PidNormalState.cpp
 * @brief Implementation of PidNormalState for normal operation
 */

#include "clevercoffee/state/states/PidNormalState.h"
#include "clevercoffee/state/states/clevercoffee/MachineStateContext.h"
#include "clevercoffee/state/states/clevercoffee/GlobalState.h"
#include "clevercoffee/state/states/clevercoffee/StateTransitionHelper.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/states/BrewStates.h"
#include "clevercoffee/state/states/WaterSteamStates.h"
#include "clevercoffee/state/states/BackflushStates.h"
#include "clevercoffee/state/states/SystemStates.h"


void PidNormalState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "PID Normal mode active - ready for operation");
    resetStandbyTimerIfNeeded(context);
}

void PidNormalState::update(MachineStateContext& context) {
    // Monitor system status and user inputs
    // Actual control logic is handled by other components
    resetStandbyTimerIfNeeded(context);
}

std::unique_ptr<MachineState> PidNormalState::checkSpecificTransitions(MachineStateContext& context) {
    // Check common safety transitions first
    if (auto state = StateTransitionHelper::checkCommonSafetyTransitions(context, getStateId())) {
        return state;
    }

    // Check for condition flags (user/external requests)
    // Check for brew start request
    if (g_state.machine.flags.requestBrewStart) {
        g_state.machine.flags.requestBrewStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew start requested");
        return std::make_unique<BrewIdleState>();
    }

    // Check for hot water start request
    if (g_state.machine.flags.requestHotWaterStart) {
        g_state.machine.flags.requestHotWaterStart = false;
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_IDLE, "Hot water start requested");
        return std::make_unique<HotWaterIdleState>();
    }

    // Check for steam start request
    if (g_state.machine.flags.requestSteamStart) {
        g_state.machine.flags.requestSteamStart = false;
        context.logStateTransition(getStateId(), MachineStateId::STEAM_IDLE, "Steam start requested");
        return std::make_unique<SteamIdleState>();
    }

    // Check for manual flush start request
    if (g_state.machine.flags.requestManualFlushStart) {
        g_state.machine.flags.requestManualFlushStart = false;
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_IDLE, "Manual flush start requested");
        return std::make_unique<ManualFlushIdleState>();
    }

    // Check for backflush start request
    if (g_state.machine.flags.requestBackflushStart) {
        g_state.machine.flags.requestBackflushStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush start requested");
        return std::make_unique<BackflushState>();
    }

    // Check for standby request
    if (g_state.machine.flags.requestStandby) {
        g_state.machine.flags.requestStandby = false;
        context.logStateTransition(getStateId(), MachineStateId::STANDBY, "Standby requested");
        return std::make_unique<StandbyState>();
    }

    // Check for standby conditions (existing logic)
    if (shouldEnterStandby(context)) {
        context.logStateTransition(getStateId(), MachineStateId::STANDBY, "Entering standby mode");
        return std::make_unique<StandbyState>();
    }

    // Continue in normal mode
    return nullptr;
}

bool PidNormalState::shouldEnterStandby(MachineStateContext& context) const {
    return context.shouldEnterStandby();
}

void PidNormalState::resetStandbyTimerIfNeeded(MachineStateContext& context) const {
    // Reset standby timer to prevent automatic sleep during active operation
    context.resetStandbyTimer(getStateId());
}
