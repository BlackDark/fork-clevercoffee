/**
 * @file TimedState.h
 * @brief Base class for states that transition after a timeout
 */

#pragma once

#include "clevercoffee/state/BaseState.h"

template<MachineStateId StateId, typename DerivedState, typename NextState>
class TimedState : public BaseState<StateId, DerivedState> {
public:
    void onEntryImpl(MachineStateContext& context) override {
        startTime_ = context.getCurrentTime();
    }

    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override {
        if (context.getCurrentTime() - startTime_ > DerivedState::TIMEOUT_MS) {
            return std::make_unique<NextState>();
        }
        return nullptr;
    }

private:
    unsigned long startTime_ = 0;
};
