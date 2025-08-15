/**
 * @file WaterSteamStates.h
 * @brief Hot water and steam states consolidated
 */

#pragma once

#include "clevercoffee/state/BaseState.h"
#include "clevercoffee/GlobalState.h"

/**
 * @brief Hot water idle state - ready to dispense hot water
 */
class HotWaterIdleState : public BaseState<MachineStateId::HOT_WATER_IDLE, HotWaterIdleState> {
public:
    static constexpr const char* STATE_NAME = "Hot Water Idle";

    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Hot water running state - dispensing hot water
 */
class HotWaterRunningState : public BaseState<MachineStateId::HOT_WATER_RUNNING, HotWaterRunningState> {
public:
    static constexpr const char* STATE_NAME = "Hot Water Running";

    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Hot water stopped state - end of hot water cycle
 */
class HotWaterStoppedState : public BaseState<MachineStateId::HOT_WATER_STOPPED, HotWaterStoppedState> {
public:
    static constexpr const char* STATE_NAME = "Hot Water Stopped";

    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Steam idle state - ready for steam operation
 */
class SteamIdleState : public BaseState<MachineStateId::STEAM_IDLE, SteamIdleState> {
public:
    static constexpr const char* STATE_NAME = "Steam Idle";

    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Steam running state - active steaming
 */
class SteamRunningState : public BaseState<MachineStateId::STEAM_RUNNING, SteamRunningState> {
public:
    static constexpr const char* STATE_NAME = "Steam Running";

    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

/**
 * @brief Steam stopped state - end of steam cycle
 */
class SteamStoppedState : public BaseState<MachineStateId::STEAM_STOPPED, SteamStoppedState> {
public:
    static constexpr const char* STATE_NAME = "Steam Stopped";

    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};
