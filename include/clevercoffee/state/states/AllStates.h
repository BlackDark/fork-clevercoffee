/**
 * @file AllStates.h
 * @brief Consolidated header for all state machine states
 */

#pragma once

#include "clevercoffee/state/BaseState.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/state/TimedState.h"

// Forward declarations
class BackflushState;
class BackflushFillingState;
class BackflushFlushingState;
class BackflushFinishedState;
class BrewIdleState;
class BrewPreinfusionState;
class BrewPreinfusionPauseState;
class BrewRunningState;
class BrewFinishedState;
class EmergencyStopState;
class SensorErrorState;
class WaterTankEmptyState;
class EepromErrorState;
class InitState;
class PidNormalState;
class PidDisabledState;
class StandbyState;
class ManualFlushIdleState;
class ManualFlushRunningState;
class HotWaterIdleState;
class HotWaterRunningState;
class HotWaterStoppedState;
class SteamIdleState;
class SteamRunningState;
class SteamStoppedState;

// Backflush States
class BackflushState : public BaseState<MachineStateId::BACKFLUSH_IDLE, BackflushState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class BackflushFillingState : public TimedState<MachineStateId::BACKFLUSH_FILLING, BackflushFillingState, BackflushFlushingState> {
public:
    static constexpr unsigned long TIMEOUT_MS = 5000;
    void update(MachineStateContext& context) override;
};

class BackflushFlushingState : public TimedState<MachineStateId::BACKFLUSH_FLUSHING, BackflushFlushingState, BackflushFinishedState> {
public:
    static constexpr unsigned long TIMEOUT_MS = 10000;
    void update(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class BackflushFinishedState : public TimedState<MachineStateId::BACKFLUSH_FINISHED, BackflushFinishedState, BackflushState> {
public:
    static constexpr unsigned long TIMEOUT_MS = 3000;
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
};

// Brew States
class BrewIdleState : public BaseState<MachineStateId::BREW_IDLE, BrewIdleState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class BrewPreinfusionState : public BaseState<MachineStateId::BREW_PREINFUSION, BrewPreinfusionState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class BrewPreinfusionPauseState : public BaseState<MachineStateId::BREW_PREINFUSION_PAUSE, BrewPreinfusionPauseState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class BrewRunningState : public BaseState<MachineStateId::BREW_RUNNING, BrewRunningState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class BrewFinishedState : public TimedState<MachineStateId::BREW_FINISHED, BrewFinishedState, BrewIdleState> {
public:
    static constexpr unsigned long TIMEOUT_MS = 3000;
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
};

// Emergency Stop State
class EmergencyStopState : public BaseState<MachineStateId::EMERGENCY_STOP, EmergencyStopState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
private:
    void performEmergencyShutdown(MachineStateContext& context);
    bool isEmergencyCleared(MachineStateContext& context) const;
    std::unique_ptr<MachineState> getRecoveryState(MachineStateContext& context) const;
};

// Error States
class SensorErrorState : public BaseState<MachineStateId::SENSOR_ERROR, SensorErrorState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
private:
    unsigned long errorStartTime_ = 0;
    static constexpr unsigned int MAX_RECOVERY_ATTEMPTS = 3;
    unsigned int recoveryAttempts_ = 0;
};

class WaterTankEmptyState : public BaseState<MachineStateId::WATER_TANK_EMPTY, WaterTankEmptyState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class EepromErrorState : public BaseState<MachineStateId::EEPROM_ERROR, EepromErrorState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

// Init State
class InitState : public BaseState<MachineStateId::INIT, InitState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
private:
    bool checkWaterTank(MachineStateContext& context) const;
    bool checkSensors(MachineStateContext& context) const;
    bool checkPidConfig(MachineStateContext& context) const;
};

// PID Normal State
class PidNormalState : public BaseState<MachineStateId::PID_NORMAL, PidNormalState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
private:
    bool shouldEnterStandby(MachineStateContext& context) const;
    void resetStandbyTimerIfNeeded(MachineStateContext& context) const;
};

// System States
class PidDisabledState : public BaseState<MachineStateId::PID_DISABLED, PidDisabledState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class StandbyState : public BaseState<MachineStateId::STANDBY, StandbyState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class ManualFlushIdleState : public BaseState<MachineStateId::MANUAL_FLUSH_IDLE, ManualFlushIdleState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class ManualFlushRunningState : public BaseState<MachineStateId::MANUAL_FLUSH_RUNNING, ManualFlushRunningState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

// Water and Steam States
class HotWaterIdleState : public BaseState<MachineStateId::HOT_WATER_IDLE, HotWaterIdleState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class HotWaterRunningState : public BaseState<MachineStateId::HOT_WATER_RUNNING, HotWaterRunningState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class HotWaterStoppedState : public TimedState<MachineStateId::HOT_WATER_STOPPED, HotWaterStoppedState, HotWaterIdleState> {
public:
    static constexpr unsigned long TIMEOUT_MS = 2000;
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
};

class SteamIdleState : public BaseState<MachineStateId::STEAM_IDLE, SteamIdleState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    void onExitImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class SteamRunningState : public BaseState<MachineStateId::STEAM_RUNNING, SteamRunningState> {
public:
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
    std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) override;
};

class SteamStoppedState : public TimedState<MachineStateId::STEAM_STOPPED, SteamStoppedState, SteamIdleState> {
public:
    static constexpr unsigned long TIMEOUT_MS = 2000;
    void update(MachineStateContext& context) override;
    void onEntryImpl(MachineStateContext& context) override;
};