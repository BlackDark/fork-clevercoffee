/**
 * @file InitState.cpp
 * @brief Implementation of initialization state
 */

#include "clevercoffee/state/states/InitState.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"
#include "clevercoffee/Logger.h"

// InitState Implementation
void InitState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "System initializing - performing startup checks");
}

void InitState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Init state: Water tank: %s, Sensors: %s, PID: %s", checkWaterTank(context) ? "OK" : "EMPTY", checkSensors(context) ? "OK" : "ERROR", checkPidConfig(context) ? "ENABLED" : "DISABLED");
}

MachineState* InitState::checkSpecificTransitions(MachineStateContext& context) {
    if (!checkPidConfig(context)) {
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, "PID disabled");
        return getStateInstance(MachineStateId::PID_DISABLED);
    } else {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "PID enabled - entering normal operation");
        return getStateInstance(MachineStateId::PID_NORMAL);
    }
}

bool InitState::checkWaterTank(MachineStateContext& context) const {
    return context.isWaterTankFull();
}

bool InitState::checkSensors(MachineStateContext& context) const {
    if (context.hasSensorError() || context.hasTemperatureError()) {
        return false;
    }
    return true;
}

bool InitState::checkPidConfig(MachineStateContext& context) const {
    return context.isPidEnabled();
}