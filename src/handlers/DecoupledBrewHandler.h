/**
 * @file DecoupledBrewHandler.h
 * @brief Brew handler with dependency injection
 */

#pragma once

#include "../services/IHardwareService.h"
#include "../services/IProcessService.h"
#include "../Logger.h"

/**
 * @class DecoupledBrewHandler  
 * @brief Brew handler that uses dependency injection instead of global state
 * 
 * This class demonstrates how handlers can be decoupled from global state
 * by accepting service interfaces via dependency injection
 */
class DecoupledBrewHandler {
private:
    IHardwareService* hardwareService_;
    IProcessService* processService_;
    
public:
    DecoupledBrewHandler(IHardwareService* hardware, IProcessService* process)
        : hardwareService_(hardware), processService_(process) {}
    
    /**
     * @brief Check if brew is currently active
     */
    bool isBrewActive() const {
        return processService_->isBrewActive();
    }
    
    /**
     * @brief Check if in brew-related machine states
     */
    bool checkBrewStates() const {
        return processService_->checkBrewStates();
    }
    
    /**
     * @brief Perform valve safety shutdown check
     */
    void valveSafetyShutdownCheck() {
        if (!isBrewActive() && !checkBrewStates()) {
            processService_->setCurrentBrewState(kBrewIdle);
            hardwareService_->turnValveOff();
        }
    }
    
    /**
     * @brief Start brewing process
     */
    void startBrew() {
        if (!processService_->isPowerSwitchOperationAllowed()) {
            LOG(WARNING, "Brew start blocked - power switch operation not allowed");
            return;
        }
        
        LOG(INFO, "Starting brew");
        processService_->setCurrentBrewState(kBrewStarted);
        processService_->startBrewTimer();
        
        hardwareService_->turnPumpOn();
        hardwareService_->turnValveOn();
        hardwareService_->setBrewLed(true);
    }
    
    /**
     * @brief Stop brewing process
     */
    void stopBrew() {
        LOG(INFO, "Stopping brew");
        processService_->setCurrentBrewState(kBrewFinished);
        processService_->stopBrewTimer();
        
        hardwareService_->turnPumpOff();
        hardwareService_->turnValveOff();
        hardwareService_->setBrewLed(false);
    }
    
    /**
     * @brief Update brew state machine
     */
    void updateBrewState() {
        // Example state machine logic using injected services
        switch (processService_->getCurrentBrewState()) {
            case kBrewIdle:
                if (hardwareService_->isBrewSwitchPressed()) {
                    startBrew();
                }
                break;
                
            case kBrewStarted:
                if (!hardwareService_->isBrewSwitchPressed()) {
                    stopBrew();
                }
                break;
                
            case kBrewFinished:
                // Transition back to idle after some time
                if (processService_->getBrewTime() > 0) {
                    processService_->setCurrentBrewState(kBrewIdle);
                }
                break;
                
            default:
                // Safety: return to idle for unknown states
                processService_->setCurrentBrewState(kBrewIdle);
                break;
        }
        
        // Always check safety
        valveSafetyShutdownCheck();
    }
};