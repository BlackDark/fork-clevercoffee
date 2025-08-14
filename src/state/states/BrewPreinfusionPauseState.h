/**
 * @file BrewPreinfusionPauseState.h
 * @brief Brew preinfusion pause state
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

class BrewPreinfusionPauseState : public MachineState {
public:
    BrewPreinfusionPauseState() = default;
    ~BrewPreinfusionPauseState() override = default;

    void onEntry(MachineStateContext& context) override;
    void onExit(MachineStateContext& context) override;
    void update(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

    MachineStateId getStateId() const override {
        return MachineStateId::BREW_PREINFUSION_PAUSE;
    }
    
    const char* getStateName() const override {
        return "Brew Preinfusion Pause";
    }
};