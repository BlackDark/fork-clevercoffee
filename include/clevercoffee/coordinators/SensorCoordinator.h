#pragma once

#include <atomic>

namespace CleverCoffee {

/**
 * @brief Coordinates sensor update operations
 *
 * This class provides thread-safe coordination for sensor operations, replacing
 * the previous global g_state.coordination flags. It prevents concurrent sensor
 * updates and manages the state of different sensor types.
 *
 * The coordinator uses atomic operations to ensure thread safety when multiple
 * contexts may access sensor state simultaneously.
 *
 * @note This class is typically accessed through SystemContext::sensorCoordinator()
 *
 * Example usage:
 * @code
 * SystemContext& ctx = ...;
 * ctx.sensorCoordinator().startTemperatureUpdate();
 * // ... perform temperature reading ...
 * ctx.sensorCoordinator().stopTemperatureUpdate();
 *
 * if (ctx.sensorCoordinator().isTemperatureUpdateRunning()) {
 *     // Handle concurrent access
 * }
 * @endcode
 */
class SensorCoordinator {
public:
    SensorCoordinator() = default;

    /**
     * @brief Start temperature update operation
     *
     * Sets the flag indicating that a temperature update is in progress.
     * This prevents other parts of the system from initiating concurrent reads.
     *
     * @post isTemperatureUpdateRunning() returns true
     */
    void startTemperatureUpdate() noexcept {
        temperatureUpdateRunning_ = true;
    }

    /**
     * @brief Stop temperature update operation
     *
     * Clears the temperature update flag, allowing new temperature operations.
     *
     * @post isTemperatureUpdateRunning() returns false
     */
    void stopTemperatureUpdate() noexcept {
        temperatureUpdateRunning_ = false;
    }

    /**
     * @brief Check if temperature update is running
     *
     * @return true if a temperature update is currently in progress, false otherwise
     */
    bool isTemperatureUpdateRunning() const noexcept {
        return temperatureUpdateRunning_;
    }

    /**
     * @brief Start scale update operation
     *
     * Sets the flag indicating that a scale weight update is in progress.
     * This prevents other parts of the system from initiating concurrent reads.
     *
     * @post isScaleUpdateRunning() returns true
     */
    void startScaleUpdate() noexcept {
        scaleUpdateRunning_ = true;
    }

    /**
     * @brief Stop scale update operation
     *
     * Clears the scale update flag, allowing new scale operations.
     *
     * @post isScaleUpdateRunning() returns false
     */
    void stopScaleUpdate() noexcept {
        scaleUpdateRunning_ = false;
    }

    /**
     * @brief Check if scale update is running
     *
     * @return true if a scale update is currently in progress, false otherwise
     */
    bool isScaleUpdateRunning() const noexcept {
        return scaleUpdateRunning_;
    }

private:
    std::atomic<bool> temperatureUpdateRunning_{false}; ///< Flag for temperature update state
    std::atomic<bool> scaleUpdateRunning_{false};       ///< Flag for scale update state
};

} // namespace CleverCoffee
