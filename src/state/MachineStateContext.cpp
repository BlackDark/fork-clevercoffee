/**
 * @file MachineStateContext.cpp
 * @brief Implementation of MachineStateContext for state machine access to machine resources
 */

#include "MachineStateContext.h"
#include "../Config.h"
#include "../control/ProcessController.h"
#include "../display/DisplayManager.h"
#include "../handlers/BrewHandler.h"
#include "../hardware/HardwareManager.h"
#include "../network/CleverCoffeeWiFiManager.h"
#include "../network/MQTTManager.h"
#include "../sensors/SensorManager.h"
// #include "../hotWaterHandler.h" - removed to avoid circular dependencies
#include "../state/GlobalState.h"
#include "../state/MachineStateIds.h"
#include "../utils/SystemUtils.h"
#include "Logger.h"
#include <Arduino.h>

// Forward declarations to avoid circular dependencies
// These functions are defined in main.cpp and various handler files
extern bool checkHotWaterStates();
// Handler functions now accessed via g_state.handlers
extern void resetStandbyTimer(int state);

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
    return g_state.handlers.brewHandler ? g_state.handlers.brewHandler->isBrewActive() : false;
}

bool MachineStateContext::isManualFlushActive() const {
    // Manual flush state is checked via machine state
    return isManualFlushState(g_state.machine.machineState) && g_state.machine.machineState != MachineStateId::MANUAL_FLUSH_IDLE;
}

bool MachineStateContext::isSteamActive() const {
    return g_state.machine.steamON;
}

bool MachineStateContext::isHotWaterActive() const {
    // Simplified implementation - check if machine is in hot water state
    return (g_state.machine.machineState == MachineStateId::HOT_WATER_RUNNING);
}

bool MachineStateContext::isBackflushActive() const {
    return g_state.machine.backflushOn;
}

// === System State Access ===

bool MachineStateContext::isPidEnabled() const {
    return Config::getInstance().pidEnabled.get();
}

bool MachineStateContext::isEmergencyStop() const {
    return g_state.machine.emergencyStop;
}

bool MachineStateContext::shouldEnterStandby() const {
    return Config::getInstance().standbyEnabled.get() && g_state.standby.standbyModeRemainingTimeMillis == 0;
}

unsigned long MachineStateContext::getStandbyRemainingTime() const {
    return g_state.standby.standbyModeRemainingTimeMillis;
}

// === Timing Functions ===

unsigned long MachineStateContext::getCurrentTime() const {
    return millis();
}

void MachineStateContext::resetStandbyTimer(int stateId) const {
    // Convert state ID to MachineState enum and call existing function
    resetStandbyTimer(stateId);
}

// === Control Functions ===

void MachineStateContext::setSteamMode(bool enabled) const {
    ::setSteamMode(enabled);
}

void MachineStateContext::setPidRuntimeState(bool enabled) const {
    setRuntimePidState(enabled);
}

void MachineStateContext::performSafeShutdown() const {
    g_state.coordination.processController->performSafeShutdown();
}

// === Display Functions ===

U8G2* MachineStateContext::getDisplay() const {
    return displayManager_ ? displayManager_->getDisplay() : nullptr;
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

// === Additional Control Functions ===

void MachineStateContext::setManualFlushState(bool active) const {
    // Set global state for manual flush
    // This would typically control hardware directly
    // For now, we'll use the global state
    if (active) {
        LOG(DEBUG, "Manual flush activated");
    }
    else {
        LOG(DEBUG, "Manual flush deactivated");
    }
}

void MachineStateContext::setHotWaterState(bool active) const {
    // Set global state for hot water
    if (active) {
        LOG(DEBUG, "Hot water mode activated");
    }
    else {
        LOG(DEBUG, "Hot water mode deactivated");
    }
}

void MachineStateContext::setSteamState(bool active) const {
    // Set global state for steam
    g_state.machine.steamON = active;
    if (active) {
        LOG(DEBUG, "Steam mode activated");
    }
    else {
        LOG(DEBUG, "Steam mode deactivated");
    }
}

void MachineStateContext::setBackflushState(bool active) const {
    // Set global state for backflush
    g_state.machine.backflushOn = active;
    if (active) {
        LOG(DEBUG, "Backflush mode activated");
    }
    else {
        LOG(DEBUG, "Backflush mode deactivated");
    }
}

void MachineStateContext::disableWaterOperations() const {
    // Disable operations that require water for safety
    LOG(INFO, "Water operations disabled due to empty tank");
}

void MachineStateContext::enableWaterOperations() const {
    // Re-enable water operations when tank is refilled
    LOG(INFO, "Water operations enabled - tank refilled");
}

void MachineStateContext::enterSafeMode() const {
    // Enter safe mode - disable critical operations
    LOG(WARNING, "Entering safe mode due to system error");
    // Disable heater, pumps, etc. for safety
}

void MachineStateContext::exitSafeMode() const {
    // Exit safe mode - re-enable normal operations
    LOG(INFO, "Exiting safe mode - system error resolved");
}

void MachineStateContext::enterStandbyMode() const {
    // Enter power-saving standby mode
    LOG(INFO, "Entering standby mode - reducing power consumption");
    if (U8G2* display = getDisplay()) {
        display->setPowerSave(1); // Enable display power saving
    }
}

void MachineStateContext::exitStandbyMode() const {
    // Exit standby mode - resume normal power consumption
    LOG(INFO, "Exiting standby mode - resuming normal operation");
    if (U8G2* display = getDisplay()) {
        display->setPowerSave(0); // Disable display power saving
    }
}

bool MachineStateContext::hasUserActivity() const {
    // Check for user activity (button presses, web interface, etc.)
    // This is a simplified implementation - in reality would check various inputs
    return false; // TODO: Implement proper user activity detection
}

bool MachineStateContext::shouldExitStandby() const {
    // Check if conditions exist to exit standby mode
    // This is a simplified implementation
    return hasUserActivity(); // For now, just check user activity
}
