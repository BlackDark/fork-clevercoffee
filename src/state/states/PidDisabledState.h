/**
 * @file PidDisabledState.h
 * @brief PID disabled state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class PidDisabledState : public MachineState {
    public:
        PidDisabledState() = default;
        ~PidDisabledState() override = default;

        void onEntry(MachineStateContext& context) override;
        void onExit(MachineStateContext& context) override;
        void update(MachineStateContext& context) override;
        std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

        int getStateId() const override {
            return MachineStateIds::PID_DISABLED;
        }
        const char* getStateName() const override {
            return "PID Disabled";
        }
};