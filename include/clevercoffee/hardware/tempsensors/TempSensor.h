/**
 * @file TempSensor.h
 *
 * @brief Interface that all temperature sensors have to implement
 */

#pragma once

#include "clevercoffee/GlobalState.h"
#include "clevercoffee/Logger.h"

#include <array>
#include <cmath>
#include <numeric>

class TempSensor {
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
