/**
 * @file WaterTankEmptyState.h
 * @brief Water tank empty state - placeholder implementation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class WaterTankEmptyState : public MachineState {
public:
    WaterTankEmptyState() = default;
    ~WaterTankEmptyState() override = default;
    
    void onEntry(MachineStateContext& context) override {}
    void onExit(MachineStateContext& context) override {}
    void update(MachineStateContext& context) override {}
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override { return nullptr; }
    
    int getStateId() const override { return MachineStateIds::WATER_TANK_EMPTY; }
    const char* getStateName() const override { return "Water Tank Empty"; }
};