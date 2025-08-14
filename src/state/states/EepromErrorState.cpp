/**
 * @file EepromErrorState.cpp
 * @brief Implementation of EepromErrorState for configuration storage errors
 */

#include "EepromErrorState.h"
#include "../MachineStateContext.h"
#include "EmergencyStopState.h"
#include "Logger.h"
#include "PidDisabledState.h"

void EepromErrorState::onEntry(MachineStateContext& context) {
    context.logStateEntry(getStateId(), getStateName());
    LOG(ERROR, "EEPROM error detected - configuration may be corrupted");

    // Enter safe mode - disable operations that depend on configuration
    context.enterSafeMode();
    context.setPidRuntimeState(false);
}

void EepromErrorState::onExit(MachineStateContext& context) {
    context.logStateExit(getStateId(), getStateName());
    LOG(INFO, "EEPROM error resolved - configuration restored");

    // Exit safe mode
    context.exitSafeMode();
}

void EepromErrorState::update(MachineStateContext& context) {
    // Monitor EEPROM status
    LOGF(DEBUG, "EEPROM Error: Configuration storage unavailable");
}

std::unique_ptr<MachineState> EepromErrorState::checkTransitions(MachineStateContext& context) {
    // Priority order for transitions:
    // 1. Emergency stop (highest priority - immediate safety)
    // 2. Configuration restored -> PID disabled (safe fallback)

    // Check emergency conditions first
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency condition during EEPROM error");
        return std::make_unique<EmergencyStopState>();
    }

    // For EEPROM errors, we typically need manual intervention
    // In a real implementation, this would check if configuration was restored
    // For now, we stay in this state until manually resolved

    // TODO: Add EEPROM recovery mechanism
    // if (context.isEepromRecovered()) {
    //     context.logStateTransition(getStateId(), MachineStateIds::PID_DISABLED,
    //                                "EEPROM recovered - entering safe mode");
    //     return std::make_unique<PidDisabledState>();
    // }

    // Stay in error state - requires manual intervention
    return nullptr;
}