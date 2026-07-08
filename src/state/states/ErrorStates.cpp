/**
 * @file ErrorStates.cpp
 * @brief Implementation of error-related state classes
 */

#include "clevercoffee/state/states/ErrorStates.h"

#include "clevercoffee/Logger.h"
#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/state/MachineStateContext.h"

// ErrorStates Implementation
void SensorErrorState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "Sensor error detected - entering safe mode");
    context.enterSafeMode();
    errorStartTime_ = millis();
}

void SensorErrorState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Sensor error resolved - exiting safe mode");
    context.exitSafeMode();
}

void SensorErrorState::update(MachineStateContext& context) {
    unsigned long errorDuration = millis() - errorStartTime_;
    LOGF(DEBUG,
         "Sensor Error: Duration=%lums, Recovery=%s",
         errorDuration,
         context.hasSensorError() ? "PENDING" : "RESOLVED");
}

std::optional<MachineStateId> SensorErrorState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during sensor error");
        return MachineStateId::EMERGENCY_STOP;
    }

    unsigned long errorDuration = millis() - errorStartTime_;

    if (!context.hasSensorError() && !context.hasTemperatureError()) {
        // Error resolved - wait for recovery delay before resuming
        using CleverCoffee::Timing::ERROR_RECOVERY_DELAY_MS;
        if (errorDuration > ERROR_RECOVERY_DELAY_MS) {
            return transitionToPidState(context, "Sensor error recovered");
        }
    } else {
        // Error still present - keep resetting the clock so recovery delay
        // is measured from when the error actually clears, not from entry.
        errorStartTime_ = millis();
    }

    // Stay in SENSOR_ERROR until error resolves — never transition to PID_DISABLED.
    // A persistent sensor error requires the user to investigate; silently
    // disabling the PID hides the problem and prevents normal recovery.
    return std::nullopt;
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

std::optional<MachineStateId> WaterTankEmptyState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isEmergencyStop()) {
        context.logStateTransition(
            getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during water tank empty");
        return MachineStateId::EMERGENCY_STOP;
    }
    if (context.isWaterTankFull()) {
        return transitionToPidState(context, "Water tank refilled");
    }
    if (context.isStandbyRequested()) {
        context.setStandbyRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::STANDBY, "Standby requested");
        return MachineStateId::STANDBY;
    }
    // Safety: with keep_heater_on_empty the heater may still be running here.
    // The standby timeout must keep working so the heater never runs unattended
    // indefinitely while the tank stays empty.
    context.initializeStandbyTimerIfNeeded();
    if (context.shouldEnterStandby()) {
        context.logStateTransition(getStateId(), MachineStateId::STANDBY, "Entering standby mode");
        return MachineStateId::STANDBY;
    }
    return std::nullopt;
}

void EepromErrorState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "EEPROM error detected - configuration may be corrupted");
    context.enterSafeMode();
    context.setPidRuntimeState(false);
    // Fresh instance - initialize error tracking
    errorStartTime_ = millis();
}

void EepromErrorState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "EEPROM error resolved - configuration restored");
    context.exitSafeMode();
}

void EepromErrorState::update(MachineStateContext& context) {
    LOGF(DEBUG, "EEPROM Error: Configuration storage unavailable");
}

std::optional<MachineStateId> EepromErrorState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during EEPROM error");
        return MachineStateId::EMERGENCY_STOP;
    }

    unsigned long errorDuration = millis() - errorStartTime_;
    using CleverCoffee::Timing::EEPROM_RECOVERY_TIMEOUT_MS;
    if (errorDuration > EEPROM_RECOVERY_TIMEOUT_MS) {
        context.logStateTransition(
            getStateId(), MachineStateId::PID_DISABLED, "EEPROM recovery timeout - attempting recovery");
        return MachineStateId::PID_DISABLED;
    }
    return std::nullopt;
}