/**
 * @file StateInfo.h
 * @brief Defines a structure for holding state information and provides a central registry for all states.
 */

#pragma once

#include <functional>
#include <memory>
#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateIds.h"

struct StateInfo {
    MachineStateId id;
    const char* name;
    std::function<std::unique_ptr<MachineState>()> factory;
};

const StateInfo* getStateInfo(MachineStateId id);
