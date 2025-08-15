/**
 * @file BaseState.h
 * @brief Template base class for state machine implementation with CRTP
 */

#pragma once

#include <memory>
#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateTransitionHelper.h"
#include "clevercoffee/Logger.h"

// Forward declarations for common state types - include will be in .cpp file
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
template<MachineStateId StateId, typename DerivedState>
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
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override {
        // Handle common safety transitions first using StateTransitionHelper
        // (but only for non-safety states to avoid circular transitions)
        if constexpr (StateId != MachineStateId::EMERGENCY_STOP &&
                     StateId != MachineStateId::SENSOR_ERROR &&
                     StateId != MachineStateId::WATER_TANK_EMPTY &&
                     StateId != MachineStateId::EEPROM_ERROR) {
            if (auto safetyState = checkCommonSafetyTransitions(context)) {
                return safetyState;
            }
        }

        // Let derived class handle specific transitions
        return checkSpecificTransitions(context);
    }

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
        return DerivedState::STATE_NAME;
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
    virtual std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) = 0;

private:
    /**
     * @brief Check common safety transitions - inline implementation to avoid linking issues
     */
    std::unique_ptr<MachineState> checkCommonSafetyTransitions(MachineStateContext& context) {
        // Forward to StateTransitionHelper to avoid circular dependencies
        return StateTransitionHelper::checkCommonSafetyTransitions(context, getStateId());
    }
};

