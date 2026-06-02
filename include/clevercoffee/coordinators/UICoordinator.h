#pragma once

#include <atomic>

namespace CleverCoffee {

/**
 * @brief Coordinates UI state and refresh requests
 *
 * This class provides thread-safe coordination for UI operations.
 * It manages display refresh requests and controls auto-sleep behavior.
 *
 * The coordinator uses atomic operations to ensure thread safety when multiple
 * contexts may access UI state simultaneously.
 *
 * @note This class is typically accessed through SystemContext::uiCoordinator()
 *
 * Example usage:
 * @code
 * SystemContext& ctx = ...;
 *
 * // Request display update when data changes
 * onTemperatureChanged(newTemp) {
 *     updateDisplay(newTemp);
 *     ctx.uiCoordinator().clearRefresh();
 * }
 *
 * // Mark refresh needed from interrupt or timer
 * onTimerEvent() {
 *     ctx.uiCoordinator().requestRefresh();
 * }
 *
 * // Control display sleep
 * if (userIsIdle()) {
 *     ctx.uiCoordinator().setAutoSleep(true);
 *     if (ctx.uiCoordinator().shouldAutoSleep()) {
 *         display.sleep();
 *     }
 * }
 * @endcode
 */
class UICoordinator {
  public:
    UICoordinator() = default;

    /**
     * @name Display Refresh Management
     * @{
     */

    /**
     * @brief Request a display refresh
     *
     * Sets a flag indicating that the display needs to be refreshed.
     * This can be called from any context (e.g., interrupts, timers).
     *
     * @post needsRefresh() returns true
     */
    void requestRefresh() noexcept {
        refreshNeeded_ = true;
    }

    /**
     * @brief Clear refresh request
     *
     * Clears the refresh flag after the display has been updated.
     * Should be called by the display update routine.
     *
     * @post needsRefresh() returns false
     */
    void clearRefresh() noexcept {
        refreshNeeded_ = false;
    }

    /**
     * @brief Check if refresh is needed
     *
     * @return true if display refresh has been requested, false otherwise
     */
    bool needsRefresh() const noexcept {
        return refreshNeeded_;
    }

    /** @} */

    /**
     * @name Auto Sleep Control
     * @{
     */

    /**
     * @brief Set auto sleep behavior
     *
     * Enables or disables automatic display sleep during idle periods.
     *
     * @param enabled true to enable auto sleep, false to disable
     */
    void setAutoSleep(bool enabled) noexcept {
        autoSleepEnabled_ = enabled;
    }

    /**
     * @brief Check if auto sleep is enabled
     *
     * @return true if auto sleep is currently enabled, false otherwise
     */
    bool shouldAutoSleep() const noexcept {
        return autoSleepEnabled_;
    }

    /** @} */

    /**
     * @name Display Buffer Management
     * @{
     */

    /**
     * @brief Mark display buffer as ready
     *
     * Indicates that the display buffer has been prepared and is ready for rendering.
     * Used to coordinate display updates with other system operations.
     */
    void setDisplayBufferReady() noexcept {
        displayBufferReady_ = true;
    }

    /**
     * @brief Clear display buffer ready flag
     *
     * Marks the display buffer as not ready, typically after rendering is complete.
     */
    void clearDisplayBufferReady() noexcept {
        displayBufferReady_ = false;
    }

    /**
     * @brief Check if display buffer is ready
     *
     * @return true if display buffer is ready for rendering, false otherwise
     */
    bool isDisplayBufferReady() const noexcept {
        return displayBufferReady_;
    }

    /** @} */

    /**
     * @name Update Running Flags
     * @{
     */

    /**
     * @brief Mark website update as running
     *
     * Sets flag indicating that a website/webserver update is in progress.
     * Used to prevent concurrent updates.
     */
    void setWebsiteUpdateRunning(bool running) noexcept {
        websiteUpdateRunning_ = running;
    }

    /**
     * @brief Check if website update is running
     *
     * @return true if website update is in progress, false otherwise
     */
    bool isWebsiteUpdateRunning() const noexcept {
        return websiteUpdateRunning_;
    }

    /**
     * @brief Mark Home Assistant IO update as running
     *
     * Sets flag indicating that a Home Assistant discovery/update is in progress.
     * Used to prevent concurrent MQTT operations.
     */
    void setHassioUpdateRunning(bool running) noexcept {
        hassioUpdateRunning_ = running;
    }

    /**
     * @brief Check if Home Assistant IO update is running
     *
     * @return true if HASSIO update is in progress, false otherwise
     */
    bool isHassioUpdateRunning() const noexcept {
        return hassioUpdateRunning_;
    }

    /** @} */

    /**
     * @name Display Offline Counter
     * @{
     */

    /**
     * @brief Set display offline counter
     *
     * Used to track offline mode display state.
     */
    void setDisplayOffline(int value) noexcept {
        displayOffline_ = value;
    }

    /**
     * @brief Get display offline counter
     *
     * @return Current display offline counter value
     */
    int getDisplayOffline() const noexcept {
        return displayOffline_;
    }

    /**
     * @brief Increment display offline counter
     */
    void incrementDisplayOffline() noexcept {
        displayOffline_++;
    }

    /** @} */

    /**
     * @name Brew timer display state
     * @{
     */

    enum class BrewTimerDisplayState : uint8_t {
        Idle     = 10,
        Running  = 20,
        PostBrew = 30,
    };

    BrewTimerDisplayState getBrewTimerDisplayState() const noexcept {
        return brewTimerDisplayState_;
    }

    void setBrewTimerDisplayState(BrewTimerDisplayState state) noexcept {
        brewTimerDisplayState_ = state;
    }

    uint32_t getBrewTimerEndTime() const noexcept {
        return brewTimerEndTime_;
    }

    void setBrewTimerEndTime(uint32_t timeMs) noexcept {
        brewTimerEndTime_ = timeMs;
    }

    /** @} */

  private:
    std::atomic<bool>     refreshNeeded_{false};        ///< Flag for pending display refresh
    std::atomic<bool>     autoSleepEnabled_{true};      ///< Flag for auto sleep state
    std::atomic<bool>     displayBufferReady_{false};   ///< Flag indicating display buffer is ready
    std::atomic<bool>     websiteUpdateRunning_{false}; ///< Flag for website update in progress
    std::atomic<bool>     hassioUpdateRunning_{false};  ///< Flag for HASSIO update in progress
    std::atomic<int>      displayOffline_{0};           ///< Counter for display offline state
    BrewTimerDisplayState brewTimerDisplayState_{BrewTimerDisplayState::Idle};
    uint32_t              brewTimerEndTime_{0};
};

} // namespace CleverCoffee
