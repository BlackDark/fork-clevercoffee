/**
 * @file EmergencyStopState.h
 * @brief Emergency stop state class
 */

#pragma once

#include "clevercoffee/state/BaseState.h"

// Emergency Stop State
class EmergencyStopState : public BaseState<MachineStateId::EMERGENCY_STOP, EmergencyStopState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    void          onExitImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;

  private:
    void          performEmergencyShutdown(MachineStateContext& context);
    bool          isEmergencyCleared(MachineStateContext& context) const;
    MachineState* getRecoveryState(MachineStateContext& context) const;
};