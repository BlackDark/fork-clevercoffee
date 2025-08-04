/**
 * @file brewUtils.h
 * @brief Utility functions for brew state checking without heavy dependencies
 *
 * This file provides lightweight access to brew state functions for modules
 * that can't include the full brewHandler.h due to circular dependencies.
 */

#pragma once

#include "../state/GlobalState.h"

/**
 * @brief Check if brewing is currently active
 * @return true if brewing is active (not idle or finished)
 */
inline bool checkBrewActive() {
    return (g_state.sensors.currBrewState != kBrewIdle && g_state.sensors.currBrewState != kBrewFinished);
}

/**
 * @brief Check if brew process is running
 * @return true if brewing is active
 */
inline bool brew() {
    return (g_state.sensors.currBrewState != kBrewIdle && g_state.sensors.currBrewState != kBrewFinished);
}

/**
 * @brief Check if manual flush is active
 * @return true if manual flush is running
 */
inline bool manualFlush() {
    return (g_state.sensors.currManualFlushState != kManualFlushIdle);
}