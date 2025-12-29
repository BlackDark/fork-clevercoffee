/**
 * @file MachineStateContext.cpp
 * @brief Implementation of MachineStateContext for state machine access to machine resources
 */

#include "clevercoffee/hardware/HardwareManager.h"  // Include before own header to resolve forward declaration
#include "clevercoffee/state/MachineStateContext.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/display/DisplayManager.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/network/CleverCoffeeWiFiManager.h"
#include "clevercoffee/network/MQTTManager.h"
// #include "../hotWaterHandler.h" - removed to avoid circular dependencies
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/standby.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/utils/SystemUtils.h"

#include <Arduino.h>

MachineStateContext::MachineStateContext(CleverCoffee::SystemContext& systemContext,
                                         DisplayManager*               displayManager,
                                         CleverCoffee::HardwareManager*              hardwareManager,
                                         CleverCoffeeWiFiManager*      wifiManager,
                                         MQTTManager*                  mqttManager)
    : systemContext_(systemContext), displayManager_(displayManager), hardwareManager_(hardwareManager),
      wifiManager_(wifiManager), mqttManager_(mqttManager) {}

// === Hardware Component Access ===

TempSensor* MachineStateContext::getTempSensor() noexcept {
    return hardwareManager_ ? hardwareManager_->getTempSensor() : nullptr;
}

const TempSensor* MachineStateContext::getTempSensor() const noexcept {
    return hardwareManager_ ? hardwareManager_->getTempSensor() : nullptr;
}

TempSensor* MachineStateContext::getTemperatureSensor() const {
    return hardwareManager_ ? hardwareManager_->getTempSensor() : nullptr;
}

Switch* MachineStateContext::getWaterTankSensor() noexcept {
    return hardwareManager_ ? hardwareManager_->getWaterTankSensor() : nullptr;
}

const Switch* MachineStateContext::getWaterTankSensor() const noexcept {
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

Relay* MachineStateContext::getHeaterRelay() noexcept {
    return hardwareManager_ ? hardwareManager_->getHeaterRelay() : nullptr;
}

const Relay* MachineStateContext::getHeaterRelay() const {
    return hardwareManager_ ? hardwareManager_->getHeaterRelay() : nullptr;
}

Relay* MachineStateContext::getPumpRelay() noexcept {
    return hardwareManager_ ? hardwareManager_->getPumpRelay() : nullptr;
}

const Relay* MachineStateContext::getPumpRelay() const {
    return hardwareManager_ ? hardwareManager_->getPumpRelay() : nullptr;
}

Relay* MachineStateContext::getValveRelay() noexcept {
    return hardwareManager_ ? hardwareManager_->getValveRelay() : nullptr;
}

const Relay* MachineStateContext::getValveRelay() const {
    return hardwareManager_ ? hardwareManager_->getValveRelay() : nullptr;
}

LED* MachineStateContext::getStatusLed() noexcept {
    return hardwareManager_ ? hardwareManager_->getStatusLed() : nullptr;
}

const LED* MachineStateContext::getStatusLed() const noexcept {
    return hardwareManager_ ? hardwareManager_->getStatusLed() : nullptr;
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
    return g_state.hardware.scale.get();
}

// === Sensor Data Access ===

double MachineStateContext::getCurrentTemperature() const noexcept {
    return systemContext_.sensorCoordinator().getTemperature();
}

bool MachineStateContext::hasTemperatureError() const noexcept {
    return systemContext_.sensorCoordinator().hasTemperatureSensorError();
}

bool MachineStateContext::isWaterTankFull() const {
    return systemContext_.sensorCoordinator().isWaterTankFull();
}

float MachineStateContext::getCurrentPressure() const {
    return systemContext_.sensorCoordinator().getPressure();
}

float MachineStateContext::getFilteredPressure() const {
    return systemContext_.sensorCoordinator().getFilteredPressure();
}

float MachineStateContext::getCurrentWeight() const noexcept {
    return systemContext_.sensorCoordinator().getWeight();
}

float MachineStateContext::getCurrentBrewWeight() const noexcept {
    // Scale brew weight tracking comes from global state (scale needs ISensor implementation)
    return g_state.sensors.currBrewWeight;
}

bool MachineStateContext::hasScaleError() const {
    return systemContext_.sensorCoordinator().hasScaleSensorError();
}

bool MachineStateContext::hasSensorError() const {
    return systemContext_.sensorCoordinator().hasSensorError();
}

// === Process Control Functions ===

// TODO those are wrong the functions behind like brew() and manualFlush() are triggering those events

bool MachineStateContext::isBrewActive() const {
    auto* brewHandler = systemContext_.brewHandler();
    return brewHandler ? brewHandler->isBrewActive() : false;
}

bool MachineStateContext::isManualFlushActive() const {
    // Manual flush state is checked via machine state
    return isManualFlushState(g_state.machine.machineState) &&
           g_state.machine.machineState != MachineStateId::MANUAL_FLUSH_IDLE;
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
    return systemContext_.standbyCoordinator().shouldEnterStandby();
}

unsigned long MachineStateContext::getStandbyRemainingTime() const {
    return systemContext_.standbyCoordinator().getRemainingTimeMillis();
}

// === Timing Functions ===

unsigned long MachineStateContext::getCurrentTime() const {
    return millis();
}

void MachineStateContext::resetStandbyTimer(MachineStateId stateId) const {
    // Reset standby timer through coordinator
    systemContext_.standbyCoordinator().reset();
}

// === Control Functions ===

void MachineStateContext::setSteamMode(bool enabled) const {
    ::setSteamMode(enabled);
}

void MachineStateContext::setPidRuntimeState(bool enabled) const {
    setRuntimePidState(enabled);
}

void MachineStateContext::performSafeShutdown() const {
    if (auto* processController = systemContext_.processController()) {
        processController->performSafeShutdown();
    }
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

void MachineStateContext::logStateTransition(MachineStateId fromState,
                                             MachineStateId toState,
                                             const char*    reason) const {
    if (reason) {
        LOGF(INFO, "State transition: %d -> %d (%s)", static_cast<int>(fromState), static_cast<int>(toState), reason);
    } else {
        LOGF(INFO, "State transition: %d -> %d", static_cast<int>(fromState), static_cast<int>(toState));
    }
}

void MachineStateContext::logStateEntry(MachineStateId stateId, const char* stateName) const {
    LOGF(INFO, "Entering state %d (%s)", static_cast<int>(stateId), stateName);
}

void MachineStateContext::logStateExit(MachineStateId stateId, const char* stateName) const {
    LOGF(INFO, "Exiting state %d (%s)", static_cast<int>(stateId), stateName);
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
    } else {
        LOG(DEBUG, "Manual flush deactivated");
    }
}

void MachineStateContext::setHotWaterState(bool active) const {
    // Set global state for hot water
    if (active) {
        LOG(DEBUG, "Hot water mode activated");
    } else {
        LOG(DEBUG, "Hot water mode deactivated");
    }
}

void MachineStateContext::setSteamState(bool active) const {
    // Set global state for steam
    g_state.machine.steamON = active;
    if (active) {
        LOG(DEBUG, "Steam mode activated");
    } else {
        LOG(DEBUG, "Steam mode deactivated");
    }
}

void MachineStateContext::setBackflushState(bool active) const {
    // Set global state for backflush
    g_state.machine.backflushOn = active;
    if (active) {
        LOG(DEBUG, "Backflush mode activated");
    } else {
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

// === Configuration Access ===

unsigned long MachineStateContext::getBackflushFillTimeMs() const {
    return static_cast<unsigned long>(Config::getInstance().backflushFillTime.get() * 1000);
}

unsigned long MachineStateContext::getBackflushFlushTimeMs() const {
    return static_cast<unsigned long>(Config::getInstance().backflushFlushTime.get() * 1000);
}

// === State Timing Functions ===

unsigned long MachineStateContext::getStateElapsedTimeMs() const {
    auto elapsed = std::chrono::steady_clock::now() - stateEntryTime_;
    return std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count();
}

bool MachineStateContext::hasStateTimeoutElapsed(unsigned long timeoutMs) const noexcept {
    return getStateElapsedTimeMs() >= timeoutMs;
}

void MachineStateContext::updateStateEntryTime(std::chrono::steady_clock::time_point entryTime) {
    stateEntryTime_ = entryTime;
}

// === IHardwareContext Interface Implementation ===

bool MachineStateContext::isWaterTankEmpty() const noexcept {
    return !isWaterTankFull();
}

double MachineStateContext::getWeight() const noexcept {
    return static_cast<double>(getCurrentWeight());
}

void MachineStateContext::tareScale() noexcept {
    if (Scale* scale = getScale()) {
        scale->tare();
    }
}

void MachineStateContext::updateHardware() noexcept {
    // SensorCoordinator auto-updates in main loop, no manual call needed
    // Access the coordinator to validate it exists, but no update call is needed
    (void)(systemContext_.sensorCoordinator());
}

// === IConfigContext Interface Implementation ===

double MachineStateContext::getBrewSetpoint() const noexcept {
    return Config::getInstance().brewSetpoint.get();
}

double MachineStateContext::getSteamSetpoint() const noexcept {
    return Config::getInstance().steamSetpoint.get();
}

double MachineStateContext::getTargetBrewTime() const noexcept {
    return Config::getInstance().brewByTimeTargetTime.get();
}

double MachineStateContext::getPreInfusionTime() const noexcept {
    return Config::getInstance().brewPreInfusionTime.get();
}

double MachineStateContext::getPidKp() const noexcept {
    return Config::getInstance().pidRegularKp.get();
}

double MachineStateContext::getPidTn() const noexcept {
    return Config::getInstance().pidRegularTn.get();
}

double MachineStateContext::getPidTv() const noexcept {
    return Config::getInstance().pidRegularTv.get();
}

Config& MachineStateContext::getConfig() noexcept {
    return Config::getInstance();
}

const Config& MachineStateContext::getConfig() const noexcept {
    return Config::getInstance();
}

// === IStateManager Interface Implementation ===

MachineStateId MachineStateContext::getCurrentStateId() const noexcept {
    return g_state.machine.machineState;
}

void MachineStateContext::transitionTo(MachineState& newState) {
    // This would typically be called by StateMachine
    // For now, just log the transition request
    LOGF(INFO, "State transition requested to: %s", newState.getStateName());
}

unsigned long MachineStateContext::getStateStartTime() const noexcept {
    auto elapsed = std::chrono::steady_clock::now() - stateEntryTime_;
    return std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count();
}

// === High-Level Hardware Control (Delegated to HardwareManager) ===

void MachineStateContext::enableHeater() noexcept {
    if (hardwareManager_) {
        hardwareManager_->enableHeater();
    }
}

void MachineStateContext::disableHeater() noexcept {
    if (hardwareManager_) {
        hardwareManager_->disableHeater();
    }
}

void MachineStateContext::setHeaterPower(uint8_t percentage) noexcept {
    if (hardwareManager_) {
        hardwareManager_->setHeaterPower(percentage);
    }
}

void MachineStateContext::enablePump() noexcept {
    if (hardwareManager_) {
        hardwareManager_->enablePump();
    }
}

void MachineStateContext::disablePump() noexcept {
    if (hardwareManager_) {
        hardwareManager_->disablePump();
    }
}

void MachineStateContext::setPumpPressure(float bar) noexcept {
    if (hardwareManager_) {
        hardwareManager_->setPumpPressure(bar);
    }
}

void MachineStateContext::openSteamValve() noexcept {
    if (hardwareManager_) {
        hardwareManager_->openSteamValve();
    }
}

void MachineStateContext::closeSteamValve() noexcept {
    if (hardwareManager_) {
        hardwareManager_->closeSteamValve();
    }
}

void MachineStateContext::openWaterValve() noexcept {
    if (hardwareManager_) {
        hardwareManager_->openWaterValve();
    }
}

void MachineStateContext::closeWaterValve() noexcept {
    if (hardwareManager_) {
        hardwareManager_->closeWaterValve();
    }
}

void MachineStateContext::openSolenoid() noexcept {
    if (hardwareManager_) {
        hardwareManager_->openSolenoid();
    }
}

void MachineStateContext::closeSolenoid() noexcept {
    if (hardwareManager_) {
        hardwareManager_->closeSolenoid();
    }
}

void MachineStateContext::emergencyShutdown() noexcept {
    if (hardwareManager_) {
        hardwareManager_->emergencyShutdown();
    }
}
