/**
 * @file PidNormalState.h
 * @brief Normal PID operation state - main operational mode
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

/**
 * @class PidNormalState
 * @brief Main operational state with PID temperature control enabled
 *
 * This state handles normal coffee machine operation with active temperature
 * control. It monitors for user inputs and system conditions to transition
 * to brewing, steaming, maintenance, or error states.
 *
 * Key responsibilities:
 * - Monitor brewing process triggers
 * - Handle manual flush operations
 * - Manage steam mode activation
 * - Process hot water requests
 * - Handle backflush operations
 * - Monitor for emergency conditions
 * - Manage standby mode transitions
 * - Check for system errors
 */
class PidNormalState : public MachineState {
    public:
        /**
         * @brief Constructor
         */
        PidNormalState() = default;

        /**
         * @brief Destructor
         */
        ~PidNormalState() override = default;

        /**
         * @brief Called when entering normal PID state
         * @param context Machine state context
         */
        void onEntry(MachineStateContext& context) override;

        /**
         * @brief Called when exiting normal PID state
         * @param context Machine state context
         */
        void onExit(MachineStateContext& context) override;

        /**
         * @brief Update state logic - monitor system and user inputs
         * @param context Machine state context
         */
        void update(MachineStateContext& context) override;

        /**
         * @brief Check for state transitions based on user inputs and system status
         * @param context Machine state context
         * @return New state to transition to, or nullptr if staying in normal mode
         */
        std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

        /**
         * @brief Get state ID
         * @return State identifier
         */
        MachineStateId getStateId() const override {
            return MachineStateId::PID_NORMAL;
        }

        /**
         * @brief Get state name
         * @return State name string
         */
        const char* getStateName() const override {
            return "PID Normal";
        }

    private:
        /**
         * @brief Check for emergency conditions
         * @param context Machine state context
         * @return true if emergency stop required
         */
        bool checkEmergencyConditions(MachineStateContext& context) const;

        /**
         * @brief Check if standby mode should be activated
         * @param context Machine state context
         * @return true if should enter standby
         */
        bool shouldEnterStandby(MachineStateContext& context) const;

        /**
         * @brief Check for critical system errors
         * @param context Machine state context
         * @return true if system errors detected
         */
        bool checkSystemErrors(MachineStateContext& context) const;

        /**
         * @brief Reset standby timer for active operations
         * @param context Machine state context
         */
        void resetStandbyTimerIfNeeded(MachineStateContext& context) const;
};