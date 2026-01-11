/**
 * @file StateInfo.cpp
 * @brief Provides a central registry for all state information.
 */

#include "clevercoffee/state/StateInfo.h"

#include "clevercoffee/state/states/BackflushStates.h"
#include "clevercoffee/state/states/BrewStates.h"
#include "clevercoffee/state/states/EmergencyStopState.h"
#include "clevercoffee/state/states/ErrorStates.h"
#include "clevercoffee/state/states/InitState.h"
#include "clevercoffee/state/states/PidStates.h"
#include "clevercoffee/state/states/SteamStates.h"
#include "clevercoffee/state/states/SystemStates.h"

#include <algorithm>
#include <array>

namespace {

// Singleton state instances
static BackflushState            backflushState;
static BackflushFillingState     backflushFillingState;
static BackflushFlushingState    backflushFlushingState;
static BackflushFinishedState    backflushFinishedState;
static BrewPreinfusionState      brewPreinfusionState;
static BrewPreinfusionPauseState brewPreinfusionPauseState;
static BrewRunningState          brewRunningState;
static BrewFinishedState         brewFinishedState;
static EmergencyStopState        emergencyStopState;
static SensorErrorState          sensorErrorState;
static WaterTankEmptyState       waterTankEmptyState;
static EepromErrorState          eepromErrorState;
static InitState                 initState;
static PidNormalState            pidNormalState;
static PidDisabledState          pidDisabledState;
static StandbyState              standbyState;
static ManualFlushRunningState   manualFlushRunningState;
static SteamRunningState         steamRunningState;

// Singleton getter functions
template <typename T>
MachineState* get_singleton_instance();

template <>
MachineState* get_singleton_instance<BackflushState>() {
    return &backflushState;
}
template <>
MachineState* get_singleton_instance<BackflushFillingState>() {
    return &backflushFillingState;
}
template <>
MachineState* get_singleton_instance<BackflushFlushingState>() {
    return &backflushFlushingState;
}
template <>
MachineState* get_singleton_instance<BackflushFinishedState>() {
    return &backflushFinishedState;
}
template <>
MachineState* get_singleton_instance<BrewPreinfusionState>() {
    return &brewPreinfusionState;
}
template <>
MachineState* get_singleton_instance<BrewPreinfusionPauseState>() {
    return &brewPreinfusionPauseState;
}
template <>
MachineState* get_singleton_instance<BrewRunningState>() {
    return &brewRunningState;
}
template <>
MachineState* get_singleton_instance<BrewFinishedState>() {
    return &brewFinishedState;
}
template <>
MachineState* get_singleton_instance<EmergencyStopState>() {
    return &emergencyStopState;
}
template <>
MachineState* get_singleton_instance<SensorErrorState>() {
    return &sensorErrorState;
}
template <>
MachineState* get_singleton_instance<WaterTankEmptyState>() {
    return &waterTankEmptyState;
}
template <>
MachineState* get_singleton_instance<EepromErrorState>() {
    return &eepromErrorState;
}
template <>
MachineState* get_singleton_instance<InitState>() {
    return &initState;
}
template <>
MachineState* get_singleton_instance<PidNormalState>() {
    return &pidNormalState;
}
template <>
MachineState* get_singleton_instance<PidDisabledState>() {
    return &pidDisabledState;
}
template <>
MachineState* get_singleton_instance<StandbyState>() {
    return &standbyState;
}
template <>
MachineState* get_singleton_instance<ManualFlushRunningState>() {
    return &manualFlushRunningState;
}
template <>
MachineState* get_singleton_instance<SteamRunningState>() {
    return &steamRunningState;
}

const std::array<StateInfo, 18> stateInfoRegistry = {
    {{MachineStateId::BACKFLUSH_IDLE, "Backflush Idle", get_singleton_instance<BackflushState>},
     {MachineStateId::BACKFLUSH_FILLING, "Backflush Filling", get_singleton_instance<BackflushFillingState>},
     {MachineStateId::BACKFLUSH_FLUSHING, "Backflush Flushing", get_singleton_instance<BackflushFlushingState>},
     {MachineStateId::BACKFLUSH_FINISHED, "Backflush Finished", get_singleton_instance<BackflushFinishedState>},
     {MachineStateId::BREW_PREINFUSION, "Brew Preinfusion", get_singleton_instance<BrewPreinfusionState>},
     {MachineStateId::BREW_PREINFUSION_PAUSE,
      "Brew Preinfusion Pause",
      get_singleton_instance<BrewPreinfusionPauseState>},
     {MachineStateId::BREW_RUNNING, "Brew Running", get_singleton_instance<BrewRunningState>},
     {MachineStateId::BREW_FINISHED, "Brew Finished", get_singleton_instance<BrewFinishedState>},
     {MachineStateId::EMERGENCY_STOP, "Emergency Stop", get_singleton_instance<EmergencyStopState>},
     {MachineStateId::SENSOR_ERROR, "Sensor Error", get_singleton_instance<SensorErrorState>},
     {MachineStateId::WATER_TANK_EMPTY, "Water Tank Empty", get_singleton_instance<WaterTankEmptyState>},
     {MachineStateId::EEPROM_ERROR, "EEPROM Error", get_singleton_instance<EepromErrorState>},
     {MachineStateId::INIT, "Init", get_singleton_instance<InitState>},
     {MachineStateId::PID_NORMAL, "PID Normal", get_singleton_instance<PidNormalState>},
     {MachineStateId::PID_DISABLED, "PID Disabled", get_singleton_instance<PidDisabledState>},
     {MachineStateId::STANDBY, "Standby", get_singleton_instance<StandbyState>},
     {MachineStateId::MANUAL_FLUSH_RUNNING, "Manual Flush Running", get_singleton_instance<ManualFlushRunningState>},
     {MachineStateId::STEAM_RUNNING, "Steam Running", get_singleton_instance<SteamRunningState>}}
};

} // namespace

const StateInfo* getStateInfo(MachineStateId id) {
    auto it = std::find_if(
        stateInfoRegistry.begin(), stateInfoRegistry.end(), [id](const StateInfo& info) { return info.id == id; });
    if (it != stateInfoRegistry.end()) {
        return &(*it);
    }
    return nullptr;
}