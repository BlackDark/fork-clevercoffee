/**
 * @file OledDriver.cpp
 * @brief OLED setup and I2C buffer flush
 */

#include "clevercoffee/ui/OledDriver.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/display/DisplayManager.h"
#include "clevercoffee/types/GlobalTypes.h"

OledDriver::OledDriver(DisplayManager* displayManager, CleverCoffee::SystemContext* systemContext)
    : displayManager_(displayManager), systemContext_(systemContext), u8g2_(nullptr), initialized_(false),
      bufferReady_(false), updateRunning_(false) {}

bool OledDriver::initialize() {
    if (!displayManager_) {
        LOG(ERROR, "OledDriver: DisplayManager is null");
        return false;
    }

    u8g2_ = displayManager_->getDisplay();
    if (!u8g2_) {
        LOG(ERROR, "OledDriver: Failed to get U8G2 instance from DisplayManager");
        return false;
    }

    prepareDisplay();
    initialized_ = true;
    return true;
}

void OledDriver::prepareDisplay() {
    if (!u8g2_) {
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

void OledDriver::forceUpdate() {
    if (!u8g2_) {
        return;
    }
    u8g2_->sendBuffer();
    updateRunning_ = true;
}

const u8g2_cb_t* OledDriver::getU8G2Rotation(int rotation) {
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
