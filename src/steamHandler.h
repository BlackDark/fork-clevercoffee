/**
 * @file steamHandler.h
 * @brief Handler for digital steam switch using modern abstractions
 */
#pragma once

#include "handlers/BaseHandler.h"
#include "Config.h"

/**
 * @class SteamHandler
 * @brief Steam switch handler using modern base class abstractions
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

// Global instance
inline SteamHandler g_steamHandler;

// Public interface function
inline void checkSteamSwitch() {
    g_steamHandler.process();
}
