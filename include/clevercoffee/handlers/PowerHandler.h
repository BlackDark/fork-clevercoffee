/**
 * @file PowerHandler.h
 * @brief Handler for power operations using modern C++ patterns
 */
#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/display/displayCommon.h"
#include "clevercoffee/handlers/BaseHandler.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/standby.h"
#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/utils/SystemUtils.h"

#include <Logger.h>

/**
 * @class PowerHandler
 * @brief Modern power handler using class-based architecture
 */
class PowerHandler : public SwitchBasedHandler {
  private:
    // Long press detection state
    unsigned long longPressStartTime_;
    bool          trackingLongPress_;
    
    // Power switch state tracking
    bool          currStatePowerSwitchPressed_;
    bool          lastPowerSwitchPressed_;
    unsigned long systemInitializedTime_;
    unsigned long firstSwitchPressTime_;
    bool          trackingPressTime_;

  public:
    explicit PowerHandler(CleverCoffee::SystemContext* ctx = nullptr)
        : SwitchBasedHandler("PowerHandler", nullptr, ctx), 
          longPressStartTime_(0),
          trackingLongPress_(false),
          currStatePowerSwitchPressed_(false),
          lastPowerSwitchPressed_(false),
          systemInitializedTime_(0),
          firstSwitchPressTime_(0),
          trackingPressTime_(false) {}
    
    /**
     * @brief Initialize with hardware switch (call after HardwareManager is ready)
     * @param powerSwitch Pointer to power switch hardware
     */
    void setHardware(Switch* powerSwitch) {
        switch_ = powerSwitch;
    }

  protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesPowerEnabled.get();
    }

    bool hasPermission() const override {
        // Power switch doesn't need standard permission checks
        return true;
    }

    void processImpl() override {
        recordSystemInitialization();

        const bool powerSwitchPressed = getSwitchReading();
        const auto switchType         = Config::getInstance().hardwareSwitchesPowerType.get();

        if (switchType == Hardware::SwitchType::TOGGLE) {
            processTogglePowerSwitch(powerSwitchPressed);
        } else if (switchType == Hardware::SwitchType::MOMENTARY) {
            processMomentaryPowerSwitch(powerSwitchPressed);
        }
    }

  private:
    void recordSystemInitialization() {
        const long currentMillis = millis();

        // Record when system was first initialized
        if ((systemContext_ != nullptr) && systemInitializedTime_ == 0) {
            systemInitializedTime_ = currentMillis;
            logInfo("System initialization time recorded");
        }
    }

    void processTogglePowerSwitch(bool pressed) {
        if (pressed != lastPowerSwitchPressed_) {
            lastPowerSwitchPressed_ = pressed;

            if (pressed) {
                powerOn();
            } else {
                powerOff();
            }
        }
    }

    void processMomentaryPowerSwitch(bool pressed) {
        const long currentMillis = millis();

        if (pressed != currStatePowerSwitchPressed_) {
            currStatePowerSwitchPressed_ = pressed;

            if (pressed && (systemContext_ != nullptr)) {
                handlePowerButtonPress(currentMillis);
            } else if (!pressed) {
                handlePowerButtonRelease();
            }
        }

        // Check for long press reboot
        checkForLongPressReboot(pressed, currentMillis);
    }

    void handlePowerButtonPress(long currentMillis) {
        // Only start tracking if system initialized for at least 5 seconds
        if (currentMillis - systemInitializedTime_ > 5000) {
            firstSwitchPressTime_ = currentMillis;
            trackingPressTime_    = true;
            trackingLongPress_                   = true;
            longPressStartTime_                  = currentMillis;
        }

        // Toggle power state
        if (systemContext_ && systemContext_->machineStateContext()->getCurrentStateId() == MachineStateId::STANDBY) {
            powerOn();
        } else {
            powerOff();
        }
    }

    void handlePowerButtonRelease() {
        trackingPressTime_    = false;
        firstSwitchPressTime_ = 0;
        trackingLongPress_                   = false;
        longPressStartTime_                  = 0;
    }

    void checkForLongPressReboot(bool pressed, long currentMillis) {
        if (pressed && (systemContext_ != nullptr) &&
            (currentMillis - systemInitializedTime_ > 5000) && trackingLongPress_ &&
            (currentMillis - longPressStartTime_ > 1000) && switch_->longPressDetected()) {
            triggerSystemReboot();
        }
    }

    void powerOn() {
        if ((systemContext_ && systemContext_->machineStateContext()->getCurrentStateId() == MachineStateId::STANDBY) ||
            (systemContext_ && systemContext_->machineStateContext()->getCurrentStateId() == MachineStateId::PID_DISABLED)) {
            // TODO: request normal operation through coordinator // Use condition flag instead of direct state assignment
            resetStandbyTimer();
            setRuntimePidState(true);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setPowerSave(0);
            logInfo("System powered on");
        }
    }

    void powerOff() {
         if ((!systemContext_ || systemContext_->machineStateContext()->getCurrentStateId() != MachineStateId::STANDBY)) {
             CleverCoffee::getGlobalSystemContext()->processController()->performSafeShutdown();
             g_state.machine.flags.requestStandby = true; // Use condition flag instead of direct state assignment
             // Use StandbyCoordinator to mark immediate standby activation
             if (auto* ctx = CleverCoffee::getGlobalSystemContext()) {
                 ctx->standbyCoordinator().setRemainingTimeMillis(0);
             }
             logInfo("System powered off");
         }
     }

    void triggerSystemReboot() {
        logInfo("Power switch long press detected - initiating system reboot");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setPowerSave(0);

        // Display reboot message
        displayMessage("REBOOTING", "Please wait...", "", "", "", "");
        delay(1000);

        CleverCoffee::getGlobalSystemContext()->processController()->performSafeShutdown();

        logInfo("System reboot initiated");
        delay(1000);
        ESP.restart();
    }
};
