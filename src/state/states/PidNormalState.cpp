/**
 * @file PidNormalState.cpp
 * @brief Implementation of PidNormalState for normal operation
 */

#include "PidNormalState.h"
#include "../MachineStateContext.h"
#include "../GlobalState.h"
#include "Logger.h"
#include "BrewState.h"
#include "HotWaterState.h"
#include "SteamState.h"
#include "BackflushState.h"
#include "ManualFlushState.h"
#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "StandbyState.h"


void PidNormalState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "PID Normal mode active - ready for operation");
    resetStandbyTimerIfNeeded(context);
}

void PidNormalState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
}

void PidNormalState::update(MachineStateContext& context) {
    // Monitor system status and user inputs
    // Actual control logic is handled by other components
    resetStandbyTimerIfNeeded(context);
}

std::unique_ptr<MachineState> PidNormalState::checkTransitions(MachineStateContext& context) {
    // Check for emergency conditions first
    if (checkEmergencyConditions(context)) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency condition detected");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (checkSystemErrors(context)) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "System error detected");
        return std::make_unique<SensorErrorState>();
    }

    // Check for condition flags (user/external requests)
    // Check for brew start request
    if (g_state.machine.flags.requestBrewStart) {
        g_state.machine.flags.requestBrewStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew start requested");
        return std::make_unique<BrewState>();
    }

    // Check for hot water start request
    if (g_state.machine.flags.requestHotWaterStart) {
        g_state.machine.flags.requestHotWaterStart = false;
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_IDLE, "Hot water start requested");
        return std::make_unique<HotWaterState>();
    }

    // Check for steam start request
    if (g_state.machine.flags.requestSteamStart) {
        g_state.machine.flags.requestSteamStart = false;
        context.logStateTransition(getStateId(), MachineStateId::STEAM_IDLE, "Steam start requested");
        return std::make_unique<SteamState>();
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

bool PidNormalState::checkEmergencyConditions(MachineStateContext& context) const {
    return context.isEmergencyStop();
}

bool PidNormalState::shouldEnterStandby(MachineStateContext& context) const {
    return context.shouldEnterStandby();
}

bool PidNormalState::checkSystemErrors(MachineStateContext& context) const {
    return context.hasSensorError();
}

void PidNormalState::resetStandbyTimerIfNeeded(MachineStateContext& context) const {
    // Reset standby timer to prevent automatic sleep during active operation
    context.resetStandbyTimer(getStateId());
}
