/**
 * @file HotWaterStates.h
 * @brief Hot water-related state classes
 */

#pragma once

#include "clevercoffee/state/BaseState.h"

// Hot Water States
class HotWaterIdleState : public BaseState<MachineStateId::HOT_WATER_IDLE, HotWaterIdleState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};

class HotWaterRunningState : public BaseState<MachineStateId::HOT_WATER_RUNNING, HotWaterRunningState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};

class HotWaterStoppedState : public BaseState<MachineStateId::HOT_WATER_STOPPED, HotWaterStoppedState> {
  public:
    void          update(MachineStateContext& context) override;
    void          onEntryImpl(MachineStateContext& context) override;
    MachineState* checkSpecificTransitions(MachineStateContext& context) override;
};