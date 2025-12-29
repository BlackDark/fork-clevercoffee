/**
 * @file ProcessController.cpp
 * @brief Implementation of ProcessController for PID and process control
 */

#include "clevercoffee/hardware/HardwareManager.h"  // Include before own header to resolve forward declaration
#include "clevercoffee/control/ProcessController.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/constants/Temperature.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/display/DisplayManager.h"
#include "clevercoffee/hardware/scales/Scale.h"
#include "clevercoffee/network/MQTTManager.h"
#include "clevercoffee/utils/SystemUtils.h"

#include <Arduino.h>

ProcessController::ProcessController(const Config&                 config,
                                      CleverCoffee::SystemContext&   systemContext,
                                      DisplayManager*                displayManager,
                                      CleverCoffee::HardwareManager* hardwareManager,
                                      MQTTManager*                   mqttManager)
    : config_(config), systemContext_(systemContext), displayManager_(displayManager), hardwareManager_(hardwareManager),
      mqttManager_(mqttManager), pidController_(nullptr), temperature_(0.0), pidOutput_(0.0), setpoint_(0.0),
      aggKp_(0.0), aggKi_(0.0), aggKd_(0.0), aggTn_(0.0), aggTv_(0.0), aggIMax_(0.0), aggbKp_(0.0), aggbKi_(0.0),
      aggbKd_(0.0), aggbTn_(0.0), aggbTv_(0.0), steamKp_(0.0), brewSetpoint_(0.0), steamSetpoint_(0.0),
      brewTempOffset_(0.0), lastMachineStatePid_(MachineStateId::INIT), initialized_(false), lastTempEvent_(0),
      currBrewTime_(0.0), totalTargetBrewTime_(0.0), brewPidDisabled_(false),
      tempEventInterval_(1000) {
    LOG(INFO, "ProcessController created");
}

bool ProcessController::initialize() {
    LOG(INFO, "Initializing ProcessController");

    setpoint_       = config_.brewSetpoint.get();
    aggKp_          = config_.pidRegularKp.get();
    aggTn_          = config_.pidRegularTn.get();
    aggTv_          = config_.pidRegularTv.get();
    aggIMax_        = config_.pidRegularIMax.get();
    aggbKp_         = config_.pidBdKp.get();
    aggbTn_         = config_.pidBdTn.get();
    aggbTv_         = config_.pidBdTv.get();
    steamKp_        = config_.pidSteamKp.get();
    brewSetpoint_   = config_.brewSetpoint.get();
    steamSetpoint_  = config_.steamSetpoint.get();
    brewTempOffset_ = config_.brewTempOffset.get();

    // Calculate initial PID parameters
    calculatePIDParameters();
    calculateBrewDetectionPIDParameters();

    // Sync with global variables
    temperature_   = g_state.process.temperature;
    pidOutput_     = g_state.process.pidOutput;
    setpoint_      = g_state.process.setpoint;
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
    // Use SensorCoordinator from SystemContext for temperature reading (includes brew offset automatically)
    temperature_ = systemContext_.sensorCoordinator().getTemperature();

    if (!g_state.machine.steamON) {
        // Apply brew temperature offset if not in steam mode
        temperature_ -= brewTempOffset_;
    }
}

// === IHardwareContext Interface Implementation ===

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
            pidOutput_                = 0;
            g_state.process.pidOutput = 0;
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
            case MachineStateId::PID_NORMAL:
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                setPIDTunings(config_.pidUsePonm.get());
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
                if (config_.pidBdEnabled.get()) {
                    LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggbKp_, aggbKi_, aggbKd_);
                    setBrewDetectionPIDTunings();
                } else {
                    LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                    setPIDTunings(config_.pidUsePonm.get());
                }
                break;
            default:
                // Use normal tuning for other states
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp_, aggKi_, aggKd_);
                setPIDTunings(config_.pidUsePonm.get());
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
    } else {
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
    } else {
        setpoint_ = brewSetpoint_;
    }

    // Sync with global variable
    g_state.process.setpoint = setpoint_;
}

bool ProcessController::shouldPIDBeEnabled(MachineStateId machineState) const {
    // PID should be disabled in these states
    return !(machineState == MachineStateId::PID_DISABLED || machineState == MachineStateId::WATER_TANK_EMPTY ||
             machineState == MachineStateId::SENSOR_ERROR || machineState == MachineStateId::EMERGENCY_STOP ||
             machineState == MachineStateId::EEPROM_ERROR || machineState == MachineStateId::STANDBY ||
             isBackflushState(machineState) || g_state.process.brewPidDisabled);
}

bool ProcessController::isPIDEnabled() const {
    return g_state.pid->GetMode() == AUTOMATIC;
}

void ProcessController::setPIDEnabled(bool enabled) {
    if (enabled) {
        g_state.pid->SetMode(AUTOMATIC);
    } else {
        g_state.pid->SetMode(MANUAL);
    }
}

void ProcessController::emergencyStop() {
     LOG(ERROR, "ProcessController emergency stop triggered!");

     // Immediately disable PID and turn off heater
     setPIDEnabled(false);
     pidOutput_                = 0;
     g_state.process.pidOutput = 0;

     if (hardwareManager_) {
         hardwareManager_->safeShutdown();
     }
}

double ProcessController::getCurrBrewTime() const {
     return currBrewTime_;
}

void ProcessController::setCurrBrewTime(double brewTime) {
     currBrewTime_ = brewTime;
     g_state.process.currBrewTime = brewTime;
}

double ProcessController::getTotalTargetBrewTime() const {
     return totalTargetBrewTime_;
}

void ProcessController::setTotalTargetBrewTime(double brewTime) {
     totalTargetBrewTime_ = brewTime;
     g_state.process.totalTargetBrewTime = brewTime;
}

bool ProcessController::isBrewPidDisabled() const {
     return brewPidDisabled_;
}

void ProcessController::setBrewPidDisabled(bool disabled) {
     brewPidDisabled_ = disabled;
     g_state.process.brewPidDisabled = disabled;
}

bool ProcessController::testEmergencyConditions() {
    const double emergencyTemp = config_.emergencyStopTemp.get();
    const double hysteresis = config_.emergencyStopHysteresis.get();
    const double sensorMinValid = CleverCoffee::Temperature::MIN_VALID_TEMP_C;
    const double sensorMaxValid = CleverCoffee::Temperature::MAX_VALID_TEMP_C;

    // STEP 1: Check for sensor disconnection or invalid reading
    if (temperature_ < sensorMinValid || temperature_ > sensorMaxValid) {
        LOGF(ERROR, "Emergency: Invalid temperature reading (%.1f°C outside valid range)", temperature_);
        emergencyStop();
        return true;
    }

    // STEP 2: Hysteresis-based emergency detection with debouncing
    if (temperature_ > emergencyTemp) {
        emergencyTempReadingCount_++;
        LOGF(WARNING, "High temperature detected: %.1f°C (reading %d/%d)",
             temperature_, emergencyTempReadingCount_, EMERGENCY_TEMP_DEBOUNCE_COUNT);

        // Require multiple consecutive high readings to trigger emergency
        if (emergencyTempReadingCount_ >= EMERGENCY_TEMP_DEBOUNCE_COUNT) {
            LOGF(ERROR, "Emergency: Temperature too high (%.1f°C > %.1f°C limit)!",
                 temperature_, emergencyTemp);
            emergencyStop();
            return true;
        }
    } else if (temperature_ < (emergencyTemp - hysteresis)) {
        // Reset counter only when temperature drops below threshold minus hysteresis
        if (emergencyTempReadingCount_ > 0) {
            LOGF(INFO, "Temperature normalized. Resetting emergency counter.");
            emergencyTempReadingCount_ = 0;
        }
    }
    // If temperature is between (threshold - hysteresis) and threshold, keep counter as-is
    // This implements hysteresis to prevent oscillation

    return false;
}

void ProcessController::updateDebugLogging() {
    const unsigned long currentMillis = millis();

    // Only log periodically and when PID is enabled
    if (!config_.pidEnabled.get() || (currentMillis - lastTempEvent_) <= tempEventInterval_) {
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

void ProcessController::handleBrewPIDDelay(MachineStateId machineState) {
    // Handle brew PID delay logic
    if (isBrewState(machineState)) {
        if (config_.brewPidDelay.get() > 0 && g_state.process.currBrewTime > 0 &&
            g_state.process.currBrewTime < config_.brewPidDelay.get() * 1000) {
            // disable PID for brewPidDelay seconds, enable PID again with new tunings after that
            if (!g_state.process.brewPidDisabled) {
                g_state.process.brewPidDisabled = true;
                g_state.pid->SetMode(MANUAL);
                pidOutput_                = 0;
                g_state.process.pidOutput = 0;
                if (hardwareManager_) {
                    hardwareManager_->getHeaterRelay()->off();
                }
                LOGF(DEBUG,
                     "disabled PID, waiting for %.0f seconds before enabling PID again",
                     config_.brewPidDelay.get());
            }
        } else {
            if (g_state.process.brewPidDisabled) {
                // enable PID again
                g_state.pid->SetMode(AUTOMATIC);
                g_state.process.brewPidDisabled = false;
                LOGF(DEBUG,
                     "Enabled PID again after %.0f seconds of brew pid delay",
                     config_.brewPidDelay.get());
            }

            if (config_.pidBdEnabled.get()) {
                setBrewDetectionPIDTunings();
            } else {
                setPIDTunings(config_.pidUsePonm.get());
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
    LOG(INFO, "ProcessController performing safe shutdown");
    
    // Disable PID control
    setPIDEnabled(false);
    pidOutput_ = 0;
    g_state.process.pidOutput = 0;

    // Delegate hardware shutdown to HardwareManager
    if (hardwareManager_) {
        hardwareManager_->safeShutdown();
    }

    LOG(INFO, "ProcessController safe shutdown completed");
}

