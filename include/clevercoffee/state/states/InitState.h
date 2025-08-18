/**
 * @file InitState.h
 * @brief Initialization state class
 */

#pragma once

#include "clevercoffee/state/BaseState.h"

// Init State
class InitState : public BaseState<MachineStateId::INIT, InitState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
private:
    bool checkWaterTank(MachineStateContext& context) const;
    bool checkSensors(MachineStateContext& context) const;
    bool checkPidConfig(MachineStateContext& context) const;
};