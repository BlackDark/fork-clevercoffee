/**
 * @file InitState.h
 * @brief Initial state for system startup and validation
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

/**
 * @class InitState
 * @brief Handles system initialization and validation before normal operation
 *
 * The InitState performs initial system checks and transitions to appropriate
 * states based on system conditions:
 * - Checks water tank status
 * - Validates sensor functionality
 * - Determines if PID should be enabled
 * - Transitions to normal operation or error states
 */
class InitState : public MachineState {
    public:
        /**
         * @brief Constructor
         */
        InitState() = default;

        /**
         * @brief Destructor
         */
        ~InitState() override = default;

        /**
         * @brief Called when entering init state
         * @param context Machine state context
         */
        void onEntry(MachineStateContext& context) override;

        /**
         * @brief Called when exiting init state
         * @param context Machine state context
         */
        void onExit(MachineStateContext& context) override;

        /**
         * @brief Update state logic - performs system validation
         * @param context Machine state context
         */
        void update(MachineStateContext& context) override;

        /**
         * @brief Check for state transitions based on system status
         * @param context Machine state context
         * @return New state to transition to, or nullptr if staying in init
         */
        std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

        /**
         * @brief Get state ID
         * @return State identifier
         */
        int getStateId() const override {
            return MachineStateIds::INIT;
        }

        /**
         * @brief Get state name
         * @return State name string
         */
        const char* getStateName() const override {
            return "Init";
        }

    private:
        /**
         * @brief Check if water tank is full
         * @param context Machine state context
         * @return true if water tank is full
         */
        bool checkWaterTank(MachineStateContext& context) const;

        /**
         * @brief Check if sensors are functioning properly
         * @param context Machine state context
         * @return true if sensors are OK
         */
        bool checkSensors(MachineStateContext& context) const;

        /**
         * @brief Check if PID should be enabled
         * @param context Machine state context
         * @return true if PID should be enabled
         */
        bool checkPidEnabled(MachineStateContext& context) const;
};