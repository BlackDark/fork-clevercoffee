/**
 * @file HotWaterRunningState.h
 * @brief Hot water running state
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class HotWaterRunningState : public MachineState {
public:
    HotWaterRunningState() = default;
    ~HotWaterRunningState() override = default;

    void onEntry(MachineStateContext& context) override;
    void onExit(MachineStateContext& context) override;
    void update(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

    MachineStateId getStateId() const override {
        return MachineStateId::HOT_WATER_RUNNING;
    }
    
    const char* getStateName() const override {
        return "Hot Water Running";
    }
};