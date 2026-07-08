/**
 * @file SensorCoordinator.h
 * @brief Coordinates all sensors: polling, caching, timeout management
 */

#pragma once

#include "clevercoffee/errors/ErrorCodes.h"
#include "clevercoffee/sensors/ISensor.h"

#include <atomic>
#include <memory>

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
     */
    SensorCoordinator(ISensor* tempSensor = nullptr, ISensor* scaleSensor = nullptr) noexcept;

    ~SensorCoordinator();

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
     * @brief Take ownership of the water tank level sensor
     * @param sensor Water tank sensor (nullptr clears the owned sensor)
     */
    void setWaterTankSensor(std::unique_ptr<Switch> sensor) noexcept;

    /**
     * @brief Get the owned water tank sensor (non-owning)
     */
    [[nodiscard]] Switch* getWaterTankSensor() const noexcept {
        return waterTankSensor_.get();
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

    // === Brew Weight Tracking ===

    /**
     * @brief Start tracking brew weight (call when brew starts)
     *
     * Captures current weight as pre-brew weight and begins tracking brew weight delta.
     */
    void startBrewWeightTracking() noexcept;

    /**
     * @brief Stop tracking brew weight (call when brew ends)
     *
     * Resets brew weight tracking state.
     */
    void stopBrewWeightTracking() noexcept;

    /**
     * @brief Get current brew weight (weight extracted during brew)
     * @return Current brew weight in grams (weight - preBrewWeight)
     */
    [[nodiscard]] double getBrewWeight() const noexcept {
        return cachedBrewWeight_;
    }

    /**
     * @brief Get pre-brew weight (weight before brew started)
     * @return Pre-brew weight in grams
     */
    [[nodiscard]] double getPreBrewWeight() const noexcept {
        return preBrewWeight_;
    }

    /**
     * @brief Check if brew weight tracking is active
     * @return true if currently tracking brew weight
     */
    [[nodiscard]] bool isBrewWeightTrackingActive() const noexcept {
        return brewWeightTrackingActive_;
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

    // === Scale Operating Modes ===

    /**
     * @brief Set scale tare mode
     *
     * When enabled, the scale will perform a tare operation (reset to zero).
     *
     * @param enabled true to enable tare mode, false to disable
     */
    void setScaleTareMode(bool enabled) noexcept {
        scaleTareMode_ = enabled;
    }

    /**
     * @brief Check if scale tare mode is enabled
     * @return true if tare mode is active
     */
    [[nodiscard]] bool isScaleTareMode() const noexcept {
        return scaleTareMode_;
    }

    /**
     * @brief Set scale calibration mode
     *
     * When enabled, the scale will perform a calibration operation.
     *
     * @param enabled true to enable calibration mode, false to disable
     */
    void setScaleCalibrationMode(bool enabled) noexcept {
        scaleCalibrationMode_ = enabled;
    }

    /**
     * @brief Check if scale calibration mode is enabled
     * @return true if calibration mode is active
     */
    [[nodiscard]] bool isScaleCalibrationMode() const noexcept {
        return scaleCalibrationMode_;
    }

  private:
    // Sensor references (not owned)
    ISensor*                tempSensor_  = nullptr;
    ISensor*                scaleSensor_ = nullptr;
    std::unique_ptr<Switch> waterTankSensor_;

    // Cached values
    double cachedTemperature_      = 0.0;
    double cachedWeight_           = 0.0;
    float  cachedPressure_         = 0.0f;
    float  cachedPressureFiltered_ = 0.0f;

    // Brew weight tracking
    double        cachedBrewWeight_         = 0.0;
    double        preBrewWeight_            = 0.0;
    bool          brewWeightTrackingActive_ = false;
    bool          preBrewWeightPending_     = false;
    bool          autoTareInProgress_       = false;
    unsigned long autoTareStartTime_        = 0;

    static constexpr float         AUTO_TARE_WEIGHT_THRESHOLD_G = 0.2f;
    static constexpr unsigned long AUTO_TARE_TIMEOUT_MS         = 3000;

    // Error tracking (atomic for thread safety)
    std::atomic<bool> tempSensorError_{false};
    std::atomic<bool> scaleSensorError_{false};
    std::atomic<bool> waterTankFull_{true}; // Assume full initially

    // Update timing
    unsigned long lastTempUpdate_      = 0;
    unsigned long lastScaleUpdate_     = 0;
    unsigned long lastPressureUpdate_  = 0;
    unsigned long lastWaterTankUpdate_ = 0;

    // Update intervals
    static constexpr unsigned long TEMP_UPDATE_INTERVAL_MS       = 400;
    static constexpr unsigned long SCALE_UPDATE_INTERVAL_MS      = 100;
    static constexpr unsigned long PRESSURE_UPDATE_INTERVAL_MS   = 50;
    static constexpr unsigned long WATER_TANK_UPDATE_INTERVAL_MS = 200;

    // Pressure filter state (low-pass filter)
    float inX_   = 0.0f;
    float inY_   = 0.0f;
    float inOld_ = 0.0f;
    float inSum_ = 0.0f;

    // Scale operating modes
    std::atomic<bool> scaleTareMode_{false};        ///< Scale tare (reset to zero) mode
    std::atomic<bool> scaleCalibrationMode_{false}; ///< Scale calibration mode

                                                    // Private update methods
    void updateTemperature() noexcept;
    void updateScale() noexcept;
    void updatePressure() noexcept;
    void updateWaterTank() noexcept;

    // Helper methods
    float filterPressureValue(float input) noexcept;
    float measurePressure() noexcept;
    void  tryCapturePreBrewWeight() noexcept;
    void  maybeStartAutoTare() noexcept;
};

} // namespace CleverCoffee
