/**
 * @file ProcessController.cpp
 * @brief Implementation of ProcessController for PID and process control
 */

#include "ProcessController.h"
#include "../Config.h"
#include "../display/DisplayManager.h"
#include "../hardware/HardwareManager.h"
#include "../hardware/scales/Scale.h"
#include "../network/MQTTManager.h"
#include "../sensors/SensorManager.h"
#include "../state/GlobalState.h"
#include "Logger.h"
#include <Arduino.h>

ProcessController::ProcessController(DisplayManager* displayManager, HardwareManager* hardwareManager, SensorManager* sensorManager, MQTTManager* mqttManager) :
    displayManager_(displayManager),
    hardwareManager_(hardwareManager),
    sensorManager_(sensorManager),
    mqttManager_(mqttManager),
    pidController_(nullptr),
    temperature_(0.0),
    pidOutput_(0.0),
    setpoint_(0.0),
    aggKp_(0.0),
    aggKi_(0.0),
    aggKd_(0.0),
    aggTn_(0.0),
    aggTv_(0.0),
    aggIMax_(0.0),
    aggbKp_(0.0),
    aggbKi_(0.0),
    aggbKd_(0.0),
    aggbTn_(0.0),
    aggbTv_(0.0),
    steamKp_(0.0),
    brewSetpoint_(0.0),
    steamSetpoint_(0.0),
    brewTempOffset_(0.0),
    lastMachineStatePid_(-1),
    initialized_(false),
    lastTempEvent_(0),
    tempEventInterval_(1000) {

    LOG(INFO, "ProcessController created");
}

bool ProcessController::initialize() {
    LOG(INFO, "Initializing ProcessController");

    setpoint_ = Config::getInstance().get<double>("brew.setpoint");
    aggKp_ = Config::getInstance().get<double>("pid.regular.kp");
    aggTn_ = Config::getInstance().get<double>("pid.regular.tn");
    aggTv_ = Config::getInstance().get<double>("pid.regular.tv");
    aggIMax_ = Config::getInstance().get<double>("pid.regular.i_max");
    aggbKp_ = Config::getInstance().get<double>("pid.bd.kp");
    aggbTn_ = Config::getInstance().get<double>("pid.bd.tn");
    aggbTv_ = Config::getInstance().get<double>("pid.bd.tv");
    steamKp_ = Config::getInstance().get<double>("pid.steam.kp");
    brewSetpoint_ = Config::getInstance().get<double>("brew.setpoint");
    steamSetpoint_ = Config::getInstance().get<double>("steam.setpoint");
    brewTempOffset_ = Config::getInstance().get<double>("brew.temp_offset");

    // Calculate initial PID parameters
    calculatePIDParameters();
    calculateBrewDetectionPIDParameters();

    // Sync with global variables
    temperature_ = g_state.process.temperature;
    pidOutput_ = g_state.process.pidOutput;
    setpoint_ = g_state.process.setpoint;
    lastTempEvent_ = 0;

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
    g_state.process.temperature = temperature_;

    // Test for emergency conditions
    testEmergencyConditions();

    // Compute PID output
    computePID();

    // Sync PID output with global variable
    g_state.process.pidOutput = pidOutput_;

    // Update debug logging if enabled
    updateDebugLogging();
}

void ProcessController::updateProcessControl(int machineState) {
    if (!initialized_) {
        LOG(WARNING, "ProcessController::updateProcessControl() called but not initialized");
        return;
    }

    // Update temperature from sensors
    updateTemperature();

    // Sync temperature with global variable
    g_state.process.temperature = temperature_;

    // Test for emergency conditions
    testEmergencyConditions();

    // Compute PID output
    computePID();

    // Sync PID output with global variable
    g_state.process.pidOutput = pidOutput_;

    // Update setpoint based on steam mode
    updateSetpoint(g_state.machine.steamON == 1);

    // Update PID state based on machine state
    updatePIDState(machineState);

    // Handle brew PID delay logic
    handleBrewPIDDelay(machineState);

    // Update debug logging if enabled
    updateDebugLogging();
}

void ProcessController::updateTemperature() {
    if (sensorManager_ != nullptr) {
        // Update SensorManager first to get fresh readings
        sensorManager_->update();

        // Use SensorManager for temperature reading (includes brew offset automatically)
        temperature_ = sensorManager_->getCurrentTemperature();

        if (!g_state.machine.steamON) {
            // Apply brew temperature offset if not in steam mode
            temperature_ -= brewTempOffset_;
        }
    }
    else if (hardwareManager_->getTempSensor() != nullptr) {
        // Fallback to direct sensor access
        temperature_ = hardwareManager_->getTempSensor()->getCurrentTemperature();

        // Apply brew offset if not in steam mode
        if (!g_state.machine.steamON) {
            temperature_ -= brewTempOffset_;
        }
    }
}

void ProcessController::computePID() {
    // Use the global PID controller for now (will be refactored later)
    g_state.pid->Compute();
    pidOutput_ = g_state.process.pidOutput; // Sync from global variable updated by g_state.pid->Compute()
}

void ProcessController::updatePIDState(int machineState) {
    // Check if PID should be enabled or disabled
    if (!shouldPIDBeEnabled(machineState)) {
        if (isPIDEnabled()) {
            // Force PID shutdown
            setPIDEnabled(false);
            pidOutput_ = 0;
            g_state.process.pidOutput = 0;
            if (hardwareManager_) {
                // Turn off heater through hardware manager
                // TODO: Add method to HardwareManager for heater control
            }
        }
    }
    else {
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
                setPIDTunings(Config::getInstance().get<bool>("pid.use_ponm"));
                break;
            case 50: // kSteam
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", steamKp_, 0.0, 0.0);
                setSteamPIDTunings();
                break;
            case 30: // kBrew - may use brew detection PID
                if (Config::getInstance().get<bool>("pid.bd.enabled")) {
                    LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggbKp_, aggbKi_, aggbKd_);
                    setBrewDetectionPIDTunings();
                }
                else {
                    LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                    setPIDTunings(Config::getInstance().get<bool>("pid.use_ponm"));
                }
                break;
            default:
                // Use normal tuning for other states
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                setPIDTunings(Config::getInstance().get<bool>("pid.use_ponm"));
                break;
        }
    }
}

void ProcessController::setPIDTunings(bool usePonM) {
    // Calculate Ki, Kd from time constants
    calculatePIDParameters();

    // Note: Machine state tracking moved to updatePIDState method

    // Set integrator limits
    g_state.pid->SetIntegratorLimits(0, aggIMax_);

    if (usePonM) {
        g_state.pid->SetTunings(aggbKp_, aggbKi_, aggbKd_, P_ON_M);
    }
    else {
        g_state.pid->SetTunings(aggKp_, aggKi_, aggKd_, 1);
    }
}

void ProcessController::setBrewDetectionPIDTunings() {
    // Calculate brew detection PID parameters
    calculateBrewDetectionPIDParameters();

    // Note: Machine state tracking moved to updatePIDState method

    g_state.pid->SetTunings(aggbKp_, aggbKi_, aggbKd_, 1);
}

void ProcessController::setSteamPIDTunings() {
    // Note: Machine state tracking moved to updatePIDState method

    g_state.pid->SetTunings(steamKp_, 0, 0, 1);
}

void ProcessController::updateSetpoint(bool steamActive) {
    if (steamActive) {
        setpoint_ = steamSetpoint_;
    }
    else {
        setpoint_ = brewSetpoint_;
    }

    // Sync with global variable
    g_state.process.setpoint = setpoint_;
}

bool ProcessController::shouldPIDBeEnabled(int machineState) const {
    // PID should be disabled in these states
    return !(machineState == 90 ||  // kPidDisabled
             machineState == 70 ||  // kWaterTankEmpty
             machineState == 100 || // kSensorError
             machineState == 80 ||  // kEmergencyStop
             machineState == 110 || // kEepromError
             machineState == 95 ||  // kStandby
             machineState == 60 ||  // kBackflush
             g_state.process.brewPidDisabled);
}

bool ProcessController::isPIDEnabled() const {
    return g_state.pid->GetMode() == AUTOMATIC;
}

void ProcessController::setPIDEnabled(bool enabled) {
    if (enabled) {
        g_state.pid->SetMode(AUTOMATIC);
    }
    else {
        g_state.pid->SetMode(MANUAL);
    }
}

void ProcessController::emergencyStop() {
    LOG(ERROR, "ProcessController emergency stop triggered!");

    // Immediately disable PID and turn off heater
    setPIDEnabled(false);
    pidOutput_ = 0;
    g_state.process.pidOutput = 0;

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
    if (!Config::getInstance().get<bool>("pid.enabled") || (currentMillis - lastTempEvent_) <= tempEventInterval_) {
        return;
    }

    lastTempEvent_ = currentMillis;
    // lastTempEvent = lastTempEvent_; // Sync with global

    // Log detailed PID information
    LOGF(TRACE, "Current PID mode: %s", g_state.pid->GetPonE() ? "PonE" : "PonM");

    // P-Part
    LOGF(TRACE, "Current PID input error: %f", g_state.pid->GetInputError());
    LOGF(TRACE, "Current PID P part: %f", g_state.pid->GetLastPPart());
    LOGF(TRACE, "Current PID kP: %f", g_state.pid->GetKp());

    // I-Part
    LOGF(TRACE, "Current PID I sum: %f", g_state.pid->GetLastIPart());
    LOGF(TRACE, "Current PID kI: %f", g_state.pid->GetKi());

    // D-Part
    LOGF(TRACE, "Current PID diff'd input: %f", g_state.pid->GetDeltaInput());
    LOGF(TRACE, "Current PID D part: %f", g_state.pid->GetLastDPart());
    LOGF(TRACE, "Current PID kD: %f", g_state.pid->GetKd());

    // Combined PID output
    LOGF(TRACE, "Current PID Output: %f", pidOutput_);
    LOGF(TRACE, "Current Temperature: %.2f°C", temperature_);
    LOGF(TRACE, "Current Setpoint: %.2f°C", setpoint_);
}

void ProcessController::calculatePIDParameters() {
    // Calculate Ki, Kd from time constants
    if (aggTn_ != 0) {
        aggKi_ = aggKp_ / aggTn_;
    }
    else {
        aggKi_ = 0;
    }

    aggKd_ = aggTv_ * aggKp_;
}

void ProcessController::calculateBrewDetectionPIDParameters() {
    // Calculate brew detection Ki, Kd from time constants
    if (aggbTn_ != 0) {
        aggbKi_ = aggbKp_ / aggbTn_;
    }
    else {
        aggbKi_ = 0;
    }

    aggbKd_ = aggbTv_ * aggbKp_;
}

void ProcessController::handleBrewPIDDelay(int machineState) {
    // Handle brew PID delay logic
    if (machineState == 30) { // kBrew
        if (Config::getInstance().get<double>("brew.pid_delay") > 0 && g_state.process.currBrewTime > 0 && g_state.process.currBrewTime < Config::getInstance().get<double>("brew.pid_delay") * 1000) {
            // disable PID for brewPidDelay seconds, enable PID again with new tunings after that
            if (!g_state.process.brewPidDisabled) {
                g_state.process.brewPidDisabled = true;
                g_state.pid->SetMode(MANUAL);
                pidOutput_ = 0;
                g_state.process.pidOutput = 0;
                if (hardwareManager_) {
                    // Turn off heater through hardware manager - for now use global heaterRelay
                    if (g_state.hardware.heaterRelay) {
                        g_state.hardware.heaterRelay->off();
                    }
                }
                LOGF(DEBUG, "disabled PID, waiting for %.0f seconds before enabling PID again", Config::getInstance().get<double>("brew.pid_delay"));
            }
        }
        else {
            if (g_state.process.brewPidDisabled) {
                // enable PID again
                g_state.pid->SetMode(AUTOMATIC);
                g_state.process.brewPidDisabled = false;
                LOGF(DEBUG, "Enabled PID again after %.0f seconds of brew pid delay", Config::getInstance().get<double>("brew.pid_delay"));
            }

            if (Config::getInstance().get<bool>("pid.bd.enabled")) {
                setBrewDetectionPIDTunings();
            }
            else {
                setPIDTunings(Config::getInstance().get<bool>("pid.use_ponm"));
            }
        }
    }
    // Reset brewPidDisabled if brew was aborted
    else if (machineState != 30 && g_state.process.brewPidDisabled) { // not kBrew
        // enable PID again
        g_state.pid->SetMode(AUTOMATIC);
        g_state.process.brewPidDisabled = false;
        LOG(DEBUG, "Enabled PID again after brew was manually stopped");
    }
}

void ProcessController::performSafeShutdown() {
    // Disable PID control
    extern void setRuntimePidState(bool state);
    setRuntimePidState(false);

    // Turn off all relays
    if (hardwareManager_ && g_state.hardware.heaterRelay && g_state.hardware.pumpRelay && g_state.hardware.valveRelay) {
        g_state.hardware.heaterRelay->off();
        g_state.hardware.pumpRelay->off();
        g_state.hardware.valveRelay->off();
    }

    // Reset all brew-related states
    if (g_state.sensors.currBrewState != kBrewIdle) {
        LOG(INFO, "Stopping active brew during safe shutdown");
        g_state.sensors.currBrewState = kBrewIdle;
        g_state.sensors.currBrewSwitchState = kBrewSwitchIdle;
        g_state.process.currBrewTime = 0;
        g_state.process.startingTime = 0;
        g_state.sensors.brewSwitchWasOff = false;
    }

    // Reset manual flush states
    if (g_state.sensors.currManualFlushState != kManualFlushIdle) {
        LOG(INFO, "Stopping manual group head flush during safe shutdown");
        g_state.sensors.currManualFlushState = kManualFlushIdle;
        g_state.sensors.currBrewSwitchState = kBrewSwitchIdle;
        g_state.process.currBrewTime = 0;
        g_state.process.startingTime = 0;
    }

    // Reset backflush state
    if (g_state.sensors.currBackflushState != kBackflushIdle) {
        LOG(INFO, "Stopping active backflush during safe shutdown");
        g_state.sensors.currBackflushState = kBackflushIdle;
        g_state.machine.currBackflushCycles = 1;
    }

    // Reset hot water state - handled by hotWaterHandler
    // TODO: Add proper hot water state reset through hotWaterHandler interface

    // Turn off steam mode if active
    if (g_state.machine.steamON) {
        LOG(INFO, "Disabling steam mode during safe shutdown");
        g_state.machine.steamON = false;
        g_state.machine.steamFirstON = false;
    }

    LOG(INFO, "Safe shutdown completed - all relays turned off, all states reset");
}
