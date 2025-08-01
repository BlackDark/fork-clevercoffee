/**
 * @file BrewState.cpp
 * @brief Implementation of BrewState for brewing operations
 */

#include "BrewState.h"
#include "PidNormalState.h"
#include "EmergencyStopState.h"
#include "PidDisabledState.h"
#include "SensorErrorState.h"
#include "../MachineStateContext.h"
#include "Logger.h"

void BrewState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Starting brew process");
    
    // Log brew parameters for debugging
    LOGF(INFO, "Brew started: Temp=%.1f°C, Weight=%.1fg", 
         context.getCurrentTemperature(),
         context.getCurrentBrewWeight());
}

void BrewState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Brew process completed");
    
    // Allow MQTT to try to reconnect when exiting brew mode
    // This was in the original code to handle connectivity issues during brewing
    context.resetMqttReconnectCount();
    
    // Log final brew statistics
    LOGF(INFO, "Brew completed: Final weight=%.1fg, Duration=%.1fs", 
         context.getCurrentBrewWeight(),
         context.getCurrentTime() / 1000.0);
}

void BrewState::update(MachineStateContext& context) {
    // Monitor brewing progress
    // The actual brew control logic is handled by the brew() function
    // which is called through context.isBrewActive()
    
    // Log brewing progress periodically
    LOGF(DEBUG, "Brewing: Temp=%.1f°C, Weight=%.1fg, Pressure=%.1fbar", 
         context.getCurrentTemperature(),
         context.getCurrentBrewWeight(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> BrewState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions during brewing:
    // 1. Emergency stop (immediate safety)
    // 2. System critical errors (PID disabled, sensor errors)
    // 3. Brew completion (normal flow)

    // Check emergency conditions first
    if (checkEmergencyConditions(context)) {
        context.logStateTransition(getStateId(), MachineStateIds::EMERGENCY_STOP, "Emergency during brewing");
        return std::make_unique<EmergencyStopState>();
    }

    // Check if PID was disabled during brewing
    if (!isPidStillEnabled(context)) {
        context.logStateTransition(getStateId(), MachineStateIds::PID_DISABLED, "PID disabled during brewing");
        return std::make_unique<PidDisabledState>();
    }

    // Check for sensor errors during brewing
    if (checkSensorErrors(context)) {
        context.logStateTransition(getStateId(), MachineStateIds::SENSOR_ERROR, "Sensor error during brewing");
        return std::make_unique<SensorErrorState>();
    }

    // Check if brew process is complete or was stopped
    if (!isBrewStillActive(context)) {
        context.logStateTransition(getStateId(), MachineStateIds::PID_NORMAL, "Brew process completed");
        return std::make_unique<PidNormalState>();
    }

    // Continue brewing
    return nullptr;
}

bool BrewState::isBrewStillActive(MachineStateContext& context) const {
    return context.isBrewActive();
}

bool BrewState::checkEmergencyConditions(MachineStateContext& context) const {
    return context.isEmergencyStop();
}

bool BrewState::checkSensorErrors(MachineStateContext& context) const {
    return context.hasSensorError() || context.hasTemperatureError();
}

bool BrewState::isPidStillEnabled(MachineStateContext& context) const {
    return context.isPidEnabled();
}