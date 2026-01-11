/**
 * @file EmergencyStopManager.h
 * @brief Centralized emergency stop logic manager
 *
 * This class provides a single source of truth for emergency stop conditions,
 * ensuring consistent behavior across all code paths. It handles:
 * - Temperature threshold detection with debouncing
 * - Hysteresis to prevent oscillation
 * - Sensor validation
 * - Emergency state management
 */

#pragma once

#include "clevercoffee/constants/Temperature.h"
#include "clevercoffee/Logger.h"

#include <cstdint>

// Forward declaration
class Config;

namespace CleverCoffee {

/**
 * @class EmergencyStopManager
 * @brief Centralized manager for emergency stop conditions
 *
 * Provides consistent emergency stop detection logic with:
 * - Configurable temperature thresholds
 * - Debouncing to prevent false alarms
 * - Hysteresis to prevent state oscillation
 * - Sensor validation
 */
class EmergencyStopManager {
public:
    /**
     * @brief Constructor
     * @param config Configuration reference (must outlive this instance)
     */
    explicit EmergencyStopManager(const Config& config);

    /**
     * @brief Check if emergency conditions are present
     * @param temperature Current temperature reading
     * @return true if emergency stop should be triggered
     *
     * Detection logic:
     * 1. Invalid sensor readings (< MIN_VALID_TEMP or > MAX_VALID_TEMP) immediately trigger emergency
     * 2. Temperature above threshold accumulates debounce count
     * 3. After DEBOUNCE_COUNT consecutive above-threshold readings, returns true
     * 4. Temperature below (threshold - hysteresis) resets debounce counter
     * 5. Temperature in hysteresis zone maintains current state
     */
    bool checkEmergencyConditions(double temperature);

    /**
     * @brief Check if emergency can be cleared
     * @param temperature Current temperature reading
     * @return true if emergency can be cleared
     *
     * Emergency can be cleared when:
     * - Temperature is below the clear threshold (threshold - hysteresis)
     * - Sensor reading is valid
     */
    bool isEmergencyCleared(double temperature) const;

    /**
     * @brief Get current emergency state
     * @return true if emergency is currently active
     */
    bool isEmergencyActive() const noexcept { return emergencyActive_; }

    /**
     * @brief Manually trigger emergency stop
     */
    void triggerEmergency() noexcept;

    /**
     * @brief Clear emergency state (call when conditions are safe)
     */
    void clearEmergency() noexcept;

    /**
     * @brief Reset internal state (for testing or reinitialization)
     */
    void reset() noexcept;

    /**
     * @brief Get current debounce count (for debugging)
     * @return Current debounce counter value
     */
    int getDebounceCount() const noexcept { return emergencyTempReadingCount_; }

private:
    // Configuration reference (not owned)
    const Config& config_;

    // Emergency state
    bool emergencyActive_ = false;
    int emergencyTempReadingCount_ = 0;

    // Constants
    static constexpr int DEBOUNCE_COUNT = 3;  ///< Require 3 consecutive readings above threshold
};

}  // namespace CleverCoffee
