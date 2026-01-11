/**
 * @file SteamHandler.h
 * @brief Handler for steam operations using modern C++ patterns
 */
#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/handlers/BaseHandler.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/state/MachineStateContext.h"

#include <Logger.h>

/**
 * @class SteamHandler
 * @brief Modern steam handler using class-based architecture
 * Provides raw switch state - state machine decides what to do
 */
class SteamHandler : public SwitchBasedHandler {
  public:
    explicit SteamHandler(CleverCoffee::SystemContext* ctx = nullptr) : SwitchBasedHandler("SteamHandler", nullptr, ctx), lastSwitchReading_(LOW), switchStateChanged_(false) {}
    
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
        return Config::getInstance().hardwareSwitchesSteamType.get();
    }

  protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesSteamEnabled.get();
    }

    void processImpl() override {
        processSwitchInput();
    }

   private:
    uint8_t lastSwitchReading_ = LOW;
    bool switchStateChanged_ = false;

    void processSwitchInput() {
        if (!switch_) return;
        const uint8_t reading = getSwitchReading();
        const auto switchType = Config::getInstance().hardwareSwitchesSteamType.get();

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
        }
        lastSwitchReading_ = reading;
    }
};
