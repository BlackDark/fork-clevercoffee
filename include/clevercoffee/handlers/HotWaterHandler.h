/**
 * @file HotWaterHandler_Simple.h
 * @brief Simplified hot water handler using global state machine
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/handlers/BaseHandler.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/handlers/PumpTimer.h"
#include "clevercoffee/state/MachineState.h"

#include <Logger.h>

/**
 * @class HotWaterHandler
 * @brief Simplified hot water handler that works with global state machine
 */
class HotWaterHandler : public SwitchBasedHandler {
  private:
    PumpTimer pumpTimer_;
    uint8_t   lastSwitchReading_ = LOW;

  public:
    HotWaterHandler()
        : SwitchBasedHandler("HotWaterHandler", nullptr),
          pumpTimer_(60000) { // 60 second max run time
    }
    
    /**
     * @brief Initialize with hardware switch (call after HardwareManager is ready)
     * @param hotWaterSwitch Pointer to hot water switch hardware
     */
    void setHardware(Switch* hotWaterSwitch) {
        switch_ = hotWaterSwitch;
    }

    bool isHotWaterActive() const {
        // Hot water is handled via pump control in PID_NORMAL and STEAM_RUNNING
        // Check if hot water switch is pressed
        if (!switch_) return false;
        return getSwitchReading() == HIGH;
    }

  protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesHotWaterEnabled.get();
    }

    bool hasPermission() const override {
        if (!SwitchBasedHandler::hasPermission()) {
            logDebug("Base permission check failed");
            return false;
        }

        if (!systemContext_) {
            logDebug("SystemContext is null");
            return false;
        }
        
        auto currentState = systemContext_->machineStateContext()->getCurrentStateId();
        if (currentState == MachineStateId::WATER_TANK_EMPTY) {
            logDebug("Permission denied: Water tank empty");
            return false;
        }

        logDebug("Permission granted");
        return true;
    }

    void processImpl() override {
        processSwitchInput();
        checkPumpTimeout();
    }

  private:
    void processSwitchInput() {
        if (!switch_) return;

        const uint8_t reading    = getSwitchReading();
        const auto    switchType = Config::getInstance().hardwareSwitchesHotWaterType.get();

        // Simplified switch processing - just update switch state for now
        // The global state machine will handle the actual hot water logic
        if (switchType == Hardware::SwitchType::TOGGLE) {
            // Handle toggle switch logic here
            if (reading == HIGH && lastSwitchReading_ == LOW) {
                logInfo("Hot water toggle switch activated");
            } else if (reading == LOW && lastSwitchReading_ == HIGH) {
                logInfo("Hot water toggle switch deactivated");
            }
        } else if (switchType == Hardware::SwitchType::MOMENTARY) {
            // Handle momentary switch logic here
            if (reading == HIGH && lastSwitchReading_ == LOW) {
                logInfo("Hot water momentary switch pressed");
            } else if (reading == LOW && lastSwitchReading_ == HIGH) {
                logInfo("Hot water momentary switch released");
            }
        }

        lastSwitchReading_ = reading;
    }

    void checkPumpTimeout() {
         if (pumpTimer_.isExpired() && isHotWaterActive()) {
             logError("Hot water pump timeout - stopping for safety");
             // Hot water is controlled via pump - disable pump through MachineStateContext
             if (systemContext_ && systemContext_->machineStateContext()) {
                 systemContext_->machineStateContext()->disablePump();
             }
         }
     }
};
