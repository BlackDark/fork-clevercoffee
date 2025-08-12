/**
 * @file ModernHotWaterHandler.h
 * @brief Refactored hot water handler using abstractions
 */

#pragma once

#include "BaseHandler.h"
#include "StateMachineHandler.h"
#include "../hotWaterHandler.h" // For enum definitions
#include "../Config.h"

/**
 * @class ModernHotWaterHandler
 * @brief Hot water handler using modern base class and state machine abstractions
 * 
 * This demonstrates how abstractions eliminate duplication and improve maintainability
 */
class ModernHotWaterHandler : public SwitchBasedHandler {
private:
    StateMachineHandler<HotWaterState> stateMachine_;
    unsigned long pumpStartTime_;
    double currentPumpOnTime_;
    
public:
    ModernHotWaterHandler() 
        : SwitchBasedHandler("HotWaterHandler", g_state.hardware.hotWaterSwitch)
        , stateMachine_(currHotWaterState)
        , pumpStartTime_(0)
        , currentPumpOnTime_(0) {
        
        initializeStateMachine();
    }
    
    /**
     * @brief Check if hot water is currently running
     */
    bool isHotWaterRunning() const {
        return stateMachine_.isInState(kHotWaterRunning);
    }
    
    /**
     * @brief Get current pump on time
     */
    double getCurrentPumpOnTime() const {
        return currentPumpOnTime_;
    }
    
protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesHotWaterEnabled.get();
    }
    
    void processImpl() override {
        stateMachine_.processStateMachine();
        updatePumpTiming();
    }
    
private:
    void initializeStateMachine() {
        // Idle state: waiting for switch press
        stateMachine_.registerStateHandler(kHotWaterIdle, [this]() -> HotWaterState {
            const uint8_t reading = getSwitchReading();
            
            if (reading == HIGH) {
                logInfo("Hot water activation requested");
                startPump();
                return kHotWaterRunning;
            }
            
            return kHotWaterIdle;
        });
        
        // Running state: pump is active
        stateMachine_.registerStateHandler(kHotWaterRunning, [this]() -> HotWaterState {
            const uint8_t reading = getSwitchReading();
            
            // Stop if switch released or timeout reached
            if (reading == LOW || hasTimedOut()) {
                logInfo("Hot water stopping");
                stopPump();
                return kHotWaterStopped;
            }
            
            return kHotWaterRunning;
        });
        
        // Stopped state: cleanup and return to idle
        stateMachine_.registerStateHandler(kHotWaterStopped, [this]() -> HotWaterState {
            // Brief pause before returning to idle
            delay(100);
            return kHotWaterIdle;
        });
    }
    
    void startPump() {
        setRelayState(g_state.hardware.pumpRelay, true);
        pumpStartTime_ = millis();
        logDebug("Pump started");
    }
    
    void stopPump() {
        setRelayState(g_state.hardware.pumpRelay, false);
        
        if (pumpStartTime_ > 0) {
            currentPumpOnTime_ += (millis() - pumpStartTime_) / 1000.0;
            pumpStartTime_ = 0;
        }
        
        logDebug("Pump stopped");
    }
    
    bool hasTimedOut() const {
        if (pumpStartTime_ == 0) return false;
        
        const unsigned long maxRunTime = Config::getInstance().hotWaterMaxRunTime.get() * 1000UL;
        return (millis() - pumpStartTime_) > maxRunTime;
    }
    
    void updatePumpTiming() {
        if (isHotWaterRunning() && pumpStartTime_ > 0) {
            // Update current runtime for display purposes
            double runtime = (millis() - pumpStartTime_) / 1000.0;
            // Could update global state or trigger callbacks here
        }
    }
};

/**
 * @brief Global instance for compatibility
 */
inline ModernHotWaterHandler g_modernHotWaterHandler;

/**
 * @brief Modern process function
 */
inline void processHotWaterModern() {
    g_modernHotWaterHandler.process();
}

/**
 * @brief Check if hot water is active (compatibility function)
 */
inline bool checkHotWaterStatesModern() {
    return g_modernHotWaterHandler.isHotWaterRunning();
}