/**
 * @file StateMachine.h
 * @brief State machine controller for coffee machine operations
 */

#pragma once

#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateContext.h"

#include <chrono>
#include <memory>

// Forward declarations
class DisplayManager;
class HardwareManager;
class SensorManager;
class CleverCoffeeWiFiManager;
class MQTTManager;

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
     * @param displayManager Display manager instance
     * @param hardwareManager Hardware manager instance
     * @param sensorManager Sensor manager instance
     * @param wifiManager WiFi manager instance
     * @param mqttManager MQTT manager instance
     */
    StateMachine(DisplayManager*          displayManager,
                 HardwareManager*         hardwareManager,
                 SensorManager*           sensorManager,
                 CleverCoffeeWiFiManager* wifiManager,
                 MQTTManager*             mqttManager);

    /**
     * @brief Destructor
     */
    ~StateMachine() = default;

    /**
     * @brief Initialize state machine with initial state
     * @param initialState Initial state to start with (defaults to InitState)
     * @return true if initialization successful
     */
    bool initialize(MachineState* initialState = nullptr);

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
     * @param newState New state to transition to
     * @param reason Optional reason for the transition (for logging)
     *
     * This method allows external code to force a state transition,
     * useful for emergency conditions or external triggers.
     */
    void transitionTo(MachineState* newState, const char* reason = nullptr);

    /**
     * @brief Get current state ID
     * @return Current state identifier, or INIT if no state
     */
    MachineStateId getCurrentStateId() const noexcept;

    /**
     * @brief Get current state name
     * @return Current state name, or "None" if no state
     */
    const char* getCurrentStateName() const noexcept;

    /**
     * @brief Check if state machine is initialized
     * @return true if state machine has a current state
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
     * @return Pointer to current state, or nullptr if none
     *
     * This is primarily for debugging and testing purposes.
     */
    const MachineState* getCurrentState() const noexcept {
        return currentState_;
    }

  private:
    /**
     * @brief Execute state transition with proper callbacks
     * @param newState New state to transition to
     * @param reason Optional reason for logging
     */
    void executeTransition(MachineState* newState, const char* reason = nullptr);

    /**
     * @brief Log state machine status for debugging
     */
    void logStateMachineStatus() const;

    // State machine components
    MachineState*       currentState_; ///< Current active state (singleton - not owned)
    MachineStateContext context_;      ///< Context for state access to resources

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