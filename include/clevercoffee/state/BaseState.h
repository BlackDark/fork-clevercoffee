/**
 * @file BaseState.h
 * @brief Template base class for state machine implementation with CRTP
 */

#pragma once

#include <memory>
#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/state/StateInfo.h"

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
    std::unique_ptr<MachineState> checkTransitions(MachineStateContext& context) override;

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
    virtual std::unique_ptr<MachineState> checkSpecificTransitions(MachineStateContext& context) = 0;


};
