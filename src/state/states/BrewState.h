/**
 * @file BrewState.h
 * @brief Brew idle state - waiting for brew to start
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

/**
 * @class BrewIdleState
 * @brief Handles brew idle state - waiting for brew switch activation
 *
 * This state monitors:
 * - Brew switch status
 * - Conditions for starting brew/backflush
 * - Transitions to appropriate brew substates
 */
class BrewIdleState : public MachineState {
public:
    BrewIdleState() = default;
    ~BrewIdleState() override = default;

    void onEntry(MachineStateContext& context) override;
    void onExit(MachineStateContext& context) override;
    void update(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

    MachineStateId getStateId() const override {
        return MachineStateId::BREW_IDLE;
    }

    const char* getStateName() const override {
        return "Brew Idle";
    }
};