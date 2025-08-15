/**
 * @file InitState.cpp
 * @brief Implementation of InitState for system startup and validation
 */

#include "clevercoffee/state/states/InitState.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateTransitionHelper.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/states/SystemStates.h"
#include "clevercoffee/state/states/PidNormalState.h"

void InitState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "System initializing - performing startup checks");
}

void InitState::update(MachineStateContext& context) {
    // Perform continuous system validation during init
    // The actual transition logic is handled in checkTransitions()

    // Log current system status for debugging
    LOGF(DEBUG, "Init state: Water tank: %s, Sensors: %s, PID: %s", checkWaterTank(context) ? "OK" : "EMPTY", checkSensors(context) ? "OK" : "ERROR", checkPidConfig(context) ? "ENABLED" : "DISABLED");
}

std::unique_ptr<MachineState> InitState::checkSpecificTransitions(MachineStateContext& context) {
    // Check common safety transitions first
    if (auto state = StateTransitionHelper::checkCommonSafetyTransitions(context, getStateId())) {
        return state;
    }

    // Determine normal operation state based on PID setting
    if (!checkPidConfig(context)) {
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, "PID disabled");
        return std::make_unique<PidDisabledState>();
    }
    else {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "PID enabled - entering normal operation");
        return std::make_unique<PidNormalState>();
    }
}

bool InitState::checkWaterTank(MachineStateContext& context) const {
    return context.isWaterTankFull();
}

bool InitState::checkSensors(MachineStateContext& context) const {
    // Check if sensor manager reports any errors
    if (context.hasSensorError()) {
        return false;
    }

    // Check temperature sensor specifically
    if (context.hasTemperatureError()) {
        return false;
    }

    return true;
}

bool InitState::checkPidConfig(MachineStateContext& context) const {
    return context.isPidEnabled();
}
