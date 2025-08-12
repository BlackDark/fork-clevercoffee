/**
 * @file powerHandler.h
 * @brief Handler for power switch using modern abstractions
 */
#pragma once

#include "handlers/BaseHandler.h"
#include "Config.h"
#include "display/displayCommon.h"
#include "standby.h"
#include "state/GlobalState.h"
#include "state/MachineState.h"
#include "utils/SystemUtils.h"

/**
 * @class PowerHandler
 * @brief Power switch handler using modern base class abstractions
 */
class PowerHandler : public SwitchBasedHandler {
private:
    unsigned long longPressStartTime_;
    bool trackingLongPress_;
    
public:
    PowerHandler() 
        : SwitchBasedHandler("PowerHandler", g_state.hardware.powerSwitch)
        , longPressStartTime_(0)
        , trackingLongPress_(false) {}
    
protected:
    bool isEnabled() const override {
        return Config::getInstance().hardwareSwitchesPowerEnabled.get();
    }
    
    bool hasPermission() const override {
        // Power switch doesn't need standard permission checks
        return true;
    }
    
    void processImpl() override {
        recordSystemInitialization();
        
        const bool powerSwitchPressed = getSwitchReading();
        const int switchType = static_cast<int>(Config::getInstance().hardwareSwitchesPowerType.get());
        
        if (switchType == Switch::TOGGLE) {
            processTogglePowerSwitch(powerSwitchPressed);
        } else if (switchType == Switch::MOMENTARY) {
            processMomentaryPowerSwitch(powerSwitchPressed);
        }
    }
    
private:
    void recordSystemInitialization() {
        const long currentMillis = millis();
        
        // Record when system was first initialized
        if (g_state.machine.systemInitialized && g_state.sensors.systemInitializedTime == 0) {
            g_state.sensors.systemInitializedTime = currentMillis;
            logInfo("System initialization time recorded");
        }
    }
    
    void processTogglePowerSwitch(bool pressed) {
        if (pressed != g_state.sensors.lastPowerSwitchPressed) {
            g_state.sensors.lastPowerSwitchPressed = pressed;
            
            if (pressed) {
                powerOn();
            } else {
                powerOff();
            }
        }
    }
    
    void processMomentaryPowerSwitch(bool pressed) {
        const long currentMillis = millis();
        
        if (pressed != g_state.sensors.currStatePowerSwitchPressed) {
            g_state.sensors.currStatePowerSwitchPressed = pressed;
            
            if (pressed && g_state.machine.systemInitialized) {
                handlePowerButtonPress(currentMillis);
            } else if (!pressed) {
                handlePowerButtonRelease();
            }
        }
        
        // Check for long press reboot
        checkForLongPressReboot(pressed, currentMillis);
    }
    
    void handlePowerButtonPress(long currentMillis) {
        // Only start tracking if system initialized for at least 5 seconds
        if (currentMillis - g_state.sensors.systemInitializedTime > 5000) {
            g_state.sensors.firstSwitchPressTime = currentMillis;
            g_state.sensors.trackingPressTime = true;
            trackingLongPress_ = true;
            longPressStartTime_ = currentMillis;
        }
        
        // Toggle power state
        if (g_state.machine.machineState == LegacyMachineState::kStandby) {
            powerOn();
        } else {
            powerOff();
        }
    }
    
    void handlePowerButtonRelease() {
        g_state.sensors.trackingPressTime = false;
        g_state.sensors.firstSwitchPressTime = 0;
        trackingLongPress_ = false;
        longPressStartTime_ = 0;
    }
    
    void checkForLongPressReboot(bool pressed, long currentMillis) {
        if (pressed && 
            g_state.machine.systemInitialized && 
            (currentMillis - g_state.sensors.systemInitializedTime > 5000) &&
            trackingLongPress_ &&
            (currentMillis - longPressStartTime_ > 1000) &&
            switch_->longPressDetected()) {
            
            triggerSystemReboot();
        }
    }
    
    void powerOn() {
        if (g_state.machine.machineState == LegacyMachineState::kStandby || 
            g_state.machine.machineState == LegacyMachineState::kPidDisabled) {
            
            g_state.machine.machineState = LegacyMachineState::kPidNormal;
            resetStandbyTimer(LegacyMachineState::kPidNormal);
            setRuntimePidState(true);
            g_state.hardware.display->setPowerSave(0);
            logInfo("System powered on");
        }
    }
    
    void powerOff() {
        if (g_state.machine.machineState != LegacyMachineState::kStandby) {
            g_state.coordination.processController->performSafeShutdown();
            g_state.machine.machineState = LegacyMachineState::kStandby;
            g_state.standby.standbyModeRemainingTimeMillis = 0;
            logInfo("System powered off");
        }
    }
    
    void triggerSystemReboot() {
        logInfo("Power switch long press detected - initiating system reboot");
        g_state.hardware.display->setPowerSave(0);
        
        // Display reboot message
        displayMessage("REBOOTING", "Please wait...", "", "", "", "");
        delay(1000);
        
        g_state.coordination.processController->performSafeShutdown();
        
        logInfo("System reboot initiated");
        delay(1000);
        ESP.restart();
    }
};

// Global instance
inline PowerHandler g_powerHandler;

// Public interface function
inline void checkPowerSwitch() {
    g_powerHandler.process();
}