/**
 * @file SteamHandler.h
 * @brief Handler for steam operations using modern C++ patterns
 */
#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/handlers/BaseHandler.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/MachineStateIds.h"

#include <Logger.h>

/**
 * @class SteamHandler
 * @brief Modern steam handler using class-based architecture
 * Provides raw switch state - state machine decides what to do
 */
class SteamHandler : public SwitchBasedHandler {
  public:
    explicit SteamHandler(CleverCoffee::SystemContext& ctx, const Config& config)
        : SwitchBasedHandler("SteamHandler", nullptr, ctx, config), lastSwitchReading_(LOW),
          switchStateChanged_(false) {}

    /**
     * @brief Initialize with hardware switch (call after HardwareManager is ready)
     * @param steamSwitch Pointer to steam switch hardware
     */
    void setHardware(Switch* steamSwitch) {
        switch_ = steamSwitch;
    }

    /**
     * @brief Check if steam switch is currently pressed
     * @return true if switch is pressed (HIGH)
     */
    bool isSteamSwitchPressed() const {
        if (!switch_) return false;
        return getSwitchReading() == HIGH;
    }

    /**
     * @brief Check if switch state changed since last check
     * @return true if switch state changed
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
        return switchStateChanged_ && isSteamSwitchPressed();
    }

    /**
     * @brief Check if switch was released (state changed to LOW)
     * @return true if switch was just released
     */
    bool wasSwitchReleased() const {
        if (!switch_) return false;
        return switchStateChanged_ && !isSteamSwitchPressed();
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
        return config_.hardwareSwitchesSteamType.get();
    }

  protected:
    bool isEnabled() const override {
        return config_.hardwareSwitchesSteamEnabled.get();
    }

    void processImpl() override {
        processSwitchInput();
    }

  private:
    uint8_t lastSwitchReading_  = LOW;
    bool    switchStateChanged_ = false;

    void processSwitchInput() {
        if (!switch_) return;
        const uint8_t reading    = getSwitchReading();
        const auto    switchType = config_.hardwareSwitchesSteamType.get();

        if (reading != lastSwitchReading_) {
            switchStateChanged_ = true;
            if (switchType == Hardware::SwitchType::TOGGLE) {
                if (reading == HIGH) {
                    logInfo("Steam toggle switch activated");
                } else {
                    logInfo("Steam toggle switch deactivated");
                }
            } else if (switchType == Hardware::SwitchType::MOMENTARY) {
                if (reading == HIGH) {
                    logInfo("Steam momentary switch pressed");
                } else {
                    logInfo("Steam momentary switch released");
                }
            }

            // Set flags for state machine transitions (flag-based approach fixes timing issues)
            auto* context = systemContext_.machineStateContext();
            if (!context) return;
            const auto currentState = context->getCurrentStateId();

            // Determine if we should set start or stop flag based on switch state and current state
            if (reading == HIGH) {
                // Switch pressed/activated
                if (switchType == Hardware::SwitchType::MOMENTARY) {
                    // Momentary: press = start steam (if not already steaming)
                    if (currentState != MachineStateId::STEAM_RUNNING) {
                        context->setSteamStartRequested(true);
                    } else {
                        // Already steaming - second press means stop
                        context->setSteamStopRequested(true);
                    }
                } else {
                    // Toggle: activated = start steam (if not already steaming)
                    if (currentState == MachineStateId::STANDBY) {
                        // In standby, only react to a rising edge so a left-on toggle does not wake steam
                        if (lastSwitchReading_ == LOW) {
                            context->setSteamStartRequested(true);
                        }
                    } else if (currentState != MachineStateId::STEAM_RUNNING) {
                        context->setSteamStartRequested(true);
                    }
                }
            } else {
                // Switch released/deactivated
                if (switchType == Hardware::SwitchType::TOGGLE) {
                    // Toggle: deactivated = stop steam (if steaming)
                    if (currentState == MachineStateId::STEAM_RUNNING) {
                        context->setSteamStopRequested(true);
                    }
                }
                // Momentary: release doesn't trigger stop (handled by second press)
            }
        }
        lastSwitchReading_ = reading;
    }
};
