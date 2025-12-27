/**
 * @file HardwareManager.cpp
 * @brief Implementation of RAII wrapper for hardware component management
 */

#include "clevercoffee/hardware/HardwareManager.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/hardware/pinmapping.h"
#include "clevercoffee/utils/memoryUtils.h"

#include <cmath>

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
    const auto heaterTriggerType =
        static_cast<Hardware::RelayTriggerType>(config_.hardwareRelaysHeaterTriggerType.get());
    heaterRelay_ = std::make_unique<Relay>(heaterRelayPin_, heaterTriggerType);
    heaterRelay_->off();
    LOG(INFO, "Heater relay initialized");

    LOG(INFO, "Initializing valve relay...");
    yield(); // Prevent watchdog timeout
    const auto valveTriggerType =
        static_cast<Hardware::RelayTriggerType>(config_.hardwareRelaysValveTriggerType.get());
    valveRelay_ = std::make_unique<Relay>(valveRelayPin_, valveTriggerType);
    valveRelay_->off();
    LOG(INFO, "Valve relay initialized");

    LOG(INFO, "Initializing pump relay...");
    yield(); // Prevent watchdog timeout
    const auto pumpTriggerType =
        static_cast<Hardware::RelayTriggerType>(config_.hardwareRelaysPumpTriggerType.get());
    pumpRelay_ = std::make_unique<Relay>(pumpRelayPin_, pumpTriggerType);
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
        const auto initialState = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        powerSwitch_ = std::make_unique<IOSwitch>(PIN_POWERSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Power switch initialized");
    }

    // Initialize steam switch
    if (config_.hardwareSwitchesSteamEnabled.get()) {
        LOG(INFO, "Initializing steam switch...");
        yield(); // Prevent watchdog timeout
        const auto type         = config_.hardwareSwitchesSteamType.get();
        const auto mode         = config_.hardwareSwitchesSteamMode.get();
        const auto initialState = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        steamSwitch_ = std::make_unique<IOSwitch>(PIN_STEAMSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Steam switch initialized");
    }

    // Initialize brew switch
    if (config_.hardwareSwitchesBrewEnabled.get()) {
        LOG(INFO, "Initializing brew switch...");
        yield(); // Prevent watchdog timeout
        const auto type         = config_.hardwareSwitchesBrewType.get();
        const auto mode         = config_.hardwareSwitchesBrewMode.get();
        const auto initialState = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        brewSwitch_ = std::make_unique<IOSwitch>(PIN_BREWSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Brew switch initialized");
    }

    // Initialize hot water switch
    if (config_.hardwareSwitchesHotWaterEnabled.get()) {
        LOG(INFO, "Initializing hot water switch...");
        yield(); // Prevent watchdog timeout
        const auto type         = config_.hardwareSwitchesHotWaterType.get();
        const auto mode         = config_.hardwareSwitchesHotWaterMode.get();
        const auto initialState = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        hotWaterSwitch_ = std::make_unique<IOSwitch>(PIN_WATERSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Hot water switch initialized");
    }

    // Initialize water tank sensor
    if (config_.hardwareSensorsWatertankEnabled.get()) {
        LOG(INFO, "Initializing water tank sensor...");
        yield(); // Prevent watchdog timeout
        const auto          mode         = config_.hardwareSensorsWatertankMode.get();
        const auto          initialState = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? HIGH : LOW;
        const GPIOPin::Type pinType =
            (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? GPIOPin::IN_PULLDOWN : GPIOPin::IN_PULLUP;
        waterTankSensor_ =
            std::make_unique<IOSwitch>(PIN_WATERTANKSENSOR, pinType, Hardware::SwitchType::TOGGLE, mode, initialState);
        LOG(INFO, "Water tank sensor initialized");
    }

    LOG(INFO, "Switches initialized successfully");
}

void HardwareManager::initializeTemperatureSensor() {
    LOG(INFO, "Getting temperature sensor type from config...");
    yield(); // Prevent watchdog timeout
    const Hardware::TemperatureSensorType tempSensorType = config_.hardwareSensorsTemperatureType.get();
    LOGF(INFO, "Temperature sensor type: %d", static_cast<int>(tempSensorType));

    if (tempSensorType == Hardware::TemperatureSensorType::TSIC_306) {
        LOG(INFO, "Initializing TSIC 306 temperature sensor...");
        yield(); // Prevent watchdog timeout
        tempSensor_ = std::make_unique<TempSensorTSIC>(PIN_TEMPSENSOR);
        LOG(INFO, "TSIC 306 temperature sensor created");
    } else if (tempSensorType == Hardware::TemperatureSensorType::DALLAS_DS18B20) {
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

    // Turn off all relays
    if (heaterRelay_) heaterRelay_->off();
    if (pumpRelay_) pumpRelay_->off();
    if (valveRelay_) valveRelay_->off();

    // Turn off all LEDs
    if (statusLed_) statusLed_->turnOff();
    if (brewLed_) brewLed_->turnOff();
    if (steamLed_) steamLed_->turnOff();

    LOG(INFO, "Safe hardware shutdown completed");
}

void HardwareManager::updateLEDs(int machineState, double temperature, double setpoint) {
    // Need to include MachineStateIds for this to work properly
    // For now, use integer constants to avoid circular dependencies
    const int PID_NORMAL = 20;
    const int BREW_IDLE = 10;
    const int BREW_PREINFUSION = 11;
    const int BREW_PREINFUSION_PAUSE = 12;
    const int BREW_RUNNING = 13;
    const int BREW_FINISHED = 14;
    const int STEAM_IDLE = 31;
    const int STEAM_RUNNING = 32;
    const int STEAM_STOPPED = 33;

    // Status LED - indicates when temperature is reached
    if (config_.hardwareLedsStatusEnabled.get() && statusLed_) {
        bool shouldTurnOn = false;

        // Turn on when at target temperature (normal or steam mode)
        if ((machineState == PID_NORMAL && (abs(temperature - setpoint) < 0.3)) ||
            (temperature > 115 && abs(temperature - setpoint) < 5)) {
            shouldTurnOn = true;
        }

        if (shouldTurnOn) {
            statusLed_->turnOn();
        } else {
            statusLed_->turnOff();
        }
    }

    // Brew LED - indicates brewing state
    if (config_.hardwareLedsBrewEnabled.get() && brewLed_) {
        bool isBrewState = (machineState >= BREW_IDLE && machineState <= BREW_FINISHED);
        if (isBrewState) {
            brewLed_->turnOn();
        } else {
            brewLed_->turnOff();
        }
    }

    // Steam LED - indicates steam mode
    if (config_.hardwareLedsSteamEnabled.get() && steamLed_) {
        bool isSteamState = (machineState >= STEAM_IDLE && machineState <= STEAM_STOPPED);
        if (isSteamState) {
            steamLed_->turnOn();
        } else {
            steamLed_->turnOff();
        }
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
        waterTankSensor_.reset();
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
