/**
 * @file displayCommon.h
 *
 * @brief Common functions for all display templates
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/network/CleverCoffeeWiFiManager.h"
#include "clevercoffee/state/GlobalState.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/utils/SystemUtils.h"
#include "clevercoffee/display/bitmaps.h"
#include "clevercoffee/display/languages.h"
#include <PID_v1.h>  // Required for PID methods in display templates
#include <U8g2lib.h> // Required for U8G2 display methods

inline const u8g2_cb_t* getU8G2Rotation(const int rotationValue) {
    switch (rotationValue) {
        case 0:
            return U8G2_R0;
        case 1:
            return U8G2_R1;
        case 2:
            return U8G2_R2;
        case 3:
            return U8G2_R3;
        default:
            return U8G2_R0;
    }
}

/**
 * @brief print error message for scales
 */
inline void displayScaleFailed() {
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawStr(0, 32, "Failed!");
        g_state.hardware.display->drawStr(0, 42, "Scale");
        g_state.hardware.display->drawStr(0, 52, "not");
        g_state.hardware.display->drawStr(0, 62, "working...");
        g_state.hardware.display->sendBuffer();
    }
    else {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawStr(0, 32, "failed!");
        g_state.hardware.display->drawStr(0, 42, "Scale not working..."); // scale timeout will most likely trigger after OTA update, but will still work after boot
        g_state.hardware.display->sendBuffer();
    }
}

/**
 * @brief Draw the system uptime at the given coordinates
 */
inline void displayUptime(const int x, const int y, const char* format) {
    // Show uptime of machine
    unsigned long seconds = millis() / 1000;
    const unsigned long hours = seconds / 3600;
    const unsigned long minutes = seconds % 3600 / 60;
    seconds = seconds % 60;

    char uptimeString[9];
    snprintf(uptimeString, sizeof(uptimeString), format, hours, minutes, seconds);

    g_state.hardware.display->setFont(u8g2_font_profont11_tf);
    g_state.hardware.display->drawStr(x, y, uptimeString);
}

/**
 * @brief Draw a WiFi signal strength indicator at the given coordinates
 */
inline void displayWiFiStatus(const int x, const int y) {
    if (WiFi.status() == WL_CONNECTED) {
        g_state.hardware.display->drawXBMP(x, y, 8, 8, Antenna_OK_Icon);

        for (int b = 0; b <= g_state.network.cleverCoffeeWiFiManager->getSignalStrength(); b++) {
            g_state.hardware.display->drawVLine(x + 5 + b * 2, y + 8 - b * 2, b * 2);
        }
    }
    else {
        g_state.hardware.display->drawXBMP(x, y, 8, 8, Antenna_NOK_Icon);

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            g_state.hardware.display->setCursor(x + 12, y - 1);
        }
        else {
            g_state.hardware.display->setCursor(x + 36, y - 1);
        }

        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        g_state.hardware.display->print("RC: ");
        g_state.hardware.display->print(g_state.network.wifiReconnects);
    }
}

/**
 * @brief Draw an MQTT status indicator at the given coordinates if MQTT is enabled
 */
inline void displayMQTTStatus(const int x, const int y) {
    if (Config::getInstance().mqttEnabled.get()) {
        if (g_state.network.mqttManager && g_state.network.mqttManager->isConnected()) {
            g_state.hardware.display->setCursor(x, y);
            g_state.hardware.display->setFont(u8g2_font_profont11_tf);
            g_state.hardware.display->print("MQTT");

            if (g_state.network.cleverCoffeeWiFiManager->getSignalStrength() <= 1) {
                g_state.hardware.display->print("!");
            }
        }
        else {
            g_state.hardware.display->setCursor(x, y);
            g_state.hardware.display->print("");
        }
    }
}

/**
 * @brief Draw the outline of a thermometer for use in conjunction with the drawTemperaturebar method
 */
inline void displayThermometerOutline(const int x, const int y) {
    g_state.hardware.display->drawLine(x + 3, y - 9, x + 3, y - 42);
    g_state.hardware.display->drawLine(x + 9, y - 9, x + 9, y - 42);
    g_state.hardware.display->drawPixel(x + 4, y - 43);
    g_state.hardware.display->drawPixel(x + 8, y - 43);
    g_state.hardware.display->drawLine(x + 5, y - 44, x + 7, y - 44);
    g_state.hardware.display->drawDisc(x + 6, y - 5, 6);

    // draw setpoint line
    const int height = map(static_cast<int>(g_state.process.setpoint), 0, 100, y - 9, y - 39);
    g_state.hardware.display->drawLine(x + 11, height, x + 16, height);
}

/**
 * @brief Draw temperature bar, e.g. inside the thermometer outline.
 *        Add 4 pixels to the x-coordinate and subtract 12 pixels from the y-coordinate of the thermometer.
 */
inline void drawTemperaturebar(const int x, const int heightRange) {
    const int width = x + 5;

    for (int i = x; i < width; i++) {
        const int height = map(static_cast<int>(g_state.process.temperature), 0, 100, 0, heightRange);
        g_state.hardware.display->drawVLine(i, 52 - height, height);
    }

    if (g_state.process.temperature > 100) {
        g_state.hardware.display->drawLine(x, heightRange - 11, x + 3, heightRange - 11);
        g_state.hardware.display->drawLine(x, heightRange - 10, x + 4, heightRange - 10);
        g_state.hardware.display->drawLine(x, heightRange - 9, x + 4, heightRange - 9);
    }
}

/**
 * @brief Draw the temperature in big font at given position
 */
inline void displayTemperature(const int x, const int y) {
    g_state.hardware.display->setFont(u8g2_font_fub30_tf);

    if (g_state.process.temperature < 99.499) {
        g_state.hardware.display->setCursor(x + 20, y);
        g_state.hardware.display->print(g_state.process.temperature, 0);
    }
    else {
        g_state.hardware.display->setCursor(x, y);
        g_state.hardware.display->print(g_state.process.temperature, 0);
    }

    g_state.hardware.display->drawCircle(x + 72, y + 4, 3);
}

/**
 * @brief determines if brew timer should be visible; postBrewTimerDuration defines how long the timer after the brew is shown
 * @return true if timer should be visible, false otherwise
 */
inline bool shouldDisplayBrewTimer() {

    enum BrewTimerState {
        kBrewTimerIdle = 10,
        kBrewTimerRunning = 20,
        kBrewTimerPostBrew = 30
    };

    static BrewTimerState currBrewTimerState = kBrewTimerIdle;

    static uint32_t brewEndTime = 0;

    switch (currBrewTimerState) {
        case kBrewTimerIdle:
            if (g_state.handlers.brewHandler && g_state.handlers.brewHandler->isBrewActive()) {
                currBrewTimerState = kBrewTimerRunning;
            }
            break;

        case kBrewTimerRunning:
            if (!g_state.handlers.brewHandler || !g_state.handlers.brewHandler->isBrewActive()) {
                currBrewTimerState = kBrewTimerPostBrew;
                brewEndTime = millis();
            }
            break;

        case kBrewTimerPostBrew:
            if (millis() - brewEndTime > static_cast<uint32_t>(Config::getInstance().displayPostBrewTimerDuration.get() * 1000)) {
                currBrewTimerState = kBrewTimerIdle;
            }
            break;
    }

    return currBrewTimerState != kBrewTimerIdle;
}

/**
 * @brief Draw current brew time with optional brew target time at given position
 *
 * Shows the current brew time in seconds. If a target time (totalTargetBrewTime) is provided (> 0), it is displayed alongside the current time.
 *
 * @param x              Horizontal position to start drawing
 * @param y              Vertical position to start drawing
 * @param label          Text label to display before the time
 * @param currBrewTime     Current brewed time in milliseconds
 * @param totalTargetBrewTime  Target brew time in milliseconds (optional, default -1)
 */
inline void displayBrewTime(const int x, const int y, const char* label, const double currBrewTime, const double totalTargetBrewTime = -1) {
    g_state.hardware.display->setDrawColor(0);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::MINIMAL) {
        g_state.hardware.display->drawBox(x, y, 100, 15);
    }
    else {
        g_state.hardware.display->drawBox(x, y + 1, 100, 10);
    }

    g_state.hardware.display->setDrawColor(1);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        g_state.hardware.display->setCursor(x, y);
        g_state.hardware.display->print(label);
        g_state.hardware.display->print(currBrewTime / 1000, 0);

        if (totalTargetBrewTime > 0) {
            g_state.hardware.display->print("/");
            g_state.hardware.display->print(totalTargetBrewTime / 1000, 0);
        }
        g_state.hardware.display->print(" s");
    }
    else {
        g_state.hardware.display->setCursor(x, y);
        g_state.hardware.display->print(label);
        g_state.hardware.display->setCursor(x + 50, y);
        g_state.hardware.display->print(currBrewTime / 1000, 0);

        if (totalTargetBrewTime > 0) {
            g_state.hardware.display->print("/");
            g_state.hardware.display->print(totalTargetBrewTime / 1000, 0);
        }

        g_state.hardware.display->print(" s");
    }
}

/**
 * @brief Draw the current weight with error handling and target indicators at given position
 *
 * If the scale reports an error, "fault" is shown on the display instead of weight.
 * Otherwise, the function displays the current weight.
 * If a target weight (setpoint) is set (> 0), it will be displayed alongside the current weight.
 * This function is intended to provide the status of the scales during brewing, flushing, or other machine states.
 *
 * @param x        Horizontal position to start drawing
 * @param y        Vertical position to start drawing
 * @param weight   Current measured weight to display
 * @param setpoint Target weight to display alongside current weight (optional, default -1)
 * @param fault    Indicates if the scale has an error (optional, default false)
 */
inline void displayBrewWeight(const int x, const int y, const float weight, const float setpoint = -1, const bool fault = false) {
    g_state.hardware.display->setDrawColor(0);
    g_state.hardware.display->drawBox(x, y + 1, 100, 10);
    g_state.hardware.display->setDrawColor(1);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        if (fault) {
            g_state.hardware.display->setCursor(x, y);
            g_state.hardware.display->print(langstring_weight_ur);
            g_state.hardware.display->print(langstring_scale_Failure);
            return;
        }

        g_state.hardware.display->setCursor(x, y);
        g_state.hardware.display->print(langstring_weight_ur);
        g_state.hardware.display->print(weight, 0);

        if (setpoint > 0) {
            g_state.hardware.display->print("/");
            g_state.hardware.display->print(setpoint, 0);
        }

        g_state.hardware.display->print(" g");
    }
    else {
        if (fault) {
            g_state.hardware.display->setCursor(x, y);
            g_state.hardware.display->print(langstring_weight);
            g_state.hardware.display->setCursor(x + 50, y);
            g_state.hardware.display->print(langstring_scale_Failure);
            return;
        }

        g_state.hardware.display->setCursor(x, y);
        g_state.hardware.display->print(langstring_weight);
        g_state.hardware.display->setCursor(x + 50, y);
        g_state.hardware.display->print(weight, 0);

        if (setpoint > 0) {
            g_state.hardware.display->print("/");
            g_state.hardware.display->print(setpoint, 0);
        }

        g_state.hardware.display->print(" g");
    }
}

/**
 * @brief Draw the brew time at given position (fullscreen brewtimer)
 */
inline void displayBrewtimeFs(const int x, const int y, const double brewtime) {
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        g_state.hardware.display->setFont(u8g2_font_fub20_tf);
        if (brewtime < 9950.000) {
            g_state.hardware.display->setCursor(x + 15, y);
        }
        else {
            g_state.hardware.display->setCursor(x, y);
        }
        g_state.hardware.display->print(brewtime / 1000, 1);
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        g_state.hardware.display->setCursor(x + 56, y + 12);
        g_state.hardware.display->print("s");
    }
    else {
        g_state.hardware.display->setFont(u8g2_font_fub25_tf);

        if (brewtime < 9950.000) {
            g_state.hardware.display->setCursor(x + 16, y);
        }
        else {
            g_state.hardware.display->setCursor(x, y);
        }

        g_state.hardware.display->print(brewtime / 1000, 1);
        g_state.hardware.display->setFont(u8g2_font_profont15_tf);

        if (brewtime < 9950.000) {
            g_state.hardware.display->setCursor(x + 67, y + 14);
        }
        else {
            g_state.hardware.display->setCursor(x + 69, y + 14);
        }

        g_state.hardware.display->print("s");
    }

    g_state.hardware.display->setFont(u8g2_font_profont11_tf);
}

/**
 * @brief Draw a bar visualizing the output in % at the given coordinates and with the given width
 */
inline void displayProgressbar(const int value, const int x, const int y, const int width) {
    g_state.hardware.display->drawFrame(x, y, width, 4);

    if (const int output = map(value, 0, 100, 0, width); output - 2 > 0) {
        g_state.hardware.display->drawLine(x + 1, y + 1, x + output - 1, y + 1);
        g_state.hardware.display->drawLine(x + 1, y + 2, x + output - 1, y + 2);
    }
}

inline void displayBluetoothStatus(const int x, const int y) {
    if (g_state.hardware.scale) {
        if (const bool connected = g_state.hardware.scale->isConnected(); connected) {
            g_state.hardware.display->drawXBMP(x, y, 8, 9, Bluetooth_Icon);
        }
    }
}

/**
 * @brief Draw a status bar at the top of the screen with icons for WiFi, MQTT,
 *        the system uptime and a separator line underneath
 */
inline void displayStatusbar() {
    // For status info
    g_state.hardware.display->drawLine(0, STATUS_BAR_Y_POS, DISPLAY_WIDTH, STATUS_BAR_Y_POS);

    if (!g_state.network.offlineMode) {
        displayWiFiStatus(4, 1);
        displayMQTTStatus(40, 0);
    }
    else {
        g_state.hardware.display->setCursor(40, 0);
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        g_state.hardware.display->print(langstring_offlinemode);
    }

    if (Config::getInstance().hardwareSensorsScaleEnabled.get() && Config::getInstance().hardwareSensorsScaleType.get() == Hardware::ScaleType::BLUETOOTH) {
        displayBluetoothStatus(24, 1);
    }

    const auto format = "%02luh %02lum";
    displayUptime(84, 0, format);
}

/**
 * @brief print message
 */
inline void displayMessage(const char* text1, const char* text2, const char* text3, const char* text4, const char* text5, const char* text6) {
    g_state.hardware.display->clearBuffer();
    g_state.hardware.display->setCursor(0, 0);
    g_state.hardware.display->print(text1);
    g_state.hardware.display->setCursor(0, 10);
    g_state.hardware.display->print(text2);
    g_state.hardware.display->setCursor(0, 20);
    g_state.hardware.display->print(text3);
    g_state.hardware.display->setCursor(0, 30);
    g_state.hardware.display->print(text4);
    g_state.hardware.display->setCursor(0, 40);
    g_state.hardware.display->print(text5);
    g_state.hardware.display->setCursor(0, 50);
    g_state.hardware.display->print(text6);
    g_state.hardware.display->sendBuffer();
}

/**
 * @brief print logo and message at boot
 */
inline void displayLogo(const char* displaymessagetext, const char* displaymessagetext2) {
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        int printrow = 47;
        g_state.hardware.display->clearBuffer();

        // Use stack allocation for tokenization buffers
        constexpr size_t MAX_MSG_LEN = SHORT_MESSAGE_SIZE;
        char text1[MAX_MSG_LEN];
        char text2[MAX_MSG_LEN];

        strncpy(text1, displaymessagetext, MAX_MSG_LEN - 1);
        text1[MAX_MSG_LEN - 1] = '\0';
        strncpy(text2, displaymessagetext2, MAX_MSG_LEN - 1);
        text2[MAX_MSG_LEN - 1] = '\0';

        char* token = strtok(text1, " ");

        while (token != nullptr) {
            g_state.hardware.display->drawStr(0, printrow, token);
            token = strtok(nullptr, " ");
            printrow += 10;
        }

        token = strtok(text2, " ");

        while (token != nullptr) {
            g_state.hardware.display->drawStr(0, printrow, token);
            token = strtok(nullptr, " ");
            printrow += 10;
        }

        g_state.hardware.display->drawXBMP(11, 4, CleverCoffee_Logo_width, CleverCoffee_Logo_height, CleverCoffee_Logo);
    }
    else {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawStr(0, 45, displaymessagetext);
        g_state.hardware.display->drawStr(0, 55, displaymessagetext2);
        g_state.hardware.display->drawXBMP(38, 0, CleverCoffee_Logo_width, CleverCoffee_Logo_height, CleverCoffee_Logo);
    }

    g_state.hardware.display->sendBuffer();
}

/**
 * @brief display fullscreen brew timer
 */
inline bool displayFullscreenBrewTimer() {
    if (!Config::getInstance().displayFullscreenBrewTimer.get()) {
        return false;
    }

    if (shouldDisplayBrewTimer()) {
        g_state.hardware.display->clearBuffer();

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            g_state.hardware.display->drawXBMP(12, 12, Brew_Cup_Logo_width, Brew_Cup_Logo_height, Brew_Cup_Logo);

            if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
                g_state.hardware.display->setFont(u8g2_font_profont22_tf);
                g_state.hardware.display->setCursor(5, 70);
                g_state.hardware.display->print(g_state.process.currBrewTime / 1000, 1);
                g_state.hardware.display->print("s");
                g_state.hardware.display->setCursor(5, 100);
                g_state.hardware.display->print(g_state.sensors.currBrewWeight, 1);
                g_state.hardware.display->print("g");
                g_state.hardware.display->setFont(u8g2_font_profont11_tf);
            }
            else {
                displayBrewtimeFs(1, 80, g_state.process.currBrewTime);
            }
        }
        else {
            g_state.hardware.display->drawXBMP(-1, 11, Brew_Cup_Logo_width, Brew_Cup_Logo_height, Brew_Cup_Logo);

            if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
                g_state.hardware.display->setFont(u8g2_font_profont22_tf);
                g_state.hardware.display->setCursor(DISPLAY_WIDTH/2, 15);
                g_state.hardware.display->print(g_state.process.currBrewTime / 1000, 1);
                g_state.hardware.display->print("s");
                g_state.hardware.display->setCursor(DISPLAY_WIDTH/2, 38);
                g_state.hardware.display->print(g_state.sensors.currBrewWeight, 1);
                g_state.hardware.display->print("g");
                g_state.hardware.display->setFont(u8g2_font_profont11_tf);
            }
            else {
                displayBrewtimeFs(48, 25, g_state.process.currBrewTime);
            }
        }

        g_state.coordination.displayBufferReady = true;
        return true;
    }

    return false;
}

/**
 * @brief display fullscreen manual flush timer
 */
inline bool displayFullscreenManualFlushTimer() {
    if (!Config::getInstance().displayFullscreenManualFlushTimer.get()) {
        return false;
    }

    if (isManualFlushState(g_state.machine.machineState) && g_state.machine.machineState == MachineStateId::MANUAL_FLUSH_RUNNING) {
        g_state.hardware.display->clearBuffer();

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            g_state.hardware.display->drawXBMP(12, 12, Manual_Flush_Logo_width, Manual_Flush_Logo_height, Manual_Flush_Logo);
            displayBrewtimeFs(1, 80, g_state.process.currBrewTime);
        }
        else {
            g_state.hardware.display->drawXBMP(0, 12, Manual_Flush_Logo_width, Manual_Flush_Logo_height, Manual_Flush_Logo);
            displayBrewtimeFs(48, 25, g_state.process.currBrewTime);
        }

        g_state.coordination.displayBufferReady = true;
        return true;
    }
    return false;
}

/**
 * @brief display fullscreen hot water on timer
 */
inline bool displayFullscreenHotWaterTimer() {
    if (!Config::getInstance().displayFullscreenHotWaterTimer.get()) {
        return false;
    }

    if (isHotWaterState(g_state.machine.machineState) && g_state.machine.machineState == MachineStateId::HOT_WATER_RUNNING) {
        g_state.hardware.display->clearBuffer();

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            g_state.hardware.display->drawXBMP(12, 12, Hot_Water_Logo_width, Hot_Water_Logo_height, Hot_Water_Logo);
            displayBrewtimeFs(1, 80, g_state.sensors.currPumpOnTime);
        }
        else {
            g_state.hardware.display->drawXBMP(0, 12, Hot_Water_Logo_width, Hot_Water_Logo_height, Hot_Water_Logo);
            displayBrewtimeFs(48, 25, g_state.sensors.currPumpOnTime);
        }

        g_state.coordination.displayBufferReady = true;
        return true;
    }
    return false;
}

/**
 * @brief display offline message
 */
inline bool displayOfflineMode() {

    if (g_state.display.displayOffline > 0 && g_state.display.displayOffline < 20) {
        displayMessage("", "", "", "", "Begin Fallback,", "No Wifi");
        g_state.display.displayOffline++;
        return true;
    }

    return false;
}

/**
 * @brief display heating logo
 */
inline bool displayMachineState() {

    if (displayOfflineMode()) {
        return true;
    }

    // Show the heating logo when we are in regular PID mode and more than 5degC below the set point
    if (Config::getInstance().displayHeatingLogo.get() > 0 && (g_state.machine.machineState == MachineStateId::PID_NORMAL) && g_state.process.setpoint - g_state.process.temperature > 5.) {
        // For status info
        g_state.hardware.display->clearBuffer();

        displayStatusbar();

        g_state.hardware.display->drawXBMP(0, 20, Heating_Logo_width, Heating_Logo_height, Heating_Logo);
        g_state.hardware.display->setFont(u8g2_font_fub25_tf);
        g_state.hardware.display->setCursor(50, 30);
        g_state.hardware.display->print(g_state.process.temperature, 1);
        g_state.hardware.display->drawCircle(122, 32, 3);

        g_state.hardware.display->sendBuffer();
        return true;
    }

    // Offline logo
    if (Config::getInstance().displayPidOffLogo.get() == 1 && g_state.machine.machineState == MachineStateId::PID_DISABLED) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawXBMP(38, 0, Off_Logo_width, Off_Logo_height, Off_Logo);
        g_state.hardware.display->setCursor(0, 55);
        g_state.hardware.display->setFont(u8g2_font_profont10_tf);
        g_state.hardware.display->print("PID is disabled manually");
        g_state.hardware.display->sendBuffer();
        return true;
    }

    if (Config::getInstance().displayPidOffLogo.get() == 1 && g_state.machine.machineState == MachineStateId::STANDBY) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawXBMP(38, 0, Off_Logo_width, Off_Logo_height, Off_Logo);
        g_state.hardware.display->setCursor(36, 55);
        g_state.hardware.display->setFont(u8g2_font_profont10_tf);
        g_state.hardware.display->print("Standby mode");
        g_state.hardware.display->sendBuffer();
        return true;
    }

    // Steam
    if (isSteamState(g_state.machine.machineState)) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawXBMP(-1, 12, Steam_Logo_width, Steam_Logo_height, Steam_Logo);

        displayTemperature(48, 16);

        g_state.hardware.display->sendBuffer();
        return true;
    }

    // Water empty
    if (g_state.machine.machineState == MachineStateId::WATER_TANK_EMPTY) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawXBMP(45, 0, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        g_state.hardware.display->sendBuffer();
        return true;
    }

    // Backflush
    if (isBackflushState(g_state.machine.machineState)) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->setFont(u8g2_font_fub17_tf);
        g_state.hardware.display->setCursor(2, 10);
        g_state.hardware.display->print("Backflush");

        switch (g_state.machine.machineState) {
            case MachineStateId::BACKFLUSH_IDLE:
                g_state.hardware.display->setFont(u8g2_font_profont12_tf);
                g_state.hardware.display->setCursor(4, 37);
                g_state.hardware.display->print(langstring_backflush_press);
                g_state.hardware.display->setCursor(4, 50);
                g_state.hardware.display->print(langstring_backflush_start);
                break;

            case MachineStateId::BACKFLUSH_FINISHED:
                g_state.hardware.display->setFont(u8g2_font_profont12_tf);
                g_state.hardware.display->setCursor(4, 37);
                g_state.hardware.display->print(langstring_backflush_press);
                g_state.hardware.display->setCursor(4, 50);
                g_state.hardware.display->print(langstring_backflush_finish);
                break;

            default:
                g_state.hardware.display->setFont(u8g2_font_fub17_tf);
                g_state.hardware.display->setCursor(42, 42);
                g_state.hardware.display->print(g_state.machine.currBackflushCycles, 0);
                g_state.hardware.display->print("/");
                g_state.hardware.display->print(Config::getInstance().backflushCycles.get(), 0);
                break;
        }

        g_state.hardware.display->sendBuffer();
        return true;
    }

    // PID Off
    if (g_state.machine.machineState == MachineStateId::EMERGENCY_STOP) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        g_state.hardware.display->setCursor(32, 24);
        g_state.hardware.display->print(langstring_current_temp);
        g_state.hardware.display->print(g_state.process.temperature, 1);
        g_state.hardware.display->print(" ");
        g_state.hardware.display->print(static_cast<char>(176));
        g_state.hardware.display->print("C");
        g_state.hardware.display->setCursor(32, 34);
        g_state.hardware.display->print(langstring_set_temp);
        g_state.hardware.display->print(g_state.process.setpoint, 1);
        g_state.hardware.display->print(" ");
        g_state.hardware.display->print(static_cast<char>(176));
        g_state.hardware.display->print("C");

        displayThermometerOutline(4, 58);

        // draw current temp in thermometer
        if (g_state.timing.isrCounter < 500) {
            drawTemperaturebar(8, 30);
            g_state.hardware.display->setCursor(32, 4);
            g_state.hardware.display->print("PID STOPPED");
        }

        g_state.hardware.display->sendBuffer();

        return true;
    }

    if (g_state.machine.machineState == MachineStateId::SENSOR_ERROR) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        
        // Use buffer for temperature conversion to avoid String allocation
        char tempBuffer[16];
        snprintf(tempBuffer, sizeof(tempBuffer), "%.1f", g_state.process.temperature);
        displayMessage(langstring_error_tsensor[0], tempBuffer, langstring_error_tsensor[1], "", "", "");
        return true;
    }

    if (g_state.machine.machineState == MachineStateId::EEPROM_ERROR) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        displayMessage("EEPROM Error, please set Values", "", "", "", "", "");
        return true;
    }

    return false;
}

/**
 * @brief Set appropriate font for current display template
 */
inline void setDisplayFont() {
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        g_state.hardware.display->setFont(u8g2_font_profont10_tf);
    }
    else {
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
    }
}

/**
 * @brief Check if a word fits on the current line
 */
inline bool wordFitsOnLine(const char* line, const char* word, int displayWidth) {
    constexpr size_t MAX_TEST_LINE = MESSAGE_BUFFER_SIZE;
    char testLine[MAX_TEST_LINE];
    snprintf(testLine, MAX_TEST_LINE, "%s%s", line, word);
    return g_state.hardware.display->getUTF8Width(testLine) <= displayWidth;
}

/**
 * @brief Add word to current line buffer
 */
inline void addWordToLine(char* line, size_t& lineLen, const char* word, size_t maxLineLen) {
    if (lineLen > 0) {
        strncat(line, " ", maxLineLen - lineLen - 1);
        lineLen++;
    }
    strncat(line, word, maxLineLen - lineLen - 1);
    lineLen += strlen(word);
}

/**
 * @brief Draw line and move to next line
 */
inline void drawLineAndAdvance(const char* line, int x, int& y, int lineHeight) {
    g_state.hardware.display->drawUTF8(x, y, line);
    y += lineHeight;
}

/**
 * @brief Start new line with given word
 */
inline void startNewLineWithWord(char* line, size_t& lineLen, const char* word, size_t maxLineLen) {
    snprintf(line, maxLineLen, "%s ", word);
    lineLen = strlen(line);
}

inline void displayWrappedMessage(const char* message) {
    g_state.hardware.display->clearBuffer();
    setDisplayFont();

    const int lineHeight = g_state.hardware.display->getMaxCharHeight() + 2;
    const int displayWidth = g_state.hardware.display->getDisplayWidth();
    const int displayHeight = g_state.hardware.display->getDisplayHeight();

    int x = 0;
    int y = 0;
    int wordCount = 0;

    // Use fixed-size buffers to avoid String allocation
    constexpr size_t MAX_WORD_LEN = 32;
    constexpr size_t MAX_LINE_LEN = MESSAGE_BUFFER_SIZE;
    char word[MAX_WORD_LEN] = {0};
    char line[MAX_LINE_LEN] = {0};
    
    size_t wordIdx = 0;
    size_t lineLen = 0;
    const size_t msgLen = strlen(message);

    for (size_t i = 0; i <= msgLen; ++i) {
        const char c = (i < msgLen) ? message[i] : '\0';

        if (c == ' ' || c == '\n' || c == '\0') {
            word[wordIdx] = '\0';
            
            if (!wordFitsOnLine(line, word, displayWidth) && lineLen > 0) {
                // Draw current line and start new line with word
                drawLineAndAdvance(line, x, y, lineHeight);
                startNewLineWithWord(line, lineLen, word, MAX_LINE_LEN);
                wordCount = 1;
            }
            else {
                // Add word to current line
                addWordToLine(line, lineLen, word, MAX_LINE_LEN);
                wordCount++;
            }

            wordIdx = 0;

            if (c == '\n') {
                drawLineAndAdvance(line, x, y, lineHeight);
                line[0] = '\0';
                lineLen = 0;
                wordCount = 0;
            }
        }
        else if (wordIdx < MAX_WORD_LEN - 1) {
            word[wordIdx++] = c;
        }
    }

    // Draw final line if it has content and fits
    if (lineLen > 0 && y + lineHeight <= displayHeight) {
        g_state.hardware.display->drawUTF8(x, y, line);
    }

    g_state.hardware.display->sendBuffer();
}
