/**
 * @file StateMachine.h
 * @brief State machine controller for coffee machine operations
 */

#pragma once

#include "MachineState.h"
#include "MachineStateContext.h"
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
        StateMachine(DisplayManager* displayManager, HardwareManager* hardwareManager, SensorManager* sensorManager, CleverCoffeeWiFiManager* wifiManager, MQTTManager* mqttManager);

        /**
         * @brief Destructor
         */
        ~StateMachine() = default;

        /**
         * @brief Initialize state machine with initial state
         * @param initialState Initial state to start with (defaults to InitState)
         * @return true if initialization successful
         */
        bool initialize(std::unique_ptr<MachineState> initialState = nullptr);

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
        void transitionTo(std::unique_ptr<MachineState> newState, const char* reason = nullptr);

        /**
         * @brief Get current state ID
         * @return Current state identifier, or -1 if no state
         */
        int getCurrentStateId() const;

        /**
         * @brief Get current state name
         * @return Current state name, or "None" if no state
         */
        const char* getCurrentStateName() const;

        /**
         * @brief Check if state machine is initialized
         * @return true if state machine has a current state
         */
        bool isInitialized() const;

        /**
         * @brief Get state context for external access
         * @return Reference to the state context
         *
         * This allows external code to access the context for specific
         * operations that don't require state transitions.
         */
        MachineStateContext& getContext() {
            return context_;
        }

        /**
         * @brief Get state context for external access (const version)
         * @return Const reference to the state context
         */
        const MachineStateContext& getContext() const {
            return context_;
        }

        /**
         * @brief Get current state for external inspection
         * @return Pointer to current state, or nullptr if none
         *
         * This is primarily for debugging and testing purposes.
         */
        const MachineState* getCurrentState() const {
            return currentState_.get();
        }

    private:
        /**
         * @brief Execute state transition with proper callbacks
         * @param newState New state to transition to
         * @param reason Optional reason for logging
         */
        void executeTransition(std::unique_ptr<MachineState> newState, const char* reason = nullptr);

        /**
         * @brief Log state machine status for debugging
         */
        void logStateMachineStatus() const;

        // State machine components
        std::unique_ptr<MachineState> currentState_; ///< Current active state
        MachineStateContext context_;                ///< Context for state access to resources

        // State machine status
        bool initialized_;             ///< True if state machine is initialized
        int lastStateId_;              ///< Last state ID for change detection
        unsigned long lastUpdateTime_; ///< Last update timestamp for timing debug
        unsigned long stateEntryTime_; ///< Time when current state was entered

        // Statistics for debugging and monitoring
        unsigned long totalStateTransitions_; ///< Total number of state transitions
        unsigned long totalUpdates_;          ///< Total number of update calls
};