/**
 * @file EmergencyStopState.h
 * @brief Emergency stop state for critical safety conditions
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

/**
 * @class EmergencyStopState
 * @brief Handles emergency stop conditions for safety
 *
 * This state is activated when critical safety conditions are detected:
 * - Excessive temperature readings
 * - Hardware malfunction detection
 * - Manual emergency stop activation
 * - Critical system failures
 *
 * In this state, all heating and pumping operations are immediately
 * stopped and the system enters a safe condition. Recovery requires
 * clearing the emergency condition and manual restart.
 */
class EmergencyStopState : public MachineState {
    public:
        /**
         * @brief Constructor
         */
        EmergencyStopState() = default;

        /**
         * @brief Destructor
         */
        ~EmergencyStopState() override = default;

        /**
         * @brief Called when entering emergency stop state
         * @param context Machine state context
         */
        void onEntry(MachineStateContext& context) override;

        /**
         * @brief Called when exiting emergency stop state
         * @param context Machine state context
         */
        void onExit(MachineStateContext& context) override;

        /**
         * @brief Update emergency stop state
         * @param context Machine state context
         */
        void update(MachineStateContext& context) override;

        /**
         * @brief Check for recovery from emergency stop
         * @param context Machine state context
         * @return New state to transition to, or nullptr if staying in emergency stop
         */
        std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

        /**
         * @brief Get state ID
         * @return State identifier
         */
        int getStateId() const override {
            return MachineStateIds::EMERGENCY_STOP;
        }

        /**
         * @brief Get state name
         * @return State name string
         */
        const char* getStateName() const override {
            return "Emergency Stop";
        }

    private:
        /**
         * @brief Perform emergency shutdown procedures
         * @param context Machine state context
         */
        void performEmergencyShutdown(MachineStateContext& context);

        /**
         * @brief Check if emergency condition has been cleared
         * @param context Machine state context
         * @return true if emergency condition is cleared
         */
        bool isEmergencyCleared(MachineStateContext& context) const;

        /**
         * @brief Determine appropriate recovery state
         * @param context Machine state context
         * @return Appropriate state for recovery
         */
        std::unique_ptr<MachineState> getRecoveryState(MachineStateContext& context) const;
};