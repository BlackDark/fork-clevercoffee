/**
 * @file SensorCoordinator.h
 * @brief Coordinates all sensors: polling, caching, timeout management
 */

#pragma once

#include "clevercoffee/sensors/ISensor.h"
#include "clevercoffee/errors/ErrorCodes.h"

#include <atomic>

namespace CleverCoffee {

/**
 * @class SensorCoordinator
 * @brief Manages all sensor reads with caching and timeout protection
 * 
 * Responsibilities:
 * - Poll sensors at regular intervals
 * - Cache sensor values for state machine (never blocks)
 * - Track sensor errors
 * - Enforce timeouts on all sensor reads
 * 
 * Called from main loop: coordinator.update()
 * Used by state machine: context.getCurrentTemperature() returns cached value
 */
class SensorCoordinator {
public:
    /**
     * @brief Constructor
     * @param tempSensor Temperature sensor implementing ISensor (can be nullptr)
     * @param scaleSensor Scale sensor implementing ISensor (can be nullptr)
     */
    SensorCoordinator(ISensor* tempSensor = nullptr, ISensor* scaleSensor = nullptr) noexcept;
    
    /**
     * @brief Update all sensor readings
     * 
     * Call this from main loop periodically.
     * Non-blocking - always returns immediately.
     */
    void update() noexcept;
    
    /**
     * @brief Set temperature sensor (late injection)
     * @param sensor Temperature sensor implementing ISensor (can be nullptr)
     */
    void setTemperatureSensor(ISensor* sensor) noexcept {
        tempSensor_ = sensor;
    }
    
    /**
     * @brief Set scale sensor (late injection)
     * @param sensor Scale sensor implementing ISensor (can be nullptr)
     */
    void setScaleSensor(ISensor* sensor) noexcept {
        scaleSensor_ = sensor;
    }
    
    // === Legacy coordination interface (for backward compatibility) ===
    // These methods are for coordinating with the old SensorManager
    // Will be removed in Phase 6 cleanup
    
    /**
     * @brief Start temperature update operation (legacy)
     * @deprecated Use update() instead
     */
    void startTemperatureUpdate() noexcept {
        temperatureUpdateRunning_ = true;
    }
    
    /**
     * @brief Stop temperature update operation (legacy)
     * @deprecated Use update() instead
     */
    void stopTemperatureUpdate() noexcept {
        temperatureUpdateRunning_ = false;
    }
    
    /**
     * @brief Check if temperature update is running (legacy)
     * @deprecated Use update() instead
     */
    bool isTemperatureUpdateRunning() const noexcept {
        return temperatureUpdateRunning_;
    }
    
    /**
     * @brief Start scale update operation (legacy)
     * @deprecated Use update() instead
     */
    void startScaleUpdate() noexcept {
        scaleUpdateRunning_ = true;
    }
    
    /**
     * @brief Stop scale update operation (legacy)
     * @deprecated Use update() instead
     */
    void stopScaleUpdate() noexcept {
        scaleUpdateRunning_ = false;
    }
    
    /**
     * @brief Check if scale update is running (legacy)
     * @deprecated Use update() instead
     */
    bool isScaleUpdateRunning() const noexcept {
        return scaleUpdateRunning_;
    }
    
    // === Temperature Sensor ===
    
    /**
     * @brief Get cached temperature value
     * @return Last successfully read temperature in Celsius
     */
    [[nodiscard]] double getTemperature() const noexcept {
        return cachedTemperature_;
    }
    
    /**
     * @brief Check if temperature sensor has error
     * @return true if temperature sensor encountered error
     */
    [[nodiscard]] bool hasTemperatureSensorError() const noexcept {
        return tempSensorError_.load(std::memory_order_relaxed);
    }
    
    // === Scale Sensor ===
    
    /**
     * @brief Get cached weight value
     * @return Last successfully read weight in grams
     */
    [[nodiscard]] double getWeight() const noexcept {
        return cachedWeight_;
    }
    
    /**
     * @brief Check if scale sensor has error
     * @return true if scale sensor encountered error
     */
    [[nodiscard]] bool hasScaleSensorError() const noexcept {
        return scaleSensorError_.load(std::memory_order_relaxed);
    }
    
    // === General ===
    
    /**
     * @brief Check if any sensor has error
     * @return true if any enabled sensor has error
     */
    [[nodiscard]] bool hasSensorError() const noexcept {
        return hasTemperatureSensorError() || hasScaleSensorError();
    }
    
private:
    // Sensor references (not owned)
    ISensor* tempSensor_ = nullptr;
    ISensor* scaleSensor_ = nullptr;
    
    // Cached values
    double cachedTemperature_ = 0.0;
    double cachedWeight_ = 0.0;
    
    // Error tracking (atomic for thread safety)
    std::atomic<bool> tempSensorError_{false};
    std::atomic<bool> scaleSensorError_{false};
    
    // Update timing
    unsigned long lastTempUpdate_ = 0;
    unsigned long lastScaleUpdate_ = 0;
    
    // Update intervals
    static constexpr unsigned long TEMP_UPDATE_INTERVAL_MS = 400;
    static constexpr unsigned long SCALE_UPDATE_INTERVAL_MS = 100;
    
    // Legacy coordination flags (for backward compatibility)
    std::atomic<bool> temperatureUpdateRunning_{false};
    std::atomic<bool> scaleUpdateRunning_{false};
    
    // Private update methods
    void updateTemperature() noexcept;
    void updateScale() noexcept;
};

}  // namespace CleverCoffee
