/**
 * @file BackflushReminderLogic.h
 * @brief Pure logic for backflush reminder shot counting
 */

#pragma once

#include "clevercoffee/defaults.h"

namespace CleverCoffee::Maintenance {

/**
 * @brief Determine whether a completed brew cycle should increment the shot counter.
 *
 * Count unless brew time is below the minimum AND (scale disabled OR weight below minimum).
 */
[[nodiscard]] inline constexpr bool qualifiesAsCountedShot(double totalBrewTimeMs,
                                                           float  brewWeight,
                                                           bool   scaleEnabled) noexcept {
    if (totalBrewTimeMs >= BACKFLUSH_REMINDER_MIN_BREW_TIME_MS) {
        return true;
    }
    if (scaleEnabled && brewWeight >= BACKFLUSH_REMINDER_MIN_BREW_WEIGHT_G) {
        return true;
    }
    return false;
}

[[nodiscard]] inline constexpr bool isReminderDueForCount(int shots, bool enabled, int threshold) noexcept {
    return enabled && shots >= threshold;
}

} // namespace CleverCoffee::Maintenance
