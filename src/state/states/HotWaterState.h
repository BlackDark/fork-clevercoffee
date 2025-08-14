/**
 * @file HotWaterState.h
 * @brief Hot water idle state - waiting for activation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class HotWaterIdleState : public MachineState {
public:
    HotWaterIdleState() = default;
    ~HotWaterIdleState() override = default;

    void onEntry(MachineStateContext& context) override;
    void onExit(MachineStateContext& context) override;
    void update(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

    MachineStateId getStateId() const override {
        return MachineStateId::HOT_WATER_IDLE;
    }
    
    const char* getStateName() const override {
        return "Hot Water Idle";
    }
};

// Alias for backward compatibility
using HotWaterState = HotWaterIdleState;