/**
 * @file DisplayWidgets.h
 * @brief Reusable display widgets (status bar, thermometer, brew info, messages)
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/display/bitmaps.h"
#include "clevercoffee/display/languages.h"
#include "clevercoffee/network/CleverCoffeeWiFiManager.h"
#include "clevercoffee/network/MQTTManager.h"
#include "clevercoffee/types/GlobalTypes.h"

#include <U8g2lib.h>

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

inline void displayUptime(CleverCoffee::SystemContext& systemContext, const int x, const int y, const char* format) {
    if (!systemContext.hardwareContext().display()) return;

    unsigned long       seconds = millis() / 1000;
    const unsigned long hours   = seconds / 3600;
    const unsigned long minutes = seconds % 3600 / 60;
    seconds                     = seconds % 60;

    char uptimeString[9];
    snprintf(uptimeString, sizeof(uptimeString), format, hours, minutes, seconds);

    systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
    systemContext.hardwareContext().display()->drawStr(x, y, uptimeString);
}

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

inline void displayThermometerOutline(CleverCoffee::SystemContext& systemContext, const int x, const int y) {
    if (!systemContext.hardwareContext().display()) return;

    systemContext.hardwareContext().display()->drawLine(x + 3, y - 9, x + 3, y - 42);
    systemContext.hardwareContext().display()->drawLine(x + 9, y - 9, x + 9, y - 42);
    systemContext.hardwareContext().display()->drawPixel(x + 4, y - 43);
    systemContext.hardwareContext().display()->drawPixel(x + 8, y - 43);
    systemContext.hardwareContext().display()->drawLine(x + 5, y - 44, x + 7, y - 44);
    systemContext.hardwareContext().display()->drawDisc(x + 6, y - 5, 6);

    const int height = map(static_cast<int>(systemContext.processSetpoint()), 0, 100, y - 9, y - 39);
    systemContext.hardwareContext().display()->drawLine(x + 11, height, x + 16, height);
}

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

inline void displayBrewTime(CleverCoffee::SystemContext& systemContext,
                            const int                    x,
                            const int                    y,
                            const char*                  label,
                            const double                 currBrewTime,
                            const double                 totalTargetBrewTime = -1) {
    if (!systemContext.hardwareContext().display()) return;

    constexpr int kValueColumnOffset = 50;
    constexpr int kValueColumnWidth  = 78;

    systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
    systemContext.hardwareContext().display()->setDrawColor(0);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        systemContext.hardwareContext().display()->drawBox(x, y + 1, 100, 10);
    } else {
        systemContext.hardwareContext().display()->drawBox(x + kValueColumnOffset, y + 1, kValueColumnWidth, 10);
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
        systemContext.hardwareContext().display()->setCursor(x + kValueColumnOffset, y);
        systemContext.hardwareContext().display()->print(currBrewTime / 1000, 0);

        if (totalTargetBrewTime > 0) {
            systemContext.hardwareContext().display()->print("/");
            systemContext.hardwareContext().display()->print(totalTargetBrewTime / 1000, 0);
        }

        systemContext.hardwareContext().display()->print(" s");
    }
}

inline void displayBrewWeight(CleverCoffee::SystemContext& systemContext,
                              const int                    x,
                              const int                    y,
                              const float                  weight,
                              const float                  setpoint = -1,
                              const bool                   fault    = false) {
    constexpr int kValueColumnOffset = 50;
    constexpr int kValueColumnWidth  = 78;

    systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
    systemContext.hardwareContext().display()->setDrawColor(0);

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        systemContext.hardwareContext().display()->drawBox(x, y + 1, 100, 10);
    } else {
        systemContext.hardwareContext().display()->drawBox(x + kValueColumnOffset, y + 1, kValueColumnWidth, 10);
    }

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

    if (systemContext.hardwareContext().scalePtr()) {
        if (systemContext.hardwareContext().scalePtr()->isConnected()) {
            systemContext.hardwareContext().display()->drawXBMP(x, y, 8, 9, Bluetooth_Icon);
        }
    }
}

inline bool displayMaintenanceStatusBar(CleverCoffee::SystemContext& systemContext, const int x, const int y) {
    if (!systemContext.hardwareContext().display() ||
        !Config::getInstance().maintenanceBackflushReminderEnabled.get() ||
        !systemContext.maintenanceCoordinator().isReminderDue()) {
        return false;
    }

    systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
    systemContext.hardwareContext().display()->drawStr(x, y, "CLEAN");
    return true;
}

inline void displayMaintenanceFooter(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.hardwareContext().display() ||
        !Config::getInstance().maintenanceBackflushReminderEnabled.get() ||
        !systemContext.maintenanceCoordinator().isReminderDue()) {
        return;
    }

    systemContext.hardwareContext().display()->setFont(u8g2_font_profont10_tf);
    systemContext.hardwareContext().display()->setCursor(0, 62);
    const char* reminderLine =
        langstring_backflush_reminder[0] != nullptr ? langstring_backflush_reminder[0] : "Backflush recommended";
    systemContext.hardwareContext().display()->print(reminderLine);
}

inline void displayStatusbar(CleverCoffee::SystemContext& systemContext) {
    if (!systemContext.hardwareContext().display()) return;
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

    if (!displayMaintenanceStatusBar(systemContext, 78, 0)) {
        const auto format = "%02luh %02lum";
        displayUptime(systemContext, 84, 0, format);
    }
}

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

inline void displayLogo(CleverCoffee::SystemContext& systemContext,
                        const char*                  displaymessagetext,
                        const char*                  displaymessagetext2) {
    if (!systemContext.hardwareContext().display()) return;
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        int printrow = 47;
        systemContext.hardwareContext().display()->clearBuffer();

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

inline void setDisplayFont(CleverCoffee::SystemContext& systemContext) {
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont10_tf);
    } else {
        systemContext.hardwareContext().display()->setFont(u8g2_font_profont11_tf);
    }
}

inline bool wordFitsOnLine(CleverCoffee::SystemContext& systemContext,
                           const char*                  line,
                           const char*                  word,
                           int                          displayWidth) {
    constexpr size_t MAX_TEST_LINE = MESSAGE_BUFFER_SIZE;
    char             testLine[MAX_TEST_LINE];
    snprintf(testLine, MAX_TEST_LINE, "%s%s", line, word);
    return systemContext.hardwareContext().display()->getUTF8Width(testLine) <= displayWidth;
}

inline void addWordToLine(char* line, size_t& lineLen, const char* word, size_t maxLineLen) {
    if (lineLen > 0) {
        strncat(line, " ", maxLineLen - lineLen - 1);
        lineLen++;
    }
    strncat(line, word, maxLineLen - lineLen - 1);
    lineLen += strlen(word);
}

inline void drawLineAndAdvance(
    CleverCoffee::SystemContext& systemContext, const char* line, int x, int& y, int lineHeight) {
    systemContext.hardwareContext().display()->drawUTF8(x, y, line);
    y += lineHeight;
}

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
                drawLineAndAdvance(systemContext, line, x, y, lineHeight);
                startNewLineWithWord(line, lineLen, word, MAX_LINE_LEN);
                wordCount = 1;
            } else {
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

    if (lineLen > 0 && y + lineHeight <= displayHeight) {
        systemContext.hardwareContext().display()->drawUTF8(x, y, line);
    }

    systemContext.hardwareContext().display()->sendBuffer();
}
