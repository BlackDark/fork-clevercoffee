/**
 * @file SteamState.h
 * @brief Steam state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class SteamState : public MachineState {
    public:
        SteamState() = default;
        ~SteamState() override = default;

        void onEntry(MachineStateContext& context) override {
        }
        void onExit(MachineStateContext& context) override {
        }
        void update(MachineStateContext& context) override {
        }
        std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override {
            return nullptr;
        }

        int getStateId() const override {
            return MachineStateIds::STEAM;
        }
        const char* getStateName() const override {
            return "Steam";
        }
};