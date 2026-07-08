/**
 * @file HardwareManager.cpp
 * @brief Implementation of RAII wrapper for hardware component management
 */

#include "clevercoffee/hardware/HardwareManager.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/constants/Temperature.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/hardware/pinmapping.h"
#include "clevercoffee/utils/memoryUtils.h"

#include <cmath>

using namespace CleverCoffee;

// Note: Hardware namespace from defaults.h is in global namespace, not CleverCoffee::Hardware
// We need to use ::Hardware to refer to the global namespace Hardware
using ::Hardware::RelayTriggerType;
using ::Hardware::SwitchMode;
using ::Hardware::SwitchType;
using ::Hardware::TemperatureSensorType;

HardwareManager::HardwareManager(const Config& config)
    : config_(config), heaterRelayPin_(PIN_HEATER, GPIOPin::OUT), pumpRelayPin_(PIN_PUMP, GPIOPin::OUT),
      valveRelayPin_(PIN_VALVE, GPIOPin::OUT) {
    LOG(INFO, "Initializing hardware components...");
    logMemory("HardwareManager Constructor Start");

    try {
        LOG(INFO, "Starting relay initialization...");
        logMemoryBasic("Before Relay Init");
        initializeRelays();
        relaysInitialized_ = true;
        logMemoryBasic("After Relay Init");
        LOG(INFO, "Relays initialized, starting LED initialization...");

        logMemoryBasic("Before LED Init");
        initializeLEDs();
        ledsInitialized_ = true;
        logMemoryBasic("After LED Init");
        LOG(INFO, "LEDs initialized, starting switch initialization...");

        logMemoryBasic("Before Switch Init");
        initializeSwitches();
        switchesInitialized_ = true;
        logMemoryBasic("After Switch Init");
        LOG(INFO, "Switches initialized, starting temperature sensor initialization...");

        logMemoryBasic("Before TempSensor Init");
        initializeTemperatureSensor();
        tempSensorInitialized_ = true;
        logMemoryBasic("After TempSensor Init");
        LOG(INFO, "Temperature sensor initialized successfully");

        logMemory("HardwareManager Constructor Complete");
        LOG(INFO, "Hardware initialization completed successfully");
    } catch (...) {
        // Cleanup partial initialization on ANY exception
        // This ensures hardware is safe even if init fails
        logMemory("HardwareManager Constructor FAILED - Cleaning up");
        LOG(ERROR, "Hardware initialization failed - performing cleanup");
        cleanupPartialInit();
        throw;
    }
}

void HardwareManager::initializeRelays() {
    LOG(INFO, "Initializing heater relay...");
    yield(); // Prevent watchdog timeout
    const auto heaterTriggerType = static_cast<RelayTriggerType>(config_.hardwareRelaysHeaterTriggerType.get());
    heaterRelay_                 = std::make_unique<Relay>(heaterRelayPin_, heaterTriggerType);
    heaterRelay_->off();
    LOG(INFO, "Heater relay initialized");

    LOG(INFO, "Initializing valve relay...");
    yield(); // Prevent watchdog timeout
    const auto valveTriggerType = static_cast<RelayTriggerType>(config_.hardwareRelaysValveTriggerType.get());
    valveRelay_                 = std::make_unique<Relay>(valveRelayPin_, valveTriggerType);
    valveRelay_->off();
    LOG(INFO, "Valve relay initialized");

    LOG(INFO, "Initializing pump relay...");
    yield(); // Prevent watchdog timeout
    const auto pumpTriggerType = static_cast<RelayTriggerType>(config_.hardwareRelaysPumpTriggerType.get());
    pumpRelay_                 = std::make_unique<Relay>(pumpRelayPin_, pumpTriggerType);
    pumpRelay_->off();
    LOG(INFO, "Pump relay initialized");

    LOG(INFO, "Relays initialized successfully");
}

void HardwareManager::initializeLEDs() {
    // Initialize status LED
    if (config_.hardwareLedsStatusEnabled.get()) {
        LOG(INFO, "Initializing status LED...");
        yield(); // Prevent watchdog timeout
        const bool inverted = config_.hardwareLedsStatusInverted.get();
        statusLedPin_       = std::make_unique<GPIOPin>(PIN_STATUSLED, GPIOPin::OUT);
        statusLed_          = std::make_unique<StandardLED>(*statusLedPin_, inverted);
        statusLed_->turnOff();
        LOG(INFO, "Status LED initialized");
    }

    // Initialize brew LED
    if (config_.hardwareLedsBrewEnabled.get()) {
        LOG(INFO, "Initializing brew LED...");
        yield(); // Prevent watchdog timeout
        const bool inverted = config_.hardwareLedsBrewInverted.get();
        brewLedPin_         = std::make_unique<GPIOPin>(PIN_BREWLED, GPIOPin::OUT);
        brewLed_            = std::make_unique<StandardLED>(*brewLedPin_, inverted);
        brewLed_->turnOff();
        LOG(INFO, "Brew LED initialized");
    }

    // Initialize steam LED
    if (config_.hardwareLedsSteamEnabled.get()) {
        LOG(INFO, "Initializing steam LED...");
        yield(); // Prevent watchdog timeout
        const bool inverted = config_.hardwareLedsSteamInverted.get();
        steamLedPin_        = std::make_unique<GPIOPin>(PIN_STEAMLED, GPIOPin::OUT);
        steamLed_           = std::make_unique<StandardLED>(*steamLedPin_, inverted);
        steamLed_->turnOff();
        LOG(INFO, "Steam LED initialized");
    }

    LOG(INFO, "LEDs initialized successfully");
}

void HardwareManager::initializeSwitches() {
    // Initialize power switch
    if (config_.hardwareSwitchesPowerEnabled.get()) {
        LOG(INFO, "Initializing power switch...");
        yield(); // Prevent watchdog timeout
        const auto type         = config_.hardwareSwitchesPowerType.get();
        const auto mode         = config_.hardwareSwitchesPowerMode.get();
        const auto initialState = (mode == SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        powerSwitch_ = std::make_unique<IOSwitch>(PIN_POWERSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Power switch initialized");
    }

    // Initialize steam switch
    if (config_.hardwareSwitchesSteamEnabled.get()) {
        LOG(INFO, "Initializing steam switch...");
        yield(); // Prevent watchdog timeout
        const auto type         = config_.hardwareSwitchesSteamType.get();
        const auto mode         = config_.hardwareSwitchesSteamMode.get();
        const auto initialState = (mode == SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        steamSwitch_ = std::make_unique<IOSwitch>(PIN_STEAMSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Steam switch initialized");
    }

    // Initialize brew switch
    if (config_.hardwareSwitchesBrewEnabled.get()) {
        LOG(INFO, "Initializing brew switch...");
        yield(); // Prevent watchdog timeout
        const auto type         = config_.hardwareSwitchesBrewType.get();
        const auto mode         = config_.hardwareSwitchesBrewMode.get();
        const auto initialState = (mode == SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        brewSwitch_ = std::make_unique<IOSwitch>(PIN_BREWSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Brew switch initialized");
    }

    // Initialize hot water switch
    if (config_.hardwareSwitchesHotWaterEnabled.get()) {
        LOG(INFO, "Initializing hot water switch...");
        yield(); // Prevent watchdog timeout
        const auto type         = config_.hardwareSwitchesHotWaterType.get();
        const auto mode         = config_.hardwareSwitchesHotWaterMode.get();
        const auto initialState = (mode == SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        hotWaterSwitch_ = std::make_unique<IOSwitch>(PIN_WATERSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Hot water switch initialized");
    }

    LOG(INFO, "Switches initialized successfully");
}

void HardwareManager::initializeTemperatureSensor() {
    LOG(INFO, "Getting temperature sensor type from config...");
    yield(); // Prevent watchdog timeout
    const TemperatureSensorType tempSensorType = config_.hardwareSensorsTemperatureType.get();
    LOGF(INFO, "Temperature sensor type: %d", static_cast<int>(tempSensorType));

    if (tempSensorType == TemperatureSensorType::TSIC_306) {
        LOG(INFO, "Initializing TSIC 306 temperature sensor...");
        yield(); // Prevent watchdog timeout
        tempSensor_ = std::make_unique<TempSensorTSIC>(PIN_TEMPSENSOR);
        LOG(INFO, "TSIC 306 temperature sensor created");
    } else if (tempSensorType == TemperatureSensorType::DALLAS_DS18B20) {
        LOG(INFO, "Initializing Dallas DS18B20 temperature sensor...");
        yield(); // Prevent watchdog timeout
        tempSensor_ = std::make_unique<TempSensorDallas>(PIN_TEMPSENSOR);
        LOG(INFO, "Dallas DS18B20 temperature sensor created");
    }

    if (tempSensor_) {
        LOG(INFO, "Temperature sensor initialized successfully");
    } else {
        LOGF(ERROR, "Unknown temperature sensor type: %d", static_cast<int>(tempSensorType));
    }
}

bool HardwareManager::isInitialized() const {
    // Check critical components
    return heaterRelay_ && pumpRelay_ && valveRelay_;
}

void HardwareManager::safeShutdown() {
    LOG(INFO, "Performing safe hardware shutdown...");

    // Turn off all relays (only if they're currently on)
    if (heaterRelay_ && heaterEnabled_) {
        heaterRelay_->off();
        heaterEnabled_ = false;
    }
    if (pumpRelay_ && pumpEnabled_) {
        pumpRelay_->off();
        pumpEnabled_ = false;
    }
    if (valveRelay_ && valveState_ != CleverCoffee::Hardware::ValveState::CLOSED) {
        valveRelay_->off();
        valveState_ = CleverCoffee::Hardware::ValveState::CLOSED;
    }
    solenoidOpen_ = false;

    // Turn off all LEDs
    if (statusLed_) statusLed_->turnOff();
    if (brewLed_) brewLed_->turnOff();
    if (steamLed_) steamLed_->turnOff();

    LOG(INFO, "Safe hardware shutdown completed");
}

// === IHardwareContext Implementation ===

double HardwareManager::getCurrentTemperature() const noexcept {
    if (tempSensor_) {
        return tempSensor_->getCurrentTemperature();
    }
    return 0.0;
}

bool HardwareManager::hasTemperatureError() const noexcept {
    if (tempSensor_) {
        return tempSensor_->hasError();
    }
    return true; // No sensor = error state
}

Relay* HardwareManager::getHeaterRelay() noexcept {
    return heaterRelay_.get();
}

Relay* HardwareManager::getPumpRelay() noexcept {
    return pumpRelay_.get();
}

Relay* HardwareManager::getValveRelay() noexcept {
    return valveRelay_.get();
}

bool HardwareManager::isWaterTankEmpty() const noexcept {
    return waterTankEmpty_;
}

double HardwareManager::getWeight() const noexcept {
    // TODO: Integrate with scale when available
    return 0.0;
}

void HardwareManager::tareScale() noexcept {
    // TODO: Integrate with scale when available
}

void HardwareManager::enableHeater() noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot enable heater - emergency mode active");
        return;
    }
    if (heaterEnabled_) {
        // Already enabled, no action needed
        return;
    }
    LOG(INFO, "Enabling heater");
    if (heaterRelay_) {
        heaterRelay_->on();
        heaterEnabled_ = true;
    }
}

void HardwareManager::disableHeater() noexcept {
    if (!heaterEnabled_) {
        // Already disabled, no action needed
        return;
    }
    LOG(INFO, "Disabling heater");
    if (heaterRelay_) {
        heaterRelay_->off();
        heaterEnabled_ = false;
    }
}

void HardwareManager::setHeaterPower(uint8_t percentage) noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot set heater power - emergency mode active");
        return;
    }
    LOGF(INFO, "Setting heater power to %d%%", percentage);
    // TODO: Implement PWM/SSR control for heater power
    // For now, simple on/off based on percentage
    if (percentage > 0) {
        enableHeater();
    } else {
        disableHeater();
    }
}

void HardwareManager::enablePump() noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot enable pump - emergency mode active");
        return;
    }
    if (waterTankEmpty_) {
        LOG(WARNING, "Cannot enable pump - water tank is empty");
        return;
    }
    if (pumpEnabled_) {
        // Already enabled, no action needed
        return;
    }
    LOG(INFO, "Enabling pump");
    if (pumpRelay_) {
        pumpRelay_->on();
        pumpEnabled_ = true;
    }
}

void HardwareManager::disablePump() noexcept {
    if (!pumpEnabled_) {
        // Already disabled, no action needed
        return;
    }
    LOG(INFO, "Disabling pump");
    if (pumpRelay_) {
        pumpRelay_->off();
        pumpEnabled_ = false;
    }
}

void HardwareManager::setPumpPressure(float bar) noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot set pump pressure - emergency mode active");
        return;
    }
    if (waterTankEmpty_) {
        LOG(WARNING, "Cannot set pump pressure - water tank is empty");
        return;
    }
    LOGF(INFO, "Setting pump pressure to %.1f bar", bar);
    // TODO: Implement dimmer/PWM control for pump pressure
    // For now, simple on/off based on pressure value
    if (bar > 0.0f) {
        enablePump();
    } else {
        disablePump();
    }
}

void HardwareManager::updateValveRelay() noexcept {
    if (!valveRelay_) {
        return;
    }

    // Relay should be ON if any valve is open, OFF if all valves are closed
    bool shouldBeOn = (valveState_ != CleverCoffee::Hardware::ValveState::CLOSED);

    // Update relay state (Relay class handles redundant operation prevention internally)
    if (shouldBeOn) {
        valveRelay_->on();
        const char* steamState = (valveState_ == CleverCoffee::Hardware::ValveState::STEAM_OPEN ||
                                  valveState_ == CleverCoffee::Hardware::ValveState::BOTH_OPEN)
                                     ? "open"
                                     : "closed";
        const char* waterState = (valveState_ == CleverCoffee::Hardware::ValveState::WATER_OPEN ||
                                  valveState_ == CleverCoffee::Hardware::ValveState::BOTH_OPEN)
                                     ? "open"
                                     : "closed";
        LOGF(DEBUG, "Valve relay turned ON (valve state: steam=%s, water=%s)", steamState, waterState);
    } else {
        valveRelay_->off();
        LOG(DEBUG, "Valve relay turned OFF (all valves closed)");
    }
}

void HardwareManager::openSteamValve() noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot open steam valve - emergency mode active");
        return;
    }

    // Update valve state
    switch (valveState_) {
        case CleverCoffee::Hardware::ValveState::CLOSED:
            valveState_ = CleverCoffee::Hardware::ValveState::STEAM_OPEN;
            LOG(INFO, "Opening steam valve");
            break;
        case CleverCoffee::Hardware::ValveState::WATER_OPEN:
            valveState_ = CleverCoffee::Hardware::ValveState::BOTH_OPEN;
            LOG(INFO, "Opening steam valve (water valve already open)");
            break;
        case CleverCoffee::Hardware::ValveState::STEAM_OPEN:
        case CleverCoffee::Hardware::ValveState::BOTH_OPEN:
            // Already open, no action needed
            return;
    }

    updateValveRelay();
}

void HardwareManager::closeSteamValve() noexcept {
    // Update valve state
    switch (valveState_) {
        case CleverCoffee::Hardware::ValveState::CLOSED:
        case CleverCoffee::Hardware::ValveState::WATER_OPEN:
            // Already closed, no action needed
            return;
        case CleverCoffee::Hardware::ValveState::STEAM_OPEN:
            valveState_ = CleverCoffee::Hardware::ValveState::CLOSED;
            LOG(INFO, "Closing steam valve");
            break;
        case CleverCoffee::Hardware::ValveState::BOTH_OPEN:
            valveState_ = CleverCoffee::Hardware::ValveState::WATER_OPEN;
            LOG(INFO, "Closing steam valve (water valve remains open)");
            break;
    }

    updateValveRelay();
}

void HardwareManager::openWaterValve() noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot open water valve - emergency mode active");
        return;
    }

    // Update valve state
    switch (valveState_) {
        case CleverCoffee::Hardware::ValveState::CLOSED:
            valveState_ = CleverCoffee::Hardware::ValveState::WATER_OPEN;
            LOG(INFO, "Opening water valve");
            break;
        case CleverCoffee::Hardware::ValveState::STEAM_OPEN:
            valveState_ = CleverCoffee::Hardware::ValveState::BOTH_OPEN;
            LOG(INFO, "Opening water valve (steam valve already open)");
            break;
        case CleverCoffee::Hardware::ValveState::WATER_OPEN:
        case CleverCoffee::Hardware::ValveState::BOTH_OPEN:
            // Already open, no action needed
            return;
    }

    updateValveRelay();
}

void HardwareManager::closeWaterValve() noexcept {
    // Update valve state
    switch (valveState_) {
        case CleverCoffee::Hardware::ValveState::CLOSED:
        case CleverCoffee::Hardware::ValveState::STEAM_OPEN:
            // Already closed, no action needed
            return;
        case CleverCoffee::Hardware::ValveState::WATER_OPEN:
            valveState_ = CleverCoffee::Hardware::ValveState::CLOSED;
            LOG(INFO, "Closing water valve");
            break;
        case CleverCoffee::Hardware::ValveState::BOTH_OPEN:
            valveState_ = CleverCoffee::Hardware::ValveState::STEAM_OPEN;
            LOG(INFO, "Closing water valve (steam valve remains open)");
            break;
    }

    updateValveRelay();
}

void HardwareManager::openSolenoid() noexcept {
    if (emergencyMode_) {
        LOG(WARNING, "Cannot open solenoid - emergency mode active");
        return;
    }
    if (solenoidOpen_) {
        // Already open, no action needed
        return;
    }
    LOG(INFO, "Opening solenoid");
    // TODO: Wire up solenoid control if available
    // Solenoid might be controlled via valve relay or separate GPIO
    solenoidOpen_ = true;
}

void HardwareManager::closeSolenoid() noexcept {
    if (!solenoidOpen_) {
        // Already closed, no action needed
        return;
    }
    LOG(INFO, "Closing solenoid");
    // TODO: Wire up solenoid control if available
    solenoidOpen_ = false;
}

void HardwareManager::emergencyShutdown() noexcept {
    LOG(ERROR, "EMERGENCY SHUTDOWN ACTIVATED");
    emergencyMode_ = true;
    disableAllHardware();
    LOG(ERROR, "All hardware disabled - system in emergency mode");
}

void HardwareManager::clearEmergencyMode() noexcept {
    LOG(INFO, "Emergency mode cleared - hardware can resume normal operation");
    emergencyMode_ = false;
}

void HardwareManager::safeHardwareShutdown() noexcept {
    LOG(INFO, "Safe hardware shutdown - disabling all hardware");
    disableAllHardware();
    // emergencyMode_ intentionally NOT set — hardware can be re-enabled normally
}

void HardwareManager::disableAllHardware() noexcept {
    if (heaterRelay_ && heaterEnabled_) {
        heaterRelay_->off();
        heaterEnabled_ = false;
    }
    if (pumpRelay_ && pumpEnabled_) {
        pumpRelay_->off();
        pumpEnabled_ = false;
    }
    if (valveRelay_ && valveState_ != CleverCoffee::Hardware::ValveState::CLOSED) {
        valveRelay_->off();
        valveState_ = CleverCoffee::Hardware::ValveState::CLOSED;
    }
    solenoidOpen_ = false;
}

void HardwareManager::setWaterTankEmpty(bool empty) noexcept {
    if (empty == waterTankEmpty_) {
        return;
    }
    waterTankEmpty_ = empty;
    if (empty) {
        LOG(WARNING, "Water tank is empty - pump operations disabled");
        // Immediately disable pump if running
        if (pumpRelay_ && pumpEnabled_) {
            pumpRelay_->off();
            pumpEnabled_ = false;
        }
    } else {
        LOG(INFO, "Water tank refilled - pump operations enabled");
    }
}

void HardwareManager::cleanupPartialInit() noexcept {
    // This method MUST NOT throw exceptions
    // Cleanup happens in REVERSE order of initialization

    LOG(INFO, "Performing emergency cleanup of partial hardware initialization...");

    // Cleanup temperature sensor (initialized last)
    if (tempSensorInitialized_) {
        LOG(INFO, "Cleaning up temperature sensor...");
        // Temperature sensor cleanup: just release the resource
        tempSensor_.reset();
        tempSensorInitialized_ = false;
        LOG(INFO, "Temperature sensor cleaned up");
    }

    // Cleanup switches
    if (switchesInitialized_) {
        LOG(INFO, "Cleaning up switches...");
        // Switches don't need physical cleanup, just release resources
        hotWaterSwitch_.reset();
        steamSwitch_.reset();
        brewSwitch_.reset();
        powerSwitch_.reset();
        switchesInitialized_ = false;
        LOG(INFO, "Switches cleaned up");
    }

    // Cleanup LEDs
    if (ledsInitialized_) {
        LOG(INFO, "Cleaning up LEDs...");
        // Turn off LEDs before releasing
        if (steamLed_) {
            steamLed_->turnOff();
            steamLed_.reset();
        }
        if (brewLed_) {
            brewLed_->turnOff();
            brewLed_.reset();
        }
        if (statusLed_) {
            statusLed_->turnOff();
            statusLed_.reset();
        }
        // Release LED pins
        steamLedPin_.reset();
        brewLedPin_.reset();
        statusLedPin_.reset();
        ledsInitialized_ = false;
        LOG(INFO, "LEDs cleaned up");
    }

    // Cleanup relays (SAFETY CRITICAL - initialized first, cleaned up last)
    if (relaysInitialized_) {
        LOG(INFO, "Cleaning up relays (SAFETY CRITICAL)...");
        // SAFETY CRITICAL: Turn off all relays before releasing
        // This ensures the coffee machine is in a safe state
        if (heaterRelay_) {
            heaterRelay_->off();
            LOG(INFO, "Heater relay turned OFF");
            heaterRelay_.reset();
        }
        if (pumpRelay_) {
            pumpRelay_->off();
            LOG(INFO, "Pump relay turned OFF");
            pumpRelay_.reset();
        }
        if (valveRelay_) {
            valveRelay_->off();
            LOG(INFO, "Valve relay turned OFF");
            valveRelay_.reset();
        }
        relaysInitialized_ = false;
        LOG(INFO, "Relays cleaned up - hardware is safe");
    }

    LOG(INFO, "Emergency cleanup completed - hardware is in safe state");
}

Scale* HardwareManager::getScale() noexcept {
    // Scale is currently managed separately, return nullptr
    // TODO: Move scale management to HardwareManager in future refactoring
    return nullptr;
}

const Scale* HardwareManager::getScale() const noexcept {
    // Scale is currently managed separately, return nullptr
    // TODO: Move scale management to HardwareManager in future refactoring
    return nullptr;
}
