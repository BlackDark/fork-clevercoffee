/**
 * @file SystemInitializer.cpp
 * @brief Implementation of RAII wrapper for system initialization
 */

#include "clevercoffee/hardware/HardwareManager.h"  // Include before own header to resolve forward declaration
#include "clevercoffee/core/SystemInitializer.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/display/DisplayManager.h"
#include "clevercoffee/display/displayTemplateManager.h"
#include "clevercoffee/display/languages.h"
#include "clevercoffee/network/CleverCoffeeWiFiManager.h"
#include "clevercoffee/network/MQTTManager.h"
#include "clevercoffee/network/WebServerManager.h"
#include "clevercoffee/sensors/SensorManager.h"
#include "clevercoffee/ui/UIManager.h"
#include "clevercoffee/utils/SystemUtils.h"
#include "clevercoffee/utils/memoryUtils.h"
#include "clevercoffee/context/SystemContext.h"

#include <Arduino.h>
#include <ArduinoOTA.h>
#include <LittleFS.h>
#include <PID_v1.h>
#include <WiFi.h>
#include <Wire.h>

// Forward declarations
extern void initTimer1();
extern void enableTimer1();

// namespace DisplayTemplateManager {
//     extern void initializeDisplay(int templateId);
// }

#include "clevercoffee/scaleHandler.h"
// checkBrewActive removed - now accessed via g_state.handlers.brewHandler

SystemInitializer::SystemInitializer()
    : systemInitialized_(false), hostname_(), displayManager_(nullptr), uiManager_(nullptr), hardwareManager_(nullptr),
      mqttManager_(nullptr), sensorManager_(nullptr), cleverCoffeeWiFiManager_(nullptr), webServerManager_(nullptr) {}

SystemInitializer::~SystemInitializer() {
    // Destructor implementation - unique_ptr will automatically clean up resources
    // This needs to be defined in the .cpp file where complete type definitions are available
}

bool SystemInitializer::initialize() {
    LOG(INFO, "Starting system initialization");

    // Create SystemContext first
    systemContext_ = std::make_unique<CleverCoffee::SystemContext>();

    logMemory("SystemInitializer Start");

    // Phase 1: Core system initialization
    logMemoryBasic("Before Logger Init");
    if (!initializeLogger()) {
        LOG(ERROR, "Logger initialization failed");
        return false;
    }

    logMemoryBasic("Before Config Init");
    if (!initializeConfiguration()) {
        LOG(ERROR, "Configuration initialization failed");
        return false;
    }
    logMemoryBasic("After Config Init");

    // Phase 2: Hardware initialization
    logMemoryBasic("Before Wire.begin()");
    Wire.begin();
    logMemoryBasic("After Wire.begin()");

    logMemoryBasic("Before Display Init");
    if (!initializeDisplay()) {
        LOG(WARNING, "Display initialization failed, continuing without display");
    } else {
        uiManager_->displayLogo("Version ", g_state.sysVersion);
    }

    logMemoryBasic("After Display Init");

    logMemoryBasic("Before Timer1 Init");
    initTimer1();
    logMemoryBasic("After Timer1 Init");

    logMemoryBasic("Before Hardware Init");
    if (!initializeHardware()) {
        LOG(ERROR, "Hardware initialization failed");
        logMemory("Hardware Init FAILED");
        uiManager_->displayLogo(String("Error "), "Hardware initialization failed");
        return false;
    }

    logMemoryBasic("After Hardware Init");

    // Phase 3: Network and services
    LOG(INFO, "Starting Phase 3: Network and services");
    if (!initializeNetworking()) {
        LOG(WARNING, "Network initialization failed, continuing in offline mode");
        g_state.network.offlineMode = true;
    }

    LOG(INFO, "Starting MQTT initialization");
    if (!initializeMQTT()) {
        LOG(WARNING, "MQTT initialization failed, continuing without MQTT");
    }

    // Phase 4: PID and sensors
    LOG(INFO, "Starting Phase 4: PID and sensors");
    if (!initializePID()) {
        LOG(ERROR, "PID initialization failed");
        return false;
    }

    LOG(INFO, "Starting sensor initialization");
    if (!initializeSensors()) {
        LOG(WARNING, "Sensor initialization incomplete");
    }

    // Phase 5: Finalization
    LOG(INFO, "Starting Phase 5: Finalization");
    setupTiming();
    enableTimer1();

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

    if (!finalizeMachineState()) {
        LOG(ERROR, "Machine state finalization failed");
        return false;
    }

    g_state.coordination.setupDone = true;
    systemInitialized_             = true;

    // System initialization complete

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
    Logger::begin();

    return true;
}

bool SystemInitializer::initializeConfiguration() {
    if (!Config::getInstance().begin()) {
        LOG(ERROR, "Failed to initialize configuration system!");
        Serial.println("Critical error detected!");
        Serial.flush();
        return false;
    }

    LOG(INFO, "Configuration system ready");

    // Use default log level
    Logger::setLevel(Logger::Level::INFO);

    calculateDerivedValues();

    // Use make_unique for proper RAII and exception safety
    pidController_ = std::make_unique<PID>(&g_state.process.temperature,
                                           &g_state.process.pidOutput,
                                           &g_state.process.setpoint,
                                           Config::getInstance().pidRegularKp.get(),
                                           g_state.process.aggKi,
                                           g_state.process.aggKd,
                                           1,
                                           DIRECT);

    // Set global reference for backward compatibility
    g_state.pid = pidController_.get();
    return true;
}

bool SystemInitializer::initializeDisplay() {
    if (!Config::getInstance().hardwareOledEnabled.get()) {
        LOG(INFO, "Display disabled in configuration");
        return true;
    }

    try {
        const Hardware::OLEDType    displayType    = Config::getInstance().hardwareOledType.get();
        const Hardware::OLEDAddress displayAddress = Config::getInstance().hardwareOledAddress.get();

        displayManager_ = std::make_unique<DisplayManager>(displayType, displayAddress);

        if (displayManager_ && displayManager_->isInitialized()) {
            // Set compatibility pointer for existing code
            g_state.hardware.display = displayManager_->getDisplay();

            // Basic display setup - full initialization will be done in main.cpp
            // The display is now ready for basic operations but NOT for complex display functions
            // that depend on global state (like language strings, templates, etc.)

            // TODO maybe separated
            uiManager_ = std::make_unique<UIManager>(displayManager_.get());

            if (uiManager_->initialize()) {
                LOG(INFO, "UIManager initialized successfully");
            } else {
                LOG(ERROR, "UIManager initialization failed!");
            }

            const System::DisplayTemplate templateId = Config::getInstance().displayTemplate.get();
            DisplayTemplateManager::initializeDisplay(templateId);
            LOG(INFO, "Display initialization completed");
            return true;
        } else {
            LOG(ERROR, "Failed to create DisplayManager");
            displayManager_.reset();
            g_state.hardware.display = nullptr;
            // TODO probably wrong
            Config::getInstance().hardwareOledEnabled.set(false);
            return false;
        }
    } catch (const std::exception& e) {
        LOGF(ERROR, "Exception during display initialization: %s", e.what());
        displayManager_.reset();
        g_state.hardware.display = nullptr;
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
        g_state.hardware.heaterRelay = hardwareManager_->getHeaterRelay();
        g_state.hardware.pumpRelay   = hardwareManager_->getPumpRelay();
        g_state.hardware.valveRelay  = hardwareManager_->getValveRelay();

        g_state.hardware.statusLed = hardwareManager_->getStatusLed();
        g_state.hardware.brewLed   = hardwareManager_->getBrewLed();
        g_state.hardware.steamLed  = hardwareManager_->getSteamLed();

        g_state.hardware.powerSwitch     = hardwareManager_->getPowerSwitch();
        g_state.hardware.brewSwitch      = hardwareManager_->getBrewSwitch();
        g_state.hardware.hotWaterSwitch  = hardwareManager_->getHotWaterSwitch();
        g_state.hardware.steamSwitch     = hardwareManager_->getSteamSwitch();
        g_state.hardware.waterTankSensor = hardwareManager_->getWaterTankSensor();

        g_state.hardware.tempSensor = hardwareManager_->getTempSensor();
        logMemoryBasic("After Hardware Pointer Updates");

        LOG(INFO, "Hardware initialization completed via HardwareManager");
        return true;
    } catch (const std::exception& e) {
        logMemory("HardwareManager Exception");
        LOGF(ERROR, "Failed to initialize HardwareManager: %s", e.what());
        return false;
    }
}

bool SystemInitializer::initializeNetworking() {
    if (Config::getInstance().systemOfflineMode.get()) {
        LOG(INFO, "Offline mode enabled, skipping network initialization");
        WiFi.disconnect();
        g_state.network.offlineMode = true;
        setRuntimePidState(true);
        return true;
    }

    try {
        cleverCoffeeWiFiManager_                = std::make_unique<CleverCoffeeWiFiManager>();
        g_state.network.cleverCoffeeWiFiManager = cleverCoffeeWiFiManager_.get();

        setupWiFi();

        LOG(INFO, "About to initialize WebServerManager");

        // Initialize LittleFS first - this was causing the hang
        if (!LittleFS.begin()) {
            LOG(WARNING, "LittleFS initialization failed, web server will run without file system");
        } else {
            LOG(INFO, "LittleFS initialized successfully");
        }

        // Initialize WebServerManager
        webServerManager_                = std::make_unique<WebServerManager>(80);
        g_state.network.webServerManager = webServerManager_.get();

        if (!webServerManager_->initialize(true)) {
            LOG(ERROR, "WebServerManager initialization failed");
            webServerManager_.reset();
        } else {
            LOG(INFO, "WebServerManager initialized successfully");
        }

        // OTA Updates
        if (WiFi.status() == WL_CONNECTED) {
            String otaPass = Config::getInstance().systemOtaPassword.get();
            ArduinoOTA.setHostname(Config::getInstance().systemHostname.get().c_str());
            ArduinoOTA.setPassword(otaPass.c_str());
            ArduinoOTA.begin();
            LOG(INFO, "OTA initialized");
        }

        LOG(INFO, "Network initialization completed");
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "Network initialization failed: %s", e.what());
        return false;
    }
}

bool SystemInitializer::initializeMQTT() {
    if (g_state.network.offlineMode || !Config::getInstance().mqttEnabled.get()) {
        LOG(INFO, "MQTT disabled, skipping MQTT initialization");
        return true;
    }

    try {
        mqttManager_ = std::make_unique<MQTTManager>();

        if (mqttManager_->setup(Config::getInstance().systemHostname.get())) {
            // Set compatibility variables
            // mqtt_enabled = mqttManager_->isEnabled();
            Config::getInstance().mqttEnabled.set(mqttManager_->isEnabled());
            // mqtt_hassio_enabled = true;
            //  TODO check if this is right
            Config::getInstance().mqttHassioEnabled.set(true);

            // Set global reference for other parts of the system
            // mqttManager = std::move(mqttManager_);
            // mqttManager_ = nullptr; // Transfer ownership

            registerMQTTParameters();
            registerMQTTSensors();

            mqttManager_->checkConnection();
            mqttManager_->sendHASSIODiscoveryMsg();

            LOG(INFO, "MQTT setup completed via MQTTManager");
            return true;
        } else {
            LOG(WARNING, "MQTT setup returned false");
            return false;
        }
    } catch (const std::exception& e) {
        LOGF(ERROR, "Failed to initialize MQTTManager: %s", e.what());
        return false;
    }
}

bool SystemInitializer::initializePID() {
    try {
        LOGF(INFO,
             "PID initialized: Kp={:.3f}, Ki={:.3f}, Kd={:.3f}",
             Config::getInstance().pidRegularKp.get(),
             g_state.process.aggKi,
             g_state.process.aggKd);

        // Set PID tunings now that parameters are calculated
        g_state.pid->SetTunings(
            Config::getInstance().pidRegularKp.get(), g_state.process.aggKi, g_state.process.aggKd, 1);

        // Initialize PID controller
        g_state.pid->SetSampleTime(g_state.process.windowSize);
        g_state.pid->SetOutputLimits(0, g_state.process.windowSize);
        g_state.pid->SetIntegratorLimits(0, 55.0); // AGGIMAX constant
        g_state.pid->SetSmoothingFactor(Config::getInstance().pidEmaFactor.get());
        g_state.pid->SetMode(AUTOMATIC);

        LOG(INFO, "PID controller initialized");
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "PID initialization failed: %s", e.what());
        return false;
    }
}

bool SystemInitializer::initializeSensors() {
    try {
        // Create and initialize SensorManager
        sensorManager_ = std::make_unique<SensorManager>();

        // Get sensor references from HardwareManager
        TempSensor* tempSensorRef      = hardwareManager_ ? hardwareManager_->getTempSensor() : nullptr;
        Switch*     waterTankSensorRef = hardwareManager_ ? hardwareManager_->getWaterTankSensor() : nullptr;

        // Get sensor coordinator from system context
        CleverCoffee::SensorCoordinator* coord = systemContext_ ? &systemContext_->sensorCoordinator() : nullptr;

        if (sensorManager_->initialize(tempSensorRef, waterTankSensorRef, coord)) {
            // Update global temperature variable for compatibility
            g_state.process.temperature = sensorManager_->getCurrentTemperature();

            LOG(INFO, "Sensor management initialized via SensorManager");

            // Note: Scale initialization is still handled separately in main.cpp
            // because it requires complex global dependencies
            if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
                LOG(INFO, "Scale sensor will be initialized separately in main.cpp");
            }

            return true;
        } else {
            LOG(WARNING, "SensorManager initialization returned false");
            return false;
        }
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
            g_state.machine.machineState = MachineStateId::PID_NORMAL;
            setRuntimePidState(true);
            LOG(INFO, "Machine initialized in PID Normal mode (momentary switch)");
        }
        // For toggle switches, force PidOn to switch state mode
        else if (Config::getInstance().hardwareSwitchesPowerEnabled.get() &&
                 static_cast<int>(Config::getInstance().hardwareSwitchesPowerType.get()) ==
                     static_cast<int>(Hardware::SwitchType::TOGGLE)) {
            if (g_state.hardware.powerSwitch && g_state.hardware.powerSwitch->isPressed()) {
                setRuntimePidState(true);
                g_state.machine.machineState = MachineStateId::PID_NORMAL;
                LOG(INFO, "Machine initialized in PID Normal mode (toggle switch ON)");
            } else {
                setRuntimePidState(false);
                g_state.machine.machineState = MachineStateId::PID_DISABLED;
                LOG(INFO, "Machine initialized in PID Disabled mode (toggle switch OFF)");
            }
        }
        // No power switch - use config PID setting
        else {
            const bool configPidEnabled = Config::getInstance().pidEnabled.get();
            setRuntimePidState(configPidEnabled);
            g_state.machine.machineState = configPidEnabled ? MachineStateId::PID_NORMAL : MachineStateId::PID_DISABLED;
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
    g_state.process.aggKi  = Config::getInstance().pidRegularTn.get() > 0
                                 ? Config::getInstance().pidRegularKp.get() / Config::getInstance().pidRegularTn.get()
                                 : 0;
    g_state.process.aggKd  = Config::getInstance().pidRegularTv.get() * Config::getInstance().pidRegularKp.get();
    g_state.process.aggbKi = Config::getInstance().pidBdTn.get() > 0
                                 ? Config::getInstance().pidBdKp.get() / Config::getInstance().pidBdTn.get()
                                 : 0;
    g_state.process.aggbKd = Config::getInstance().pidBdTv.get() * Config::getInstance().pidBdKp.get();

    LOG(DEBUG, "Calculated derived PID values");
}

void SystemInitializer::setupTiming() {
    // Initialize timing variables
    unsigned long currentTime                 = millis();
    g_state.timing.previousMillistemp         = currentTime;
    g_state.timing.windowStartTime            = currentTime;
    g_state.timing.previousMillisMQTT         = currentTime;
    g_state.network.lastMQTTConnectionAttempt = currentTime;

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
    mqttManager_->registerSensor("temperature", [] { return g_state.process.temperature; });
    mqttManager_->registerSensor("heaterPower", [] { return g_state.process.pidOutput / 10; });
    mqttManager_->registerSensor("standbyModeTimeRemaining",
                                 [] { return g_state.standby.standbyModeRemainingTimeMillis / 1000; });
    mqttManager_->registerSensor("currentKp", [] { return g_state.pid->GetKp(); });
    mqttManager_->registerSensor("currentKi", [] { return g_state.pid->GetKi(); });
    mqttManager_->registerSensor("currentKd", [] { return g_state.pid->GetKd(); });
    mqttManager_->registerSensor("machineState", [] { return static_cast<double>(g_state.machine.machineState); });

    // Brew-specific sensors
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        mqttManager_->registerSensor("currBrewTime", [] { return g_state.process.currBrewTime / 1000; });
    }

    // Scale-specific sensors
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        mqttManager_->registerSensor("currReadingWeight", [] { return g_state.sensors.currReadingWeight; });
        mqttManager_->registerSensor("currBrewWeight", [] { return g_state.sensors.currBrewWeight; });
    }

    // Pressure sensor
    if (Config::getInstance().hardwareSensorsPressureEnabled.get()) {
        mqttManager_->registerSensor("pressure", [] { return g_state.sensors.inputPressureFilter; });
    }

    LOG(DEBUG, "MQTT sensors registered");
}

CleverCoffeeWiFiManager* SystemInitializer::getWiFiManager() const {
    return cleverCoffeeWiFiManager_.get();
}

void SystemInitializer::setupWiFi() {
    try {
        const bool oledEnabled = Config::getInstance().hardwareOledEnabled.get();

        // Create a display callback for WiFi status updates
        std::function<void(const char*, const char*)> displayCallback = nullptr;

        displayCallback = [this](const char* line1, const char* line2) {
            uiManager_->displayLogo(String(line1), line2 ? String(line2) : String(""));
        };

        // Setup WiFi with display feedback
        if (!g_state.network.cleverCoffeeWiFiManager->setupAndConnect(
                Config::getInstance().systemHostname.get(), WIFI_PASSWORD, false, displayCallback)) {
            g_state.network.offlineMode = true;
            uiManager_->displayLogo(langstring_nowifi[0], langstring_nowifi[1]);
        } else {
            uiManager_->displayLogo("WiFi Connected", WiFi.localIP().toString());
        }

        // Check if restart is required after AP configuration
        if (g_state.network.cleverCoffeeWiFiManager->requiresRestart()) {
            // Device will restart inside WiFiManager, this code may not be reached
        }

        LOG(INFO, "WiFi setup completed via WiFiManager");
    } catch (const std::exception& e) {
        LOG(ERROR, "Failed to initialize WiFiManager");
        g_state.network.offlineMode = true;
        uiManager_->displayLogo(langstring_nowifi[0], langstring_nowifi[1]);
    }
}
