/**
 * @file SystemInitializer.cpp
 * @brief Implementation of RAII wrapper for system initialization
 */

#include "clevercoffee/core/SystemInitializer.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/display/DisplayManager.h"
#include "clevercoffee/display/DisplayTemplateManager.h"
#include "clevercoffee/display/DisplayWidgets.h"
#include "clevercoffee/display/languages.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/handlers/PowerHandler.h"
#include "clevercoffee/handlers/SteamHandler.h"
#include "clevercoffee/hardware/HardwareManager.h" // Include before own header to resolve forward declaration
#include "clevercoffee/isr.h"
#include "clevercoffee/network/CleverCoffeeWiFiManager.h"
#include "clevercoffee/network/MQTTManager.h"
#include "clevercoffee/network/WebServerManager.h"
#include "clevercoffee/ota.h"
#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/ui/OledDriver.h"
#include "clevercoffee/utils/Resilience.h"
#include "clevercoffee/utils/SystemUtils.h"
#include "clevercoffee/utils/memoryUtils.h"

#include <Arduino.h>
#include <LittleFS.h>
#include <PID_v1.h>
#include <WiFi.h>
#include <Wire.h>

// Forward declarations
extern void initTimer1();
extern void enableTimer1();
extern void disableTimer1();

namespace {

// Valid for program lifetime — SystemInitializer owns the HardwareManager
CleverCoffee::HardwareManager* g_otaHardwareManager = nullptr;

void otaPrepareHardware() noexcept {
    disableTimer1();
    if (g_otaHardwareManager) {
        g_otaHardwareManager->disableHeater();
    }
}

void otaRestoreHardware() noexcept {
    enableTimer1();
}

} // namespace

// namespace DisplayTemplateManager {
//     extern void initializeDisplay(int templateId);
// }

SystemInitializer::SystemInitializer(Watchdog* watchdog)
    : systemInitialized_(false), initState_(InitState::NOT_INITIALIZED), hostname_(), displayManager_(nullptr),
      oledDriver_(nullptr), hardwareManager_(nullptr), mqttManager_(nullptr), cleverCoffeeWiFiManager_(nullptr),
      webServerManager_(nullptr), watchdog_(watchdog) {}

SystemInitializer::~SystemInitializer() {
    // Destructor implementation - unique_ptr will automatically clean up resources
    // This needs to be defined in the .cpp file where complete type definitions are available
}

bool SystemInitializer::initialize() {
    LOG(INFO, "Starting system initialization");
    initState_ = InitState::INITIALIZING;

    // Create SystemContext first
    systemContext_ = std::make_unique<CleverCoffee::SystemContext>();

    logMemory("SystemInitializer Start");

    // Phase 1: Core system initialization
    logMemoryBasic("Before Logger Init");
    if (!initializeLogger()) {
        LOG(ERROR, "Logger initialization failed");
        initState_ = InitState::FAILED;
        return false;
    }

    logMemoryBasic("Before Config Init");
    if (!initializeConfiguration()) {
        LOG(ERROR, "Configuration initialization failed");
        initState_ = InitState::FAILED;
        return false;
    }
    logMemoryBasic("After Config Init");

    // Phase 2: Hardware initialization
    logMemoryBasic("Before Wire.begin()");
    Wire.begin();
    logMemoryBasic("After Wire.begin()");

    logMemoryBasic("Before Display Init");
    if (!initializeDisplay()) {
        // DisplayManager is required - initialization failure is critical
        LOG(ERROR, "DisplayManager initialization failed - system cannot continue");
        initState_ = InitState::FAILED;
        return false;
    }

    // DisplayManager and OledDriver always exist now - show logo if hardware is connected
    if (displayManager_->isInitialized()) {
        displayLogo(*systemContext_, "Version ", systemContext_->sysVersion());
    }

    logMemoryBasic("After Display Init");

    logMemoryBasic("Before Hardware Init");
    if (!initializeHardware()) {
        LOG(ERROR, "Hardware initialization failed");
        logMemory("Hardware Init FAILED");
        if (oledDriver_) {
            displayLogo(*systemContext_, "Error ", "Hardware initialization failed");
        }
        initState_ = InitState::FAILED;
        return false;
    }

    logMemoryBasic("After Hardware Init");

    if (!initializeHandlers()) {
        LOG(ERROR, "Handler initialization failed");
        initState_ = InitState::FAILED;
        return false;
    }

    // Phase 3: Network and services
    LOG(INFO, "Starting Phase 3: Network and services");
    if (!initializeNetworking()) {
        if (!Config::getInstance().systemOfflineMode.get()) {
            // Network is required (not in offline mode) but failed - this is a critical error
            LOG(ERROR, "Network is required but initialization failed - system cannot continue");
            initState_ = InitState::FAILED;
            return false;
        }
        // Offline mode is configured - this is OK
        LOG(INFO, "Offline mode configured, network initialization skipped");
        systemContext_->networkCoordinator().setOfflineMode(true);
    }

    LOG(INFO, "Starting MQTT initialization");
    if (!initializeMQTT()) {
        if (Config::getInstance().mqttEnabled.get() && !systemContext_->networkCoordinator().isOfflineMode()) {
            // MQTT is enabled but failed to initialize - this is a critical error
            LOG(ERROR, "MQTT is enabled but initialization failed - system cannot continue");
            initState_ = InitState::FAILED;
            return false;
        }
        // MQTT is not configured or offline mode - this is OK
        LOG(INFO, "MQTT not configured or offline mode, continuing without MQTT");
    }

    // Phase 4: PID and sensors
    LOG(INFO, "Starting Phase 4: PID and sensors");
    if (!initializePID()) {
        LOG(ERROR, "PID initialization failed");
        initState_ = InitState::FAILED;
        return false;
    }

    LOG(INFO, "Starting sensor initialization");
    if (!initializeSensors()) {
        LOG(WARNING, "Sensor initialization incomplete");
    }

    // Phase 5: Finalization
    LOG(INFO, "Starting Phase 5: Finalization");

    // CRITICAL: Set ISR SystemContext BEFORE enabling timer ISR
    // The ISR needs access to hardware context for relay control
    LOGF(DEBUG, "Setting ISR SystemContext at %p", static_cast<void*>(systemContext_.get()));
    CleverCoffee::ISR::setSystemContext(systemContext_.get());
    auto* ctxCheck = CleverCoffee::ISR::getSystemContext();
    LOGF(DEBUG,
         "Global SystemContext set: ptr=%p, valid=%d",
         static_cast<void*>(ctxCheck),
         (ctxCheck != nullptr ? 1 : 0));

    LOG(DEBUG, "Calling setupTiming()");
    setupTiming();

    LOG(DEBUG, "Calling initTimer1() - create timer after ISR context is available");
    initTimer1();

    LOG(DEBUG, "Calling enableTimer1() - ISR will now fire");
    enableTimer1();

    // Mark ISR as ready to execute - all initialization is complete
    systemContext_->markISRReady();
    LOG(INFO, "ISR marked as ready - timer ISR can now safely execute");

    LOG(DEBUG, "Timer enabled - ISR should be firing every 10ms");

    // Report LittleFS usage only if it was successfully initialized
    if (LittleFS.totalBytes() > 0) {
        double fsUsage = (static_cast<double>(LittleFS.usedBytes()) / LittleFS.totalBytes()) * 100;
        LOGF(INFO,
             "LittleFS: %d%% (used %ld bytes from %ld bytes)",
             static_cast<int>(ceil(fsUsage)),
             LittleFS.usedBytes(),
             LittleFS.totalBytes());
    } else {
        LOG(WARNING, "LittleFS not available or not initialized");
    }

    // System initialization complete
    systemInitialized_ = true; // CRITICAL: Mark system as initialized so isInitialized() returns true
    initState_         = InitState::INITIALIZED;

    logMemory("SystemInitializer Complete");
    LOG(INFO, "System initialization completed successfully");
    return true;
}

bool SystemInitializer::initializeLogger() {
    // Start serial console
    Serial.begin(115200);

    // Initialize the logger
    Logger::Config loggerConfig;
    Logger::init(loggerConfig);

    // Start the logger
    (void)Logger::begin();

    return true;
}

bool SystemInitializer::initializeConfiguration() {
    // Mount LittleFS first so we can seed NVS from /config.json on first boot
    if (!LittleFS.begin()) {
        LOG(WARNING, "LittleFS initialization failed — cannot seed from config.json");
    }

    if (!Config::getInstance().begin()) {
        LOG(ERROR, "Failed to initialize configuration system!");
        Serial.println("Critical error detected!");
        Serial.flush();
        return false;
    }

    // On first boot, seed NVS from /config.json in LittleFS (factory provisioning / Wokwi)
    if (Config::getInstance().seedFromLittleFS()) {
        LOG(INFO, "Config: Seeded NVS from LittleFS /config.json");
    }
    Logger::update(); // flush setup logs before WiFi (portal can block loop() for 60s)

    // Inject SystemContext into Config for StateParamDef lambdas
    Config::getInstance().setSystemContext(systemContext_.get());

    if (!systemContext_->maintenanceCoordinator().begin()) {
        LOG(WARNING, "Maintenance coordinator failed to load persisted state");
    }

    LOG(INFO, "Configuration system ready");

    // Set log level from configuration
    const System::LogLevel configLogLevel = Config::getInstance().systemLogLevel.get();
    Logger::setLevel(static_cast<Logger::Level>(configLogLevel));
    LOGF(INFO, "Log level set to: %s", Logger::getLevelString(static_cast<Logger::Level>(configLogLevel)));

    calculateDerivedValues();

    // Use make_unique for proper RAII and exception safety
    pidController_ = std::make_unique<PID>(systemContext_->processTemperaturePtr(),
                                           systemContext_->processPidOutputPtr(),
                                           systemContext_->processSetpointPtr(),
                                           Config::getInstance().pidRegularKp.get(),
                                           systemContext_->processPidAggKi(),
                                           systemContext_->processPidAggKd(),
                                           1,
                                           DIRECT);

    // Assign PID controller to SystemContext so it can be used
    systemContext_->setPidController(pidController_.get());

    // Set global reference for backward compatibility
    // PID controller is now managed via systemContext
    return true;
}

bool SystemInitializer::initializeDisplay() {
    initLangStrings();

    // DisplayManager is ALWAYS created - it's a required component
    // Even if feature is disabled, manager exists to track state
    try {
        const Hardware::OLEDType    displayType    = Config::getInstance().hardwareOledType.get();
        const Hardware::OLEDAddress displayAddress = Config::getInstance().hardwareOledAddress.get();

        // DisplayManager is ALWAYS created - required component
        displayManager_ = std::make_unique<DisplayManager>(displayType, displayAddress);
        if (!displayManager_) {
            LOG(ERROR, "Failed to create DisplayManager - system cannot continue");
            initState_ = InitState::FAILED;
            return false;
        }

        // OledDriver is ALWAYS created - required component
        oledDriver_ = std::make_unique<OledDriver>(displayManager_.get(), systemContext_.get());
        if (!oledDriver_) {
            LOG(ERROR, "Failed to create OledDriver - system cannot continue");
            initState_ = InitState::FAILED;
            return false;
        }

        if (Config::getInstance().hardwareOledEnabled.get()) {
            if (displayManager_->isInitialized()) {
                // Hardware is connected - proceed with full setup
                systemContext_->hardwareContext().setDisplay(displayManager_->getDisplay());

                if (oledDriver_->initialize()) {
                    LOG(INFO, "OledDriver initialized successfully");
                } else {
                    LOG(ERROR, "OledDriver initialization failed!");
                }

                DisplayTemplateManager::setSystemContext(systemContext_.get());
                LOG(INFO, "Display initialization completed");
            } else {
                // Hardware not connected, but manager exists and tracks state
                LOG(WARNING, "DisplayManager created but hardware not connected - manager will track state");
                systemContext_->hardwareContext().setDisplay(nullptr);
            }
        } else {
            LOG(INFO, "Display disabled in configuration, but DisplayManager and OledDriver created");
        }

        // Manager always exists - required component
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "Exception during display initialization: %s", e.what());
        initState_ = InitState::FAILED;
        return false;
    }
}

bool SystemInitializer::initializeHardware() {
    try {
        logMemoryBasic("Before HardwareManager Creation");
        hardwareManager_ = std::make_unique<CleverCoffee::HardwareManager>(Config::getInstance());
        logMemoryBasic("After HardwareManager Creation");

        // Update compatibility pointers to reference HardwareManager components
        logMemoryBasic("Before Hardware Pointer Updates");

        // Populate HardwareContext (modern DI approach)
        systemContext_->hardwareContext().setHeaterRelay(hardwareManager_->getHeaterRelay());
        systemContext_->hardwareContext().setPumpRelay(hardwareManager_->getPumpRelay());
        systemContext_->hardwareContext().setValveRelay(hardwareManager_->getValveRelay());

        systemContext_->hardwareContext().setStatusLed(hardwareManager_->getStatusLed());
        systemContext_->hardwareContext().setBrewLed(hardwareManager_->getBrewLed());
        systemContext_->hardwareContext().setSteamLed(hardwareManager_->getSteamLed());

        systemContext_->hardwareContext().setPowerSwitch(hardwareManager_->getPowerSwitch());
        systemContext_->hardwareContext().setBrewSwitch(hardwareManager_->getBrewSwitch());
        systemContext_->hardwareContext().setHotWaterSwitch(hardwareManager_->getHotWaterSwitch());
        systemContext_->hardwareContext().setSteamSwitch(hardwareManager_->getSteamSwitch());
        systemContext_->hardwareContext().setWaterTankSensor(hardwareManager_->getWaterTankSensor());

        systemContext_->hardwareContext().setTempSensor(hardwareManager_->getTempSensor());

        logMemoryBasic("After Hardware Pointer Updates");

        LOG(INFO, "Hardware initialization completed via HardwareManager");
        return true;
    } catch (const std::exception& e) {
        logMemory("HardwareManager Exception");
        LOGF(ERROR, "Failed to initialize HardwareManager: %s", e.what());
        return false;
    }
}

bool SystemInitializer::initializeHandlers() {
    try {
        const auto& config = Config::getInstance();
        brewHandler_       = std::make_unique<BrewHandler>(*systemContext_, config);
        hotWaterHandler_   = std::make_unique<HotWaterHandler>(*systemContext_, config);
        powerHandler_      = std::make_unique<PowerHandler>(*systemContext_, config);
        steamHandler_      = std::make_unique<SteamHandler>(*systemContext_, config);

        auto& hwContext = systemContext_->hardwareContext();
        brewHandler_->setHardware(hwContext.brewSwitch());
        hotWaterHandler_->setHardware(hwContext.hotWaterSwitch());
        powerHandler_->setHardware(hwContext.powerSwitch());
        steamHandler_->setHardware(hwContext.steamSwitch());

        systemContext_->setBrewHandler(brewHandler_.get());
        systemContext_->setHotWaterHandler(hotWaterHandler_.get());
        systemContext_->setPowerHandler(powerHandler_.get());
        systemContext_->setSteamHandler(steamHandler_.get());

        LOG(INFO, "Handlers initialized");
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "Handler initialization failed: %s", e.what());
        return false;
    }
}

bool SystemInitializer::initializeNetworking() {
    // WiFiManager is ALWAYS created - it's a required component
    // Even if offline mode, manager exists to track state
    try {
        cleverCoffeeWiFiManager_ = std::make_unique<CleverCoffeeWiFiManager>(&systemContext_->networkCoordinator());
        if (!cleverCoffeeWiFiManager_) {
            LOG(ERROR, "Failed to create WiFiManager - system cannot continue");
            initState_ = InitState::FAILED;
            return false;
        }

        systemContext_->setCleverCoffeeWiFiManager(cleverCoffeeWiFiManager_.get());
        systemContext_->setWifiManager(cleverCoffeeWiFiManager_.get());

        if (Config::getInstance().systemOfflineMode.get()) {
            LOG(INFO, "Offline mode enabled, WiFiManager created but network disabled");
            WiFi.disconnect();
            systemContext_->networkCoordinator().setOfflineMode(true);
            setUserPidEnabled(*systemContext_, true);
            return true;
        }

        setupWiFi();

        LOG(INFO, "About to initialize WebServerManager");

        // Initialize WebServerManager
        webServerManager_ = std::make_unique<WebServerManager>(80);
        webServerManager_->setSystemContext(systemContext_.get());
        systemContext_->setWebServerManager(webServerManager_.get());

        if (!webServerManager_->initialize(true)) {
            LOG(ERROR, "WebServerManager initialization failed");
            webServerManager_.reset();
        } else {
            LOG(INFO, "WebServerManager initialized successfully");
        }

        // OTA Updates
        if (WiFi.status() == WL_CONNECTED) {
            setupOtaIntegration();
            const String otaPass = Config::getInstance().systemOtaPassword.get();
            OTA::initializeArduinoOta(Config::getInstance().systemHostname.get().c_str(), otaPass.c_str());
        }

        LOG(INFO, "Network initialization completed");
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "Network initialization failed: %s", e.what());
        return false;
    }
}

void SystemInitializer::setupOtaIntegration() {
    g_otaHardwareManager = hardwareManager_.get();
    OTA::setWatchdog(watchdog_);
    OTA::setDisplayContext(systemContext_.get());
    OTA::setSessionCallbacks({otaPrepareHardware, otaRestoreHardware});
}

bool SystemInitializer::initializeMQTT() {
    // MQTTManager is ALWAYS created - it's a required component
    // Even if disabled, manager exists to track state
    try {
        mqttManager_ = std::make_unique<MQTTManager>();
        if (!mqttManager_) {
            LOG(ERROR, "Failed to create MQTTManager - system cannot continue");
            initState_ = InitState::FAILED;
            return false;
        }

        mqttManager_->setSystemContext(systemContext_.get());
        // Note: CleverCoffeeWiFiManager doesn't need SystemContext - it uses NetworkCoordinator
        mqttManager_->setUICoordinator(&systemContext_->uiCoordinator());
        mqttManager_->setSensorCoordinator(&systemContext_->sensorCoordinator());
        mqttManager_->setNetworkCoordinator(&systemContext_->networkCoordinator());

        systemContext_->setMQTTManager(mqttManager_.get());

        if (systemContext_->networkCoordinator().isOfflineMode() || !Config::getInstance().mqttEnabled.get()) {
            LOG(INFO, "MQTT disabled, but MQTTManager created");
            return true;
        }

        if (mqttManager_->setup(Config::getInstance().systemHostname.get())) {
            (void)Config::getInstance().mqttEnabled.set(mqttManager_->isEnabled());
            (void)Config::getInstance().mqttHassioEnabled.set(true);

            registerMQTTParameters();
            registerMQTTSensors();

            LOG(INFO, "MQTT setup completed via MQTTManager");
            return true;
        } else {
            LOG(WARNING, "MQTT setup returned false, but manager exists");
            return true; // Manager exists, setup can be retried later
        }
    } catch (const std::exception& e) {
        LOGF(ERROR, "Failed to initialize MQTTManager: %s", e.what());
        initState_ = InitState::FAILED;
        return false;
    }
}

bool SystemInitializer::initializePID() {
    try {
        LOGF(INFO,
             "PID initialized: Kp={:.3f}, Ki={:.3f}, Kd={:.3f}",
             Config::getInstance().pidRegularKp.get(),
             systemContext_->processPidAggKi(),
             systemContext_->processPidAggKd());

        // Set PID tunings now that parameters are calculated
        systemContext_->setPidTunings(Config::getInstance().pidRegularKp.get(),
                                      systemContext_->processPidAggKi(),
                                      systemContext_->processPidAggKd(),
                                      1);

        // Initialize PID controller
        systemContext_->setPidSampleTime(systemContext_->processWindowSize());
        systemContext_->setPidOutputLimits(0, systemContext_->processWindowSize());
        systemContext_->setPidIntegratorLimits(0, 55.0); // AGGIMAX constant
        systemContext_->setPidSmoothingFactor(Config::getInstance().pidEmaFactor.get());
        systemContext_->setPidMode(AUTOMATIC);

        LOG(INFO, "PID controller initialized");
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "PID initialization failed: %s", e.what());
        return false;
    }
}

bool SystemInitializer::initializeSensors() {
    try {
        // Get sensor coordinator from system context
        CleverCoffee::SensorCoordinator* coord = systemContext_ ? &systemContext_->sensorCoordinator() : nullptr;

        if (!coord) {
            LOG(WARNING, "SensorCoordinator not available in SystemContext");
            return false;
        }

        // Get sensor references from HardwareManager
        TempSensor* tempSensorRef      = hardwareManager_ ? hardwareManager_->getTempSensor() : nullptr;
        Switch*     waterTankSensorRef = hardwareManager_ ? hardwareManager_->getWaterTankSensor() : nullptr;

        // Inject sensors into SensorCoordinator
        if (tempSensorRef) {
            coord->setTemperatureSensor(tempSensorRef);
            LOG(INFO, "Temperature sensor injected into SensorCoordinator");
        }
        if (waterTankSensorRef) {
            coord->setWaterTankSensor(waterTankSensorRef);
            LOG(INFO, "Water tank sensor injected into SensorCoordinator");
        }

        // Update global temperature variable for compatibility
        systemContext_->setProcessTemperature(coord->getTemperature());

        LOG(INFO, "Sensor management initialized via SensorCoordinator");

        // Note: Scale initialization is still handled separately in main.cpp
        // because it requires complex global dependencies
        if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
            LOG(INFO, "Scale sensor will be initialized separately in main.cpp");
        }

        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "Sensor initialization failed: %s", e.what());
        return false;
    }
}

bool SystemInitializer::finalizeMachineState() {
    try {
        // For momentary switches, start in normal operation mode
        if (Config::getInstance().hardwareSwitchesPowerEnabled.get() &&
            static_cast<int>(Config::getInstance().hardwareSwitchesPowerType.get()) ==
                static_cast<int>(Hardware::SwitchType::MOMENTARY)) {
            systemContext_->machineStateContext()->setCurrentStateId(MachineStateId::PID_NORMAL);
            setUserPidEnabled(*systemContext_, true);
            LOG(INFO, "Machine initialized in PID Normal mode (momentary switch)");
        }
        // For toggle switches, force PidOn to switch state mode
        else if (Config::getInstance().hardwareSwitchesPowerEnabled.get() &&
                 static_cast<int>(Config::getInstance().hardwareSwitchesPowerType.get()) ==
                     static_cast<int>(Hardware::SwitchType::TOGGLE)) {
            if (systemContext_->hardwareContext().powerSwitch() &&
                systemContext_->hardwareContext().powerSwitch()->isPressed()) {
                setUserPidEnabled(*systemContext_, true);
                systemContext_->machineStateContext()->setCurrentStateId(MachineStateId::PID_NORMAL);
                LOG(INFO, "Machine initialized in PID Normal mode (toggle switch ON)");
            } else {
                setRuntimePidState(*systemContext_, false);
                systemContext_->machineStateContext()->setCurrentStateId(MachineStateId::PID_DISABLED);
                LOG(INFO, "Machine initialized in PID Disabled mode (toggle switch OFF)");
            }
        }
        // No power switch - use config PID setting
        else {
            const bool configPidEnabled = Config::getInstance().pidEnabled.get();
            setRuntimePidState(*systemContext_, configPidEnabled);
            systemContext_->machineStateContext()->setCurrentStateId(configPidEnabled ? MachineStateId::PID_NORMAL
                                                                                      : MachineStateId::PID_DISABLED);
            LOG(INFO,
                configPidEnabled ? "Machine initialized in PID Normal mode (config enabled)"
                                 : "Machine initialized in PID Disabled mode (config disabled)");
        }

        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "Machine state finalization failed: %s", e.what());
        return false;
    }
}

void SystemInitializer::calculateDerivedValues() {
    // Calculate derived PID values
    double aggKi = Config::getInstance().pidRegularTn.get() > 0
                       ? Config::getInstance().pidRegularKp.get() / Config::getInstance().pidRegularTn.get()
                       : 0;
    systemContext_->setProcessPidAggKi(aggKi);

    double aggKd = Config::getInstance().pidRegularTv.get() * Config::getInstance().pidRegularKp.get();
    systemContext_->setProcessPidAggKd(aggKd);

    double aggbKi = Config::getInstance().pidBdTn.get() > 0
                        ? Config::getInstance().pidBdKp.get() / Config::getInstance().pidBdTn.get()
                        : 0;
    // Note: aggbKi and aggbKd are mapped to aggKi/aggKd for now
    systemContext_->setProcessPidAggKi(aggbKi);

    double aggbKd = Config::getInstance().pidBdTv.get() * Config::getInstance().pidBdKp.get();
    systemContext_->setProcessPidAggKd(aggbKd);

    LOG(DEBUG, "Calculated derived PID values");
}

void SystemInitializer::setupTiming() {
    // Initialize timing variables (removed: previousMillistemp, windowStartTime, previousMillisMQTT are unused)
    unsigned long currentTime = millis();

    // Initialize network timing in coordinator
    if (systemContext_) {
        systemContext_->networkCoordinator().setLastMqttConnectionAttempt(currentTime);
        systemContext_->networkCoordinator().setLastWifiConnectionAttempt(currentTime);
    }

    LOG(DEBUG, "Timing variables initialized");
}

void SystemInitializer::registerMQTTParameters() {
    if (!mqttManager_) return;

    // Core parameters
    mqttManager_->registerParameter("pidON", "pid.enabled");
    mqttManager_->registerParameter("brewSetpoint", "brew.setpoint");
    mqttManager_->registerParameter("brewTempOffset", "brew.temp_offset");
    mqttManager_->registerParameter("steamON", "STEAM_MODE");
    mqttManager_->registerParameter("steamSetpoint", "steam.setpoint");
    mqttManager_->registerParameter("pidUsePonM", "pid.use_ponm");
    mqttManager_->registerParameter("aggKp", "pid.regular.kp");
    mqttManager_->registerParameter("aggTn", "pid.regular.tn");
    mqttManager_->registerParameter("aggTv", "pid.regular.tv");
    mqttManager_->registerParameter("aggIMax", "pid.regular.i_max");
    mqttManager_->registerParameter("steamKp", "pid.steam.kp");
    mqttManager_->registerParameter("standbyModeOn", "standby.enabled");

    // Brew-specific parameters
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        mqttManager_->registerParameter("aggbKp", "pid.bd.kp");
        mqttManager_->registerParameter("aggbTn", "pid.bd.tn");
        mqttManager_->registerParameter("aggbTv", "pid.bd.tv");
        mqttManager_->registerParameter("pidUseBD", "pid.bd.enabled");
        mqttManager_->registerParameter("brewPidDelay", "brew.pid_delay");
        mqttManager_->registerParameter("targetBrewTime", "brew.by_time.target_time");
        mqttManager_->registerParameter("preinfusion", "brew.pre_infusion.time");
        mqttManager_->registerParameter("preinfusionPause", "brew.pre_infusion.pause");
        mqttManager_->registerParameter("backflushOn", "BACKFLUSH_ON");
        mqttManager_->registerParameter("backflushCycles", "backflush.cycles");
        mqttManager_->registerParameter("backflushFillTime", "backflush.fill_time");
        mqttManager_->registerParameter("backflushFlushTime", "backflush.flush_time");
        mqttManager_->registerParameter("backflushReminderEnabled", "maintenance.backflush_reminder.enabled");
        mqttManager_->registerParameter("backflushReminderThreshold", "maintenance.backflush_reminder.threshold");
    }

    // Scale-specific parameters
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        mqttManager_->registerParameter("targetBrewWeight", "brew.by_weight.target_weight");
        mqttManager_->registerParameter("scaleCalibration", "hardware.sensors.scale.calibration");

        if (Config::getInstance().hardwareSensorsScaleType.get() == Hardware::ScaleType::HX711_DUAL) {
            mqttManager_->registerParameter("scale2Calibration", "hardware.sensors.scale.calibration2");
        }

        mqttManager_->registerParameter("scaleKnownWeight", "hardware.sensors.scale.known_weight");
        mqttManager_->registerParameter("scaleTareOn", "TARE_ON");
        mqttManager_->registerParameter("scaleCalibrationOn", "CALIBRATION_ON");
    }

    LOG(DEBUG, "MQTT parameters registered");
}

void SystemInitializer::registerMQTTSensors() {
    if (!mqttManager_) return;

    // Core sensors
    mqttManager_->registerSensor("temperature", [this] {
        if (systemContext_ && systemContext_->processController()) {
            return systemContext_->processController()->getCurrentTemperature();
        }
        return systemContext_->processTemperature();
    });
    mqttManager_->registerSensor("heaterPower", [this] {
        if (systemContext_ && systemContext_->processController()) {
            return systemContext_->processController()->getPIDOutput() / 10;
        }
        return systemContext_->processPidOutput() / 10;
    });
    mqttManager_->registerSensor("standbyModeTimeRemaining", [this] {
        return systemContext_->standbyCoordinator().getRemainingTimeMillis() / 1000.0;
    });
    mqttManager_->registerSensor("shotsSinceBackflush", [this] {
        return static_cast<double>(systemContext_->maintenanceCoordinator().getShotsSinceBackflush());
    });
    mqttManager_->registerSensor("backflushReminderDue", [this] {
        return systemContext_->maintenanceCoordinator().isReminderDue() ? 1.0 : 0.0;
    });
    mqttManager_->registerSensor("currentKp", [this] { return systemContext_->pidKp(); });
    mqttManager_->registerSensor("currentKi", [this] { return systemContext_->pidKi(); });
    mqttManager_->registerSensor("currentKd", [this] { return systemContext_->pidKd(); });
    // Machine state sensor registration - use lambda that captures systemContext
    mqttManager_->registerSensor("machineState", [this] {
        if (systemContext_ && systemContext_->machineStateContext()) {
            return static_cast<double>(systemContext_->machineStateContext()->getCurrentStateId());
        }
        return static_cast<double>(MachineStateId::INIT);
    });

    // Brew-specific sensors
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        mqttManager_->registerSensor("currBrewTime", [this] {
            if (systemContext_ && systemContext_->processController()) {
                return systemContext_->processController()->getCurrBrewTime() / 1000;
            }
            return systemContext_->processCurrentBrewTime() / 1000;
        });
    }

    // Scale-specific sensors
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        mqttManager_->registerSensor("currReadingWeight",
                                     [this] { return systemContext_->sensorCoordinator().getWeight(); });
        mqttManager_->registerSensor("currBrewWeight",
                                     [this] { return systemContext_->sensorCoordinator().getBrewWeight(); });
    }

    // Pressure sensor
    if (Config::getInstance().hardwareSensorsPressureEnabled.get()) {
        mqttManager_->registerSensor("pressure",
                                     [this] { return systemContext_->sensorCoordinator().getFilteredPressure(); });
    }

    LOG(DEBUG, "MQTT sensors registered");
}

CleverCoffeeWiFiManager& SystemInitializer::getWiFiManager() const {
    if (!cleverCoffeeWiFiManager_) {
        LOG(FATAL, "WiFiManager not initialized - system bug!");
    }
    return *cleverCoffeeWiFiManager_;
}

void SystemInitializer::setupWiFi() {
    // startConfigPortal blocks loopTask for up to 60s; suspend our TWDT subscription (disableLoopWDT
    // only removes the Arduino framework subscription, not esp_task_wdt_add from Watchdog::begin).
    struct WatchdogResumeGuard {
        Watchdog* wdt;
        explicit WatchdogResumeGuard(Watchdog* watchdog) : wdt(watchdog) {
            if (wdt) {
                wdt->suspend();
            }
        }
        ~WatchdogResumeGuard() {
            if (wdt) {
                wdt->resume();
            }
        }
        WatchdogResumeGuard(const WatchdogResumeGuard&)            = delete;
        WatchdogResumeGuard& operator=(const WatchdogResumeGuard&) = delete;
    } watchdogGuard(watchdog_);

    try {
        const bool oledEnabled = Config::getInstance().hardwareOledEnabled.get();

        // Create a display callback for WiFi status updates
        std::function<void(const char*, const char*)> displayCallback = nullptr;

        displayCallback = [this](const char* line1, const char* line2) {
            displayLogo(*systemContext_, line1, line2 ? line2 : "");
        };

        // Setup WiFi with display feedback
        if (!systemContext_->cleverCoffeeWiFiManager()->setupAndConnect(
                Config::getInstance().systemHostname.get(), WIFI_PASSWORD, false, displayCallback)) {
            systemContext_->networkCoordinator().setOfflineMode(true);
            displayLogo(*systemContext_, langstring_nowifi[0], langstring_nowifi[1]);
        } else {
            displayLogo(*systemContext_, "WiFi Connected", WiFi.localIP().toString().c_str());
        }

        // Check if restart is required after AP configuration
        if (systemContext_->cleverCoffeeWiFiManager()->requiresRestart()) {
            // Device will restart inside WiFiManager, this code may not be reached
        }

        LOG(INFO, "WiFi setup completed via WiFiManager");
    } catch (const std::exception& e) {
        LOG(ERROR, "Failed to initialize WiFiManager");
        systemContext_->networkCoordinator().setOfflineMode(true);
        displayLogo(*systemContext_, langstring_nowifi[0], langstring_nowifi[1]);
    }
}
