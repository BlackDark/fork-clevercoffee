/**
 * @file StateMachine.cpp
 * @brief Implementation of StateMachine controller
 */

#include "clevercoffee/state/StateMachine.h"

#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/StateFactory.h"

#include <Arduino.h>
#include <chrono>

StateMachine::StateMachine(CleverCoffee::SystemContext& systemContext,
                           CleverCoffee::HardwareManager& hardwareManager,
                           DisplayManager&               displayManager,
                           CleverCoffeeWiFiManager&      wifiManager,
                           MQTTManager&                  mqttManager)
    : currentState_(nullptr),  // Will be set in initialize() - temporary, will always have a state after init
      context_(systemContext, hardwareManager, displayManager, wifiManager, mqttManager),
      initialized_(false), lastStateId_(MachineStateId::INIT), lastUpdateTime_(std::chrono::steady_clock::now()),
      startTime_(std::chrono::steady_clock::now()), totalStateTransitions_(0), totalUpdates_(0) {
    LOG(INFO, "StateMachine created");
}

void StateMachine::initialize(MachineStateId initialStateId) {
    LOG(INFO, "Initializing StateMachine");

    // Create initial state - always succeeds (uses fallback if needed)
    auto initialState = createStateInstance(initialStateId);
    if (!initialState) {
        // Fallback to INIT state if creation failed
        LOG(WARNING, "Failed to create initial state, using INIT as fallback");
        initialState = createStateInstance(MachineStateId::INIT);
        if (!initialState) {
            // This should never happen, but if it does, we're in serious trouble
            LOGF(FATAL, "CRITICAL: Cannot create INIT state. System will restart.");
            ESP.restart();
            return;  // Unreachable
        }
        initialStateId = MachineStateId::INIT;
    }

    // Store initial state (StateMachine always has a valid state)
    currentState_ = std::move(initialState);

    // Initialize timing
    auto now        = std::chrono::steady_clock::now();
    lastUpdateTime_ = now;
    startTime_      = now;
    lastStateId_    = currentState_->getStateId();

    // Update context with initial state entry time
    context_.updateStateEntryTime(now);

    // Call state entry callback
    LOG(INFO, "StateMachine entering initial state");
    currentState_->onEntry(context_);

    initialized_           = true;
    totalStateTransitions_ = 1; // Count initial state as first transition

    LOGF(INFO,
         "StateMachine initialized in state %d (%s)",
         static_cast<int>(getCurrentStateId()),
         getCurrentStateName());
}

void StateMachine::update() {
    if (!initialized_) {
        LOG(WARNING, "StateMachine::update() called but not initialized");
        return;
    }

    totalUpdates_++;
    auto currentTime = std::chrono::steady_clock::now();

    // Update current state
    currentState_->update(context_);

    // Check for state transitions - states return optional<MachineStateId>
    if (auto newStateId = currentState_->checkTransitions(context_)) {
        executeTransition(newStateId.value(), "State transition");
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

void StateMachine::transitionTo(MachineStateId newStateId, const char* reason) {
    LOGF(INFO, "Forced state transition: %s", reason ? reason : "External trigger");
    executeTransition(newStateId, reason);
}

void StateMachine::executeTransition(MachineStateId newStateId, const char* reason) {
    // Prevent self-transition
    if (currentState_->getStateId() == newStateId) {
        LOG(DEBUG, "Skipping self-transition");
        return;
    }

    // Log transition
    const MachineStateId oldStateId   = currentState_->getStateId();
    const char*          oldStateName = currentState_->getStateName();

    // Create new state instance
    auto newState = createStateInstance(newStateId);
    if (!newState) {
        LOG(ERROR, "Failed to create new state instance");
        return;
    }

    const char* newStateName = newState->getStateName();

    LOGF(INFO,
         "State transition: %d (%s) -> %d (%s) [%s]",
         static_cast<int>(oldStateId),
         oldStateName,
         static_cast<int>(newStateId),
         newStateName,
         reason ? reason : "State logic");

    context_.logStateTransition(oldStateId, newStateId, reason);

    // Call exit callback on current state
    currentState_->onExit(context_);

    // Transition to new state (unique_ptr handles old instance deletion)
    currentState_ = std::move(newState);
    auto now      = std::chrono::steady_clock::now();
    totalStateTransitions_++;

    // Update context with state entry time
    context_.updateStateEntryTime(now);

    // Call entry callback on new state
    currentState_->onEntry(context_);
}

MachineStateId StateMachine::getCurrentStateId() const noexcept {
    return currentState_->getStateId();
}

const char* StateMachine::getCurrentStateName() const noexcept {
    return currentState_->getStateName();
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
    LOGF(DEBUG,
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
