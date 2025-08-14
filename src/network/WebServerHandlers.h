/**
 * @file WebServerHandlers.h
 * @brief Handler functions for web server API endpoints
 */

#pragma once

#include "Config.h"
#include "Logger.h"
#include "state/GlobalState.h"
#include "utils/SystemUtils.h"

/**
 * @brief Start brewing process
 */
inline void handleBrewStart() {
    if (!Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        LOG(WARNING, "Brew switch not enabled");
        return;
    }

    if (g_state.sensors.currBrewState == MachineStateId::BREW_IDLE) {
        // Trigger brew start by setting the brew switch state
        g_state.sensors.currBrewSwitchState = SwitchState::SHORT_PRESSED;
        g_state.sensors.brewSwitchWasOff = true;
        LOG(INFO, "Brew started via web API");
    }
    else {
        LOG(WARNING, "Cannot start brew - already brewing or not idle");
    }
}

/**
 * @brief Stop brewing process
 */
inline void handleBrewStop() {
    if (!Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        LOG(WARNING, "Brew switch not enabled");
        return;
    }

    if (g_state.sensors.currBrewState != MachineStateId::BREW_IDLE && g_state.sensors.currBrewState != MachineStateId::BREW_FINISHED) {
        // Stop brew by resetting brew switch state
        g_state.sensors.currBrewSwitchState = SwitchState::IDLE;
        g_state.sensors.currBrewState = MachineStateId::BREW_FINISHED;
        LOG(INFO, "Brew stopped via web API");
    }
    else {
        LOG(WARNING, "No active brew to stop");
    }
}

/**
 * @brief Start steam mode
 */
inline void handleSteamStart() {
    if (!g_state.machine.steamON) {
        setSteamMode(true);
        LOG(INFO, "Steam mode started via web API");
    }
    else {
        LOG(WARNING, "Steam mode already active");
    }
}

/**
 * @brief Stop steam mode
 */
inline void handleSteamStop() {
    if (g_state.machine.steamON) {
        setSteamMode(false);
        LOG(INFO, "Steam mode stopped via web API");
    }
    else {
        LOG(WARNING, "Steam mode not active");
    }
}

/**
 * @brief Start hot water mode
 */
inline void handleHotWaterStart() {
    // Set machine state to hot water mode
    // Set hot water state instead of main machine state
    g_state.sensors.currHotWaterState = MachineStateId::HOT_WATER_RUNNING;
    LOG(INFO, "Hot water mode started via web API");
}

/**
 * @brief Stop hot water mode
 */
inline void handleHotWaterStop() {
    // Return to normal PID mode if in hot water mode
    if (g_state.sensors.currHotWaterState == MachineStateId::HOT_WATER_RUNNING) {
        g_state.sensors.currHotWaterState = MachineStateId::HOT_WATER_IDLE;
        LOG(INFO, "Hot water mode stopped via web API");
    }
    else {
        LOG(WARNING, "Hot water mode not active");
    }
}

/**
 * @brief Tare the scale
 */
inline void handleTare() {
    if (!Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        LOG(WARNING, "Scale not enabled");
        return;
    }

    // Trigger scale tare operation
    g_state.sensors.autoTareInProgress = true;
    g_state.sensors.autoTareStartTime = millis();
    LOG(INFO, "Scale tare initiated via web API");
}

/**
 * @brief Start scale calibration
 */
inline void handleCalibration() {
    if (!Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        LOG(WARNING, "Scale not enabled");
        return;
    }

    // Note: Actual calibration logic would need to be implemented
    // This is a placeholder that sets a flag
    LOG(INFO, "Scale calibration initiated via web API");
    // TODO: Implement actual scale calibration logic
}
