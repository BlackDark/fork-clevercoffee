/**
 * @file DisplayManager.h
 * @brief RAII wrapper for U8G2 display management
 */

#pragma once

#include "../defaults.h"
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
class DisplayManager {
    public:
        /**
         * @brief Construct DisplayManager for specific display type
         * @param type Display type (0 = SH1106, 1 = SSD1306)
         * @param address Display I2C address (0 = 0x3C, 1 = 0x3D)
         */
        DisplayManager(Hardware::OLEDType type, Hardware::OLEDAddress address);

        /**
         * @brief Destructor - automatically cleans up display resources
         */
        ~DisplayManager() = default;

        // Disable copy constructor and assignment operator
        DisplayManager(const DisplayManager&) = delete;
        DisplayManager& operator=(const DisplayManager&) = delete;

        // Enable move constructor and assignment operator
        DisplayManager(DisplayManager&&) = default;
        DisplayManager& operator=(DisplayManager&&) = default;

        /**
         * @brief Get raw U8G2 pointer for compatibility with existing code
         * @return Pointer to U8G2 instance, or nullptr if not initialized
         */
        U8G2* get() const {
            return display_.get();
        }

        /**
         * @brief Check if display is successfully initialized
         * @return true if display is ready for use
         */
        bool isInitialized() const {
            return display_ != nullptr;
        }

        /**
         * @brief Arrow operator for direct access to U8G2 methods
         * @return Pointer to U8G2 instance
         */
        U8G2* operator->() const {
            return display_.get();
        }

        /**
         * @brief Dereference operator for direct access to U8G2 object
         * @return Reference to U8G2 instance
         */
        U8G2& operator*() const {
            return *display_;
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
