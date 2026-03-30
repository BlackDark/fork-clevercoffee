/**
 * @file SensorCoordinator.cpp
 * @brief Implementation of SensorCoordinator
 */

#include "clevercoffee/coordinators/SensorCoordinator.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/hardware/Switch.h"
#include "clevercoffee/hardware/pressureSensor.h"

namespace CleverCoffee {

SensorCoordinator::SensorCoordinator(ISensor* tempSensor, ISensor* scaleSensor, Switch* waterTankSensor) noexcept
    : tempSensor_(tempSensor), scaleSensor_(scaleSensor), waterTankSensor_(waterTankSensor) {
    if (tempSensor_) {
        LOG(INFO, "SensorCoordinator initialized with temperature sensor");
    }
    if (scaleSensor_) {
        LOG(INFO, "SensorCoordinator initialized with scale sensor");
    }
    if (waterTankSensor_) {
        LOG(INFO, "SensorCoordinator initialized with water tank sensor");
    }
}

void SensorCoordinator::update() noexcept {
    updateTemperature();
    updatePressure();
    updateWaterTank();
    updateScale();
}

void SensorCoordinator::updateTemperature() noexcept {
    if (!tempSensor_) {
        return;
    }

    unsigned long now = millis();

    // Time to start a new read?
    if (now - lastTempUpdate_ >= TEMP_UPDATE_INTERVAL_MS) {
        tempSensor_->startRead();
        lastTempUpdate_ = now;
    }

    // Try to get result
    auto result = tempSensor_->tryGetValue();
    if (result) {
        // Success
        cachedTemperature_ = result.value();
        tempSensorError_.store(false, std::memory_order_relaxed);
    } else {
        // Check error type
        auto error = result.error();

        // NOT_READY is expected while reading - not an error
        if (error.code() != ErrorCode::SENSOR_NOT_READY) {
            // Real error
            tempSensorError_.store(true, std::memory_order_relaxed);
            LOGF(ERROR, "Temperature sensor error: %s", error.message());
        }
    }
}

void SensorCoordinator::updateScale() noexcept {
    if (!scaleSensor_) {
        return;
    }

    // Scale update is called more frequently, check for new data
    auto result = scaleSensor_->tryGetValue();
    if (result) {
        // Success
        cachedWeight_ = result.value();
        scaleSensorError_.store(false, std::memory_order_relaxed);

        // Update brew weight if tracking is active
        if (brewWeightTrackingActive_) {
            cachedBrewWeight_ = cachedWeight_ - preBrewWeight_;
        }
    } else {
        // Check error type
        auto error = result.error();

        // NOT_READY is expected - scale might not have new data yet
        if (error.code() != ErrorCode::SENSOR_NOT_READY) {
            // Real error
            scaleSensorError_.store(true, std::memory_order_relaxed);
            LOGF(ERROR, "Scale sensor error: %s", error.message());
        }
    }
}

void SensorCoordinator::updatePressure() noexcept {
    if (!Config::getInstance().hardwareSensorsPressureEnabled.get()) {
        return;
    }

    unsigned long now = millis();

    // Time to update pressure?
    if (now - lastPressureUpdate_ < PRESSURE_UPDATE_INTERVAL_MS) {
        return;
    }
    lastPressureUpdate_ = now;

    // Read pressure
    cachedPressure_ = measurePressure();
    // Use internal filter method instead of helper function
    cachedPressureFiltered_ = filterPressureValue(cachedPressure_);
}

void SensorCoordinator::updateWaterTank() noexcept {
    if (!Config::getInstance().hardwareSensorsWatertankEnabled.get() || !waterTankSensor_) {
        return;
    }

    unsigned long now = millis();

    // Time to update water tank?
    if (now - lastWaterTankUpdate_ < WATER_TANK_UPDATE_INTERVAL_MS) {
        return;
    }
    lastWaterTankUpdate_ = now;

    bool currentWaterTankState = waterTankFull_.load(std::memory_order_relaxed);
    bool isWaterDetected       = waterTankSensor_->isPressed();

    if (isWaterDetected && !currentWaterTankState) {
        waterTankFull_.store(true, std::memory_order_relaxed);
        waterTankConsecutiveReads_ = 0;
        LOG(INFO, "Water tank is full");
    } else if (!isWaterDetected && currentWaterTankState) {
        waterTankFull_.store(false, std::memory_order_relaxed);
        waterTankConsecutiveReads_ = 0;
        LOG(INFO, "Water tank is empty");
    }
}

float SensorCoordinator::filterPressureValue(float input) noexcept {
    // Low-pass filter implementation
    // y(n) = 0.3 * x(n) + 0.7 * y(n-1)
    inX_   = input * 0.3f;
    inY_   = inOld_ * 0.7f;
    inSum_ = inX_ + inY_;
    inOld_ = inSum_;
    return inSum_;
}

float SensorCoordinator::measurePressure() noexcept {
    // Use the global measurePressure function from pressureSensor.h
    return ::measurePressure();
}

void SensorCoordinator::startBrewWeightTracking() noexcept {
    LOG(INFO, "SensorCoordinator: Starting brew weight tracking");
    preBrewWeight_            = cachedWeight_;
    cachedBrewWeight_         = 0.0;
    brewWeightTrackingActive_ = true;
    LOGF(DEBUG, "Pre-brew weight captured: %.2f g", preBrewWeight_);
}

void SensorCoordinator::stopBrewWeightTracking() noexcept {
    LOG(INFO, "SensorCoordinator: Stopping brew weight tracking");
    brewWeightTrackingActive_ = false;
    cachedBrewWeight_         = 0.0;
    preBrewWeight_            = 0.0;
}

} // namespace CleverCoffee
