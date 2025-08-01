/**
 * @file PidNormalState.cpp
 * @brief Implementation of PidNormalState for normal operation
 */

#include "PidNormalState.h"
#include "BrewState.h"
#include "ManualFlushState.h"
#include "BackflushState.h"
#include "SteamState.h"
#include "HotWaterState.h"
#include "EmergencyStopState.h"
#include "StandbyState.h"
#include "PidDisabledState.h"
#include "WaterTankEmptyState.h"
#include "SensorErrorState.h"
#include "../MachineStateContext.h"
#include "Logger.h"

void PidNormalState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Entering normal PID operation mode");
    
    // Ensure PID is enabled when entering this state
    context.setPidRuntimeState(true);
}

void PidNormalState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Exiting normal PID operation mode");
}

void PidNormalState::update(MachineStateContext& context) {
    // Monitor system status during normal operation
    // The actual transition logic is handled in checkTransitions()
    
    // Log periodic status for debugging (could be rate-limited)
    LOGF(DEBUG, "PID Normal: Temp=%.1f°C, Tank=%s, Sensors=%s", 
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.hasSensorError() ? "ERROR" : "OK");
}

std::unique_ptr<MachineState> PidNormalState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency stop (highest priority - immediate safety)
    // 2. Active processes (brew, steam, flush, hot water)
    // 3. System errors (sensors, water tank)
    // 4. Mode changes (PID disabled, standby)

    // Check emergency conditions first
    if (checkEmergencyConditions(context)) {
        context.logStateTransition(getStateId(), MachineStateIds::EMERGENCY_STOP, "Emergency condition detected");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for active brewing process
    if (context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateIds::BREW, "Brew process started");
        resetStandbyTimerIfNeeded(context);
        return std::make_unique<BrewState>();
    }

    // Check for manual flush
    if (context.isManualFlushActive()) {
        context.logStateTransition(getStateId(), MachineStateIds::MANUAL_FLUSH, "Manual flush started");
        resetStandbyTimerIfNeeded(context);
        return std::make_unique<ManualFlushState>();
    }

    // Check for backflush operation
    if (context.isBackflushActive()) {
        context.logStateTransition(getStateId(), MachineStateIds::BACKFLUSH, "Backflush started");
        resetStandbyTimerIfNeeded(context);
        return std::make_unique<BackflushState>();
    }

    // Check for steam mode
    if (context.isSteamActive()) {
        context.logStateTransition(getStateId(), MachineStateIds::STEAM, "Steam mode activated");
        resetStandbyTimerIfNeeded(context);
        return std::make_unique<SteamState>();
    }

    // Check for hot water operation
    if (context.isHotWaterActive()) {
        context.logStateTransition(getStateId(), MachineStateIds::HOT_WATER, "Hot water mode activated");
        resetStandbyTimerIfNeeded(context);
        return std::make_unique<HotWaterState>();
    }

    // Check for standby mode (before error checks to allow graceful shutdown)
    if (shouldEnterStandby(context)) {
        context.logStateTransition(getStateId(), MachineStateIds::STANDBY, "Standby timeout reached");
        context.setPidRuntimeState(false);
        return std::make_unique<StandbyState>();
    }

    // Check if PID was disabled
    if (!context.isPidEnabled()) {
        context.logStateTransition(getStateId(), MachineStateIds::PID_DISABLED, "PID disabled");
        return std::make_unique<PidDisabledState>();
    }

    // Check for water tank empty
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateIds::WATER_TANK_EMPTY, "Water tank empty");
        return std::make_unique<WaterTankEmptyState>();
    }

    // Check for sensor errors
    if (checkSystemErrors(context)) {
        context.logStateTransition(getStateId(), MachineStateIds::SENSOR_ERROR, "Sensor error detected");
        return std::make_unique<SensorErrorState>();
    }

    // No state change - continue in normal operation
    return nullptr;
}

bool PidNormalState::checkEmergencyConditions(MachineStateContext& context) const {
    return context.isEmergencyStop();
}

bool PidNormalState::shouldEnterStandby(MachineStateContext& context) const {
    return context.shouldEnterStandby();
}

bool PidNormalState::checkSystemErrors(MachineStateContext& context) const {
    return context.hasSensorError() || context.hasTemperatureError();
}

void PidNormalState::resetStandbyTimerIfNeeded(MachineStateContext& context) const {
    // Reset standby timer when transitioning to active operations
    // This prevents standby activation during active use
    context.resetStandbyTimer(getStateId());
}