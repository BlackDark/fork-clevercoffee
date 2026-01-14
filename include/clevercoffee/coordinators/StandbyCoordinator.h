#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"

namespace CleverCoffee {

// Standby display-off timeout constant (avoids dependency on GlobalState.h)
inline constexpr unsigned long getDisplayOffTimeoutMillis() {
    return 10 * 60 * 1000; // 10 minutes
}

/**
 * @brief Coordinates standby mode and power management
 *
 * This class manages standby timers and display timeout logic,
 * providing a clean interface for standby-related operations.
 *
 * @note This class is typically accessed through SystemContext::standbyCoordinator()
 */
class StandbyCoordinator {
  public:
    StandbyCoordinator() = default;

    /**
     * @brief Update standby timer countdown
     *
     * Should be called periodically (typically every loop iteration).
     * Updates countdown timers and logs remaining time.
     */
    void update() {
        if (!Config::getInstance().standbyEnabled.get()) {
            return;
        }

        const unsigned long currentTime = millis();

        // Update every second since last update
        if (currentTime - lastStandbyTimeMillis_ >= 1000) {
            lastStandbyTimeMillis_ = currentTime;

            if (standbyModeRemainingTimeMillis_ != 0) {
                const unsigned long standbyModeTimeMillis = getStandbyTimeoutMillis();
                const unsigned long elapsedTime           = currentTime - standbyModeStartTimeMillis_;

                if (standbyModeTimeMillis > elapsedTime) {
                    standbyModeRemainingTimeMillis_ = standbyModeTimeMillis - elapsedTime;

                    if (elapsedTime % 60000 < 1000) {
                        LOGF(INFO, "Standby time remaining: %i minutes", (standbyModeRemainingTimeMillis_ / 60000) + 1);
                    }
                } else {
                    standbyModeRemainingTimeMillis_ = 0;
                    LOG(INFO, "Entering standby mode...");
                }
            } else if (standbyModeRemainingTimeDisplayOffMillis_ != 0) {
                const unsigned long standbyModeTimeMillis = getStandbyTimeoutMillis() + getDisplayOffTimeoutMillis();
                const unsigned long elapsedTime           = currentTime - standbyModeStartTimeMillis_;

                if (standbyModeTimeMillis > elapsedTime) {
                    standbyModeRemainingTimeDisplayOffMillis_ = standbyModeTimeMillis - elapsedTime;

                    if (elapsedTime % 60000 < 1000) {
                        LOGF(INFO,
                             "Standby time until display is turned off: %i minutes",
                             (standbyModeRemainingTimeDisplayOffMillis_ / 60000) + 1);
                    }
                } else {
                    standbyModeRemainingTimeDisplayOffMillis_ = 0;
                    LOG(INFO, "Turning off display...");
                }
            }
        }
    }

    /**
     * @brief Reset standby timer to configured timeout
     *
     * Restarts the standby countdown from the configured standby time.
     * Only resets if standby is enabled.
     */
    void reset() {
        // Only reset if standby is enabled
        if (!Config::getInstance().standbyEnabled.get()) {
            return;
        }

        standbyModeRemainingTimeMillis_           = getStandbyTimeoutMillis();
        standbyModeRemainingTimeDisplayOffMillis_ = getDisplayOffTimeoutMillis();
        standbyModeStartTimeMillis_               = millis();

        LOGF(INFO, "Resetting standby timer to %i minutes", static_cast<int>(Config::getInstance().standbyTime.get()));
    }

    /**
     * @brief Get remaining standby time in milliseconds
     * @return Remaining time until standby mode activates
     */
    unsigned long getRemainingTimeMillis() const noexcept {
        return standbyModeRemainingTimeMillis_;
    }

    /**
     * @brief Set remaining standby time
     * @param millis Time in milliseconds
     */
    void setRemainingTimeMillis(unsigned long millis) noexcept {
        standbyModeRemainingTimeMillis_ = millis;
    }

    /**
     * @brief Check if standby should be triggered
     * @return true if standby timer has expired and timer was initialized
     */
    bool shouldEnterStandby() const noexcept {
        // Only enter standby if:
        // 1. Standby is enabled
        // 2. Timer has expired (remaining time is 0)
        // 3. Timer was initialized (start time is not 0) - prevents immediate entry when first enabled
        return Config::getInstance().standbyEnabled.get() && standbyModeRemainingTimeMillis_ == 0 &&
               standbyModeStartTimeMillis_ != 0;
    }

    /**
     * @brief Initialize standby timer if not already initialized
     * Should be called when standby is enabled to start the countdown
     */
    void initializeIfNeeded() {
        if (!Config::getInstance().standbyEnabled.get()) {
            return;
        }
        // If timer hasn't been started yet, initialize it
        if (standbyModeStartTimeMillis_ == 0) {
            reset();
        }
    }

  private:
    /**
     * @brief Get configured standby timeout in milliseconds
     * @return Timeout in milliseconds
     */
    static unsigned long getStandbyTimeoutMillis() {
        return static_cast<unsigned long>(Config::getInstance().standbyTime.get() * 60 * 1000);
    }

    unsigned long standbyModeRemainingTimeMillis_{0}; ///< Countdown to standby mode
    unsigned long standbyModeStartTimeMillis_{0};     ///< Time when standby countdown started
    unsigned long standbyModeRemainingTimeDisplayOffMillis_{getDisplayOffTimeoutMillis()}; ///< Display off countdown
    unsigned long lastStandbyTimeMillis_{0};                                               ///< Last update timestamp
};

} // namespace CleverCoffee
