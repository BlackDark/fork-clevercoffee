#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include <PID_v1.h>  // Required for PID method implementations

// Forward declaration of global g_state - must be at global scope
extern GlobalState g_state;

namespace CleverCoffee {

// ===== PROCESS STATE ACCESSORS =====

double SystemContext::processTemperature() const noexcept {
    return g_state.process.temperature;
}

void SystemContext::setProcessTemperature(double temp) noexcept {
    g_state.process.temperature = temp;
}

double SystemContext::processSetpoint() const noexcept {
    return g_state.process.setpoint;
}

void SystemContext::setProcessSetpoint(double setpoint) noexcept {
    g_state.process.setpoint = setpoint;
}

double SystemContext::processPidOutput() const noexcept {
    return g_state.process.pidOutput;
}

void SystemContext::setProcessPidOutput(double output) noexcept {
    g_state.process.pidOutput = output;
}

double SystemContext::processCurrentBrewTime() const noexcept {
    return g_state.process.currBrewTime;
}

void SystemContext::setProcessCurrentBrewTime(double time) noexcept {
    g_state.process.currBrewTime = time;
}

double SystemContext::processTotalTargetBrewTime() const noexcept {
    return g_state.process.totalTargetBrewTime;
}

void SystemContext::setProcessTotalTargetBrewTime(double time) noexcept {
    g_state.process.totalTargetBrewTime = time;
}

bool SystemContext::isProcessBrewPidDisabled() const noexcept {
    return g_state.process.brewPidDisabled;
}

void SystemContext::setProcessBrewPidDisabled(bool disabled) noexcept {
    g_state.process.brewPidDisabled = disabled;
}

double SystemContext::processPreviousInput() const noexcept {
    return g_state.process.previousInput;
}

void SystemContext::setProcessPreviousInput(double input) noexcept {
    g_state.process.previousInput = input;
}

double SystemContext::processPidAggKi() const noexcept {
    return g_state.process.aggKi;
}

void SystemContext::setProcessPidAggKi(double value) noexcept {
    g_state.process.aggKi = value;
}

double SystemContext::processPidAggKd() const noexcept {
    return g_state.process.aggKd;
}

void SystemContext::setProcessPidAggKd(double value) noexcept {
    g_state.process.aggKd = value;
}

double SystemContext::processPidKi() const noexcept {
    return g_state.process.aggKi;
}

void SystemContext::setProcessPidKi(double value) noexcept {
    g_state.process.aggKi = value;
}

double SystemContext::processPidKd() const noexcept {
    return g_state.process.aggKd;
}

void SystemContext::setProcessPidKd(double value) noexcept {
    g_state.process.aggKd = value;
}

int SystemContext::processWindowSize() const noexcept {
    return g_state.process.windowSize;
}

void SystemContext::setProcessWindowSize(int size) noexcept {
    g_state.process.windowSize = size;
}

bool SystemContext::isProcessPidEnabled() const noexcept {
    return g_state.process.pidEnabled;
}

void SystemContext::setProcessPidEnabled(bool enabled) noexcept {
    g_state.process.pidEnabled = enabled;
}

double* SystemContext::processTemperaturePtr() noexcept {
    return &g_state.process.temperature;
}

double* SystemContext::processPidOutputPtr() noexcept {
    return &g_state.process.pidOutput;
}

double* SystemContext::processSetpointPtr() noexcept {
    return &g_state.process.setpoint;
}

// ===== DISPLAY SNAPSHOT =====

SystemContext::DisplaySnapshot SystemContext::getDisplaySnapshot() const noexcept {
    DisplaySnapshot snapshot;
    snapshot.currentTemperature = g_state.process.temperature;
    snapshot.setpointTemperature = g_state.process.setpoint;
    snapshot.pidOutputPercent = g_state.process.pidOutput;
    snapshot.currentBrewTime = g_state.process.currBrewTime;
    snapshot.targetBrewTime = g_state.process.totalTargetBrewTime;
    snapshot.brewPidDisabled = g_state.process.brewPidDisabled;
    snapshot.pidKp = 0.0;
    snapshot.pidKi = 0.0;
    snapshot.pidKd = 0.0;
    snapshot.pumpOnTime = g_state.sensors.currPumpOnTime;
    snapshot.inputPressure = g_state.sensors.inputPressure;
    snapshot.brewWeight = g_state.sensors.currBrewWeight;
    snapshot.isrCounter = g_state.timing.isrCounter;
    snapshot.displayBufferReady = g_state.coordination.displayBufferReady;
    
    return snapshot;
}

void SystemContext::markDisplayBufferReady(bool ready) noexcept {
    g_state.coordination.displayBufferReady = ready;
}

// ===== COMMAND/CONTROL ACCESSORS =====

void SystemContext::requestScaleTare() noexcept {
    g_state.sensors.scaleTareOn = true;
}

void SystemContext::requestScaleCalibration() noexcept {
    g_state.sensors.scaleCalibrationOn = true;
}

void SystemContext::setHassioDiscoveryRunning(bool running) noexcept {
    g_state.coordination.hassioUpdateRunning = running;
}

void SystemContext::setHassioFailed(bool failed) noexcept {
    g_state.network.hassioFailed = failed;
}

// ===== UTILITY ACCESSORS =====

void SystemContext::updatePressureFilter(float input) noexcept {
    g_state.sensors.inX = input * 0.3f;
    g_state.sensors.inSum = g_state.sensors.inX + g_state.sensors.inY;
    g_state.sensors.inOld = g_state.sensors.inSum;
}

float SystemContext::getPressureFilterOutput() const noexcept {
    return g_state.sensors.inSum;
}

// ===== CRITICAL MACHINE CONTROL ACCESSORS =====

hw_timer_t* SystemContext::machineTimer() noexcept {
    return g_state.machine.timer;
}

void SystemContext::setMachineTimer(hw_timer_t* timer) noexcept {
    g_state.machine.timer = timer;
}

bool SystemContext::isMachineTimerInitialized() const noexcept {
    return g_state.machine.timer != nullptr && (uintptr_t)g_state.machine.timer >= 0x1000;
}

unsigned int SystemContext::isrCounter() const noexcept {
    return g_state.timing.isrCounter;
}

void SystemContext::setIsrCounter(unsigned int value) noexcept {
    g_state.timing.isrCounter = value;
}

void SystemContext::incrementIsrCounter() noexcept {
    g_state.timing.isrCounter++;
}

bool SystemContext::isEmergencyStopActive() const noexcept {
    return g_state.machine.emergencyStop;
}

void SystemContext::setEmergencyStop(bool active) noexcept {
    g_state.machine.emergencyStop = active;
}

void SystemContext::triggerEmergencyStop() noexcept {
    g_state.machine.emergencyStop = true;
    // Could add logging or notifications here in the future
}

// ===== PID ABSTRACTION LAYER =====

void SystemContext::computePid() noexcept {
    if (g_state.pid) {
        g_state.pid->Compute();
    }
}

void SystemContext::setPidTunings(double kp, double ki, double kd, int ponM) noexcept {
    if (g_state.pid) {
        g_state.pid->SetTunings(kp, ki, kd, ponM);
    }
}

void SystemContext::setPidMode(int mode) noexcept {
    if (g_state.pid) {
        g_state.pid->SetMode(mode);
    }
}

void SystemContext::setPidOutputLimits(double min, double max) noexcept {
    if (g_state.pid) {
        g_state.pid->SetOutputLimits(min, max);
    }
}

void SystemContext::setPidIntegratorLimits(double min, double max) noexcept {
    if (g_state.pid) {
        g_state.pid->SetIntegratorLimits(min, max);
    }
}

void SystemContext::setPidSampleTime(int sampleTime) noexcept {
    if (g_state.pid) {
        g_state.pid->SetSampleTime(sampleTime);
    }
}

void SystemContext::setPidSmoothingFactor(double factor) noexcept {
    if (g_state.pid) {
        g_state.pid->SetSmoothingFactor(factor);
    }
}

int SystemContext::pidMode() const noexcept {
    return g_state.pid ? g_state.pid->GetMode() : MANUAL;
}

double SystemContext::pidKp() const noexcept {
    return g_state.pid ? g_state.pid->GetKp() : 0.0;
}

double SystemContext::pidKi() const noexcept {
    return g_state.pid ? g_state.pid->GetKi() : 0.0;
}

double SystemContext::pidKd() const noexcept {
    return g_state.pid ? g_state.pid->GetKd() : 0.0;
}

double SystemContext::pidLastPPart() const noexcept {
    return g_state.pid ? g_state.pid->GetLastPPart() : 0.0;
}

double SystemContext::pidLastIPart() const noexcept {
    return g_state.pid ? g_state.pid->GetLastIPart() : 0.0;
}

double SystemContext::pidLastDPart() const noexcept {
    return g_state.pid ? g_state.pid->GetLastDPart() : 0.0;
}

double SystemContext::pidInputError() const noexcept {
    return g_state.pid ? g_state.pid->GetInputError() : 0.0;
}

double SystemContext::pidDeltaInput() const noexcept {
    return g_state.pid ? g_state.pid->GetDeltaInput() : 0.0;
}

PID* SystemContext::pidController() noexcept {
    return g_state.pid;
}

const PID* SystemContext::pidController() const noexcept {
    return g_state.pid;
}

// ===== SCALE AND SENSOR OPERATIONS =====

bool SystemContext::scaleCalibrationOn() const noexcept {
    return g_state.sensors.scaleCalibrationOn;
}

void SystemContext::setScaleCalibrationOn(bool on) noexcept {
    g_state.sensors.scaleCalibrationOn = on;
}

bool SystemContext::scaleTareOn() const noexcept {
    return g_state.sensors.scaleTareOn;
}

void SystemContext::setScaleTareOn(bool on) noexcept {
    g_state.sensors.scaleTareOn = on;
}

double SystemContext::currBrewWeight() const noexcept {
    return g_state.sensors.currBrewWeight;
}

void SystemContext::setCurrBrewWeight(double weight) noexcept {
    g_state.sensors.currBrewWeight = weight;
}

double SystemContext::currReadingWeight() const noexcept {
    return g_state.sensors.currReadingWeight;
}

void SystemContext::setCurrReadingWeight(double weight) noexcept {
    g_state.sensors.currReadingWeight = weight;
}

double SystemContext::currPumpOnTime() const noexcept {
    return g_state.sensors.currPumpOnTime;
}

void SystemContext::setCurrPumpOnTime(double time) noexcept {
    g_state.sensors.currPumpOnTime = time;
}

float SystemContext::inputPressure() const noexcept {
    return g_state.sensors.inputPressure;
}

void SystemContext::setInputPressure(float pressure) noexcept {
    g_state.sensors.inputPressure = pressure;
}

bool SystemContext::scaleFailure() const noexcept {
    return g_state.sensors.scaleFailure;
}

void SystemContext::setScaleFailure(bool failed) noexcept {
    g_state.sensors.scaleFailure = failed;
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
    return g_state.network.offlineMode;
}

void SystemContext::setOfflineMode(bool offline) noexcept {
    g_state.network.offlineMode = offline;
}

bool SystemContext::hassioDiscoveryRunning() const noexcept {
    return g_state.coordination.hassioUpdateRunning;
}


bool SystemContext::hassioFailed() const noexcept {
    return g_state.network.hassioFailed;
}


unsigned int SystemContext::wifiReconnects() const noexcept {
    return g_state.network.wifiReconnects;
}

void SystemContext::setWifiReconnects(unsigned int count) noexcept {
    g_state.network.wifiReconnects = count;
}

// ===== MACHINE MODE FLAGS =====

bool SystemContext::steamMode() const noexcept {
    return g_state.machine.steamON;
}

void SystemContext::setSteamMode(bool on) noexcept {
    g_state.machine.steamON = on;
}

bool SystemContext::steamFirstOn() const noexcept {
    return g_state.machine.steamFirstON;
}

void SystemContext::setSteamFirstOn(bool on) noexcept {
    g_state.machine.steamFirstON = on;
}

bool SystemContext::backflushMode() const noexcept {
    return g_state.machine.backflushOn;
}

void SystemContext::setBackflushMode(bool on) noexcept {
    g_state.machine.backflushOn = on;
}
// ===== DISPLAY COORDINATION =====

bool SystemContext::displayBufferReady() const noexcept {
    return g_state.coordination.displayBufferReady;
}

void SystemContext::setDisplayBufferReady(bool ready) noexcept {
    g_state.coordination.displayBufferReady = ready;
}

// ===== PRESSURE FILTER VARIABLES =====

float SystemContext::inX() const noexcept {
    return g_state.sensors.inX;
}

void SystemContext::setInX(float value) noexcept {
    g_state.sensors.inX = value;
}

float SystemContext::inY() const noexcept {
    return g_state.sensors.inY;
}

void SystemContext::setInY(float value) noexcept {
    g_state.sensors.inY = value;
}

float SystemContext::inOld() const noexcept {
    return g_state.sensors.inOld;
}

void SystemContext::setInOld(float value) noexcept {
    g_state.sensors.inOld = value;
}

float SystemContext::inSum() const noexcept {
    return g_state.sensors.inSum;
}

void SystemContext::setInSum(float value) noexcept {
    g_state.sensors.inSum = value;
}

float SystemContext::inputPressureFilter() const noexcept {
    return g_state.sensors.inputPressureFilter;
}

float SystemContext::preBrewWeight() const noexcept {
    return g_state.sensors.preBrewWeight;
}

void SystemContext::setPreBrewWeight(float weight) noexcept {
    g_state.sensors.preBrewWeight = weight;
}

const char* SystemContext::sysVersion() const noexcept {
    return g_state.sysVersion;
}


}  // namespace CleverCoffee
