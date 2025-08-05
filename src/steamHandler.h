/**
 * @file steamHandler.h
 *
 * @brief Handler for digital steam switch
 */
#pragma once

#include "Config.h"
#include "hardware/Switch.h"
#include "state/GlobalState.h"
#include "state/MachineState.h"

inline void checkSteamSwitch() {
    if (!Config::getInstance().hardwareSwitchesSteamEnabled.get() || g_state.hardware.steamSwitch == nullptr) {
        return;
    }

    if (!isPowerSwitchOperationAllowed()) {
        return;
    }

    const uint8_t steamSwitchReading = g_state.hardware.steamSwitch->isPressed();

    if (static_cast<int>(Config::getInstance().hardwareSwitchesSteamType.get()) == Switch::TOGGLE) {
        // Set g_state.machine.steamON to 1 when steamswitch is HIGH
        if (steamSwitchReading == HIGH) {
            g_state.machine.steamON = true;
        }

        // if activated via web interface then steamFirstON == 1, prevent override
        if (steamSwitchReading == LOW && !g_state.machine.steamFirstON) {
            g_state.machine.steamON = false;
        }
    }
    else if (static_cast<int>(Config::getInstance().hardwareSwitchesSteamType.get()) == Switch::MOMENTARY) {
        if (steamSwitchReading != g_state.sensors.currStateSteamSwitch) {
            g_state.sensors.currStateSteamSwitch = steamSwitchReading;

            // only toggle heating power if the new button state is HIGH
            if (g_state.sensors.currStateSteamSwitch == HIGH) {
                if (g_state.machine.steamON == 0) {
                    g_state.machine.steamON = true;
                }
                else {
                    g_state.machine.steamON = false;
                }
            }
        }
    }
}
