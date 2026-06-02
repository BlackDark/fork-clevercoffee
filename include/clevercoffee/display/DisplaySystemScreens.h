/**
 * @file DisplaySystemScreens.h
 * @brief Shared system screens (standby, heating logo, steam, errors, backflush)
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/display/DisplayWidgets.h"
#include "clevercoffee/display/bitmaps.h"
#include "clevercoffee/display/displayHelpers.h"
#include "clevercoffee/display/languages.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/utils/SystemUtils.h"

using CleverCoffee::Display::getCurrentDisplayState;
using CleverCoffee::Display::isHeatingLogoConditionMet;

inline bool drawSystemScreen(CleverCoffee::SystemContext& systemContext, const bool useSharedHeatingLogo) {
    if (!systemContext.hardwareContext().display() || !systemContext.machineStateContext()) return false;

    const bool isUpright = Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT;

    if (useSharedHeatingLogo && isHeatingLogoConditionMet(systemContext)) {
        systemContext.hardwareContext().display()->clearBuffer();

        displayStatusbar(systemContext);

        systemContext.hardwareContext().display()->drawXBMP(
            0, 20, Heating_Logo_width, Heating_Logo_height, Heating_Logo);
        systemContext.hardwareContext().display()->setFont(u8g2_font_fub25_tf);
        systemContext.hardwareContext().display()->setCursor(50, 30);
        systemContext.hardwareContext().display()->print(systemContext.processTemperature(), 1);
        systemContext.hardwareContext().display()->drawCircle(122, 32, 3);

        systemContext.hardwareContext().display()->sendBuffer();
        return true;
    }

    if (Config::getInstance().displayPidOffLogo.get() == 1 &&
        getCurrentDisplayState(systemContext) == MachineStateId::PID_DISABLED) {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->drawXBMP(38, 0, Off_Logo_width, Off_Logo_height, Off_Logo);
        systemContext.hardwareContext().display()->setCursor(0, 55);
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont10_tf);
        systemContext.hardwareContext().display()->print("PID is disabled manually");
        systemContext.hardwareContext().display()->sendBuffer();
        return true;
    }

    if (Config::getInstance().displayPidOffLogo.get() == 1 &&
        getCurrentDisplayState(systemContext) == MachineStateId::STANDBY) {
        systemContext.hardwareContext().display()->clearBuffer();
        if (isUpright) {
            systemContext.hardwareContext().display()->drawXBMP(6, 50, Off_Logo_width, Off_Logo_height, Off_Logo);
            systemContext.hardwareContext().display()->setCursor(1, 110);
        } else {
            systemContext.hardwareContext().display()->drawXBMP(38, 0, Off_Logo_width, Off_Logo_height, Off_Logo);
            systemContext.hardwareContext().display()->setCursor(36, 55);
        }
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont10_tf);
        systemContext.hardwareContext().display()->print("Standby mode");
        systemContext.hardwareContext().display()->sendBuffer();
        return true;
    }

    if (isSteamState(systemContext.machineStateContext()->getCurrentStateId())) {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->drawXBMP(-1, 12, Steam_Logo_width, Steam_Logo_height, Steam_Logo);

        displayTemperature(systemContext, 48, 16);

        systemContext.hardwareContext().display()->sendBuffer();
        return true;
    }

    if (getCurrentDisplayState(systemContext) == MachineStateId::WATER_TANK_EMPTY) {
        systemContext.hardwareContext().display()->clearBuffer();
        if (isUpright) {
            systemContext.hardwareContext().display()->drawXBMP(
                8, 50, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
        } else {
            systemContext.hardwareContext().display()->drawXBMP(
                45, 0, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
        }
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        systemContext.hardwareContext().display()->sendBuffer();
        return true;
    }

    if (isBackflushState(systemContext.machineStateContext()->getCurrentStateId())) {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->setFont(u8g2_font_fub17_tf);
        systemContext.hardwareContext().display()->setCursor(2, 10);
        systemContext.hardwareContext().display()->print("Backflush");

        switch (systemContext.machineStateContext()->getCurrentStateId()) {
            case MachineStateId::BACKFLUSH_IDLE:
                systemContext.hardwareContext().display()->setFont(u8g2_font_profont12_tf);
                systemContext.hardwareContext().display()->setCursor(4, 37);
                systemContext.hardwareContext().display()->print(langstring_backflush_press);
                systemContext.hardwareContext().display()->setCursor(4, 50);
                systemContext.hardwareContext().display()->print(langstring_backflush_start);
                break;

            case MachineStateId::BACKFLUSH_FINISHED:
                systemContext.hardwareContext().display()->setFont(u8g2_font_profont12_tf);
                systemContext.hardwareContext().display()->setCursor(4, 37);
                systemContext.hardwareContext().display()->print(langstring_backflush_press);
                systemContext.hardwareContext().display()->setCursor(4, 50);
                systemContext.hardwareContext().display()->print(langstring_backflush_finish);
                break;

            default:
                systemContext.hardwareContext().display()->setFont(u8g2_font_fub17_tf);
                systemContext.hardwareContext().display()->setCursor(42, 42);
                systemContext.hardwareContext().display()->print(
                    systemContext.machineStateContext()->getBackflushCycleCount(), 0);
                systemContext.hardwareContext().display()->print("/");
                systemContext.hardwareContext().display()->print(Config::getInstance().backflushCycles.get(), 0);
                break;
        }

        systemContext.hardwareContext().display()->sendBuffer();
        return true;
    }

    if (getCurrentDisplayState(systemContext) == MachineStateId::EMERGENCY_STOP) {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        systemContext.hardwareContext().display()->setCursor(32, 24);
        systemContext.hardwareContext().display()->print(langstring_current_temp);
        systemContext.hardwareContext().display()->print(systemContext.processTemperature(), 1);
        systemContext.hardwareContext().display()->print(" ");
        systemContext.hardwareContext().display()->print(static_cast<char>(176));
        systemContext.hardwareContext().display()->print("C");
        systemContext.hardwareContext().display()->setCursor(32, 34);
        systemContext.hardwareContext().display()->print(langstring_set_temp);
        systemContext.hardwareContext().display()->print(systemContext.processSetpoint(), 1);
        systemContext.hardwareContext().display()->print(" ");
        systemContext.hardwareContext().display()->print(static_cast<char>(176));
        systemContext.hardwareContext().display()->print("C");

        displayThermometerOutline(systemContext, 4, 58);

        if (systemContext.isrCounter() < 500) {
            drawTemperaturebar(systemContext, 8, 30);
            systemContext.hardwareContext().display()->setCursor(32, 4);
            systemContext.hardwareContext().display()->print("PID STOPPED");
        }

        systemContext.hardwareContext().display()->sendBuffer();

        return true;
    }

    if (getCurrentDisplayState(systemContext) == MachineStateId::SENSOR_ERROR) {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);

        char tempBuffer[16];
        snprintf(tempBuffer, sizeof(tempBuffer), "%.1f", systemContext.processTemperature());

        if (isUpright) {
            displayMessage(systemContext,
                           langstring_error_tsensor_ur[0],
                           langstring_error_tsensor_ur[1],
                           tempBuffer,
                           langstring_error_tsensor_ur[2],
                           langstring_error_tsensor_ur[3],
                           langstring_error_tsensor_ur[4]);
        } else {
            displayMessage(
                systemContext, langstring_error_tsensor[0], tempBuffer, langstring_error_tsensor[1], "", "", "");
        }
        return true;
    }

    if (getCurrentDisplayState(systemContext) == MachineStateId::EEPROM_ERROR) {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        displayMessage(systemContext, "EEPROM Error, please set Values", "", "", "", "", "");
        return true;
    }

    return false;
}
