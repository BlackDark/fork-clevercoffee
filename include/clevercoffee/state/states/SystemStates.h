/**
 * @file SystemStates.h
 * @brief System-related state classes
 */

#pragma once

#include "clevercoffee/state/BaseState.h"

#include <optional>

// System States
class StandbyState : public BaseState<MachineStateId::STANDBY, StandbyState> {
  public:
    void                          update(MachineStateContext& context) override;
    void                          onEntryImpl(MachineStateContext& context) override;
    void                          onExitImpl(MachineStateContext& context) override;
    std::optional<MachineStateId> checkSpecificTransitions(MachineStateContext& context) override;
};

class ManualFlushRunningState : public BaseState<MachineStateId::MANUAL_FLUSH_RUNNING, ManualFlushRunningState> {
  public:
    void                          update(MachineStateContext& context) override;
    void                          onEntryImpl(MachineStateContext& context) override;
    void                          onExitImpl(MachineStateContext& context) override;
    std::optional<MachineStateId> checkSpecificTransitions(MachineStateContext& context) override;
};