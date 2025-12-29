/**
 * @file PowerHandler.h
 * @brief Handler for power operations using modern C++ patterns
 */
#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/display/displayCommon.h"
#include "clevercoffee/handlers/BaseHandler.h"
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
    unsigned long longPressStartTime_;
    bool          trackingLongPress_;

  public:
    PowerHandler()
        : SwitchBasedHandler("PowerHandler", nullptr), longPressStartTime_(0),
          trackingLongPress_(false) {}
    
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
        if (g_state.machine.systemInitialized && g_state.sensors.systemInitializedTime == 0) {
            g_state.sensors.systemInitializedTime = currentMillis;
            logInfo("System initialization time recorded");
        }
    }

    void processTogglePowerSwitch(bool pressed) {
        if (pressed != g_state.sensors.lastPowerSwitchPressed) {
            g_state.sensors.lastPowerSwitchPressed = pressed;

            if (pressed) {
                powerOn();
            } else {
                powerOff();
            }
        }
    }

    void processMomentaryPowerSwitch(bool pressed) {
        const long currentMillis = millis();

        if (pressed != g_state.sensors.currStatePowerSwitchPressed) {
            g_state.sensors.currStatePowerSwitchPressed = pressed;

            if (pressed && g_state.machine.systemInitialized) {
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
        if (currentMillis - g_state.sensors.systemInitializedTime > 5000) {
            g_state.sensors.firstSwitchPressTime = currentMillis;
            g_state.sensors.trackingPressTime    = true;
            trackingLongPress_                   = true;
            longPressStartTime_                  = currentMillis;
        }

        // Toggle power state
        if (g_state.machine.machineState == MachineStateId::STANDBY) {
            powerOn();
        } else {
            powerOff();
        }
    }

    void handlePowerButtonRelease() {
        g_state.sensors.trackingPressTime    = false;
        g_state.sensors.firstSwitchPressTime = 0;
        trackingLongPress_                   = false;
        longPressStartTime_                  = 0;
    }

    void checkForLongPressReboot(bool pressed, long currentMillis) {
        if (pressed && g_state.machine.systemInitialized &&
            (currentMillis - g_state.sensors.systemInitializedTime > 5000) && trackingLongPress_ &&
            (currentMillis - longPressStartTime_ > 1000) && switch_->longPressDetected()) {
            triggerSystemReboot();
        }
    }

    void powerOn() {
        if (g_state.machine.machineState == MachineStateId::STANDBY ||
            g_state.machine.machineState == MachineStateId::PID_DISABLED) {
            g_state.machine.flags.requestNormalOperation =
                true; // Use condition flag instead of direct state assignment
            resetStandbyTimer();
            setRuntimePidState(true);
            g_state.hardware.display->setPowerSave(0);
            logInfo("System powered on");
        }
    }

    void powerOff() {
         if (g_state.machine.machineState != MachineStateId::STANDBY) {
             g_state.coordination.processController->performSafeShutdown();
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
        g_state.hardware.display->setPowerSave(0);

        // Display reboot message
        displayMessage("REBOOTING", "Please wait...", "", "", "", "");
        delay(1000);

        g_state.coordination.processController->performSafeShutdown();

        logInfo("System reboot initiated");
        delay(1000);
        ESP.restart();
    }
};
