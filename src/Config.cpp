/**
 * @file Config.cpp
 * @brief Implementation of next-generation type-safe configuration system
 */

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/utils/memoryUtils.h"
#include <ArduinoJson.h>

// Optimized static const enum option vectors (single instance + read-only memory)
static const std::vector<std::pair<Hardware::SwitchType, String>> switchTypeOptions = {{Hardware::SwitchType::MOMENTARY, "Momentary"}, {Hardware::SwitchType::TOGGLE, "Toggle"}};

static const std::vector<std::pair<Hardware::SwitchMode, String>> switchModeOptions = {{Hardware::SwitchMode::NORMALLY_OPEN, "Normally Open"}, {Hardware::SwitchMode::NORMALLY_CLOSED, "Normally Closed"}};

static const std::vector<std::pair<Hardware::RelayTriggerType, String>> relayTriggerOptions = {{Hardware::RelayTriggerType::LOW_TRIGGER, "Low Trigger"}, {Hardware::RelayTriggerType::HIGH_TRIGGER, "High Trigger"}};

static const std::vector<std::pair<System::DisplayTemplate, String>> displayTemplateOptions = {{System::DisplayTemplate::STANDARD, "Standard"},
                                                                                               {System::DisplayTemplate::MINIMAL, "Minimal"},
                                                                                               {System::DisplayTemplate::TEMPERATURE_ONLY, "Temperature Only"},
                                                                                               {System::DisplayTemplate::SCALE, "Scale"},
                                                                                               {System::DisplayTemplate::UPRIGHT, "Upright"}};

static const std::vector<std::pair<System::Language, String>> languageOptions = {{System::Language::ENGLISH, "English"}, {System::Language::GERMAN, "German"}, {System::Language::SPANISH, "Spanish"}};

static const std::vector<std::pair<Hardware::OLEDType, String>> oledTypeOptions = {{Hardware::OLEDType::SSD1306, "SSD1306"}, {Hardware::OLEDType::SH1106, "SH1106"}};

static const std::vector<std::pair<Hardware::OLEDAddress, String>> oledAddressOptions = {{Hardware::OLEDAddress::ADDR_3C, "0x3C"}, {Hardware::OLEDAddress::ADDR_3D, "0x3D"}};

static const std::vector<std::pair<Hardware::TemperatureSensorType, String>> temperatureSensorTypeOptions = {{Hardware::TemperatureSensorType::TSIC_306, "TSIC 306"},
                                                                                                             {Hardware::TemperatureSensorType::DALLAS_DS18B20, "Dallas DS18B20"}};

static const std::vector<std::pair<Hardware::ScaleType, String>> scaleTypeOptions = {
    {Hardware::ScaleType::HX711_DUAL, "HX711 (2 load cells)"}, {Hardware::ScaleType::HX711_SINGLE, "HX711 (1 load cell)"}, {Hardware::ScaleType::BLUETOOTH, "Bluetooth"}};

static const std::vector<std::pair<System::LogLevel, String>> logLevelOptions = {{System::LogLevel::TRACE, "TRACE"},     {System::LogLevel::DEBUG, "DEBUG"}, {System::LogLevel::INFO, "INFO"},
                                                                                 {System::LogLevel::WARNING, "WARNING"}, {System::LogLevel::ERROR, "ERROR"}, {System::LogLevel::FATAL, "FATAL"},
                                                                                 {System::LogLevel::SILENT, "SILENT"}};

static const std::vector<std::pair<Process::BrewMode, String>> brewModeOptions = {{Process::BrewMode::MANUAL_BREW, "Manual"}, {Process::BrewMode::AUTOMATIC_BREW, "Automatic"}};

// Accessor functions for static const vectors
const std::vector<std::pair<Hardware::SwitchType, String>>& getSwitchTypeOptions() {
    return switchTypeOptions;
}
const std::vector<std::pair<Hardware::SwitchMode, String>>& getSwitchModeOptions() {
    return switchModeOptions;
}
const std::vector<std::pair<Hardware::RelayTriggerType, String>>& getRelayTriggerOptions() {
    return relayTriggerOptions;
}
const std::vector<std::pair<System::DisplayTemplate, String>>& getDisplayTemplateOptions() {
    return displayTemplateOptions;
}
const std::vector<std::pair<System::Language, String>>& getLanguageOptions() {
    return languageOptions;
}
const std::vector<std::pair<Hardware::OLEDType, String>>& getOledTypeOptions() {
    return oledTypeOptions;
}
const std::vector<std::pair<Hardware::OLEDAddress, String>>& getOledAddressOptions() {
    return oledAddressOptions;
}
const std::vector<std::pair<Hardware::TemperatureSensorType, String>>& getTemperatureSensorTypeOptions() {
    return temperatureSensorTypeOptions;
}
const std::vector<std::pair<Hardware::ScaleType, String>>& getScaleTypeOptions() {
    return scaleTypeOptions;
}
const std::vector<std::pair<System::LogLevel, String>>& getLogLevelOptions() {
    return logLevelOptions;
}
const std::vector<std::pair<Process::BrewMode, String>>& getBrewModeOptions() {
    return brewModeOptions;
}

bool Config::begin() {
    LOG(INFO, "Config: Initializing next-generation configuration system");
    logMemory("Config Begin Start");

    // Load all parameters from NVS
    logMemoryBasic("Before Config Load");
    if (!loadAll()) {
        LOG(WARNING, "Config: Failed to load some parameters from NVS, using defaults");
    }
    logMemoryBasic("After Config Load");

    logMemory("Config Begin Complete");
    LOG(INFO, "Config: Configuration system ready");
    return true;
}

bool Config::loadAll() {
    Preferences prefs;
    if (!prefs.begin(STORAGE_NAMESPACE, true)) { // Read-only mode
        LOG(ERROR, "Config: Failed to open NVS namespace");
        return false;
    }

    auto allParams = getAllConfigParams();
    int loadedCount = 0;

    for (auto* param : allParams) {
        if (param->loadFromNvs(prefs)) {
            loadedCount++;
        }
    }

    prefs.end();

    LOGF(INFO, "Config: Loaded %d/%d parameters from NVS", loadedCount, allParams.size());
    return loadedCount > 0;
}

bool Config::saveAll() {
    Preferences prefs;
    if (!prefs.begin(STORAGE_NAMESPACE, false)) { // Read-write mode
        LOG(ERROR, "Config: Failed to open NVS namespace for writing");
        return false;
    }

    auto allParams = getAllConfigParams();
    int savedCount = 0;

    for (auto* param : allParams) {
        if (param->saveToNvs(prefs)) {
            savedCount++;
        }
    }

    prefs.end();

    LOGF(INFO, "Config: Saved %d/%d parameters to NVS", savedCount, allParams.size());
    return savedCount == allParams.size();
}

void Config::resetAllToDefaults() {
    LOG(INFO, "Config: Resetting all parameters to defaults");

    // Clear NVS storage first
    Preferences prefs;
    if (prefs.begin(STORAGE_NAMESPACE, false)) {
        prefs.clear();
        prefs.end();
        LOG(INFO, "Config: Cleared NVS storage");
    }

    // Reset all parameters to defaults (this only updates memory)
    auto allParams = getAllConfigParams();
    for (auto* param : allParams) {
        param->resetToDefault();
    }

    LOG(INFO, "Config: All parameters reset to defaults");
}

String Config::exportToJson() {
    JsonDocument doc;
    JsonObject root = doc.to<JsonObject>();

    auto allParams = getAllConfigParams();
    for (auto* param : allParams) {
        JsonObject paramObj = root[param->getKey()].to<JsonObject>();
        param->toJson(paramObj);
    }

    String output;
    if (serializeJsonPretty(doc, output) == 0) {
        return "{}";
    }

    return output;
}

bool Config::importFromJson(const String& json) {
    JsonDocument doc;
    DeserializationError error = deserializeJson(doc, json);

    if (error) {
        LOGF(ERROR, "Config: JSON parse error: %s", error.c_str());
        return false;
    }

    auto allParams = getAllConfigParams();
    int updatedCount = 0;

    for (auto* param : allParams) {
        if (doc[param->getKey()].is<JsonObject>()) {
            JsonObject paramObj = doc[param->getKey()];
            if (paramObj["value"].is<JsonVariant>()) {
                if (param->fromString(paramObj["value"])) {
                    updatedCount++;
                }
                else {
                    LOGF(WARNING, "Config: Failed to import parameter: %s", param->getKey().c_str());
                }
            }
        }
    }

    LOGF(INFO, "Config: Imported %d/%d parameters from JSON", updatedCount, allParams.size());

    return updatedCount > 0;
}

void Config::getAllParameters(JsonArray& array, const String& filter) {
    auto allParams = getAllConfigParams();

    for (auto* param : allParams) {
        // Apply filter logic if needed
        bool includeParam = true;

        if (!filter.isEmpty()) {
            if (filter == "hardware") {
                includeParam = param->getSection() == 4;
            }
            else if (filter == "behavior") {
                includeParam = param->getSection() >= 0 && param->getSection() <= 3;
            }
            else if (filter == "other") {
                includeParam = param->getSection() == 5;
            }
            else if (filter == "all") {
                includeParam = true;
            }
            else {
                includeParam = param->getSection() == 0 || param->getSection() == 1;
            }
        }

        if (includeParam) {
            JsonObject paramObj = array.add<JsonObject>();
            param->toJson(paramObj);
        }
    }
}

void Config::getAllStateParams(JsonArray& array) {
    // State parameters are commented out for now
    // Add them back as needed by uncommenting the definitions in Config.h
    // and then adding the .toJson() calls here

    // Example when state parameters are uncommented:
    /*
    JsonObject tempObj = array.add<JsonObject>();
    stateTemperature.toJson(tempObj);

    JsonObject powerObj = array.add<JsonObject>();
    stateHeaterPower.toJson(powerObj);

    JsonObject stateObj = array.add<JsonObject>();
    stateMachineState.toJson(stateObj);
    */
}

ConfigParamDef* Config::findConfigParameter(const String& key) {
    auto allParams = getAllConfigParams();

    for (auto* param : allParams) {
        if (param->getKey() == key) {
            return param;
        }
    }

    return nullptr;
}

std::vector<ConfigParamDef*> Config::getAllConfigParams() {
    return {
        // === PID PARAMETERS (Section 0) ===
        &pidEnabled,
        &pidUsePonm,
        &pidEmaFactor,
        &pidRegularKp,
        &pidRegularTn,
        &pidRegularTv,
        &pidRegularIMax,
        &pidSteamKp,

        // === BREW PARAMETERS (Section 1) ===
        &brewSetpoint,
        &brewTempOffset,
        &steamSetpoint,

        // === BREW DETECTION PID PARAMETERS (Section 2) ===
        &pidBdEnabled,
        &brewPidDelay,
        &pidBdKp,
        &pidBdTn,
        &pidBdTv,

        // === BREWING CONTROL PARAMETERS (Section 3) ===
        &brewByTimeEnabled,
        &brewByTimeTargetTime,
        &brewByWeightEnabled,
        &brewByWeightTargetWeight,
        &brewByWeightAutoTare,
        &brewPreInfusionEnabled,
        &brewPreInfusionTime,
        &brewPreInfusionPause,
        &displayFullscreenBrewTimer,
        &displayFullscreenManualFlushTimer,
        &displayFullscreenHotWaterTimer,
        &displayPostBrewTimerDuration,
        &displayHeatingLogo,
        &displayPidOffLogo,

        // === HARDWARE LEDS PARAMETERS (Section 4) ===

        &hardwareLedsStatusEnabled,
        &hardwareLedsStatusInverted,
        &hardwareLedsBrewEnabled,
        &hardwareLedsBrewInverted,
        &hardwareLedsSteamEnabled,
        &hardwareLedsSteamInverted,

        // === DISPLAY PARAMETERS (Section 5) ===
        &displayTemplate,
        &displayInverted,
        &displayLanguage,

        // === BACKFLUSH PARAMETERS (Section 6) ===
        &backflushCycles,
        &backflushFillTime,
        &backflushFlushTime,

        // === STANDBY PARAMETERS (Section 7) ===
        &standbyEnabled,
        &standbyTime,

        // === MQTT PARAMETERS (Section 8) ===
        &mqttEnabled,
        &mqttBroker,
        &mqttPort,
        &mqttUsername,
        &mqttPassword,
        &mqttTopic,
        &mqttHassioEnabled,
        &mqttHassioPrefix,

        // === SYSTEM PARAMETERS (Section 9) ===
        &systemHostname,
        &systemOtaPassword,
        &systemOfflineMode,
        &systemLogLevel,
        &systemAuthEnabled,
        &systemAuthUsername,
        &systemAuthPassword,
        &systemTimingDebugEnabled,
        &systemShowdisplayEnabled,

        // === HARDWARE OLED PARAMETERS (Section 11) ===
        &hardwareOledEnabled,
        &hardwareOledType,
        &hardwareOledAddress,

        // === HARDWARE RELAYS PARAMETERS (Section 12) ===
        &hardwareRelaysHeaterTriggerType,
        &hardwareRelaysValveTriggerType,
        &hardwareRelaysPumpTriggerType,
        // === HARDWARE SWITCHES PARAMETERS (Section 13) ===
        &hardwareSwitchesBrewEnabled,
        &hardwareSwitchesBrewType,
        &hardwareSwitchesBrewMode,
        &hardwareSwitchesSteamEnabled,
        &hardwareSwitchesSteamType,
        &hardwareSwitchesSteamMode,
        &hardwareSwitchesPowerEnabled,
        &hardwareSwitchesPowerType,
        &hardwareSwitchesPowerMode,
        &hardwareSwitchesHotWaterEnabled,
        &hardwareSwitchesHotWaterType,
        &hardwareSwitchesHotWaterMode,

        // === HARDWARE SENSORS PARAMETERS (Section 15 & 4) ===
        &hardwareSensorsTemperatureType,
        &hardwareSensorsPressureEnabled,
        &hardwareSensorsWatertankEnabled,
        &hardwareSensorsWatertankMode,
        &hardwareSensorsScaleEnabled,
        &hardwareSensorsScaleSamples,
        &hardwareSensorsScaleType,
        &hardwareSensorsScaleCalibration,
        &hardwareSensorsScaleCalibration2,
        &hardwareSensorsScaleKnownWeight,
    };
}
