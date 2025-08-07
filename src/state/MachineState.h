/**
 * @file MachineState.h
 * @brief State machine interface definitions for coffee machine states
 */

#pragma once

#include <memory>

#if __cplusplus >= 202002L
#include <concepts>

// C++20 concept for state validation
template<typename T>
concept StateLike = requires(T& state, MachineStateContext& ctx) {
    { state.getStateId() } -> std::convertible_to<int>;
    { state.getStateName() } -> std::convertible_to<const char*>;
    { state.update(ctx) } -> std::same_as<void>;
    { state.checkTransitions(ctx) } -> std::convertible_to<std::unique_ptr<MachineState>>;
};
#endif

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
        virtual void onEntry(MachineStateContext& context) {
        }

        /**
         * @brief Called when exiting this state
         * @param context The machine state context containing all necessary data
         */
        virtual void onExit(MachineStateContext& context) {
        }

        /**
         * @brief Update the state - called every cycle while in this state
         * @param context The machine state context containing all necessary data
         */
        virtual void update(MachineStateContext& context) = 0;

        /**
         * @brief Check for state transitions
         * @param context The machine state context containing all necessary data
         * @return New state to transition to, or nullptr if no transition
         */
        virtual std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) = 0;

        /**
         * @brief Get the state ID for debugging and logging
         * @return State identifier
         */
        virtual int getStateId() const = 0;

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