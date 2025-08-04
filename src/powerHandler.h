/**
 * @file powerHandler.h
 *
 * @brief Handler for digital power switch
 */
#pragma once

#include "Config.h"
#include "state/GlobalState.h"
#include "state/MachineState.h"
#include "hardware/Switch.h"
#include "standby.h"
#include "utils/legacyUtils.h"
#include "display/displayCommon.h"

void performSafeShutdown();

inline void checkPowerSwitch() {
    if (!Config::getInstance().get<bool>("hardware.switches.power.enabled") || g_state.hardware.powerSwitch == nullptr) {
        return;
    }

    const bool powerSwitchPressed = reinterpret_cast<Switch*>(g_state.hardware.powerSwitch)->isPressed();
    const long currentMillis = millis();

    // Record when system was first initialized
    if (g_state.machine.systemInitialized && g_state.sensors.systemInitializedTime == 0) {
        g_state.sensors.systemInitializedTime = currentMillis;
    }

    if (const int powerSwitchType = Config::getInstance().get<int>("hardware.switches.power.type"); powerSwitchType == Switch::TOGGLE) {
        if (powerSwitchPressed != g_state.sensors.lastPowerSwitchPressed) {
            g_state.sensors.lastPowerSwitchPressed = powerSwitchPressed;

            if (powerSwitchPressed) {
                if (g_state.machine.machineState == LegacyMachineState::kStandby || g_state.machine.machineState == LegacyMachineState::kPidDisabled) {
                    g_state.machine.machineState = LegacyMachineState::kPidNormal;
                    resetStandbyTimer(LegacyMachineState::kPidNormal);
                    setRuntimePidState(true);
                    g_state.hardware.display->setPowerSave(0);
                }
            }
            else {
                if (g_state.machine.machineState != LegacyMachineState::kStandby) {
                    performSafeShutdown();
                    g_state.machine.machineState = LegacyMachineState::kStandby;
                    g_state.standby.standbyModeRemainingTimeMillis = 0;
                }
            }
        }
    }
    else if (powerSwitchType == Switch::MOMENTARY) {
        if (powerSwitchPressed != g_state.sensors.currStatePowerSwitchPressed) {
            g_state.sensors.currStatePowerSwitchPressed = powerSwitchPressed;

            if (g_state.sensors.currStatePowerSwitchPressed && g_state.machine.systemInitialized) {
                // Only start tracking press time if system has been initialized for at least 5 seconds
                if (currentMillis - g_state.sensors.systemInitializedTime > 5000) {
                    g_state.sensors.firstSwitchPressTime = currentMillis;
                    g_state.sensors.trackingPressTime = true;
                }

                if (g_state.machine.machineState == LegacyMachineState::kStandby) {
                    g_state.machine.machineState = LegacyMachineState::kPidNormal;
                    resetStandbyTimer(LegacyMachineState::kPidNormal);
                    setRuntimePidState(true);
                    g_state.hardware.display->setPowerSave(0);
                }
                else {
                    performSafeShutdown();
                    g_state.machine.machineState = LegacyMachineState::kStandby;
                    g_state.standby.standbyModeRemainingTimeMillis = 0;
                }
            }
            else if (!g_state.sensors.currStatePowerSwitchPressed) {
                // Switch released - stop tracking
                g_state.sensors.trackingPressTime = false;
                g_state.sensors.firstSwitchPressTime = 0;
            }
        }

        // Check for long press to trigger reboot (only for momentary switches)
        // Only reboot when:
        // 1. System is initialized
        // 2. At least 5 seconds have passed since initialization
        // 3. A press that started after initialization is actively tracked
        // 4. The press has lasted long enough for longPressDetected()
        if (powerSwitchPressed && g_state.machine.systemInitialized && (currentMillis - g_state.sensors.systemInitializedTime > 5000) && g_state.sensors.trackingPressTime && (currentMillis - g_state.sensors.firstSwitchPressTime > 1000) && // Minimum 1 second actual press
            reinterpret_cast<Switch*>(g_state.hardware.powerSwitch)->longPressDetected()) {
            LOG(INFO, "Power switch long press detected - initiating system reboot");
            g_state.hardware.display->setPowerSave(0);

            // Display reboot message
            displayMessage("REBOOTING", "Please wait...", "", "", "", "");
            delay(1000);

            performSafeShutdown();

            LOG(INFO, "System reboot initiated");
            delay(500);

            ESP.restart();
        }
    }
}

/**
 * @brief Check if power switch allows operation (for brew/steam/hot water handlers)
 * @return true if operation is allowed, false otherwise
 */
inline bool isPowerSwitchOperationAllowed() {
    if (!Config::getInstance().get<bool>("hardware.switches.power.enabled") || g_state.hardware.powerSwitch == nullptr) {
        return true; // No power switch configured, allow operation
    }

    if (const int powerSwitchType = Config::getInstance().get<int>("hardware.switches.power.type"); powerSwitchType == Switch::TOGGLE) {
        return reinterpret_cast<Switch*>(g_state.hardware.powerSwitch)->isPressed();
    }
    else if (powerSwitchType == Switch::MOMENTARY) {
        // For momentary switches, check machine state instead of switch state
        return g_state.machine.machineState != LegacyMachineState::kStandby;
    }

    return true;
}
