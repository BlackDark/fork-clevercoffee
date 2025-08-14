/**
 * @file BrewHandler_Simple.h
 * @brief Simplified brew handler using global state machine
 */

#pragma once

#include "BaseHandler.h"
#include "../Config.h"
#include <Logger.h>
#include "../hardware/Relay.h"
#include "../hardware/Switch.h"
#include "../state/GlobalState.h"
#include "../state/MachineState.h"
#include "../scaleHandler.h"
#include "PumpTimer.h"

/**
 * @class BrewHandler
 * @brief Simplified brew handler that works with global state machine
 */
class BrewHandler : public SwitchBasedHandler {
private:
    PumpTimer pumpTimer_;
    unsigned long brewStartTime_ = 0;
    uint8_t lastSwitchReading_ = LOW;
    
public:
    BrewHandler() 
        : SwitchBasedHandler("BrewHandler", g_state.hardware.brewSwitch)
        , pumpTimer_(300000) { // 5 minute max brew time safety
    }
    
    bool isBrewActive() const {
        return (g_state.sensors.currBrewState != MachineStateId::BREW_IDLE && 
                g_state.sensors.currBrewState != MachineStateId::BREW_FINISHED);
    }
    
    void valveSafetyShutdownCheck() {
        // Safety check to ensure valve is closed when not brewing
        if (!isBrewActive()) {
            setRelayState(g_state.hardware.valveRelay, false);
        }
    }
    
protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesBrewEnabled.get();
    }
    
    bool hasPermission() const override {
        if (!SwitchBasedHandler::hasPermission()) {
            return false;
        }
        
        if (g_state.machine.machineState == MachineStateId::WATER_TANK_EMPTY) {
            return false;
        }
        
        // Check if hot water is active (detailed state check)
        if (g_state.sensors.currHotWaterState == MachineStateId::HOT_WATER_RUNNING) {
            return false;
        }
        
        return true;
    }
    
    void processImpl() override {
        processSwitchInput();
        checkPumpTimeout();
    }
    
private:
    void processSwitchInput() {
        if (!switch_) return;
        
        const uint8_t reading = getSwitchReading();
        g_state.sensors.brewSwitchReading = reading;
        
        const int switchType = static_cast<int>(Config::getInstance().hardwareSwitchesBrewType.get());
        
        // Simplified switch processing - just update switch state for now
        // The global state machine will handle the actual brewing logic
        if (switchType == Switch::TOGGLE) {
            // Handle toggle switch logic here
            if (reading == HIGH && lastSwitchReading_ == LOW) {
                // Switch was just pressed
                logDebug("Brew toggle switch pressed");
            }
        } else if (switchType == Switch::MOMENTARY) {
            // Handle momentary switch logic here
            if (reading == HIGH && lastSwitchReading_ == LOW) {
                logDebug("Brew momentary switch pressed");
            }
        }
        
        lastSwitchReading_ = reading;
    }
    
    void checkPumpTimeout() {
        if (pumpTimer_.isExpired() && isBrewActive()) {
            logError("Pump timeout - stopping for safety");
            g_state.sensors.currBrewState = MachineStateId::BREW_FINISHED;
        }
    }
};