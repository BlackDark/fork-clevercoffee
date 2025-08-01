/**
 * @file StateMachine.cpp
 * @brief Implementation of StateMachine controller
 */

#include "StateMachine.h"
#include "states/InitState.h"
#include "MachineStateIds.h"
#include "Logger.h"
#include <Arduino.h>

StateMachine::StateMachine(
    DisplayManager* displayManager,
    HardwareManager* hardwareManager,
    SensorManager* sensorManager,
    CleverCoffeeWiFiManager* wifiManager,
    MQTTManager* mqttManager
) : currentState_(nullptr),
    context_(displayManager, hardwareManager, sensorManager, wifiManager, mqttManager),
    initialized_(false),
    lastStateId_(-1),
    lastUpdateTime_(0),
    stateEntryTime_(0),
    totalStateTransitions_(0),
    totalUpdates_(0) {
    
    LOG(INFO, "StateMachine created");
}

bool StateMachine::initialize(std::unique_ptr<MachineState> initialState) {
    LOG(INFO, "Initializing StateMachine");
    
    // Use InitState as default initial state if none provided
    if (!initialState) {
        initialState = std::make_unique<InitState>();
    }
    
    // Set initial state
    currentState_ = std::move(initialState);
    
    if (!currentState_) {
        LOG(ERROR, "Failed to create initial state");
        initialized_ = false;
        return false;
    }
    
    // Initialize timing
    lastUpdateTime_ = millis();
    stateEntryTime_ = lastUpdateTime_;
    lastStateId_ = currentState_->getStateId();
    
    // Call state entry callback
    LOG(INFO, "StateMachine entering initial state");
    currentState_->onEntry(context_);
    
    initialized_ = true;
    totalStateTransitions_ = 1; // Count initial state as first transition
    
    LOGF(INFO, "StateMachine initialized in state %d (%s)", 
         getCurrentStateId(), getCurrentStateName());
    
    return true;
}

void StateMachine::update() {
    if (!initialized_ || !currentState_) {
        LOG(WARNING, "StateMachine::update() called but not initialized");
        return;
    }
    
    totalUpdates_++;
    unsigned long currentTime = millis();
    
    // Update current state
    currentState_->update(context_);
    
    // Check for state transitions
    if (auto newState = currentState_->checkTransitions(context_)) {
        executeTransition(std::move(newState), "State transition");
    }
    
    lastUpdateTime_ = currentTime;
    
    // Periodic logging for debugging (every 10 seconds when state changes or first run)
    static unsigned long lastLogTime = 0;
    if (currentTime - lastLogTime > 10000 || lastStateId_ != getCurrentStateId()) {
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
    const int oldStateId = currentState_ ? currentState_->getStateId() : -1;
    const char* oldStateName = currentState_ ? currentState_->getStateName() : "None";
    const int newStateId = newState->getStateId();
    const char* newStateName = newState->getStateName();
    
    LOGF(INFO, "State transition: %d (%s) -> %d (%s) [%s]", 
         oldStateId, oldStateName, newStateId, newStateName,
         reason ? reason : "State logic");
    
    context_.logStateTransition(oldStateId, newStateId, reason);
    
    // Call exit callback on current state
    if (currentState_) {
        currentState_->onExit(context_);
    }
    
    // Transition to new state
    currentState_ = std::move(newState);
    stateEntryTime_ = millis();
    totalStateTransitions_++;
    
    // Call entry callback on new state
    if (currentState_) {
        currentState_->onEntry(context_);
    }
}

int StateMachine::getCurrentStateId() const {
    return currentState_ ? currentState_->getStateId() : -1;
}

const char* StateMachine::getCurrentStateName() const {
    return currentState_ ? currentState_->getStateName() : "None";
}

bool StateMachine::isInitialized() const {
    return initialized_ && currentState_ != nullptr;
}

void StateMachine::logStateMachineStatus() const {
    if (!isInitialized()) {
        LOG(WARNING, "StateMachine status: Not initialized");
        return;
    }
    
    unsigned long currentTime = millis();
    unsigned long timeInState = currentTime - stateEntryTime_;
    unsigned long uptime = currentTime; // Assuming Arduino millis() from start
    
    LOGF(INFO, "StateMachine status: State=%d (%s), TimeInState=%lums, "
              "Transitions=%lu, Updates=%lu, Uptime=%lus", 
         getCurrentStateId(), getCurrentStateName(), timeInState,
         totalStateTransitions_, totalUpdates_, uptime / 1000);
    
    // Log context status for debugging
    LOGF(DEBUG, "Context status: Temp=%.1f°C, Tank=%s, Sensors=%s, PID=%s", 
         context_.getCurrentTemperature(),
         context_.isWaterTankFull() ? "Full" : "Empty",
         context_.hasSensorError() ? "Error" : "OK",
         context_.isPidEnabled() ? "On" : "Off");
}