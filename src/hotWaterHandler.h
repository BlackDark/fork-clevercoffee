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
#include "brewStates.h"

/**
 * @class HotWaterHandler  
 * @brief Hot water handler using modern abstractions
 */
class HotWaterHandler : public SwitchBasedHandler {
private:
    StateMachineHandler<HotWaterState> stateMachine_;
    HandlerUtils::PumpTimer pumpTimer_;
    uint8_t lastSwitchReading_ = LOW;
    
public:
    HotWaterHandler() 
        : SwitchBasedHandler("HotWaterHandler", g_state.hardware.hotWaterSwitch)
        , stateMachine_(g_state.sensors.currHotWaterState)
        , pumpTimer_(60000) { // 60 second max run time
        
        initializeStateMachine();
    }
    
    bool isHotWaterActive() const {
        return g_state.sensors.currHotWaterState == kHotWaterRunning;
    }
    
    bool checkHotWaterStates() const {
        return isHotWaterActive();
    }
    
    bool checkHotWaterActive() const {
        return (g_state.machine.machineState == LegacyMachineState::kHotWater || 
                (g_state.machine.machineState == LegacyMachineState::kSteam && isHotWaterActive()));
    }
    
    double getCurrentPumpOnTime() const {
        return g_state.sensors.currPumpOnTime;
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
            if (g_state.sensors.currHotWaterSwitchState == kHotWaterSwitchShortPressed) {
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
        switch (g_state.sensors.currHotWaterSwitchState) {
            case kHotWaterSwitchIdle:
                if (reading == HIGH) {
                    g_state.sensors.currHotWaterSwitchState = kHotWaterSwitchShortPressed;
                    logDebug("Toggle switch ON -> ShortPressed");
                }
                break;
                
            case kHotWaterSwitchShortPressed:
                if (reading == LOW) {
                    g_state.sensors.currHotWaterSwitchState = kHotWaterSwitchIdle;
                    logDebug("Toggle switch OFF -> Idle");
                } else if (g_state.sensors.currHotWaterState == kHotWaterStopped) {
                    g_state.sensors.currHotWaterSwitchState = kHotWaterSwitchWaitForRelease;
                    logDebug("Hot water stopped -> WaitForRelease");
                }
                break;
                
            case kHotWaterSwitchWaitForRelease:
                if (reading == LOW) {
                    g_state.sensors.currHotWaterSwitchState = kHotWaterSwitchIdle;
                    logDebug("Switch released -> Idle");
                }
                break;
                
            default:
                g_state.sensors.currHotWaterSwitchState = kHotWaterSwitchIdle;
                break;
        }
    }
    
    void processMomentarySwitchState(uint8_t reading) {
        switch (g_state.sensors.currHotWaterSwitchState) {
            case kHotWaterSwitchIdle:
                if (reading == HIGH && lastSwitchReading_ == LOW) {
                    g_state.sensors.currHotWaterSwitchState = kHotWaterSwitchPressed;
                    logDebug("Momentary press detected");
                }
                break;
                
            case kHotWaterSwitchPressed:
                if (reading == LOW) {
                    g_state.sensors.currHotWaterSwitchState = kHotWaterSwitchShortPressed;
                    logDebug("Short press confirmed");
                }
                break;
                
            case kHotWaterSwitchShortPressed:
                // Action handled by state machine
                g_state.sensors.currHotWaterSwitchState = kHotWaterSwitchIdle;
                break;
                
            default:
                g_state.sensors.currHotWaterSwitchState = kHotWaterSwitchIdle;
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
        if (g_state.sensors.currHotWaterSwitchState == kHotWaterSwitchIdle && 
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