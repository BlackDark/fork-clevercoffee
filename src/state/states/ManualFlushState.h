/**
 * @file ManualFlushState.h
 * @brief Manual flush state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class ManualFlushIdleState : public MachineState {
    public:
        ManualFlushIdleState() = default;
        ~ManualFlushIdleState() override = default;

        void onEntry(MachineStateContext& context) override;
        void onExit(MachineStateContext& context) override;
        void update(MachineStateContext& context) override;
        std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

        MachineStateId getStateId() const override {
            return MachineStateId::MANUAL_FLUSH_IDLE;
        }
        const char* getStateName() const override {
            return "Manual Flush";
        }
};

// Alias for backward compatibility
using ManualFlushState = ManualFlushIdleState;