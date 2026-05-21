/**
 * @file HandlerTestStubs.cpp
 * @brief Shared stubs for handler test files
 *
 * Provides all necessary stub implementations for testing handlers in native_test.
 * Each handler test_main.cpp should #include this file.
 *
 * Strategy: Include source .cpp files that compile cleanly in native_test,
 * and provide stub implementations for hardware-dependent code (GPIOPin, Relay,
 * HardwareManager, DisplayManager, MQTTManager) that cannot compile without
 * Arduino hardware functions.
 */

// === Full header includes needed BEFORE source includes ===
// These provide complete type definitions for classes that are only
// forward-declared in MachineStateContext.h or other headers
#include "clevercoffee/hardware/HardwareManager.h"
#include "clevercoffee/display/DisplayManager.h"
#include "clevercoffee/utils/Resilience.h"
#include "clevercoffee/network/MQTTManager.h"

// === Source includes that compile cleanly in native_test ===
#include "../mocks/ConfigStubs.cpp"
#include "../../src/Logger.cpp"
#include "../../src/coordinators/SensorCoordinator.cpp"
#include "../../src/coordinators/MaintenanceCoordinator.cpp"
#include "../../src/context/SystemContext.cpp"

// === GPIOPin stubs (real impl uses digitalWrite/digitalRead/analogRead/pinMode) ===
GPIOPin::GPIOPin(int pinNumber, Type pinType) : pin(pinNumber), pinType(pinType) {}
void GPIOPin::write(bool /*value*/) const noexcept {}
int GPIOPin::read() const noexcept { return 0; }
GPIOPin::Type GPIOPin::getType() const noexcept { return pinType; }
void GPIOPin::setType(Type newType) { pinType = newType; }

// === Relay stubs (real impl uses GPIOPin::write) ===
Relay::Relay(GPIOPin& gpioInstance, ::Hardware::RelayTriggerType trigger) : gpio(gpioInstance), relayTrigger(trigger) {}
void Relay::on() const noexcept {}
void Relay::off() const noexcept {}
GPIOPin& Relay::getGPIOInstance() const noexcept { return gpio; }

// === HardwareManager stubs ===
namespace CleverCoffee {
HardwareManager::HardwareManager(const Config& config)
    : config_(config),
      heaterRelayPin_(0, GPIOPin::OUT),
      pumpRelayPin_(0, GPIOPin::OUT),
      valveRelayPin_(0, GPIOPin::OUT) {}

void HardwareManager::initializeRelays() {}
void HardwareManager::initializeLEDs() {}
void HardwareManager::initializeSwitches() {}
void HardwareManager::initializeTemperatureSensor() {}
bool HardwareManager::isInitialized() const { return false; }
void HardwareManager::safeShutdown() {}
void HardwareManager::updateLEDs(MachineStateId, double, double) {}
double HardwareManager::getCurrentTemperature() const noexcept { return 25.0; }
bool HardwareManager::hasTemperatureError() const noexcept { return false; }
Relay* HardwareManager::getHeaterRelay() noexcept { return nullptr; }
Relay* HardwareManager::getPumpRelay() noexcept { return nullptr; }
Relay* HardwareManager::getValveRelay() noexcept { return nullptr; }
bool HardwareManager::isWaterTankEmpty() const noexcept { return false; }
double HardwareManager::getWeight() const noexcept { return 0.0; }
void HardwareManager::tareScale() noexcept {}
void HardwareManager::updateHardware() noexcept {}
void HardwareManager::enableHeater() noexcept {}
void HardwareManager::disableHeater() noexcept {}
void HardwareManager::setHeaterPower(uint8_t) noexcept {}
void HardwareManager::enablePump() noexcept {}
void HardwareManager::disablePump() noexcept {}
void HardwareManager::setPumpPressure(float) noexcept {}
void HardwareManager::updateValveRelay() noexcept {}
void HardwareManager::openSteamValve() noexcept {}
void HardwareManager::closeSteamValve() noexcept {}
void HardwareManager::openWaterValve() noexcept {}
void HardwareManager::closeWaterValve() noexcept {}
void HardwareManager::openSolenoid() noexcept {}
void HardwareManager::closeSolenoid() noexcept {}
void HardwareManager::emergencyShutdown() noexcept {}
void HardwareManager::updateSafetyState() noexcept {}
void HardwareManager::cleanupPartialInit() noexcept {}
Scale* HardwareManager::getScale() noexcept { return nullptr; }
const Scale* HardwareManager::getScale() const noexcept { return nullptr; }
} // namespace CleverCoffee

// === DisplayManager stubs ===
DisplayManager::DisplayManager(::Hardware::OLEDType, ::Hardware::OLEDAddress) {}
std::unique_ptr<U8G2> DisplayManager::createDisplay(::Hardware::OLEDType, ::Hardware::OLEDAddress) { return nullptr; }

// === MQTTManager stubs ===
MQTTManager* MQTTManager::instance_ = nullptr;
MQTTManager::MQTTManager() {}
MQTTManager::~MQTTManager() {}
void MQTTManager::checkConnection() {}
void MQTTManager::loop() {}
int  MQTTManager::writeSysParamsToMQTT(bool) { return 0; }
int  MQTTManager::sendHASSIODiscoveryMsg() { return 0; }

// === ProcessController stub (PowerHandler calls performSafeShutdown) ===
#include "clevercoffee/control/ProcessController.h"

ProcessController::ProcessController(const Config& config, CleverCoffee::SystemContext& ctx,
                                     CleverCoffee::IHardwareContext& hw, IDisplayManager& disp, IMQTTManager& mqtt)
    : config_(config), systemContext_(ctx), hardwareManager_(hw), displayManager_(disp), mqttManager_(mqtt) {}
void ProcessController::performSafeShutdown() {}

// === MachineStateContext constructor stub ===

MachineStateContext::MachineStateContext(CleverCoffee::SystemContext&   systemContext,
                                         CleverCoffee::HardwareManager& hardwareManager,
                                         DisplayManager&                displayManager,
                                         IWiFiManager&                  wifiManager,
                                         MQTTManager&                   mqttManager)
    : systemContext_(systemContext), hardwareManager_(hardwareManager), displayManager_(displayManager),
      wifiManager_(wifiManager), mqttManager_(mqttManager) {}

// === MachineStateContext non-inline method stubs ===

// Hardware Component Access
TempSensor*       MachineStateContext::getTempSensor() noexcept { return nullptr; }
const TempSensor* MachineStateContext::getTempSensor() const noexcept { return nullptr; }
Switch*           MachineStateContext::getWaterTankSensor() noexcept { return nullptr; }
const Switch*     MachineStateContext::getWaterTankSensor() const noexcept { return nullptr; }
Switch*           MachineStateContext::getBrewSwitch() const { return nullptr; }
Switch*           MachineStateContext::getSteamSwitch() const { return nullptr; }
Switch*           MachineStateContext::getHotWaterSwitch() const { return nullptr; }
Switch*           MachineStateContext::getPowerSwitch() const { return nullptr; }
Relay*            MachineStateContext::getHeaterRelay() noexcept { return nullptr; }
const Relay*      MachineStateContext::getHeaterRelay() const { return nullptr; }
Relay*            MachineStateContext::getPumpRelay() noexcept { return nullptr; }
const Relay*      MachineStateContext::getPumpRelay() const { return nullptr; }
Relay*            MachineStateContext::getValveRelay() noexcept { return nullptr; }
const Relay*      MachineStateContext::getValveRelay() const { return nullptr; }
LED*              MachineStateContext::getStatusLed() noexcept { return nullptr; }
const LED*        MachineStateContext::getStatusLed() const noexcept { return nullptr; }
LED*              MachineStateContext::getBrewLED() const { return nullptr; }
LED*              MachineStateContext::getSteamLED() const { return nullptr; }
Scale*            MachineStateContext::getScale() const { return nullptr; }

// Sensor Data Access
double MachineStateContext::getCurrentTemperature() const noexcept { return 25.0; }
bool   MachineStateContext::hasTemperatureError() const noexcept { return false; }
bool   MachineStateContext::isWaterTankFull() const { return true; }
float  MachineStateContext::getCurrentPressure() const { return 0.0f; }
float  MachineStateContext::getFilteredPressure() const { return 0.0f; }
float  MachineStateContext::getCurrentWeight() const noexcept { return 0.0f; }
float  MachineStateContext::getCurrentBrewWeight() const noexcept { return 0.0f; }
bool   MachineStateContext::hasScaleError() const { return false; }
bool   MachineStateContext::hasSensorError() const { return false; }

// Process Control Functions
bool MachineStateContext::isBrewActive() const {
    auto currentState = getCurrentStateId();
    return (isBrewState(currentState) && currentState != MachineStateId::BREW_FINISHED);
}
bool MachineStateContext::isManualFlushActive() const { return false; }
bool MachineStateContext::isSteamActive() const { return getCurrentStateId() == MachineStateId::STEAM_RUNNING; }
bool MachineStateContext::isHotWaterActive() const { return false; }
bool MachineStateContext::isBackflushActive() const { return false; }

// System State Access
bool          MachineStateContext::isPidEnabled() const { return Config::getInstance().pidEnabled.get(); }
bool          MachineStateContext::isEmergencyStop() const { return emergencyStop_; }
bool          MachineStateContext::shouldEnterStandby() const { return false; }
unsigned long MachineStateContext::getStandbyRemainingTime() const { return 0; }

// Timing Functions
unsigned long MachineStateContext::getCurrentTime() const { return millis(); }
void MachineStateContext::resetStandbyTimer(MachineStateId /*stateId*/) const {}
void MachineStateContext::initializeStandbyTimerIfNeeded() const {}

// Request flag setters (non-inline ones)
void MachineStateContext::setBrewStartRequested(bool requested) noexcept { requestBrewStart_ = requested; }
void MachineStateContext::setSteamStartRequested(bool requested) noexcept { requestSteamStart_ = requested; }
void MachineStateContext::setNormalOperationRequested(bool requested) noexcept { requestNormalOperation_ = requested; }
void MachineStateContext::setHotWaterActivity(bool /*active*/) noexcept {}

// Control Functions
void MachineStateContext::setSteamMode(bool /*enabled*/) const {}
void MachineStateContext::setPidRuntimeState(bool /*enabled*/) const {}
void MachineStateContext::performSafeShutdown() const {}
void MachineStateContext::setManualFlushState(bool /*active*/) const {}
void MachineStateContext::setSteamState(bool active) { steamON_ = active; }
void MachineStateContext::setBackflushEnterRequested(bool requested) noexcept {
    requestEnterBackflush_ = requested;
}

void MachineStateContext::setBackflushCycleStartRequested(bool requested) noexcept {
    requestBackflushCycleStart_ = requested;
}

void MachineStateContext::setBackflushStopRequested(bool requested) noexcept {
    requestBackflushStop_ = requested;
}

void MachineStateContext::applyBackflushMode(bool active) noexcept {
    if (active == backflushOn_) {
        return;
    }
    if (active && Config::getInstance().backflushCycles.get() <= 0) {
        return;
    }
    backflushOn_ = active;
    if (active) {
        currBackflushCycles_ = 1;
        requestEnterBackflush_ = true;
    } else {
        requestBackflushStop_ = true;
    }
}

void MachineStateContext::setBackflushState(bool active) { applyBackflushMode(active); }
void MachineStateContext::disableWaterOperations() const {}
void MachineStateContext::enableWaterOperations() const {}
void MachineStateContext::enterSafeMode() const {}
void MachineStateContext::exitSafeMode() const {}
void MachineStateContext::enterStandbyMode() const {}
void MachineStateContext::exitStandbyMode() const {}
bool MachineStateContext::hasUserActivity() const { return false; }
bool MachineStateContext::shouldExitStandby() const { return false; }

// Display Functions
U8G2* MachineStateContext::getDisplay() const { return nullptr; }
void  MachineStateContext::setDisplayPowerSave(int /*mode*/) const {}

// Logging Functions
void MachineStateContext::logStateTransition(MachineStateId /*from*/, MachineStateId /*to*/, const char* /*reason*/) const {}
MachineStateId MachineStateContext::getPidState() const noexcept {
    return isPidEnabled() ? MachineStateId::PID_NORMAL : MachineStateId::PID_DISABLED;
}
void MachineStateContext::logStateEntry(MachineStateId /*stateId*/, const char* /*stateName*/) const {}
void MachineStateContext::logStateExit(MachineStateId /*stateId*/, const char* /*stateName*/) const {}

// MQTT Integration
void MachineStateContext::resetMqttReconnectCount() const {}

// Configuration Access
unsigned long MachineStateContext::getBackflushFillTimeMs() const { return 0; }
unsigned long MachineStateContext::getBackflushFlushTimeMs() const { return 0; }

// State Timing Functions
unsigned long MachineStateContext::getStateElapsedTimeMs() const { return 0; }
bool MachineStateContext::hasStateTimeoutElapsed(unsigned long /*timeoutMs*/) const noexcept { return false; }
void MachineStateContext::updateStateEntryTime(std::chrono::steady_clock::time_point entryTime) { stateEntryTime_ = entryTime; }

// IHardwareContext Interface Implementation
bool   MachineStateContext::isWaterTankEmpty() const noexcept { return false; }
double MachineStateContext::getWeight() const noexcept { return 0.0; }
void   MachineStateContext::tareScale() noexcept {}
void   MachineStateContext::updateHardware() noexcept {}
void   MachineStateContext::enableHeater() noexcept {}
void   MachineStateContext::disableHeater() noexcept {}
void   MachineStateContext::setHeaterPower(uint8_t /*percentage*/) noexcept {}
void   MachineStateContext::enablePump() noexcept {}
void   MachineStateContext::disablePump() noexcept {}
void   MachineStateContext::setPumpPressure(float /*bar*/) noexcept {}
void   MachineStateContext::openSteamValve() noexcept {}
void   MachineStateContext::closeSteamValve() noexcept {}
void   MachineStateContext::openWaterValve() noexcept {}
void   MachineStateContext::closeWaterValve() noexcept {}
void   MachineStateContext::openSolenoid() noexcept {}
void   MachineStateContext::closeSolenoid() noexcept {}
void   MachineStateContext::emergencyShutdown() noexcept {}

// IConfigContext Interface Implementation
double        MachineStateContext::getBrewSetpoint() const noexcept { return Config::getInstance().brewSetpoint.get(); }
double        MachineStateContext::getSteamSetpoint() const noexcept { return Config::getInstance().steamSetpoint.get(); }
double        MachineStateContext::getTargetBrewTime() const noexcept { return Config::getInstance().brewByTimeTargetTime.get(); }
double        MachineStateContext::getPreInfusionTime() const noexcept { return Config::getInstance().brewPreInfusionTime.get(); }
double        MachineStateContext::getPidKp() const noexcept { return Config::getInstance().pidRegularKp.get(); }
double        MachineStateContext::getPidTn() const noexcept { return Config::getInstance().pidRegularTn.get(); }
double        MachineStateContext::getPidTv() const noexcept { return Config::getInstance().pidRegularTv.get(); }
Config&       MachineStateContext::getConfig() noexcept { return Config::getInstance(); }
const Config& MachineStateContext::getConfig() const noexcept { return Config::getInstance(); }

// IStateManager Interface Implementation
MachineStateId MachineStateContext::getCurrentStateId() const noexcept { return currentStateId_; }
void           MachineStateContext::transitionTo(MachineStateId /*newStateId*/) {}
unsigned long  MachineStateContext::getStateStartTime() const noexcept { return 0; }

// Private helper
void MachineStateContext::resetStandbyTimerOnUserActivity() const {}
