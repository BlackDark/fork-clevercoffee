/**
 * @file StateTransitionHelper.h
 * @brief Helper utilities for common state transition patterns
 */

#pragma once

#include <memory>
#include "clevercoffee/state/MachineStateIds.h"

// Forward declarations
class MachineState;
class MachineStateContext;

/**
 * @namespace StateTransitionHelper
 * @brief Utility functions for common state transition patterns
 * 
 * These functions help reduce code duplication across state implementations
 * by providing common transition logic for emergency stops, sensor errors, etc.
 */
namespace StateTransitionHelper {

    /**
     * @brief Check for emergency stop condition and return appropriate state
     * @param context The machine state context
     * @param currentStateId The current state ID for logging
     * @return EmergencyStopState if emergency stop is active, nullptr otherwise
     */
    std::unique_ptr<MachineState> checkEmergencyStop(MachineStateContext& context, MachineStateId currentStateId);

    /**
     * @brief Check for sensor error condition and return appropriate state
     * @param context The machine state context
     * @param currentStateId The current state ID for logging
     * @return SensorErrorState if sensor error is detected, nullptr otherwise
     */
    std::unique_ptr<MachineState> checkSensorError(MachineStateContext& context, MachineStateId currentStateId);

    /**
     * @brief Check for water tank empty condition and return appropriate state
     * @param context The machine state context
     * @param currentStateId The current state ID for logging
     * @return WaterTankEmptyState if tank is empty and not already in error state, nullptr otherwise
     */
    std::unique_ptr<MachineState> checkWaterTankEmpty(MachineStateContext& context, MachineStateId currentStateId);

    /**
     * @brief Perform common safety transition checks in priority order
     * @param context The machine state context
     * @param currentStateId The current state ID for logging
     * @return New state if safety condition triggered, nullptr otherwise
     * 
     * Checks in order:
     * 1. Emergency stop (highest priority)
     * 2. Sensor errors 
     * 3. Water tank empty (unless already in error state)
     */
    std::unique_ptr<MachineState> checkCommonSafetyTransitions(MachineStateContext& context, MachineStateId currentStateId);

    /**
     * @brief Get normal operation state based on PID setting
     * @param context The machine state context
     * @param currentStateId The current state ID for logging
     * @return PidNormalState if PID enabled, PidDisabledState otherwise
     */
    std::unique_ptr<MachineState> getNormalOperationState(MachineStateContext& context, MachineStateId currentStateId);
}