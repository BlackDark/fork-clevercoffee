/**
 * @file SystemInitializer.cpp
 * @brief Implementation of RAII wrapper for system initialization
 */

#include "SystemInitializer.h"
#include "../Config.h"
#include "../defaults.h"
#include "../display/DisplayManager.h"
// Forward declarations for display functions that require global state
// These will be called from main.cpp after full system initialization
extern void u8g2_prepare();
extern void initLangStrings(Config& config);
extern void displayLogo(const String& text1, const String& text2);
namespace DisplayTemplateManager {
    extern void initializeDisplay(int templateId);
}
#include "../hardware/HardwareManager.h"
#include "../network/CleverCoffeeWiFiManager.h"
#include "../network/MQTTManager.h"
#include "../sensors/SensorManager.h"
#include "Logger.h"
#include <Arduino.h>
#include <ArduinoOTA.h>
#include <LittleFS.h>
#include <PID_v1.h>
#include <WiFi.h>
#include <Wire.h>


// Forward declarations
extern void initTimer1();
extern void enableTimer1();
extern void u8g2_prepare();
extern void initLangStrings(Config& config);
extern void wiFiSetup();
extern void serverSetup();
extern void initScale();
extern void setRuntimePidState(bool state);
extern bool checkBrewActive();

SystemInitializer::SystemInitializer() :
    systemInitialized_(false), displayManager_(nullptr), hardwareManager_(nullptr), mqttManager_(nullptr), sensorManager_(nullptr) {
}

bool SystemInitializer::initialize() {
    LOG(INFO, "Starting system initialization");

    // Phase 1: Core system initialization
    if (!initializeLogger()) {
        LOG(ERROR, "Logger initialization failed");
        return false;
    }

    if (!initializeConfiguration()) {
        LOG(ERROR, "Configuration initialization failed");
        return false;
    }

    // Phase 2: Hardware initialization
    Wire.begin();

    if (!initializeDisplay()) {
        LOG(WARNING, "Display initialization failed, continuing without display");
    }

    initTimer1();

    if (!initializeHardware()) {
        LOG(ERROR, "Hardware initialization failed");
        return false;
    }

    // Phase 3: Network and services
    if (!initializeNetworking()) {
        LOG(WARNING, "Network initialization failed, continuing in offline mode");
        g_state.network.offlineMode = true;
    }

    if (!initializeMQTT()) {
        LOG(WARNING, "MQTT initialization failed, continuing without MQTT");
    }

    // Phase 4: PID and sensors
    if (!initializePID()) {
        LOG(ERROR, "PID initialization failed");
        return false;
    }

    if (!initializeSensors()) {
        LOG(WARNING, "Sensor initialization incomplete");
    }

    // Phase 5: Finalization
    setupTiming();
    enableTimer1();

    double fsUsage = (static_cast<double>(LittleFS.usedBytes()) / LittleFS.totalBytes()) * 100;
    LOGF(INFO, "LittleFS: %d%% (used %ld bytes from %ld bytes)", static_cast<int>(ceil(fsUsage)), LittleFS.usedBytes(), LittleFS.totalBytes());

    if (!finalizeMachineState()) {
        LOG(ERROR, "Machine state finalization failed");
        return false;
    }

    g_state.coordination.setupDone = true;
    systemInitialized_ = true;

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
    int level = Config::getInstance().get<int>("system.log_level");
    Logger::setLevel(static_cast<Logger::Level>(level));

    calculateDerivedValues();

    // TODO what is better?
    // bPID = std::make_unique<PID>(&g_state.process.temperature, &g_state.process.pidOutput, &g_state.process.setpoint, Config::getInstance().get<double>("pid.regular.kp"), aggKi, aggKd, 1, DIRECT);
    g_state.pid = new PID(&g_state.process.temperature, &g_state.process.pidOutput, &g_state.process.setpoint, Config::getInstance().get<double>("pid.regular.kp"), g_state.process.aggKi, g_state.process.aggKd, 1, DIRECT);
    // g_state.pid = PID(&g_state.process.temperature, &g_state.process.pidOutput, &g_state.process.setpoint, Config::getInstance().get<double>("pid.regular.kp"), aggKi, aggKd, 1, DIRECT)*;
    return true;
}

bool SystemInitializer::initializeDisplay() {
    if (!Config::getInstance().get<bool>("hardware.oled.enabled")) {
        LOG(INFO, "Display disabled in configuration");
        return true;
    }

    try {
        const int displayType = Config::getInstance().get<int>("hardware.oled.type");
        const int displayAddress = Config::getInstance().get<int>("hardware.oled.address");

        displayManager_ = std::make_unique<DisplayManager>(displayType, displayAddress);

        if (displayManager_ && displayManager_->isInitialized()) {
            // Set compatibility pointer for existing code
            g_state.hardware.display = displayManager_->get();

            // Basic display setup - full initialization will be done in main.cpp
            // The display is now ready for basic operations but NOT for complex display functions
            // that depend on global state (like language strings, templates, etc.)

            LOG(INFO, "Display initialization completed");
            return true;
        }
        else {
            LOG(ERROR, "Failed to create DisplayManager");
            displayManager_.reset();
            g_state.hardware.display = nullptr;
            Config::getInstance().set<bool>("hardware.oled.enabled", false);
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
        hardwareManager_ = std::make_unique<HardwareManager>();

        // Update compatibility pointers to reference HardwareManager components
        g_state.hardware.heaterRelay = &hardwareManager_->getHeaterRelay();
        g_state.hardware.pumpRelay = &hardwareManager_->getPumpRelay();
        g_state.hardware.valveRelay = &hardwareManager_->getValveRelay();

        g_state.hardware.statusLed = hardwareManager_->getStatusLed();
        g_state.hardware.brewLed = hardwareManager_->getBrewLed();
        g_state.hardware.steamLed = hardwareManager_->getSteamLed();

        g_state.hardware.powerSwitch = hardwareManager_->getPowerSwitch();
        g_state.hardware.brewSwitch = hardwareManager_->getBrewSwitch();
        g_state.hardware.hotWaterSwitch = hardwareManager_->getSteamSwitch();
        g_state.hardware.powerSwitch = hardwareManager_->getHotWaterSwitch();
        g_state.hardware.waterTankSensor = hardwareManager_->getWaterTankSensor();

        g_state.hardware.tempSensor = hardwareManager_->getTempSensor();

        LOG(INFO, "Hardware initialization completed via HardwareManager");
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "Failed to initialize HardwareManager: %s", e.what());
        return false;
    }
}

bool SystemInitializer::initializeNetworking() {
    if (Config::getInstance().get<bool>("system.offline_mode")) {
        LOG(INFO, "Offline mode enabled, skipping network initialization");
        WiFi.disconnect();
        g_state.network.offlineMode = true;
        setRuntimePidState(true);
        return true;
    }

    try {
        cleverCoffeeWiFiManager_ = std::make_unique<CleverCoffeeWiFiManager>();
        g_state.network.cleverCoffeeWiFiManager = cleverCoffeeWiFiManager_.get();

        wiFiSetup();
        serverSetup();

        // OTA Updates
        if (WiFi.status() == WL_CONNECTED) {
            String otaPass = Config::getInstance().get<String>("system.ota_password");
            ArduinoOTA.setHostname(Config::getInstance().get<String>("system.hostname").c_str());
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
    if (g_state.network.offlineMode || !Config::getInstance().get<bool>("mqtt.enabled")) {
        LOG(INFO, "MQTT disabled, skipping MQTT initialization");
        return true;
    }

    try {
        mqttManager_ = std::make_unique<MQTTManager>();

        if (mqttManager_->setup(Config::getInstance().get<String>("system.hostname"))) {
            // Set compatibility variables
            // mqtt_enabled = mqttManager_->isEnabled();
            Config::getInstance().set<bool>("mqtt.enabled", mqttManager_->isEnabled());
            // mqtt_hassio_enabled = true;
            //  TODO check if this is right
            Config::getInstance().set<bool>("mqtt.hassio.enabled", true);

            // Set global reference for other parts of the system
            //mqttManager = std::move(mqttManager_);
            //mqttManager_ = nullptr; // Transfer ownership

            registerMQTTParameters();
            registerMQTTSensors();

            mqttManager_->checkConnection();
            mqttManager_->sendHASSIODiscoveryMsg();

            LOG(INFO, "MQTT setup completed via MQTTManager");
            return true;
        }
        else {
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
        // Initialize PID controller
        g_state.pid->SetSampleTime(g_state.process.windowSize);
        g_state.pid->SetOutputLimits(0, g_state.process.windowSize);
        g_state.pid->SetIntegratorLimits(0, 55.0); // AGGIMAX constant
        g_state.pid->SetSmoothingFactor(Config::getInstance().get<double>("pid.ema_factor"));
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
        TempSensor* tempSensorRef = hardwareManager_ ? hardwareManager_->getTempSensor() : nullptr;
        Switch* waterTankSensorRef = hardwareManager_ ? hardwareManager_->getWaterTankSensor() : nullptr;

        if (sensorManager_->initialize(tempSensorRef, waterTankSensorRef)) {
            // Update global temperature variable for compatibility
            g_state.process.temperature = sensorManager_->getCurrentTemperature();

            LOG(INFO, "Sensor management initialized via SensorManager");

            // Note: Scale initialization is still handled separately in main.cpp
            // because it requires complex global dependencies
            if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
                LOG(INFO, "Scale sensor will be initialized separately in main.cpp");
            }

            return true;
        }
        else {
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
        if (Config::getInstance().get<bool>("hardware.switches.power.enabled") && Config::getInstance().get<int>("hardware.switches.power.type") == static_cast<int>(Switch::MOMENTARY)) {
            g_state.machine.machineState = kPidNormal;
            setRuntimePidState(true);
            LOG(INFO, "Machine initialized in PID Normal mode (momentary switch)");
        }
        // For toggle switches, force PidOn to switch state mode
        else if (Config::getInstance().get<bool>("hardware.switches.power.enabled") && Config::getInstance().get<int>("hardware.switches.power.type") == static_cast<int>(Switch::TOGGLE)) {
            if (g_state.hardware.powerSwitch && g_state.hardware.powerSwitch->isPressed()) {
                setRuntimePidState(true);
                g_state.machine.machineState = kPidNormal;
                LOG(INFO, "Machine initialized in PID Normal mode (toggle switch ON)");
            }
            else {
                setRuntimePidState(false);
                g_state.machine.machineState = kPidDisabled;
                LOG(INFO, "Machine initialized in PID Disabled mode (toggle switch OFF)");
            }
        }

        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "Machine state finalization failed: %s", e.what());
        return false;
    }
}

void SystemInitializer::calculateDerivedValues() {
    // Calculate derived PID values
    g_state.process.aggKi = Config::getInstance().get<double>("pid.regular.tn") > 0 ? Config::getInstance().get<double>("pid.regular.kp") / Config::getInstance().get<double>("pid.regular.tn") : 0;
    g_state.process.aggKd = Config::getInstance().get<double>("pid.regular.tv") * Config::getInstance().get<double>("pid.regular.kp");
    g_state.process.aggbKi = Config::getInstance().get<double>("pid.bd.tn") > 0 ? Config::getInstance().get<double>("pid.bd.kp") / Config::getInstance().get<double>("pid.bd.tn") : 0;
    g_state.process.aggbKd = Config::getInstance().get<double>("pid.bd.tv") * Config::getInstance().get<double>("pid.bd.kp");

    LOG(DEBUG, "Calculated derived PID values");
}

void SystemInitializer::setupTiming() {
    // Initialize timing variables
    unsigned long currentTime = millis();
    g_state.timing.previousMillistemp = currentTime;
    g_state.timing.windowStartTime = currentTime;
    g_state.timing.previousMillisMQTT = currentTime;
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
    if (Config::getInstance().get<bool>("hardware.switches.brew.enabled")) {
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
    if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
        mqttManager_->registerParameter("targetBrewWeight", "brew.by_weight.target_weight");
        mqttManager_->registerParameter("scaleCalibration", "hardware.sensors.scale.calibration");

        if (Config::getInstance().get<int>("hardware.sensors.scale.type") == 0) {
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
    mqttManager_->registerSensor("standbyModeTimeRemaining", [] { return g_state.standby.standbyModeRemainingTimeMillis / 1000; });
    mqttManager_->registerSensor("currentKp", [] { return g_state.pid->GetKp(); });
    mqttManager_->registerSensor("currentKi", [] { return g_state.pid->GetKi(); });
    mqttManager_->registerSensor("currentKd", [] { return g_state.pid->GetKd(); });
    mqttManager_->registerSensor("machineState", [] { return static_cast<double>(g_state.machine.machineState); });

    // Brew-specific sensors
    if (Config::getInstance().get<bool>("hardware.switches.brew.enabled")) {
        mqttManager_->registerSensor("currBrewTime", [] { return g_state.process.currBrewTime / 1000; });
    }

    // Scale-specific sensors
    if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
        mqttManager_->registerSensor("currReadingWeight", [] { return g_state.sensors.currReadingWeight; });
        mqttManager_->registerSensor("currBrewWeight", [] { return g_state.sensors.currBrewWeight; });
    }

    // Pressure sensor
    if (Config::getInstance().get<bool>("hardware.sensors.pressure.enabled")) {
        mqttManager_->registerSensor("pressure", [] { return g_state.sensors.inputPressureFilter; });
    }

    LOG(DEBUG, "MQTT sensors registered");
}

CleverCoffeeWiFiManager* SystemInitializer::getWiFiManager() const {
    return cleverCoffeeWiFiManager_.get();
}
