/**
 * @file ProcessController.cpp
 * @brief Implementation of ProcessController for PID and process control
 */

#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/Config.h"
#include "clevercoffee/display/DisplayManager.h"
#include "clevercoffee/hardware/HardwareManager.h"
#include "clevercoffee/hardware/scales/Scale.h"
#include "clevercoffee/network/MQTTManager.h"
#include "clevercoffee/sensors/SensorManager.h"
#include "clevercoffee/state/GlobalState.h"
#include "clevercoffee/utils/SystemUtils.h"
#include "clevercoffee/Logger.h"
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
    lastMachineStatePid_(MachineStateId::INIT),
    initialized_(false),
    lastTempEvent_(0),
    tempEventInterval_(1000) {

    LOG(INFO, "ProcessController created");
}

bool ProcessController::initialize() {
    LOG(INFO, "Initializing ProcessController");

    setpoint_ = Config::getInstance().brewSetpoint.get();
    aggKp_ = Config::getInstance().pidRegularKp.get();
    aggTn_ = Config::getInstance().pidRegularTn.get();
    aggTv_ = Config::getInstance().pidRegularTv.get();
    aggIMax_ = Config::getInstance().pidRegularIMax.get();
    aggbKp_ = Config::getInstance().pidBdKp.get();
    aggbTn_ = Config::getInstance().pidBdTn.get();
    aggbTv_ = Config::getInstance().pidBdTv.get();
    steamKp_ = Config::getInstance().pidSteamKp.get();
    brewSetpoint_ = Config::getInstance().brewSetpoint.get();
    steamSetpoint_ = Config::getInstance().steamSetpoint.get();
    brewTempOffset_ = Config::getInstance().brewTempOffset.get();

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

void ProcessController::updateProcessControl(MachineStateId machineState) {
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

void ProcessController::updatePIDState(MachineStateId machineState) {
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
            case MachineStateId::PID_NORMAL:
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                setPIDTunings(Config::getInstance().pidUsePonm.get());
                break;
            case MachineStateId::STEAM_IDLE:
            case MachineStateId::STEAM_RUNNING:
            case MachineStateId::STEAM_STOPPED:
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", steamKp_, 0.0, 0.0);
                setSteamPIDTunings();
                break;
            case MachineStateId::BREW_IDLE:
            case MachineStateId::BREW_PREINFUSION:
            case MachineStateId::BREW_PREINFUSION_PAUSE:
            case MachineStateId::BREW_RUNNING:
            case MachineStateId::BREW_FINISHED: // may use brew detection PID
                if (Config::getInstance().pidBdEnabled.get()) {
                    LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggbKp_, aggbKi_, aggbKd_);
                    setBrewDetectionPIDTunings();
                }
                else {
                    LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                    setPIDTunings(Config::getInstance().pidUsePonm.get());
                }
                break;
            default:
                // Use normal tuning for other states
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                setPIDTunings(Config::getInstance().pidUsePonm.get());
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

bool ProcessController::shouldPIDBeEnabled(MachineStateId machineState) const {
    // PID should be disabled in these states
    return !(machineState == MachineStateId::PID_DISABLED ||
             machineState == MachineStateId::WATER_TANK_EMPTY ||
             machineState == MachineStateId::SENSOR_ERROR ||
             machineState == MachineStateId::EMERGENCY_STOP ||
             machineState == MachineStateId::EEPROM_ERROR ||
             machineState == MachineStateId::STANDBY ||
             isBackflushState(machineState) ||
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
    if (!Config::getInstance().pidEnabled.get() || (currentMillis - lastTempEvent_) <= tempEventInterval_) {
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

void ProcessController::handleBrewPIDDelay(MachineStateId machineState) {
    // Handle brew PID delay logic
    if (isBrewState(machineState)) {
        if (Config::getInstance().brewPidDelay.get() > 0 && g_state.process.currBrewTime > 0 && g_state.process.currBrewTime < Config::getInstance().brewPidDelay.get() * 1000) {
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
                LOGF(DEBUG, "disabled PID, waiting for %.0f seconds before enabling PID again", Config::getInstance().brewPidDelay.get());
            }
        }
        else {
            if (g_state.process.brewPidDisabled) {
                // enable PID again
                g_state.pid->SetMode(AUTOMATIC);
                g_state.process.brewPidDisabled = false;
                LOGF(DEBUG, "Enabled PID again after %.0f seconds of brew pid delay", Config::getInstance().brewPidDelay.get());
            }

            if (Config::getInstance().pidBdEnabled.get()) {
                setBrewDetectionPIDTunings();
            }
            else {
                setPIDTunings(Config::getInstance().pidUsePonm.get());
            }
        }
    }
    // Reset brewPidDisabled if brew was aborted
    else if (!isBrewState(machineState) && g_state.process.brewPidDisabled) { // not BREW
        // enable PID again
        g_state.pid->SetMode(AUTOMATIC);
        g_state.process.brewPidDisabled = false;
        LOG(DEBUG, "Enabled PID again after brew was manually stopped");
    }
}

void ProcessController::performSafeShutdown() {
    // Disable PID control
    setRuntimePidState(false);

    // Turn off all relays
    if (hardwareManager_ && g_state.hardware.heaterRelay && g_state.hardware.pumpRelay && g_state.hardware.valveRelay) {
        g_state.hardware.heaterRelay->off();
        g_state.hardware.pumpRelay->off();
        g_state.hardware.valveRelay->off();
    }

    // Reset all brew-related states
    if (isBrewState(g_state.machine.machineState) && g_state.machine.machineState != MachineStateId::BREW_IDLE) {
        LOG(INFO, "Stopping active brew during safe shutdown");
        g_state.machine.flags.requestBrewStop = true;  // Use condition flag instead of direct state assignment
        g_state.sensors.currBrewSwitchState = SwitchState::IDLE;
        g_state.process.currBrewTime = 0;
        g_state.process.startingTime = 0;
        g_state.sensors.brewSwitchWasOff = false;
    }

    // Reset manual flush states
    if (isManualFlushState(g_state.machine.machineState) && g_state.machine.machineState != MachineStateId::MANUAL_FLUSH_IDLE) {
        LOG(INFO, "Stopping manual group head flush during safe shutdown");
        g_state.machine.flags.requestManualFlushStop = true;  // Use condition flag instead of direct state assignment
        g_state.sensors.currBrewSwitchState = SwitchState::IDLE;
        g_state.process.currBrewTime = 0;
        g_state.process.startingTime = 0;
    }

    // Reset backflush state
    if (isBackflushState(g_state.machine.machineState) && g_state.machine.machineState != MachineStateId::BACKFLUSH_IDLE) {
        LOG(INFO, "Stopping active backflush during safe shutdown");
        g_state.machine.flags.requestBackflushStop = true;  // Use condition flag instead of direct state assignment
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
