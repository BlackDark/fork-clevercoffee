/**
 * @file BackflushStates.h
 * @brief All backflush-related states consolidated
 */

#pragma once

#include "clevercoffee/state/states/clevercoffee/BaseState.h"
#include "clevercoffee/state/states/clevercoffee/GlobalState.h"

/**
 * @brief Backflush idle state - ready for backflush operation
 */
class BackflushState : public BaseState<MachineStateId::BACKFLUSH_IDLE, BackflushState> {
public:
    static constexpr const char* STATE_NAME = "Backflush Idle";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Backflush filling state - filling group with water
 */
class BackflushFillingState : public BaseState<MachineStateId::BACKFLUSH_FILLING, BackflushFillingState> {
public:
    static constexpr const char* STATE_NAME = "Backflush Filling";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Backflush flushing state - actively flushing
 */
class BackflushFlushingState : public BaseState<MachineStateId::BACKFLUSH_FLUSHING, BackflushFlushingState> {
public:
    static constexpr const char* STATE_NAME = "Backflush Flushing";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Backflush finished state - backflush cycle complete
 */
class BackflushFinishedState : public BaseState<MachineStateId::BACKFLUSH_FINISHED, BackflushFinishedState> {
public:
    static constexpr const char* STATE_NAME = "Backflush Finished";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};