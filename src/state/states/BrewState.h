/**
 * @file BrewState.h
 * @brief Brewing operation state
 */

#pragma once

#include "../MachineState.h"
#include "../MachineStateIds.h"

/**
 * @class BrewState
 * @brief Handles active brewing operations
 *
 * This state manages the brewing process including:
 * - Monitoring brew switch status
 * - Controlling pump and valve operations
 * - Managing brew timing and weight targets
 * - Handling pre-infusion sequences
 * - Monitoring for completion or abort conditions
 * - Handling emergency stops during brewing
 * - Managing MQTT reconnection allowance
 */
class BrewState : public MachineState {
public:
    /**
     * @brief Constructor
     */
    BrewState() = default;

    /**
     * @brief Destructor
     */
    ~BrewState() override = default;

    /**
     * @brief Called when entering brew state
     * @param context Machine state context
     */
    void onEntry(MachineStateContext& context) override;

    /**
     * @brief Called when exiting brew state
     * @param context Machine state context
     */
    void onExit(MachineStateContext& context) override;

    /**
     * @brief Update brewing process
     * @param context Machine state context
     */
    void update(MachineStateContext& context) override;

    /**
     * @brief Check for state transitions during brewing
     * @param context Machine state context
     * @return New state to transition to, or nullptr if continuing to brew
     */
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

    /**
     * @brief Get state ID
     * @return State identifier
     */
    int getStateId() const override {
        return MachineStateIds::BREW;
    }

    /**
     * @brief Get state name
     * @return State name string
     */
    const char* getStateName() const override {
        return "Brew";
    }

private:
    /**
     * @brief Check if brew process is still active
     * @param context Machine state context
     * @return true if brewing should continue
     */
    bool isBrewStillActive(MachineStateContext& context) const;

    /**
     * @brief Check for emergency conditions during brewing
     * @param context Machine state context
     * @return true if emergency stop required
     */
    bool checkEmergencyConditions(MachineStateContext& context) const;

    /**
     * @brief Check for sensor errors during brewing
     * @param context Machine state context
     * @return true if sensor errors detected
     */
    bool checkSensorErrors(MachineStateContext& context) const;

    /**
     * @brief Check if PID is still enabled
     * @param context Machine state context
     * @return true if PID is enabled
     */
    bool isPidStillEnabled(MachineStateContext& context) const;
};