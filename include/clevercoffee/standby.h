/**
 * @file standby.h
 *
 * @brief Standby mode (deprecated - use StandbyCoordinator via SystemContext instead)
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"

inline unsigned long getStandbyTimeoutMillis() {
    return static_cast<unsigned long>(Config::getInstance().standbyTime.get() * 60 * 1000);
}

/**
 * @brief Decrements the remaining standby time every second, counting down from the configured duration
 * @deprecated Use StandbyCoordinator::update() via SystemContext instead
 */
inline void updateStandbyTimer() {
    if (auto* ctx = CleverCoffee::getGlobalSystemContext()) {
        ctx->standbyCoordinator().update();
    }
    // Silently skip if SystemContext not yet initialized
}

/**
 * @brief Reset standby timer to configured timeout
 * @deprecated Use StandbyCoordinator::reset() via SystemContext instead
 */
inline void resetStandbyTimer() {
    if (auto* ctx = CleverCoffee::getGlobalSystemContext()) {
        ctx->standbyCoordinator().reset();
    }
    // Silently skip if SystemContext not yet initialized
}
