#pragma once

#include <atomic>

namespace CleverCoffee {

/**
 * @brief Coordinates UI state and refresh requests
 *
 * Replaces g_state.ui flags with thread-safe coordinator.
 * Manages display refresh and sleep behavior.
 */
class UICoordinator {
public:
    UICoordinator() = default;

    /**
     * @brief Request a display refresh
     */
    void requestRefresh() noexcept {
        refreshNeeded_ = true;
    }

    /**
     * @brief Clear refresh request
     */
    void clearRefresh() noexcept {
        refreshNeeded_ = false;
    }

    /**
     * @brief Check if refresh is needed
     */
    bool needsRefresh() const noexcept {
        return refreshNeeded_;
    }

    /**
     * @brief Set auto sleep behavior
     */
    void setAutoSleep(bool enabled) noexcept {
        autoSleepEnabled_ = enabled;
    }

    /**
     * @brief Check if auto sleep is enabled
     */
    bool shouldAutoSleep() const noexcept {
        return autoSleepEnabled_;
    }

private:
    std::atomic<bool> refreshNeeded_{false};
    std::atomic<bool> autoSleepEnabled_{true};
};

} // namespace CleverCoffee
