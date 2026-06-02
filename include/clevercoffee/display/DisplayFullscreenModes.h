/**
 * @file DisplayFullscreenModes.h
 * @brief Fullscreen display modes (brew timer, hot water, manual flush, offline)
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/display/DisplayBrewTimerState.h"
#include "clevercoffee/display/DisplayWidgets.h"
#include "clevercoffee/display/bitmaps.h"
#include "clevercoffee/display/displayHelpers.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/utils/SystemUtils.h"

using CleverCoffee::Display::getCurrentDisplayState;

inline bool shouldDisplayHotWaterTimer(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.machineStateContext()) return false;

    auto currentState = systemContext.machineStateContext()->getCurrentStateId();
    bool pumpActive   = (systemContext.currPumpOnTime() > 0);
    return pumpActive && (currentState == MachineStateId::PID_NORMAL || currentState == MachineStateId::STEAM_RUNNING);
}

inline bool displayFullscreenBrewTimer(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.hardwareContext().display()) return false;
    if (!Config::getInstance().displayFullscreenBrewTimer.get()) {
        return false;
    }

    if (shouldDisplayBrewTimer(systemContext)) {
        systemContext.hardwareContext().display()->clearBuffer();

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            systemContext.hardwareContext().display()->drawXBMP(
                12, 12, Brew_Cup_Logo_width, Brew_Cup_Logo_height, Brew_Cup_Logo);

            if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
                systemContext.hardwareContext().display()->setFont(u8g2_font_profont22_tf);
                systemContext.hardwareContext().display()->setCursor(5, 70);
                systemContext.hardwareContext().display()->print(systemContext.processCurrentBrewTime() / 1000, 1);
                systemContext.hardwareContext().display()->print("s");
                systemContext.hardwareContext().display()->setCursor(5, 100);
                systemContext.hardwareContext().display()->print(systemContext.sensorCoordinator().getBrewWeight(), 1);
                systemContext.hardwareContext().display()->print("g");
                systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            } else {
                displayBrewtimeFs(systemContext, 1, 80, systemContext.processCurrentBrewTime());
            }
        } else {
            systemContext.hardwareContext().display()->drawXBMP(
                -1, 11, Brew_Cup_Logo_width, Brew_Cup_Logo_height, Brew_Cup_Logo);

            if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
                systemContext.hardwareContext().display()->setFont(u8g2_font_profont22_tf);
                systemContext.hardwareContext().display()->setCursor(48, 36);
                systemContext.hardwareContext().display()->print(
                    static_cast<int>(systemContext.processCurrentBrewTime() / 1000.0));
                systemContext.hardwareContext().display()->print("s");
                systemContext.hardwareContext().display()->setCursor(48, 58);
                systemContext.hardwareContext().display()->print(systemContext.sensorCoordinator().getBrewWeight(), 1);
                systemContext.hardwareContext().display()->print("g");
                systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            } else {
                displayBrewtimeFs(systemContext, 48, 25, systemContext.processCurrentBrewTime());
            }
        }

        systemContext.setDisplayBufferReady(true);
        return true;
    }

    return false;
}

inline bool displayFullscreenManualFlushTimer(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.hardwareContext().display() || !systemContext.machineStateContext()) return false;
    if (!Config::getInstance().displayFullscreenManualFlushTimer.get()) {
        return false;
    }

    if (isManualFlushState(systemContext.machineStateContext()->getCurrentStateId()) &&
        getCurrentDisplayState(systemContext) == MachineStateId::MANUAL_FLUSH_RUNNING) {
        systemContext.hardwareContext().display()->clearBuffer();

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            systemContext.hardwareContext().display()->drawXBMP(
                12, 12, Manual_Flush_Logo_width, Manual_Flush_Logo_height, Manual_Flush_Logo);
            displayBrewtimeFs(systemContext, 1, 80, systemContext.processCurrentBrewTime());
        } else {
            systemContext.hardwareContext().display()->drawXBMP(
                0, 12, Manual_Flush_Logo_width, Manual_Flush_Logo_height, Manual_Flush_Logo);
            displayBrewtimeFs(systemContext, 48, 25, systemContext.processCurrentBrewTime());
        }

        systemContext.setDisplayBufferReady(true);
        return true;
    }
    return false;
}

inline bool displayFullscreenHotWaterTimer(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.hardwareContext().display()) return false;
    if (!Config::getInstance().displayFullscreenHotWaterTimer.get()) {
        return false;
    }

    auto currentState = systemContext.machineStateContext()->getCurrentStateId();
    bool pumpActive   = (systemContext.currPumpOnTime() > 0);

    if (pumpActive && (currentState == MachineStateId::PID_NORMAL || currentState == MachineStateId::STEAM_RUNNING)) {
        systemContext.hardwareContext().display()->clearBuffer();

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            systemContext.hardwareContext().display()->drawXBMP(
                12, 12, Hot_Water_Logo_width, Hot_Water_Logo_height, Hot_Water_Logo);
            displayBrewtimeFs(systemContext, 1, 80, systemContext.currPumpOnTime());
        } else {
            systemContext.hardwareContext().display()->drawXBMP(
                0, 12, Hot_Water_Logo_width, Hot_Water_Logo_height, Hot_Water_Logo);
            displayBrewtimeFs(systemContext, 48, 25, systemContext.currPumpOnTime());
        }

        systemContext.setDisplayBufferReady(true);
        return true;
    }
    return false;
}

inline bool displayOfflineMode(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.hardwareContext().display()) return false;
    if (systemContext.uiCoordinator().getDisplayOffline() > 0 &&
        systemContext.uiCoordinator().getDisplayOffline() < 20) {
        displayMessage(systemContext, "", "", "", "", "Begin Fallback,", "No Wifi");
        systemContext.uiCoordinator().incrementDisplayOffline();
        return true;
    }

    return false;
}
