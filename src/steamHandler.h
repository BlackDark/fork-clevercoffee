/**
 * @file steamHandler.h
 *
 * @brief Handler for digital steam switch
 */
#pragma once

inline uint8_t currStateSteamSwitch;

inline void checkSteamSwitch() {
    if (!Config::getInstance().get<bool>("hardware.switches.steam.enabled") || steamSwitch == nullptr) {
        return;
    }

    if (!isPowerSwitchOperationAllowed()) {
        return;
    }

    const uint8_t steamSwitchReading = steamSwitch->isPressed();

    if (Config::getInstance().get<int>("hardware.switches.steam.type") == Switch::TOGGLE) {
        // Set g_state.machine.steamON to 1 when steamswitch is HIGH
        if (steamSwitchReading == HIGH) {
            g_state.machine.steamON = true;
        }

        // if activated via web interface then steamFirstON == 1, prevent override
        if (steamSwitchReading == LOW && !steamFirstON) {
            g_state.machine.steamON = false;
        }
    }
    else if (Config::getInstance().get<int>("hardware.switches.steam.type") == Switch::MOMENTARY) {
        if (steamSwitchReading != currStateSteamSwitch) {
            currStateSteamSwitch = steamSwitchReading;

            // only toggle heating power if the new button state is HIGH
            if (currStateSteamSwitch == HIGH) {
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
