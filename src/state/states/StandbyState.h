/**
 * @file StandbyState.h
 * @brief Standby state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class StandbyState : public MachineState {
    public:
        StandbyState() = default;
        ~StandbyState() override = default;

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
            return MachineStateIds::STANDBY;
        }
        const char* getStateName() const override {
            return "Standby";
        }
};