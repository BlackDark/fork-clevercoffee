/**
 * @file ConfigV2.cpp  
 * @brief Implementation of next-generation type-safe configuration system
 */

#include "ConfigV2.h"
#include "state/GlobalState.h"
#include "Logger.h"
#include <ArduinoJson.h>

bool ConfigV2::begin() {
    LOG(INFO, "ConfigV2: Initializing next-generation configuration system");
    
    // Load all parameters from NVS
    if (!loadAll()) {
        LOG(WARNING, "ConfigV2: Failed to load some parameters from NVS, using defaults");
    }
    
    LOG(INFO, "ConfigV2: Configuration system ready");
    return true;
}

bool ConfigV2::loadAll() {
    Preferences prefs;
    if (!prefs.begin(STORAGE_NAMESPACE, true)) { // Read-only mode
        LOG(ERROR, "ConfigV2: Failed to open NVS namespace");
        return false;
    }
    
    auto allParams = getAllParamDefs();
    int loadedCount = 0;
    
    for (auto* param : allParams) {
        if (param->loadFromNvs(prefs)) {
            loadedCount++;
        }
    }
    
    prefs.end();
    
    LOGF(INFO, "ConfigV2: Loaded %d/%d parameters from NVS", loadedCount, allParams.size());
    return loadedCount > 0;
}

bool ConfigV2::saveAll() {
    Preferences prefs;
    if (!prefs.begin(STORAGE_NAMESPACE, false)) { // Read-write mode
        LOG(ERROR, "ConfigV2: Failed to open NVS namespace for writing");
        return false;
    }
    
    auto allParams = getAllParamDefs();
    int savedCount = 0;
    
    for (auto* param : allParams) {
        if (param->saveToNvs(prefs)) {
            savedCount++;
        }
    }
    
    prefs.end();
    
    LOGF(INFO, "ConfigV2: Saved %d/%d parameters to NVS", savedCount, allParams.size());
    return savedCount == allParams.size();
}

void ConfigV2::resetAllToDefaults() {
    LOG(INFO, "ConfigV2: Resetting all parameters to defaults");
    
    auto allParams = getAllParamDefs();
    for (auto* param : allParams) {
        param->resetToDefault();
    }
    
    // Clear NVS storage
    Preferences prefs;
    if (prefs.begin(STORAGE_NAMESPACE, false)) {
        prefs.clear();
        prefs.end();
        LOG(INFO, "ConfigV2: Cleared NVS storage");
    }
}

String ConfigV2::exportToJson() {
    JsonDocument doc;
    JsonObject root = doc.to<JsonObject>();
    
    auto allParams = getAllParamDefs();
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

bool ConfigV2::importFromJson(const String& json) {
    JsonDocument doc;
    DeserializationError error = deserializeJson(doc, json);
    
    if (error) {
        LOGF(ERROR, "ConfigV2: JSON parse error: %s", error.c_str());
        return false;
    }
    
    auto allParams = getAllParamDefs();
    int updatedCount = 0;
    
    for (auto* param : allParams) {
        if (doc.containsKey(param->getKey())) {
            JsonObject paramObj = doc[param->getKey()];
            if (paramObj.containsKey("value")) {
                if (param->fromJson(paramObj["value"])) {
                    updatedCount++;
                } else {
                    LOGF(WARNING, "ConfigV2: Failed to import parameter: %s", param->getKey().c_str());
                }
            }
        }
    }
    
    LOGF(INFO, "ConfigV2: Imported %d/%d parameters from JSON", updatedCount, allParams.size());
    
    // Save updated values
    saveAll();
    
    return updatedCount > 0;
}

void ConfigV2::getAllParameters(JsonArray& array, const String& filter) {
    auto allParams = getAllParamDefs();
    
    for (auto* param : allParams) {
        // Apply filter logic if needed
        bool includeParam = true;
        
        if (!filter.isEmpty()) {
            if (filter == "hardware") {
                includeParam = param->getSection() == 4;
            } else if (filter == "behavior") {
                includeParam = param->getSection() >= 0 && param->getSection() <= 3;
            } else if (filter == "other") {
                includeParam = param->getSection() == 5;
            } else if (filter == "all") {
                includeParam = true;
            } else {
                includeParam = param->getSection() == 0 || param->getSection() == 1;
            }
        }
        
        if (includeParam) {
            JsonObject paramObj = array.add<JsonObject>();
            param->toJson(paramObj);
        }
    }
}

void ConfigV2::getAllStateParams(JsonArray& array) {
    // Add state parameters to JSON array for web interface
    JsonObject tempObj = array.add<JsonObject>();
    stateTemperature.toJson(tempObj);
    
    JsonObject powerObj = array.add<JsonObject>();
    stateHeaterPower.toJson(powerObj);
    
    JsonObject stateObj = array.add<JsonObject>();
    stateMachineState.toJson(stateObj);
}

BaseParamDef* ConfigV2::findParameter(const String& key) {
    auto allParams = getAllParamDefs();
    
    for (auto* param : allParams) {
        if (param->getKey() == key) {
            return param;
        }
    }
    
    return nullptr;
}

std::vector<BaseParamDef*> ConfigV2::getAllParamDefs() {
    return {
        // Core PID Parameters
        &pidEnabled,
        &pidUsePonM,
        &pidRegularKp,
        &pidRegularTn,
        &pidRegularTv,
        
        // Brew Parameters
        &brewSetpoint,
        &brewTempOffset,
        &brewPidEnabled,
        
        // Steam Parameters
        &steamSetpoint,
        
        // Hardware Parameters
        &hardwareOledType,
        &hardwareOledEnabled,
        &hardwareOledAddress,
        
        // System Parameters
        &systemHostname,
        &systemOfflineMode,
        &systemLogLevel
    };
}