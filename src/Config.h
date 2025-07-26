
/**
 * @file Config.h
 *
 * @brief Unified configuration management system
 */

#pragma once

#include "GlobalVariables.h"
#include "Logger.h"
#include "defaults.h"
#include "hardware/Relay.h"
#include "hardware/Switch.h"
#include <ArduinoJson.h>
#include <Preferences.h>
#include <functional>
#include <map>
#include <optional>

enum class ParamType { BOOL, INT, UINT8, DOUBLE, FLOAT, STRING, ENUM };

struct ParamDef {
    ParamType type;
    double minValue = 0.0;
    double maxValue = 0.0;
    size_t maxLength = 0;
    void* globalVar = nullptr;
    std::function<bool()> showCondition = []() { return true; };
    const char* displayName = "";
    const char* helpText = "";
    int section = 0;
    int position = 0;

    // Default values stored as variants
    bool defaultBool = false;
    int defaultInt = 0;
    uint8_t defaultUInt8 = 0;
    double defaultDouble = 0.0;
    float defaultFloat = 0.0f;
    ::String defaultString = "";

    // Enum support
    const char* const* enumOptions = nullptr;
    size_t enumCount = 0;

    static ParamDef Bool(bool* var, bool defaultVal, const char* name = "", int sec = 0, int pos = 0, const char* help = "") {
        ParamDef def;
        def.type = ParamType::BOOL;
        def.globalVar = var;
        def.defaultBool = defaultVal;
        def.displayName = name;
        def.helpText = help;
        def.section = sec;
        def.position = pos;
        return def;
    }

    static ParamDef Int(int* var, int defaultVal, int min, int max, const char* name = "", int sec = 0, int pos = 0, const char* help = "") {
        ParamDef def;
        def.type = ParamType::INT;
        def.minValue = min;
        def.maxValue = max;
        def.globalVar = var;
        def.defaultInt = defaultVal;
        def.displayName = name;
        def.helpText = help;
        def.section = sec;
        def.position = pos;
        return def;
    }

    static ParamDef Double(double* var, double defaultVal, double min, double max, const char* name = "", int sec = 0, int pos = 0, const char* help = "") {
        ParamDef def;
        def.type = ParamType::DOUBLE;
        def.minValue = min;
        def.maxValue = max;
        def.globalVar = var;
        def.defaultDouble = defaultVal;
        def.displayName = name;
        def.helpText = help;
        def.section = sec;
        def.position = pos;
        return def;
    }

    static ParamDef UInt8(uint8_t* var, uint8_t defaultVal, uint8_t min, uint8_t max, const char* name = "", int sec = 0, int pos = 0, const char* help = "") {
        ParamDef def;
        def.type = ParamType::UINT8;
        def.minValue = min;
        def.maxValue = max;
        def.globalVar = var;
        def.defaultUInt8 = defaultVal;
        def.displayName = name;
        def.helpText = help;
        def.section = sec;
        def.position = pos;
        return def;
    }

    static ParamDef Float(float* var, float defaultVal, float min, float max, const char* name = "", int sec = 0, int pos = 0, const char* help = "") {
        ParamDef def;
        def.type = ParamType::FLOAT;
        def.minValue = min;
        def.maxValue = max;
        def.globalVar = var;
        def.defaultFloat = defaultVal;
        def.displayName = name;
        def.helpText = help;
        def.section = sec;
        def.position = pos;
        return def;
    }

    static ParamDef String(::String* var, const ::String& defaultVal, size_t maxLen, const char* name = "", int sec = 0, int pos = 0, const char* help = "") {
        ParamDef def;
        def.type = ParamType::STRING;
        def.maxLength = maxLen;
        def.globalVar = var;
        def.defaultString = defaultVal;
        def.displayName = name;
        def.helpText = help;
        def.section = sec;
        def.position = pos;
        return def;
    }

    static ParamDef Enum(int* var, int defaultVal, const char* const* options, size_t optionCount, const char* name = "", int sec = 0, int pos = 0, const char* help = "") {
        ParamDef def;
        def.type = ParamType::ENUM;
        def.globalVar = var;
        def.defaultInt = defaultVal;
        def.enumOptions = options;
        def.enumCount = optionCount;
        def.displayName = name;
        def.helpText = help;
        def.section = sec;
        def.position = pos;
        return def;
    }

    // Method to add show condition
    ParamDef& condition(std::function<bool()> cond) {
        showCondition = cond;
        return *this;
    }
};

class Config {
    public:
        static Config& getInstance() {
            static Config instance;
            return instance;
        }

        bool begin() {
            initializeParams();
            initializeGlobalVariablesWithDefaults();
            loadFromNVS();
            LOG(INFO, "Configuration system initialized");
            return true;
        }

        // Type-safe parameter access
        template <typename T>
        T get(const ::String& path) const {
            auto it = _params.find(path.c_str());
            if (it != _params.end()) {
                const ParamDef& def = it->second;
                if (def.globalVar) {
                    if constexpr (std::is_same_v<T, bool>) {
                        return *static_cast<bool*>(def.globalVar);
                    } else if constexpr (std::is_same_v<T, int>) {
                        return *static_cast<int*>(def.globalVar);
                    } else if constexpr (std::is_same_v<T, uint8_t>) {
                        return *static_cast<uint8_t*>(def.globalVar);
                    } else if constexpr (std::is_same_v<T, double>) {
                        return *static_cast<double*>(def.globalVar);
                    } else if constexpr (std::is_same_v<T, float>) {
                        return *static_cast<float*>(def.globalVar);
                    } else if constexpr (std::is_same_v<T, ::String>) {
                        return *static_cast<::String*>(def.globalVar);
                    }
                }
            }
            return T{};
        }

        // Type-safe parameter setting with validation and NVS save
        template <typename T>
        bool set(const ::String& path, const T& value) {
            auto it = _params.find(path.c_str());
            if (it == _params.end()) return false;

            const ParamDef& def = it->second;

            // Validate numeric ranges
            if constexpr (std::is_arithmetic_v<T>) {
                if (def.minValue != def.maxValue) { // Only validate if range is set
                    if (value < static_cast<T>(def.minValue) || value > static_cast<T>(def.maxValue)) return false;
                }
            }
            if constexpr (std::is_same_v<T, ::String>) {
                if (def.maxLength > 0 && value.length() > def.maxLength) return false;
            }

            // Update global variable
            if (def.globalVar) {
                *static_cast<T*>(def.globalVar) = value;
                saveToNVS(path.c_str(), value);
                return true;
            }
            return false;
        }

        // JSON import/export
        bool loadFromJson(const ::String& jsonString);
        ::String exportToJson() const;

        // API support
        JsonDocument getParametersForAPI(const ::String& section = "") const;

        // Compatibility methods for existing webserver
        bool validateAndApplyFromJson(const ::String& jsonString) { return loadFromJson(jsonString); }
        void syncGlobalVariables() { /* Already handled by direct binding */ }
        ::String generateJsonConfig() const { return exportToJson(); }

        // NVS operations
        void loadFromNVS();
        void saveToNVS();

        // Reset parameter to default value
        bool resetToDefault(const ::String& path);

        // Reset all parameters to defaults
        void resetAllToDefaults();

        // Check if parameter exists
        bool hasParameter(const ::String& key) const {
            return _params.find(key.c_str()) != _params.end();
        }

        // Try to get parameter value, returns false if parameter doesn't exist or type mismatch
        template<typename T>
        bool tryGet(const ::String& key, T& value) const {
            auto it = _params.find(key.c_str());
            if (it == _params.end()) {
                return false;
            }

            const ParamDef& def = it->second;
            if (!def.globalVar) {
                return false;
            }

            try {
                if constexpr (std::is_same_v<T, bool>) {
                    if (def.type == ParamType::BOOL) {
                        value = *static_cast<bool*>(def.globalVar);
                        return true;
                    }
                } else if constexpr (std::is_same_v<T, int>) {
                    if (def.type == ParamType::INT) {
                        value = *static_cast<int*>(def.globalVar);
                        return true;
                    }
                } else if constexpr (std::is_same_v<T, uint8_t>) {
                    if (def.type == ParamType::UINT8) {
                        value = *static_cast<uint8_t*>(def.globalVar);
                        return true;
                    }
                } else if constexpr (std::is_same_v<T, double>) {
                    if (def.type == ParamType::DOUBLE) {
                        value = *static_cast<double*>(def.globalVar);
                        return true;
                    }
                } else if constexpr (std::is_same_v<T, float>) {
                    if (def.type == ParamType::FLOAT) {
                        value = *static_cast<float*>(def.globalVar);
                        return true;
                    }
                } else if constexpr (std::is_same_v<T, ::String>) {
                    if (def.type == ParamType::STRING) {
                        value = *static_cast<::String*>(def.globalVar);
                        return true;
                    }
                }
                return false;
            } catch (...) {
                return false;
            }
        }

        // Get parameters map for webserver compatibility
        const std::map<std::string, ParamDef>& getParameters() const {
            return _params;
        }

    private:
        std::map<std::string, ParamDef> _params;
        Preferences _prefs;

        void initializeParams();
        void initializeGlobalVariablesWithDefaults();

        template<typename T>
        void saveToNVS(const char* path, const T& value) {
            _prefs.begin("config", false);
            if constexpr (std::is_same_v<T, bool>) {
                _prefs.putBool(path, value);
            } else if constexpr (std::is_same_v<T, int>) {
                _prefs.putInt(path, value);
            } else if constexpr (std::is_same_v<T, uint8_t>) {
                _prefs.putUChar(path, value);
            } else if constexpr (std::is_same_v<T, double>) {
                _prefs.putDouble(path, value);
            } else if constexpr (std::is_same_v<T, float>) {
                _prefs.putFloat(path, value);
            } else if constexpr (std::is_same_v<T, ::String>) {
                _prefs.putString(path, value);
            }
            _prefs.end();
        }
};
