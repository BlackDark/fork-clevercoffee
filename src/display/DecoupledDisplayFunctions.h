/**
 * @file DecoupledDisplayFunctions.h
 * @brief Display functions with dependency injection
 */

#pragma once

#include "DisplayContext.h"
#include "IDisplay.h"
#include "../Config.h"
#include "../network/MQTTManager.h"
#include "../network/CleverCoffeeWiFiManager.h"
#include "../defaults.h"

/**
 * @brief Set appropriate font for current display template
 * @param context Display context containing display interface
 */
inline void setDisplayFont(DisplayContext& context) {
    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        context.setFont(u8g2_font_profont10_tf);
    }
    else {
        context.setFont(u8g2_font_profont11_tf);
    }
}

/**
 * @brief Draw an MQTT status indicator at the given coordinates if MQTT is enabled
 * @param context Display context containing display and network data
 * @param x X coordinate
 * @param y Y coordinate
 */
inline void displayMQTTStatus(DisplayContext& context, const int x, const int y) {
    if (!Config::getInstance().mqttEnabled.get()) {
        return;
    }
    
    auto* display = context.getDisplay();
    const auto& networkData = context.getNetworkData();
    
    if (networkData.mqttManager && networkData.mqttManager->isConnected()) {
        display->setCursor(x, y);
        display->setFont(u8g2_font_profont11_tf);
        display->print("MQTT");
        
        if (networkData.wifiManager && networkData.wifiManager->getSignalStrength() <= 1) {
            display->print("!");
        }
    }
    else {
        display->setCursor(x, y);
        display->print("");
    }
}

/**
 * @brief Draw the outline of a thermometer
 * @param context Display context containing display interface
 * @param x X coordinate
 * @param y Y coordinate
 */
inline void displayThermometerOutline(DisplayContext& context, const int x, const int y) {
    auto* display = context.getDisplay();
    const auto& processData = context.getProcessData();
    
    display->drawLine(x + 3, y - 9, x + 3, y - 42);
    display->drawLine(x + 9, y - 9, x + 9, y - 42);
    display->drawPixel(x + 4, y - 43);
    display->drawPixel(x + 8, y - 43);
    display->drawLine(x + 5, y - 44, x + 7, y - 44);
    display->drawDisc(x + 6, y - 5, 6);
    
    // draw setpoint line
    const int height = map(static_cast<int>(processData.setpoint), 0, 100, y - 9, y - 39);
    display->drawLine(x + 11, height, x + 16, height);
}

/**
 * @brief Draw temperature bar, e.g. inside the thermometer outline
 * @param context Display context containing display and process data
 * @param x X coordinate
 * @param heightRange Height range for the bar
 */
inline void drawTemperaturebar(DisplayContext& context, const int x, const int heightRange) {
    auto* display = context.getDisplay();
    const auto& processData = context.getProcessData();
    
    const int width = x + 5;
    
    for (int i = x; i < width; i++) {
        const int height = map(static_cast<int>(processData.temperature), 0, 100, 0, heightRange);
        display->drawVLine(i, 52 - height, height);
    }
    
    if (processData.temperature > 100) {
        display->drawLine(x, heightRange - 11, x + 3, heightRange - 11);
        display->drawLine(x, heightRange - 10, x + 4, heightRange - 10);
        display->drawLine(x, heightRange - 9, x + 4, heightRange - 9);
    }
}

/**
 * @brief Draw the temperature in big font at given position
 * @param context Display context containing display and process data
 * @param x X coordinate
 * @param y Y coordinate
 */
inline void displayTemperature(DisplayContext& context, const int x, const int y) {
    auto* display = context.getDisplay();
    const auto& processData = context.getProcessData();
    
    display->setFont(u8g2_font_fub30_tf);
    
    if (processData.temperature < 99.499) {
        display->setCursor(x + 20, y);
        display->print(processData.temperature, 0);
    }
    else {
        display->setCursor(x, y);
        display->print(processData.temperature, 0);
    }
    
    display->drawCircle(x + 72, y + 4, 3);
}

/**
 * @brief Display wrapped message with word wrapping
 * @param context Display context containing display interface
 * @param message Message to display
 */
inline void displayWrappedMessage(DisplayContext& context, const char* message) {
    auto* display = context.getDisplay();
    
    display->clearBuffer();
    setDisplayFont(context);
    
    const int lineHeight = display->getMaxCharHeight() + 2;
    const int displayWidth = display->getDisplayWidth();
    const int displayHeight = display->getDisplayHeight();
    
    int x = 0;
    int y = 0;
    
    // Use fixed-size buffers to avoid String allocation
    constexpr size_t MAX_WORD_LEN = 32;
    constexpr size_t MAX_LINE_LEN = MESSAGE_BUFFER_SIZE;
    char word[MAX_WORD_LEN] = {0};
    char line[MAX_LINE_LEN] = {0};
    char testLine[MAX_LINE_LEN] = {0};
    
    size_t wordIdx = 0;
    size_t lineLen = 0;
    const size_t msgLen = strlen(message);
    
    for (size_t i = 0; i <= msgLen; ++i) {
        const char c = (i < msgLen) ? message[i] : '\0';
        
        if (c == ' ' || c == '\n' || c == '\0') {
            word[wordIdx] = '\0';
            
            // Test if adding word would exceed display width
            snprintf(testLine, MAX_LINE_LEN, "%s%s", line, word);
            
            if (display->getUTF8Width(testLine) > displayWidth && lineLen > 0) {
                // Draw current line and start new line with word
                display->drawUTF8(x, y, line);
                y += lineHeight;
                snprintf(line, MAX_LINE_LEN, "%s ", word);
                lineLen = strlen(line);
            }
            else {
                // Add word to current line
                if (lineLen > 0) {
                    strncat(line, " ", MAX_LINE_LEN - lineLen - 1);
                    lineLen++;
                }
                strncat(line, word, MAX_LINE_LEN - lineLen - 1);
                lineLen += strlen(word);
            }
            
            wordIdx = 0;
            
            if (c == '\n') {
                display->drawUTF8(x, y, line);
                y += lineHeight;
                line[0] = '\0';
                lineLen = 0;
            }
        }
        else if (wordIdx < MAX_WORD_LEN - 1) {
            word[wordIdx++] = c;
        }
    }
    
    // Draw final line if it has content and fits
    if (lineLen > 0 && y + lineHeight <= displayHeight) {
        display->drawUTF8(x, y, line);
    }
    
    display->sendBuffer();
}