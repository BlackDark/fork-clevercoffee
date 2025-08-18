/**
 * @file BrewStates.h
 * @brief Brew-related state classes
 */

#pragma once

#include "clevercoffee/state/BaseState.h"

// Brew States
class BrewIdleState : public BaseState<MachineStateId::BREW_IDLE, BrewIdleState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};

class BrewPreinfusionState : public BaseState<MachineStateId::BREW_PREINFUSION, BrewPreinfusionState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};

class BrewPreinfusionPauseState : public BaseState<MachineStateId::BREW_PREINFUSION_PAUSE, BrewPreinfusionPauseState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};

class BrewRunningState : public BaseState<MachineStateId::BREW_RUNNING, BrewRunningState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};

class BrewFinishedState : public BaseState<MachineStateId::BREW_FINISHED, BrewFinishedState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};