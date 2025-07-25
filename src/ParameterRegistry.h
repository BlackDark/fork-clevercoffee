
#pragma once

#include "Config.h"
#include "Parameter.h"
#include <Preferences.h>
#include <map>
#include <memory>
#include <vector>

enum ParameterSection {
    sPIDSection = 0,
    sTempSection = 1,
    sBrewPidSection = 2,
    sBrewSection = 3,
    sScaleSection = 4,
    sDisplaySection = 5,
    sMaintenanceSection = 6,
    sPowerSection = 7,
    sMqttSection = 8,
    sSystemSection = 9,
    sOtherSection = 10,
    sHardwareOledSection = 11,
    sHardwareRelaySection = 12,
    sHardwareSwitchSection = 13,
    sHardwareLedSection = 14,
    sHardwareSensorSection = 15
};

inline const char* getSectionName(const int sectionId) {
    switch (sectionId) {
        case sPIDSection:
            return "PID Controller";
        case sTempSection:
            return "Temperature";
        case sBrewSection:
            return "Brew Control";
        case sBrewPidSection:
            return "Brew PID";
        case sDisplaySection:
            return "Display";
        case sPowerSection:
            return "Power Management";
        case sScaleSection:
            return "Scale";
        case sMaintenanceSection:
            return "Maintenance";
        case sMqttSection:
            return "MQTT";
        case sSystemSection:
            return "System";
        case sOtherSection:
            return "Other";
        case sHardwareOledSection:
            return "OLED";
        case sHardwareRelaySection:
            return "Relays";
        case sHardwareSwitchSection:
            return "Switches";
        case sHardwareLedSection:
            return "LEDs";
        case sHardwareSensorSection:
            return "Sensors";
        default:
            return "Unknown Section";
    }
}

class ParameterRegistry {
    private:
        ParameterRegistry() :
            _ready(false), _config(nullptr) {
        }

        static ParameterRegistry _singleton;

        bool _ready;

        std::vector<std::shared_ptr<Parameter>> _parameters;
        std::map<std::string, std::shared_ptr<Parameter>> _parameterMap;
        Config* _config;

        void addParam(const std::shared_ptr<Parameter>& param) {
            _parameters.push_back(param);
            _parameterMap[param->getId()] = param;
        }

    public:
        static ParameterRegistry& getInstance() {
            return _singleton;
        }

        [[nodiscard]] bool isReady() const {
            return _ready;
        }

        void initialize(Config& config);

        [[nodiscard]] const std::vector<std::shared_ptr<Parameter>>& getParameters() const {
            return _parameters;
        }

        void syncGlobalVariables() const;

        std::shared_ptr<Parameter> getParameterById(const char* id);

        template <typename T>
        bool setParameterValue(const char* id, const T& value) {
            const auto param = getParameterById(id);

            if (!param) {
                LOGF(WARNING, "ParameterRegistry::setParameterValue: Parameter '%s' not found", id);
                return false;
            }

            LOGF(INFO, "ParameterRegistry::setParameterValue: Setting parameter '%s'", id);

            if constexpr (std::is_same_v<T, String> || std::is_same_v<T, std::string>) {
                // Handle string parameters
                if (param->getType() == kCString) {
                    String newValue = String(value);
                    LOGF(INFO, "ParameterRegistry::setParameterValue: Setting string parameter '%s' = '%s'", id, newValue.c_str());
                    param->setStringValue(newValue);
                }
                else {
                    const double numericValue = value.toDouble();
                    LOGF(INFO, "ParameterRegistry::setParameterValue: Converting string '%s' to numeric %.6f for parameter '%s'", String(value).c_str(), numericValue, id);
                    param->setValue(numericValue);
                }
            }
            else if constexpr (std::is_same_v<T, bool>) {
                // Handle boolean parameters
                double boolValue = value ? 1.0 : 0.0;
                LOGF(INFO, "ParameterRegistry::setParameterValue: Setting boolean parameter '%s' = %.0f", id, boolValue);
                param->setValue(boolValue);
            }
            else {
                // Handle all numeric types (int, float, double, uint8_t, etc.)
                double numericValue = static_cast<double>(value);
                LOGF(INFO, "ParameterRegistry::setParameterValue: Setting numeric parameter '%s' = %.6f", id, numericValue);
                param->setValue(numericValue);
            }

            LOGF(INFO, "ParameterRegistry::setParameterValue: Successfully set parameter '%s'", id);
            return true;
        }

        void forceSave() {
            // Save all parameters to NVS
            saveAllToPreferences();
            LOG(INFO, "Configuration forcibly saved to NVS");
        }

        // NVS/Preferences methods
        void loadAllFromPreferences() {
            std::vector<Parameter*> params;
            for (const auto& param : _parameters) {
                params.push_back(param.get());
            }
            Parameter::loadAllFromPreferences(params);
        }

        void saveAllToPreferences() {
            std::vector<Parameter*> params;
            for (const auto& param : _parameters) {
                params.push_back(param.get());
            }
            Parameter::saveAllToPreferences(params);
        }

        String generateJsonConfig() {
            std::vector<Parameter*> params;
            for (const auto& param : _parameters) {
                params.push_back(param.get());
            }
            return Parameter::generateJsonConfig(params);
        }

        // Convenience method for adding string config parameters
        void addStringConfigParam(
            const char* configPath, const char* displayName, int section, int position, String* globalVar, double maxLength, const char* helpText = "", const std::function<bool()>& showCondition = [] { return true; }) {

            const auto param = std::make_shared<Parameter>(
                configPath, displayName, kCString, section, position, [globalVar]() -> String { return globalVar ? *globalVar : String(); },
                [globalVar](const String& val) {
                    if (globalVar) *globalVar = val;
                },
                maxLength, !String(helpText).isEmpty(), helpText, showCondition, globalVar);

            addParam(param);
        }

        // Convenience method for adding boolean config parameters
        void addBoolConfigParam(const char* configPath, const char* displayName, int section, int position, bool* globalVar, const char* helpText = "", const std::function<bool()>& showCondition = [] { return true; }) {

            const auto param = std::make_shared<Parameter>(
                configPath, displayName, kUInt8, section, position, [globalVar]() -> bool { return globalVar ? *globalVar : false; },
                [globalVar](const bool val) {
                    if (globalVar) *globalVar = val;
                },
                !String(helpText).isEmpty(), helpText, showCondition, globalVar);

            addParam(param);
        }

        // Convenience method for adding numeric config parameters
        template <typename T>
        void addNumericConfigParam(
            const char* configPath, const char* displayName, EditableKind type, int section, int position, T* globalVar, double minValue, double maxValue, const char* helpText = "", std::function<bool()> showCondition = [] {
                return true;
            }) {

            auto param = std::make_shared<Parameter>(
                configPath, displayName, type, section, position, [globalVar]() -> double { return globalVar ? static_cast<double>(*globalVar) : 0.0; },
                [globalVar](const double val) {
                    T typedVal = static_cast<T>(val);
                    if (globalVar) *globalVar = typedVal;
                },
                minValue, maxValue, !String(helpText).isEmpty(), helpText, showCondition, globalVar);

            addParam(param);
        }

        // Convenience method for adding enum config parameters
        void addEnumConfigParam(
            const char* configPath,
            const char* displayName,
            int section,
            int position,
            int* globalVar,
            const char* const options[],
            int optionCount,
            const char* helpText = "",
            const std::function<bool()>& showCondition = [] { return true; }) {

            const auto param = std::make_shared<Parameter>(
                configPath, displayName, kEnum, section, position, [globalVar]() -> double { return globalVar ? static_cast<double>(*globalVar) : 0.0; },
                [globalVar](const double val) {
                    const int intVal = static_cast<int>(val);
                    if (globalVar) *globalVar = intVal;
                },
                options, optionCount, !String(helpText).isEmpty(), helpText, showCondition, globalVar);

            addParam(param);
        }
};
