/**
 * @file Config.cpp
 * @brief Simplified unified configuration implementation
 */

#include "Config.h"

// Global variable definitions - these are the actual storage locations
// PID Parameters
bool pidON = false;
bool usePonM = false;
double aggKp = AGGKP;
double aggTn = AGGTN;
double aggTv = AGGTV;
double aggIMax = AGGIMAX;
double steamKp = STEAMKP;
double emaFactor = EMA_FACTOR;

// Brew Detection PID
bool useBDPID = false;
double aggbKp = AGGBKP;
double aggbTn = AGGBTN;
double aggbTv = AGGBTV;

// Temperature Parameters
double brewSetpoint = SETPOINT;
double brewTempOffset = TEMPOFFSET;
double brewPidDelay = BREW_PID_DELAY;
double steamSetpoint = STEAMSETPOINT;

// Brew Parameters
double targetBrewTime = TARGET_BREW_TIME;
double preinfusion = PRE_INFUSION_TIME;
double preinfusionPause = PRE_INFUSION_PAUSE_TIME;
double targetBrewWeight = TARGET_BREW_WEIGHT;
int backflushCycles = BACKFLUSH_CYCLES;
double backflushFillTime = BACKFLUSH_FILL_TIME;
double backflushFlushTime = BACKFLUSH_FLUSH_TIME;

// System Parameters
String hostname = HOSTNAME;
String otaPassword = OTAPASS;
bool offlineMode = false;
bool authEnabled = false;
String authUsername = AUTH_USERNAME;
String authPassword = AUTH_PASSWORD;
bool timingDebugActive = false;
bool includeDisplayInLogs = true;

// Power Management
bool standbyModeOn = false;
double standbyModeTime = STANDBY_MODE_TIME;

// MQTT Parameters
bool mqttEnabled = false;
String mqttBroker = "";
int mqttPort = 1883;
String mqttUsername = MQTT_USERNAME;
String mqttPassword = MQTT_PASSWORD;
String mqttTopic = MQTT_TOPIC;
bool mqttHassioEnabled = false;
String mqttHassioPrefix = MQTT_HASSIO_PREFIX;

// Hardware - OLED
bool oledEnabled = true;
int oledType = 0;
int oledAddress = 0;

// Hardware - Relays
int heaterTriggerType = 1;
int valveTriggerType = 1;
int pumpTriggerType = 1;

// Hardware - Switches
bool brewSwitchEnabled = false;
int brewSwitchType = 1;
int brewSwitchMode = 0;
bool steamSwitchEnabled = false;
int steamSwitchType = 1;
int steamSwitchMode = 0;
bool powerSwitchEnabled = false;
int powerSwitchType = 1;
int powerSwitchMode = 0;
bool hotWaterSwitchEnabled = false;
int hotWaterSwitchType = 1;
int hotWaterSwitchMode = 0;

// Hardware - LEDs
bool statusLedEnabled = false;
bool statusLedInverted = false;
bool brewLedEnabled = false;
bool brewLedInverted = false;
bool steamLedEnabled = false;
bool steamLedInverted = false;

// Hardware - Sensors
int temperatureSensorType = 0;
bool pressureSensorEnabled = false;
bool waterTankSensorEnabled = false;
int waterTankSensorMode = 1;

// Scale settings
bool scaleEnabled = false;
int scaleSamples = SCALE_SAMPLES;
int scaleType = 0;
double scaleCalibrationFactor = SCALE_CALIBRATION_FACTOR;
double scaleCalibrationFactor2 = SCALE_CALIBRATION_FACTOR;
double scaleKnownWeight = SCALE_KNOWN_WEIGHT;

// Display Parameters
int displayTemplate = 0;
bool displayInverted = false;
int displayLanguage = 0;
bool featureFullscreenBrewTimer = false;
bool featureFullscreenManualFlushTimer = false;
bool featureFullscreenHotWaterTimer = false;
double postBrewTimerDuration = POST_BREW_TIMER_DURATION;
bool featureHeatingLogo = true;
bool featurePidOffLogo = true;

// Runtime variables (not configurable)
bool steamON = false;
bool backflushOn = false;
double temperature = 0.0;
bool scaleTareOn = false;
bool scaleCalibrationOn = false;
extern int logLevel;
const char sysVersion[64] = "4.0.0-beta2+feat-ui.3cfd5e7";

// Backward compatibility reference
Config& config = Config::getInstance();

void Config::initializeParams() {
    // PID Parameters - directly bind to global variables with defaults and help text
    _params["pid.enabled"] = ParamDef::Bool(&pidON, false, "Enable PID Controller", 0, 101, "Enables or disables the PID temperature controller");
    _params["pid.use_ponm"] = ParamDef::Bool(&usePonM, false, "Enable PonM", 0, 102, "Use PonM mode (Proportional on Measurement)");
    _params["pid.ema_factor"] = ParamDef::Double(&emaFactor, EMA_FACTOR, PID_EMA_FACTOR_MIN, PID_EMA_FACTOR_MAX, "PID EMA Factor", 0, 111, "Smoothing of input for derivative component. Smaller = less smoothing but less delay");
    _params["pid.regular.kp"] = ParamDef::Double(&aggKp, AGGKP, PID_KP_REGULAR_MIN, PID_KP_REGULAR_MAX, "PID Kp", 0, 112, "Proportional gain (in Watts/°C) for the main PID controller");
    _params["pid.regular.tn"] = ParamDef::Double(&aggTn, AGGTN, PID_TN_REGULAR_MIN, PID_TN_REGULAR_MAX, "PID Tn", 0, 113, "Integral time constant (in seconds) for the main PID controller");
    _params["pid.regular.tv"] = ParamDef::Double(&aggTv, AGGTV, PID_TV_REGULAR_MIN, PID_TV_REGULAR_MAX, "PID Tv", 0, 114, "Differential time constant (in seconds) for the main PID controller");
    _params["pid.regular.i_max"] = ParamDef::Double(&aggIMax, AGGIMAX, PID_I_MAX_REGULAR_MIN, PID_I_MAX_REGULAR_MAX, "PID Integrator Max", 0, 115, "Internal integrator limit to prevent windup (in Watts)");
    _params["pid.steam.kp"] = ParamDef::Double(&steamKp, STEAMKP, PID_KP_STEAM_MIN, PID_KP_STEAM_MAX, "Steam Kp", 0, 116, "Proportional gain for the steaming mode");

    // Brew Detection PID
    _params["pid.bd.enabled"] = ParamDef::Bool(&useBDPID, false, "Enable Brew PID", 2, 701, "Use separate PID parameters while brew is running");
    _params["pid.bd.kp"] = ParamDef::Double(&aggbKp, AGGBKP, PID_KP_BD_MIN, PID_KP_BD_MAX, "BD Kp", 2, 712, "Proportional gain for PID when brewing has been detected");
    _params["pid.bd.tn"] = ParamDef::Double(&aggbTn, AGGBTN, PID_TN_BD_MIN, PID_TN_BD_MAX, "BD Tn", 2, 713, "Integral time constant for PID when brewing has been detected");
    _params["pid.bd.tv"] = ParamDef::Double(&aggbTv, AGGBTV, PID_TV_BD_MIN, PID_TV_BD_MAX, "BD Tv", 2, 714, "Differential time constant for PID when brewing has been detected");

    // Temperature Parameters
    _params["brew.setpoint"] = ParamDef::Double(&brewSetpoint, SETPOINT, BREW_SETPOINT_MIN, BREW_SETPOINT_MAX, "Setpoint (°C)", 1, 201, "The temperature that the PID will attempt to reach and hold");
    _params["brew.temp_offset"] = ParamDef::Double(&brewTempOffset, TEMPOFFSET, BREW_TEMP_OFFSET_MIN, BREW_TEMP_OFFSET_MAX, "Offset (°C)", 1, 202, "Optional offset added to the user-visible setpoint to compensate sensor offsets");
    _params["brew.pid_delay"] = ParamDef::Double(&brewPidDelay, BREW_PID_DELAY, BREW_PID_DELAY_MIN, BREW_PID_DELAY_MAX, "Brew PID Delay (s)", 2, 711, "Delay time during which PID will be disabled once brew is detected");
    _params["steam.setpoint"] = ParamDef::Double(&steamSetpoint, STEAMSETPOINT, STEAM_SETPOINT_MIN, STEAM_SETPOINT_MAX, "Steam Setpoint (°C)", 1, 203, "The temperature that the PID will use for steam mode");

    // System Parameters
    _params["system.hostname"] = ParamDef::String(&hostname, HOSTNAME, HOSTNAME_MAX_LENGTH, "Hostname", 6, 1101, "Hostname of your machine, changes require a restart");
    _params["system.ota_password"] = ParamDef::String(&otaPassword, OTAPASS, PASSWORD_MAX_LENGTH, "OTA Password", 6, 1102, "Password for over-the-air updates, changes require a restart");
    _params["system.offline_mode"] = ParamDef::Bool(&offlineMode, false, "Offline Mode", 6, 1103, "Run in offline mode without WiFi connection");
    _params["system.auth.enabled"] = ParamDef::Bool(&authEnabled, false, "Enable Authentication", 6, 1201, "Enables authentication for accessing certain parts of the website");
    _params["system.auth.username"] = ParamDef::String(&authUsername, AUTH_USERNAME, USERNAME_MAX_LENGTH, "Website Username", 6, 1202, "Username for accessing the website and authenticating web requests");
    _params["system.auth.password"] = ParamDef::String(&authPassword, AUTH_PASSWORD, PASSWORD_MAX_LENGTH, "Website Password", 6, 1203, "Password for accessing the website and authenticating web requests");
    _params["system.timing_debug.enabled"] = ParamDef::Bool(&timingDebugActive, false, "Loop timing in console", 6, 1301, "Enable or disable the process loop time debugging in console");
    _params["system.showdisplay.enabled"] = ParamDef::Bool(&includeDisplayInLogs, true, "Activate display recording", 6, 1303, "Enable or disable showing sendBuffer loops in debug logs");

    // Power Management
    _params["standby.enabled"] = ParamDef::Bool(&standbyModeOn, false, "Enable Standby Timer", 7, 801, "Turn heater off after standby time has elapsed");
    _params["standby.time"] = ParamDef::Double(&standbyModeTime, STANDBY_MODE_TIME, STANDBY_MODE_TIME_MIN, STANDBY_MODE_TIME_MAX, "Standby Time", 7, 802, "Time in minutes until the heater is turned off");

    // MQTT Parameters
    _params["mqtt.enabled"] = ParamDef::Bool(&mqttEnabled, false, "MQTT Enabled", 5, 1001, "Enables MQTT, change requires a restart");
    _params["mqtt.broker"] = ParamDef::String(&mqttBroker, "", MQTT_BROKER_MAX_LENGTH, "MQTT Broker", 5, 1011, "IP address or hostname of your MQTT broker");
    _params["mqtt.port"] = ParamDef::Int(&mqttPort, 1883, 1, 65535, "MQTT Port", 5, 1012, "Port number of your MQTT broker");
    _params["mqtt.username"] = ParamDef::String(&mqttUsername, MQTT_USERNAME, USERNAME_MAX_LENGTH, "Username", 5, 1013, "Username for your MQTT broker");
    _params["mqtt.password"] = ParamDef::String(&mqttPassword, MQTT_PASSWORD, PASSWORD_MAX_LENGTH, "Password", 5, 1014, "Password for your MQTT broker");
    _params["mqtt.topic"] = ParamDef::String(&mqttTopic, MQTT_TOPIC, MQTT_TOPIC_MAX_LENGTH, "Topic Prefix", 5, 1015, "Custom MQTT topic prefix");
    _params["mqtt.hassio.enabled"] = ParamDef::Bool(&mqttHassioEnabled, false, "Hass.io enabled", 5, 1021, "Enables Home Assistant integration");
    _params["mqtt.hassio.prefix"] = ParamDef::String(&mqttHassioPrefix, MQTT_HASSIO_PREFIX, MQTT_HASSIO_PREFIX_MAX_LENGTH, "Hass.io Prefix", 5, 1022, "Custom MQTT topic prefix for Home Assistant");

    // Hardware - OLED
    _params["hardware.oled.enabled"] = ParamDef::Bool(&oledEnabled, true, "Enable OLED Display", 4, 2001, "Enable or disable the OLED display");
    _params["hardware.oled.type"] = ParamDef::Int(&oledType, 0, 0, 1, "OLED Type", 4, 2002, "Select your OLED display type");
    _params["hardware.oled.address"] = ParamDef::Int(&oledAddress, 0, 0, 1, "I2C Address", 4, 2003, "I2C address of the OLED display");

    // Hardware - Relays
    _params["hardware.relays.heater.trigger_type"] = ParamDef::Int(&heaterTriggerType, 1, 0, 1, "Heater Relay Trigger Type", 4, 2101, "Relay trigger type for heater control");
    _params["hardware.relays.valve.trigger_type"] = ParamDef::Int(&valveTriggerType, 1, 0, 1, "Valve Relay Trigger Type", 4, 2102, "Relay trigger type for valve control");
    _params["hardware.relays.pump.trigger_type"] = ParamDef::Int(&pumpTriggerType, 1, 0, 1, "Pump Relay Trigger Type", 4, 2103, "Relay trigger type for pump control");

    // Hardware - Switches
    _params["hardware.switches.brew.enabled"] = ParamDef::Bool(&brewSwitchEnabled, false, "Enable Brew Switch", 4, 2201, "Enable physical brew switch");
    _params["hardware.switches.brew.type"] = ParamDef::Int(&brewSwitchType, 1, 0, 2, "Brew Switch Type", 4, 2202, "Type of brew switch connected");
    _params["hardware.switches.brew.mode"] = ParamDef::Int(&brewSwitchMode, 0, 0, 1, "Brew Switch Mode", 4, 2203, "Electrical configuration of brew switch");
    _params["hardware.switches.steam.enabled"] = ParamDef::Bool(&steamSwitchEnabled, false, "Enable Steam Switch", 4, 2211, "Enable physical steam switch");
    _params["hardware.switches.steam.type"] = ParamDef::Int(&steamSwitchType, 1, 0, 2, "Steam Switch Type", 4, 2212, "Type of steam switch connected");
    _params["hardware.switches.steam.mode"] = ParamDef::Int(&steamSwitchMode, 0, 0, 1, "Steam Switch Mode", 4, 2213, "Electrical configuration of steam switch");
    _params["hardware.switches.power.enabled"] = ParamDef::Bool(&powerSwitchEnabled, false, "Enable Power Switch", 4, 2221, "Enable physical power switch");
    _params["hardware.switches.power.type"] = ParamDef::Int(&powerSwitchType, 1, 0, 2, "Power Switch Type", 4, 2222, "Type of power switch connected");
    _params["hardware.switches.power.mode"] = ParamDef::Int(&powerSwitchMode, 0, 0, 1, "Power Switch Mode", 4, 2223, "Electrical configuration of power switch");
    _params["hardware.switches.hot_water.enabled"] = ParamDef::Bool(&hotWaterSwitchEnabled, false, "Enable Water Switch", 4, 2231, "Enable physical water switch");
    _params["hardware.switches.hot_water.type"] = ParamDef::Int(&hotWaterSwitchType, 1, 0, 2, "Water Switch Type", 4, 2232, "Type of water switch connected");
    _params["hardware.switches.hot_water.mode"] = ParamDef::Int(&hotWaterSwitchMode, 0, 0, 1, "Water Switch Mode", 4, 2233, "Electrical configuration of water switch");

    // Hardware - LEDs
    _params["hardware.leds.status.enabled"] = ParamDef::Bool(&statusLedEnabled, false, "Enable Status LED", 4, 2301, "Enable status indicator LED");
    _params["hardware.leds.status.inverted"] = ParamDef::Bool(&statusLedInverted, false, "Invert Status LED", 4, 2302, "Invert the status LED logic (for common anode LEDs)");
    _params["hardware.leds.brew.enabled"] = ParamDef::Bool(&brewLedEnabled, false, "Enable Brew LED", 4, 2311, "Enable brew indicator LED");
    _params["hardware.leds.brew.inverted"] = ParamDef::Bool(&brewLedInverted, false, "Invert Brew LED", 4, 2312, "Invert the brew LED logic");
    _params["hardware.leds.steam.enabled"] = ParamDef::Bool(&steamLedEnabled, false, "Enable Steam LED", 4, 2321, "Enable steam indicator LED");
    _params["hardware.leds.steam.inverted"] = ParamDef::Bool(&steamLedInverted, false, "Invert Steam LED", 4, 2322, "Invert the steam LED logic");

    // Hardware - Sensors
    _params["hardware.sensors.temperature.type"] = ParamDef::Int(&temperatureSensorType, 0, 0, 1, "Temperature Sensor Type", 4, 2401, "Select temperature sensor type");
    _params["hardware.sensors.pressure.enabled"] = ParamDef::Bool(&pressureSensorEnabled, false, "Enable Pressure Sensor", 4, 2411, "Enable pressure sensor functionality");
    _params["hardware.sensors.watertank.enabled"] = ParamDef::Bool(&waterTankSensorEnabled, false, "Enable Water Tank Sensor", 4, 2421, "Enable water tank level sensor");
    _params["hardware.sensors.watertank.mode"] = ParamDef::Int(&waterTankSensorMode, 1, 0, 1, "Water Tank Sensor Mode", 4, 2422, "Electrical configuration of water tank sensor");

    // Scale settings
    _params["hardware.sensors.scale.enabled"] = ParamDef::Bool(&scaleEnabled, false, "Enable Scale", 4, 2501, "Enable scale functionality");
    _params["hardware.sensors.scale.samples"] = ParamDef::Int(&scaleSamples, SCALE_SAMPLES, 1, 20, "Scale Samples", 4, 2502, "Number of samples used for calibration");
    _params["hardware.sensors.scale.type"] = ParamDef::Int(&scaleType, 0, 0, 5, "Scale Type", 4, 2503, "Type of scale connected");
    _params["hardware.sensors.scale.calibration"] = ParamDef::Double(&scaleCalibrationFactor, SCALE_CALIBRATION_FACTOR, SCALE_CALIBRATION_MIN, SCALE_CALIBRATION_MAX, "Scale Calibration", 4, 2504, "Raw data is divided by this value to convert to readable data");
    _params["hardware.sensors.scale.calibration2"] = ParamDef::Double(&scaleCalibrationFactor2, SCALE_CALIBRATION_FACTOR, SCALE_CALIBRATION_MIN, SCALE_CALIBRATION_MAX, "Scale Calibration 2", 4, 2505, "Second calibration factor for dual load cell scales");
    _params["hardware.sensors.scale.known_weight"] = ParamDef::Double(&scaleKnownWeight, SCALE_KNOWN_WEIGHT, SCALE_KNOWN_WEIGHT_MIN, SCALE_KNOWN_WEIGHT_MAX, "Scale Known Weight", 4, 2506, "Calibration weight for scale (weight of the tray)");

    // Brew settings
    _params["brew.by_weight.target_weight"] = ParamDef::Double(&targetBrewWeight, TARGET_BREW_WEIGHT, TARGET_BREW_WEIGHT_MIN, TARGET_BREW_WEIGHT_MAX, "Target Brew Weight (g)", 3, 322, "Brew is running until this weight has been measured");

    // Display Parameters
    _params["display.template"] = ParamDef::Int(&displayTemplate, 0, 0, 4, "Display Template", 3, 901, "Set the display template, changes require a reboot");
    _params["display.inverted"] = ParamDef::Bool(&displayInverted, false, "Invert Display", 3, 902, "Set the display rotation, changes require a reboot");
    _params["display.language"] = ParamDef::Int(&displayLanguage, 0, 0, 2, "Display Language", 3, 903, "Set the language for the OLED display");
    _params["display.fullscreen_brew_timer"] = ParamDef::Bool(&featureFullscreenBrewTimer, false, "Enable Fullscreen Brew Timer", 3, 904, "Enable fullscreen overlay during brew");
    _params["display.fullscreen_manual_flush_timer"] = ParamDef::Bool(&featureFullscreenManualFlushTimer, false, "Enable Fullscreen Manual Flush Timer", 3, 905, "Enable fullscreen overlay during manual flush");
    _params["display.fullscreen_hot_water_timer"] = ParamDef::Bool(&featureFullscreenHotWaterTimer, false, "Enable Fullscreen Hot Water Timer", 3, 906, "Enable fullscreen overlay during hot water mode");
    _params["display.post_brew_timer_duration"] = ParamDef::Double(&postBrewTimerDuration, POST_BREW_TIMER_DURATION, POST_BREW_TIMER_DURATION_MIN, POST_BREW_TIMER_DURATION_MAX, "Post Brew Timer Duration (s)", 3, 907, "Time in seconds that brew timer will be shown after brew finished");
    _params["display.heating_logo"] = ParamDef::Bool(&featureHeatingLogo, true, "Enable Heating Logo", 3, 908, "Full screen logo will be shown if temperature is 5°C below setpoint");
    _params["display.pid_off_logo"] = ParamDef::Bool(&featurePidOffLogo, true, "Enable 'PID Disabled' Logo", 3, 909, "Full screen logo will be shown if PID is disabled");
}

bool Config::loadFromJson(const ::String& jsonString) {
    JsonDocument doc;
    DeserializationError error = deserializeJson(doc, jsonString);

    if (error) {
        LOGF(ERROR, "JSON parsing failed: %s", error.c_str());
        return false;
    }

    for (JsonPairConst pair : doc.as<JsonObjectConst>()) {
        ::String path = pair.key().c_str();
        JsonVariantConst value = pair.value();

        auto it = _params.find(path.c_str());
        if (it == _params.end()) {
            LOGF(WARNING, "Unknown parameter: %s", path.c_str());
            continue;
        }

        const ParamDef& def = it->second;

        // Type-safe value setting
        switch (def.type) {
            case ParamType::BOOL:
                if (value.is<bool>()) {
                    set<bool>(path, value.as<bool>());
                }
                break;
            case ParamType::INT:
                if (value.is<int>()) {
                    set<int>(path, value.as<int>());
                }
                break;
            case ParamType::DOUBLE:
                if (value.is<double>()) {
                    set<double>(path, value.as<double>());
                }
                break;
            case ParamType::STRING:
                if (value.is<const char*>()) {
                    set<::String>(path, ::String(value.as<const char*>()));
                }
                break;
        }
    }

    return true;
}

::String Config::exportToJson() const {
    JsonDocument doc;

    for (const auto& [path, def] : _params) {
        switch (def.type) {
            case ParamType::BOOL:
                doc[path] = get<bool>(String(path.c_str()));
                break;
            case ParamType::INT:
                doc[path] = get<int>(String(path.c_str()));
                break;
            case ParamType::DOUBLE:
                doc[path] = get<double>(String(path.c_str()));
                break;
            case ParamType::STRING:
                doc[path] = get<::String>(::String(path.c_str()));
                break;
        }
    }

    ::String output;
    serializeJson(doc, output);
    return output;
}

JsonDocument Config::getParametersForAPI(const ::String& section) const {
    JsonDocument doc;
    JsonArray array = doc.to<JsonArray>();

    for (const auto& [path, def] : _params) {
        if (!def.showCondition()) continue;

        JsonObject paramObj = array.add<JsonObject>();
        paramObj["id"] = path;
        paramObj["name"] = def.displayName;
        paramObj["section"] = def.section;
        paramObj["position"] = def.position;
        paramObj["type"] = static_cast<int>(def.type);
        paramObj["help"] = def.helpText;

        // Add current value and constraints
        switch (def.type) {
            case ParamType::BOOL:
                paramObj["value"] = get<bool>(String(path.c_str()));
                paramObj["default"] = def.defaultBool;
                break;
            case ParamType::INT:
                paramObj["value"] = get<int>(String(path.c_str()));
                paramObj["default"] = def.defaultInt;
                paramObj["min"] = def.minValue;
                paramObj["max"] = def.maxValue;
                break;
            case ParamType::DOUBLE:
                paramObj["value"] = get<double>(String(path.c_str()));
                paramObj["default"] = def.defaultDouble;
                paramObj["min"] = def.minValue;
                paramObj["max"] = def.maxValue;
                break;
            case ParamType::STRING:
                paramObj["value"] = get<::String>(::String(path.c_str()));
                paramObj["default"] = def.defaultString;
                paramObj["maxLength"] = def.maxLength;
                break;
        }
    }

    return doc;
}

void Config::loadFromNVS() {
    for (const auto& [path, def] : _params) {
        _prefs.begin("config", true);

        switch (def.type) {
            case ParamType::BOOL: {
                bool value = _prefs.getBool(path.c_str(), def.defaultBool);
                if (def.globalVar) *static_cast<bool*>(def.globalVar) = value;
                break;
            }
            case ParamType::INT: {
                int value = _prefs.getInt(path.c_str(), def.defaultInt);
                if (def.globalVar) *static_cast<int*>(def.globalVar) = value;
                break;
            }
            case ParamType::DOUBLE: {
                double value = _prefs.getDouble(path.c_str(), def.defaultDouble);
                if (def.globalVar) *static_cast<double*>(def.globalVar) = value;
                break;
            }
            case ParamType::STRING: {
                ::String value = _prefs.getString(path.c_str(), def.defaultString);
                if (def.globalVar) *static_cast<::String*>(def.globalVar) = value;
                break;
            }
        }

        _prefs.end();
    }
}

void Config::saveToNVS() {
    for (const auto& [path, def] : _params) {
        if (!def.globalVar) continue;

        switch (def.type) {
            case ParamType::BOOL:
                saveToNVS(path.c_str(), *static_cast<bool*>(def.globalVar));
                break;
            case ParamType::INT:
                saveToNVS(path.c_str(), *static_cast<int*>(def.globalVar));
                break;
            case ParamType::DOUBLE:
                saveToNVS(path.c_str(), *static_cast<double*>(def.globalVar));
                break;
            case ParamType::STRING:
                saveToNVS(path.c_str(), *static_cast<::String*>(def.globalVar));
                break;
        }
    }
}

bool Config::resetToDefault(const ::String& path) {
    auto it = _params.find(path.c_str());
    if (it == _params.end()) return false;

    const ParamDef& def = it->second;

    switch (def.type) {
        case ParamType::BOOL:
            return set<bool>(path, def.defaultBool);
        case ParamType::INT:
            return set<int>(path, def.defaultInt);
        case ParamType::DOUBLE:
            return set<double>(path, def.defaultDouble);
        case ParamType::STRING:
            return set<::String>(path, def.defaultString);
    }
    return false;
}

void Config::resetAllToDefaults() {
    LOGF(INFO, "Resetting all parameters to default values");
    for (const auto& [path, def] : _params) {
        resetToDefault(::String(path.c_str()));
    }
    LOGF(INFO, "All parameters reset to defaults");
}

void Config::initializeGlobalVariablesWithDefaults() {
    LOGF(INFO, "Initializing global variables with default values");

    for (const auto& [path, def] : _params) {
        if (!def.globalVar) continue;

        // Set global variables to their default values before NVS loading
        switch (def.type) {
            case ParamType::BOOL:
                *static_cast<bool*>(def.globalVar) = def.defaultBool;
                break;
            case ParamType::INT:
                *static_cast<int*>(def.globalVar) = def.defaultInt;
                break;
            case ParamType::DOUBLE:
                *static_cast<double*>(def.globalVar) = def.defaultDouble;
                break;
            case ParamType::STRING:
                *static_cast<::String*>(def.globalVar) = def.defaultString;
                break;
        }
    }

    LOGF(INFO, "Global variables initialized with defaults");
}
