/**
 * @file SystemStates.h
 * @brief System states - PID disabled, standby, manual flush
 */

#pragma once

#include "clevercoffee/state/states/clevercoffee/BaseState.h"
#include "clevercoffee/state/states/clevercoffee/GlobalState.h"

/**
 * @brief PID disabled state - operations without temperature control
 */
class PidDisabledState : public BaseState<MachineStateId::PID_DISABLED, PidDisabledState> {
public:
    static constexpr const char* STATE_NAME = "PID Disabled";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Standby state - power-saving mode
 */
class StandbyState : public BaseState<MachineStateId::STANDBY, StandbyState> {
public:
    static constexpr const char* STATE_NAME = "Standby";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Manual flush idle state - ready for manual flush
 */
class ManualFlushIdleState : public BaseState<MachineStateId::MANUAL_FLUSH_IDLE, ManualFlushIdleState> {
public:
    static constexpr const char* STATE_NAME = "Manual Flush Idle";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Manual flush running state - actively flushing
 */
class ManualFlushRunningState : public BaseState<MachineStateId::MANUAL_FLUSH_RUNNING, ManualFlushRunningState> {
public:
    static constexpr const char* STATE_NAME = "Manual Flush Running";
    
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};