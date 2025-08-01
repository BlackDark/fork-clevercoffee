/**
 * @file HotWaterState.h
 * @brief Hot water state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class HotWaterState : public MachineState {
public:
    HotWaterState() = default;
    ~HotWaterState() override = default;
    
    void onEntry(MachineStateContext& context) override {}
    void onExit(MachineStateContext& context) override {}
    void update(MachineStateContext& context) override {}
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override { return nullptr; }
    
    int getStateId() const override { return MachineStateIds::HOT_WATER; }
    const char* getStateName() const override { return "Hot Water"; }
};