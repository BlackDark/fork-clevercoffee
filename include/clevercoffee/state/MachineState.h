/**
 * @file MachineState.h
 * @brief State machine interface definitions for coffee machine states
 */

#pragma once

#include "clevercoffee/state/MachineStateIds.h"

#include <memory>
#include <optional>

// Forward declaration
class MachineStateContext;

/**
 * @class MachineState
 * @brief Abstract base class for all coffee machine states
 *
 * This class defines the interface for the State pattern implementation.
 * Each concrete state implements specific behavior for entry, exit, update,
 * and transition logic.
 */
class MachineState {
  public:
    /**
     * @brief Virtual destructor for proper cleanup
     */
    virtual ~MachineState() = default;

    /**
     * @brief Called when entering this state
     * @param context The machine state context containing all necessary data
     */
    virtual void onEntry(MachineStateContext& context) {}

    /**
     * @brief Called when exiting this state
     * @param context The machine state context containing all necessary data
     */
    virtual void onExit(MachineStateContext& context) {}

    /**
     * @brief Update the state - called every cycle while in this state
     * @param context The machine state context containing all necessary data
     */
    virtual void update(MachineStateContext& context) = 0;

    /**
     * @brief Check for state transitions
     * @param context The machine state context containing all necessary data
     * @return Optional state ID to transition to, or nullopt if no transition
     */
    virtual std::optional<MachineStateId> checkTransitions(MachineStateContext& context) = 0;

    /**
     * @brief Get the state ID for debugging and logging
     * @return State identifier
     */
    virtual MachineStateId getStateId() const = 0;

    /**
     * @brief Get human-readable state name for debugging
     * @return State name string
     */
    virtual const char* getStateName() const = 0;

  protected:
    /**
     * @brief Protected constructor - only concrete states can be instantiated
     */
    MachineState() = default;
};
