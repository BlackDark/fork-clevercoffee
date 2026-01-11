/**
 * @file EmergencyStopState.cpp
 * @brief Implementation of emergency stop state
 */

#include "clevercoffee/state/states/EmergencyStopState.h"

#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/control/ProcessController.h"

// EmergencyStopState Implementation
void EmergencyStopState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "EMERGENCY STOP ACTIVATED - System entering safe mode");
    performEmergencyShutdown(context);
    LOGF(ERROR,
         "Emergency conditions: Temp=%.1f°C, EmergencyStop=%s",
         context.getCurrentTemperature(),
         context.isEmergencyStop() ? "ACTIVE" : "INACTIVE");
}

void EmergencyStopState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Emergency stop cleared - System ready for restart");
}

void EmergencyStopState::update(MachineStateContext& context) {
    LOGF(INFO,
         "Emergency Stop Active: Temp=%.1f°C, Emergency=%s",
         context.getCurrentTemperature(),
         context.isEmergencyStop() ? "ACTIVE" : "CLEARED");
    performEmergencyShutdown(context);
}

MachineState* EmergencyStopState::checkSpecificTransitions(MachineStateContext& context) {
    if (isEmergencyCleared(context)) {
        context.logStateTransition(getStateId(), MachineStateId::INIT, "Emergency condition cleared - restarting");
        return getRecoveryState(context);
    }
    return nullptr;
}

void EmergencyStopState::performEmergencyShutdown(MachineStateContext& context) {
    context.performSafeShutdown();
    context.setPidRuntimeState(false);
}

bool EmergencyStopState::isEmergencyCleared(MachineStateContext& context) const {
    // Use centralized emergency stop manager through ProcessController
    double currentTemp = context.getCurrentTemperature();
    if (auto* processController = context.systemContext().processController()) {
        bool cleared = processController->isEmergencyCleared(currentTemp);
        if (cleared && context.isEmergencyStop()) {
            // Clear the SystemContext flag when emergency is cleared
            context.systemContext().setEmergencyStop(false);
        }
        return cleared;
    }
    // Fallback: if ProcessController not available, use simple threshold check
    const double SAFE_TEMPERATURE_THRESHOLD = 100.0;
    if (currentTemp > SAFE_TEMPERATURE_THRESHOLD) {
        LOGF(WARNING, "Temperature still elevated: %.1f°C (ProcessController not available)", currentTemp);
        return false;
    }
    // Clear the SystemContext flag if temperature is safe
    if (context.isEmergencyStop()) {
        context.systemContext().setEmergencyStop(false);
    }
    return true;
}

MachineState* EmergencyStopState::getRecoveryState(MachineStateContext& context) const {
    return getStateInstance(MachineStateId::INIT);
}