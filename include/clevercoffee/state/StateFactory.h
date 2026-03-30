/**
 * @file StateFactory.h
 * @brief Creates state instances.
 */

#pragma once

#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateIds.h"

#include <memory>

// Forward declarations
std::unique_ptr<MachineState> createStateInstance(MachineStateId id);
const char*                   getStateName(MachineStateId id);
