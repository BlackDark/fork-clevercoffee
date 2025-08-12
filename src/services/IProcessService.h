/**
 * @file IProcessService.h  
 * @brief Process service interface for dependency injection
 */

#pragma once

#include "../brewStates.h"
#include "../state/MachineState.h"

/**
 * @interface IProcessService
 * @brief Interface for process state operations
 * 
 * This interface abstracts process state operations to reduce coupling to global state
 */
class IProcessService {
public:
    virtual ~IProcessService() = default;
    
    // Temperature operations
    virtual double getCurrentTemperature() const = 0;
    virtual double getSetpoint() const = 0;
    virtual void setSetpoint(double setpoint) = 0;
    
    // Brew state operations
    virtual BrewStates getCurrentBrewState() const = 0;
    virtual void setCurrentBrewState(BrewStates state) = 0;
    virtual bool isBrewActive() const = 0;
    
    // Machine state operations  
    virtual LegacyMachineState getMachineState() const = 0;
    virtual void setMachineState(LegacyMachineState state) = 0;
    
    // PID operations
    virtual double getPIDOutput() const = 0;
    virtual void updatePID() = 0;
    
    // Timer operations
    virtual unsigned long getBrewTime() const = 0;
    virtual void startBrewTimer() = 0;
    virtual void stopBrewTimer() = 0;
    virtual void resetBrewTimer() = 0;
    
    // Safety checks
    virtual bool isPowerSwitchOperationAllowed() const = 0;
    virtual bool checkBrewStates() const = 0;
};