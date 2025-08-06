/**
 * @file HotWaterState.cpp
 * @brief Implementation of HotWaterState for hot water dispensing
 */

#include "HotWaterState.h"
#include "../MachineStateContext.h"
#include "EmergencyStopState.h"
#include "Logger.h"
#include "PidNormalState.h"
#include "SensorErrorState.h"
#include "WaterTankEmptyState.h"

void HotWaterState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Starting hot water dispensing");

    // Start hot water operation - activate appropriate hardware
    context.setHotWaterState(true);
}

void HotWaterState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Stopping hot water dispensing");

    // Stop hot water operation - deactivate hardware
    context.setHotWaterState(false);
}

void HotWaterState::update(MachineStateContext& context) {
    // Monitor hot water operation
    LOGF(DEBUG, "Hot Water: Active, Temp=%.1f°C, Tank=%s, Sensors=%s", context.getCurrentTemperature(), context.isWaterTankFull() ? "OK" : "EMPTY", context.hasSensorError() ? "ERROR" : "OK");
}

std::unique_ptr<MachineState> HotWaterState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency stop (highest priority - immediate safety)
    // 2. System errors (sensors, water tank)
    // 3. Hot water completion (normal operation)

    // Check emergency conditions first
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateIds::EMERGENCY_STOP, "Emergency condition detected during hot water");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for water tank empty
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateIds::WATER_TANK_EMPTY, "Water tank empty during hot water");
        return std::make_unique<WaterTankEmptyState>();
    }

    // Check for sensor errors
    if (context.hasSensorError() || context.hasTemperatureError()) {
        context.logStateTransition(getStateId(), MachineStateIds::SENSOR_ERROR, "Sensor error detected during hot water");
        return std::make_unique<SensorErrorState>();
    }

    // Check if hot water is no longer active (user stopped it)
    if (!context.isHotWaterActive()) {
        context.logStateTransition(getStateId(), MachineStateIds::PID_NORMAL, "Hot water operation completed");
        return std::make_unique<PidNormalState>();
    }

    // Continue hot water operation
    return nullptr;
}