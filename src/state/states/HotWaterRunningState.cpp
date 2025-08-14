/**
 * @file HotWaterRunningState.cpp
 * @brief Implementation of HotWaterRunningState for active hot water dispensing
 */

#include "HotWaterRunningState.h"
#include "../MachineStateContext.h"
#include "../GlobalState.h"
#include "EmergencyStopState.h"
#include "SensorErrorState.h"
#include "HotWaterState.h"
#include "PidNormalState.h"
#include "Logger.h"


void HotWaterRunningState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Hot water running - dispensing hot water");
}

void HotWaterRunningState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Hot water dispensing completed");
}

void HotWaterRunningState::update(MachineStateContext& context) {
    // Monitor hot water dispensing progress
    LOGF(DEBUG, "Hot Water Running: Temp=%.1f°C, Pressure=%.1fbar, Tank=%s",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.isWaterTankFull() ? "OK" : "EMPTY");
}

std::unique_ptr<MachineState> HotWaterRunningState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency conditions (highest priority)
    // 2. Sensor errors
    // 3. Hot water stop request (condition flag)
    // 4. Manual hot water switch deactivation
    // 5. Water tank empty
    // 6. Hot water completion conditions

    // Check for emergency stop
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during hot water");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for sensor errors
    if (context.hasSensorError()) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error during hot water");
        return std::make_unique<SensorErrorState>();
    }

    // Check for condition flags - hot water stop request
    auto& flags = g_state.machine.flags;
    if (flags.requestHotWaterStop) {
        flags.requestHotWaterStop = false; // Reset flag
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_IDLE, "Hot water stop requested");
        return std::make_unique<HotWaterState>();
    }

    // Check if hot water switch was deactivated manually
    if (!context.isHotWaterActive()) {
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_IDLE, "Hot water switch deactivated");
        return std::make_unique<HotWaterState>();
    }

    // Check water tank
    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateId::WATER_TANK_EMPTY, "Water tank empty during hot water");
        // For now, stop the hot water
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_IDLE, "Water tank empty - stopping hot water");
        return std::make_unique<HotWaterState>();
    }

    // TODO: Add hot water completion conditions based on time or other criteria
    // This would be handled by the hot water control logic and set appropriate flags

    // Continue hot water dispensing
    return nullptr;
}
