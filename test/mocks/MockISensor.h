/**
 * @file MockISensor.h
 * @brief Mock implementation of ISensor interface for testing
 * 
 * Provides a reusable mock sensor that implements the ISensor interface
 * for use in unit tests.
 */

#pragma once

#include "clevercoffee/sensors/ISensor.h"
#include "clevercoffee/errors/ErrorCodes.h"
#include <memory>

using CleverCoffee::ISensor;
using CleverCoffee::Expected;
using CleverCoffee::Error;
using CleverCoffee::ErrorCode;

/**
 * @brief Mock ISensor implementation for testing
 * 
 * This class provides a simple mock implementation of ISensor that can be used
 * in tests without requiring actual hardware.
 * 
 * Usage:
 * @code
 * MockISensor mockTemp(95.5);
 * SensorCoordinator coord(&mockTemp, nullptr, nullptr);
 * 
 * coord.update();
 * EXPECT_DOUBLE_EQ(95.5, coord.getTemperature());
 * @endcode
 */
class MockISensor : public ISensor {
public:
    /**
     * @brief Constructor
     * @param initialValue Initial sensor value
     * @param connected Whether sensor is connected (default: true)
     */
    explicit MockISensor(double initialValue = 0.0, bool connected = true)
        : value_(initialValue)
        , connected_(connected)
        , readStarted_(false)
        , shouldFail_(false)
        , failError_(Error(ErrorCode::UNKNOWN_ERROR, "Unknown error")) {}
    
    /**
     * @brief Start a sensor read operation
     */
    void startRead() noexcept override {
        readStarted_ = true;
    }
    
    /**
     * @brief Try to get the sensor value
     * @return Expected value or error
     */
    Expected<double, Error> tryGetValue() noexcept override {
        if (!readStarted_) {
            return Expected<double, Error>(Error(ErrorCode::SENSOR_NOT_READY, "Sensor not ready"));
        }
        
        if (shouldFail_) {
            readStarted_ = false;
            return Expected<double, Error>(failError_);
        }
        
        readStarted_ = false;
        return Expected<double, Error>(value_);
    }
    
    /**
     * @brief Check if sensor is connected
     */
    bool isConnected() const noexcept override {
        return connected_;
    }
    
    /**
     * @brief Get sensor type string
     */
    const char* getSensorType() const noexcept override {
        return "MockISensor";
    }
    
    // Test control methods
    void setValue(double value) { value_ = value; }
    void setConnected(bool connected) { connected_ = connected; }
    void setShouldFail(bool fail, Error error = Error(ErrorCode::UNKNOWN_ERROR, "Unknown error")) {
        shouldFail_ = fail;
        failError_ = error;
    }
    double getValue() const { return value_; }
    
private:
    double value_;
    bool connected_;
    bool readStarted_;
    bool shouldFail_;
    Error failError_;
};
