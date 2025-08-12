/**
 * @file GlobalStateHardwareService.h
 * @brief Hardware service implementation using global state
 */

#pragma once

#include "IHardwareService.h"
#include "../state/GlobalState.h"

/**
 * @class GlobalStateHardwareService
 * @brief Hardware service implementation that wraps global state access
 * 
 * This class provides a service layer over the global state, allowing gradual
 * migration away from direct global state access
 */
class GlobalStateHardwareService : public IHardwareService {
public:
    // Relay operations
    Relay* getHeaterRelay() override {
        return g_state.hardware.heaterRelay;
    }
    
    Relay* getPumpRelay() override {
        return g_state.hardware.pumpRelay;
    }
    
    Relay* getValveRelay() override {
        return g_state.hardware.valveRelay;
    }
    
    void turnHeaterOn() override {
        if (g_state.hardware.heaterRelay) {
            g_state.hardware.heaterRelay->on();
        }
    }
    
    void turnHeaterOff() override {
        if (g_state.hardware.heaterRelay) {
            g_state.hardware.heaterRelay->off();
        }
    }
    
    void turnPumpOn() override {
        if (g_state.hardware.pumpRelay) {
            g_state.hardware.pumpRelay->on();
        }
    }
    
    void turnPumpOff() override {
        if (g_state.hardware.pumpRelay) {
            g_state.hardware.pumpRelay->off();
        }
    }
    
    void turnValveOn() override {
        if (g_state.hardware.valveRelay) {
            g_state.hardware.valveRelay->on();
        }
    }
    
    void turnValveOff() override {
        if (g_state.hardware.valveRelay) {
            g_state.hardware.valveRelay->off();
        }
    }
    
    // Switch operations
    Switch* getPowerSwitch() override {
        return g_state.hardware.powerSwitch;
    }
    
    Switch* getBrewSwitch() override {
        return g_state.hardware.brewSwitch;
    }
    
    Switch* getSteamSwitch() override {
        return g_state.hardware.steamSwitch;
    }
    
    Switch* getHotWaterSwitch() override {
        return g_state.hardware.hotWaterSwitch;
    }
    
    bool isPowerSwitchPressed() override {
        return g_state.hardware.powerSwitch && g_state.hardware.powerSwitch->isPressed();
    }
    
    bool isBrewSwitchPressed() override {
        return g_state.hardware.brewSwitch && g_state.hardware.brewSwitch->isPressed();
    }
    
    bool isSteamSwitchPressed() override {
        return g_state.hardware.steamSwitch && g_state.hardware.steamSwitch->isPressed();
    }
    
    bool isHotWaterSwitchPressed() override {
        return g_state.hardware.hotWaterSwitch && g_state.hardware.hotWaterSwitch->isPressed();
    }
    
    // LED operations
    void setStatusLed(bool state) override {
        if (g_state.hardware.statusLed) {
            if (state) {
                g_state.hardware.statusLed->on();
            } else {
                g_state.hardware.statusLed->off();
            }
        }
    }
    
    void setBrewLed(bool state) override {
        if (g_state.hardware.brewLed) {
            if (state) {
                g_state.hardware.brewLed->on();
            } else {
                g_state.hardware.brewLed->off();
            }
        }
    }
    
    void setSteamLed(bool state) override {
        if (g_state.hardware.steamLed) {
            if (state) {
                g_state.hardware.steamLed->on();
            } else {
                g_state.hardware.steamLed->off();
            }
        }
    }
};