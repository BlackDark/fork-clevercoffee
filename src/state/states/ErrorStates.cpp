/**
 * @file ErrorStates.cpp
 * @brief Implementation of error-related state classes
 */

#include "clevercoffee/state/states/ErrorStates.h"

#include "clevercoffee/Logger.h"
#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"

// ErrorStates Implementation
void SensorErrorState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "Sensor error detected - entering safe mode");
    context.enterSafeMode();
    errorStartTime_   = millis();
    recoveryAttempts_ = 1; // Reset to 1 for this error episode
    LOGF(INFO, "Sensor error recovery attempt %u/%u", recoveryAttempts_, MAX_RECOVERY_ATTEMPTS);
}

void SensorErrorState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Sensor error resolved - exiting safe mode");
    context.exitSafeMode();
}

void SensorErrorState::update(MachineStateContext& context) {
    unsigned long currentTime   = millis();
    unsigned long errorDuration = currentTime - errorStartTime_;
    LOGF(DEBUG,
         "Sensor Error: Duration=%lums, Recovery=%s",
         errorDuration,
         context.hasSensorError() ? "PENDING" : "RESOLVED");
}

MachineState* SensorErrorState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during sensor error");
        return getStateInstance(MachineStateId::EMERGENCY_STOP);
    }
    if (!context.hasSensorError() && !context.hasTemperatureError()) {
        unsigned long errorDuration = millis() - errorStartTime_;
        using CleverCoffee::Timing::ERROR_RECOVERY_DELAY_MS;
        if (errorDuration > ERROR_RECOVERY_DELAY_MS) {
            return transitionToPidState(context, "Sensor error recovered");
        }
    } else {
        errorStartTime_ = millis();
    }
    unsigned long errorDuration = millis() - errorStartTime_;
    using CleverCoffee::Timing::MAX_SENSOR_ERROR_DURATION_MS;
    if (errorDuration > MAX_SENSOR_ERROR_DURATION_MS || recoveryAttempts_ >= MAX_RECOVERY_ATTEMPTS) {
        const char* reason = (recoveryAttempts_ >= MAX_RECOVERY_ATTEMPTS)
                                 ? "Too many sensor recovery attempts - disabling PID for safety"
                                 : "Persistent sensor error - disabling PID for safety";
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, reason);
        context.setPidRuntimeState(false);
        return getStateInstance(MachineStateId::PID_DISABLED);
    }
    return nullptr;
}

void WaterTankEmptyState::onEntryImpl(MachineStateContext& context) {
    LOG(WARNING, "Water tank empty - please refill");
}

void WaterTankEmptyState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Water Tank Empty: Tank=%s, Temp=%.1f°C",
         context.isWaterTankFull() ? "FILLED" : "EMPTY",
         context.getCurrentTemperature());
}

MachineState* WaterTankEmptyState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isEmergencyStop()) {
        context.logStateTransition(
            getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during water tank empty");
        return getStateInstance(MachineStateId::EMERGENCY_STOP);
    }
    if (context.isWaterTankFull()) {
        return transitionToPidState(context, "Water tank refilled");
    }
    return nullptr;
}

void EepromErrorState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "EEPROM error detected - configuration may be corrupted");
    context.enterSafeMode();
    context.setPidRuntimeState(false);
    errorStartTime_ = millis();  // Reset timer on entry
}

void EepromErrorState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "EEPROM error resolved - configuration restored");
    context.exitSafeMode();
}

void EepromErrorState::update(MachineStateContext& context) {
    LOGF(DEBUG, "EEPROM Error: Configuration storage unavailable");
}

MachineState* EepromErrorState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during EEPROM error");
        return getStateInstance(MachineStateId::EMERGENCY_STOP);
    }
    using CleverCoffee::Timing::EEPROM_RECOVERY_TIMEOUT_MS;
    if (millis() - errorStartTime_ > EEPROM_RECOVERY_TIMEOUT_MS) {
        context.logStateTransition(
            getStateId(), MachineStateId::PID_DISABLED, "EEPROM recovery timeout - attempting recovery");
        return getStateInstance(MachineStateId::PID_DISABLED);
    }
    return nullptr;
}