/**
 * @file EmergencyStopState.h
 * @brief Emergency stop state class
 */

#pragma once

#include "clevercoffee/state/BaseState.h"
#include <optional>

// Emergency Stop State
class EmergencyStopState : public BaseState<MachineStateId::EMERGENCY_STOP, EmergencyStopState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    void          onExitImpl(MachineStateContext& context) override;
    std::optional<MachineStateId> checkSpecificTransitions(MachineStateContext& context) override;

  private:
    void performEmergencyShutdown(MachineStateContext& context);
    bool isEmergencyCleared(MachineStateContext& context) const;
};