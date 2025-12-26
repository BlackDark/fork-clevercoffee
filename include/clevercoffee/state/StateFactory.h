/**
 * @file StateFactory.h
 * @brief Creates state instances from a central registry.
 */

#pragma once

#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/StateInfo.h"

#include <memory>

/**
 * @brief Get singleton state instance by ID using the central state registry.
 * @param id The state ID to get.
 * @return Raw pointer to singleton state (never nullptr).
 * @note If state ID is not registered, system will restart immediately with error log.
 */
inline MachineState* getStateInstance(MachineStateId id) {
    if (const auto* info = getStateInfo(id)) {
        if (info->getInstance) {
            return info->getInstance();
        }
    }
    // CRITICAL: State not found or not registered
    // This indicates a fatal configuration error - restart immediately
    LOGF(FATAL, "CRITICAL: State not registered (ID=%d). System will restart.", static_cast<int>(id));
    ESP.restart();
    return nullptr;  // Unreachable, but satisfies return type
}
