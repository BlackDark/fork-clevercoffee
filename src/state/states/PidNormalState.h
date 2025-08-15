/**
 * @file PidNormalState.h
 * @brief Main operational hub state
 */

#pragma once

#include "../BaseState.h"

class PidNormalState : public BaseState<MachineStateId::PID_NORMAL, PidNormalState> {
public:
    static constexpr const char* STATE_NAME = "PID Normal";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;

private:
    bool shouldEnterStandby(MachineStateContext& context) const;
    void resetStandbyTimerIfNeeded(MachineStateContext& context) const;
};