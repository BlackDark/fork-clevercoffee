/**
 * @file UIManager.cpp
 * @brief Implementation of UIManager for display and user interface management
 */

#include "UIManager.h"
#include "../Config.h"
#include "../handlers/BrewHandler.h"
#include "../display/DisplayManager.h"
#include "../display/bitmaps.h"
#include "../state/GlobalState.h"
#include "Logger.h"
#include <Arduino.h>

int getSignalStrength();

UIManager::UIManager(DisplayManager* displayManager) :
    displayManager_(displayManager), u8g2_(nullptr), initialized_(false), bufferReady_(false), updateRunning_(false), brewTimerState_(BrewTimerState::Idle), brewEndTime_(0) {

    LOG(INFO, "UIManager created");
}

bool UIManager::initialize() {
    LOG(INFO, "Initializing UIManager");

    if (!displayManager_) {
        LOG(ERROR, "UIManager: DisplayManager is null");
        return false;
    }

    u8g2_ = displayManager_->getDisplay();
    if (!u8g2_) {
        LOG(ERROR, "UIManager: Failed to get U8G2 instance from DisplayManager");
        return false;
    }

    // Prepare display with default settings
    prepareDisplay();

    initialized_ = true;

    LOG(INFO, "UIManager initialized successfully");
    return true;
}

void UIManager::update() {
    if (!initialized_) {
        LOG(WARNING, "UIManager::update() called but not initialized");
        return;
    }

    // Handle display buffer updates
    if (bufferReady_) {
        u8g2_->sendBuffer();
        bufferReady_ = false;
        updateRunning_ = true;
    }
}

void UIManager::prepareDisplay() {
    if (!u8g2_) {
        LOG(ERROR, "UIManager::prepareDisplay() called but u8g2 is null");
        return;
    }

    int rotation = 0;
    u8g2_->clearBuffer();
    u8g2_->setFont(u8g2_font_profont11_tf);
    u8g2_->setFontRefHeightExtendedText();
    u8g2_->setDrawColor(1);
    u8g2_->setFontPosTop();
    u8g2_->setFontDirection(0);

    if (Config::getInstance().displayInverted.get()) {
        rotation += 2;
    }

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        rotation++;
    }

    u8g2_->setDisplayRotation(getU8G2Rotation(rotation));
}

void UIManager::displayLogo(const String& message1, const String& message2) {
    if (!u8g2_) return;

    if (Config::getInstance().displayTemplate.get() == System::DisplayTemplate::UPRIGHT) {
        int printrow = 47;
        u8g2_->clearBuffer();

        // Create modifiable copies
        char text1[message1.length() + 1];
        char text2[message2.length() + 1];

        strcpy(text1, message1.c_str());
        strcpy(text2, message2.c_str());

        char* token = strtok(text1, " ");

        while (token != nullptr) {
            u8g2_->drawStr(0, printrow, token);
            token = strtok(nullptr, " ");
            printrow += 10;
        }

        token = strtok(text2, " ");

        while (token != nullptr) {
            u8g2_->drawStr(0, printrow, token);
            token = strtok(nullptr, " ");
            printrow += 10;
        }

        u8g2_->drawXBMP(11, 4, CleverCoffee_Logo_width, CleverCoffee_Logo_height, CleverCoffee_Logo);
    }
    else {
        u8g2_->clearBuffer();
        u8g2_->drawStr(0, 45, message1.c_str());
        u8g2_->drawStr(0, 55, message2.c_str());
        u8g2_->drawXBMP(38, 0, CleverCoffee_Logo_width, CleverCoffee_Logo_height, CleverCoffee_Logo);
    }

    u8g2_->sendBuffer();
}

void UIManager::displayMessage(const String& text1, const String& text2, const String& text3, const String& text4, const String& text5, const String& text6) {
    if (!u8g2_) return;

    u8g2_->clearBuffer();
    u8g2_->setCursor(0, 0);
    u8g2_->print(text1);
    u8g2_->setCursor(0, 10);
    u8g2_->print(text2);
    u8g2_->setCursor(0, 20);
    u8g2_->print(text3);
    u8g2_->setCursor(0, 30);
    u8g2_->print(text4);
    u8g2_->setCursor(0, 40);
    u8g2_->print(text5);
    u8g2_->setCursor(0, 50);
    u8g2_->print(text6);
    u8g2_->sendBuffer();
}

bool UIManager::shouldDisplayBrewTimer() {
    switch (brewTimerState_) {
        case BrewTimerState::Idle:
            if (g_state.handlers.brewHandler && g_state.handlers.brewHandler->isBrewActive()) {
                brewTimerState_ = BrewTimerState::Running;
            }
            break;

        case BrewTimerState::Running:
            if (!g_state.handlers.brewHandler || !g_state.handlers.brewHandler->isBrewActive()) {
                brewTimerState_ = BrewTimerState::PostBrew;
                brewEndTime_ = millis();
            }
            break;

        case BrewTimerState::PostBrew:
            if (millis() - brewEndTime_ > static_cast<uint32_t>(Config::getInstance().displayPostBrewTimerDuration.get() * 1000)) {
                brewTimerState_ = BrewTimerState::Idle;
            }
            break;
    }

    return brewTimerState_ != BrewTimerState::Idle;
}

void UIManager::displayBrewTime() {
    if (!u8g2_) return;

    // Implementation will be added based on existing displayBrewTime function
    // For now, just show the current brew time
    u8g2_->setFont(u8g2_font_profont15_tf);
    char timeStr[16];
    snprintf(timeStr, sizeof(timeStr), "%.1fs", g_state.process.currBrewTime / 1000.0);
    u8g2_->drawStr(0, 20, timeStr);
}

void UIManager::displayFullscreenBrewTimer() {
    if (!u8g2_) return;

    u8g2_->clearBuffer();
    u8g2_->setFont(u8g2_font_profont22_tn);

    char timeStr[16];
    snprintf(timeStr, sizeof(timeStr), "%.1f", g_state.process.currBrewTime / 1000.0);

    // Center the text
    int textWidth = u8g2_->getStrWidth(timeStr);
    int x = (u8g2_->getDisplayWidth() - textWidth) / 2;

    u8g2_->drawStr(x, 32, timeStr);
    u8g2_->sendBuffer();
}

void UIManager::displayFullscreenManualFlushTimer() {
    // Implementation similar to brew timer but for manual flush
    displayFullscreenBrewTimer(); // Placeholder
}

void UIManager::displayFullscreenHotWaterTimer() {
    // Implementation similar to brew timer but for hot water
    displayFullscreenBrewTimer(); // Placeholder
}

void UIManager::displayStatusbar() {
    if (!u8g2_) return;

    u8g2_->setFont(u8g2_font_profont11_tf);

    // WiFi status
    if (!g_state.network.offlineMode) {
        for (int i = 0; i < getSignalStrength(); i++) {
            u8g2_->drawLine(100 + i * 2, 8 - i, 100 + i * 2, 8);
        }
    }

    // Show reconnection attempts
    if (g_state.network.wifiReconnects > 0) {
        char reconnectStr[8];
        snprintf(reconnectStr, sizeof(reconnectStr), "R%d", g_state.network.wifiReconnects);
        u8g2_->drawStr(110, 0, reconnectStr);
    }
}

void UIManager::displayWiFiStatus() {
    displayStatusbar(); // WiFi status is part of status bar
}

void UIManager::displayMQTTStatus() {
    if (!u8g2_) return;

    // Display MQTT connection indicator
    u8g2_->setFont(u8g2_font_profont11_tf);
    u8g2_->drawStr(90, 0, "MQTT");
}

void UIManager::displayBluetoothStatus() {
    if (!u8g2_) return;

    // Display Bluetooth connection indicator
    u8g2_->setFont(u8g2_font_profont11_tf);
    u8g2_->drawStr(70, 0, "BT");
}

void UIManager::displayOfflineMode() {
    if (!u8g2_) return;

    u8g2_->setFont(u8g2_font_profont11_tf);
    u8g2_->drawStr(0, 0, "OFFLINE");
}

void UIManager::displayTemperature() {
    if (!u8g2_) return;

    u8g2_->setFont(u8g2_font_profont22_tn);

    char tempStr[16];
    snprintf(tempStr, sizeof(tempStr), "%.1f°C", g_state.process.temperature);

    u8g2_->drawStr(10, 32, tempStr);
}

void UIManager::displayThermometerOutline() {
    if (!u8g2_) return;

    // Draw thermometer outline (simplified version)
    u8g2_->drawFrame(120, 10, 6, 40);
    u8g2_->drawDisc(123, 52, 4);

    // Fill based on g_state.process.temperature relative to setpoint
    if (g_state.process.temperature > 0 && g_state.process.setpoint > 0) {
        int fillHeight = static_cast<int>((g_state.process.temperature / g_state.process.setpoint) * 35);
        if (fillHeight > 35) fillHeight = 35;

        for (int i = 0; i < fillHeight; i++) {
            u8g2_->drawHLine(121, 48 - i, 4);
        }
    }
}

void UIManager::displayBrewWeight() {
    if (!u8g2_) return;

    u8g2_->setFont(u8g2_font_profont15_tf);

    char weightStr[16];
    snprintf(weightStr, sizeof(weightStr), "%.1fg", g_state.sensors.currReadingWeight);

    u8g2_->drawStr(0, 50, weightStr);
}

void UIManager::displayProgressbar(int progress, int x, int y, int width, int height) {
    if (!u8g2_) return;

    // Clamp progress to 0-100
    if (progress < 0) progress = 0;
    if (progress > 100) progress = 100;

    // Draw outline
    u8g2_->drawFrame(x, y, width, height);

    // Draw fill
    int fillWidth = (progress * (width - 2)) / 100;
    if (fillWidth > 0) {
        u8g2_->drawBox(x + 1, y + 1, fillWidth, height - 2);
    }
}

void UIManager::displayUptime() {
    if (!u8g2_) return;

    u8g2_->setFont(u8g2_font_profont11_tf);

    unsigned long seconds = millis() / 1000;
    unsigned long minutes = seconds / 60;
    unsigned long hours = minutes / 60;

    char uptimeStr[16];
    snprintf(uptimeStr, sizeof(uptimeStr), "%luh %lum", hours, minutes % 60);

    u8g2_->drawStr(0, 0, uptimeStr);
}

void UIManager::displayMachineState() {
    if (!u8g2_) return;

    u8g2_->setFont(u8g2_font_profont11_tf);

    const char* stateStr = "Unknown";

    // Map machine state to display string
    // This will be enhanced based on actual machine state values
    if (g_state.machine.steamON) {
        stateStr = "Steam";
    }
    else if (g_state.process.currBrewTime > 0) {
        stateStr = "Brewing";
    }
    else {
        stateStr = "Ready";
    }

    u8g2_->drawStr(0, 54, stateStr);
}

void UIManager::displayScaleFailed() {
    displayMessage("Scale", "Failed", "Check", "Connection", "", "");
}

void UIManager::displayWrappedMessage(const char* message) {
    if (!u8g2_) return;

    u8g2_->clearBuffer();
    u8g2_->setFont(u8g2_font_profont11_tf);

    // Simple word wrapping implementation
    const int maxWidth = u8g2_->getDisplayWidth() - 4;
    const int lineHeight = 12;
    int currentY = 10;

    String remainingText = message;

    while (remainingText.length() > 0 && currentY < 60) {
        String line = "";
        String word = "";
        int spacePos = remainingText.indexOf(' ');

        if (spacePos == -1) {
            // Last word
            line = remainingText;
            remainingText = "";
        }
        else {
            // Build line word by word
            while (spacePos != -1) {
                word = remainingText.substring(0, spacePos);
                String testLine = line.length() > 0 ? line + " " + word : word;

                if (u8g2_->getStrWidth(testLine.c_str()) <= maxWidth) {
                    line = testLine;
                    remainingText = remainingText.substring(spacePos + 1);
                    spacePos = remainingText.indexOf(' ');
                }
                else {
                    break;
                }
            }

            if (line.length() == 0) {
                // Single word too long, just use it
                line = word;
                remainingText = remainingText.substring(spacePos + 1);
            }
        }

        u8g2_->drawStr(2, currentY, line.c_str());
        currentY += lineHeight;
    }

    u8g2_->sendBuffer();
}

void UIManager::forceUpdate() {
    if (!u8g2_) return;

    u8g2_->sendBuffer();
    updateRunning_ = true;
}

const u8g2_cb_t* UIManager::getU8G2Rotation(int rotation) {
    // Map rotation index to U8G2 rotation constants
    switch (rotation) {
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
