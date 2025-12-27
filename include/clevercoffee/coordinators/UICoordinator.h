#pragma once

#include <atomic>

namespace CleverCoffee {

/**
 * @brief Coordinates UI state and refresh requests
 *
 * This class provides thread-safe coordination for UI operations, replacing
 * the previous global g_state.ui flags. It manages display refresh requests
 * and controls auto-sleep behavior.
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

private:
    std::atomic<bool> refreshNeeded_{false};  ///< Flag for pending display refresh
    std::atomic<bool> autoSleepEnabled_{true}; ///< Flag for auto sleep state
};

} // namespace CleverCoffee
