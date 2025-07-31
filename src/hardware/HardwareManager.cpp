/**
 * @file HardwareManager.cpp
 * @brief Implementation of RAII wrapper for hardware component management
 */

#include "HardwareManager.h"
#include "../Config.h"
#include "Logger.h"
#include "pinmapping.h"

HardwareManager::HardwareManager() :
    heaterRelayPin_(PIN_HEATER, GPIOPin::OUT), pumpRelayPin_(PIN_PUMP, GPIOPin::OUT), valveRelayPin_(PIN_VALVE, GPIOPin::OUT) {

    LOG(INFO, "Initializing hardware components...");

    try {
        initializeRelays();
        initializeLEDs();
        initializeSwitches();
        initializeTemperatureSensor();

        LOG(INFO, "Hardware initialization completed successfully");
    } catch (const std::exception& e) {
        LOG(ERROR, "Hardware initialization failed");
        throw;
    }
}

void HardwareManager::initializeRelays() {
    // Initialize heater relay
    const auto heaterTriggerType = static_cast<Relay::TriggerType>(Config::getInstance().get<int>("hardware.relays.heater.trigger_type"));
    heaterRelay_ = std::make_unique<Relay>(heaterRelayPin_, heaterTriggerType);
    heaterRelay_->off();

    // Initialize valve relay
    const auto valveTriggerType = static_cast<Relay::TriggerType>(Config::getInstance().get<int>("hardware.relays.valve.trigger_type"));
    valveRelay_ = std::make_unique<Relay>(valveRelayPin_, valveTriggerType);
    valveRelay_->off();

    // Initialize pump relay
    const auto pumpTriggerType = static_cast<Relay::TriggerType>(Config::getInstance().get<int>("hardware.relays.pump.trigger_type"));
    pumpRelay_ = std::make_unique<Relay>(pumpRelayPin_, pumpTriggerType);
    pumpRelay_->off();

    LOG(INFO, "Relays initialized successfully");
}

void HardwareManager::initializeLEDs() {
    const auto& config = Config::getInstance();

    // Initialize status LED
    if (config.get<bool>("hardware.leds.status.enabled")) {
        const bool inverted = config.get<bool>("hardware.leds.status.inverted");
        statusLedPin_ = std::make_unique<GPIOPin>(PIN_STATUSLED, GPIOPin::OUT);
        statusLed_ = std::make_unique<StandardLED>(*statusLedPin_, inverted);
        statusLed_->turnOff();
    }

    // Initialize brew LED
    if (config.get<bool>("hardware.leds.brew.enabled")) {
        const bool inverted = config.get<bool>("hardware.leds.brew.inverted");
        brewLedPin_ = std::make_unique<GPIOPin>(PIN_BREWLED, GPIOPin::OUT);
        brewLed_ = std::make_unique<StandardLED>(*brewLedPin_, inverted);
        brewLed_->turnOff();
    }

    // Initialize steam LED
    if (config.get<bool>("hardware.leds.steam.enabled")) {
        const bool inverted = config.get<bool>("hardware.leds.steam.inverted");
        steamLedPin_ = std::make_unique<GPIOPin>(PIN_STEAMLED, GPIOPin::OUT);
        steamLed_ = std::make_unique<StandardLED>(*steamLedPin_, inverted);
        steamLed_->turnOff();
    }

    LOG(INFO, "LEDs initialized successfully");
}

void HardwareManager::initializeSwitches() {
    const auto& config = Config::getInstance();

    // Initialize power switch
    if (config.get<bool>("hardware.switches.power.enabled")) {
        const auto type = static_cast<Switch::Type>(config.get<int>("hardware.switches.power.type"));
        const auto mode = static_cast<Switch::Mode>(config.get<int>("hardware.switches.power.mode"));
        powerSwitch_ = std::make_unique<IOSwitch>(PIN_POWERSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
    }

    // Initialize steam switch
    if (config.get<bool>("hardware.switches.steam.enabled")) {
        const auto type = static_cast<Switch::Type>(config.get<int>("hardware.switches.steam.type"));
        const auto mode = static_cast<Switch::Mode>(config.get<int>("hardware.switches.steam.mode"));
        steamSwitch_ = std::make_unique<IOSwitch>(PIN_STEAMSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
    }

    // Initialize brew switch
    if (config.get<bool>("hardware.switches.brew.enabled")) {
        const auto type = static_cast<Switch::Type>(config.get<int>("hardware.switches.brew.type"));
        const auto mode = static_cast<Switch::Mode>(config.get<int>("hardware.switches.brew.mode"));
        brewSwitch_ = std::make_unique<IOSwitch>(PIN_BREWSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
    }

    // Initialize hot water switch
    if (config.get<bool>("hardware.switches.hot_water.enabled")) {
        const auto type = static_cast<Switch::Type>(config.get<int>("hardware.switches.hot_water.type"));
        const auto mode = static_cast<Switch::Mode>(config.get<int>("hardware.switches.hot_water.mode"));
        hotWaterSwitch_ = std::make_unique<IOSwitch>(PIN_WATERSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
    }

    // Initialize water tank sensor
    if (config.get<bool>("hardware.sensors.watertank.enabled")) {
        const auto mode = static_cast<Switch::Mode>(config.get<int>("hardware.sensors.watertank.mode"));
        const GPIOPin::Type pinType = (mode == Switch::NORMALLY_OPEN) ? GPIOPin::IN_PULLDOWN : GPIOPin::IN_PULLUP;
        waterTankSensor_ = std::make_unique<IOSwitch>(PIN_WATERTANKSENSOR, pinType, Switch::TOGGLE, mode, !mode);
    }

    LOG(INFO, "Switches initialized successfully");
}

void HardwareManager::initializeTemperatureSensor() {
    const int tempSensorType = Config::getInstance().get<int>("hardware.sensors.temperature.type");

    if (tempSensorType == 0) {
        tempSensor_ = std::make_unique<TempSensorTSIC>(PIN_TEMPSENSOR);
    }
    else if (tempSensorType == 1) {
        tempSensor_ = std::make_unique<TempSensorDallas>(PIN_TEMPSENSOR);
    }

    if (tempSensor_) {
        LOG(INFO, "Temperature sensor initialized successfully");
    }
    else {
        LOGF(ERROR, "Unknown temperature sensor type: %d", tempSensorType);
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