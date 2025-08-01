/**
 * @file SystemInitializer.cpp
 * @brief Implementation of RAII wrapper for system initialization
 */

#include "SystemInitializer.h"
#include "../Config.h"
#include "../GlobalVariables.h"
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
#include "../network/MQTTManager.h"
#include "../network/WiFiManager.h"
#include "../sensors/SensorManager.h"
#include "Logger.h"
#include <Arduino.h>
#include <ArduinoOTA.h>
#include <LittleFS.h>
#include <PID_v1.h>
#include <WiFi.h>
#include <Wire.h>

// Machine state constants
enum LegacyMachineState {
    kInit = 0,
    kPidNormal = 20,
    kBrew = 30,
    kManualFlush = 35,
    kHotWater = 40,
    kSteam = 50,
    kBackflush = 60,
    kWaterTankEmpty = 70,
    kEmergencyStop = 80,
    kPidDisabled = 90,
    kStandby = 95,
    kSensorError = 100,
    kEepromError = 110
};

// External dependencies
extern String hostname;
extern bool offlineMode;
extern bool setupDone;
extern double aggKi, aggKd, aggbKi, aggbKd;
extern double aggKp, aggTn, aggTv, aggbKp, aggbTn, aggbTv;
extern double brewTempOffset;
extern double temperature;
extern double pidOutput;
extern double currBrewTime;
extern double currBrewWeight;
extern double currReadingWeight;
extern double inputPressureFilter;
extern double standbyModeRemainingTimeMillis;
extern unsigned long previousMillistemp;
extern unsigned long windowStartTime;
extern unsigned long previousMillisMQTT;
extern unsigned long lastMQTTConnectionAttempt;
extern unsigned long previousMillisPressure;
extern int windowSize;
extern int machineState;
extern bool mqtt_enabled;
extern bool mqtt_hassio_enabled;
extern PubSubClient* mqtt;
extern PID bPID;
extern double emaFactor;
extern String otaPass;
extern const char sysVersion[64];

// Hardware compatibility pointers
extern Switch* powerSwitch;
extern Switch* brewSwitch;
extern Switch* steamSwitch;
extern Switch* hotWaterSwitch;
extern Switch* waterTankSensor;
extern LED* statusLed;
extern LED* brewLed;
extern LED* steamLed;
extern Relay* heaterRelay;
extern Relay* pumpRelay;
extern Relay* valveRelay;
extern TempSensor* tempSensor;
extern U8G2* u8g2;

// Manager instances
extern std::unique_ptr<MQTTManager> mqttManager;
extern std::unique_ptr<CleverCoffeeWiFiManager> wifiManager;

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
    systemInitialized_(false),
    displayManager_(nullptr),
    hardwareManager_(nullptr),
    mqttManager_(nullptr),
    sensorManager_(nullptr) {
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

    calculateDerivedValues();
    initTimer1();

    if (!initializeHardware()) {
        LOG(ERROR, "Hardware initialization failed");
        return false;
    }

    // Phase 3: Network and services
    if (!initializeNetworking()) {
        LOG(WARNING, "Network initialization failed, continuing in offline mode");
        offlineMode = true;
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
    LOGF(INFO, "LittleFS: %d%% (used %ld bytes from %ld bytes)", 
         static_cast<int>(ceil(fsUsage)), LittleFS.usedBytes(), LittleFS.totalBytes());

    if (!finalizeMachineState()) {
        LOG(ERROR, "Machine state finalization failed");
        return false;  
    }

    setupDone = true;
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
    
    hostname = Config::getInstance().get<String>("system.hostname");
    
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
            u8g2 = displayManager_->get();

            // Basic display setup - full initialization will be done in main.cpp
            // The display is now ready for basic operations but NOT for complex display functions
            // that depend on global state (like language strings, templates, etc.)
            
            LOG(INFO, "Display initialization completed");
            return true;
        } else {
            LOG(ERROR, "Failed to create DisplayManager");
            displayManager_.reset();
            u8g2 = nullptr;
            Config::getInstance().set<bool>("hardware.oled.enabled", false);
            return false;
        }
    } catch (const std::exception& e) {
        LOGF(ERROR, "Exception during display initialization: %s", e.what());
        displayManager_.reset();
        u8g2 = nullptr;
        return false;
    }
}

bool SystemInitializer::initializeHardware() {
    try {
        hardwareManager_ = std::make_unique<HardwareManager>();

        // Update compatibility pointers to reference HardwareManager components
        heaterRelay = &hardwareManager_->getHeaterRelay();
        pumpRelay = &hardwareManager_->getPumpRelay();
        valveRelay = &hardwareManager_->getValveRelay();

        statusLed = hardwareManager_->getStatusLed();
        brewLed = hardwareManager_->getBrewLed();
        steamLed = hardwareManager_->getSteamLed();

        powerSwitch = hardwareManager_->getPowerSwitch();
        brewSwitch = hardwareManager_->getBrewSwitch();
        steamSwitch = hardwareManager_->getSteamSwitch();
        hotWaterSwitch = hardwareManager_->getHotWaterSwitch();
        waterTankSensor = hardwareManager_->getWaterTankSensor();

        tempSensor = hardwareManager_->getTempSensor();

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
        offlineMode = true;
        setRuntimePidState(true);
        return true;
    }

    try {
        wiFiSetup();
        serverSetup();

        // OTA Updates
        if (WiFi.status() == WL_CONNECTED) {
            otaPass = Config::getInstance().get<String>("system.ota_password");
            ArduinoOTA.setHostname(hostname.c_str());
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
    if (offlineMode || !Config::getInstance().get<bool>("mqtt.enabled")) {
        LOG(INFO, "MQTT disabled, skipping MQTT initialization");
        return true;
    }

    try {
        mqttManager_ = std::make_unique<MQTTManager>();

        if (mqttManager_->setup(hostname)) {
            // Set compatibility variables
            mqtt_enabled = mqttManager_->isEnabled();
            mqtt_hassio_enabled = true;
            mqtt = &mqttManager_->getClient();
            
            // Set global reference for other parts of the system
            mqttManager = std::move(mqttManager_);
            mqttManager_ = nullptr; // Transfer ownership
            
            registerMQTTParameters();
            registerMQTTSensors();

            mqttManager->checkConnection();
            mqttManager->sendHASSIODiscoveryMsg();

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
        // Initialize PID controller
        bPID.SetSampleTime(windowSize);
        bPID.SetOutputLimits(0, windowSize);
        bPID.SetIntegratorLimits(0, 55.0); // AGGIMAX constant
        bPID.SetSmoothingFactor(emaFactor);
        bPID.SetMode(AUTOMATIC);

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
            temperature = sensorManager_->getCurrentTemperature();
            
            LOG(INFO, "Sensor management initialized via SensorManager");
            
            // Note: Scale initialization is still handled separately in main.cpp 
            // because it requires complex global dependencies
            if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
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
        if (Config::getInstance().get<bool>("hardware.switches.power.enabled") && 
            Config::getInstance().get<int>("hardware.switches.power.type") == static_cast<int>(Switch::MOMENTARY)) {
            machineState = kPidNormal;
            setRuntimePidState(true);
            LOG(INFO, "Machine initialized in PID Normal mode (momentary switch)");
        }
        // For toggle switches, force PidOn to switch state mode
        else if (Config::getInstance().get<bool>("hardware.switches.power.enabled") && 
                 Config::getInstance().get<int>("hardware.switches.power.type") == static_cast<int>(Switch::TOGGLE)) {
            if (powerSwitch && powerSwitch->isPressed()) {
                setRuntimePidState(true);
                machineState = kPidNormal;
                LOG(INFO, "Machine initialized in PID Normal mode (toggle switch ON)");
            } else {
                setRuntimePidState(false);
                machineState = kPidDisabled;
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
    aggKi = aggTn > 0 ? aggKp / aggTn : 0;
    aggKd = aggTv * aggKp;
    aggbKi = aggbTn > 0 ? aggbKp / aggbTn : 0;
    aggbKd = aggbTv * aggbKp;
    
    LOG(DEBUG, "Calculated derived PID values");
}

void SystemInitializer::setupTiming() {
    // Initialize timing variables
    unsigned long currentTime = millis();
    previousMillistemp = currentTime;
    windowStartTime = currentTime;
    previousMillisMQTT = currentTime;
    lastMQTTConnectionAttempt = currentTime;
    
    LOG(DEBUG, "Timing variables initialized");
}

void SystemInitializer::registerMQTTParameters() {
    if (!mqttManager) return;

    // Core parameters
    mqttManager->registerParameter("pidON", "pid.enabled");
    mqttManager->registerParameter("brewSetpoint", "brew.setpoint");
    mqttManager->registerParameter("brewTempOffset", "brew.temp_offset");
    mqttManager->registerParameter("steamON", "STEAM_MODE");
    mqttManager->registerParameter("steamSetpoint", "steam.setpoint");
    mqttManager->registerParameter("pidUsePonM", "pid.use_ponm");
    mqttManager->registerParameter("aggKp", "pid.regular.kp");
    mqttManager->registerParameter("aggTn", "pid.regular.tn");
    mqttManager->registerParameter("aggTv", "pid.regular.tv");
    mqttManager->registerParameter("aggIMax", "pid.regular.i_max");
    mqttManager->registerParameter("steamKp", "pid.steam.kp");
    mqttManager->registerParameter("standbyModeOn", "standby.enabled");

    // Brew-specific parameters
    if (Config::getInstance().get<bool>("hardware.switches.brew.enabled")) {
        mqttManager->registerParameter("aggbKp", "pid.bd.kp");
        mqttManager->registerParameter("aggbTn", "pid.bd.tn");
        mqttManager->registerParameter("aggbTv", "pid.bd.tv");
        mqttManager->registerParameter("pidUseBD", "pid.bd.enabled");
        mqttManager->registerParameter("brewPidDelay", "brew.pid_delay");
        mqttManager->registerParameter("targetBrewTime", "brew.by_time.target_time");
        mqttManager->registerParameter("preinfusion", "brew.pre_infusion.time");
        mqttManager->registerParameter("preinfusionPause", "brew.pre_infusion.pause");
        mqttManager->registerParameter("backflushOn", "BACKFLUSH_ON");
        mqttManager->registerParameter("backflushCycles", "backflush.cycles");
        mqttManager->registerParameter("backflushFillTime", "backflush.fill_time");
        mqttManager->registerParameter("backflushFlushTime", "backflush.flush_time");
    }

    // Scale-specific parameters
    if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
        mqttManager->registerParameter("targetBrewWeight", "brew.by_weight.target_weight");
        mqttManager->registerParameter("scaleCalibration", "hardware.sensors.scale.calibration");
        
        if (Config::getInstance().get<int>("hardware.sensors.scale.type") == 0) {
            mqttManager->registerParameter("scale2Calibration", "hardware.sensors.scale.calibration2");
        }
        
        mqttManager->registerParameter("scaleKnownWeight", "hardware.sensors.scale.known_weight");
        mqttManager->registerParameter("scaleTareOn", "TARE_ON");
        mqttManager->registerParameter("scaleCalibrationOn", "CALIBRATION_ON");
    }

    LOG(DEBUG, "MQTT parameters registered");
}

void SystemInitializer::registerMQTTSensors() {
    if (!mqttManager) return;

    // Core sensors
    mqttManager->registerSensor("temperature", [] { return temperature; });
    mqttManager->registerSensor("heaterPower", [] { return pidOutput / 10; });
    mqttManager->registerSensor("standbyModeTimeRemaining", [] { return standbyModeRemainingTimeMillis / 1000; });
    mqttManager->registerSensor("currentKp", [] { return bPID.GetKp(); });
    mqttManager->registerSensor("currentKi", [] { return bPID.GetKi(); });
    mqttManager->registerSensor("currentKd", [] { return bPID.GetKd(); });
    mqttManager->registerSensor("machineState", [] { return machineState; });

    // Brew-specific sensors
    if (Config::getInstance().get<bool>("hardware.switches.brew.enabled")) {
        mqttManager->registerSensor("currBrewTime", [] { return currBrewTime / 1000; });
    }

    // Scale-specific sensors
    if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
        mqttManager->registerSensor("currReadingWeight", [] { return currReadingWeight; });
        mqttManager->registerSensor("currBrewWeight", [] { return currBrewWeight; });
    }

    // Pressure sensor
    if (Config::getInstance().get<bool>("hardware.sensors.pressure.enabled")) {
        mqttManager->registerSensor("pressure", [] { return inputPressureFilter; });
    }

    LOG(DEBUG, "MQTT sensors registered");
}

CleverCoffeeWiFiManager* SystemInitializer::getWiFiManager() const {
    return wifiManager.get();
}