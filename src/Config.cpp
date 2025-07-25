/**
 * @file Config.cpp
 *
 * @brief Implementation of configuration management methods
 */

#include "Config.h"
#include "ParameterRegistry.h"
#include <functional>
#include <vector>

bool Config::validateAndApplyConfig(const JsonDocument& doc) {
    LOGF(INFO, "Validating and applying configuration");

    auto& registry = ParameterRegistry::getInstance();

    // Helper function to recursively extract all paths from JSON
    std::function<void(JsonVariantConst, const String&, std::vector<std::pair<String, JsonVariantConst>>&)> extractPaths = [&](JsonVariantConst obj, const String& prefix,
                                                                                                                               std::vector<std::pair<String, JsonVariantConst>>& paths) {
        if (obj.is<JsonObjectConst>()) {
            for (JsonPairConst pair : obj.as<JsonObjectConst>()) {
                String newPath = prefix.isEmpty() ? String(pair.key().c_str()) : prefix + "." + pair.key().c_str();
                extractPaths(pair.value(), newPath, paths);
            }
        }
        else {
            // Leaf value
            paths.emplace_back(prefix, obj);
        }
    };

    // Extract all paths from the document
    std::vector<std::pair<String, JsonVariantConst>> docPaths;
    extractPaths(doc.as<JsonVariantConst>(), "", docPaths);

    LOGF(DEBUG, "Found %d parameters in uploaded config", docPaths.size());

    // Validate and apply each parameter
    for (const auto& [path, value] : docPaths) {
        auto it = _configDefs.find(path.c_str());

        if (it == _configDefs.end()) {
            LOGF(WARNING, "Unknown parameter in config: %s - skipping", path.c_str());
            continue;
        }

        const ConfigDef& def = it->second;

        // Validate and apply based on type
        bool validationSuccess = false;
        try {
            switch (def.type) {
                case ConfigDef::BOOL:
                    if (value.is<bool>()) {
                        bool boolVal = value.as<bool>();
                        registry.setParameterValue(path.c_str(), boolVal);
                        validationSuccess = true;
                        LOGF(TRACE, "Applied bool %s = %s", path.c_str(), boolVal ? "true" : "false");
                    }
                    else {
                        LOGF(ERROR, "Invalid type for boolean parameter %s", path.c_str());
                    }
                    break;

                case ConfigDef::INT:
                    if (value.is<int>()) {
                        auto intVal = value.as<int>();
                        if (intVal >= def.minValue && intVal <= def.maxValue) {
                            registry.setParameterValue(path.c_str(), static_cast<double>(intVal));
                            validationSuccess = true;
                            LOGF(TRACE, "Applied int %s = %d", path.c_str(), intVal);
                        }
                        else {
                            LOGF(ERROR, "Value %d for %s outside range [%.2f, %.2f]", intVal, path.c_str(), def.minValue, def.maxValue);
                        }
                    }
                    else {
                        LOGF(ERROR, "Invalid type for integer parameter %s", path.c_str());
                    }
                    break;

                case ConfigDef::DOUBLE:
                    if (value.is<double>() || value.is<float>()) {
                        auto doubleVal = value.as<double>();
                        if (doubleVal >= def.minValue && doubleVal <= def.maxValue) {
                            registry.setParameterValue(path.c_str(), doubleVal);
                            validationSuccess = true;
                            LOGF(TRACE, "Applied double %s = %.4f", path.c_str(), doubleVal);
                        }
                        else {
                            LOGF(ERROR, "Value %.4f for %s outside range [%.2f, %.2f]", doubleVal, path.c_str(), def.minValue, def.maxValue);
                        }
                    }
                    else {
                        LOGF(ERROR, "Invalid type for double parameter %s", path.c_str());
                    }
                    break;

                case ConfigDef::STRING:
                    if (value.is<const char*>() || value.is<String>()) {
                        auto stringVal = value.as<String>();
                        if (stringVal.length() <= def.maxLength) {
                            registry.setParameterValue(path.c_str(), stringVal);
                            validationSuccess = true;
                            LOGF(TRACE, "Applied string %s = %s", path.c_str(), stringVal.c_str());
                        }
                        else {
                            LOGF(ERROR, "String value for %s too long: %d > %d", path.c_str(), stringVal.length(), def.maxLength);
                        }
                    }
                    else {
                        LOGF(ERROR, "Invalid type for string parameter %s", path.c_str());
                    }
                    break;
            }
        } catch (const std::exception& e) {
            LOGF(ERROR, "Exception applying parameter %s: %s", path.c_str(), e.what());
            validationSuccess = false;
        }

        if (!validationSuccess) {
            LOGF(ERROR, "Failed to validate parameter: %s", path.c_str());
            return false;
        }
    }

    LOGF(INFO, "Successfully validated and applied all configuration parameters");

    return true;
}
