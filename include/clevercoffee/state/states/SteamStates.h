/**
 * @file SteamStates.h
 * @brief Steam-related state classes
 */

#pragma once

#include "clevercoffee/state/BaseState.h"

// Steam States
class SteamIdleState : public BaseState<MachineStateId::STEAM_IDLE, SteamIdleState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};

class SteamRunningState : public BaseState<MachineStateId::STEAM_RUNNING, SteamRunningState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};

class SteamStoppedState : public BaseState<MachineStateId::STEAM_STOPPED, SteamStoppedState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};