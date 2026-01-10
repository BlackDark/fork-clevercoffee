#include "clevercoffee/context/SystemContext.h"
#include <PID_v1.h>  // Required for PID method implementations

#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/Logger.h"

#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/handlers/PowerHandler.h"
#include "clevercoffee/handlers/SteamHandler.h"

namespace CleverCoffee {

// ===== PROCESS STATE ACCESSORS =====

double SystemContext::processTemperature() const noexcept {
    return process_temperature_;
}

void SystemContext::setProcessTemperature(double temp) noexcept {
    process_temperature_ = temp;
}

double SystemContext::processSetpoint() const noexcept {
    return process_setpoint_;
}

void SystemContext::setProcessSetpoint(double setpoint) noexcept {
    process_setpoint_ = setpoint;
}

double SystemContext::processPidOutput() const noexcept {
    return process_pidOutput_;
}

void SystemContext::setProcessPidOutput(double output) noexcept {
    process_pidOutput_ = output;
}

double SystemContext::processCurrentBrewTime() const noexcept {
    return process_currBrewTime_;
}

void SystemContext::setProcessCurrentBrewTime(double time) noexcept {
    process_currBrewTime_ = time;
}

double SystemContext::processTotalTargetBrewTime() const noexcept {
    return process_totalTargetBrewTime_;
}

void SystemContext::setProcessTotalTargetBrewTime(double time) noexcept {
    process_totalTargetBrewTime_ = time;
}

bool SystemContext::isProcessBrewPidDisabled() const noexcept {
    return process_brewPidDisabled_;
}

void SystemContext::setProcessBrewPidDisabled(bool disabled) noexcept {
    process_brewPidDisabled_ = disabled;
}

double SystemContext::processPreviousInput() const noexcept {
    return process_previousInput_;
}

void SystemContext::setProcessPreviousInput(double input) noexcept {
    process_previousInput_ = input;
}

double SystemContext::processPidAggKi() const noexcept {
    return process_aggKi_;
}

void SystemContext::setProcessPidAggKi(double value) noexcept {
    process_aggKi_ = value;
}

double SystemContext::processPidAggKd() const noexcept {
    return process_aggKd_;
}

void SystemContext::setProcessPidAggKd(double value) noexcept {
    process_aggKd_ = value;
}

double SystemContext::processPidKi() const noexcept {
    return process_aggKi_;
}

void SystemContext::setProcessPidKi(double value) noexcept {
    process_aggKi_ = value;
}

double SystemContext::processPidKd() const noexcept {
    return process_aggKd_;
}

void SystemContext::setProcessPidKd(double value) noexcept {
    process_aggKd_ = value;
}

int SystemContext::processWindowSize() const noexcept {
    return process_windowSize_;
}

void SystemContext::setProcessWindowSize(int size) noexcept {
    process_windowSize_ = size;
}

bool SystemContext::isProcessPidEnabled() const noexcept {
    return process_pidEnabled_;
}

void SystemContext::setProcessPidEnabled(bool enabled) noexcept {
    process_pidEnabled_ = enabled;
}

double* SystemContext::processTemperaturePtr() noexcept {
    return &process_temperature_;
}

double* SystemContext::processPidOutputPtr() noexcept {
    return &process_pidOutput_;
}

double* SystemContext::processSetpointPtr() noexcept {
    return &process_setpoint_;
}

// ===== DISPLAY SNAPSHOT =====

SystemContext::DisplaySnapshot SystemContext::getDisplaySnapshot() const noexcept {
    DisplaySnapshot snapshot;
    snapshot.currentTemperature = process_temperature_;
    snapshot.setpointTemperature = process_setpoint_;
    snapshot.pidOutputPercent = process_pidOutput_;
    snapshot.currentBrewTime = process_currBrewTime_;
    snapshot.targetBrewTime = process_totalTargetBrewTime_;
    snapshot.brewPidDisabled = process_brewPidDisabled_;
    snapshot.pidKp = 0.0;
    snapshot.pidKi = 0.0;
    snapshot.pidKd = 0.0;
    snapshot.pumpOnTime = sensors_currPumpOnTime_;
    snapshot.inputPressure = sensors_inputPressure_;
    snapshot.brewWeight = sensors_currBrewWeight_;
    snapshot.isrCounter = timing_isrCounter_;
    snapshot.displayBufferReady = coordination_displayBufferReady_;
    
    return snapshot;
}

void SystemContext::markDisplayBufferReady(bool ready) noexcept {
    coordination_displayBufferReady_ = ready;
}

// ===== COMMAND/CONTROL ACCESSORS =====

void SystemContext::requestScaleTare() noexcept {
    sensors_scaleTareOn_ = true;
}

void SystemContext::requestScaleCalibration() noexcept {
    sensors_scaleCalibrationOn_ = true;
}

void SystemContext::setHassioDiscoveryRunning(bool running) noexcept {
    coordination_hassioUpdateRunning_ = running;
}

void SystemContext::setHassioFailed(bool failed) noexcept {
    network_hassioFailed_ = failed;
}

// ===== UTILITY ACCESSORS =====

void SystemContext::updatePressureFilter(float input) noexcept {
    sensors_inX_ = input * 0.3f;
    sensors_inSum_ = sensors_inX_ + sensors_inY_;
    sensors_inOld_ = sensors_inSum_;
}

float SystemContext::getPressureFilterOutput() const noexcept {
    return sensors_inSum_;
}

// ===== CRITICAL MACHINE CONTROL ACCESSORS =====

hw_timer_t* SystemContext::machineTimer() noexcept {
    return machine_timer_;
}

void SystemContext::setMachineTimer(hw_timer_t* timer) noexcept {
    machine_timer_ = timer;
}

bool SystemContext::isMachineTimerInitialized() const noexcept {
    return machine_timer_ != nullptr && (uintptr_t)machine_timer_ >= 0x1000;
}

unsigned int SystemContext::isrCounter() const noexcept {
    return timing_isrCounter_;
}

void SystemContext::setIsrCounter(unsigned int value) noexcept {
    timing_isrCounter_ = value;
}

void SystemContext::incrementIsrCounter() noexcept {
    timing_isrCounter_++;
}

bool SystemContext::isEmergencyStopActive() const noexcept {
    return machine_emergencyStop_;
}

void SystemContext::setEmergencyStop(bool active) noexcept {
    machine_emergencyStop_ = active;
}

void SystemContext::triggerEmergencyStop() noexcept {
    machine_emergencyStop_ = true;
    // Could add logging or notifications here in the future
}

// ===== PID ABSTRACTION LAYER =====

void SystemContext::computePid() noexcept {
    if (pid_) {
        pid_->Compute();
    }
}

void SystemContext::setPidTunings(double kp, double ki, double kd, int ponM) noexcept {
    if (pid_) {
        pid_->SetTunings(kp, ki, kd, ponM);
    }
}

void SystemContext::setPidMode(int mode) noexcept {
    if (pid_) {
        pid_->SetMode(mode);
    }
}

void SystemContext::setPidOutputLimits(double min, double max) noexcept {
    if (pid_) {
        pid_->SetOutputLimits(min, max);
    }
}

void SystemContext::setPidIntegratorLimits(double min, double max) noexcept {
    if (pid_) {
        pid_->SetIntegratorLimits(min, max);
    }
}

void SystemContext::setPidSampleTime(int sampleTime) noexcept {
    if (pid_) {
        pid_->SetSampleTime(sampleTime);
    }
}

void SystemContext::setPidSmoothingFactor(double factor) noexcept {
    if (pid_) {
        pid_->SetSmoothingFactor(factor);
    }
}

int SystemContext::pidMode() const noexcept {
    return pid_ ? pid_->GetMode() : MANUAL;
}

double SystemContext::pidKp() const noexcept {
    return pid_ ? pid_->GetKp() : 0.0;
}

double SystemContext::pidKi() const noexcept {
    return pid_ ? pid_->GetKi() : 0.0;
}

double SystemContext::pidKd() const noexcept {
    return pid_ ? pid_->GetKd() : 0.0;
}

double SystemContext::pidLastPPart() const noexcept {
    return pid_ ? pid_->GetLastPPart() : 0.0;
}

double SystemContext::pidLastIPart() const noexcept {
    return pid_ ? pid_->GetLastIPart() : 0.0;
}

double SystemContext::pidLastDPart() const noexcept {
    return pid_ ? pid_->GetLastDPart() : 0.0;
}

double SystemContext::pidInputError() const noexcept {
    return pid_ ? pid_->GetInputError() : 0.0;
}

double SystemContext::pidDeltaInput() const noexcept {
    return pid_ ? pid_->GetDeltaInput() : 0.0;
}

PID* SystemContext::pidController() noexcept {
    return pid_;
}

const PID* SystemContext::pidController() const noexcept {
    return pid_;
}

// ===== SCALE AND SENSOR OPERATIONS =====

bool SystemContext::scaleCalibrationOn() const noexcept {
    return sensors_scaleCalibrationOn_;
}

void SystemContext::setScaleCalibrationOn(bool on) noexcept {
    sensors_scaleCalibrationOn_ = on;
}

bool SystemContext::scaleTareOn() const noexcept {
    return sensors_scaleTareOn_;
}

void SystemContext::setScaleTareOn(bool on) noexcept {
    sensors_scaleTareOn_ = on;
}

double SystemContext::currBrewWeight() const noexcept {
    return sensors_currBrewWeight_;
}

void SystemContext::setCurrBrewWeight(double weight) noexcept {
    sensors_currBrewWeight_ = weight;
}

double SystemContext::currReadingWeight() const noexcept {
    return sensors_currReadingWeight_;
}

void SystemContext::setCurrReadingWeight(double weight) noexcept {
    sensors_currReadingWeight_ = weight;
}

double SystemContext::currPumpOnTime() const noexcept {
    return sensors_currPumpOnTime_;
}

void SystemContext::setCurrPumpOnTime(double time) noexcept {
    sensors_currPumpOnTime_ = time;
}

float SystemContext::inputPressure() const noexcept {
    return sensors_inputPressure_;
}

void SystemContext::setInputPressure(float pressure) noexcept {
    sensors_inputPressure_ = pressure;
}

bool SystemContext::scaleFailure() const noexcept {
    return sensors_scaleFailure_;
}

void SystemContext::setScaleFailure(bool failed) noexcept {
    sensors_scaleFailure_ = failed;
}

// ===== NETWORK MANAGER REFERENCES =====

CleverCoffeeWiFiManager* SystemContext::wifiManager() noexcept {
    return cleverCoffeeWiFiManager_;
}

void SystemContext::setWifiManager(CleverCoffeeWiFiManager* manager) noexcept {
    cleverCoffeeWiFiManager_ = manager;
}

// webServerManager() and setWebServerManager() are defined inline in header

bool SystemContext::offlineMode() const noexcept {
    return network_offlineMode_;
}

void SystemContext::setOfflineMode(bool offline) noexcept {
    network_offlineMode_ = offline;
}

bool SystemContext::hassioDiscoveryRunning() const noexcept {
    return coordination_hassioUpdateRunning_;
}


bool SystemContext::hassioFailed() const noexcept {
    return network_hassioFailed_;
}


unsigned int SystemContext::wifiReconnects() const noexcept {
    return network_wifiReconnects_;
}

void SystemContext::setWifiReconnects(unsigned int count) noexcept {
    network_wifiReconnects_ = count;
}

// ===== MACHINE MODE FLAGS =====

bool SystemContext::steamMode() const noexcept {
    return machine_steamON_;
}

void SystemContext::setSteamMode(bool on) noexcept {
    machine_steamON_ = on;
}

bool SystemContext::steamFirstOn() const noexcept {
    return machine_steamFirstON_;
}

void SystemContext::setSteamFirstOn(bool on) noexcept {
    machine_steamFirstON_ = on;
}

bool SystemContext::backflushMode() const noexcept {
    return machine_backflushOn_;
}

void SystemContext::setBackflushMode(bool on) noexcept {
    machine_backflushOn_ = on;
}
// ===== DISPLAY COORDINATION =====

bool SystemContext::displayBufferReady() const noexcept {
    return coordination_displayBufferReady_;
}

void SystemContext::setDisplayBufferReady(bool ready) noexcept {
    coordination_displayBufferReady_ = ready;
}

// ===== PRESSURE FILTER VARIABLES =====

float SystemContext::inX() const noexcept {
    return sensors_inX_;
}

void SystemContext::setInX(float value) noexcept {
    sensors_inX_ = value;
}

float SystemContext::inY() const noexcept {
    return sensors_inY_;
}

void SystemContext::setInY(float value) noexcept {
    sensors_inY_ = value;
}

float SystemContext::inOld() const noexcept {
    return sensors_inOld_;
}

void SystemContext::setInOld(float value) noexcept {
    sensors_inOld_ = value;
}

float SystemContext::inSum() const noexcept {
    return sensors_inSum_;
}

void SystemContext::setInSum(float value) noexcept {
    sensors_inSum_ = value;
}

float SystemContext::inputPressureFilter() const noexcept {
    return sensors_inputPressureFilter_;
}

float SystemContext::preBrewWeight() const noexcept {
    return sensors_preBrewWeight_;
}

void SystemContext::setPreBrewWeight(float weight) noexcept {
    sensors_preBrewWeight_ = weight;
}

const char* SystemContext::sysVersion() const noexcept {
    return sysVersion_;
}

// ===== GLOBAL STATE AND INITIALIZATION =====

// WiFi password definition
extern const char* WIFI_PASSWORD;

// Global system context reference
extern CleverCoffee::SystemContext* g_systemContext;

}  // namespace CleverCoffee

// ===== GLOBAL DEFINITIONS (outside namespace) =====

// WiFi password definition
const char* WIFI_PASSWORD = WM_PASS;

// Global system context reference
CleverCoffee::SystemContext* CleverCoffee::g_systemContext = nullptr;

// ===== HANDLER INSTANCES =====

// Handler instances (static storage duration, initialized at program startup)
static BrewHandler     brewHandler;
static HotWaterHandler hotWaterHandler;
static PowerHandler    powerHandler;
static SteamHandler    steamHandler;

// ===== HANDLER INITIALIZATION FUNCTION =====

void initializeHandlers(CleverCoffee::SystemContext* systemContext) {
    // Validate that systemContext is provided and valid
    if (!systemContext) {
        LOG(ERROR, "initializeHandlers: systemContext is nullptr");
        return;
    }
    
    // Initialize handler hardware using the passed systemContext parameter
    // Use a reference to avoid repeated dereferences
    auto& hwContext = systemContext->hardwareContext();
    
    // Set hardware on all handlers
    brewHandler.setHardware(hwContext.brewSwitch(), hwContext.valveRelay());
    hotWaterHandler.setHardware(hwContext.hotWaterSwitch());
    powerHandler.setHardware(hwContext.powerSwitch());
    steamHandler.setHardware(hwContext.steamSwitch());
    
    // Register in SystemContext if provided
    if (systemContext) {
        // Set SystemContext on handlers for state machine access
        brewHandler.setSystemContext(systemContext);
        hotWaterHandler.setSystemContext(systemContext);
        powerHandler.setSystemContext(systemContext);
        steamHandler.setSystemContext(systemContext);
        
        // Register handlers with SystemContext
        systemContext->setBrewHandler(&brewHandler);
        systemContext->setHotWaterHandler(&hotWaterHandler);
        systemContext->setPowerHandler(&powerHandler);
        systemContext->setSteamHandler(&steamHandler);
    }
}
