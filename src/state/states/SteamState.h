/**
 * @file SteamState.h
 * @brief Steam state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class SteamIdleState : public MachineState {
    public:
        SteamIdleState() = default;
        ~SteamIdleState() override = default;

        void onEntry(MachineStateContext& context) override;
        void onExit(MachineStateContext& context) override;
        void update(MachineStateContext& context) override;
        std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

        MachineStateId getStateId() const override {
            return MachineStateId::STEAM_IDLE;
        }
        const char* getStateName() const override {
            return "Steam";
        }
};

// Alias for backward compatibility
using SteamState = SteamIdleState;