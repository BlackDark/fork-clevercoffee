/**
 * @file BackflushStates.h
 * @brief Backflush-related state classes
 */

#pragma once

#include "clevercoffee/state/BaseState.h"
#include <optional>

// Backflush States
class BackflushState : public BaseState<MachineStateId::BACKFLUSH_IDLE, BackflushState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    std::optional<MachineStateId> checkSpecificTransitions(MachineStateContext& context) override;
};

class BackflushFillingState : public BaseState<MachineStateId::BACKFLUSH_FILLING, BackflushFillingState> {
  public:
    void          update(MachineStateContext& context) override;
    std::optional<MachineStateId> checkSpecificTransitions(MachineStateContext& context) override;
};

class BackflushFlushingState : public BaseState<MachineStateId::BACKFLUSH_FLUSHING, BackflushFlushingState> {
  public:
    void          update(MachineStateContext& context) override;
    std::optional<MachineStateId> checkSpecificTransitions(MachineStateContext& context) override;
};

class BackflushFinishedState : public BaseState<MachineStateId::BACKFLUSH_FINISHED, BackflushFinishedState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    std::optional<MachineStateId> checkSpecificTransitions(MachineStateContext& context) override;
};