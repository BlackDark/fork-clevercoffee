/**
 * @file PidStates.h
 * @brief PID-related state classes
 */

#pragma once

#include "clevercoffee/state/BaseState.h"

// PID Normal State
class PidNormalState : public BaseState<MachineStateId::PID_NORMAL, PidNormalState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;

  private:
    bool shouldEnterStandby(MachineStateContext& context) const;
    void resetStandbyTimerIfNeeded(MachineStateContext& context) const;
};

// PID Disabled State
class PidDisabledState : public BaseState<MachineStateId::PID_DISABLED, PidDisabledState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};