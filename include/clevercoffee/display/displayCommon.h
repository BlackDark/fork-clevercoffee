/**
 * @file displayCommon.h
 *
 * @brief Common functions for all display templates
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/display/bitmaps.h"
#include "clevercoffee/display/languages.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/network/CleverCoffeeWiFiManager.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/utils/SystemUtils.h"

/**
 * @brief Helper to get current machine state for display rendering
 * @param systemContext System context (required)
 * @return Current machine state ID
 */
inline MachineStateId getCurrentDisplayState(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.machineStateContext()) {
        return MachineStateId::INIT;
    }
    return systemContext.machineStateContext()->getCurrentStateId();
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
 * @param systemContext System context (required)
 */
inline void displayScaleFailed(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.hardwareContext().display()) return;

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->drawStr(0, 32, "Failed!");
        systemContext.hardwareContext().display()->drawStr(0, 42, "Scale");
        systemContext.hardwareContext().display()->drawStr(0, 52, "not");
        systemContext.hardwareContext().display()->drawStr(0, 62, "working...");
        systemContext.hardwareContext().display()->sendBuffer();
    } else {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->drawStr(0, 32, "failed!");
        systemContext.hardwareContext().display()->drawStr(
            0, 42, "Scale not working..."); // scale timeout will most likely trigger after OTA update, but will still
                                            // work after boot
        systemContext.hardwareContext().display()->sendBuffer();
    }
}

/**
 * @brief Draw the system uptime at the given coordinates
 * @param systemContext System context (required)
 */
inline void displayUptime(CleverCoffee::SystemContext& systemContext, const int x, const int y, const char* format) {
    if (!systemContext.hardwareContext().display()) return;

    // Show uptime of machine
    unsigned long       seconds = millis() / 1000;
    const unsigned long hours   = seconds / 3600;
    const unsigned long minutes = seconds % 3600 / 60;
    seconds                     = seconds % 60;

    char uptimeString[9];
    snprintf(uptimeString, sizeof(uptimeString), format, hours, minutes, seconds);

    systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
    systemContext.hardwareContext().display()->drawStr(x, y, uptimeString);
}

/**
 * @brief Draw a WiFi signal strength indicator at the given coordinates
 * @param systemContext System context (required)
 */
inline void displayWiFiStatus(CleverCoffee::SystemContext& systemContext, const int x, const int y) {
    if (!systemContext.hardwareContext().display()) return;

    if (WiFi.status() == WL_CONNECTED) {
        systemContext.hardwareContext().display()->drawXBMP(x, y, 8, 8, Antenna_OK_Icon);

        if (systemContext.cleverCoffeeWiFiManager()) {
            for (int b = 0; b <= systemContext.cleverCoffeeWiFiManager()->getSignalStrength(); b++) {
                systemContext.hardwareContext().display()->drawVLine(x + 5 + b * 2, y + 8 - b * 2, b * 2);
            }
        }
    } else {
        systemContext.hardwareContext().display()->drawXBMP(x, y, 8, 8, Antenna_NOK_Icon);

        if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
            systemContext.hardwareContext().display()->setCursor(x + 12, y - 1);
        } else {
            systemContext.hardwareContext().display()->setCursor(x + 36, y - 1);
        }

        systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        systemContext.hardwareContext().display()->print("RC: ");
        systemContext.hardwareContext().display()->print(systemContext.networkCoordinator().getWifiReconnects());
    }
}

/**
 * @brief Draw an MQTT status indicator at the given coordinates if MQTT is enabled
 * @param systemContext System context (required)
 */
inline void displayMQTTStatus(CleverCoffee::SystemContext& systemContext, const int x, const int y) {
    if (!systemContext.hardwareContext().display()) return;

    if (Config::getInstance().mqttEnabled.get()) {
        if (systemContext.mqttManager() && systemContext.mqttManager()->isConnected()) {
            systemContext.hardwareContext().display()->setCursor(x, y);
            systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            systemContext.hardwareContext().display()->print("MQTT");

            if (systemContext.cleverCoffeeWiFiManager() &&
                systemContext.cleverCoffeeWiFiManager()->getSignalStrength() <= 1) {
                systemContext.hardwareContext().display()->print("!");
            }
        } else {
            systemContext.hardwareContext().display()->setCursor(x, y);
            systemContext.hardwareContext().display()->print("");
        }
    }
}

/**
 * @brief Draw the outline of a thermometer for use in conjunction with the drawTemperaturebar method
 */
inline void displayThermometerOutline(CleverCoffee::SystemContext& systemContext, const int x, const int y) {
    if (!systemContext.hardwareContext().display()) return;

    systemContext.hardwareContext().display()->drawLine(x + 3, y - 9, x + 3, y - 42);
    systemContext.hardwareContext().display()->drawLine(x + 9, y - 9, x + 9, y - 42);
    systemContext.hardwareContext().display()->drawPixel(x + 4, y - 43);
    systemContext.hardwareContext().display()->drawPixel(x + 8, y - 43);
    systemContext.hardwareContext().display()->drawLine(x + 5, y - 44, x + 7, y - 44);
    systemContext.hardwareContext().display()->drawDisc(x + 6, y - 5, 6);

    // draw setpoint line
    const int height = map(static_cast<int>(systemContext.processSetpoint()), 0, 100, y - 9, y - 39);
    systemContext.hardwareContext().display()->drawLine(x + 11, height, x + 16, height);
}

/**
 * @brief Draw temperature bar, e.g. inside the thermometer outline.
 *        Add 4 pixels to the x-coordinate and subtract 12 pixels from the y-coordinate of the thermometer.
 */
inline void drawTemperaturebar(CleverCoffee::SystemContext& systemContext, const int x, const int heightRange) {
    if (!systemContext.hardwareContext().display()) return;

    const int width = x + 5;

    for (int i = x; i < width; i++) {
        const int height = map(static_cast<int>(systemContext.processTemperature()), 0, 100, 0, heightRange);
        systemContext.hardwareContext().display()->drawVLine(i, 52 - height, height);
    }

    if (systemContext.processTemperature() > 100) {
        systemContext.hardwareContext().display()->drawLine(x, heightRange - 11, x + 3, heightRange - 11);
        systemContext.hardwareContext().display()->drawLine(x, heightRange - 10, x + 4, heightRange - 10);
        systemContext.hardwareContext().display()->drawLine(x, heightRange - 9, x + 4, heightRange - 9);
    }
}

/**
 * @brief Draw the temperature in big font at given position
 */
inline void displayTemperature(CleverCoffee::SystemContext& systemContext, const int x, const int y) {
    if (!systemContext.hardwareContext().display()) return;

    systemContext.hardwareContext().display()->setFont(u8g2_font_fub30_tf);

    if (systemContext.processTemperature() < 99.499) {
        systemContext.hardwareContext().display()->setCursor(x + 20, y);
        systemContext.hardwareContext().display()->print(systemContext.processTemperature(), 0);
    } else {
        systemContext.hardwareContext().display()->setCursor(x, y);
        systemContext.hardwareContext().display()->print(systemContext.processTemperature(), 0);
    }

    systemContext.hardwareContext().display()->drawCircle(x + 72, y + 4, 3);
}

/**
 * @brief determines if brew timer should be visible; postBrewTimerDuration defines how long the timer after the brew is
 * shown
 * @return true if timer should be visible, false otherwise
 */
inline bool shouldDisplayHotWaterTimer(CleverCoffee::SystemContext& systemContext) {
    // Hot water states removed - check if pump is active in PID_NORMAL or STEAM_RUNNING
    if (!systemContext.machineStateContext()) return false;

    auto currentState = systemContext.machineStateContext()->getCurrentStateId();
    bool pumpActive   = (systemContext.currPumpOnTime() > 0);
    return pumpActive && (currentState == MachineStateId::PID_NORMAL || currentState == MachineStateId::STEAM_RUNNING);
}

inline bool shouldDisplayBrewTimer(CleverCoffee::SystemContext& systemContext) {
    enum BrewTimerState {
        kBrewTimerIdle     = 10,
        kBrewTimerRunning  = 20,
        kBrewTimerPostBrew = 30
    };

    static BrewTimerState currBrewTimerState = kBrewTimerIdle;

    static uint32_t brewEndTime = 0;

    switch (currBrewTimerState) {
        case kBrewTimerIdle:
            if (systemContext.brewHandler().isBrewActive()) {
                currBrewTimerState = kBrewTimerRunning;
            }
            break;

        case kBrewTimerRunning:
            if (!systemContext.brewHandler().isBrewActive()) {
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
inline void displayBrewTime(CleverCoffee::SystemContext& systemContext,
                            const int                    x,
                            const int                    y,
                            const char*                  label,
                            const double                 currBrewTime,
                            const double                 totalTargetBrewTime = -1) {
    if (!systemContext.hardwareContext().display()) return;

    systemContext.hardwareContext().display()->setDrawColor(0);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::MINIMAL) {
        systemContext.hardwareContext().display()->drawBox(x, y, 100, 15);
    } else {
        systemContext.hardwareContext().display()->drawBox(x, y + 1, 100, 10);
    }

    systemContext.hardwareContext().display()->setDrawColor(1);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        systemContext.hardwareContext().display()->setCursor(x, y);
        systemContext.hardwareContext().display()->print(label);
        systemContext.hardwareContext().display()->print(currBrewTime / 1000, 0);

        if (totalTargetBrewTime > 0) {
            systemContext.hardwareContext().display()->print("/");
            systemContext.hardwareContext().display()->print(totalTargetBrewTime / 1000, 0);
        }
        systemContext.hardwareContext().display()->print(" s");
    } else {
        systemContext.hardwareContext().display()->setCursor(x, y);
        systemContext.hardwareContext().display()->print(label);
        systemContext.hardwareContext().display()->setCursor(x + 50, y);
        systemContext.hardwareContext().display()->print(currBrewTime / 1000, 0);

        if (totalTargetBrewTime > 0) {
            systemContext.hardwareContext().display()->print("/");
            systemContext.hardwareContext().display()->print(totalTargetBrewTime / 1000, 0);
        }

        systemContext.hardwareContext().display()->print(" s");
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
inline void displayBrewWeight(CleverCoffee::SystemContext& systemContext,
                              const int                    x,
                              const int                    y,
                              const float                  weight,
                              const float                  setpoint = -1,
                              const bool                   fault    = false) {
    systemContext.hardwareContext().display()->setDrawColor(0);
    systemContext.hardwareContext().display()->drawBox(x, y + 1, 100, 10);
    systemContext.hardwareContext().display()->setDrawColor(1);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        if (fault) {
            systemContext.hardwareContext().display()->setCursor(x, y);
            systemContext.hardwareContext().display()->print(langstring_weight_ur);
            systemContext.hardwareContext().display()->print(langstring_scale_Failure);
            return;
        }

        systemContext.hardwareContext().display()->setCursor(x, y);
        systemContext.hardwareContext().display()->print(langstring_weight_ur);
        systemContext.hardwareContext().display()->print(weight, 0);

        if (setpoint > 0) {
            systemContext.hardwareContext().display()->print("/");
            systemContext.hardwareContext().display()->print(setpoint, 0);
        }

        systemContext.hardwareContext().display()->print(" g");
    } else {
        if (fault) {
            systemContext.hardwareContext().display()->setCursor(x, y);
            systemContext.hardwareContext().display()->print(langstring_weight);
            systemContext.hardwareContext().display()->setCursor(x + 50, y);
            systemContext.hardwareContext().display()->print(langstring_scale_Failure);
            return;
        }

        systemContext.hardwareContext().display()->setCursor(x, y);
        systemContext.hardwareContext().display()->print(langstring_weight);
        systemContext.hardwareContext().display()->setCursor(x + 50, y);
        systemContext.hardwareContext().display()->print(weight, 0);

        if (setpoint > 0) {
            systemContext.hardwareContext().display()->print("/");
            systemContext.hardwareContext().display()->print(setpoint, 0);
        }

        systemContext.hardwareContext().display()->print(" g");
    }
}

/**
 * @brief Draw the brew time at given position (fullscreen brewtimer)
 */
inline void displayBrewtimeFs(CleverCoffee::SystemContext& systemContext,
                              const int                    x,
                              const int                    y,
                              const double                 brewtime) {
    if (!systemContext.hardwareContext().display()) return;
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        systemContext.hardwareContext().display()->setFont(u8g2_font_fub20_tf);
        if (brewtime < 9950.000) {
            systemContext.hardwareContext().display()->setCursor(x + 15, y);
        } else {
            systemContext.hardwareContext().display()->setCursor(x, y);
        }
        systemContext.hardwareContext().display()->print(brewtime / 1000, 1);
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        systemContext.hardwareContext().display()->setCursor(x + 56, y + 12);
        systemContext.hardwareContext().display()->print("s");
    } else {
        systemContext.hardwareContext().display()->setFont(u8g2_font_fub25_tf);

        if (brewtime < 9950.000) {
            systemContext.hardwareContext().display()->setCursor(x + 16, y);
        } else {
            systemContext.hardwareContext().display()->setCursor(x, y);
        }

        systemContext.hardwareContext().display()->print(brewtime / 1000, 1);
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont15_tf);

        if (brewtime < 9950.000) {
            systemContext.hardwareContext().display()->setCursor(x + 67, y + 14);
        } else {
            systemContext.hardwareContext().display()->setCursor(x + 69, y + 14);
        }

        systemContext.hardwareContext().display()->print("s");
    }

    systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
}

/**
 * @brief Draw a bar visualizing the output in % at the given coordinates and with the given width
 */
inline void displayProgressbar(
    CleverCoffee::SystemContext& systemContext, const int value, const int x, const int y, const int width) {
    if (!systemContext.hardwareContext().display()) return;
    systemContext.hardwareContext().display()->drawFrame(x, y, width, 4);

    if (const int output = map(value, 0, 100, 0, width); output - 2 > 0) {
        systemContext.hardwareContext().display()->drawLine(x + 1, y + 1, x + output - 1, y + 1);
        systemContext.hardwareContext().display()->drawLine(x + 1, y + 2, x + output - 1, y + 2);
    }
}

inline void displayBluetoothStatus(CleverCoffee::SystemContext& systemContext, const int x, const int y) {
    if (!systemContext.hardwareContext().display()) return;

    // Check if scale sensor exists and is connected via SensorCoordinator
    // Note: For now, we check if scale sensor pointer exists. A proper connection check
    // would require adding isConnected() to SensorCoordinator or ISensor interface.
    // For bluetooth scales, connection status should be checked via the scale hardware.
    if (systemContext.hardwareContext().scalePtr()) {
        // Legacy scale interface - check if connected
        if (systemContext.hardwareContext().scalePtr()->isConnected()) {
            systemContext.hardwareContext().display()->drawXBMP(x, y, 8, 9, Bluetooth_Icon);
        }
    }
}

/**
 * @brief Draw a status bar at the top of the screen with icons for WiFi, MQTT,
 *        the system uptime and a separator line underneath
 */
inline void displayStatusbar(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.hardwareContext().display()) return;
    // For status info
    systemContext.hardwareContext().display()->drawLine(0, STATUS_BAR_Y_POS, DISPLAY_WIDTH, STATUS_BAR_Y_POS);

    if (!systemContext.networkCoordinator().isOfflineMode()) {
        displayWiFiStatus(systemContext, 4, 1);
        displayMQTTStatus(systemContext, 40, 0);
    } else {
        systemContext.hardwareContext().display()->setCursor(40, 0);
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        systemContext.hardwareContext().display()->print(langstring_offlinemode);
    }

    if (Config::getInstance().hardwareSensorsScaleEnabled.get() &&
        Config::getInstance().hardwareSensorsScaleType.get() == Hardware::ScaleType::BLUETOOTH) {
        displayBluetoothStatus(systemContext, 24, 1);
    }

    const auto format = "%02luh %02lum";
    displayUptime(systemContext, 84, 0, format);
}

/**
 * @brief print message
 */
inline void displayMessage(CleverCoffee::SystemContext& systemContext,
                           const char*                  text1,
                           const char*                  text2,
                           const char*                  text3,
                           const char*                  text4,
                           const char*                  text5,
                           const char*                  text6) {
    systemContext.hardwareContext().display()->clearBuffer();
    systemContext.hardwareContext().display()->setCursor(0, 0);
    systemContext.hardwareContext().display()->print(text1);
    systemContext.hardwareContext().display()->setCursor(0, 10);
    systemContext.hardwareContext().display()->print(text2);
    systemContext.hardwareContext().display()->setCursor(0, 20);
    systemContext.hardwareContext().display()->print(text3);
    systemContext.hardwareContext().display()->setCursor(0, 30);
    systemContext.hardwareContext().display()->print(text4);
    systemContext.hardwareContext().display()->setCursor(0, 40);
    systemContext.hardwareContext().display()->print(text5);
    systemContext.hardwareContext().display()->setCursor(0, 50);
    systemContext.hardwareContext().display()->print(text6);
    systemContext.hardwareContext().display()->sendBuffer();
}

/**
 * @brief print logo and message at boot
 */
inline void displayLogo(CleverCoffee::SystemContext& systemContext,
                        const char*                  displaymessagetext,
                        const char*                  displaymessagetext2) {
    if (!systemContext.hardwareContext().display()) return;
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        int printrow = 47;
        systemContext.hardwareContext().display()->clearBuffer();

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
            systemContext.hardwareContext().display()->drawStr(0, printrow, token);
            token     = strtok(nullptr, " ");
            printrow += 10;
        }

        token = strtok(text2, " ");

        while (token != nullptr) {
            systemContext.hardwareContext().display()->drawStr(0, printrow, token);
            token     = strtok(nullptr, " ");
            printrow += 10;
        }

        systemContext.hardwareContext().display()->drawXBMP(
            11, 4, CleverCoffee_Logo_width, CleverCoffee_Logo_height, CleverCoffee_Logo);
    } else {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->drawStr(0, 45, displaymessagetext);
        systemContext.hardwareContext().display()->drawStr(0, 55, displaymessagetext2);
        systemContext.hardwareContext().display()->drawXBMP(
            38, 0, CleverCoffee_Logo_width, CleverCoffee_Logo_height, CleverCoffee_Logo);
    }

    systemContext.hardwareContext().display()->sendBuffer();
}

/**
 * @brief display fullscreen brew timer
 */
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
                systemContext.hardwareContext().display()->setCursor(DISPLAY_WIDTH / 2, 15);
                systemContext.hardwareContext().display()->print(systemContext.processCurrentBrewTime() / 1000, 1);
                systemContext.hardwareContext().display()->print("s");
                systemContext.hardwareContext().display()->setCursor(DISPLAY_WIDTH / 2, 38);
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

/**
 * @brief display fullscreen manual flush timer
 */
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

/**
 * @brief display fullscreen hot water on timer
 */
inline bool displayFullscreenHotWaterTimer(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.hardwareContext().display()) return false;
    // Hot water states removed - hot water is handled via pump control in PID_NORMAL and STEAM_RUNNING
    // Display hot water timer when pump is active and we're in PID_NORMAL or STEAM_RUNNING
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

/**
 * @brief display offline message
 */
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

/**
 * @brief display heating logo
 */
inline bool displayMachineState(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.hardwareContext().display() || !systemContext.machineStateContext()) return false;
    if (displayOfflineMode(systemContext)) {
        return true;
    }

    // Show the heating logo when we are in regular PID mode and more than 5degC below the set point
    if (Config::getInstance().displayHeatingLogo.get() > 0 &&
        (getCurrentDisplayState(systemContext) == MachineStateId::PID_NORMAL) &&
        systemContext.processSetpoint() - systemContext.processTemperature() > 5.) {
        // For status info
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

    // Offline logo
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
        systemContext.hardwareContext().display()->drawXBMP(38, 0, Off_Logo_width, Off_Logo_height, Off_Logo);
        systemContext.hardwareContext().display()->setCursor(36, 55);
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont10_tf);
        systemContext.hardwareContext().display()->print("Standby mode");
        systemContext.hardwareContext().display()->sendBuffer();
        return true;
    }

    // Steam
    if (isSteamState(systemContext.machineStateContext()->getCurrentStateId())) {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->drawXBMP(-1, 12, Steam_Logo_width, Steam_Logo_height, Steam_Logo);

        displayTemperature(systemContext, 48, 16);

        systemContext.hardwareContext().display()->sendBuffer();
        return true;
    }

    // Water empty
    if (getCurrentDisplayState(systemContext) == MachineStateId::WATER_TANK_EMPTY) {
        systemContext.hardwareContext().display()->clearBuffer();
        systemContext.hardwareContext().display()->drawXBMP(
            45, 0, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
        systemContext.hardwareContext().display()->sendBuffer();
        return true;
    }

    // Backflush
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

    // PID Off
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

        // draw current temp in thermometer
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

        // Use buffer for temperature conversion to avoid String allocation
        char tempBuffer[16];
        snprintf(tempBuffer, sizeof(tempBuffer), "%.1f", systemContext.processTemperature());
        displayMessage(systemContext, langstring_error_tsensor[0], tempBuffer, langstring_error_tsensor[1], "", "", "");
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

/**
 * @brief Set appropriate font for current display template
 */
inline void setDisplayFont(CleverCoffee::SystemContext& systemContext) {
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont10_tf);
    } else {
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
    }
}

/**
 * @brief Check if a word fits on the current line
 */
inline bool wordFitsOnLine(CleverCoffee::SystemContext& systemContext,
                           const char*                  line,
                           const char*                  word,
                           int                          displayWidth) {
    constexpr size_t MAX_TEST_LINE = MESSAGE_BUFFER_SIZE;
    char             testLine[MAX_TEST_LINE];
    snprintf(testLine, MAX_TEST_LINE, "%s%s", line, word);
    return systemContext.hardwareContext().display()->getUTF8Width(testLine) <= displayWidth;
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
inline void drawLineAndAdvance(
    CleverCoffee::SystemContext& systemContext, const char* line, int x, int& y, int lineHeight) {
    systemContext.hardwareContext().display()->drawUTF8(x, y, line);
    y += lineHeight;
}

/**
 * @brief Start new line with given word
 */
inline void startNewLineWithWord(char* line, size_t& lineLen, const char* word, size_t maxLineLen) {
    snprintf(line, maxLineLen, "%s ", word);
    lineLen = strlen(line);
}

inline void displayWrappedMessage(CleverCoffee::SystemContext& systemContext, const char* message) {
    systemContext.hardwareContext().display()->clearBuffer();
    setDisplayFont(systemContext);

    const int lineHeight    = systemContext.hardwareContext().display()->getMaxCharHeight() + 2;
    const int displayWidth  = systemContext.hardwareContext().display()->getDisplayWidth();
    const int displayHeight = systemContext.hardwareContext().display()->getDisplayHeight();

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

            if (!wordFitsOnLine(systemContext, line, word, displayWidth) && lineLen > 0) {
                // Draw current line and start new line with word
                drawLineAndAdvance(systemContext, line, x, y, lineHeight);
                startNewLineWithWord(line, lineLen, word, MAX_LINE_LEN);
                wordCount = 1;
            } else {
                // Add word to current line
                addWordToLine(line, lineLen, word, MAX_LINE_LEN);
                wordCount++;
            }

            wordIdx = 0;

            if (c == '\n') {
                drawLineAndAdvance(systemContext, line, x, y, lineHeight);
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
        systemContext.hardwareContext().display()->drawUTF8(x, y, line);
    }

    systemContext.hardwareContext().display()->sendBuffer();
}
