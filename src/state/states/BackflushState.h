/**
 * @file BackflushState.h
 * @brief Backflush state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class BackflushIdleState : public MachineState {
    public:
        BackflushIdleState() = default;
        ~BackflushIdleState() override = default;

        void onEntry(MachineStateContext& context) override;
        void onExit(MachineStateContext& context) override;
        void update(MachineStateContext& context) override;
        std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

        MachineStateId getStateId() const override {
            return MachineStateId::BACKFLUSH_IDLE;
        }
        const char* getStateName() const override {
            return "Backflush";
        }
};

// Alias for backward compatibility
using BackflushState = BackflushIdleState;