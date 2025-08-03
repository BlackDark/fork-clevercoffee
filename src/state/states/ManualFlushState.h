/**
 * @file ManualFlushState.h
 * @brief Manual flush state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class ManualFlushState : public MachineState {
    public:
        ManualFlushState() = default;
        ~ManualFlushState() override = default;

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
            return MachineStateIds::MANUAL_FLUSH;
        }
        const char* getStateName() const override {
            return "Manual Flush";
        }
};