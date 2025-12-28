/**
 * @file SensorCoordinator.cpp
 * @brief Implementation of SensorCoordinator
 */

#include "clevercoffee/coordinators/SensorCoordinator.h"

#include "clevercoffee/Logger.h"

namespace CleverCoffee {

SensorCoordinator::SensorCoordinator(ISensor* tempSensor, ISensor* scaleSensor) noexcept
    : tempSensor_(tempSensor), scaleSensor_(scaleSensor) {
    
    if (tempSensor_) {
        LOG(INFO, "SensorCoordinator initialized with temperature sensor");
    }
    if (scaleSensor_) {
        LOG(INFO, "SensorCoordinator initialized with scale sensor");
    }
}

void SensorCoordinator::update() noexcept {
    updateTemperature();
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

}  // namespace CleverCoffee
