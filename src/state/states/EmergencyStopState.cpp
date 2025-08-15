/**
 * @file EmergencyStopState.cpp
 * @brief Implementation of EmergencyStopState for safety conditions
 */

#include "EmergencyStopState.h"
#include "../MachineStateContext.h"
#include "../StateTransitionHelper.h"
#include "InitState.h"
#include "Logger.h"

void EmergencyStopState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "EMERGENCY STOP ACTIVATED - System entering safe mode");

    // Immediately perform emergency shutdown
    performEmergencyShutdown(context);

    // Log emergency conditions for diagnosis
    LOGF(ERROR, "Emergency conditions: Temp=%.1f°C, EmergencyStop=%s", context.getCurrentTemperature(), context.isEmergencyStop() ? "ACTIVE" : "INACTIVE");
}

void EmergencyStopState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Emergency stop cleared - System ready for restart");
}

void EmergencyStopState::update(MachineStateContext& context) {
    // Continuously monitor emergency conditions
    // Ensure system remains in safe state

    // Log current status periodically for monitoring
    LOGF(INFO, "Emergency Stop Active: Temp=%.1f°C, Emergency=%s", context.getCurrentTemperature(), context.isEmergencyStop() ? "ACTIVE" : "CLEARED");

    // Ensure emergency shutdown is maintained
    performEmergencyShutdown(context);
}

std::unique_ptr<MachineState> EmergencyStopState::checkSpecificTransitions(MachineStateContext& context) {
    // Only transition out of emergency stop when condition is fully cleared
    // and system is safe to restart

    if (isEmergencyCleared(context)) {
        context.logStateTransition(getStateId(), MachineStateId::INIT, "Emergency condition cleared - restarting");
        return getRecoveryState(context);
    }

    // Stay in emergency stop until condition is cleared
    return nullptr;
}

void EmergencyStopState::performEmergencyShutdown(MachineStateContext& context) {
    // Perform safe shutdown of all critical systems
    context.performSafeShutdown();

    // Disable PID control to stop heating
    context.setPidRuntimeState(false);

    // Note: Individual hardware shutdowns are handled by performSafeShutdown()
    // which should turn off heater, pump, and valve relays
}

bool EmergencyStopState::isEmergencyCleared(MachineStateContext& context) const {
    // Emergency is cleared when:
    // 1. Emergency stop flag is not active
    // 2. Temperature is within safe range
    // 3. System is stable

    if (context.isEmergencyStop()) {
        return false;
    }

    // Check if temperature has returned to safe levels
    double currentTemp = context.getCurrentTemperature();
    const double SAFE_TEMPERATURE_THRESHOLD = 100.0; // Celsius

    if (currentTemp > SAFE_TEMPERATURE_THRESHOLD) {
        LOGF(WARNING, "Temperature still elevated: %.1f°C", currentTemp);
        return false;
    }

    return true;
}

std::unique_ptr<MachineState> EmergencyStopState::getRecoveryState(MachineStateContext& context) const {
    // After emergency stop, always return to init state for full system check
    // This ensures all systems are validated before resuming operation
    return std::make_unique<InitState>();
}