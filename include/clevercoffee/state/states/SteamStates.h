/**
 * @file SteamStates.h
 * @brief Steam-related state classes
 */

#pragma once

#include "clevercoffee/state/BaseState.h"

// Steam States
class SteamRunningState : public BaseState<MachineStateId::STEAM_RUNNING, SteamRunningState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    void          onExitImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};