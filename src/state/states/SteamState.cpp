/**
 * @file SteamState.cpp
 * @brief Implementation of SteamState for steam generation operations
 */

#include "SteamState.h"
#include "../MachineStateContext.h"
#include "EmergencyStopState.h"
#include "HotWaterState.h"
#include "Logger.h"
#include "PidNormalState.h"
#include "SensorErrorState.h"
#include "WaterTankEmptyState.h"

void SteamState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Starting steam generation");

    // Start steam operation - activate steam hardware and adjust PID
    context.setSteamState(true);
}

void SteamState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Stopping steam generation");

    // Stop steam operation - deactivate steam hardware
    context.setSteamState(false);
}

void SteamState::update(MachineStateContext& context) {
    // Monitor steam operation
    LOGF(DEBUG, "Steam: Active, Temp=%.1f°C, Tank=%s, Sensors=%s", context.getCurrentTemperature(), context.isWaterTankFull() ? "OK" : "EMPTY", context.hasSensorError() ? "ERROR" : "OK");
}

std::unique_ptr<MachineState> SteamState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency stop (highest priority - immediate safety)
    // 2. Hot water operation (steam mode can include hot water)
    // 3. System errors (sensors, water tank)
    // 4. Steam completion (normal operation)

    // Check emergency conditions first
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateIds::EMERGENCY_STOP, "Emergency condition detected during steam");
        return std::make_unique<EmergencyStopState>();
    }

    // Check if hot water is active during steam mode
    if (context.isHotWaterActive() && context.isSteamActive()) {
        context.logStateTransition(getStateId(), MachineStateIds::HOT_WATER, "Hot water activated during steam mode");
        return std::make_unique<HotWaterState>();
    }

    // Check for water tank empty
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateIds::WATER_TANK_EMPTY, "Water tank empty during steam");
        return std::make_unique<WaterTankEmptyState>();
    }

    // Check for sensor errors
    if (context.hasSensorError() || context.hasTemperatureError()) {
        context.logStateTransition(getStateId(), MachineStateIds::SENSOR_ERROR, "Sensor error detected during steam");
        return std::make_unique<SensorErrorState>();
    }

    // Check if steam is no longer active (user stopped it)
    if (!context.isSteamActive()) {
        context.logStateTransition(getStateId(), MachineStateIds::PID_NORMAL, "Steam operation completed");
        return std::make_unique<PidNormalState>();
    }

    // Continue steam operation
    return nullptr;
}