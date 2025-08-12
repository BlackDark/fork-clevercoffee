/**
 * @file ModernSteamHandler.h
 * @brief Refactored steam handler using base abstractions
 */

#pragma once

#include "BaseHandler.h"
#include "../Config.h"

/**
 * @class ModernSteamHandler
 * @brief Steam switch handler using modern base class abstractions
 * 
 * This demonstrates how the base handler eliminates duplication
 * compared to the original steamHandler.h
 */
class ModernSteamHandler : public SwitchBasedHandler {
public:
    ModernSteamHandler() 
        : SwitchBasedHandler("SteamHandler", g_state.hardware.steamSwitch) {}
    
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

/**
 * @brief Global instance and process function for compatibility
 */
inline ModernSteamHandler g_modernSteamHandler;

inline void checkSteamSwitchModern() {
    g_modernSteamHandler.process();
}