/**
 * @file BaseState.h
 * @brief Template base class for state machine implementation with CRTP
 */

#pragma once

#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/StateFactory.h"
#include "clevercoffee/state/StateInfo.h"

#include <memory>

// Forward declarations for common state types
class EmergencyStopState;
class SensorErrorState;
class WaterTankEmptyState;
class EepromErrorState;

/**
 * @class BaseState
 * @brief CRTP template base class that eliminates boilerplate code for state implementations
 *
 * This template class implements the common patterns found across all machine states:
 * - Entry/exit logging
 * - Common transition checking (emergency, errors, etc.)
 * - State ID and name management
 *
 * @tparam StateId The MachineStateId enum value for this state
 * @tparam DerivedState The concrete state class deriving from this template
 */
template <MachineStateId StateId, typename DerivedState>
class BaseState : public MachineState {
  public:
    /**
     * @brief Called when entering this state - handles logging and delegates to derived class
     */
    void onEntry(MachineStateContext& context) override {
        context.logStateEntry(getStateId(), getStateName());
        onEntryImpl(context);
    }

    /**
     * @brief Called when exiting this state - handles logging and delegates to derived class
     */
    void onExit(MachineStateContext& context) override {
        context.logStateExit(getStateId(), getStateName());
        onExitImpl(context);
    }

    /**
     * @brief Check for state transitions - handles common safety checks then delegates to derived class
     */
    MachineState* checkTransitions(MachineStateContext& context) override;

    /**
     * @brief Get the state ID
     */
    MachineStateId getStateId() const override {
        return StateId;
    }

    /**
     * @brief Get human-readable state name
     */
    const char* getStateName() const override {
        if (const auto* info = getStateInfo(StateId)) {
            return info->name;
        }
        return "Unknown";
    }

    /**
     * @brief Override in derived class for custom entry behavior
     * Default implementation does nothing
     */
    virtual void onEntryImpl(MachineStateContext& context) {
        // Default: no additional entry behavior
    }

    /**
     * @brief Override in derived class for custom exit behavior
     * Default implementation does nothing
     */
    virtual void onExitImpl(MachineStateContext& context) {
        // Default: no additional exit behavior
    }

    /**
     * @brief Must be implemented by derived class for state-specific transitions
     * @return New state to transition to, or nullptr if no transition
     */
    virtual MachineState* checkSpecificTransitions(MachineStateContext& context) = 0;

protected:
    /**
     * @brief Helper: Check if brew stop was requested and transition to PID state
     * @param context Machine state context
     * @return New state if brew stop requested, nullptr otherwise
     */
    MachineState* checkBrewStopRequest(MachineStateContext& context) {
        if (context.isBrewStopRequested()) {
            context.setBrewStopRequested(false);
            const MachineStateId pidState = context.getPidState();
            context.logStateTransition(getStateId(), pidState, "Brew stop requested");
            return getStateInstance(pidState);
        }
        return nullptr;
    }

    /**
     * @brief Helper: Transition to PID state (normal or disabled based on PID enabled state)
     * @param context Machine state context
     * @param reason Reason for transition (for logging)
     * @return New PID state instance
     */
    MachineState* transitionToPidState(MachineStateContext& context, const char* reason) {
        const MachineStateId pidState = context.getPidState();
        context.logStateTransition(getStateId(), pidState, reason);
        return getStateInstance(pidState);
    }

    /**
     * @brief Helper: Clean up pump and valve hardware on state exit
     * @param context Machine state context
     */
    void cleanupPumpAndValve(MachineStateContext& context) {
        context.disablePump();
        context.closeWaterValve();
    }
};

// Template implementation
template <MachineStateId StateId, typename DerivedState>
MachineState* BaseState<StateId, DerivedState>::checkTransitions(MachineStateContext& context) {
    // Emergency stop check - highest priority
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop triggered");
        return getStateInstance(MachineStateId::EMERGENCY_STOP);
    }

    // Critical error checks
    if (context.hasSensorError()) {
        context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error detected");
        return getStateInstance(MachineStateId::SENSOR_ERROR);
    }

    if (!context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateId::WATER_TANK_EMPTY, "Water tank empty");
        return getStateInstance(MachineStateId::WATER_TANK_EMPTY);
    }

    // Delegate to derived class for state-specific transitions
    return checkSpecificTransitions(context);
}
