/**
 * @file SystemInitializer.h
 * @brief RAII wrapper for system initialization
 */

#pragma once

#include <Arduino.h>
#include <functional>
#include <memory>

#if __cplusplus >= 202300L
#include <expected>
#endif

// C++23 Error handling for initialization
#if __cplusplus >= 202300L
enum class InitError {
    LoggerInitFailed,
    ConfigInitFailed, 
    DisplayInitFailed,
    HardwareInitFailed,
    NetworkingInitFailed,
    MQTTInitFailed,
    PIDInitFailed,
    SensorInitFailed,
    I2CInitFailed,
    MemoryAllocationFailed
};
#endif

// Forward declarations
class DisplayManager;
class UIManager;
class HardwareManager;
class MQTTManager;
class SensorManager;
class WebServerManager;
class CleverCoffeeWiFiManager;

/**
 * @class SystemInitializer
 * @brief RAII wrapper for complete system initialization
 *
 * This class provides safe management of system initialization using RAII principles.
 * It encapsulates all initialization phases with proper error handling and logging.
 */
class SystemInitializer {
    public:
        /**
         * @brief Constructor - initializes system initializer
         */
        SystemInitializer();

        /**
         * @brief Destructor - automatically cleans up resources
         */
        ~SystemInitializer();

        // Disable copy constructor and assignment operator
        SystemInitializer(const SystemInitializer&) = delete;
        SystemInitializer& operator=(const SystemInitializer&) = delete;

        // Enable move constructor and assignment operator
        SystemInitializer(SystemInitializer&&) = default;
        SystemInitializer& operator=(SystemInitializer&&) = default;

        /**
         * @brief Initialize complete system
         * @return true if initialization successful
         */
        bool initialize();

#if __cplusplus >= 202300L
        /**
         * @brief Initialize complete system with detailed error reporting (C++23)
         * @return Success or specific initialization error
         */
        std::expected<void, InitError> initializeModern();
#endif

        /**
         * @brief Check if system is initialized
         * @return true if system is ready
         */
        bool isInitialized() const {
            return systemInitialized_;
        }

        /**
         * @brief Get display manager
         * @return Pointer to display manager (may be null)
         */
        DisplayManager* getDisplayManager() const {
            return displayManager_.get();
        }

        /**
         * @brief Get display manager
         * @return Pointer to display manager (may be null)
         */
        UIManager* getUIManager() const {
            return uiManager_.get();
        }

        /**
         * @brief Get hardware manager
         * @return Pointer to hardware manager (may be null)
         */
        HardwareManager* getHardwareManager() const {
            return hardwareManager_.get();
        }

        /**
         * @brief Get MQTT manager
         * @return Pointer to MQTT manager (may be null)
         */
        MQTTManager* getMQTTManager() const {
            return mqttManager_.get();
        }

        /**
         * @brief Get sensor manager
         * @return Pointer to sensor manager (may be null)
         */
        SensorManager* getSensorManager() const {
            return sensorManager_.get();
        }

        /**
         * @brief Get WiFi manager
         * @return Pointer to WiFi manager (may be null)
         */
        class CleverCoffeeWiFiManager* getWiFiManager() const;

        /**
         * @brief Get web server manager
         * @return Pointer to web server manager (may be null)
         */
        WebServerManager* getWebServerManager() const {
            return webServerManager_.get();
        }

    private:
        // Initialization state
        bool systemInitialized_;
        String hostname_;

        // Manager instances
        std::unique_ptr<DisplayManager> displayManager_;
        std::unique_ptr<UIManager> uiManager_;
        std::unique_ptr<HardwareManager> hardwareManager_;
        std::unique_ptr<MQTTManager> mqttManager_;
        std::unique_ptr<SensorManager> sensorManager_;
        std::unique_ptr<CleverCoffeeWiFiManager> cleverCoffeeWiFiManager_;
        std::unique_ptr<WebServerManager> webServerManager_;

        // Initialization phases
        bool initializeLogger();
        bool initializeConfiguration();
        bool initializeDisplay();
        bool initializeHardware();
        bool initializeNetworking();
        bool initializeMQTT();
        bool initializePID();
        bool initializeSensors();
        bool finalizeMachineState();

#if __cplusplus >= 202300L
        // C++23 initialization phases with detailed error reporting
        std::expected<void, InitError> initializeLoggerModern();
        std::expected<void, InitError> initializeConfigurationModern();
        std::expected<void, InitError> initializeDisplayModern();
        std::expected<void, InitError> initializeHardwareModern();
        std::expected<void, InitError> initializeNetworkingModern();
        std::expected<void, InitError> initializeMQTTModern();
        std::expected<void, InitError> initializePIDModern();
        std::expected<void, InitError> initializeSensorsModern();
#endif

        // Network setup helpers
        void setupWiFi();

        // Helper methods
        void calculateDerivedValues();
        void setupTiming();
        void registerMQTTParameters();
        void registerMQTTSensors();
};
