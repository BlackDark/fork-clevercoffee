/**
 * @file SensorCoordinator.h
 * @brief Coordinates all sensors: polling, caching, timeout management
 */

#pragma once

#include "clevercoffee/sensors/ISensor.h"
#include "clevercoffee/errors/ErrorCodes.h"

#include <atomic>

// Forward declarations
class Switch;

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
     * @param waterTankSensor Water tank level sensor (can be nullptr)
     */
    SensorCoordinator(ISensor* tempSensor = nullptr, ISensor* scaleSensor = nullptr, Switch* waterTankSensor = nullptr) noexcept;
    
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
    
    /**
     * @brief Set water tank sensor (late injection)
     * @param sensor Water tank level sensor (can be nullptr)
     */
    void setWaterTankSensor(Switch* sensor) noexcept {
        waterTankSensor_ = sensor;
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
    
    // === Pressure Sensor ===
    
    /**
     * @brief Get cached raw pressure value
     * @return Last successfully read pressure in bar
     */
    [[nodiscard]] float getPressure() const noexcept {
        return cachedPressure_;
    }
    
    /**
     * @brief Get filtered pressure value
     * @return Filtered pressure in bar (low-pass filtered)
     */
    [[nodiscard]] float getFilteredPressure() const noexcept {
        return cachedPressureFiltered_;
    }
    
    // === Water Tank Sensor ===
    
    /**
     * @brief Check if water tank is full
     * @return true if water tank has water
     */
    [[nodiscard]] bool isWaterTankFull() const noexcept {
        return waterTankFull_.load(std::memory_order_relaxed);
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
    Switch*  waterTankSensor_ = nullptr;
    
    // Cached values
    double cachedTemperature_ = 0.0;
    double cachedWeight_ = 0.0;
    float  cachedPressure_ = 0.0f;
    float  cachedPressureFiltered_ = 0.0f;
    
    // Error tracking (atomic for thread safety)
    std::atomic<bool> tempSensorError_{false};
    std::atomic<bool> scaleSensorError_{false};
    std::atomic<bool> waterTankFull_{true};  // Assume full initially
    
    // Update timing
    unsigned long lastTempUpdate_ = 0;
    unsigned long lastScaleUpdate_ = 0;
    unsigned long lastPressureUpdate_ = 0;
    unsigned long lastWaterTankUpdate_ = 0;
    
    // Update intervals
    static constexpr unsigned long TEMP_UPDATE_INTERVAL_MS = 400;
    static constexpr unsigned long SCALE_UPDATE_INTERVAL_MS = 100;
    static constexpr unsigned long PRESSURE_UPDATE_INTERVAL_MS = 50;
    static constexpr unsigned long WATER_TANK_UPDATE_INTERVAL_MS = 200;
    
    // Pressure filter state (low-pass filter)
    float inX_ = 0.0f;
    float inY_ = 0.0f;
    float inOld_ = 0.0f;
    float inSum_ = 0.0f;
    
    // Water tank debouncing
    int waterTankConsecutiveReads_ = 0;
    static constexpr int WATER_TANK_READS_NEEDED = 3;
    
    // Legacy coordination flags (for backward compatibility)
    std::atomic<bool> temperatureUpdateRunning_{false};
    std::atomic<bool> scaleUpdateRunning_{false};
    
    // Private update methods
    void updateTemperature() noexcept;
    void updateScale() noexcept;
    void updatePressure() noexcept;
    void updateWaterTank() noexcept;
    
    // Helper methods
    float filterPressureValue(float input) noexcept;
    float measurePressure() noexcept;
};

}  // namespace CleverCoffee
