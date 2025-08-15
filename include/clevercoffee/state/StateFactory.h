/**
 * @file StateFactory.h
 * @brief Creates state instances from a central registry.
 */

#pragma once

#include <memory>
#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/StateInfo.h"

/**
 * @brief Create a state instance by ID using the central state registry.
 * @param id The state ID to create.
 * @return Unique pointer to the state, or nullptr if ID not found.
 */
inline std::unique_ptr<MachineState> createState(MachineStateId id) {
    if (const auto* info = getStateInfo(id)) {
        if (info->factory) {
            return info->factory();
        }
    }
    return nullptr;
}
