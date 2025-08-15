/**
 * @file ErrorStates.cpp
 * @brief All error and safety states implementation
 */

#include "ErrorStates.h"
#include "EmergencyStopState.h"
#include "../MachineStateContext.h"
#include "../StateTransitionHelper.h"
#include "Logger.h"

// SensorErrorState Implementation
void SensorErrorState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "Sensor error detected - entering safe mode");
    
    context.enterSafeMode();
    errorStartTime_ = millis();
    recoveryAttempts_++;
    
    LOGF(INFO, "Sensor error recovery attempt %u/%u", recoveryAttempts_, MAX_RECOVERY_ATTEMPTS);
}

void SensorErrorState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Sensor error resolved - exiting safe mode");
    context.exitSafeMode();
}

void SensorErrorState::update(MachineStateContext& context) {
    unsigned long currentTime = millis();
    unsigned long errorDuration = currentTime - errorStartTime_;

    LOGF(DEBUG, "Sensor Error: Duration=%lums, Recovery=%s", errorDuration, 
         context.hasSensorError() ? "PENDING" : "RESOLVED");
}

std::unique_ptr<MachineState> SensorErrorState::checkSpecificTransitions(MachineStateContext& context) {
    // Only check emergency stop (don't check sensor errors since we ARE the sensor error state)
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during sensor error");
        return std::make_unique<EmergencyStopState>();
    }

    // Check if sensors have recovered
    if (!context.hasSensorError() && !context.hasTemperatureError()) {
        // Allow some time for sensor stability before returning to normal
        unsigned long errorDuration = millis() - errorStartTime_;
        constexpr unsigned long RECOVERY_DELAY_MS = 5000; // 5 seconds

        if (errorDuration > RECOVERY_DELAY_MS) {
            // Return to normal operation state based on PID setting
            return StateTransitionHelper::getNormalOperationState(context, getStateId());
        }
    }
    else {
        // Reset recovery timer if error is still present
        errorStartTime_ = millis();
    }

    // Check for persistent error or too many recovery attempts - fallback to PID disabled for safety
    unsigned long errorDuration = millis() - errorStartTime_;
    constexpr unsigned long MAX_ERROR_DURATION_MS = 60000; // 1 minute

    if (errorDuration > MAX_ERROR_DURATION_MS || recoveryAttempts_ >= MAX_RECOVERY_ATTEMPTS) {
        const char* reason = (recoveryAttempts_ >= MAX_RECOVERY_ATTEMPTS) ? 
            "Too many sensor recovery attempts - disabling PID for safety" :
            "Persistent sensor error - disabling PID for safety";
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, reason);
        context.setPidRuntimeState(false);
        // Will need to return PidDisabledState - placeholder for now
        return nullptr;
    }

    return nullptr;
}

// WaterTankEmptyState Implementation
void WaterTankEmptyState::onEntryImpl(MachineStateContext& context) {
    LOG(WARNING, "Water tank empty - please refill");
    
    // Water operations will be prevented by safety checks in other states
}

void WaterTankEmptyState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Water Tank Empty: Tank=%s, Temp=%.1f°C", 
         context.isWaterTankFull() ? "FILLED" : "EMPTY",
         context.getCurrentTemperature());
}

std::unique_ptr<MachineState> WaterTankEmptyState::checkSpecificTransitions(MachineStateContext& context) {
    // Only check emergency stop (don't check water tank since we ARE the water tank error state)
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during water tank empty");
        return std::make_unique<EmergencyStopState>();
    }

    // Check if water tank was refilled
    if (context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Water tank refilled");
        return StateTransitionHelper::getNormalOperationState(context, getStateId());
    }

    return nullptr;
}

// EepromErrorState Implementation
void EepromErrorState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "EEPROM error detected - configuration may be corrupted");
    
    context.enterSafeMode();
    context.setPidRuntimeState(false);
}

void EepromErrorState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "EEPROM error resolved - configuration restored");
    context.exitSafeMode();
}

void EepromErrorState::update(MachineStateContext& context) {
    LOGF(DEBUG, "EEPROM Error: Configuration storage unavailable");
}

std::unique_ptr<MachineState> EepromErrorState::checkSpecificTransitions(MachineStateContext& context) {
    // Only check emergency stop
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during EEPROM error");
        return std::make_unique<EmergencyStopState>();
    }

    // Check for recovery conditions with timeout-based recovery mechanism
    static unsigned long eepromErrorStartTime = 0;
    if (eepromErrorStartTime == 0) {
        eepromErrorStartTime = millis();
    }
    
    // Allow recovery after extended timeout (5 minutes) 
    constexpr unsigned long EEPROM_RECOVERY_TIMEOUT = 300000; // 5 minutes
    if (millis() - eepromErrorStartTime > EEPROM_RECOVERY_TIMEOUT) {
        eepromErrorStartTime = 0;
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, "EEPROM recovery timeout - attempting recovery");
        // Will need to return PidDisabledState - placeholder for now
        return nullptr;
    }

    return nullptr;
}