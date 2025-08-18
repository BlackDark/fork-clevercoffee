/**
 * @file DisplayManager.cpp
 * @brief Implementation of RAII wrapper for U8G2 display management
 */

#include "clevercoffee/display/DisplayManager.h"

#include "clevercoffee/Logger.h"
#include "clevercoffee/hardware/pinmapping.h"

DisplayManager::DisplayManager(Hardware::OLEDType type, Hardware::OLEDAddress address) {
    display_ = createDisplay(type, address);

    if (display_) {
        // Set I2C address based on parameter
        if (address == Hardware::OLEDAddress::ADDR_3C) {
            display_->setI2CAddress(0x3C * 2);
        } else {
            display_->setI2CAddress(0x3D * 2);
        }

        // Initialize the display
        display_->begin();
        display_->clearBuffer();

        LOG(INFO, "Display initialized successfully");
    } else {
        LOG(ERROR, "Failed to create display instance");
    }
}

std::unique_ptr<U8G2> DisplayManager::createDisplay(Hardware::OLEDType type, Hardware::OLEDAddress address) {
    switch (type) {
        case Hardware::OLEDType::SH1106:
            // SH1106 1.3" display
            return std::make_unique<U8G2_SH1106_128X64_NONAME_F_HW_I2C>(U8G2_R0, U8X8_PIN_NONE, PIN_I2CSCL, PIN_I2CSDA);

        case Hardware::OLEDType::SSD1306:
            // SSD1306 0.96" display
            return std::make_unique<U8G2_SSD1306_128X64_NONAME_F_HW_I2C>(
                U8G2_R0, U8X8_PIN_NONE, PIN_I2CSCL, PIN_I2CSDA);

        default:
            LOGF(ERROR, "Unknown display type: %d", type);
            return nullptr;
    }
}
