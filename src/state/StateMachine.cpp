/**
 * @file StateMachine.cpp
 * @brief Implementation of StateMachine controller
 */

#include "clevercoffee/state/StateMachine.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/StateFactory.h"
#include "clevercoffee/state/states/AllStates.h"
#include <Arduino.h>
#include <chrono>

StateMachine::StateMachine(DisplayManager* displayManager, HardwareManager* hardwareManager, SensorManager* sensorManager, CleverCoffeeWiFiManager* wifiManager, MQTTManager* mqttManager) :
    currentState_(nullptr),
    context_(displayManager, hardwareManager, sensorManager, wifiManager, mqttManager),
    initialized_(false),
    lastStateId_(MachineStateId::INIT),
    lastUpdateTime_(std::chrono::steady_clock::now()),
    stateEntryTime_(std::chrono::steady_clock::now()),
    startTime_(std::chrono::steady_clock::now()), // <-- Add this line
    totalStateTransitions_(0),
    totalUpdates_(0) {

    LOG(INFO, "StateMachine created");
}

bool StateMachine::initialize(std::unique_ptr<MachineState> initialState) {
    LOG(INFO, "Initializing StateMachine");

    // Use InitState as default initial state if none provided
    if (!initialState) {
        initialState = createState(MachineStateId::INIT);
    }

    // Set initial state
    currentState_ = std::move(initialState);

    if (!currentState_) {
        LOG(ERROR, "Failed to create initial state");
        initialized_ = false;
        return false;
    }

    // Initialize timing
    auto now = std::chrono::steady_clock::now();
    lastUpdateTime_ = now;
    stateEntryTime_ = now;
    startTime_ = now; // <-- Set start time on initialize
    lastStateId_ = currentState_->getStateId();

    // Call state entry callback
    LOG(INFO, "StateMachine entering initial state");
    currentState_->onEntry(context_);

    initialized_ = true;
    totalStateTransitions_ = 1; // Count initial state as first transition

    LOGF(INFO, "StateMachine initialized in state %d (%s)", static_cast<int>(getCurrentStateId()), getCurrentStateName());

    return true;
}

void StateMachine::update() {
    if (!initialized_ || !currentState_) {
        LOG(WARNING, "StateMachine::update() called but not initialized");
        return;
    }

    totalUpdates_++;
    auto currentTime = std::chrono::steady_clock::now();

    // Update current state
    currentState_->update(context_);

    // Check for state transitions
    if (auto newState = currentState_->checkTransitions(context_)) {
        executeTransition(std::move(newState), "State transition");
    }

    lastUpdateTime_ = currentTime;

    // Periodic logging for debugging (every 10 seconds when state changes or first run)
    static auto lastLogTime = std::chrono::steady_clock::now();
    static constexpr auto LOG_INTERVAL = std::chrono::seconds(10);

    if ((currentTime - lastLogTime) > LOG_INTERVAL || lastStateId_ != getCurrentStateId()) {
        logStateMachineStatus();
        lastLogTime = currentTime;
        lastStateId_ = getCurrentStateId();
    }
}

void StateMachine::transitionTo(std::unique_ptr<MachineState> newState, const char* reason) {
    if (!newState) {
        LOG(ERROR, "StateMachine::transitionTo() called with null state");
        return;
    }

    LOGF(INFO, "Forced state transition: %s", reason ? reason : "External trigger");
    executeTransition(std::move(newState), reason);
}

void StateMachine::executeTransition(std::unique_ptr<MachineState> newState, const char* reason) {
    if (!newState) {
        LOG(ERROR, "executeTransition called with null state");
        return;
    }

    // Log transition
    const MachineStateId oldStateId = currentState_ ? currentState_->getStateId() : MachineStateId::INIT;
    const char* oldStateName = currentState_ ? currentState_->getStateName() : "None";
    const MachineStateId newStateId = newState->getStateId();
    const char* newStateName = newState->getStateName();

    LOGF(INFO, "State transition: %d (%s) -> %d (%s) [%s]", static_cast<int>(oldStateId), oldStateName, static_cast<int>(newStateId), newStateName, reason ? reason : "State logic");

    context_.logStateTransition(oldStateId, newStateId, reason);

    // Call exit callback on current state
    if (currentState_) {
        currentState_->onExit(context_);
    }

    // Transition to new state
    currentState_ = std::move(newState);
    stateEntryTime_ = std::chrono::steady_clock::now();
    totalStateTransitions_++;

    // Call entry callback on new state
    if (currentState_) {
        currentState_->onEntry(context_);
    }
}

MachineStateId StateMachine::getCurrentStateId() const noexcept {
    return currentState_ ? currentState_->getStateId() : MachineStateId::INIT;
}

const char* StateMachine::getCurrentStateName() const noexcept {
    return currentState_ ? currentState_->getStateName() : "None";
}

bool StateMachine::isInitialized() const noexcept {
    return initialized_ && currentState_ != nullptr;
}

void StateMachine::logStateMachineStatus() const {
    if (!isInitialized()) {
        LOG(WARNING, "StateMachine status: Not initialized");
        return;
    }
    auto now = std::chrono::steady_clock::now();
    auto timeInState = std::chrono::duration_cast<std::chrono::milliseconds>(now - stateEntryTime_);
    auto uptime = std::chrono::duration_cast<std::chrono::seconds>(now - startTime_); // <-- Use startTime_ for uptime
    LOGF(INFO,
         "StateMachine status: State=%d (%s), TimeInState=%lldms, "
         "Transitions=%zu, Updates=%zu, Uptime=%llds",
         static_cast<int>(getCurrentStateId()), getCurrentStateName(), timeInState.count(), totalStateTransitions_, totalUpdates_, uptime.count());

    // Log context status for debugging
    LOGF(DEBUG, "Context status: Temp=%.1f°C, Tank=%s, Sensors=%s, PID=%s", context_.getCurrentTemperature(), context_.isWaterTankFull() ? "Full" : "Empty", context_.hasSensorError() ? "Error" : "OK",
         context_.isPidEnabled() ? "On" : "Off");
}
