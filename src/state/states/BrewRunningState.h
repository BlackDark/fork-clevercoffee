/**
 * @file BrewRunningState.h
 * @brief Brew running state
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class BrewRunningState : public MachineState {
public:
    BrewRunningState() = default;
    ~BrewRunningState() override = default;

    void onEntry(MachineStateContext& context) override;
    void onExit(MachineStateContext& context) override;
    void update(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

    MachineStateId getStateId() const override {
        return MachineStateId::BREW_RUNNING;
    }
    
    const char* getStateName() const override {
        return "Brew Running";
    }
};