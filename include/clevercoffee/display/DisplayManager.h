/**
 * @file DisplayManager.h
 * @brief RAII wrapper for U8G2 display management
 */

#pragma once

#include "clevercoffee/defaults.h"
#include "clevercoffee/display/IDisplayManager.h"

#include <U8g2lib.h>
#include <memory>

/**
 * @class DisplayManager
 * @brief RAII wrapper for U8G2 display with automatic resource management
 *
 * This class provides safe management of U8G2 display instances using RAII principles.
 * It automatically handles display initialization and cleanup, preventing memory leaks
 * and ensuring proper display shutdown.
 */
class DisplayManager : public IDisplayManager {
  public:
    /**
     * @brief Construct DisplayManager for specific display type
     * @param type Display type (0 = SH1106, 1 = SSD1306)
     * @param address Display I2C address (0 = 0x3C, 1 = 0x3D)
     */
    explicit DisplayManager(Hardware::OLEDType type, Hardware::OLEDAddress address);

    /**
     * @brief Destructor
     */
    ~DisplayManager() = default;

    // Disable copy constructor and assignment operator
    DisplayManager(const DisplayManager&)            = delete;
    DisplayManager& operator=(const DisplayManager&) = delete;

    // Enable move constructor and assignment operator
    DisplayManager(DisplayManager&&)            = default;
    DisplayManager& operator=(DisplayManager&&) = default;

    /**
     * @brief Get raw U8G2 pointer for compatibility with existing code
     * @return Pointer to U8G2 instance, or nullptr if not initialized
     */
    U8G2* getDisplay() const noexcept override {
        return display_.get();
    }

    /**
     * @brief Check if display is successfully initialized
     * @return true if display is ready for use
     */
    bool isInitialized() const noexcept override {
        return display_ != nullptr;
    }

    // === High-level display operations ===

    void setPowerSave(bool enabled) noexcept override {
        if (display_) {
            display_->setPowerSave(enabled ? 1 : 0);
        }
    }

    void clear() noexcept override {
        if (display_) {
            display_->clearBuffer();
        }
    }

    void update() noexcept override {
        if (display_) {
            display_->sendBuffer();
        }
    }

    uint8_t drawString(int x, int y, const char* text) noexcept override {
        if (display_ && text) {
            return display_->drawStr(x, y, text);
        }
        return 0;
    }

    void setFont(const uint8_t* font) noexcept override {
        if (display_ && font) {
            display_->setFont(font);
        }
    }

  private:
    std::unique_ptr<U8G2> display_;

    /**
     * @brief Create appropriate U8G2 instance based on type
     * @param type Display type
     * @param address Display I2C address
     * @return Unique pointer to created display instance
     */
    std::unique_ptr<U8G2> createDisplay(Hardware::OLEDType type, Hardware::OLEDAddress address);
};
