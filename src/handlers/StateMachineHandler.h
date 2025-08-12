/**
 * @file StateMachineHandler.h
 * @brief Generic state machine handler to eliminate code duplication
 */

#pragma once

#include <functional>
#include <unordered_map>

/**
 * @class StateMachineHandler
 * @brief Generic state machine handler that eliminates state management duplication
 * 
 * This class abstracts common state machine patterns found in handlers:
 * - State transitions
 * - State-specific behavior
 * - Debug logging
 * - State validation
 */
template<typename StateType>
class StateMachineHandler {
private:
    StateType& currentState_;
    std::unordered_map<StateType, std::function<StateType()>> stateHandlers_;
    std::function<void(StateType, StateType)> stateChangeCallback_;
    std::function<void(const char*)> debugLogger_;
    
public:
    /**
     * @brief Constructor for state machine handler
     * @param currentState Reference to current state variable
     * @param stateChangeCallback Optional callback for state changes
     * @param debugLogger Optional debug logging function
     */
    StateMachineHandler(StateType& currentState,
                       std::function<void(StateType, StateType)> stateChangeCallback = nullptr,
                       std::function<void(const char*)> debugLogger = nullptr)
        : currentState_(currentState)
        , stateChangeCallback_(stateChangeCallback) 
        , debugLogger_(debugLogger) {}
    
    /**
     * @brief Register a state handler function
     * @param state The state to handle
     * @param handler Function that returns the next state
     */
    void registerStateHandler(StateType state, std::function<StateType()> handler) {
        stateHandlers_[state] = handler;
    }
    
    /**
     * @brief Process current state and handle transitions
     */
    void processStateMachine() {
        auto it = stateHandlers_.find(currentState_);
        if (it != stateHandlers_.end()) {
            StateType nextState = it->second();
            
            if (nextState != currentState_) {
                transitionToState(nextState);
            }
        }
    }
    
    /**
     * @brief Get current state
     */
    StateType getCurrentState() const {
        return currentState_;
    }
    
    /**
     * @brief Check if in specific state
     */
    bool isInState(StateType state) const {
        return currentState_ == state;
    }
    
    /**
     * @brief Check if in any of the specified states
     */
    template<typename... States>
    bool isInAnyState(States... states) const {
        return ((currentState_ == states) || ...);
    }
    
private:
    /**
     * @brief Transition to new state with optional callbacks
     */
    void transitionToState(StateType newState) {
        StateType oldState = currentState_;
        currentState_ = newState;
        
        if (debugLogger_) {
            // Create debug message (would need state-to-string conversion)
            debugLogger_("State transition occurred");
        }
        
        if (stateChangeCallback_) {
            stateChangeCallback_(oldState, newState);
        }
    }
};

// Removed createHotWaterStateMachine - specific implementations are now in individual handlers