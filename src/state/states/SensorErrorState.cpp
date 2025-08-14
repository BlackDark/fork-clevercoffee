/**
 * @file SensorErrorState.cpp
 * @brief Implementation of SensorErrorState for sensor error conditions
 */

#include "SensorErrorState.h"
#include "../MachineStateContext.h"
#include "EmergencyStopState.h"
#include "Logger.h"
#include "PidDisabledState.h"
#include "PidNormalState.h"

void SensorErrorState::onEntry(MachineStateContext& context) {
    context.logStateEntry(static_cast<int>(getStateId()), getStateName());
    LOG(ERROR, "Sensor error detected - entering safe mode");

    // Disable critical operations for safety
    context.enterSafeMode();

    // Reset error recovery timer
    errorStartTime_ = millis();
}

void SensorErrorState::onExit(MachineStateContext& context) {
    context.logStateExit(static_cast<int>(getStateId()), getStateName());
    LOG(INFO, "Sensor error resolved - exiting safe mode");

    // Re-enable normal operations
    context.exitSafeMode();
}

void SensorErrorState::update(MachineStateContext& context) {
    // Monitor sensor status and attempt recovery
    unsigned long currentTime = millis();
    unsigned long errorDuration = currentTime - errorStartTime_;

    LOGF(DEBUG, "Sensor Error: Duration=%lums, Recovery=%s", errorDuration, context.hasSensorError() ? "PENDING" : "RESOLVED");
}

std::unique_ptr<MachineState> SensorErrorState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency stop (highest priority - immediate safety)
    // 2. Persistent sensor error -> PID disabled (safe fallback)
    // 3. Sensor error resolved -> Normal operation

    // Check emergency conditions first
    if (context.isEmergencyStop()) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::EMERGENCY_STOP), "Emergency condition during sensor error");
        return std::make_unique<EmergencyStopState>();
    }

    // Check if sensors have recovered
    if (!context.hasSensorError() && !context.hasTemperatureError()) {
        // Allow some time for sensor stability before returning to normal
        unsigned long errorDuration = millis() - errorStartTime_;
        constexpr unsigned long RECOVERY_DELAY_MS = 5000; // 5 seconds

        if (errorDuration > RECOVERY_DELAY_MS) {
            // Check if we should return to normal PID or stay disabled
            if (context.isPidEnabled()) {
                context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::PID_NORMAL), "Sensor error resolved - returning to normal operation");
                return std::make_unique<PidNormalState>();
            }
            else {
                context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::PID_DISABLED), "Sensor error resolved but PID disabled");
                return std::make_unique<PidDisabledState>();
            }
        }
    }
    else {
        // Reset recovery timer if error is still present
        errorStartTime_ = millis();
    }

    // Check for persistent error - fallback to PID disabled for safety
    unsigned long errorDuration = millis() - errorStartTime_;
    constexpr unsigned long MAX_ERROR_DURATION_MS = 60000; // 1 minute

    if (errorDuration > MAX_ERROR_DURATION_MS) {
        context.logStateTransition(static_cast<int>(getStateId()), static_cast<int>(MachineStateId::PID_DISABLED), "Persistent sensor error - disabling PID for safety");
        context.setPidRuntimeState(false);
        return std::make_unique<PidDisabledState>();
    }

    // Continue monitoring sensor error
    return nullptr;
}