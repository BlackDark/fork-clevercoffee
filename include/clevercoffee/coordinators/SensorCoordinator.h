#pragma once

#include <atomic>

namespace CleverCoffee {

/**
 * @brief Coordinates sensor update operations
 *
 * Replaces g_state.coordination flags with thread-safe coordinator.
 * Prevents concurrent sensor updates and manages update state.
 */
class SensorCoordinator {
public:
    SensorCoordinator() = default;

    /**
     * @brief Start temperature update operation
     */
    void startTemperatureUpdate() noexcept {
        temperatureUpdateRunning_ = true;
    }

    /**
     * @brief Stop temperature update operation
     */
    void stopTemperatureUpdate() noexcept {
        temperatureUpdateRunning_ = false;
    }

    /**
     * @brief Check if temperature update is running
     */
    bool isTemperatureUpdateRunning() const noexcept {
        return temperatureUpdateRunning_;
    }

    /**
     * @brief Start scale update operation
     */
    void startScaleUpdate() noexcept {
        scaleUpdateRunning_ = true;
    }

    /**
     * @brief Stop scale update operation
     */
    void stopScaleUpdate() noexcept {
        scaleUpdateRunning_ = false;
    }

    /**
     * @brief Check if scale update is running
     */
    bool isScaleUpdateRunning() const noexcept {
        return scaleUpdateRunning_;
    }

private:
    std::atomic<bool> temperatureUpdateRunning_{false};
    std::atomic<bool> scaleUpdateRunning_{false};
};

} // namespace CleverCoffee
