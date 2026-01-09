/**
 * @file TempSensor.h
 *
 * @brief Interface that all temperature sensors have to implement
 */

#pragma once

#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/sensors/ISensor.h"

#include <array>
#include <cmath>
#include <numeric>

class TempSensor : public CleverCoffee::ISensor {
  public:
    /**
     * @brief Abstract temperature sensor class
     * @details This class provides temperature sensor interface. Timing is handled externally
     *          by centralized timer system. Error detection and moving average are maintained internally.
     */
    TempSensor() = default;

    /**
     * @brief Update temperature reading from sensor
     * @details Called by external timer system to update sensor reading
     * @return true if update successful, false if error occurred
     */
    bool updateTemperature() {
        // Update temperature and detect errors:
        if (sample_temperature(last_temperature_)) {
            LOGF(TRACE, "Temperature reading successful: %.1f", last_temperature_);

            // Reset error counter and error state
            bad_readings_ = 0;
            error_        = false;

            // Update moving average
            update_moving_average();
            return true;
        } else if (!error_) {
            // Increment error counter
            LOGF(DEBUG, "Error during temperature reading, incrementing error counter to %i", bad_readings_);
            bad_readings_++;
        }

        if (bad_readings_ >= max_bad_readings_ && !error_) {
            error_ = true;
            LOGF(ERROR, "Temperature sensor malfunction, %i consecutive errors", bad_readings_);
        }
        return false;
    }

    /**
     * @brief Returns the current temperature
     * @details Returns the last successfully read temperature value
     * @return Temperature in degrees Celsius
     */
    double getCurrentTemperature() const noexcept {
        return last_temperature_;
    }

    /**
     * @brief Returns the average temperature change rate
     * @return Average temperature rate
     */
    double getAverageTemperatureRate() const noexcept {
        return average_temp_rate_;
    }

    /**
     * @brief Default destructor
     */
    virtual ~TempSensor() = default;

    /**
     * @brief Returns error state of the temperature sensor
     * @return true if the sensor is in error, false otherwise
     */
    [[nodiscard]] bool hasError() const noexcept {
        return error_;
    }

    /**
     * @brief Validate temperature reading
     * @param temp Temperature value to validate
     * @return true if temperature is within valid range
     */
    static constexpr bool isValidTemperature(double temp) noexcept {
        return temp >= -50.0 && temp <= 150.0; // Practical range for coffee machines
    }

    // ============= ISensor Interface Implementation =============
    
    /**
     * @brief Start an async sensor read (ISensor interface)
     * @details For temperature sensors, this initiates a read cycle
     */
    void startRead() noexcept override {
        read_in_progress_ = true;
        read_start_time_ = millis();
    }
    
    /**
     * @brief Try to get the sensor reading result (ISensor interface)
     * @return Expected<double, Error> containing temperature or error
     */
    CleverCoffee::Expected<double, CleverCoffee::Error> tryGetValue() noexcept override {
        using namespace CleverCoffee;
        
        if (!read_in_progress_) {
            return Error(ErrorCode::SENSOR_NOT_READY, "Read not started");
        }
        
        // Check for timeout (400ms is standard temp sensor read interval)
        constexpr uint32_t timeout_ms = 1000;
        if (millis() - read_start_time_ > timeout_ms) {
            read_in_progress_ = false;
            return Error(ErrorCode::SENSOR_TIMEOUT, "Temperature read timeout");
        }
        
        // Try to update temperature
        double temp_value{};
        if (sample_temperature(temp_value)) {
            read_in_progress_ = false;
            last_temperature_ = temp_value;
            
            // Reset error counter and error state
            bad_readings_ = 0;
            error_ = false;
            
            // Update moving average
            update_moving_average();
            
            LOGF(TRACE, "Temperature reading successful: %.1f", last_temperature_);
            return last_temperature_;
        }
        
        // Read failed - could be still in progress or actual error
        // For Dallas sensors, might need more time, so return SENSOR_NOT_READY
        return Error(ErrorCode::SENSOR_NOT_READY, "Temperature not ready");
    }
    
    /**
     * @brief Get the sensor type name (ISensor interface)
     * @return Human-readable sensor type
     */
    const char* getSensorType() const noexcept override {
        return "TempSensor";
    }
    
    /**
     * @brief Check if sensor is connected (ISensor interface)
     * @return true if sensor is operational
     */
    bool isConnected() const noexcept override {
        return !error_;
    }

  protected:
    /**
     * @brief Samples the current temperature from the sensor
     * @details Requests sampling from attached sensor and returns reading. This method is purely virtual and must be
     *          implemented by every child class. The argument is passed by reference and is updated if the return value
     *          of the function is true.
     * @return Boolean indicating whether the reading has been successful
     */
    virtual bool sample_temperature(double& temperature) const = 0;

  private:
    double last_temperature_{};
    int    bad_readings_{0};
    int    max_bad_readings_{10};
    bool   error_{false};
    
    // ISensor async read tracking
    bool     read_in_progress_{false};
    uint32_t read_start_time_{0};

    /**
     * @brief FIR moving average filter for software brew detection
     */
    void update_moving_average() {
        if (valueIndex < 0) {
            for (int index = 0; index < numValues; index++) {
                tempValues[index]      = last_temperature_;
                timeValues[index]      = 0;
                tempChangeRates[index] = 0;
            }
        }

        timeValues[valueIndex] = millis();
        tempValues[valueIndex] = last_temperature_;

        // local change rate of temperature
        double tempChangeRate = 0;

        if (valueIndex == numValues - 1) {
            tempChangeRate =
                (tempValues[numValues - 1] - tempValues[0]) / (timeValues[numValues - 1] - timeValues[0]) * 10000;
        } else {
            tempChangeRate = (tempValues[valueIndex] - tempValues[valueIndex + 1]) /
                             (timeValues[valueIndex] - timeValues[valueIndex + 1]) * 10000;
        }

        tempChangeRates[valueIndex] = tempChangeRate;

        const double totalTempChangeRateSum =
            std::accumulate(std::begin(tempChangeRates), std::end(tempChangeRates), 0, std::plus<>());
        average_temp_rate_ = totalTempChangeRateSum / numValues * 100;

        if (valueIndex >= numValues - 1) {
            // ...wrap around to the beginning:
            valueIndex = 0;
        } else {
            valueIndex++;
        }
    }

    // Moving average:
    double                               average_temp_rate_{};
    constexpr static size_t              numValues = 15;
    std::array<double, numValues>        tempValues{};   // array of temp values
    std::array<unsigned long, numValues> timeValues{};   // array of time values
    std::array<double, numValues>        tempChangeRates{};
    int                                  valueIndex{-1}; // the index of the current value
};
