/**
 * @file BrewHandler_Simple.h
 * @brief Simplified brew handler using global state machine
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/handlers/BaseHandler.h"
#include "clevercoffee/handlers/PumpTimer.h"
#include "clevercoffee/hardware/Relay.h"
#include "clevercoffee/hardware/Switch.h"
#include "clevercoffee/state/MachineState.h"

#include <Logger.h>

/**
 * @class BrewHandler
 * @brief Simplified brew handler that works with global state machine
 */
class BrewHandler : public SwitchBasedHandler {
  private:
    PumpTimer     pumpTimer_;
    unsigned long brewStartTime_     = 0;
    uint8_t       lastSwitchReading_ = LOW;
    Relay*        valveRelay_        = nullptr;

  public:
    BrewHandler()
        : SwitchBasedHandler("BrewHandler", nullptr),
          pumpTimer_(300000) { // 5 minute max brew time safety
    }
    
    /**
     * @brief Initialize with hardware switch and relay (call after HardwareManager is ready)
     * @param brewSwitch Pointer to brew switch hardware
     * @param valveRelay Pointer to valve relay hardware
     */
    void setHardware(Switch* brewSwitch, Relay* valveRelay) {
        switch_ = brewSwitch;
        valveRelay_ = valveRelay;
    }

    bool isBrewActive() const {
        return (isBrewState(g_state.machine.machineState) &&
                g_state.machine.machineState != MachineStateId::BREW_IDLE &&
                g_state.machine.machineState != MachineStateId::BREW_FINISHED);
    }

    void valveSafetyShutdownCheck() {
        // Safety check to ensure valve is closed when not brewing
        if (!isBrewActive() && valveRelay_) {
            setRelayState(valveRelay_, false);
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
        if (g_state.machine.machineState == MachineStateId::HOT_WATER_RUNNING) {
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

        const uint8_t reading             = getSwitchReading();
        g_state.sensors.brewSwitchReading = reading;

        const auto switchType = Config::getInstance().hardwareSwitchesBrewType.get();

        // Simplified switch processing - just update switch state for now
        // The global state machine will handle the actual brewing logic
        if (switchType == Hardware::SwitchType::TOGGLE) {
            // Handle toggle switch logic here
            if (reading == HIGH && lastSwitchReading_ == LOW) {
                // Switch was just pressed
                logDebug("Brew toggle switch pressed");
            }
        } else if (switchType == Hardware::SwitchType::MOMENTARY) {
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
            g_state.machine.flags.requestBrewStop = true; // Use condition flag instead of direct state assignment
        }
    }
};
