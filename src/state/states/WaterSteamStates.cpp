/**
 * @file WaterSteamStates.cpp
 * @brief Hot water and steam states implementation
 */

#include "WaterSteamStates.h"
#include "../MachineStateContext.h"
#include "Logger.h"

// HotWaterIdleState Implementation
void HotWaterIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Hot water idle - ready to dispense hot water");
}

void HotWaterIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Hot Water Idle: Temp=%.1f°C, Tank=%s, Pressure=%.1fbar", 
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> HotWaterIdleState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    
    // Check for hot water start request
    if (flags.requestHotWaterStart) {
        flags.requestHotWaterStart = false;
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_RUNNING, "Hot water start requested");
        return std::make_unique<HotWaterRunningState>();
    }

    // Check if hot water switch was activated manually
    if (context.isHotWaterActive()) {
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_RUNNING, "Hot water switch activated");
        return std::make_unique<HotWaterRunningState>();
    }

    return nullptr;
}

// HotWaterRunningState Implementation
void HotWaterRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Hot water running - dispensing hot water");
}

void HotWaterRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Hot Water Running: Temp=%.1f°C, Pressure=%.1fbar", 
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> HotWaterRunningState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestHotWaterStop) {
        flags.requestHotWaterStop = false;
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_STOPPED, "Hot water stop requested");
        return std::make_unique<HotWaterStoppedState>();
    }

    // Check if hot water was deactivated
    if (!context.isHotWaterActive()) {
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_STOPPED, "Hot water deactivated");
        return std::make_unique<HotWaterStoppedState>();
    }

    return nullptr;
}

// HotWaterStoppedState Implementation
void HotWaterStoppedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Hot water dispensing stopped");
}

void HotWaterStoppedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Hot Water Stopped: Temp=%.1f°C", context.getCurrentTemperature());
}

std::unique_ptr<MachineState> HotWaterStoppedState::checkSpecificTransitions(MachineStateContext& context) {
    // Automatically transition back to hot water idle after brief delay
    static unsigned long stopTime = 0;
    if (stopTime == 0) {
        stopTime = millis();
    }
    
    constexpr unsigned long STOP_DISPLAY_TIME = 2000; // 2 seconds
    if (millis() - stopTime > STOP_DISPLAY_TIME) {
        stopTime = 0;
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_IDLE, "Hot water stop timeout");
        return std::make_unique<HotWaterIdleState>();
    }

    return nullptr;
}

// SteamIdleState Implementation
void SteamIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Steam mode activated");
    context.setSteamMode(true);
}

void SteamIdleState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting steam mode");
    context.setSteamMode(false);
}

void SteamIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Steam: Temp=%.1f°C, Tank=%s, SteamActive=%s",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.isSteamActive() ? "YES" : "NO");
}

std::unique_ptr<MachineState> SteamIdleState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestSteamStop) {
        flags.requestSteamStop = false;
        // Return to normal operation - will need StateTransitionHelper
        return nullptr; // Placeholder
    }

    // Check if steam switch was deactivated manually
    if (!context.isSteamActive()) {
        // Return to normal operation
        return nullptr; // Placeholder
    }

    return nullptr;
}

// SteamRunningState Implementation
void SteamRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Steam running - actively steaming");
}

void SteamRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Steam Running: Temp=%.1f°C, Pressure=%.1fbar", 
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> SteamRunningState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestSteamStop) {
        flags.requestSteamStop = false;
        context.logStateTransition(getStateId(), MachineStateId::STEAM_STOPPED, "Steam stop requested");
        return std::make_unique<SteamStoppedState>();
    }

    // Check if steam was deactivated
    if (!context.isSteamActive()) {
        context.logStateTransition(getStateId(), MachineStateId::STEAM_STOPPED, "Steam deactivated");
        return std::make_unique<SteamStoppedState>();
    }

    return nullptr;
}

// SteamStoppedState Implementation
void SteamStoppedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Steam stopped - steaming complete");
}

void SteamStoppedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Steam Stopped: Temp=%.1f°C", context.getCurrentTemperature());
}

std::unique_ptr<MachineState> SteamStoppedState::checkSpecificTransitions(MachineStateContext& context) {
    // Automatically transition back to steam idle after brief delay
    static unsigned long stopTime = 0;
    if (stopTime == 0) {
        stopTime = millis();
    }
    
    constexpr unsigned long STOP_DISPLAY_TIME = 2000; // 2 seconds
    if (millis() - stopTime > STOP_DISPLAY_TIME) {
        stopTime = 0;
        context.logStateTransition(getStateId(), MachineStateId::STEAM_IDLE, "Steam stop timeout");
        return std::make_unique<SteamIdleState>();
    }

    return nullptr;
}