/**
 * @file ProcessController.cpp
 * @brief Implementation of ProcessController for PID and process control
 */

#include "ProcessController.h"
#include "../display/DisplayManager.h"
#include "../hardware/HardwareManager.h"
#include "../sensors/SensorManager.h"
#include "../network/MQTTManager.h"
#include "../hardware/tempsensors/TempSensor.h"
#include "../hardware/scales/Scale.h"
#include "../GlobalVariables.h"
#include "../Config.h"
#include "Logger.h"
#include <Arduino.h>

// External global variables that ProcessController will manage
extern double temperature;
extern double pidOutput;
extern double setpoint;
extern PID bPID;
extern TempSensor* tempSensor;
extern unsigned long lastTempEvent;
extern unsigned long tempEventInterval;
extern double currBrewTime;
extern double brewPidDelay;
extern bool useBDPID;
extern bool usePonM;
extern bool pidON;
extern Relay* heaterRelay;

// Forward declarations for variables from brewHandler.h
extern bool brewPidDisabled;

ProcessController::ProcessController(
    DisplayManager* displayManager,
    HardwareManager* hardwareManager,
    SensorManager* sensorManager,
    MQTTManager* mqttManager
) : displayManager_(displayManager),
    hardwareManager_(hardwareManager),
    sensorManager_(sensorManager),
    mqttManager_(mqttManager),
    pidController_(nullptr),
    temperature_(0.0),
    pidOutput_(0.0),
    setpoint_(brewSetpoint),
    aggKp_(aggKp),
    aggKi_(0.0),
    aggKd_(0.0),
    aggTn_(aggTn),
    aggTv_(aggTv),
    aggIMax_(aggIMax),
    aggbKp_(aggbKp),
    aggbKi_(0.0),
    aggbKd_(0.0),
    aggbTn_(aggbTn),
    aggbTv_(aggbTv),
    steamKp_(steamKp),
    brewSetpoint_(brewSetpoint),
    steamSetpoint_(steamSetpoint),
    brewTempOffset_(brewTempOffset),
    lastMachineStatePid_(-1),
    initialized_(false),
    lastTempEvent_(0),
    tempEventInterval_(tempEventInterval) {
    
    LOG(INFO, "ProcessController created");
}

bool ProcessController::initialize() {
    LOG(INFO, "Initializing ProcessController");
    
    // Calculate initial PID parameters
    calculatePIDParameters();
    calculateBrewDetectionPIDParameters();
    
    // Sync with global variables
    temperature_ = temperature;
    pidOutput_ = pidOutput;
    setpoint_ = setpoint;
    lastTempEvent_ = lastTempEvent;
    
    initialized_ = true;
    
    LOG(INFO, "ProcessController initialized successfully");
    return true;
}

void ProcessController::update() {
    if (!initialized_) {
        LOG(WARNING, "ProcessController::update() called but not initialized");
        return;
    }
    
    // Update temperature from sensors
    updateTemperature();
    
    // Sync temperature with global variable
    temperature = temperature_;
    
    // Test for emergency conditions
    testEmergencyConditions();
    
    // Compute PID output
    computePID();
    
    // Sync PID output with global variable
    pidOutput = pidOutput_;
    
    // Update debug logging if enabled
    updateDebugLogging();
}

void ProcessController::updateProcessControl(int machineState, bool brewPidDisabled) {
    if (!initialized_) {
        LOG(WARNING, "ProcessController::updateProcessControl() called but not initialized");
        return;
    }
    
    // Update temperature from sensors
    updateTemperature();
    
    // Sync temperature with global variable
    temperature = temperature_;
    
    // Test for emergency conditions
    testEmergencyConditions();
    
    // Compute PID output
    computePID();
    
    // Sync PID output with global variable
    pidOutput = pidOutput_;
    
    // Update setpoint based on steam mode
    updateSetpoint(steamON == 1);
    
    // Update PID state based on machine state
    updatePIDState(machineState);
    
    // Handle brew PID delay logic
    handleBrewPIDDelay(machineState, brewPidDisabled);
    
    // Update debug logging if enabled
    updateDebugLogging();
}

void ProcessController::updateTemperature() {
    if (sensorManager_ != nullptr) {
        // Update SensorManager first to get fresh readings
        sensorManager_->update();
        
        // Use SensorManager for temperature reading (includes brew offset automatically)
        temperature_ = sensorManager_->getCurrentTemperature();

        // For steam mode, get raw temperature without brew offset
        if (steamON && tempSensor != nullptr) {
            temperature_ = tempSensor->getCurrentTemperature();
        }
    } else if (tempSensor != nullptr) {
        // Fallback to direct sensor access
        temperature_ = tempSensor->getCurrentTemperature();

        // Apply brew offset if not in steam mode
        if (!steamON) {
            temperature_ -= brewTempOffset_;
        }
    }
}

void ProcessController::computePID() {
    // Use the global PID controller for now (will be refactored later)
    bPID.Compute();
    pidOutput_ = pidOutput; // Sync from global variable updated by bPID.Compute()
}

void ProcessController::updatePIDState(int machineState) {
    const bool brewPidDisabled = false; // TODO: Get this from appropriate source
    
    // Check if PID should be enabled or disabled
    if (!shouldPIDBeEnabled(machineState, brewPidDisabled)) {
        if (isPIDEnabled()) {
            // Force PID shutdown
            setPIDEnabled(false);
            pidOutput_ = 0;
            pidOutput = 0;
            if (hardwareManager_) {
                // Turn off heater through hardware manager
                // TODO: Add method to HardwareManager for heater control
            }
        }
    } else {
        // Enable PID if it was disabled
        if (!isPIDEnabled()) {
            setPIDEnabled(true);
        }
    }
    
    // Log PID values when machine state changes
    if (lastMachineStatePid_ != machineState) {
        lastMachineStatePid_ = machineState;
        
        // Set appropriate PID tuning based on machine state
        switch (machineState) {
            case 20: // kPidNormal
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                setPIDTunings(usePonM);
                break;
            case 50: // kSteam
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", steamKp_, 0.0, 0.0);
                setSteamPIDTunings();
                break;
            case 30: // kBrew - may use brew detection PID
                if (useBDPID) {
                    LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggbKp_, aggbKi_, aggbKd_);
                    setBrewDetectionPIDTunings();
                } else {
                    LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                    setPIDTunings(usePonM);
                }
                break;
            default:
                // Use normal tuning for other states
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                setPIDTunings(usePonM);
                break;
        }
    }
}

void ProcessController::setPIDTunings(bool usePonM) {
    // Calculate Ki, Kd from time constants
    calculatePIDParameters();
    
    // Note: Machine state tracking moved to updatePIDState method
    
    // Set integrator limits
    bPID.SetIntegratorLimits(0, aggIMax_);
    
    if (usePonM) {
        bPID.SetTunings(aggbKp_, aggbKi_, aggbKd_, P_ON_M);
    } else {
        bPID.SetTunings(aggKp_, aggKi_, aggKd_, 1);
    }
}

void ProcessController::setBrewDetectionPIDTunings() {
    // Calculate brew detection PID parameters
    calculateBrewDetectionPIDParameters();
    
    // Note: Machine state tracking moved to updatePIDState method
    
    bPID.SetTunings(aggbKp_, aggbKi_, aggbKd_, 1);
}

void ProcessController::setSteamPIDTunings() {
    // Note: Machine state tracking moved to updatePIDState method
    
    bPID.SetTunings(steamKp_, 0, 0, 1);
}

void ProcessController::updateSetpoint(bool steamActive) {
    if (steamActive) {
        setpoint_ = steamSetpoint_;
    } else {
        setpoint_ = brewSetpoint_;
    }
    
    // Sync with global variable
    setpoint = setpoint_;
}

bool ProcessController::shouldPIDBeEnabled(int machineState, bool brewPidDisabled) const {
    // PID should be disabled in these states
    return !(machineState == 90 ||   // kPidDisabled
             machineState == 70 ||   // kWaterTankEmpty
             machineState == 100 ||  // kSensorError
             machineState == 80 ||   // kEmergencyStop
             machineState == 110 ||  // kEepromError
             machineState == 95 ||   // kStandby
             machineState == 60 ||   // kBackflush
             brewPidDisabled);
}

bool ProcessController::isPIDEnabled() const {
    return bPID.GetMode() == AUTOMATIC;
}

void ProcessController::setPIDEnabled(bool enabled) {
    if (enabled) {
        bPID.SetMode(AUTOMATIC);
    } else {
        bPID.SetMode(MANUAL);
    }
}

void ProcessController::emergencyStop() {
    LOG(ERROR, "ProcessController emergency stop triggered!");
    
    // Immediately disable PID and turn off heater
    setPIDEnabled(false);
    pidOutput_ = 0;
    pidOutput = 0;
    
    if (hardwareManager_) {
        // Emergency heater shutdown through hardware manager
        // TODO: Add emergency shutdown method to HardwareManager
    }
}

bool ProcessController::testEmergencyConditions() {
    // Test if temperature is too high (emergency condition)
    const double emergencyTemp = 150.0; // TODO: Get from config
    
    if (temperature_ > emergencyTemp) {
        LOG(ERROR, "Emergency: Temperature too high!");
        emergencyStop();
        return true;
    }
    
    return false;
}

void ProcessController::updateDebugLogging() {
    const unsigned long currentMillis = millis();
    
    // Only log periodically and when PID is enabled
    if (!pidON || (currentMillis - lastTempEvent_) <= tempEventInterval_) {
        return;
    }
    
    lastTempEvent_ = currentMillis;
    lastTempEvent = lastTempEvent_; // Sync with global
    
    // Log detailed PID information
    LOGF(TRACE, "Current PID mode: %s", bPID.GetPonE() ? "PonE" : "PonM");
    
    // P-Part
    LOGF(TRACE, "Current PID input error: %f", bPID.GetInputError());
    LOGF(TRACE, "Current PID P part: %f", bPID.GetLastPPart());
    LOGF(TRACE, "Current PID kP: %f", bPID.GetKp());
    
    // I-Part
    LOGF(TRACE, "Current PID I sum: %f", bPID.GetLastIPart());
    LOGF(TRACE, "Current PID kI: %f", bPID.GetKi());
    
    // D-Part
    LOGF(TRACE, "Current PID diff'd input: %f", bPID.GetDeltaInput());
    LOGF(TRACE, "Current PID D part: %f", bPID.GetLastDPart());
    LOGF(TRACE, "Current PID kD: %f", bPID.GetKd());
    
    // Combined PID output
    LOGF(TRACE, "Current PID Output: %f", pidOutput_);
    LOGF(TRACE, "Current Temperature: %.2f°C", temperature_);
    LOGF(TRACE, "Current Setpoint: %.2f°C", setpoint_);
}

void ProcessController::calculatePIDParameters() {
    // Calculate Ki, Kd from time constants
    if (aggTn_ != 0) {
        aggKi_ = aggKp_ / aggTn_;
    } else {
        aggKi_ = 0;
    }
    
    aggKd_ = aggTv_ * aggKp_;
}

void ProcessController::calculateBrewDetectionPIDParameters() {
    // Calculate brew detection Ki, Kd from time constants
    if (aggbTn_ != 0) {
        aggbKi_ = aggbKp_ / aggbTn_;
    } else {
        aggbKi_ = 0;
    }
    
    aggbKd_ = aggbTv_ * aggbKp_;
}

void ProcessController::handleBrewPIDDelay(int machineState, bool brewPidDisabled) {
    // Handle brew PID delay logic
    if (machineState == 30) { // kBrew
        if (brewPidDelay > 0 && currBrewTime > 0 && currBrewTime < brewPidDelay * 1000) {
            // disable PID for brewPidDelay seconds, enable PID again with new tunings after that
            if (!brewPidDisabled) {
                brewPidDisabled = true;
                bPID.SetMode(MANUAL);
                pidOutput_ = 0;
                pidOutput = 0;
                if (hardwareManager_) {
                    // Turn off heater through hardware manager - for now use global heaterRelay
                    if (heaterRelay) {
                        heaterRelay->off();
                    }
                }
                LOGF(DEBUG, "disabled PID, waiting for %.0f seconds before enabling PID again", brewPidDelay);
            }
        }
        else {
            if (brewPidDisabled) {
                // enable PID again
                bPID.SetMode(AUTOMATIC);
                brewPidDisabled = false;
                LOGF(DEBUG, "Enabled PID again after %.0f seconds of brew pid delay", brewPidDelay);
            }

            if (useBDPID) {
                setBrewDetectionPIDTunings();
            }
            else {
                setPIDTunings(usePonM);
            }
        }
    }
    // Reset brewPidDisabled if brew was aborted
    else if (machineState != 30 && brewPidDisabled) { // not kBrew
        // enable PID again
        bPID.SetMode(AUTOMATIC);
        brewPidDisabled = false;
        LOG(DEBUG, "Enabled PID again after brew was manually stopped");
    }
}