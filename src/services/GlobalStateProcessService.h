/**
 * @file GlobalStateProcessService.h
 * @brief Process service implementation using global state
 */

#pragma once

#include "IProcessService.h"
#include "../state/GlobalState.h"
#include "../brewHandler.h"

/**
 * @class GlobalStateProcessService
 * @brief Process service implementation that wraps global state access
 * 
 * This class provides a service layer over the global state, allowing gradual
 * migration away from direct global state access
 */
class GlobalStateProcessService : public IProcessService {
public:
    // Temperature operations
    double getCurrentTemperature() const override {
        return g_state.process.temperature;
    }
    
    double getSetpoint() const override {
        return g_state.process.setpoint;
    }
    
    void setSetpoint(double setpoint) override {
        g_state.process.setpoint = setpoint;
    }
    
    // Brew state operations
    BrewStates getCurrentBrewState() const override {
        return g_state.sensors.currBrewState;
    }
    
    void setCurrentBrewState(BrewStates state) override {
        g_state.sensors.currBrewState = state;
    }
    
    bool isBrewActive() const override {
        return checkBrewActive();
    }
    
    // Machine state operations
    LegacyMachineState getMachineState() const override {
        return g_state.machine.machineState;
    }
    
    void setMachineState(LegacyMachineState state) override {
        g_state.machine.machineState = state;
    }
    
    // PID operations
    double getPIDOutput() const override {
        return g_state.process.pidOutput;
    }
    
    void updatePID() override {
        if (g_state.pid) {
            g_state.pid->Compute();
        }
    }
    
    // Timer operations
    unsigned long getBrewTime() const override {
        return g_state.sensors.brewTime;
    }
    
    void startBrewTimer() override {
        g_state.sensors.brewStartTime = millis();
    }
    
    void stopBrewTimer() override {
        if (g_state.sensors.brewStartTime > 0) {
            g_state.sensors.brewTime = millis() - g_state.sensors.brewStartTime;
        }
    }
    
    void resetBrewTimer() override {
        g_state.sensors.brewStartTime = 0;
        g_state.sensors.brewTime = 0;
    }
    
    // Safety checks
    bool isPowerSwitchOperationAllowed() const override {
        return ::isPowerSwitchOperationAllowed();
    }
    
    bool checkBrewStates() const override {
        return ::checkBrewStates();
    }
};