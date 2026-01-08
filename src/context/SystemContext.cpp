#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"

namespace CleverCoffee {

// ===== PROCESS STATE ACCESSORS =====

double SystemContext::processTemperature() const noexcept {
    extern GlobalState g_state;
    return g_state.process.temperature;
}

void SystemContext::setProcessTemperature(double temp) noexcept {
    extern GlobalState g_state;
    g_state.process.temperature = temp;
}

double SystemContext::processSetpoint() const noexcept {
    extern GlobalState g_state;
    return g_state.process.setpoint;
}

void SystemContext::setProcessSetpoint(double setpoint) noexcept {
    extern GlobalState g_state;
    g_state.process.setpoint = setpoint;
}

double SystemContext::processPidOutput() const noexcept {
    extern GlobalState g_state;
    return g_state.process.pidOutput;
}

void SystemContext::setProcessPidOutput(double output) noexcept {
    extern GlobalState g_state;
    g_state.process.pidOutput = output;
}

double SystemContext::processCurrentBrewTime() const noexcept {
    extern GlobalState g_state;
    return g_state.process.currBrewTime;
}

void SystemContext::setProcessCurrentBrewTime(double time) noexcept {
    extern GlobalState g_state;
    g_state.process.currBrewTime = time;
}

double SystemContext::processTotalTargetBrewTime() const noexcept {
    extern GlobalState g_state;
    return g_state.process.totalTargetBrewTime;
}

void SystemContext::setProcessTotalTargetBrewTime(double time) noexcept {
    extern GlobalState g_state;
    g_state.process.totalTargetBrewTime = time;
}

bool SystemContext::isProcessBrewPidDisabled() const noexcept {
    extern GlobalState g_state;
    return g_state.process.brewPidDisabled;
}

void SystemContext::setProcessBrewPidDisabled(bool disabled) noexcept {
    extern GlobalState g_state;
    g_state.process.brewPidDisabled = disabled;
}

double SystemContext::processPreviousInput() const noexcept {
    extern GlobalState g_state;
    return g_state.process.previousInput;
}

void SystemContext::setProcessPreviousInput(double input) noexcept {
    extern GlobalState g_state;
    g_state.process.previousInput = input;
}

double SystemContext::processPidAggKi() const noexcept {
    extern GlobalState g_state;
    return g_state.process.aggKi;
}

void SystemContext::setProcessPidAggKi(double value) noexcept {
    extern GlobalState g_state;
    g_state.process.aggKi = value;
}

double SystemContext::processPidAggKd() const noexcept {
    extern GlobalState g_state;
    return g_state.process.aggKd;
}

void SystemContext::setProcessPidAggKd(double value) noexcept {
    extern GlobalState g_state;
    g_state.process.aggKd = value;
}

double SystemContext::processPidKi() const noexcept {
    extern GlobalState g_state;
    return g_state.process.aggKi;
}

void SystemContext::setProcessPidKi(double value) noexcept {
    extern GlobalState g_state;
    g_state.process.aggKi = value;
}

double SystemContext::processPidKd() const noexcept {
    extern GlobalState g_state;
    return g_state.process.aggKd;
}

void SystemContext::setProcessPidKd(double value) noexcept {
    extern GlobalState g_state;
    g_state.process.aggKd = value;
}

// ===== DISPLAY SNAPSHOT =====

SystemContext::DisplaySnapshot SystemContext::getDisplaySnapshot() const noexcept {
    extern GlobalState g_state;
    
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
    extern GlobalState g_state;
    g_state.coordination.displayBufferReady = ready;
}

// ===== COMMAND/CONTROL ACCESSORS =====

void SystemContext::requestScaleTare() noexcept {
    extern GlobalState g_state;
    g_state.sensors.scaleTareOn = true;
}

void SystemContext::requestScaleCalibration() noexcept {
    extern GlobalState g_state;
    g_state.sensors.scaleCalibrationOn = true;
}

void SystemContext::setHassioDiscoveryRunning(bool running) noexcept {
    extern GlobalState g_state;
    g_state.coordination.hassioUpdateRunning = running;
}

void SystemContext::setHassioFailed(bool failed) noexcept {
    extern GlobalState g_state;
    g_state.network.hassioFailed = failed;
}

// ===== UTILITY ACCESSORS =====

void SystemContext::updatePressureFilter(float input) noexcept {
    extern GlobalState g_state;
    g_state.sensors.inX = input * 0.3f;
    g_state.sensors.inSum = g_state.sensors.inX + g_state.sensors.inY;
    g_state.sensors.inOld = g_state.sensors.inSum;
}

float SystemContext::getPressureFilterOutput() const noexcept {
    extern GlobalState g_state;
    return g_state.sensors.inSum;
}

}  // namespace CleverCoffee
