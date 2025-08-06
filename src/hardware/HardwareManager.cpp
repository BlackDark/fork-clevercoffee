/**
 * @file HardwareManager.cpp
 * @brief Implementation of RAII wrapper for hardware component management
 */

#include "HardwareManager.h"
#include "../Config.h"
#include "../utils/memoryUtils.h"
#include "Logger.h"
#include "pinmapping.h"

#if __cplusplus >= 202300L
// Use modern logging when available
#define LOG_HW(level, ...) MODERN_LOG(level, __VA_ARGS__)
#else
#define LOG_HW(level, message) LOG(level, message)
#define LOGF_HW(level, format, ...) LOGF(level, format, __VA_ARGS__)
#endif

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
    const auto heaterTriggerType = static_cast<Relay::TriggerType>(Config::getInstance().hardwareRelaysHeaterTriggerType.get());
    heaterRelay_ = std::make_unique<Relay>(heaterRelayPin_, heaterTriggerType);
    heaterRelay_->off();
    LOG(INFO, "Heater relay initialized");

    LOG(INFO, "Initializing valve relay...");
    yield(); // Prevent watchdog timeout
    const auto valveTriggerType = static_cast<Relay::TriggerType>(Config::getInstance().hardwareRelaysValveTriggerType.get());
    valveRelay_ = std::make_unique<Relay>(valveRelayPin_, valveTriggerType);
    valveRelay_->off();
    LOG(INFO, "Valve relay initialized");

    LOG(INFO, "Initializing pump relay...");
    yield(); // Prevent watchdog timeout
    const auto pumpTriggerType = static_cast<Relay::TriggerType>(Config::getInstance().hardwareRelaysPumpTriggerType.get());
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
        const auto type = static_cast<Switch::Type>(Config::getInstance().hardwareSwitchesPowerType.get());
        const auto mode = static_cast<Switch::Mode>(Config::getInstance().hardwareSwitchesPowerMode.get());
        powerSwitch_ = std::make_unique<IOSwitch>(PIN_POWERSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
        LOG(INFO, "Power switch initialized");
    }

    // Initialize steam switch
    if (Config::getInstance().hardwareSwitchesSteamEnabled.get()) {
        LOG(INFO, "Initializing steam switch...");
        yield(); // Prevent watchdog timeout
        const auto type = static_cast<Switch::Type>(Config::getInstance().hardwareSwitchesSteamType.get());
        const auto mode = static_cast<Switch::Mode>(Config::getInstance().hardwareSwitchesSteamMode.get());
        steamSwitch_ = std::make_unique<IOSwitch>(PIN_STEAMSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
        LOG(INFO, "Steam switch initialized");
    }

    // Initialize brew switch
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        LOG(INFO, "Initializing brew switch...");
        yield(); // Prevent watchdog timeout
        const auto type = static_cast<Switch::Type>(Config::getInstance().hardwareSwitchesBrewType.get());
        const auto mode = static_cast<Switch::Mode>(Config::getInstance().hardwareSwitchesBrewMode.get());
        brewSwitch_ = std::make_unique<IOSwitch>(PIN_BREWSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
        LOG(INFO, "Brew switch initialized");
    }

    // Initialize hot water switch
    if (Config::getInstance().hardwareSwitchesHotWaterEnabled.get()) {
        LOG(INFO, "Initializing hot water switch...");
        yield(); // Prevent watchdog timeout
        const auto type = static_cast<Switch::Type>(Config::getInstance().hardwareSwitchesHotWaterType.get());
        const auto mode = static_cast<Switch::Mode>(Config::getInstance().hardwareSwitchesHotWaterMode.get());
        hotWaterSwitch_ = std::make_unique<IOSwitch>(PIN_WATERSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
        LOG(INFO, "Hot water switch initialized");
    }

    // Initialize water tank sensor
    if (Config::getInstance().hardwareSensorsWatertankEnabled.get()) {
        LOG(INFO, "Initializing water tank sensor...");
        yield(); // Prevent watchdog timeout
        const auto mode = static_cast<Switch::Mode>(Config::getInstance().hardwareSensorsWatertankMode.get());
        const GPIOPin::Type pinType = (mode == Switch::NORMALLY_OPEN) ? GPIOPin::IN_PULLDOWN : GPIOPin::IN_PULLUP;
        waterTankSensor_ = std::make_unique<IOSwitch>(PIN_WATERTANKSENSOR, pinType, Switch::TOGGLE, mode, !mode);
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

#if __cplusplus >= 202002L

// Private constructor for modern initialization
HardwareManager::HardwareManager(bool modernInit) :
    heaterRelayPin_(PIN_HEATER, GPIOPin::OUT), 
    pumpRelayPin_(PIN_PUMP, GPIOPin::OUT), 
    valveRelayPin_(PIN_VALVE, GPIOPin::OUT) {
    // Skip automatic initialization - will be done explicitly through initializeModern()
}

std::expected<std::unique_ptr<HardwareManager>, HardwareInitError> HardwareManager::createModern() {
    LOG(INFO, "Creating HardwareManager with modern C++23 initialization...");
    
    try {
        auto manager = std::unique_ptr<HardwareManager>(new HardwareManager(true));
        
        if (auto result = manager->initializeModern(); !result) {
            return std::unexpected(result.error());
        }
        
        LOG(INFO, "HardwareManager created successfully with modern initialization");
        return manager;
        
    } catch (const std::bad_alloc&) {
        return std::unexpected(HardwareInitError::MemoryAllocationFailed);
    } catch (...) {
        return std::unexpected(HardwareInitError::GPIOInitFailed);
    }
}

std::expected<void, HardwareInitError> HardwareManager::initializeModern() {
    LOG(INFO, "Initializing hardware components with modern error handling...");
    logMemory("HardwareManager Modern Init Start");
    
    // Initialize components in order, with specific error handling
    if (auto result = initializeRelaysModern(); !result) {
        return std::unexpected(result.error());
    }
    
    if (auto result = initializeLEDsModern(); !result) {
        return std::unexpected(result.error());
    }
    
    if (auto result = initializeSwitchesModern(); !result) {
        return std::unexpected(result.error());
    }
    
    if (auto result = initializeTemperatureSensorModern(); !result) {
        return std::unexpected(result.error());
    }
    
    logMemory("HardwareManager Modern Init Complete");
    LOG(INFO, "Hardware initialization completed successfully with modern error handling");
    
    return {};
}

std::expected<void, HardwareInitError> HardwareManager::initializeRelaysModern() {
    LOG(INFO, "Modern relay initialization starting...");
    
    try {
        // Initialize heater relay
#if __cplusplus >= 202300L
        LOG_HW(INFO, "Initializing heater relay with trigger type: {}", 
               static_cast<int>(Config::getInstance().hardwareRelaysHeaterTriggerType.get()));
#else
        LOG(INFO, "Initializing heater relay...");
#endif
        yield(); // Prevent watchdog timeout
        
        const auto heaterTriggerType = static_cast<Relay::TriggerType>(
            Config::getInstance().hardwareRelaysHeaterTriggerType.get());
        heaterRelay_ = std::make_unique<Relay>(heaterRelayPin_, heaterTriggerType);
        heaterRelay_->off();
        
        // Initialize valve relay
        LOG(INFO, "Initializing valve relay...");
        yield(); // Prevent watchdog timeout
        
        const auto valveTriggerType = static_cast<Relay::TriggerType>(
            Config::getInstance().hardwareRelaysValveTriggerType.get());
        valveRelay_ = std::make_unique<Relay>(valveRelayPin_, valveTriggerType);
        valveRelay_->off();
        
        // Initialize pump relay
        LOG(INFO, "Initializing pump relay...");
        yield(); // Prevent watchdog timeout
        
        const auto pumpTriggerType = static_cast<Relay::TriggerType>(
            Config::getInstance().hardwareRelaysPumpTriggerType.get());
        pumpRelay_ = std::make_unique<Relay>(pumpRelayPin_, pumpTriggerType);
        pumpRelay_->off();
        
        LOG(INFO, "Modern relay initialization completed successfully");
        return {};
        
    } catch (const std::bad_alloc&) {
        LOG(ERROR, "Memory allocation failed during relay initialization");
        return std::unexpected(HardwareInitError::MemoryAllocationFailed);
    } catch (...) {
        LOG(ERROR, "Relay initialization failed with unknown error");
        return std::unexpected(HardwareInitError::RelayInitFailed);
    }
}

std::expected<void, HardwareInitError> HardwareManager::initializeLEDsModern() {
    LOG(INFO, "Modern LED initialization starting...");
    
    try {
        const auto& config = Config::getInstance();
        
        // Initialize status LED
        if (config.hardwareLedsStatusEnabled.get()) {
#if __cplusplus >= 202300L
            LOG_HW(INFO, "Initializing status LED (inverted: {})", 
                   config.hardwareLedsStatusInverted.get());
#else
            LOG(INFO, "Initializing status LED...");
#endif
            yield(); // Prevent watchdog timeout
            
            const bool inverted = config.hardwareLedsStatusInverted.get();
            statusLedPin_ = std::make_unique<GPIOPin>(PIN_STATUSLED, GPIOPin::OUT);
            statusLed_ = std::make_unique<StandardLED>(*statusLedPin_, inverted);
            statusLed_->turnOff();
        }
        
        // Initialize brew LED
        if (config.hardwareLedsBrewEnabled.get()) {
            LOG(INFO, "Initializing brew LED...");
            yield(); // Prevent watchdog timeout
            
            const bool inverted = config.hardwareLedsBrewInverted.get();
            brewLedPin_ = std::make_unique<GPIOPin>(PIN_BREWLED, GPIOPin::OUT);
            brewLed_ = std::make_unique<StandardLED>(*brewLedPin_, inverted);
            brewLed_->turnOff();
        }
        
        // Initialize steam LED
        if (config.hardwareLedsSteamEnabled.get()) {
            LOG(INFO, "Initializing steam LED...");
            yield(); // Prevent watchdog timeout
            
            const bool inverted = config.hardwareLedsSteamInverted.get();
            steamLedPin_ = std::make_unique<GPIOPin>(PIN_STEAMLED, GPIOPin::OUT);
            steamLed_ = std::make_unique<StandardLED>(*steamLedPin_, inverted);
            steamLed_->turnOff();
        }
        
        LOG(INFO, "Modern LED initialization completed successfully");
        return {};
        
    } catch (const std::bad_alloc&) {
        LOG(ERROR, "Memory allocation failed during LED initialization");
        return std::unexpected(HardwareInitError::MemoryAllocationFailed);
    } catch (...) {
        LOG(ERROR, "LED initialization failed with unknown error");
        return std::unexpected(HardwareInitError::LEDInitFailed);
    }
}

std::expected<void, HardwareInitError> HardwareManager::initializeSwitchesModern() {
    LOG(INFO, "Modern switch initialization starting...");
    
    try {
        const auto& config = Config::getInstance();
        
        // Initialize power switch
        if (config.hardwareSwitchesPowerEnabled.get()) {
#if __cplusplus >= 202300L
            LOG_HW(INFO, "Initializing power switch (type: {}, mode: {})",
                   static_cast<int>(config.hardwareSwitchesPowerType.get()),
                   static_cast<int>(config.hardwareSwitchesPowerMode.get()));
#else
            LOG(INFO, "Initializing power switch...");
#endif
            yield(); // Prevent watchdog timeout
            
            const auto type = static_cast<Switch::Type>(config.hardwareSwitchesPowerType.get());
            const auto mode = static_cast<Switch::Mode>(config.hardwareSwitchesPowerMode.get());
            powerSwitch_ = std::make_unique<IOSwitch>(PIN_POWERSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
        }
        
        // Initialize steam switch
        if (config.hardwareSwitchesSteamEnabled.get()) {
            LOG(INFO, "Initializing steam switch...");
            yield(); // Prevent watchdog timeout
            
            const auto type = static_cast<Switch::Type>(config.hardwareSwitchesSteamType.get());
            const auto mode = static_cast<Switch::Mode>(config.hardwareSwitchesSteamMode.get());
            steamSwitch_ = std::make_unique<IOSwitch>(PIN_STEAMSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
        }
        
        // Initialize brew switch
        if (config.hardwareSwitchesBrewEnabled.get()) {
            LOG(INFO, "Initializing brew switch...");
            yield(); // Prevent watchdog timeout
            
            const auto type = static_cast<Switch::Type>(config.hardwareSwitchesBrewType.get());
            const auto mode = static_cast<Switch::Mode>(config.hardwareSwitchesBrewMode.get());
            brewSwitch_ = std::make_unique<IOSwitch>(PIN_BREWSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
        }
        
        // Initialize hot water switch
        if (config.hardwareSwitchesHotWaterEnabled.get()) {
            LOG(INFO, "Initializing hot water switch...");
            yield(); // Prevent watchdog timeout
            
            const auto type = static_cast<Switch::Type>(config.hardwareSwitchesHotWaterType.get());
            const auto mode = static_cast<Switch::Mode>(config.hardwareSwitchesHotWaterMode.get());
            hotWaterSwitch_ = std::make_unique<IOSwitch>(PIN_WATERSWITCH, GPIOPin::IN_HARDWARE, type, mode, mode);
        }
        
        // Initialize water tank sensor
        if (config.hardwareSensorsWatertankEnabled.get()) {
            LOG(INFO, "Initializing water tank sensor...");
            yield(); // Prevent watchdog timeout
            
            const auto mode = static_cast<Switch::Mode>(config.hardwareSensorsWatertankMode.get());
            const GPIOPin::Type pinType = (mode == Switch::NORMALLY_OPEN) ? GPIOPin::IN_PULLDOWN : GPIOPin::IN_PULLUP;
            waterTankSensor_ = std::make_unique<IOSwitch>(PIN_WATERTANKSENSOR, pinType, Switch::TOGGLE, mode, !mode);
        }
        
        LOG(INFO, "Modern switch initialization completed successfully");
        return {};
        
    } catch (const std::bad_alloc&) {
        LOG(ERROR, "Memory allocation failed during switch initialization");
        return std::unexpected(HardwareInitError::MemoryAllocationFailed);
    } catch (...) {
        LOG(ERROR, "Switch initialization failed with unknown error");
        return std::unexpected(HardwareInitError::SwitchInitFailed);
    }
}

std::expected<void, HardwareInitError> HardwareManager::initializeTemperatureSensorModern() {
    LOG(INFO, "Modern temperature sensor initialization starting...");
    
    try {
        const Hardware::TemperatureSensorType tempSensorType = 
            Config::getInstance().hardwareSensorsTemperatureType.get();
            
#if __cplusplus >= 202300L
        LOG_HW(INFO, "Temperature sensor type: {} ({})", 
               static_cast<int>(tempSensorType),
               (tempSensorType == Hardware::TemperatureSensorType::TSIC_306) ? "TSIC 306" : "Dallas DS18B20");
#else
        LOGF(INFO, "Temperature sensor type: %d", static_cast<int>(tempSensorType));
#endif
        
        if (tempSensorType == Hardware::TemperatureSensorType::TSIC_306) {
            LOG(INFO, "Creating TSIC 306 temperature sensor...");
            yield(); // Prevent watchdog timeout
            tempSensor_ = std::make_unique<TempSensorTSIC>(PIN_TEMPSENSOR);
        }
        else if (tempSensorType == Hardware::TemperatureSensorType::DALLAS_DS18B20) {
            LOG(INFO, "Creating Dallas DS18B20 temperature sensor...");
            yield(); // Prevent watchdog timeout
            tempSensor_ = std::make_unique<TempSensorDallas>(PIN_TEMPSENSOR);
        }
        else {
            LOG(ERROR, "Unknown temperature sensor type specified in configuration");
            return std::unexpected(HardwareInitError::UnknownComponentType);
        }
        
        if (!tempSensor_) {
            LOG(ERROR, "Temperature sensor creation failed");
            return std::unexpected(HardwareInitError::TemperatureSensorFailed);
        }
        
        LOG(INFO, "Modern temperature sensor initialization completed successfully");
        return {};
        
    } catch (const std::bad_alloc&) {
        LOG(ERROR, "Memory allocation failed during temperature sensor initialization");
        return std::unexpected(HardwareInitError::MemoryAllocationFailed);
    } catch (...) {
        LOG(ERROR, "Temperature sensor initialization failed with unknown error");
        return std::unexpected(HardwareInitError::TemperatureSensorFailed);
    }
}

#endif // __cplusplus >= 202002L
