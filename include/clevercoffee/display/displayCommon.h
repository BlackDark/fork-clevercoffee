/**
 * @file displayCommon.h
 *
 * @brief Common functions for all display templates
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/display/bitmaps.h"
#include "clevercoffee/display/languages.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/network/CleverCoffeeWiFiManager.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/utils/SystemUtils.h"


/**
 * @brief Helper to get current machine state for display rendering
 * @return Current machine state ID
 */
inline MachineStateId getCurrentDisplayState() {
    auto ctx = CleverCoffee::getGlobalSystemContext();
    if (!ctx || !ctx->machineStateContext()) {
        return MachineStateId::INIT;
    }
    return ctx->machineStateContext()->getCurrentStateId();
}

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
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(0, 32, "Failed!");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(0, 42, "Scale");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(0, 52, "not");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(0, 62, "working...");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
    } else {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(0, 32, "failed!");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(
            0, 42, "Scale not working..."); // scale timeout will most likely trigger after OTA update, but will still
                                            // work after boot
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
    }
}

/**
 * @brief Draw the system uptime at the given coordinates
 */
inline void displayUptime(const int x, const int y, const char* format) {
    // Show uptime of machine
    unsigned long       seconds = millis() / 1000;
    const unsigned long hours   = seconds / 3600;
    const unsigned long minutes = seconds % 3600 / 60;
    seconds                     = seconds % 60;

    char uptimeString[9];
    snprintf(uptimeString, sizeof(uptimeString), format, hours, minutes, seconds);

    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(x, y, uptimeString);
}

/**
 * @brief Draw a WiFi signal strength indicator at the given coordinates
 */
inline void displayWiFiStatus(const int x, const int y) {
    if (WiFi.status() == WL_CONNECTED) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(x, y, 8, 8, Antenna_OK_Icon);

        for (int b = 0; b <= CleverCoffee::getGlobalSystemContext()->cleverCoffeeWiFiManager()->getSignalStrength(); b++) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawVLine(x + 5 + b * 2, y + 8 - b * 2, b * 2);
        }
    } else {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(x, y, 8, 8, Antenna_NOK_Icon);

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 12, y - 1);
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 36, y - 1);
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("RC: ");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(CleverCoffee::getGlobalSystemContext()->networkCoordinator().getWifiReconnects());
    }
}

/**
 * @brief Draw an MQTT status indicator at the given coordinates if MQTT is enabled
 */
inline void displayMQTTStatus(const int x, const int y) {
    if (Config::getInstance().mqttEnabled.get()) {
        if (CleverCoffee::getGlobalSystemContext()->mqttManager() && CleverCoffee::getGlobalSystemContext()->mqttManager()->isConnected()) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("MQTT");

            if (CleverCoffee::getGlobalSystemContext()->cleverCoffeeWiFiManager()->getSignalStrength() <= 1) {
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("!");
            }
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("");
        }
    }
}

/**
 * @brief Draw the outline of a thermometer for use in conjunction with the drawTemperaturebar method
 */
inline void displayThermometerOutline(const int x, const int y) {
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(x + 3, y - 9, x + 3, y - 42);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(x + 9, y - 9, x + 9, y - 42);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawPixel(x + 4, y - 43);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawPixel(x + 8, y - 43);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(x + 5, y - 44, x + 7, y - 44);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawDisc(x + 6, y - 5, 6);

    // draw setpoint line
    const int height = map(static_cast<int>(g_state.process.setpoint), 0, 100, y - 9, y - 39);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(x + 11, height, x + 16, height);
}

/**
 * @brief Draw temperature bar, e.g. inside the thermometer outline.
 *        Add 4 pixels to the x-coordinate and subtract 12 pixels from the y-coordinate of the thermometer.
 */
inline void drawTemperaturebar(const int x, const int heightRange) {
    const int width = x + 5;

    for (int i = x; i < width; i++) {
        const int height = map(static_cast<int>(g_state.process.temperature), 0, 100, 0, heightRange);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawVLine(i, 52 - height, height);
    }

    if (g_state.process.temperature > 100) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(x, heightRange - 11, x + 3, heightRange - 11);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(x, heightRange - 10, x + 4, heightRange - 10);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(x, heightRange - 9, x + 4, heightRange - 9);
    }
}

/**
 * @brief Draw the temperature in big font at given position
 */
inline void displayTemperature(const int x, const int y) {
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_fub30_tf);

    if (g_state.process.temperature < 99.499) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 20, y);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(g_state.process.temperature, 0);
    } else {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(g_state.process.temperature, 0);
    }

    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawCircle(x + 72, y + 4, 3);
}

/**
 * @brief determines if brew timer should be visible; postBrewTimerDuration defines how long the timer after the brew is
 * shown
 * @return true if timer should be visible, false otherwise
 */
inline bool shouldDisplayBrewTimer() {
    enum BrewTimerState {
        kBrewTimerIdle     = 10,
        kBrewTimerRunning  = 20,
        kBrewTimerPostBrew = 30
    };

    static BrewTimerState currBrewTimerState = kBrewTimerIdle;

    static uint32_t brewEndTime = 0;

    switch (currBrewTimerState) {
        case kBrewTimerIdle:
            if (CleverCoffee::getGlobalSystemContext()->brewHandler() && CleverCoffee::getGlobalSystemContext()->brewHandler()->isBrewActive()) {
                currBrewTimerState = kBrewTimerRunning;
            }
            break;

        case kBrewTimerRunning:
            if (!CleverCoffee::getGlobalSystemContext()->brewHandler() || !CleverCoffee::getGlobalSystemContext()->brewHandler()->isBrewActive()) {
                currBrewTimerState = kBrewTimerPostBrew;
                brewEndTime        = millis();
            }
            break;

        case kBrewTimerPostBrew:
            if (millis() - brewEndTime >
                static_cast<uint32_t>(Config::getInstance().displayPostBrewTimerDuration.get() * 1000)) {
                currBrewTimerState = kBrewTimerIdle;
            }
            break;
    }

    return currBrewTimerState != kBrewTimerIdle;
}

/**
 * @brief Draw current brew time with optional brew target time at given position
 *
 * Shows the current brew time in seconds. If a target time (totalTargetBrewTime) is provided (> 0), it is displayed
 * alongside the current time.
 *
 * @param x              Horizontal position to start drawing
 * @param y              Vertical position to start drawing
 * @param label          Text label to display before the time
 * @param currBrewTime     Current brewed time in milliseconds
 * @param totalTargetBrewTime  Target brew time in milliseconds (optional, default -1)
 */
inline void displayBrewTime(
    const int x, const int y, const char* label, const double currBrewTime, const double totalTargetBrewTime = -1) {
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setDrawColor(0);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::MINIMAL) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawBox(x, y, 100, 15);
    } else {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawBox(x, y + 1, 100, 10);
    }

    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setDrawColor(1);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(label);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(currBrewTime / 1000, 0);

        if (totalTargetBrewTime > 0) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("/");
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(totalTargetBrewTime / 1000, 0);
        }
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(" s");
    } else {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(label);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 50, y);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(currBrewTime / 1000, 0);

        if (totalTargetBrewTime > 0) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("/");
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(totalTargetBrewTime / 1000, 0);
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(" s");
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
inline void displayBrewWeight(
    const int x, const int y, const float weight, const float setpoint = -1, const bool fault = false) {
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setDrawColor(0);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawBox(x, y + 1, 100, 10);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setDrawColor(1);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        if (fault) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_weight_ur);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_scale_Failure);
            return;
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_weight_ur);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(weight, 0);

        if (setpoint > 0) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("/");
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(setpoint, 0);
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(" g");
    } else {
        if (fault) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_weight);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 50, y);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_scale_Failure);
            return;
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_weight);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 50, y);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(weight, 0);

        if (setpoint > 0) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("/");
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(setpoint, 0);
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(" g");
    }
}

/**
 * @brief Draw the brew time at given position (fullscreen brewtimer)
 */
inline void displayBrewtimeFs(const int x, const int y, const double brewtime) {
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_fub20_tf);
        if (brewtime < 9950.000) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 15, y);
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
        }
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(brewtime / 1000, 1);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 56, y + 12);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("s");
    } else {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_fub25_tf);

        if (brewtime < 9950.000) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 16, y);
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x, y);
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(brewtime / 1000, 1);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont15_tf);

        if (brewtime < 9950.000) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 67, y + 14);
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(x + 69, y + 14);
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("s");
    }

    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
}

/**
 * @brief Draw a bar visualizing the output in % at the given coordinates and with the given width
 */
inline void displayProgressbar(const int value, const int x, const int y, const int width) {
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawFrame(x, y, width, 4);

    if (const int output = map(value, 0, 100, 0, width); output - 2 > 0) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(x + 1, y + 1, x + output - 1, y + 1);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(x + 1, y + 2, x + output - 1, y + 2);
    }
}

inline void displayBluetoothStatus(const int x, const int y) {
    if (CleverCoffee::getGlobalSystemContext()->hardwareContext().scale()) {
        if (const bool connected = CleverCoffee::getGlobalSystemContext()->hardwareContext().scale()->isConnected(); connected) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(x, y, 8, 9, Bluetooth_Icon);
        }
    }
}

/**
 * @brief Draw a status bar at the top of the screen with icons for WiFi, MQTT,
 *        the system uptime and a separator line underneath
 */
inline void displayStatusbar() {
    // For status info
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(0, STATUS_BAR_Y_POS, DISPLAY_WIDTH, STATUS_BAR_Y_POS);

    if (!CleverCoffee::getGlobalSystemContext()->networkCoordinator().isOfflineMode()) {
        displayWiFiStatus(4, 1);
        displayMQTTStatus(40, 0);
    } else {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(40, 0);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_offlinemode);
    }

    if (Config::getInstance().hardwareSensorsScaleEnabled.get() &&
        Config::getInstance().hardwareSensorsScaleType.get() == Hardware::ScaleType::BLUETOOTH) {
        displayBluetoothStatus(24, 1);
    }

    const auto format = "%02luh %02lum";
    displayUptime(84, 0, format);
}

/**
 * @brief print message
 */
inline void displayMessage(
    const char* text1, const char* text2, const char* text3, const char* text4, const char* text5, const char* text6) {
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(0, 0);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(text1);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(0, 10);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(text2);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(0, 20);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(text3);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(0, 30);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(text4);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(0, 40);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(text5);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(0, 50);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(text6);
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
}

/**
 * @brief print logo and message at boot
 */
inline void displayLogo(const char* displaymessagetext, const char* displaymessagetext2) {
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        int printrow = 47;
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();

        // Use stack allocation for tokenization buffers
        constexpr size_t MAX_MSG_LEN = SHORT_MESSAGE_SIZE;
        char             text1[MAX_MSG_LEN];
        char             text2[MAX_MSG_LEN];

        strncpy(text1, displaymessagetext, MAX_MSG_LEN - 1);
        text1[MAX_MSG_LEN - 1] = '\0';
        strncpy(text2, displaymessagetext2, MAX_MSG_LEN - 1);
        text2[MAX_MSG_LEN - 1] = '\0';

        char* token = strtok(text1, " ");

        while (token != nullptr) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(0, printrow, token);
            token     = strtok(nullptr, " ");
            printrow += 10;
        }

        token = strtok(text2, " ");

        while (token != nullptr) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(0, printrow, token);
            token     = strtok(nullptr, " ");
            printrow += 10;
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(11, 4, CleverCoffee_Logo_width, CleverCoffee_Logo_height, CleverCoffee_Logo);
    } else {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(0, 45, displaymessagetext);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawStr(0, 55, displaymessagetext2);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(38, 0, CleverCoffee_Logo_width, CleverCoffee_Logo_height, CleverCoffee_Logo);
    }

    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
}

/**
 * @brief display fullscreen brew timer
 */
inline bool displayFullscreenBrewTimer() {
    if (!Config::getInstance().displayFullscreenBrewTimer.get()) {
        return false;
    }

    if (shouldDisplayBrewTimer()) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(12, 12, Brew_Cup_Logo_width, Brew_Cup_Logo_height, Brew_Cup_Logo);

            if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont22_tf);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(5, 70);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(g_state.process.currBrewTime / 1000, 1);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("s");
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(5, 100);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(g_state.sensors.currBrewWeight, 1);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("g");
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            } else {
                displayBrewtimeFs(1, 80, g_state.process.currBrewTime);
            }
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(-1, 11, Brew_Cup_Logo_width, Brew_Cup_Logo_height, Brew_Cup_Logo);

            if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont22_tf);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(DISPLAY_WIDTH / 2, 15);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(g_state.process.currBrewTime / 1000, 1);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("s");
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(DISPLAY_WIDTH / 2, 38);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(g_state.sensors.currBrewWeight, 1);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("g");
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            } else {
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

    if (isManualFlushState(CleverCoffee::getGlobalSystemContext()->machineStateContext()->getCurrentStateId()) &&
        getCurrentDisplayState() == MachineStateId::MANUAL_FLUSH_RUNNING) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(
                12, 12, Manual_Flush_Logo_width, Manual_Flush_Logo_height, Manual_Flush_Logo);
            displayBrewtimeFs(1, 80, g_state.process.currBrewTime);
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(
                0, 12, Manual_Flush_Logo_width, Manual_Flush_Logo_height, Manual_Flush_Logo);
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

    if (isHotWaterState(CleverCoffee::getGlobalSystemContext()->machineStateContext()->getCurrentStateId()) &&
        getCurrentDisplayState() == MachineStateId::HOT_WATER_RUNNING) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(12, 12, Hot_Water_Logo_width, Hot_Water_Logo_height, Hot_Water_Logo);
            displayBrewtimeFs(1, 80, g_state.sensors.currPumpOnTime);
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(0, 12, Hot_Water_Logo_width, Hot_Water_Logo_height, Hot_Water_Logo);
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
    if (CleverCoffee::getGlobalSystemContext()->uiCoordinator().getDisplayOffline() > 0 && CleverCoffee::getGlobalSystemContext()->uiCoordinator().getDisplayOffline() < 20) {
        displayMessage("", "", "", "", "Begin Fallback,", "No Wifi");
        CleverCoffee::getGlobalSystemContext()->uiCoordinator().incrementDisplayOffline();
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
    if (Config::getInstance().displayHeatingLogo.get() > 0 &&
        (getCurrentDisplayState() == MachineStateId::PID_NORMAL) &&
        g_state.process.setpoint - g_state.process.temperature > 5.) {
        // For status info
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();

        displayStatusbar();

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(0, 20, Heating_Logo_width, Heating_Logo_height, Heating_Logo);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_fub25_tf);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(50, 30);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(g_state.process.temperature, 1);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawCircle(122, 32, 3);

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
        return true;
    }

    // Offline logo
    if (Config::getInstance().displayPidOffLogo.get() == 1 &&
        getCurrentDisplayState() == MachineStateId::PID_DISABLED) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(38, 0, Off_Logo_width, Off_Logo_height, Off_Logo);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(0, 55);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont10_tf);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("PID is disabled manually");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
        return true;
    }

    if (Config::getInstance().displayPidOffLogo.get() == 1 && getCurrentDisplayState() == MachineStateId::STANDBY) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(38, 0, Off_Logo_width, Off_Logo_height, Off_Logo);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(36, 55);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont10_tf);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("Standby mode");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
        return true;
    }

    // Steam
    if (isSteamState(CleverCoffee::getGlobalSystemContext()->machineStateContext()->getCurrentStateId())) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(-1, 12, Steam_Logo_width, Steam_Logo_height, Steam_Logo);

        displayTemperature(48, 16);

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
        return true;
    }

    // Water empty
    if (getCurrentDisplayState() == MachineStateId::WATER_TANK_EMPTY) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(
            45, 0, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
        return true;
    }

    // Backflush
    if (isBackflushState(CleverCoffee::getGlobalSystemContext()->machineStateContext()->getCurrentStateId())) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_fub17_tf);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(2, 10);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("Backflush");

        switch (CleverCoffee::getGlobalSystemContext()->machineStateContext()->getCurrentStateId()) {
            case MachineStateId::BACKFLUSH_IDLE:
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont12_tf);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(4, 37);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_backflush_press);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(4, 50);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_backflush_start);
                break;

            case MachineStateId::BACKFLUSH_FINISHED:
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont12_tf);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(4, 37);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_backflush_press);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(4, 50);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_backflush_finish);
                break;

            default:
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_fub17_tf);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(42, 42);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(
                    CleverCoffee::getGlobalSystemContext()->machineStateContext()->getBackflushCycleCount(), 0);
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("/");
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(Config::getInstance().backflushCycles.get(), 0);
                break;
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
        return true;
    }

    // PID Off
    if (getCurrentDisplayState() == MachineStateId::EMERGENCY_STOP) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(32, 24);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_current_temp);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(g_state.process.temperature, 1);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(" ");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(static_cast<char>(176));
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("C");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(32, 34);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_set_temp);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(g_state.process.setpoint, 1);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(" ");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(static_cast<char>(176));
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("C");

        displayThermometerOutline(4, 58);

        // draw current temp in thermometer
        if (g_state.timing.isrCounter < 500) {
            drawTemperaturebar(8, 30);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(32, 4);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("PID STOPPED");
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();

        return true;
    }

    if (getCurrentDisplayState() == MachineStateId::SENSOR_ERROR) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);

        // Use buffer for temperature conversion to avoid String allocation
        char tempBuffer[16];
        snprintf(tempBuffer, sizeof(tempBuffer), "%.1f", g_state.process.temperature);
        displayMessage(langstring_error_tsensor[0], tempBuffer, langstring_error_tsensor[1], "", "", "");
        return true;
    }

    if (getCurrentDisplayState() == MachineStateId::EEPROM_ERROR) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
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
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont10_tf);
    } else {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
    }
}

/**
 * @brief Check if a word fits on the current line
 */
inline bool wordFitsOnLine(const char* line, const char* word, int displayWidth) {
    constexpr size_t MAX_TEST_LINE = MESSAGE_BUFFER_SIZE;
    char             testLine[MAX_TEST_LINE];
    snprintf(testLine, MAX_TEST_LINE, "%s%s", line, word);
    return CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->getUTF8Width(testLine) <= displayWidth;
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
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawUTF8(x, y, line);
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
    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();
    setDisplayFont();

    const int lineHeight    = CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->getMaxCharHeight() + 2;
    const int displayWidth  = CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->getDisplayWidth();
    const int displayHeight = CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->getDisplayHeight();

    int x         = 0;
    int y         = 0;
    int wordCount = 0;

    // Use fixed-size buffers to avoid String allocation
    constexpr size_t MAX_WORD_LEN       = 32;
    constexpr size_t MAX_LINE_LEN       = MESSAGE_BUFFER_SIZE;
    char             word[MAX_WORD_LEN] = {0};
    char             line[MAX_LINE_LEN] = {0};

    size_t       wordIdx = 0;
    size_t       lineLen = 0;
    const size_t msgLen  = strlen(message);

    for (size_t i = 0; i <= msgLen; ++i) {
        const char c = (i < msgLen) ? message[i] : '\0';

        if (c == ' ' || c == '\n' || c == '\0') {
            word[wordIdx] = '\0';

            if (!wordFitsOnLine(line, word, displayWidth) && lineLen > 0) {
                // Draw current line and start new line with word
                drawLineAndAdvance(line, x, y, lineHeight);
                startNewLineWithWord(line, lineLen, word, MAX_LINE_LEN);
                wordCount = 1;
            } else {
                // Add word to current line
                addWordToLine(line, lineLen, word, MAX_LINE_LEN);
                wordCount++;
            }

            wordIdx = 0;

            if (c == '\n') {
                drawLineAndAdvance(line, x, y, lineHeight);
                line[0]   = '\0';
                lineLen   = 0;
                wordCount = 0;
            }
        } else if (wordIdx < MAX_WORD_LEN - 1) {
            word[wordIdx++] = c;
        }
    }

    // Draw final line if it has content and fits
    if (lineLen > 0 && y + lineHeight <= displayHeight) {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawUTF8(x, y, line);
    }

    CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->sendBuffer();
}
