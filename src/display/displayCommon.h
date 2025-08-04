/**
 * @file displayCommon.h
 *
 * @brief Common functions for all display templates
 */

#pragma once

#include "../Config.h"
#include "../state/GlobalState.h"
#include "bitmaps.h"
#include "languages.h"
#include "../utils/legacyUtils.h"

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
 * @brief initialize display
 */
inline void u8g2_prepare() {
    int rotation = 0;
    g_state.hardware.display->clearBuffer();
    g_state.hardware.display->setFont(u8g2_font_profont11_tf);
    g_state.hardware.display->setFontRefHeightExtendedText();
    g_state.hardware.display->setDrawColor(1);
    g_state.hardware.display->setFontPosTop();
    g_state.hardware.display->setFontDirection(0);

    if (Config::getInstance().get<bool>("display.inverted")) {
        rotation += 2;
    }

    if (Config::getInstance().get<int>("display.template") == 4) {
        rotation++;
    }

    g_state.hardware.display->setDisplayRotation(getU8G2Rotation(rotation));
}

/**
 * @brief print error message for scales
 */
inline void displayScaleFailed() {
    if (Config::getInstance().get<int>("display.template") == 4) {
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

        for (int b = 0; b <= getSignalStrength(); b++) {
            g_state.hardware.display->drawVLine(x + 5 + b * 2, y + 8 - b * 2, b * 2);
        }
    }
    else {
        g_state.hardware.display->drawXBMP(x, y, 8, 8, Antenna_NOK_Icon);

        if (Config::getInstance().get<int>("display.template") == 4) {
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
    if (Config::getInstance().get<bool>("mqtt.enabled")) {
        if (g_state.network.mqttManager && g_state.network.mqttManager->isConnected()) {
            g_state.hardware.display->setCursor(x, y);
            g_state.hardware.display->setFont(u8g2_font_profont11_tf);
            g_state.hardware.display->print("MQTT");

            if (getSignalStrength() <= 1) {
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
            if (checkBrewActive()) {
                currBrewTimerState = kBrewTimerRunning;
            }
            break;

        case kBrewTimerRunning:
            if (!checkBrewActive()) {
                currBrewTimerState = kBrewTimerPostBrew;
                brewEndTime = millis();
            }
            break;

        case kBrewTimerPostBrew:
            if (millis() - brewEndTime > static_cast<uint32_t>(Config::getInstance().get<double>("display.post_brew_timer_duration") * 1000)) {
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

    if (Config::getInstance().get<int>("display.template") == 1) {
        g_state.hardware.display->drawBox(x, y, 100, 15);
    }
    else {
        g_state.hardware.display->drawBox(x, y + 1, 100, 10);
    }

    g_state.hardware.display->setDrawColor(1);

    if (Config::getInstance().get<int>("display.template") == 4) {
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

    if (Config::getInstance().get<int>("display.template") == 4) {
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
    if (Config::getInstance().get<int>("display.template") == 4) {
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
    g_state.hardware.display->drawLine(0, 12, 128, 12);

    if (!g_state.network.offlineMode) {
        displayWiFiStatus(4, 1);
        displayMQTTStatus(40, 0);
    }
    else {
        g_state.hardware.display->setCursor(40, 0);
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        g_state.hardware.display->print(langstring_offlinemode);
    }

    if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled") && Config::getInstance().get<int>("hardware.sensors.scale.type") == 2) {
        displayBluetoothStatus(24, 1);
    }

    const auto format = "%02luh %02lum";
    displayUptime(84, 0, format);
}

/**
 * @brief print message
 */
inline void displayMessage(const String& text1, const String& text2, const String& text3, const String& text4, const String& text5, const String& text6) {
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
inline void displayLogo(const String& displaymessagetext, const String& displaymessagetext2) {
    if (Config::getInstance().get<int>("display.template") == 4) {
        int printrow = 47;
        g_state.hardware.display->clearBuffer();

        // Create modifiable copies
        char text1[displaymessagetext.length() + 1];
        char text2[displaymessagetext2.length() + 1];

        strcpy(text1, displaymessagetext.c_str());
        strcpy(text2, displaymessagetext2.c_str());

        char* token = strtok(text1, " ");

        while (token != nullptr) {
            g_state.hardware.display->drawStr(0, printrow, token);
            token = strtok(nullptr, " "); // Get the next token
            printrow += 10;
        }

        token = strtok(text2, " ");

        while (token != nullptr) {
            g_state.hardware.display->drawStr(0, printrow, token);
            token = strtok(nullptr, " "); // Get the next token
            printrow += 10;
        }

        g_state.hardware.display->drawXBMP(11, 4, CleverCoffee_Logo_width, CleverCoffee_Logo_height, CleverCoffee_Logo);
    }
    else {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawStr(0, 45, displaymessagetext.c_str());
        g_state.hardware.display->drawStr(0, 55, displaymessagetext2.c_str());
        g_state.hardware.display->drawXBMP(38, 0, CleverCoffee_Logo_width, CleverCoffee_Logo_height, CleverCoffee_Logo);
    }

    g_state.hardware.display->sendBuffer();
}

/**
 * @brief display fullscreen brew timer
 */
inline bool displayFullscreenBrewTimer() {
    if (!Config::getInstance().get<bool>("display.fullscreen_brew_timer")) {
        return false;
    }

    if (shouldDisplayBrewTimer()) {
        g_state.hardware.display->clearBuffer();

        if (Config::getInstance().get<int>("display.template") == 4) {
            g_state.hardware.display->drawXBMP(12, 12, Brew_Cup_Logo_width, Brew_Cup_Logo_height, Brew_Cup_Logo);

            if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
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

            if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
                g_state.hardware.display->setFont(u8g2_font_profont22_tf);
                g_state.hardware.display->setCursor(64, 15);
                g_state.hardware.display->print(g_state.process.currBrewTime / 1000, 1);
                g_state.hardware.display->print("s");
                g_state.hardware.display->setCursor(64, 38);
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
    if (!Config::getInstance().get<bool>("display.fullscreen_manual_flush_timer")) {
        return false;
    }

    if (g_state.machine.machineState == kManualFlush) {
        g_state.hardware.display->clearBuffer();

        if (Config::getInstance().get<int>("display.template") == 4) {
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
    if (!Config::getInstance().get<bool>("display.fullscreen_hot_water_timer")) {
        return false;
    }

    if (g_state.machine.machineState == kHotWater) {
        g_state.hardware.display->clearBuffer();

        if (Config::getInstance().get<int>("display.template") == 4) {
            g_state.hardware.display->drawXBMP(12, 12, Hot_Water_Logo_width, Hot_Water_Logo_height, Hot_Water_Logo);
            displayBrewtimeFs(1, 80, currPumpOnTime);
        }
        else {
            g_state.hardware.display->drawXBMP(0, 12, Hot_Water_Logo_width, Hot_Water_Logo_height, Hot_Water_Logo);
            displayBrewtimeFs(48, 25, currPumpOnTime);
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
    if (Config::getInstance().get<bool>("display.heating_logo") > 0 && (g_state.machine.machineState == kPidNormal || g_state.machine.machineState == kSteam) && g_state.process.setpoint - g_state.process.temperature > 5.) {
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
    if (Config::getInstance().get<bool>("display.pid_off_logo") == 1 && g_state.machine.machineState == kPidDisabled) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawXBMP(38, 0, Off_Logo_width, Off_Logo_height, Off_Logo);
        g_state.hardware.display->setCursor(0, 55);
        g_state.hardware.display->setFont(u8g2_font_profont10_tf);
        g_state.hardware.display->print("PID is disabled manually");
        g_state.hardware.display->sendBuffer();
        return true;
    }

    if (Config::getInstance().get<bool>("display.pid_off_logo") == 1 && g_state.machine.machineState == kStandby) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawXBMP(38, 0, Off_Logo_width, Off_Logo_height, Off_Logo);
        g_state.hardware.display->setCursor(36, 55);
        g_state.hardware.display->setFont(u8g2_font_profont10_tf);
        g_state.hardware.display->print("Standby mode");
        g_state.hardware.display->sendBuffer();
        return true;
    }

    // Steam
    if (g_state.machine.machineState == kSteam) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawXBMP(-1, 12, Steam_Logo_width, Steam_Logo_height, Steam_Logo);

        displayTemperature(48, 16);

        g_state.hardware.display->sendBuffer();
        return true;
    }

    // Water empty
    if (g_state.machine.machineState == kWaterTankEmpty) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawXBMP(45, 0, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        g_state.hardware.display->sendBuffer();
        return true;
    }

    // Backflush
    if (g_state.machine.machineState == kBackflush) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->setFont(u8g2_font_fub17_tf);
        g_state.hardware.display->setCursor(2, 10);
        g_state.hardware.display->print("Backflush");

        switch (g_state.sensors.currBackflushState) {
            case kBackflushIdle:
                g_state.hardware.display->setFont(u8g2_font_profont12_tf);
                g_state.hardware.display->setCursor(4, 37);
                g_state.hardware.display->print(langstring_backflush_press);
                g_state.hardware.display->setCursor(4, 50);
                g_state.hardware.display->print(langstring_backflush_start);
                break;

            case kBackflushFinished:
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
                g_state.hardware.display->print(Config::getInstance().get<double>("backflush.cycles"), 0);
                break;
        }

        g_state.hardware.display->sendBuffer();
        return true;
    }

    // PID Off
    if (g_state.machine.machineState == kEmergencyStop) {
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

    if (g_state.machine.machineState == kSensorError) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        displayMessage(langstring_error_tsensor[0], String(g_state.process.temperature), langstring_error_tsensor[1], "", "", "");
        return true;
    }

    if (g_state.machine.machineState == kEepromError) {
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        displayMessage("EEPROM Error, please set Values", "", "", "", "", "");
        return true;
    }

    return false;
}

inline void displayWrappedMessage(const String& message) {
    g_state.hardware.display->clearBuffer();

    if (Config::getInstance().get<int>("display.template") == 4) {
        g_state.hardware.display->setFont(u8g2_font_profont10_tf);
    }
    else {
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
    }

    int lineHeight = g_state.hardware.display->getMaxCharHeight() + 2;
    int displayWidth = g_state.hardware.display->getDisplayWidth();
    int displayHeight = g_state.hardware.display->getDisplayHeight();
    int maxLines = displayHeight / lineHeight;

    int x = 0;
    int y = 0;
    int wordCount = 0;

    String word;
    String line;

    for (size_t i = 0; i <= message.length(); ++i) {
        char c = message[i];

        if (c == ' ' || c == '\n' || c == '\0') {
            if (g_state.hardware.display->getUTF8Width((line + word).c_str()) > displayWidth) {
                g_state.hardware.display->setCursor(x, y);

                if (wordCount == 0) {
                    g_state.hardware.display->drawUTF8(x, y, word.c_str());
                    y += lineHeight;
                    line = "";
                }
                else {
                    g_state.hardware.display->drawUTF8(x, y, line.c_str());
                    y += lineHeight;
                    line = word + " ";
                    wordCount = 1;
                }
            }
            else {
                line += word + " ";
                wordCount += 1;
            }

            word = "";

            if (c == '\n') {
                g_state.hardware.display->setCursor(x, y);
                g_state.hardware.display->drawUTF8(x, y, line.c_str());
                y += lineHeight;
                line = "";
                wordCount = 0;
            }
        }
        else {
            word += c;
        }
    }

    if (line.length() > 0 && y + lineHeight <= displayHeight) {
        g_state.hardware.display->setCursor(x, y);
        g_state.hardware.display->drawUTF8(x, y, line.c_str());
    }

    g_state.hardware.display->sendBuffer();
}
