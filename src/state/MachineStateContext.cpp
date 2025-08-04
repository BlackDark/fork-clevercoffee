/**
 * @file MachineStateContext.cpp
 * @brief Implementation of MachineStateContext for state machine access to machine resources
 */

#include "MachineStateContext.h"
#include "../Config.h"
#include "../display/DisplayManager.h"
#include "../hardware/HardwareManager.h"
#include "../network/CleverCoffeeWiFiManager.h"
#include "../network/MQTTManager.h"
#include "../sensors/SensorManager.h"
// #include "../hotWaterHandler.h" - removed to avoid circular dependencies
#include "../utils/brewUtils.h"
#include "Logger.h"
#include <Arduino.h>

// Forward declarations to avoid circular dependencies
// These functions are defined in main.cpp and various handler files
extern void setSteamMode(const bool steamMode);
extern void setRuntimePidState(bool enabled);
extern void performSafeShutdown();
extern bool manualFlush();
extern bool checkHotWaterStates();
extern bool brew();

// Forward declaration for standby timer reset function
extern void resetStandbyTimer(LegacyMachineState state);

MachineStateContext::MachineStateContext(DisplayManager* displayManager, HardwareManager* hardwareManager, SensorManager* sensorManager, CleverCoffeeWiFiManager* wifiManager, MQTTManager* mqttManager) :
    displayManager_(displayManager), hardwareManager_(hardwareManager), sensorManager_(sensorManager), wifiManager_(wifiManager), mqttManager_(mqttManager) {
}

// === Hardware Component Access ===

TempSensor* MachineStateContext::getTemperatureSensor() const {
    return hardwareManager_ ? hardwareManager_->getTempSensor() : nullptr;
}

Switch* MachineStateContext::getWaterTankSensor() const {
    return hardwareManager_ ? hardwareManager_->getWaterTankSensor() : nullptr;
}

Switch* MachineStateContext::getBrewSwitch() const {
    return hardwareManager_ ? hardwareManager_->getBrewSwitch() : nullptr;
}

Switch* MachineStateContext::getSteamSwitch() const {
    return hardwareManager_ ? hardwareManager_->getSteamSwitch() : nullptr;
}

Switch* MachineStateContext::getHotWaterSwitch() const {
    return hardwareManager_ ? hardwareManager_->getHotWaterSwitch() : nullptr;
}

Switch* MachineStateContext::getPowerSwitch() const {
    return hardwareManager_ ? hardwareManager_->getPowerSwitch() : nullptr;
}

Relay* MachineStateContext::getHeaterRelay() const {
    return hardwareManager_ ? &hardwareManager_->getHeaterRelay() : nullptr;
}

Relay* MachineStateContext::getPumpRelay() const {
    return hardwareManager_ ? &hardwareManager_->getPumpRelay() : nullptr;
}

Relay* MachineStateContext::getValveRelay() const {
    return hardwareManager_ ? &hardwareManager_->getValveRelay() : nullptr;
}

LED* MachineStateContext::getStatusLED() const {
    return hardwareManager_ ? hardwareManager_->getStatusLed() : nullptr;
}

LED* MachineStateContext::getBrewLED() const {
    return hardwareManager_ ? hardwareManager_->getBrewLed() : nullptr;
}

LED* MachineStateContext::getSteamLED() const {
    return hardwareManager_ ? hardwareManager_->getSteamLed() : nullptr;
}

Scale* MachineStateContext::getScale() const {
    return sensorManager_ ? sensorManager_->getScale() : nullptr;
}

// === Sensor Data Access ===

double MachineStateContext::getCurrentTemperature() const {
    return sensorManager_ ? sensorManager_->getCurrentTemperature() : 0.0;
}

bool MachineStateContext::hasTemperatureError() const {
    return sensorManager_ ? sensorManager_->hasTemperatureError() : true;
}

bool MachineStateContext::isWaterTankFull() const {
    return sensorManager_ ? sensorManager_->isWaterTankFull() : g_state.machine.waterTankFull;
}

float MachineStateContext::getCurrentPressure() const {
    return sensorManager_ ? sensorManager_->getCurrentPressure() : 0.0f;
}

float MachineStateContext::getFilteredPressure() const {
    return sensorManager_ ? sensorManager_->getFilteredPressure() : 0.0f;
}

float MachineStateContext::getCurrentWeight() const {
    return sensorManager_ ? sensorManager_->getCurrentWeight() : 0.0f;
}

float MachineStateContext::getCurrentBrewWeight() const {
    return sensorManager_ ? sensorManager_->getCurrentBrewWeight() : 0.0f;
}

bool MachineStateContext::hasScaleError() const {
    return sensorManager_ ? sensorManager_->hasScaleError() : false;
}

bool MachineStateContext::hasSensorError() const {
    return sensorManager_ ? sensorManager_->hasSensorError() : false;
}

// === Process Control Functions ===

// TODO those are wrong the functions behind like brew() and manualFlush() are triggering those events

bool MachineStateContext::isBrewActive() const {
    return brew();
}

bool MachineStateContext::isManualFlushActive() const {
    return manualFlush();
}

bool MachineStateContext::isSteamActive() const {
    return g_state.machine.steamON;
}

bool MachineStateContext::isHotWaterActive() const {
    // Simplified implementation - check if machine is in hot water state
    return (g_state.machine.machineState == LegacyMachineState::kHotWater);
}

bool MachineStateContext::isBackflushActive() const {
    return g_state.machine.backflushOn;
}

// === System State Access ===

bool MachineStateContext::isPidEnabled() const {
    return Config::getInstance().get<bool>("pid.enabled");
}

bool MachineStateContext::isEmergencyStop() const {
    return g_state.machine.emergencyStop;
}

bool MachineStateContext::shouldEnterStandby() const {
    return Config::getInstance().get<bool>("standby.enabled") && g_state.standby.standbyModeRemainingTimeMillis == 0;
}

unsigned long MachineStateContext::getStandbyRemainingTime() const {
    return g_state.standby.standbyModeRemainingTimeMillis;
}

// === Configuration Access ===

Config& MachineStateContext::getConfig() const {
    return Config::getInstance();
}

// Template specializations for common types to avoid header dependency issues
template <>
bool MachineStateContext::getConfigValue<bool>(const char* key) const {
    return getConfig().get<bool>(key);
}

template <>
int MachineStateContext::getConfigValue<int>(const char* key) const {
    return getConfig().get<int>(key);
}

template <>
double MachineStateContext::getConfigValue<double>(const char* key) const {
    return getConfig().get<double>(key);
}

// === Timing Functions ===

unsigned long MachineStateContext::getCurrentTime() const {
    return millis();
}

void MachineStateContext::resetStandbyTimer(int stateId) const {
    // Convert state ID to MachineState enum and call existing function
    resetStandbyTimer(static_cast<LegacyMachineState>(stateId));
}

// === Control Functions ===

void MachineStateContext::setSteamMode(bool enabled) const {
    ::setSteamMode(enabled);
}

void MachineStateContext::setPidRuntimeState(bool enabled) const {
    setRuntimePidState(enabled);
}

void MachineStateContext::performSafeShutdown() const {
    ::performSafeShutdown();
}

// === Display Functions ===

U8G2* MachineStateContext::getDisplay() const {
    return displayManager_ ? displayManager_->get() : nullptr;
}

void MachineStateContext::setDisplayPowerSave(int mode) const {
    if (U8G2* display = getDisplay()) {
        display->setPowerSave(mode);
    }
}

// === Logging Functions ===

void MachineStateContext::logStateTransition(int fromState, int toState, const char* reason) const {
    if (reason) {
        LOGF(INFO, "State transition: %d -> %d (%s)", fromState, toState, reason);
    }
    else {
        LOGF(INFO, "State transition: %d -> %d", fromState, toState);
    }
}

void MachineStateContext::logStateEntry(int stateId, const char* stateName) const {
    LOGF(INFO, "Entering state %d (%s)", stateId, stateName);
}

void MachineStateContext::logStateExit(int stateId, const char* stateName) const {
    LOGF(INFO, "Exiting state %d (%s)", stateId, stateName);
}

// === MQTT Integration ===

void MachineStateContext::resetMqttReconnectCount() const {
    g_state.network.MQTTReCnctCount = 0;
}
