/**
 * @file BrewHandler_Simple.h
 * @brief Simplified brew handler using global state machine
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/handlers/BaseHandler.h"
#include "clevercoffee/handlers/PumpTimer.h"
#include "clevercoffee/hardware/Relay.h"
#include "clevercoffee/hardware/Switch.h"
#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/MachineStateIds.h"

#include <Logger.h>

/**
 * @class BrewHandler
 * @brief Simplified brew handler that works with global state machine
 */
class BrewHandler : public SwitchBasedHandler {
  private:
    PumpTimer     pumpTimer_;
    unsigned long brewStartTime_      = 0;
    uint8_t       lastSwitchReading_  = LOW;
    Relay*        valveRelay_         = nullptr;
    bool          switchStateChanged_ = false; // Track if switch state changed

  public:
    explicit BrewHandler(CleverCoffee::SystemContext& ctx, const Config& config)
        : SwitchBasedHandler("BrewHandler", nullptr, ctx, config), pumpTimer_(300000) { // 5 minute max brew time safety
    }

    /**
     * @brief Initialize with hardware switch and relay (call after HardwareManager is ready)
     * @param brewSwitch Pointer to brew switch hardware
     * @param valveRelay Pointer to valve relay hardware
     */
    void setHardware(Switch* brewSwitch, Relay* valveRelay) {
        switch_     = brewSwitch;
        valveRelay_ = valveRelay;
    }

    /**
     * @brief Check if brew switch is currently pressed
     * @return true if switch is pressed, false otherwise
     */
    bool isBrewSwitchPressed() const {
        if (!switch_) return false;
        return getSwitchReading() == HIGH;
    }

    /**
     * @brief Check if switch state changed (pressed or released)
     * @return true if switch state changed since last check
     */
    bool hasSwitchStateChanged() const {
        if (!switch_) return false;
        return switchStateChanged_;
    }

    /**
     * @brief Check if switch was pressed (state changed to HIGH)
     * @return true if switch was just pressed
     */
    bool wasSwitchPressed() const {
        if (!switch_) return false;
        return switchStateChanged_ && isBrewSwitchPressed();
    }

    /**
     * @brief Check if switch was released (state changed to LOW)
     * @return true if switch was just released
     */
    bool wasSwitchReleased() const {
        if (!switch_) return false;
        return switchStateChanged_ && !isBrewSwitchPressed();
    }

    /**
     * @brief Clear switch state change flag (call after processing)
     */
    void clearSwitchStateChange() {
        switchStateChanged_ = false;
    }

    /**
     * @brief Get the switch type from config
     * @return SwitchType (TOGGLE or MOMENTARY)
     */
    Hardware::SwitchType getSwitchType() const {
        return config_.hardwareSwitchesBrewType.get();
    }

    /**
     * @brief Check if brew is currently active (in a brew state)
     * @return true if in an active brew state, false otherwise
     */
    bool isBrewActive() const {
        auto* context = systemContext_.machineStateContext();
        if (!context) return false;
        auto currentState = context->getCurrentStateId();
        return (isBrewState(currentState) && currentState != MachineStateId::BREW_FINISHED);
    }

    void valveSafetyShutdownCheck() {
        // Safety check to ensure valve is closed when not brewing
        if (!isBrewActive() && valveRelay_) {
            setRelayState(valveRelay_, false);
        }
    }

  protected:
    bool isEnabled() const override {
        return config_.hardwareSwitchesBrewEnabled.get();
    }

    bool hasPermission() const override {
        // Always allow switch input processing to detect state changes (press/release)
        // This ensures we can always log switch activations regardless of machine state
        // The state machine will handle whether the action is actually allowed

        if (!SwitchBasedHandler::hasPermission()) {
            logDebug("Base permission check failed");
            return false;
        }

        auto* context = systemContext_.machineStateContext();
        if (!context) {
            logDebug("MachineStateContext is null");
            return false;
        }

        // Only block if water tank is empty (critical safety issue)
        auto currentState = context->getCurrentStateId();
        if (currentState == MachineStateId::WATER_TANK_EMPTY) {
            logDebug("Permission denied: Water tank empty");
            return false;
        }

        // Allow processing even during hot water or brew states to detect switch releases
        // The state machine will handle the actual state transitions
        logDebug("Permission granted");
        return true;
    }

    void processImpl() override {
        // Always process switch input to detect state changes
        processSwitchInput();
        checkPumpTimeout();
    }

  private:
    void processSwitchInput() {
        if (!switch_) return;

        const uint8_t reading    = getSwitchReading();
        const auto    switchType = config_.hardwareSwitchesBrewType.get();

        // Detect state changes
        if (reading != lastSwitchReading_) {
            switchStateChanged_ = true;

            // Log switch events
            if (switchType == Hardware::SwitchType::TOGGLE) {
                if (reading == HIGH) {
                    logInfo("Brew toggle switch activated");
                } else {
                    logInfo("Brew toggle switch deactivated");
                }
            } else if (switchType == Hardware::SwitchType::MOMENTARY) {
                if (reading == HIGH) {
                    logInfo("Brew momentary switch pressed");
                } else {
                    logInfo("Brew momentary switch released");
                }
            }

            // Set flags for state machine transitions (flag-based approach fixes timing issues)
            auto* context = systemContext_.machineStateContext();
            if (!context) return;
            const auto currentState = context->getCurrentStateId();

            if (context->isBackflushModeActive() || isBackflushState(currentState)) {
                if (reading == HIGH) {
                    if (currentState == MachineStateId::BACKFLUSH_IDLE ||
                        currentState == MachineStateId::BACKFLUSH_FINISHED) {
                        context->setBackflushCycleStartRequested(true);
                    } else if (isBackflushState(currentState)) {
                        context->setBackflushStopRequested(true);
                    }
                }
                lastSwitchReading_ = reading;
                return;
            }

            // Determine if we should set start or stop flag based on switch state and current state
            if (reading == HIGH) {
                // Switch pressed/activated
                if (switchType == Hardware::SwitchType::MOMENTARY) {
                    // Momentary: press = start brew (if not already brewing)
                    if (!isBrewState(currentState) || currentState == MachineStateId::BREW_FINISHED) {
                        context->setBrewStartRequested(true);
                    } else {
                        // Already brewing - second press means stop
                        context->setBrewStopRequested(true);
                    }
                } else {
                    // Toggle: activated = start brew (if not already brewing)
                    if (!isBrewState(currentState) || currentState == MachineStateId::BREW_FINISHED) {
                        context->setBrewStartRequested(true);
                    }
                }
            } else {
                // Switch released/deactivated
                if (switchType == Hardware::SwitchType::TOGGLE) {
                    // Toggle: deactivated = stop brew (if brewing)
                    if (isBrewState(currentState) && currentState != MachineStateId::BREW_FINISHED) {
                        context->setBrewStopRequested(true);
                    }
                }
                // Momentary: release doesn't trigger stop (handled by second press)
            }
        }

        // Always update last reading to track state changes
        lastSwitchReading_ = reading;
    }

    void checkPumpTimeout() {
        if (pumpTimer_.isExpired() && isBrewActive()) {
            logError("Pump timeout - stopping for safety");
            // Request brew stop through MachineStateContext (proper state transition request)
            if (auto* context = systemContext_.machineStateContext()) {
                context->setBrewStopRequested(true);
            }
        }
    }
};
