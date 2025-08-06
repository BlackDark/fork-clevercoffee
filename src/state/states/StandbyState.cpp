/**
 * @file StandbyState.cpp
 * @brief Implementation of StandbyState for power-saving standby mode
 */

#include "StandbyState.h"
#include "../MachineStateContext.h"
#include "EmergencyStopState.h"
#include "Logger.h"
#include "PidNormalState.h"
#include "SensorErrorState.h"

void StandbyState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(INFO, "Entering standby mode - reducing power consumption");

    // Enter power-saving mode
    context.enterStandbyMode();

    // Disable PID control to save power
    context.setPidRuntimeState(false);
}

void StandbyState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "Exiting standby mode - resuming normal operation");

    // Exit power-saving mode
    context.exitStandbyMode();

    // Re-enable PID control
    context.setPidRuntimeState(true);
}

void StandbyState::update(MachineStateContext& context) {
    // Monitor standby status with reduced frequency
    LOGF(DEBUG, "Standby: Power saving active, UserActivity=%s, Sensors=%s", context.hasUserActivity() ? "DETECTED" : "IDLE", context.hasSensorError() ? "ERROR" : "OK");
}

std::unique_ptr<MachineState> StandbyState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency stop (highest priority - immediate safety)
    // 2. User activity detected (power button, switches, web interface)
    // 3. Sensor errors (system integrity)

    // Check emergency conditions first
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateIds::EMERGENCY_STOP, "Emergency condition detected in standby");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for user activity to wake up from standby
    if (context.hasUserActivity() || context.shouldExitStandby()) {
        context.logStateTransition(getStateId(), MachineStateIds::PID_NORMAL, "User activity detected - exiting standby");
        // Reset MQTT reconnect count when exiting standby (matches original logic)
        context.resetMqttReconnectCount();
        return std::make_unique<PidNormalState>();
    }

    // Check for sensor errors (even in standby, we should monitor)
    if (context.hasSensorError() || context.hasTemperatureError()) {
        context.logStateTransition(getStateId(), MachineStateIds::SENSOR_ERROR, "Sensor error detected in standby");
        // Reset MQTT reconnect count when exiting standby (matches original logic)
        context.resetMqttReconnectCount();
        return std::make_unique<SensorErrorState>();
    }

    // Continue in standby mode
    return nullptr;
}