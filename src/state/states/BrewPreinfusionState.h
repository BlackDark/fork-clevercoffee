/**
 * @file BrewPreinfusionState.h
 * @brief Brew preinfusion state
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class BrewPreinfusionState : public MachineState {
public:
    BrewPreinfusionState() = default;
    ~BrewPreinfusionState() override = default;

    void onEntry(MachineStateContext& context) override;
    void onExit(MachineStateContext& context) override;
    void update(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

    MachineStateId getStateId() const override {
        return MachineStateId::BREW_PREINFUSION;
    }
    
    const char* getStateName() const override {
        return "Brew Preinfusion";
    }
};