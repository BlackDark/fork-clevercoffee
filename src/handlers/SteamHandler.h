/**
 * @file SteamHandler.h
 * @brief Handler for steam operations using modern C++ patterns
 */
#pragma once

#include "BaseHandler.h"
#include "../Config.h"
#include <Logger.h>
#include "../state/GlobalState.h"

/**
 * @class SteamHandler
 * @brief Modern steam handler using class-based architecture
 */
class SteamHandler : public SwitchBasedHandler {
public:
    SteamHandler() : SwitchBasedHandler("SteamHandler", g_state.hardware.steamSwitch) {}
    
protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesSteamEnabled.get();
    }
    
    void processImpl() override {
        const uint8_t reading = getSwitchReading();
        const int switchType = static_cast<int>(Config::getInstance().hardwareSwitchesSteamType.get());
        
        if (switchType == Switch::TOGGLE) {
            processToggleSwitch(reading, g_state.machine.steamON, g_state.machine.steamFirstON);
        }
        else if (switchType == Switch::MOMENTARY) {
            processMomentarySwitch(reading, g_state.sensors.currStateSteamSwitch, g_state.machine.steamON);
        }
    }
};

