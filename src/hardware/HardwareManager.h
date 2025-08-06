/**
 * @file HardwareManager.h
 * @brief RAII wrapper for hardware component management
 */

#pragma once

#include "GPIOPin.h"
#include "IOSwitch.h"
#include "LED.h"
#include "Relay.h"
#include "StandardLED.h"
#include "Switch.h"
#include "tempsensors/TempSensor.h"
#include "tempsensors/TempSensorDallas.h"
#include "tempsensors/TempSensorTSIC.h"
#include <memory>

#if __cplusplus >= 202002L
#include <expected>

/**
 * @enum HardwareInitError
 * @brief Specific hardware initialization error types for std::expected
 */
enum class HardwareInitError {
    RelayInitFailed,         ///< Relay initialization failed (heater, pump, or valve)
    LEDInitFailed,           ///< LED initialization failed (status, brew, or steam)
    SwitchInitFailed,        ///< Switch initialization failed (power, brew, steam, hot water)
    TemperatureSensorFailed, ///< Temperature sensor initialization failed
    MemoryAllocationFailed,  ///< Insufficient memory for component allocation
    ConfigurationError,      ///< Invalid configuration parameters
    GPIOInitFailed,          ///< GPIO pin initialization failed
    UnknownComponentType     ///< Unknown or unsupported component type
};

/**
 * @brief Convert HardwareInitError to human-readable string
 */
constexpr const char* hardwareErrorToString(HardwareInitError error) noexcept {
    switch (error) {
        case HardwareInitError::RelayInitFailed: return "Relay initialization failed";
        case HardwareInitError::LEDInitFailed: return "LED initialization failed";
        case HardwareInitError::SwitchInitFailed: return "Switch initialization failed";
        case HardwareInitError::TemperatureSensorFailed: return "Temperature sensor initialization failed";
        case HardwareInitError::MemoryAllocationFailed: return "Memory allocation failed";
        case HardwareInitError::ConfigurationError: return "Configuration error";
        case HardwareInitError::GPIOInitFailed: return "GPIO initialization failed";
        case HardwareInitError::UnknownComponentType: return "Unknown component type";
    }
    return "Unknown hardware error";
}

#endif

/**
 * @class HardwareManager
 * @brief RAII wrapper for hardware components with automatic resource management
 *
 * This class provides safe management of hardware components using RAII principles.
 * It automatically handles component initialization and cleanup, preventing memory leaks
 * and ensuring proper hardware shutdown.
 */
class HardwareManager {
    public:
        /**
         * @brief Constructor - initializes all hardware components
         */
        HardwareManager();

#if __cplusplus >= 202002L
        /**
         * @brief Create HardwareManager with modern C++23 error handling
         * @return HardwareManager instance or specific error information
         */
        static std::expected<std::unique_ptr<HardwareManager>, HardwareInitError> createModern();
        
        /**
         * @brief Initialize hardware with detailed error context
         * @return Success or specific initialization error
         */
        std::expected<void, HardwareInitError> initializeModern();
#endif

        /**
         * @brief Destructor - automatically cleans up hardware resources
         */
        ~HardwareManager() = default;

        // Disable copy constructor and assignment operator
        HardwareManager(const HardwareManager&) = delete;
        HardwareManager& operator=(const HardwareManager&) = delete;

        // Enable move constructor and assignment operator
        HardwareManager(HardwareManager&&) = default;
        HardwareManager& operator=(HardwareManager&&) = default;

        // Relay access methods
        Relay& getHeaterRelay() const {
            return *heaterRelay_;
        }
        Relay& getPumpRelay() const {
            return *pumpRelay_;
        }
        Relay& getValveRelay() const {
            return *valveRelay_;
        }

        // LED access methods
        LED* getStatusLed() const {
            return statusLed_.get();
        }
        LED* getBrewLed() const {
            return brewLed_.get();
        }
        LED* getSteamLed() const {
            return steamLed_.get();
        }

        // Switch access methods
        Switch* getPowerSwitch() const {
            return powerSwitch_.get();
        }
        Switch* getBrewSwitch() const {
            return brewSwitch_.get();
        }
        Switch* getSteamSwitch() const {
            return steamSwitch_.get();
        }
        Switch* getHotWaterSwitch() const {
            return hotWaterSwitch_.get();
        }
        Switch* getWaterTankSensor() const {
            return waterTankSensor_.get();
        }

        // Temperature sensor access
        TempSensor* getTempSensor() const {
            return tempSensor_.get();
        }

        /**
         * @brief Check if all critical hardware is initialized
         * @return true if critical components are ready
         */
        bool isInitialized() const;

        /**
         * @brief Perform safe shutdown of all hardware
         */
        void safeShutdown();

    private:
        // GPIO Pins for relays (stack allocated)
        GPIOPin heaterRelayPin_;
        GPIOPin pumpRelayPin_;
        GPIOPin valveRelayPin_;

        // Relays (managed with smart pointers)
        std::unique_ptr<Relay> heaterRelay_;
        std::unique_ptr<Relay> pumpRelay_;
        std::unique_ptr<Relay> valveRelay_;

        // GPIO Pins for LEDs (managed with smart pointers)
        std::unique_ptr<GPIOPin> statusLedPin_;
        std::unique_ptr<GPIOPin> brewLedPin_;
        std::unique_ptr<GPIOPin> steamLedPin_;

        // LEDs (managed with smart pointers)
        std::unique_ptr<LED> statusLed_;
        std::unique_ptr<LED> brewLed_;
        std::unique_ptr<LED> steamLed_;

        // Switches (managed with smart pointers)
        std::unique_ptr<Switch> powerSwitch_;
        std::unique_ptr<Switch> brewSwitch_;
        std::unique_ptr<Switch> steamSwitch_;
        std::unique_ptr<Switch> hotWaterSwitch_;
        std::unique_ptr<Switch> waterTankSensor_;

        // Temperature sensor (managed with smart pointer)
        std::unique_ptr<TempSensor> tempSensor_;

        /**
         * @brief Initialize relay components
         */
        void initializeRelays();

        /**
         * @brief Initialize LED components
         */
        void initializeLEDs();

        /**
         * @brief Initialize switch components
         */
        void initializeSwitches();

        /**
         * @brief Initialize temperature sensor
         */
        void initializeTemperatureSensor();

#if __cplusplus >= 202002L
        /**
         * @brief Modern relay initialization with std::expected
         */
        std::expected<void, HardwareInitError> initializeRelaysModern();

        /**
         * @brief Modern LED initialization with std::expected
         */
        std::expected<void, HardwareInitError> initializeLEDsModern();

        /**
         * @brief Modern switch initialization with std::expected
         */
        std::expected<void, HardwareInitError> initializeSwitchesModern();

        /**
         * @brief Modern temperature sensor initialization with std::expected
         */
        std::expected<void, HardwareInitError> initializeTemperatureSensorModern();

        /**
         * @brief Private constructor for createModern() factory method
         */
        explicit HardwareManager(bool modernInit);
#endif
};