/**
 * @file HotWaterHandler_Simple.h
 * @brief Simplified hot water handler using global state machine
 */

#pragma once

#include "BaseHandler.h"
#include "../Config.h"
#include <Logger.h>
#include "../state/GlobalState.h"
#include "../state/MachineState.h"
#include "PumpTimer.h"

/**
 * @class HotWaterHandler  
 * @brief Simplified hot water handler that works with global state machine
 */
class HotWaterHandler : public SwitchBasedHandler {
private:
    PumpTimer pumpTimer_;
    uint8_t lastSwitchReading_ = LOW;
    
public:
    HotWaterHandler() 
        : SwitchBasedHandler("HotWaterHandler", g_state.hardware.hotWaterSwitch)
        , pumpTimer_(60000) { // 60 second max run time
    }
    
    bool isHotWaterActive() const {
        return g_state.sensors.currHotWaterState == MachineStateId::HOT_WATER_RUNNING;
    }
    
protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesHotWaterEnabled.get();
    }
    
    bool hasPermission() const override {
        if (!SwitchBasedHandler::hasPermission()) {
            return false;
        }
        
        if (g_state.machine.machineState == MachineStateId::WATER_TANK_EMPTY) {
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
        const int switchType = static_cast<int>(Config::getInstance().hardwareSwitchesHotWaterType.get());
        
        // Simplified switch processing - just update switch state for now
        // The global state machine will handle the actual hot water logic
        if (switchType == Switch::TOGGLE) {
            // Handle toggle switch logic here
            if (reading == HIGH && lastSwitchReading_ == LOW) {
                logDebug("Hot water toggle switch pressed");
            }
        } else if (switchType == Switch::MOMENTARY) {
            // Handle momentary switch logic here
            if (reading == HIGH && lastSwitchReading_ == LOW) {
                logDebug("Hot water momentary switch pressed");
            }
        }
        
        lastSwitchReading_ = reading;
    }
    
    void checkPumpTimeout() {
        if (pumpTimer_.isExpired() && isHotWaterActive()) {
            logError("Hot water pump timeout - stopping for safety");
            g_state.sensors.currHotWaterState = MachineStateId::HOT_WATER_STOPPED;
        }
    }
};