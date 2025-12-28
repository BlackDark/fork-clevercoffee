/**
 * @file StateMachine.cpp
 * @brief Implementation of StateMachine controller
 */

#include "clevercoffee/state/StateMachine.h"

#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/StateFactory.h"
#include "clevercoffee/state/states/BackflushStates.h"
#include "clevercoffee/state/states/BrewStates.h"
#include "clevercoffee/state/states/EmergencyStopState.h"
#include "clevercoffee/state/states/ErrorStates.h"
#include "clevercoffee/state/states/HotWaterStates.h"
#include "clevercoffee/state/states/InitState.h"
#include "clevercoffee/state/states/PidStates.h"
#include "clevercoffee/state/states/SteamStates.h"
#include "clevercoffee/state/states/SystemStates.h"

#include <Arduino.h>
#include <chrono>

StateMachine::StateMachine(CleverCoffee::SystemContext& systemContext,
                           DisplayManager*               displayManager,
                           CleverCoffee::HardwareManager*              hardwareManager,
                           CleverCoffeeWiFiManager*      wifiManager,
                           MQTTManager*                  mqttManager)
    : currentState_(*getStateInstance(MachineStateId::INIT)),  // Temporary initialization, will be replaced in initialize()
      context_(systemContext, displayManager, hardwareManager, wifiManager, mqttManager),
      initialized_(false), lastStateId_(MachineStateId::INIT), lastUpdateTime_(std::chrono::steady_clock::now()),
      startTime_(std::chrono::steady_clock::now()), totalStateTransitions_(0), totalUpdates_(0) {
    LOG(INFO, "StateMachine created");
}

bool StateMachine::initialize(MachineState* initialState) {
    LOG(INFO, "Initializing StateMachine");

    // Use InitState as default initial state if none provided
    if (!initialState) {
        initialState = getStateInstance(MachineStateId::INIT);
    }

    // Validate that we have a valid state
    if (!initialState) {
        LOG(ERROR, "Failed to get initial state");
        initialized_ = false;
        return false;
    }

    // Set initial state using reference_wrapper
    currentState_ = *initialState;

    // Initialize timing
    auto now        = std::chrono::steady_clock::now();
    lastUpdateTime_ = now;
    startTime_      = now;
    lastStateId_    = currentState_.get().getStateId();

    // Update context with initial state entry time
    context_.updateStateEntryTime(now);

    // Call state entry callback
    LOG(INFO, "StateMachine entering initial state");
    currentState_.get().onEntry(context_);

    initialized_           = true;
    totalStateTransitions_ = 1; // Count initial state as first transition

    LOGF(INFO,
         "StateMachine initialized in state %d (%s)",
         static_cast<int>(getCurrentStateId()),
         getCurrentStateName());

    return true;
}

void StateMachine::update() {
    if (!initialized_) {
        LOG(WARNING, "StateMachine::update() called but not initialized");
        return;
    }

    totalUpdates_++;
    auto currentTime = std::chrono::steady_clock::now();

    // Update current state
    currentState_.get().update(context_);

    // Check for state transitions
    if (auto newState = currentState_.get().checkTransitions(context_)) {
        executeTransition(*newState, "State transition");
    }

    lastUpdateTime_ = currentTime;

    // Periodic logging for debugging (every 10 seconds when state changes or first run)
    static auto           lastLogTime  = std::chrono::steady_clock::now();
    static constexpr auto LOG_INTERVAL = std::chrono::seconds(10);

    if ((currentTime - lastLogTime) > LOG_INTERVAL || lastStateId_ != getCurrentStateId()) {
        logStateMachineStatus();
        lastLogTime  = currentTime;
        lastStateId_ = getCurrentStateId();
    }
}

void StateMachine::transitionTo(MachineState& newState, const char* reason) {
    LOGF(INFO, "Forced state transition: %s", reason ? reason : "External trigger");
    executeTransition(newState, reason);
}

void StateMachine::executeTransition(MachineState& newState, const char* reason) {
    // Prevent self-transition
    if (&currentState_.get() == &newState) {
        LOG(DEBUG, "Skipping self-transition");
        return;
    }

    // Log transition
    const MachineStateId oldStateId   = currentState_.get().getStateId();
    const char*          oldStateName = currentState_.get().getStateName();
    const MachineStateId newStateId   = newState.getStateId();
    const char*          newStateName = newState.getStateName();

    LOGF(INFO,
         "State transition: %d (%s) -> %d (%s) [%s]",
         static_cast<int>(oldStateId),
         oldStateName,
         static_cast<int>(newStateId),
         newStateName,
         reason ? reason : "State logic");

    context_.logStateTransition(oldStateId, newStateId, reason);

    // Call exit callback on current state
    currentState_.get().onExit(context_);

    // Transition to new state (singleton - no ownership transfer)
    currentState_ = newState;
    auto now      = std::chrono::steady_clock::now();
    totalStateTransitions_++;

    // Update context with state entry time
    context_.updateStateEntryTime(now);

    // Call entry callback on new state
    currentState_.get().onEntry(context_);
}

MachineStateId StateMachine::getCurrentStateId() const noexcept {
    return currentState_.get().getStateId();
}

const char* StateMachine::getCurrentStateName() const noexcept {
    return currentState_.get().getStateName();
}

bool StateMachine::isInitialized() const noexcept {
    return initialized_;
}

void StateMachine::logStateMachineStatus() const {
    if (!isInitialized()) {
        LOG(WARNING, "StateMachine status: Not initialized");
        return;
    }
    auto now         = std::chrono::steady_clock::now();
    auto timeInState = std::chrono::milliseconds(context_.getStateElapsedTimeMs());
    auto uptime      = std::chrono::duration_cast<std::chrono::seconds>(now - startTime_);
    LOGF(INFO,
         "StateMachine status: State=%d (%s), TimeInState=%lldms, "
         "Transitions=%zu, Updates=%zu, Uptime=%llds",
         static_cast<int>(getCurrentStateId()),
         getCurrentStateName(),
         timeInState.count(),
         totalStateTransitions_,
         totalUpdates_,
         uptime.count());

    // Log context status for debugging
    LOGF(DEBUG,
         "Context status: Temp=%.1f°C, Tank=%s, Sensors=%s, PID=%s",
         context_.getCurrentTemperature(),
         context_.isWaterTankFull() ? "Full" : "Empty",
         context_.hasSensorError() ? "Error" : "OK",
         context_.isPidEnabled() ? "On" : "Off");
}
