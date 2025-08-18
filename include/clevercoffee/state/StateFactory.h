/**
 * @file StateFactory.h
 * @brief Creates state instances from a central registry.
 */

#pragma once

#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/StateInfo.h"

#include <memory>

/**
 * @brief Get singleton state instance by ID using the central state registry.
 * @param id The state ID to get.
 * @return Raw pointer to singleton state, or nullptr if ID not found.
 */
inline MachineState* getStateInstance(MachineStateId id) {
    if (const auto* info = getStateInfo(id)) {
        if (info->getInstance) {
            return info->getInstance();
        }
    }
    return nullptr;
}
