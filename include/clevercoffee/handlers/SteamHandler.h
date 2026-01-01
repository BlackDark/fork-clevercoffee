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
 */
class SteamHandler : public SwitchBasedHandler {
  public:
    explicit SteamHandler(CleverCoffee::SystemContext* ctx = nullptr) : SwitchBasedHandler("SteamHandler", nullptr, ctx) {}
    
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
             processToggleSteamSwitch(reading);
         } else if (switchType == Hardware::SwitchType::MOMENTARY) {
             // Steam switch processed via hardware state
         }
     }

   private:
     void processToggleSteamSwitch(uint8_t reading) {
         if (!systemContext_) return;

         auto* machineContext = systemContext_->machineStateContext();
         if (!machineContext) return;

         bool changed = false;

         if (reading == HIGH) {
             if (!machineContext->isSteamModeActive()) {
                 machineContext->setSteamModeActive(true);
                 changed = true;
                 logDebug("Steam toggle switch activated");
             }
         } else if (reading == LOW && !machineContext->isSteamFirstActivated()) {
             if (machineContext->isSteamModeActive()) {
                 machineContext->setSteamModeActive(false);
                 changed = true;
                 logDebug("Steam toggle switch deactivated");
             }
         }

         if (changed) {
             machineContext->setSteamFirstActivated(true);
         }
     }
};
