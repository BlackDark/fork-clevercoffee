/**
 * @file SensorManager.h
 * @brief RAII wrapper for sensor management
 */

#pragma once

#include "../hardware/Switch.h"
#include "../hardware/tempsensors/TempSensor.h"
#include <Arduino.h>
#include <functional>
#include <memory>
#include <vector>
#include <array>
#include <algorithm>
#include <numeric>

// Forward declarations
class Scale;

/**
 * @class SensorManager
 * @brief RAII wrapper for unified sensor management
 *
 * This class provides safe management of all sensors using RAII principles.
 * It encapsulates temperature sensors, pressure sensors, water tank sensors,
 * and scale management with proper error handling and unified interface.
 */
class SensorManager {
    public:
        /**
         * @brief Constructor - initializes sensor manager
         */
        SensorManager();

        /**
         * @brief Destructor - automatically cleans up sensor resources
         */
        ~SensorManager() = default;

        // Disable copy constructor and assignment operator
        SensorManager(const SensorManager&) = delete;
        SensorManager& operator=(const SensorManager&) = delete;

        // Enable move constructor and assignment operator
        SensorManager(SensorManager&&) = default;
        SensorManager& operator=(SensorManager&&) = default;

        /**
         * @brief Initialize all sensors
         * @param tempSensor Temperature sensor instance from HardwareManager
         * @param waterTankSensor Water tank sensor instance from HardwareManager
         * @return true if initialization successful
         */
        bool initialize(TempSensor* tempSensor, Switch* waterTankSensor);

        /**
         * @brief Update all sensor readings
         */
        void update();

        /**
         * @brief Check if sensors are ready
         * @return true if all enabled sensors are functional
         */
        bool areSensorsReady() const noexcept;

        /**
         * @brief Check if any sensor has an error
         * @return true if any enabled sensor has an error
         */
        bool hasSensorError() const noexcept;

        // Temperature sensor interface
        /**
         * @brief Get current temperature reading
         * @return Temperature in Celsius
         */
        double getCurrentTemperature() const noexcept;

        /**
         * @brief Check if temperature sensor has an error
         * @return true if temperature sensor has an error
         */
        bool hasTemperatureError() const noexcept;

        // Water tank sensor interface
        /**
         * @brief Check if water tank is full
         * @return true if water tank has water
         */
        bool isWaterTankFull() const noexcept;

        /**
         * @brief Update water tank sensor reading
         */
        void updateWaterTankSensor();

        // Pressure sensor interface
        /**
         * @brief Get current pressure reading
         * @return Raw pressure value
         */
        float getCurrentPressure() const noexcept;

        /**
         * @brief Get filtered pressure reading
         * @return Filtered pressure value
         */
        float getFilteredPressure() const noexcept;

        /**
         * @brief Update pressure sensor reading
         */
        void updatePressureSensor();

        // Scale interface
        /**
         * @brief Initialize scale sensor
         * @return true if scale initialization successful
         */
        bool initializeScale();

        /**
         * @brief Update scale reading
         */
        void updateScale();

        /**
         * @brief Get current scale reading
         * @return Current weight reading
         */
        float getCurrentWeight() const noexcept;

        /**
         * @brief Get current brew weight
         * @return Current brew weight reading
         */
        float getCurrentBrewWeight() const noexcept;

        /**
         * @brief Check if scale has an error
         * @return true if scale has an error
         */
        bool hasScaleError() const noexcept;

        /**
         * @brief Get scale instance for direct access
         * @return Pointer to scale instance (may be null)
         */
        Scale* getScale() const noexcept;

        // Modern C++23 sensor processing methods
        /**
         * @brief Process multiple sensor readings using std::ranges
         * @param readings Vector of sensor readings to process
         * @return Filtered average of valid readings
         */
        double processTemperatureReadings(const std::vector<double>& readings) const;

        /**
         * @brief Process pressure readings with ranges-based filtering
         * @param readings Array of pressure readings
         * @return Smoothed pressure value from valid readings
         */
        float processPressureReadings(const std::array<float, 10>& readings) const;

    private:
        // Sensor references (not owned by this class)
        TempSensor* tempSensor_;
        Switch* waterTankSensor_;

        // Sensor state
        bool sensorsInitialized_;
        double temperature_;

        // Water tank state
        bool waterTankFull_;
        int waterTankCheckConsecutiveReads_;
        static constexpr int waterTankCountsNeeded_ = 3;

        // Pressure sensor state
        float inputPressure_;
        float inputPressureFilter_;
        float inX_, inY_, inOld_, inSum_; // Filter variables
        unsigned long previousMillisPressure_;
        static constexpr unsigned long intervalPressure_ = 100;

        // Scale state (these are global variables, not instance variables)

        // Modern C++23 sensor processing state
        static constexpr size_t PRESSURE_HISTORY_SIZE = 10;
        static constexpr size_t TEMP_HISTORY_SIZE = 5;
        mutable std::array<float, PRESSURE_HISTORY_SIZE> pressureHistory_;
        mutable std::array<double, TEMP_HISTORY_SIZE> tempHistory_;
        mutable size_t pressureHistoryIndex_;
        mutable size_t tempHistoryIndex_;

        /**
         * @brief Initialize temperature sensor
         * @return true if successful
         */
        bool initializeTemperatureSensor();

        /**
         * @brief Initialize water tank sensor
         * @return true if successful
         */
        bool initializeWaterTankSensor();

        /**
         * @brief Initialize pressure sensor
         * @return true if successful
         */
        bool initializePressureSensor();

        /**
         * @brief Filter pressure value using moving average
         * @param input Raw pressure input
         * @return Filtered pressure value
         */
        float filterPressureValue(float input);

        /**
         * @brief Read raw pressure from hardware
         * @return Raw pressure reading
         */
        float measurePressure();
};
