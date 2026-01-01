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
        if (!systemContext_) return false;
        return systemContext_->machineStateContext()->getCurrentStateId() == MachineStateId::HOT_WATER_RUNNING;
    }

  protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesHotWaterEnabled.get();
    }

    bool hasPermission() const override {
        if (!SwitchBasedHandler::hasPermission()) {
            return false;
        }

        if (!systemContext_) return false;
        if (systemContext_->machineStateContext()->getCurrentStateId() == MachineStateId::WATER_TANK_EMPTY) {
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

        const uint8_t reading    = getSwitchReading();
        const auto    switchType = Config::getInstance().hardwareSwitchesHotWaterType.get();

        // Simplified switch processing - just update switch state for now
        // The global state machine will handle the actual hot water logic
        if (switchType == Hardware::SwitchType::TOGGLE) {
            // Handle toggle switch logic here
            if (reading == HIGH && lastSwitchReading_ == LOW) {
                logDebug("Hot water toggle switch pressed");
            }
        } else if (switchType == Hardware::SwitchType::MOMENTARY) {
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
            g_state.machine.flags.requestHotWaterStop = true; // Use condition flag instead of direct state assignment
        }
    }
};
