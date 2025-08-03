/**
 * @file UIManager.h
 * @brief User Interface Manager for display and user interactions
 */

#pragma once

#include <Arduino.h>
#include <U8g2lib.h>
#include <memory>

// Forward declarations
class DisplayManager;
class Config;

/**
 * @class UIManager
 * @brief Central manager for all user interface operations
 *
 * This class manages all UI-related functionality including:
 * - Display operations and screen updates
 * - Brew timer displays and state management
 * - Status indicators (WiFi, MQTT, Bluetooth)
 * - Message and logo displays
 * - Temperature and brewing visualizations
 * - Progress bars and fullscreen timers
 *
 * Key responsibilities:
 * - Encapsulate all U8G2 display operations
 * - Manage display update timing and buffering
 * - Provide high-level UI functions for different app states
 * - Handle display rotation and configuration
 * - Coordinate with DisplayManager for hardware abstraction
 */
class UIManager {
    public:
        /**
         * @brief Constructor
         * @param displayManager Display hardware manager instance
         */
        explicit UIManager(DisplayManager* displayManager);

        /**
         * @brief Destructor
         */
        ~UIManager() = default;

        // Disable copy constructor and assignment operator
        UIManager(const UIManager&) = delete;
        UIManager& operator=(const UIManager&) = delete;

        // Enable move constructor and assignment operator
        UIManager(UIManager&&) = default;
        UIManager& operator=(UIManager&&) = default;

        /**
         * @brief Initialize UI manager
         * @return true if initialization successful
         */
        bool initialize();

        /**
         * @brief Main UI update - call from main loop
         *
         * Handles display updates, timing, and buffer management.
         * This should be called regularly from the main loop.
         */
        void update();

        /**
         * @brief Prepare U8G2 display with default settings
         *
         * Sets up font, rotation, colors based on configuration.
         */
        void prepareDisplay();

        // === Logo and Message Display ===

        /**
         * @brief Display CleverCoffee logo with custom messages
         * @param message1 First line of text
         * @param message2 Second line of text
         */
        void displayLogo(const String& message1, const String& message2);

        /**
         * @brief Display multi-line message
         * @param text1 Line 1 (y=0)
         * @param text2 Line 2 (y=10)
         * @param text3 Line 3 (y=20)
         * @param text4 Line 4 (y=30)
         * @param text5 Line 5 (y=40)
         * @param text6 Line 6 (y=50)
         */
        void displayMessage(const String& text1, const String& text2 = "", const String& text3 = "", const String& text4 = "", const String& text5 = "", const String& text6 = "");

        // === Brew Timer Management ===

        /**
         * @brief Check if brew timer should be displayed
         * @return true if timer should be shown
         *
         * Manages brew timer state machine and post-brew display duration.
         */
        bool shouldDisplayBrewTimer();

        /**
         * @brief Display current brew time
         */
        void displayBrewTime();

        /**
         * @brief Display fullscreen brew timer
         */
        void displayFullscreenBrewTimer();

        /**
         * @brief Display fullscreen manual flush timer
         */
        void displayFullscreenManualFlushTimer();

        /**
         * @brief Display fullscreen hot water timer
         */
        void displayFullscreenHotWaterTimer();

        // === Status and Indicators ===

        /**
         * @brief Display status bar with connection indicators
         */
        void displayStatusbar();

        /**
         * @brief Display WiFi connection status
         */
        void displayWiFiStatus();

        /**
         * @brief Display MQTT connection status
         */
        void displayMQTTStatus();

        /**
         * @brief Display Bluetooth connection status
         */
        void displayBluetoothStatus();

        /**
         * @brief Display offline mode indicator
         */
        void displayOfflineMode();

        // === Temperature and Brewing ===

        /**
         * @brief Display temperature in large font
         */
        void displayTemperature();

        /**
         * @brief Display thermometer outline graphic
         */
        void displayThermometerOutline();

        /**
         * @brief Display brewing weight
         */
        void displayBrewWeight();

        /**
         * @brief Display progress bar
         * @param progress Progress value (0-100)
         * @param x X position
         * @param y Y position
         * @param width Bar width
         * @param height Bar height
         */
        void displayProgressbar(int progress, int x, int y, int width, int height);

        // === System Information ===

        /**
         * @brief Display system uptime
         */
        void displayUptime();

        /**
         * @brief Display current machine state
         */
        void displayMachineState();

        /**
         * @brief Display scale failure message
         */
        void displayScaleFailed();

        /**
         * @brief Display wrapped text message
         * @param message Text to display with wrapping
         */
        void displayWrappedMessage(const String& message);

        // === Display Buffer Management ===

        /**
         * @brief Check if display buffer is ready for update
         * @return true if buffer is ready
         */
        bool isBufferReady() const {
            return bufferReady_;
        }

        /**
         * @brief Set buffer ready state
         * @param ready Buffer ready state
         */
        void setBufferReady(bool ready) {
            bufferReady_ = ready;
        }

        /**
         * @brief Check if display update is running
         * @return true if update is in progress
         */
        bool isUpdateRunning() const {
            return updateRunning_;
        }

        /**
         * @brief Set update running state
         * @param running Update running state
         */
        void setUpdateRunning(bool running) {
            updateRunning_ = running;
        }

        /**
         * @brief Force display update
         *
         * Immediately sends buffer to display.
         */
        void forceUpdate();

    private:
        /**
         * @brief Brew timer state enumeration
         */
        enum class BrewTimerState {
            Idle = 10,
            Running = 20,
            PostBrew = 30
        };

        /**
         * @brief Get U8G2 rotation value from rotation index
         * @param rotation Rotation index
         * @return U8G2 rotation constant
         */
        const u8g2_cb_t* getU8G2Rotation(int rotation);

        // Manager dependencies
        DisplayManager* displayManager_;
        U8G2* u8g2_;

        // Display state
        bool initialized_;
        bool bufferReady_;
        bool updateRunning_;

        // Brew timer state
        BrewTimerState brewTimerState_;
        uint32_t brewEndTime_;
};