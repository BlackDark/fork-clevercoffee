#include "clevercoffee/context/SystemContext.h"

#include "clevercoffee/Logger.h"
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/handlers/PowerHandler.h"
#include "clevercoffee/handlers/SteamHandler.h"

#include <PID_v1.h> // Required for PID method implementations
#include <memory>

namespace CleverCoffee {

// ===== PROCESS STATE ACCESSORS =====
// NOTE: These methods delegate to ProcessState class
// All deprecated process_* members have been removed - ProcessState is the single source of truth

double SystemContext::processTemperature() const noexcept {
    return processState_.temperature();
}

void SystemContext::setProcessTemperature(double temp) noexcept {
    processState_.setTemperature(temp);
}

double SystemContext::processSetpoint() const noexcept {
    return processState_.setpoint();
}

void SystemContext::setProcessSetpoint(double setpoint) noexcept {
    processState_.setSetpoint(setpoint);
}

double SystemContext::processPidOutput() const noexcept {
    return processState_.pidOutput();
}

void SystemContext::setProcessPidOutput(double output) noexcept {
    processState_.setPidOutput(output);
}

double SystemContext::processCurrentBrewTime() const noexcept {
    return processState_.currentBrewTime();
}

void SystemContext::setProcessCurrentBrewTime(double time) noexcept {
    processState_.setCurrentBrewTime(time);
}

double SystemContext::processTotalTargetBrewTime() const noexcept {
    return processState_.totalTargetBrewTime();
}

void SystemContext::setProcessTotalTargetBrewTime(double time) noexcept {
    processState_.setTotalTargetBrewTime(time);
}

bool SystemContext::isProcessBrewPidDisabled() const noexcept {
    return processState_.brewPidDisabled();
}

void SystemContext::setProcessBrewPidDisabled(bool disabled) noexcept {
    processState_.setBrewPidDisabled(disabled);
}

double SystemContext::processPreviousInput() const noexcept {
    return processState_.previousInput();
}

void SystemContext::setProcessPreviousInput(double input) noexcept {
    processState_.setPreviousInput(input);
}

double SystemContext::processPidAggKi() const noexcept {
    return processState_.pidAggKi();
}

void SystemContext::setProcessPidAggKi(double value) noexcept {
    processState_.setPidAggKi(value);
}

double SystemContext::processPidAggKd() const noexcept {
    return processState_.pidAggKd();
}

void SystemContext::setProcessPidAggKd(double value) noexcept {
    processState_.setPidAggKd(value);
}

double SystemContext::processPidKi() const noexcept {
    return processState_.pidKi();
}

void SystemContext::setProcessPidKi(double value) noexcept {
    processState_.setPidKi(value);
}

double SystemContext::processPidKd() const noexcept {
    return processState_.pidKd();
}

void SystemContext::setProcessPidKd(double value) noexcept {
    processState_.setPidKd(value);
}

int SystemContext::processWindowSize() const noexcept {
    return processState_.windowSize();
}

void SystemContext::setProcessWindowSize(int size) noexcept {
    processState_.setWindowSize(size);
}

bool SystemContext::isProcessPidEnabled() const noexcept {
    return processState_.pidEnabled();
}

void SystemContext::setProcessPidEnabled(bool enabled) noexcept {
    processState_.setPidEnabled(enabled);
}

double* SystemContext::processTemperaturePtr() noexcept {
    return processState_.temperaturePtr();
}

double* SystemContext::processPidOutputPtr() noexcept {
    return processState_.pidOutputPtr();
}

double* SystemContext::processSetpointPtr() noexcept {
    return processState_.setpointPtr();
}

// ===== DISPLAY SNAPSHOT =====

SystemContext::DisplaySnapshot SystemContext::getDisplaySnapshot() const noexcept {
    DisplaySnapshot snapshot;
    snapshot.currentTemperature  = processState_.temperature();
    snapshot.setpointTemperature = processState_.setpoint();
    snapshot.pidOutputPercent    = processState_.pidOutput();
    snapshot.currentBrewTime     = processState_.currentBrewTime();
    snapshot.targetBrewTime      = processState_.totalTargetBrewTime();
    snapshot.brewPidDisabled     = processState_.brewPidDisabled();
    // Get actual PID tuning values from PID controller
    snapshot.pidKp              = pidKp();
    snapshot.pidKi              = pidKi();
    snapshot.pidKd              = pidKd();
    snapshot.pumpOnTime         = sensorState_.currPumpOnTime();
    snapshot.inputPressure      = sensorState_.inputPressure();
    snapshot.brewWeight         = sensorState_.currBrewWeight();
    snapshot.isrCounter         = timing_isrCounter_.load(std::memory_order_relaxed);
    snapshot.displayBufferReady = uiCoordinator_.isDisplayBufferReady();

    return snapshot;
}

// ===== COMMAND/CONTROL ACCESSORS =====

void SystemContext::requestScaleTare() noexcept {
    sensorState_.setScaleTareOn(true);
}

void SystemContext::requestScaleCalibration() noexcept {
    sensorState_.setScaleCalibrationOn(true);
}

void SystemContext::setHassioDiscoveryRunning(bool running) noexcept {
    // Delegate to UICoordinator (single source of truth)
    uiCoordinator_.setHassioUpdateRunning(running);
}

void SystemContext::setHassioFailed(bool failed) noexcept {
    // Delegate to NetworkCoordinator (single source of truth)
    networkCoordinator_.setHassioFailed(failed);
}

// ===== UTILITY ACCESSORS =====
// NOTE: Pressure filter is now fully internal to SensorCoordinator.
// The deprecated updatePressureFilter/getPressureFilterOutput methods have been removed.

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

bool SystemContext::isISRReady() const noexcept {
    return timing_isrReady_.load(std::memory_order_acquire);
}

void SystemContext::markISRReady() noexcept {
    timing_isrReady_.store(true, std::memory_order_release);
}

unsigned int SystemContext::isrCounter() const noexcept {
    return timing_isrCounter_.load(std::memory_order_relaxed);
}

void SystemContext::setIsrCounter(unsigned int value) noexcept {
    timing_isrCounter_.store(value, std::memory_order_relaxed);
}

void SystemContext::incrementIsrCounter() noexcept {
    timing_isrCounter_.fetch_add(1, std::memory_order_relaxed);
}

bool SystemContext::isEmergencyStopActive() const noexcept {
    // Delegate to MachineStateContext (single source of truth)
    if (machineStateContext_) {
        return machineStateContext_->isEmergencyStop();
    }
    // Fallback: return false if not initialized (should not happen after initialization)
    return false;
}

void SystemContext::setEmergencyStop(bool active) noexcept {
    // Delegate to MachineStateContext (single source of truth)
    if (machineStateContext_) {
        machineStateContext_->setEmergencyStop(active);
    }
}

void SystemContext::triggerEmergencyStop() noexcept {
    // Delegate to MachineStateContext (single source of truth)
    if (machineStateContext_) {
        machineStateContext_->setEmergencyStop(true);
    }
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

void SystemContext::setPidController(PID* pid) noexcept {
    pid_ = pid;
}

PID* SystemContext::pidController() noexcept {
    return pid_;
}

const PID* SystemContext::pidController() const noexcept {
    return pid_;
}

// ===== SENSOR OPERATIONS =====
// NOTE: Deprecated scale/weight/pressure accessors have been removed.
// Use sensorCoordinator() methods directly:
//   - sensorCoordinator().getBrewWeight() instead of currBrewWeight()
//   - sensorCoordinator().getWeight() instead of currReadingWeight()
//   - sensorCoordinator().isScaleCalibrationMode() instead of scaleCalibrationOn()
//   - sensorCoordinator().isScaleTareMode() instead of scaleTareOn()
//   - sensorCoordinator().getPressure() instead of inputPressure()
//   - sensorCoordinator().getFilteredPressure() instead of inputPressureFilter()

double SystemContext::currPumpOnTime() const noexcept {
    return sensorState_.currPumpOnTime();
}

void SystemContext::setCurrPumpOnTime(double time) noexcept {
    sensorState_.setCurrPumpOnTime(time);
}

bool SystemContext::scaleFailure() const noexcept {
    return sensorState_.scaleFailure();
}

void SystemContext::setScaleFailure(bool failed) noexcept {
    sensorState_.setScaleFailure(failed);
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
    // Delegate to NetworkCoordinator (single source of truth)
    return networkCoordinator_.isOfflineMode();
}

void SystemContext::setOfflineMode(bool offline) noexcept {
    // Delegate to NetworkCoordinator (single source of truth)
    networkCoordinator_.setOfflineMode(offline);
}

bool SystemContext::hassioDiscoveryRunning() const noexcept {
    // Delegate to UICoordinator (single source of truth)
    return uiCoordinator_.isHassioUpdateRunning();
}

bool SystemContext::hassioFailed() const noexcept {
    // Delegate to NetworkCoordinator (single source of truth)
    return networkCoordinator_.hasHassioFailed();
}

unsigned int SystemContext::wifiReconnects() const noexcept {
    // Delegate to NetworkCoordinator (single source of truth)
    return networkCoordinator_.getWifiReconnects();
}

// ===== MACHINE MODE FLAGS =====

bool SystemContext::steamMode() const noexcept {
    // Delegate to MachineStateContext (single source of truth)
    if (machineStateContext_) {
        return machineStateContext_->isSteamModeActive();
    }
    // Fallback: return false if not initialized (should not happen after initialization)
    return false;
}

void SystemContext::setSteamMode(bool on) noexcept {
    // Delegate to MachineStateContext (single source of truth)
    if (machineStateContext_) {
        machineStateContext_->setSteamModeActive(on);
    }
}

bool SystemContext::steamFirstOn() const noexcept {
    // Delegate to MachineStateContext (single source of truth)
    if (machineStateContext_) {
        return machineStateContext_->isSteamFirstActivated();
    }
    // Fallback: return false if not initialized (should not happen after initialization)
    return false;
}

void SystemContext::setSteamFirstOn(bool on) noexcept {
    // Delegate to MachineStateContext (single source of truth)
    if (machineStateContext_) {
        machineStateContext_->setSteamFirstActivated(on);
    }
}

bool SystemContext::backflushMode() const noexcept {
    // Delegate to MachineStateContext (single source of truth)
    if (machineStateContext_) {
        return machineStateContext_->isBackflushModeActive();
    }
    // Fallback: return false if not initialized (should not happen after initialization)
    return false;
}

void SystemContext::setBackflushMode(bool on) noexcept {
    if (machineStateContext_) {
        const bool wasOn = machineStateContext_->isBackflushModeActive();
        machineStateContext_->applyBackflushMode(on);
        if (wasOn && !on) {
            maintenanceCoordinator_.resetSinceBackflush();
        }
    }
}
// ===== DISPLAY COORDINATION =====

bool SystemContext::displayBufferReady() const noexcept {
    // Delegate to UICoordinator (single source of truth)
    return uiCoordinator_.isDisplayBufferReady();
}

void SystemContext::setDisplayBufferReady(bool ready) noexcept {
    // Delegate to UICoordinator (single source of truth)
    if (ready) {
        uiCoordinator_.setDisplayBufferReady();
    } else {
        uiCoordinator_.clearDisplayBufferReady();
    }
}

// NOTE: Deprecated pressure filter methods (inX, setInX, inY, setInY, inOld, setInOld, inSum, setInSum,
// inputPressureFilter) have been removed. Use sensorCoordinator().getFilteredPressure() instead.

float SystemContext::preBrewWeight() const noexcept {
    return sensorState_.preBrewWeight();
}

void SystemContext::setPreBrewWeight(float weight) noexcept {
    sensorState_.setPreBrewWeight(weight);
}

const char* SystemContext::sysVersion() const noexcept {
    return sysVersion_;
}

// ===== GLOBAL STATE AND INITIALIZATION =====

// WiFi password definition
extern const char* WIFI_PASSWORD;

} // namespace CleverCoffee

// ===== GLOBAL DEFINITIONS (outside namespace) =====

// WiFi password definition
const char* WIFI_PASSWORD = WM_PASS;

// ===== HANDLER INSTANCES =====

// Handler instances (static storage duration, created during initialization)
// These are created in initializeHandlers() with SystemContext reference
static std::unique_ptr<BrewHandler>     brewHandler;
static std::unique_ptr<HotWaterHandler> hotWaterHandler;
static std::unique_ptr<PowerHandler>    powerHandler;
static std::unique_ptr<SteamHandler>    steamHandler;

// ===== HANDLER INITIALIZATION FUNCTION =====

void initializeHandlers(CleverCoffee::SystemContext& systemContext) {
    // Create handlers with SystemContext reference and Config (required)
    const auto& config = Config::getInstance();
    brewHandler        = std::make_unique<BrewHandler>(systemContext, config);
    hotWaterHandler    = std::make_unique<HotWaterHandler>(systemContext, config);
    powerHandler       = std::make_unique<PowerHandler>(systemContext, config);
    steamHandler       = std::make_unique<SteamHandler>(systemContext, config);

    // Initialize handler hardware
    auto& hwContext = systemContext.hardwareContext();
    brewHandler->setHardware(hwContext.brewSwitch(), hwContext.valveRelay());
    hotWaterHandler->setHardware(hwContext.hotWaterSwitch());
    powerHandler->setHardware(hwContext.powerSwitch());
    steamHandler->setHardware(hwContext.steamSwitch());

    // Register handlers with SystemContext (non-owning pointers)
    systemContext.setBrewHandler(brewHandler.get());
    systemContext.setHotWaterHandler(hotWaterHandler.get());
    systemContext.setPowerHandler(powerHandler.get());
    systemContext.setSteamHandler(steamHandler.get());
}
