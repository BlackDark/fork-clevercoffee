/**
 * @file HardwareManager.cpp
 * @brief Implementation of RAII wrapper for hardware component management
 */

#include "clevercoffee/hardware/HardwareManager.h"
#include "clevercoffee/Config.h"
#include "clevercoffee/utils/memoryUtils.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/hardware/pinmapping.h"

HardwareManager::HardwareManager() :
    heaterRelayPin_(PIN_HEATER, GPIOPin::OUT), pumpRelayPin_(PIN_PUMP, GPIOPin::OUT), valveRelayPin_(PIN_VALVE, GPIOPin::OUT) {

    LOG(INFO, "Initializing hardware components...");
    logMemory("HardwareManager Constructor Start");

    try {
        LOG(INFO, "Starting relay initialization...");
        logMemoryBasic("Before Relay Init");
        initializeRelays();
        logMemoryBasic("After Relay Init");
        LOG(INFO, "Relays initialized, starting LED initialization...");

        logMemoryBasic("Before LED Init");
        initializeLEDs();
        logMemoryBasic("After LED Init");
        LOG(INFO, "LEDs initialized, starting switch initialization...");

        logMemoryBasic("Before Switch Init");
        initializeSwitches();
        logMemoryBasic("After Switch Init");
        LOG(INFO, "Switches initialized, starting temperature sensor initialization...");

        logMemoryBasic("Before TempSensor Init");
        initializeTemperatureSensor();
        logMemoryBasic("After TempSensor Init");
        LOG(INFO, "Temperature sensor initialized successfully");

        logMemory("HardwareManager Constructor Complete");
        LOG(INFO, "Hardware initialization completed successfully");
    } catch (const std::exception& e) {
        logMemory("HardwareManager Constructor FAILED");
        LOG(ERROR, "Hardware initialization failed");
        throw;
    }
}

void HardwareManager::initializeRelays() {
    LOG(INFO, "Initializing heater relay...");
    yield(); // Prevent watchdog timeout
    const auto heaterTriggerType = static_cast<Hardware::RelayTriggerType>(Config::getInstance().hardwareRelaysHeaterTriggerType.get());
    heaterRelay_ = std::make_unique<Relay>(heaterRelayPin_, heaterTriggerType);
    heaterRelay_->off();
    LOG(INFO, "Heater relay initialized");

    LOG(INFO, "Initializing valve relay...");
    yield(); // Prevent watchdog timeout
    const auto valveTriggerType = static_cast<Hardware::RelayTriggerType>(Config::getInstance().hardwareRelaysValveTriggerType.get());
    valveRelay_ = std::make_unique<Relay>(valveRelayPin_, valveTriggerType);
    valveRelay_->off();
    LOG(INFO, "Valve relay initialized");

    LOG(INFO, "Initializing pump relay...");
    yield(); // Prevent watchdog timeout
    const auto pumpTriggerType = static_cast<Hardware::RelayTriggerType>(Config::getInstance().hardwareRelaysPumpTriggerType.get());
    pumpRelay_ = std::make_unique<Relay>(pumpRelayPin_, pumpTriggerType);
    pumpRelay_->off();
    LOG(INFO, "Pump relay initialized");

    LOG(INFO, "Relays initialized successfully");
}

void HardwareManager::initializeLEDs() {
    const auto& config = Config::getInstance();

    // Initialize status LED
    if (Config::getInstance().hardwareLedsStatusEnabled.get()) {
        LOG(INFO, "Initializing status LED...");
        yield(); // Prevent watchdog timeout
        const bool inverted = Config::getInstance().hardwareLedsStatusInverted.get();
        statusLedPin_ = std::make_unique<GPIOPin>(PIN_STATUSLED, GPIOPin::OUT);
        statusLed_ = std::make_unique<StandardLED>(*statusLedPin_, inverted);
        statusLed_->turnOff();
        LOG(INFO, "Status LED initialized");
    }

    // Initialize brew LED
    if (Config::getInstance().hardwareLedsBrewEnabled.get()) {
        LOG(INFO, "Initializing brew LED...");
        yield(); // Prevent watchdog timeout
        const bool inverted = Config::getInstance().hardwareLedsBrewInverted.get();
        brewLedPin_ = std::make_unique<GPIOPin>(PIN_BREWLED, GPIOPin::OUT);
        brewLed_ = std::make_unique<StandardLED>(*brewLedPin_, inverted);
        brewLed_->turnOff();
        LOG(INFO, "Brew LED initialized");
    }

    // Initialize steam LED
    if (Config::getInstance().hardwareLedsSteamEnabled.get()) {
        LOG(INFO, "Initializing steam LED...");
        yield(); // Prevent watchdog timeout
        const bool inverted = Config::getInstance().hardwareLedsSteamInverted.get();
        steamLedPin_ = std::make_unique<GPIOPin>(PIN_STEAMLED, GPIOPin::OUT);
        steamLed_ = std::make_unique<StandardLED>(*steamLedPin_, inverted);
        steamLed_->turnOff();
        LOG(INFO, "Steam LED initialized");
    }

    LOG(INFO, "LEDs initialized successfully");
}

void HardwareManager::initializeSwitches() {
    const auto& config = Config::getInstance();

    // Initialize power switch
    if (Config::getInstance().hardwareSwitchesPowerEnabled.get()) {
        LOG(INFO, "Initializing power switch...");
        yield(); // Prevent watchdog timeout
        const auto type = Config::getInstance().hardwareSwitchesPowerType.get();
        const auto mode = Config::getInstance().hardwareSwitchesPowerMode.get();
        const auto initialState = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        powerSwitch_ = std::make_unique<IOSwitch>(PIN_POWERSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Power switch initialized");
    }

    // Initialize steam switch
    if (Config::getInstance().hardwareSwitchesSteamEnabled.get()) {
        LOG(INFO, "Initializing steam switch...");
        yield(); // Prevent watchdog timeout
        const auto type = Config::getInstance().hardwareSwitchesSteamType.get();
        const auto mode = Config::getInstance().hardwareSwitchesSteamMode.get();
        const auto initialState = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        steamSwitch_ = std::make_unique<IOSwitch>(PIN_STEAMSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Steam switch initialized");
    }

    // Initialize brew switch
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        LOG(INFO, "Initializing brew switch...");
        yield(); // Prevent watchdog timeout
        const auto type = Config::getInstance().hardwareSwitchesBrewType.get();
        const auto mode = Config::getInstance().hardwareSwitchesBrewMode.get();
        const auto initialState = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        brewSwitch_ = std::make_unique<IOSwitch>(PIN_BREWSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Brew switch initialized");
    }

    // Initialize hot water switch
    if (Config::getInstance().hardwareSwitchesHotWaterEnabled.get()) {
        LOG(INFO, "Initializing hot water switch...");
        yield(); // Prevent watchdog timeout
        const auto type = Config::getInstance().hardwareSwitchesHotWaterType.get();
        const auto mode = Config::getInstance().hardwareSwitchesHotWaterMode.get();
        const auto initialState = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? LOW : HIGH;
        hotWaterSwitch_ = std::make_unique<IOSwitch>(PIN_WATERSWITCH, GPIOPin::IN_HARDWARE, type, mode, initialState);
        LOG(INFO, "Hot water switch initialized");
    }

    // Initialize water tank sensor
    if (Config::getInstance().hardwareSensorsWatertankEnabled.get()) {
        LOG(INFO, "Initializing water tank sensor...");
        yield(); // Prevent watchdog timeout
        const auto mode = Config::getInstance().hardwareSensorsWatertankMode.get();
        const auto initialState = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? HIGH : LOW;
        const GPIOPin::Type pinType = (mode == Hardware::SwitchMode::NORMALLY_OPEN) ? GPIOPin::IN_PULLDOWN : GPIOPin::IN_PULLUP;
        waterTankSensor_ = std::make_unique<IOSwitch>(PIN_WATERTANKSENSOR, pinType, Hardware::SwitchType::TOGGLE, mode, initialState);
        LOG(INFO, "Water tank sensor initialized");
    }

    LOG(INFO, "Switches initialized successfully");
}

void HardwareManager::initializeTemperatureSensor() {
    LOG(INFO, "Getting temperature sensor type from config...");
    yield(); // Prevent watchdog timeout
    const Hardware::TemperatureSensorType tempSensorType = Config::getInstance().hardwareSensorsTemperatureType.get();
    LOGF(INFO, "Temperature sensor type: %d", static_cast<int>(tempSensorType));

    if (tempSensorType == Hardware::TemperatureSensorType::TSIC_306) {
        LOG(INFO, "Initializing TSIC 306 temperature sensor...");
        yield(); // Prevent watchdog timeout
        tempSensor_ = std::make_unique<TempSensorTSIC>(PIN_TEMPSENSOR);
        LOG(INFO, "TSIC 306 temperature sensor created");
    }
    else if (tempSensorType == Hardware::TemperatureSensorType::DALLAS_DS18B20) {
        LOG(INFO, "Initializing Dallas DS18B20 temperature sensor...");
        yield(); // Prevent watchdog timeout
        tempSensor_ = std::make_unique<TempSensorDallas>(PIN_TEMPSENSOR);
        LOG(INFO, "Dallas DS18B20 temperature sensor created");
    }

    if (tempSensor_) {
        LOG(INFO, "Temperature sensor initialized successfully");
    }
    else {
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

