#pragma once

/**
 * @file MockSensorManager.h
 * @brief Simple mock for testing temperature and sensor logic
 */

class MockSensorManager {
private:
    double currentTemperature_        = 20.0;
    float currentPressure_            = 0.0f;
    bool hasTemperatureError_         = false;
    bool hasScaleError_               = false;

public:
    MockSensorManager() = default;

    // Temperature simulation
    void setTemperature(double temp) {
        currentTemperature_ = temp;
    }

    double getCurrentTemperature() const {
        return currentTemperature_;
    }

    void setTemperatureError(bool hasError) {
        hasTemperatureError_ = hasError;
    }

    bool hasTemperatureError() const {
        return hasTemperatureError_;
    }

    // Pressure simulation
    void setPressure(float pressure) {
        currentPressure_ = pressure;
    }

    float getCurrentPressure() const {
        return currentPressure_;
    }

    float getFilteredPressure() const {
        return currentPressure_;
    }

    // Error checks
    bool hasSensorError() const {
        return hasTemperatureError_ || hasScaleError_;
    }

    bool hasScaleError() const {
        return hasScaleError_;
    }

    void setScaleError(bool hasError) {
        hasScaleError_ = hasError;
    }
};
