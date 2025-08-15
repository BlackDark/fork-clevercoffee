/**
 * @file BrewStates.h
 * @brief All brew-related states consolidated
 */

#pragma once

#include "../BaseState.h"
#include "../GlobalState.h"

/**
 * @brief Brew idle state - ready to start brewing
 */
class BrewIdleState : public BaseState<MachineStateId::BREW_IDLE, BrewIdleState> {
public:
    static constexpr const char* STATE_NAME = "Brew Idle";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Brew preinfusion state - pre-wetting phase
 */
class BrewPreinfusionState : public BaseState<MachineStateId::BREW_PREINFUSION, BrewPreinfusionState> {
public:
    static constexpr const char* STATE_NAME = "Brew Preinfusion";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Brew preinfusion pause state - blooming phase
 */
class BrewPreinfusionPauseState : public BaseState<MachineStateId::BREW_PREINFUSION_PAUSE, BrewPreinfusionPauseState> {
public:
    static constexpr const char* STATE_NAME = "Brew Preinfusion Pause";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Brew running state - active brewing
 */
class BrewRunningState : public BaseState<MachineStateId::BREW_RUNNING, BrewRunningState> {
public:
    static constexpr const char* STATE_NAME = "Brew Running";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Brew finished state - brew cycle completed
 */
class BrewFinishedState : public BaseState<MachineStateId::BREW_FINISHED, BrewFinishedState> {
public:
    static constexpr const char* STATE_NAME = "Brew Finished";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};