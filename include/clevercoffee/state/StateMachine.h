/**
 * @file StateMachine.h
 * @brief State machine controller for coffee machine operations
 */

#pragma once

#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/MachineStateIds.h"

#include <chrono>
#include <functional>
#include <memory>
#include <optional>

// Forward declarations
class DisplayManager;
class CleverCoffeeWiFiManager;
class MQTTManager;

namespace CleverCoffee {
class HardwareManager;
class SystemContext;
}

/**
 * @class StateMachine
 * @brief Central controller for the coffee machine state machine
 *
 * The StateMachine class manages the current state and coordinates state
 * transitions. It provides a clean interface to replace the existing
 * switch-based state handling in main.cpp.
 *
 * Key responsibilities:
 * - Maintain current state
 * - Execute state logic (update, transitions)
 * - Handle state entry/exit callbacks
 * - Provide state information for logging and debugging
 * - Manage the state context for resource access
 */
class StateMachine {
  public:
    /**
      * @brief Constructor
      * @param systemContext System context instance (REQUIRED)
      * @param hardwareManager Hardware manager instance (REQUIRED - CRITICAL component)
      * @param displayManager Display manager (REQUIRED - always exists)
      * @param wifiManager WiFi manager (REQUIRED - always exists)
      * @param mqttManager MQTT manager (REQUIRED - always exists)
      */
    StateMachine(CleverCoffee::SystemContext& systemContext,
                 CleverCoffee::HardwareManager& hardwareManager,
                 DisplayManager&               displayManager,
                 CleverCoffeeWiFiManager&      wifiManager,
                 MQTTManager&                  mqttManager);

    /**
     * @brief Destructor
     */
    ~StateMachine() = default;

    /**
     * @brief Initialize state machine with initial state
     * @param initialStateId Initial state ID to start with (defaults to INIT)
     * @note Always succeeds - uses INIT as fallback if creation fails
     */
    void initialize(MachineStateId initialStateId = MachineStateId::INIT);

    /**
     * @brief Update state machine - call this from main loop
     *
     * This method should be called regularly from the main loop.
     * It executes the current state's update logic and checks for
     * transitions to new states.
     */
    void update();

    /**
     * @brief Force transition to a new state
     * @param newStateId State ID to transition to
     * @param reason Optional reason for the transition (for logging)
     *
     * This method allows external code to force a state transition,
     * useful for emergency conditions or external triggers.
     */
    void transitionTo(MachineStateId newStateId, const char* reason = nullptr);

    /**
     * @brief Get current state ID
     * @return Current state identifier (always valid - StateMachine always has a state)
     */
    MachineStateId getCurrentStateId() const noexcept;

    /**
     * @brief Get current state name
     * @return Current state name (always valid - StateMachine always has a state)
     */
    const char* getCurrentStateName() const noexcept;

    /**
     * @brief Check if state machine is initialized
     * @return true if state machine is initialized
     */
    bool isInitialized() const noexcept;

    /**
     * @brief Get state context for external access
     * @return Reference to the state context
     *
     * This allows external code to access the context for specific
     * operations that don't require state transitions.
     */
    MachineStateContext& getContext() noexcept {
        return context_;
    }

    /**
     * @brief Get state context for external access (const version)
     * @return Const reference to the state context
     */
    const MachineStateContext& getContext() const noexcept {
        return context_;
    }

    /**
     * @brief Get current state for external inspection
     * @return Reference to current state
     *
     * This is primarily for debugging and testing purposes.
     * @note Always valid - StateMachine always has a state
     */
    const MachineState& getCurrentState() const noexcept {
        return *currentState_;
    }

  private:
    /**
     * @brief Execute state transition with proper callbacks
     * @param newStateId State ID to transition to
     * @param reason Optional reason for logging
     */
    void executeTransition(MachineStateId newStateId, const char* reason = nullptr);

    /**
     * @brief Log state machine status for debugging
     */
    void logStateMachineStatus() const;

    // State machine components
    std::unique_ptr<MachineState> currentState_;  ///< Current active state (always valid)
    MachineStateContext context_;  ///< Context for state access to resources

    // State machine status
    bool                                  initialized_;    ///< True if state machine is initialized
    MachineStateId                        lastStateId_;    ///< Last state ID for change detection
    std::chrono::steady_clock::time_point lastUpdateTime_; ///< Last update timestamp for timing debug
    std::chrono::steady_clock::time_point startTime_;      ///< State machine start time

    // Statistics for debugging and monitoring
    std::size_t totalStateTransitions_; ///< Total number of state transitions
    std::size_t totalUpdates_;          ///< Total number of update calls

    /**
     * @brief Get time spent in current state
     * @return Duration in current state
     */
    std::chrono::milliseconds getTimeInCurrentState() const noexcept {
        return std::chrono::milliseconds(context_.getStateElapsedTimeMs());
    }

    /**
     * @brief Get total uptime of state machine
     * @return Duration since initialization
     */
    std::chrono::seconds getUptime() const noexcept {
        return std::chrono::duration_cast<std::chrono::seconds>(std::chrono::steady_clock::now() - startTime_);
    }
};