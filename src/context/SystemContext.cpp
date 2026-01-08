#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"

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

}  // namespace CleverCoffee
