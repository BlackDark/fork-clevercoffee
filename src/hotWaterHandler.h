/**
 * @file hotWaterHandler.h
 * @brief Handler for hot water switch using modern abstractions
 */

#pragma once

#include "handlers/BaseHandler.h"
#include "handlers/StateMachineHandler.h"
#include "handlers/HandlerUtils.h"
#include "Config.h"
#include "state/MachineState.h"

// Hot water states
enum HotWaterState {
    kHotWaterIdle = 10,
    kHotWaterRunning = 20,
    kHotWaterStopped = 30,
};

// Switch states  
enum HotWaterSwitchState {
    kHotWaterSwitchIdle = 10,
    kHotWaterSwitchPressed = 20,
    kHotWaterSwitchShortPressed = 30,
    kHotWaterSwitchLongPressed = 40,
    kHotWaterSwitchWaitForRelease = 50
};

/**
 * @class HotWaterHandler  
 * @brief Hot water handler using modern abstractions
 */
class HotWaterHandler : public SwitchBasedHandler {
private:
    StateMachineHandler<HotWaterState> stateMachine_;
    HandlerUtils::PumpTimer pumpTimer_;
    HotWaterState currentHotWaterState_;
    HotWaterSwitchState currentSwitchState_;
    uint8_t lastSwitchReading_;
    
public:
    HotWaterHandler() 
        : SwitchBasedHandler("HotWaterHandler", g_state.hardware.hotWaterSwitch)
        , stateMachine_(currentHotWaterState_)
        , pumpTimer_(60000) // 60 second max run time
        , currentHotWaterState_(kHotWaterIdle)
        , currentSwitchState_(kHotWaterSwitchIdle)
        , lastSwitchReading_(LOW) {
        
        initializeStateMachine();
    }
    
    bool isHotWaterActive() const {
        return currentHotWaterState_ == kHotWaterRunning;
    }
    
    bool checkHotWaterStates() const {
        return isHotWaterActive();
    }
    
    bool checkHotWaterActive() const {
        return (g_state.machine.machineState == LegacyMachineState::kHotWater || 
                (g_state.machine.machineState == LegacyMachineState::kSteam && isHotWaterActive()));
    }
    
protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesHotWaterEnabled.get();
    }
    
    bool hasPermission() const override {
        if (!SwitchBasedHandler::hasPermission()) {
            return false;
        }
        
        // Block when water tank is empty
        if (g_state.machine.machineState == LegacyMachineState::kWaterTankEmpty) {
            static bool loggedOnce = false;
            if (!loggedOnce) {
                logError("Hot water blocked: Water tank empty");
                loggedOnce = true;
            }
            return false;
        }
        
        return true;
    }
    
    void processImpl() override {
        processSwitchInput();
        stateMachine_.processStateMachine();
        
        // Safety check for pump timeout
        if (pumpTimer_.isExpired()) {
            logError("Hot water pump timeout - stopping for safety");
            stopHotWater();
        }
    }
    
private:
    void initializeStateMachine() {
        // Idle state
        stateMachine_.registerStateHandler(kHotWaterIdle, [this]() -> HotWaterState {
            if (currentSwitchState_ == kHotWaterSwitchShortPressed) {
                startHotWater();
                return kHotWaterRunning;
            }
            return kHotWaterIdle;
        });
        
        // Running state
        stateMachine_.registerStateHandler(kHotWaterRunning, [this]() -> HotWaterState {
            if (shouldStopHotWater()) {
                stopHotWater();
                return kHotWaterStopped;
            }
            return kHotWaterRunning;
        });
        
        // Stopped state
        stateMachine_.registerStateHandler(kHotWaterStopped, [this]() -> HotWaterState {
            // Brief pause then return to idle
            delay(100);
            return kHotWaterIdle;
        });
    }
    
    void processSwitchInput() {
        if (!switch_) return;
        
        const uint8_t reading = getSwitchReading();
        const int switchType = static_cast<int>(Config::getInstance().hardwareSwitchesHotWaterType.get());
        
        if (switchType == Switch::TOGGLE) {
            processToggleSwitchState(reading);
        } else if (switchType == Switch::MOMENTARY) {
            processMomentarySwitchState(reading);
        }
        
        lastSwitchReading_ = reading;
    }
    
    void processToggleSwitchState(uint8_t reading) {
        switch (currentSwitchState_) {
            case kHotWaterSwitchIdle:
                if (reading == HIGH) {
                    currentSwitchState_ = kHotWaterSwitchShortPressed;
                    logDebug("Toggle switch ON -> ShortPressed");
                }
                break;
                
            case kHotWaterSwitchShortPressed:
                if (reading == LOW) {
                    currentSwitchState_ = kHotWaterSwitchIdle;
                    logDebug("Toggle switch OFF -> Idle");
                } else if (currentHotWaterState_ == kHotWaterStopped) {
                    currentSwitchState_ = kHotWaterSwitchWaitForRelease;
                    logDebug("Hot water stopped -> WaitForRelease");
                }
                break;
                
            case kHotWaterSwitchWaitForRelease:
                if (reading == LOW) {
                    currentSwitchState_ = kHotWaterSwitchIdle;
                    logDebug("Switch released -> Idle");
                }
                break;
                
            default:
                currentSwitchState_ = kHotWaterSwitchIdle;
                break;
        }
    }
    
    void processMomentarySwitchState(uint8_t reading) {
        switch (currentSwitchState_) {
            case kHotWaterSwitchIdle:
                if (reading == HIGH && lastSwitchReading_ == LOW) {
                    currentSwitchState_ = kHotWaterSwitchPressed;
                    logDebug("Momentary press detected");
                }
                break;
                
            case kHotWaterSwitchPressed:
                if (reading == LOW) {
                    currentSwitchState_ = kHotWaterSwitchShortPressed;
                    logDebug("Short press confirmed");
                }
                break;
                
            case kHotWaterSwitchShortPressed:
                // Action handled by state machine
                currentSwitchState_ = kHotWaterSwitchIdle;
                break;
                
            default:
                currentSwitchState_ = kHotWaterSwitchIdle;
                break;
        }
    }
    
    void startHotWater() {
        setRelayState(g_state.hardware.pumpRelay, true);
        pumpTimer_.start();
        logInfo("Hot water started");
    }
    
    void stopHotWater() {
        setRelayState(g_state.hardware.pumpRelay, false);
        pumpTimer_.stop();
        logInfo("Hot water stopped");
    }
    
    bool shouldStopHotWater() const {
        // Stop conditions
        if (currentSwitchState_ == kHotWaterSwitchIdle && 
            static_cast<int>(Config::getInstance().hardwareSwitchesHotWaterType.get()) == Switch::TOGGLE) {
            return true;
        }
        
        if (g_state.machine.machineState == LegacyMachineState::kWaterTankEmpty ||
            g_state.machine.machineState == LegacyMachineState::kEmergencyStop ||
            g_state.machine.machineState == LegacyMachineState::kSensorError) {
            return true;
        }
        
        return false;
    }
};

// Global instance and compatibility variables
inline HotWaterHandler g_hotWaterHandler;

// Legacy compatibility variables
inline HotWaterSwitchState currHotWaterSwitchState = kHotWaterSwitchIdle;
inline HotWaterState currHotWaterState = kHotWaterIdle;
inline uint8_t hotWaterSwitchReading = LOW;
inline uint8_t currReadingHotWaterSwitch = LOW;
inline double currPumpOnTime = 0;
inline unsigned long pumpStartingTime = 0;

// Public interface functions
inline void checkHotWaterSwitch() {
    g_hotWaterHandler.process();
}

inline bool checkHotWaterStates() {
    return g_hotWaterHandler.checkHotWaterStates();
}

inline bool checkHotWaterActive() {
    return g_hotWaterHandler.checkHotWaterActive();
}

inline void debugHotWaterState(String state) {
    // Modern handlers use structured logging instead
    LOGF(DEBUG, "[HotWaterHandler] State: %s", state.c_str());
}