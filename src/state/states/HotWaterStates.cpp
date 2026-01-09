/**
 * @file HotWaterStates.cpp
 * @brief Implementation of hot water-related state classes
 */

#include "clevercoffee/state/states/HotWaterStates.h"

#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateFactory.h"

// HotWaterStates Implementation
void HotWaterIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Hot water idle - ready to dispense hot water");
}

void HotWaterIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Hot Water Idle: Temp=%.1f°C, Tank=%s, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.getFilteredPressure());
}

MachineState* HotWaterIdleState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isHotWaterStartRequested()) {
        context.setHotWaterStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_RUNNING, "Hot water start requested");
        return getStateInstance(MachineStateId::HOT_WATER_RUNNING);
    }
    if (context.isHotWaterActive()) {
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_RUNNING, "Hot water switch activated");
        return getStateInstance(MachineStateId::HOT_WATER_RUNNING);
    }
    return nullptr;
}

void HotWaterRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Hot water running - dispensing hot water");
}

void HotWaterRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Hot Water Running: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

MachineState* HotWaterRunningState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isHotWaterStopRequested()) {
        context.setHotWaterStopRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_STOPPED, "Hot water stop requested");
        return getStateInstance(MachineStateId::HOT_WATER_STOPPED);
    }
    if (!context.isHotWaterActive()) {
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_STOPPED, "Hot water deactivated");
        return getStateInstance(MachineStateId::HOT_WATER_STOPPED);
    }
    return nullptr;
}

void HotWaterStoppedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Hot water dispensing stopped");
}

void HotWaterStoppedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Hot Water Stopped: Temp=%.1f°C", context.getCurrentTemperature());
}

MachineState* HotWaterStoppedState::checkSpecificTransitions(MachineStateContext& context) {
    // Use a hardcoded 2 second timeout for the stopped state (display time)
    if (context.hasStateTimeoutElapsed(2000)) {
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_IDLE, "Hot water stopped display timeout");
        return getStateInstance(MachineStateId::HOT_WATER_IDLE);
    }

    return nullptr;
}