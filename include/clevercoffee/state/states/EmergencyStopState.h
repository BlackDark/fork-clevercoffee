/**
 * @file EmergencyStopState.h
 * @brief Emergency stop safety state
 */

#pragma once

#include "clevercoffee/state/states/clevercoffee/BaseState.h"

class EmergencyStopState : public BaseState<MachineStateId::EMERGENCY_STOP, EmergencyStopState> {
public:
    static constexpr const char* STATE_NAME = "Emergency Stop";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;

private:
    void performEmergencyShutdown(MachineStateContext& context);
    bool isEmergencyCleared(MachineStateContext& context) const;
    std::unique_ptr<MachineState> getRecoveryState(MachineStateContext& context) const;
};