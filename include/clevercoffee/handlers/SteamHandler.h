/**
 * @file SteamHandler.h
 * @brief Handler for steam operations using modern C++ patterns
 */
#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/handlers/BaseHandler.h"

#include <Logger.h>

/**
 * @class SteamHandler
 * @brief Modern steam handler using class-based architecture
 */
class SteamHandler : public SwitchBasedHandler {
  public:
    SteamHandler() : SwitchBasedHandler("SteamHandler", nullptr) {}
    
    /**
     * @brief Initialize with hardware switch (call after HardwareManager is ready)
     * @param steamSwitch Pointer to steam switch hardware
     */
    void setHardware(Switch* steamSwitch) {
        switch_ = steamSwitch;
    }

  protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesSteamEnabled.get();
    }

    void processImpl() override {
        const uint8_t reading    = getSwitchReading();
        const auto    switchType = Config::getInstance().hardwareSwitchesSteamType.get();

        if (switchType == Hardware::SwitchType::TOGGLE) {
            processToggleSwitch(reading, g_state.machine.steamON, g_state.machine.steamFirstON);
        } else if (switchType == Hardware::SwitchType::MOMENTARY) {
            processMomentarySwitch(reading, g_state.sensors.currStateSteamSwitch, g_state.machine.steamON);
        }
    }
};
