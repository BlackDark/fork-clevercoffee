/**
 * @file BackflushState.h
 * @brief Backflush state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class BackflushState : public MachineState {
    public:
        BackflushState() = default;
        ~BackflushState() override = default;

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
            return MachineStateIds::BACKFLUSH;
        }
        const char* getStateName() const override {
            return "Backflush";
        }
};