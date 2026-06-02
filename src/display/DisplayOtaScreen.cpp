/**
 * @file DisplayOtaScreen.cpp
 * @brief Fullscreen OLED screen for firmware/filesystem OTA updates
 */

#include "clevercoffee/display/DisplayOtaScreen.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/display/DisplayLayoutUtils.h"
#include "clevercoffee/display/DisplayWidgets.h"
#include "clevercoffee/ota.h"

#include <U8g2lib.h>

namespace {

const char* otaTypeLabel(const String& type) {
    if (type == OTA::Type::Filesystem) {
        return "Filesystem";
    }
    return "Firmware";
}

void truncateToWidth(U8G2* display, String& message) {
    constexpr int kMaxWidth = DISPLAY_WIDTH - 4;
    while (!message.isEmpty() && display->getStrWidth(message.c_str()) > kMaxWidth) {
        message.remove(message.length() - 1);
    }
    if (message.endsWith("..")) {
        return;
    }
    if (display->getStrWidth(message.c_str()) > kMaxWidth && message.length() > 3) {
        message = message.substring(0, message.length() - 3) + "...";
    }
}

} // namespace

namespace CleverCoffee::Display {

bool drawOtaScreen(SystemContext& systemContext) {
    if (!OTA::shouldShowOtaDisplay()) {
        return false;
    }

    U8G2* display = systemContext.hardwareContext().display();
    if (!display) {
        return false;
    }

    const uint8_t progress   = OTA::getProgress();
    const bool    hasError   = OTA::hasError();
    const String& status     = OTA::getUpdateStatus();
    const String& updateType = OTA::getUpdateType();

    display->clearBuffer();

    using Layout::drawStrCenteredOnScreen;

    if (hasError || status == OTA::Status::Error) {
        display->setFont(u8g2_font_fub17_tf);
        drawStrCenteredOnScreen(display, 8, "Update failed");

        display->setFont(u8g2_font_profont10_tf);
        String message = OTA::getErrorMessage();
        if (message.isEmpty()) {
            message = "Unknown error";
        }
        truncateToWidth(display, message);
        drawStrCenteredOnScreen(display, 32, message.c_str());

        display->setFont(u8g2_font_profont10_tf);
        drawStrCenteredOnScreen(display, 48, "Retry from web UI");
    } else if (status == OTA::Status::Complete) {
        display->setFont(u8g2_font_fub17_tf);
        drawStrCenteredOnScreen(display, 12, "Update OK");

        display->setFont(u8g2_font_profont11_tf);
        drawStrCenteredOnScreen(display, 36, "Restarting...");
    } else {
        display->setFont(u8g2_font_fub17_tf);
        drawStrCenteredOnScreen(display, 6, "Updating");

        display->setFont(u8g2_font_profont10_tf);
        drawStrCenteredOnScreen(display, 24, otaTypeLabel(updateType));

        displayProgressbar(systemContext, progress, 14, 38, 100);

        char percentBuf[8];
        snprintf(percentBuf, sizeof(percentBuf), "%u%%", progress);
        display->setFont(u8g2_font_profont11_tf);
        drawStrCenteredOnScreen(display, 50, percentBuf);
    }

    display->sendBuffer();
    return true;
}

void refreshOtaDisplay(SystemContext& systemContext) {
    if (!Config::getInstance().hardwareOledEnabled.get()) {
        return;
    }
    if (U8G2* display = systemContext.hardwareContext().display()) {
        display->setPowerSave(0);
    }
    (void)drawOtaScreen(systemContext);
}

} // namespace CleverCoffee::Display
