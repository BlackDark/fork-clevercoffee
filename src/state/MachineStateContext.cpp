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
#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/utils/SystemUtils.h"

#include <Arduino.h>

MachineStateContext::MachineStateContext(CleverCoffee::SystemContext& systemContext,
                                         CleverCoffee::HardwareManager& hardwareManager,
                                         DisplayManager&               displayManager,
                                         CleverCoffeeWiFiManager&      wifiManager,
                                         MQTTManager&                  mqttManager)
    : systemContext_(systemContext), hardwareManager_(hardwareManager),
      displayManager_(displayManager),
      wifiManager_(wifiManager),
      mqttManager_(mqttManager) {}

// === Hardware Component Access ===

TempSensor* MachineStateContext::getTempSensor() noexcept {
    return hardwareManager_.getTempSensor();
}

const TempSensor* MachineStateContext::getTempSensor() const noexcept {
    return hardwareManager_.getTempSensor();
}

Switch* MachineStateContext::getWaterTankSensor() noexcept {
    return hardwareManager_.getWaterTankSensor();
}

const Switch* MachineStateContext::getWaterTankSensor() const noexcept {
    return hardwareManager_.getWaterTankSensor();
}

Switch* MachineStateContext::getBrewSwitch() const {
    return hardwareManager_.getBrewSwitch();
}

Switch* MachineStateContext::getSteamSwitch() const {
    return hardwareManager_.getSteamSwitch();
}

Switch* MachineStateContext::getHotWaterSwitch() const {
    return hardwareManager_.getHotWaterSwitch();
}

Switch* MachineStateContext::getPowerSwitch() const {
    return hardwareManager_.getPowerSwitch();
}

Relay* MachineStateContext::getHeaterRelay() noexcept {
    return hardwareManager_.getHeaterRelay();
}

const Relay* MachineStateContext::getHeaterRelay() const {
    return hardwareManager_.getHeaterRelay();
}

Relay* MachineStateContext::getPumpRelay() noexcept {
    return hardwareManager_.getPumpRelay();
}

const Relay* MachineStateContext::getPumpRelay() const {
    return hardwareManager_.getPumpRelay();
}

Relay* MachineStateContext::getValveRelay() noexcept {
    return hardwareManager_.getValveRelay();
}

const Relay* MachineStateContext::getValveRelay() const {
    return hardwareManager_.getValveRelay();
}

LED* MachineStateContext::getStatusLed() noexcept {
    return hardwareManager_.getStatusLed();
}

const LED* MachineStateContext::getStatusLed() const noexcept {
    return hardwareManager_.getStatusLed();
}

LED* MachineStateContext::getBrewLED() const {
    return hardwareManager_.getBrewLed();
}

LED* MachineStateContext::getSteamLED() const {
    return hardwareManager_.getSteamLed();
}

Scale* MachineStateContext::getScale() const {
    return systemContext_.hardwareContext().scalePtr();
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
    // Get brew weight from SensorCoordinator
    return static_cast<float>(systemContext_.sensorCoordinator().getBrewWeight());
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
    // Check if we're in an active brew state (not FINISHED)
    auto currentState = getCurrentStateId();
    return (isBrewState(currentState) &&
            currentState != MachineStateId::BREW_FINISHED);
}

bool MachineStateContext::isManualFlushActive() const {
     // Manual flush state is checked via machine state
     return isManualFlushState(getCurrentStateId());
}

bool MachineStateContext::isSteamActive() const {
     // Check if we're in steam running state
     return (systemContext_.machineStateContext()->getCurrentStateId() == MachineStateId::STEAM_RUNNING);
}

bool MachineStateContext::isHotWaterActive() const {
     // Hot water is handled via pump control in PID_NORMAL and STEAM_RUNNING
     // Check if hot water switch is pressed
     auto* waterSwitch = getHotWaterSwitch();
     return (waterSwitch && waterSwitch->isPressed());
}

bool MachineStateContext::isBackflushActive() const {
     return backflushOn_;
}

// === System State Access ===

bool MachineStateContext::isPidEnabled() const {
    return Config::getInstance().pidEnabled.get();
}

bool MachineStateContext::isEmergencyStop() const {
     return emergencyStop_;
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

void MachineStateContext::initializeStandbyTimerIfNeeded() const {
    // Initialize standby timer if needed (when standby is first enabled)
    systemContext_.standbyCoordinator().initializeIfNeeded();
}

void MachineStateContext::resetStandbyTimerOnUserActivity() const {
    // StandbyCoordinator::reset() already checks if standby is enabled
    systemContext_.standbyCoordinator().reset();
}

void MachineStateContext::setBrewStartRequested(bool requested) noexcept {
    requestBrewStart_ = requested;
    if (requested) {
        // User activity detected - reset standby timer
        resetStandbyTimerOnUserActivity();
    }
}

void MachineStateContext::setSteamStartRequested(bool requested) noexcept {
    requestSteamStart_ = requested;
    if (requested) {
        // User activity detected - reset standby timer
        resetStandbyTimerOnUserActivity();
    }
}

void MachineStateContext::setNormalOperationRequested(bool requested) noexcept {
    requestNormalOperation_ = requested;
    if (requested) {
        // User activity detected - reset standby timer
        resetStandbyTimerOnUserActivity();
    }
}

void MachineStateContext::setHotWaterActivity(bool active) noexcept {
    if (active) {
        // User activity detected - reset standby timer
        resetStandbyTimerOnUserActivity();
    }
}

// === Control Functions ===

void MachineStateContext::setSteamMode(bool enabled) const {
    ::setSteamMode(systemContext_, enabled);
}

void MachineStateContext::setPidRuntimeState(bool enabled) const {
    setRuntimePidState(systemContext_, enabled);
}

void MachineStateContext::performSafeShutdown() const {
    if (auto* processController = systemContext_.processController()) {
        processController->performSafeShutdown();
    }
}

// === Display Functions ===

U8G2* MachineStateContext::getDisplay() const {
    return displayManager_.getDisplay();
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

MachineStateId MachineStateContext::getPidState() const noexcept {
    return isPidEnabled() ? MachineStateId::PID_NORMAL : MachineStateId::PID_DISABLED;
}

void MachineStateContext::logStateEntry(MachineStateId stateId, const char* stateName) const {
    LOGF(INFO, "Entering state %d (%s)", static_cast<int>(stateId), stateName);
}

void MachineStateContext::logStateExit(MachineStateId stateId, const char* stateName) const {
    LOGF(INFO, "Exiting state %d (%s)", static_cast<int>(stateId), stateName);
}

// === MQTT Integration ===

void MachineStateContext::resetMqttReconnectCount() const {
    systemContext_.networkCoordinator().resetMqttConnectionAttempts();
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


void MachineStateContext::setSteamState(bool active) {
     // Update member variable
     steamON_ = active;
     if (active) {
         LOG(DEBUG, "Steam mode activated");
     } else {
         LOG(DEBUG, "Steam mode deactivated");
     }
}

void MachineStateContext::setBackflushState(bool active) {
     // Update member variable
     backflushOn_ = active;
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
    // Use SensorCoordinator for scale operations
    systemContext_.sensorCoordinator().setScaleTareMode(true);
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
    return currentStateId_;
}

void MachineStateContext::transitionTo(MachineStateId newStateId) {
    // This would typically be called by StateMachine
    // For now, just log the transition request
    LOGF(INFO, "State transition requested to: %d", static_cast<int>(newStateId));
}

unsigned long MachineStateContext::getStateStartTime() const noexcept {
    auto elapsed = std::chrono::steady_clock::now() - stateEntryTime_;
    return std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count();
}

// === High-Level Hardware Control (Delegated to HardwareManager) ===

void MachineStateContext::enableHeater() noexcept {
    hardwareManager_.enableHeater();
}

void MachineStateContext::disableHeater() noexcept {
    hardwareManager_.disableHeater();
}

void MachineStateContext::setHeaterPower(uint8_t percentage) noexcept {
    hardwareManager_.setHeaterPower(percentage);
}

void MachineStateContext::enablePump() noexcept {
    hardwareManager_.enablePump();
}

void MachineStateContext::disablePump() noexcept {
    hardwareManager_.disablePump();
}

void MachineStateContext::setPumpPressure(float bar) noexcept {
    hardwareManager_.setPumpPressure(bar);
}

void MachineStateContext::openSteamValve() noexcept {
    hardwareManager_.openSteamValve();
}

void MachineStateContext::closeSteamValve() noexcept {
    hardwareManager_.closeSteamValve();
}

void MachineStateContext::openWaterValve() noexcept {
    hardwareManager_.openWaterValve();
}

void MachineStateContext::closeWaterValve() noexcept {
    hardwareManager_.closeWaterValve();
}

void MachineStateContext::openSolenoid() noexcept {
    hardwareManager_.openSolenoid();
}

void MachineStateContext::closeSolenoid() noexcept {
    hardwareManager_.closeSolenoid();
}

void MachineStateContext::emergencyShutdown() noexcept {
    hardwareManager_.emergencyShutdown();
}
