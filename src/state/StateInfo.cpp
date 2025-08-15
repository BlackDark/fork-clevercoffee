/**
 * @file StateInfo.cpp
 * @brief Provides a central registry for all state information.
 */

#include "clevercoffee/state/StateInfo.h"
#include "clevercoffee/state/states/AllStates.h"

#include <array>
#include <algorithm>

namespace {

template<typename T>
std::unique_ptr<MachineState> make_state() {
    return std::make_unique<T>();
}

const std::array<StateInfo, 25> stateInfoRegistry = {{
    { MachineStateId::BACKFLUSH_IDLE, "Backflush Idle", make_state<BackflushState> },
    { MachineStateId::BACKFLUSH_FILLING, "Backflush Filling", make_state<BackflushFillingState> },
    { MachineStateId::BACKFLUSH_FLUSHING, "Backflush Flushing", make_state<BackflushFlushingState> },
    { MachineStateId::BACKFLUSH_FINISHED, "Backflush Finished", make_state<BackflushFinishedState> },
    { MachineStateId::BREW_IDLE, "Brew Idle", make_state<BrewIdleState> },
    { MachineStateId::BREW_PREINFUSION, "Brew Preinfusion", make_state<BrewPreinfusionState> },
    { MachineStateId::BREW_PREINFUSION_PAUSE, "Brew Preinfusion Pause", make_state<BrewPreinfusionPauseState> },
    { MachineStateId::BREW_RUNNING, "Brew Running", make_state<BrewRunningState> },
    { MachineStateId::BREW_FINISHED, "Brew Finished", make_state<BrewFinishedState> },
    { MachineStateId::EMERGENCY_STOP, "Emergency Stop", make_state<EmergencyStopState> },
    { MachineStateId::SENSOR_ERROR, "Sensor Error", make_state<SensorErrorState> },
    { MachineStateId::WATER_TANK_EMPTY, "Water Tank Empty", make_state<WaterTankEmptyState> },
    { MachineStateId::EEPROM_ERROR, "EEPROM Error", make_state<EepromErrorState> },
    { MachineStateId::INIT, "Init", make_state<InitState> },
    { MachineStateId::PID_NORMAL, "PID Normal", make_state<PidNormalState> },
    { MachineStateId::PID_DISABLED, "PID Disabled", make_state<PidDisabledState> },
    { MachineStateId::STANDBY, "Standby", make_state<StandbyState> },
    { MachineStateId::MANUAL_FLUSH_IDLE, "Manual Flush Idle", make_state<ManualFlushIdleState> },
    { MachineStateId::MANUAL_FLUSH_RUNNING, "Manual Flush Running", make_state<ManualFlushRunningState> },
    { MachineStateId::HOT_WATER_IDLE, "Hot Water Idle", make_state<HotWaterIdleState> },
    { MachineStateId::HOT_WATER_RUNNING, "Hot Water Running", make_state<HotWaterRunningState> },
    { MachineStateId::HOT_WATER_STOPPED, "Hot Water Stopped", make_state<HotWaterStoppedState> },
    { MachineStateId::STEAM_IDLE, "Steam Idle", make_state<SteamIdleState> },
    { MachineStateId::STEAM_RUNNING, "Steam Running", make_state<SteamRunningState> },
    { MachineStateId::STEAM_STOPPED, "Steam Stopped", make_state<SteamStoppedState> }
}};

} // namespace

const StateInfo* getStateInfo(MachineStateId id) {
    auto it = std::find_if(stateInfoRegistry.begin(), stateInfoRegistry.end(),
                           [id](const StateInfo& info) { return info.id == id; });
    if (it != stateInfoRegistry.end()) {
        return &(*it);
    }
    return nullptr;
}