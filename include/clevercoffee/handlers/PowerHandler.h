/**
 * @file PowerHandler.h
 * @brief Handler for power operations using modern C++ patterns
 */
#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/display/displayCommon.h"
#include "clevercoffee/handlers/BaseHandler.h"
#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateContext.h"
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
    explicit PowerHandler(CleverCoffee::SystemContext& ctx, const Config& config)
        : SwitchBasedHandler("PowerHandler", nullptr, ctx, config), longPressStartTime_(0), trackingLongPress_(false),
          currStatePowerSwitchPressed_(false), lastPowerSwitchPressed_(false), systemInitializedTime_(0),
          firstSwitchPressTime_(0), trackingPressTime_(false) {}

    /**
     * @brief Initialize with hardware switch (call after HardwareManager is ready)
     * @param powerSwitch Pointer to power switch hardware
     */
    void setHardware(Switch* powerSwitch) {
        switch_ = powerSwitch;
    }

  protected:
    bool isEnabled() const override {
        return config_.hardwareSwitchesPowerEnabled.get();
    }

    bool hasPermission() const override {
        // Power switch doesn't need standard permission checks
        logDebug("Permission granted (power switch always allowed)");
        return true;
    }

    void processImpl() override {
        recordSystemInitialization();

        const bool powerSwitchPressed = getSwitchReading();
        const auto switchType         = config_.hardwareSwitchesPowerType.get();

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
        if (systemInitializedTime_ == 0) {
            systemInitializedTime_ = currentMillis;
            logInfo("System initialization time recorded");
        }
    }

    void processTogglePowerSwitch(bool pressed) {
        if (pressed != lastPowerSwitchPressed_) {
            lastPowerSwitchPressed_ = pressed;

            if (pressed) {
                logInfo("Power toggle switch activated");
                powerOn();
            } else {
                logInfo("Power toggle switch deactivated");
                powerOff();
            }
        }
    }

    void processMomentaryPowerSwitch(bool pressed) {
        const long currentMillis = millis();

        if (pressed != currStatePowerSwitchPressed_) {
            currStatePowerSwitchPressed_ = pressed;

            if (pressed) {
                logInfo("Power momentary switch pressed");
                handlePowerButtonPress(currentMillis);
            } else if (!pressed) {
                logInfo("Power momentary switch released");
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
            trackingLongPress_    = true;
            longPressStartTime_   = currentMillis;
        }

        // Toggle power state
        auto* context = systemContext_.machineStateContext();
        if (!context) return;
        if (context->getCurrentStateId() == MachineStateId::STANDBY) {
            powerOn();
        } else {
            powerOff();
        }
    }

    void handlePowerButtonRelease() {
        trackingPressTime_    = false;
        firstSwitchPressTime_ = 0;
        trackingLongPress_    = false;
        longPressStartTime_   = 0;
    }

    void checkForLongPressReboot(bool pressed, long currentMillis) {
        if (pressed && (currentMillis - systemInitializedTime_ > 5000) && trackingLongPress_ &&
            (currentMillis - longPressStartTime_ > 1000) && switch_->longPressDetected()) {
            triggerSystemReboot();
        }
    }

    void powerOn() {
        auto* context = systemContext_.machineStateContext();
        if (!context) return;
        if ((context->getCurrentStateId() == MachineStateId::STANDBY) ||
            (context->getCurrentStateId() == MachineStateId::PID_DISABLED)) {
            // Request normal operation through MachineStateContext (proper state transition request)
            // This will automatically reset standby timer on user activity
            context->setNormalOperationRequested(true);
            setRuntimePidState(systemContext_, true);
            systemContext_.hardwareContext().display()->setPowerSave(0);
            logInfo("System powered on");
        }
    }

    void powerOff() {
        auto* context = systemContext_.machineStateContext();
        if (!context) return;
        if (context->getCurrentStateId() != MachineStateId::STANDBY) {
            if (auto* processController = systemContext_.processController()) {
                processController->performSafeShutdown();
            }
            context->setStandbyRequested(true);
            // Use StandbyCoordinator to mark immediate standby activation
            systemContext_.standbyCoordinator().setRemainingTimeMillis(0);
            logInfo("System powered off");
        }
    }

    void triggerSystemReboot() {
        logInfo("Power switch long press detected - initiating system reboot");
        systemContext_.hardwareContext().display()->setPowerSave(0);

        // Display reboot message
        displayMessage(systemContext_, "REBOOTING", "Please wait...", "", "", "", "");
        delay(1000);

        if (auto* processController = systemContext_.processController()) {
            processController->performSafeShutdown();
        }

        logInfo("System reboot initiated");
        delay(1000);
        ESP.restart();
    }
};
