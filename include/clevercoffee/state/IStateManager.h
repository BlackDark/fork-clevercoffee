/**
 * @file IStateManager.h
 * @brief Interface for state management
 *
 * Provides abstraction for state transitions and timing,
 * decoupling states from concrete state machine implementation.
 */

#pragma once

#include "clevercoffee/state/MachineStateIds.h"

// Forward declaration
class MachineState;

namespace CleverCoffee {

/**
 * @brief Interface for state management
 *
 * Abstracts state machine operations, allowing states to
 * trigger transitions without knowing the concrete implementation.
 */
class IStateManager {
public:
    virtual ~IStateManager() = default;

    virtual MachineStateId getCurrentStateId() const noexcept = 0;
    virtual void transitionTo(MachineState& newState) = 0;
    virtual bool hasStateTimeoutElapsed(unsigned long timeoutMs) const noexcept = 0;
    virtual unsigned long getStateStartTime() const noexcept = 0;
};

} // namespace CleverCoffee
