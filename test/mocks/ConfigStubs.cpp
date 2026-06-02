/**
 * @file ConfigStubs.cpp
 * @brief Stub implementations of Config helper functions for tests
 * 
 * Provides minimal implementations of functions from Config.cpp that are
 * needed by Config's constructor but don't need full functionality in tests.
 */

#include "clevercoffee/Config.h"
#include "clevercoffee/types/GlobalTypes.h"
#include <vector>
#include <utility>

// Stub implementations of the static option vector getters
// These are called by Config's constructor to initialize enum ParamDef options

const std::vector<std::pair<Hardware::SwitchType, String>>& getSwitchTypeOptions() {
    static const std::vector<std::pair<Hardware::SwitchType, String>> options = {
        {Hardware::SwitchType::MOMENTARY, "Momentary"},
        {Hardware::SwitchType::TOGGLE, "Toggle"}
    };
    return options;
}

const std::vector<std::pair<Hardware::SwitchMode, String>>& getSwitchModeOptions() {
    static const std::vector<std::pair<Hardware::SwitchMode, String>> options = {
        {Hardware::SwitchMode::NORMALLY_OPEN, "Normally Open"},
        {Hardware::SwitchMode::NORMALLY_CLOSED, "Normally Closed"}
    };
    return options;
}

const std::vector<std::pair<Hardware::RelayTriggerType, String>>& getRelayTriggerOptions() {
    static const std::vector<std::pair<Hardware::RelayTriggerType, String>> options = {
        {Hardware::RelayTriggerType::LOW_TRIGGER, "Low Trigger"},
        {Hardware::RelayTriggerType::HIGH_TRIGGER, "High Trigger"}
    };
    return options;
}

const std::vector<std::pair<System::DisplayTemplate, String>>& getDisplayTemplateOptions() {
    static const std::vector<std::pair<System::DisplayTemplate, String>> options = {
        {System::DisplayTemplate::STANDARD, "Standard"},
        {System::DisplayTemplate::MINIMAL, "Minimal"},
        {System::DisplayTemplate::TEMPERATURE_ONLY, "Temperature Only"},
        {System::DisplayTemplate::SCALE, "Scale"},
        {System::DisplayTemplate::UPRIGHT, "Upright"},
        {System::DisplayTemplate::MODERN, "Modern"}
    };
    return options;
}

const std::vector<std::pair<System::Language, String>>& getLanguageOptions() {
    static const std::vector<std::pair<System::Language, String>> options = {
        {System::Language::ENGLISH, "English"},
        {System::Language::GERMAN, "German"},
        {System::Language::SPANISH, "Spanish"}
    };
    return options;
}

const std::vector<std::pair<Hardware::OLEDType, String>>& getOledTypeOptions() {
    static const std::vector<std::pair<Hardware::OLEDType, String>> options = {
        {Hardware::OLEDType::SSD1306, "SSD1306"},
        {Hardware::OLEDType::SH1106, "SH1106"}
    };
    return options;
}

const std::vector<std::pair<Hardware::OLEDAddress, String>>& getOledAddressOptions() {
    static const std::vector<std::pair<Hardware::OLEDAddress, String>> options = {
        {Hardware::OLEDAddress::ADDR_3C, "0x3C"},
        {Hardware::OLEDAddress::ADDR_3D, "0x3D"}
    };
    return options;
}

const std::vector<std::pair<Hardware::TemperatureSensorType, String>>& getTemperatureSensorTypeOptions() {
    static const std::vector<std::pair<Hardware::TemperatureSensorType, String>> options = {
        {Hardware::TemperatureSensorType::TSIC_306, "TSIC 306"},
        {Hardware::TemperatureSensorType::DALLAS_DS18B20, "Dallas DS18B20"}
    };
    return options;
}

const std::vector<std::pair<Hardware::ScaleType, String>>& getScaleTypeOptions() {
    static const std::vector<std::pair<Hardware::ScaleType, String>> options = {
        {Hardware::ScaleType::HX711_DUAL, "HX711 (2 load cells)"},
        {Hardware::ScaleType::HX711_SINGLE, "HX711 (1 load cell)"},
        {Hardware::ScaleType::BLUETOOTH, "Bluetooth"}
    };
    return options;
}

const std::vector<std::pair<System::LogLevel, String>>& getLogLevelOptions() {
    static const std::vector<std::pair<System::LogLevel, String>> options = {
        {System::LogLevel::TRACE, "TRACE"},
        {System::LogLevel::DEBUG, "DEBUG"},
        {System::LogLevel::INFO, "INFO"},
        {System::LogLevel::WARNING, "WARNING"},
        {System::LogLevel::ERROR, "ERROR"},
        {System::LogLevel::FATAL, "FATAL"},
        {System::LogLevel::SILENT, "SILENT"}
    };
    return options;
}

const std::vector<std::pair<Process::BrewMode, String>>& getBrewModeOptions() {
    static const std::vector<std::pair<Process::BrewMode, String>> options = {
        {Process::BrewMode::MANUAL_BREW, "Manual"},
        {Process::BrewMode::AUTOMATIC_BREW, "Automatic"}
    };
    return options;
}

// Stub implementation of getAllConfigParams() - needed by Config methods
std::vector<ConfigParamDef*> Config::getAllConfigParams() {
    // Return empty vector for tests - Config tests don't need full parameter list
    return std::vector<ConfigParamDef*>();
}
