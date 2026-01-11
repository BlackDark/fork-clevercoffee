/**
 * @file IStateManager.h
 * @brief Interface for state management
 *
 * This interface provides an abstraction for state transitions and timing,
 * decoupling states from the concrete state machine implementation.
 */

#pragma once

#include "clevercoffee/state/MachineStateIds.h"

// Forward declaration
class MachineState;

namespace CleverCoffee {

/**
 * @brief Interface for state management
 *
 * This interface abstracts state machine operations, allowing states to
 * trigger transitions and check timing without knowing the concrete
 * state machine implementation. This design provides several benefits:
 *
 * 1. Decoupling: States don't need to know about StateMachine class
 * 2. Testability: Easy to create mock implementations for testing
 * 3. Flexibility: State machine implementation can change independently
 *
 * Design Pattern: Dependency Inversion
 * - States depend on this abstraction, not concrete state machine
 * - State machine implements this interface to serve state needs
 *
 * Example usage in a state:
 * @code
 * class PreheatState : public MachineState {
 * public:
 *     void execute(IStateManager& manager) override {
 *         if (manager.hasStateTimeoutElapsed(5000)) {
 *             // Transition to next state after 5 seconds
 *             manager.transitionTo(nextState);
 *         }
 *
 *         MachineStateId current = manager.getCurrentStateId();
 *         // ... use current state ID ...
 *     }
 * };
 * @endcode
 *
 * Example mock for testing:
 * @code
 * class MockStateManager : public IStateManager {
 * public:
 *     MachineStateId getCurrentStateId() const noexcept override {
 *         return MachineStateId::BREW;
 *     }
 *
 *     void transitionTo(MachineState& newState) override {
 *         transitions.push_back(&newState);
 *     }
 *
 *     bool hasStateTimeoutElapsed(unsigned long timeout) const noexcept override {
 *         return true; // Always timeout for testing
 *     }
 *
 *     std::vector<MachineState*> transitions;
 * };
 * @endcode
 */
class IStateManager {
public:
    virtual ~IStateManager() = default;

    /**
     * @brief Get the current state identifier
     *
     * Returns the ID of the currently active state. This can be used
     * for conditional logic based on the current state.
     *
     * @return The current state's ID
     */
    virtual MachineStateId getCurrentStateId() const noexcept = 0;

    /**
     * @brief Transition to a new state
     *
     * Requests a state transition to the specified state ID. The state machine
     * will handle the transition, including exit/enter actions.
     *
     * @param newStateId State ID to transition to
     *
     * @note This is a non-blocking request. The actual transition happens
     *       in the state machine's update cycle.
     */
    virtual void transitionTo(MachineStateId newStateId) = 0;

    /**
     * @brief Check if state timeout has elapsed
     *
     * Determines whether the specified timeout period has passed since
     * the current state was entered. Useful for time-based state transitions.
     *
     * @param timeoutMs Timeout duration in milliseconds
     * @return true if the timeout has elapsed, false otherwise
     *
     * Example:
     * @code
     * if (manager.hasStateTimeoutElapsed(5000)) {
     *     // 5 seconds have passed since state entry
     *     manager.transitionTo(nextState);
     * }
     * @endcode
     */
    virtual bool hasStateTimeoutElapsed(unsigned long timeoutMs) const noexcept = 0;

    /**
     * @brief Get the time when current state was entered
     *
     * Returns the timestamp (milliseconds) when the current state was entered.
     * Can be used for custom timing logic.
     *
     * @return State entry time in milliseconds (typically from millis())
     *
     * Example:
     * @code
     * unsigned long entryTime = manager.getStateStartTime();
     * unsigned long elapsed = millis() - entryTime;
     * if (elapsed > 10000) {
     *     // Custom timeout handling
     * }
     * @endcode
     */
    virtual unsigned long getStateStartTime() const noexcept = 0;
};

} // namespace CleverCoffee
