/**
 * @file InitState.h
 * @brief System initialization state
 */

#pragma once

#include "clevercoffee/state/BaseState.h"

class InitState : public BaseState<MachineStateId::INIT, InitState> {
public:
    static constexpr const char* STATE_NAME = "Init";

    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;

private:
    bool checkWaterTank(MachineStateContext& context) const;
    bool checkSensors(MachineStateContext& context) const;
    bool checkPidConfig(MachineStateContext& context) const;
};
