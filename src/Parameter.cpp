#include "Parameter.h"
#include "Logger.h"
#include <ArduinoJson.h>
#include <Preferences.h>
#include <cmath>

static Preferences paramPrefs;

// Constructor for general parameters with string getters/setters
Parameter::Parameter(const char* id,
                     const char* displayName,
                     EditableKind type,
                     int section,
                     int position,
                     std::function<double()> getter,
                     std::function<void(double)> setter,
                     double minValue,
                     double maxValue,
                     bool hasHelpText,
                     const char* helpText,
                     std::function<bool()> showCondition,
                     std::function<String()> stringGetter,
                     std::function<void(const String&)> stringSetter,
                     void* globalVariablePointer) :
    _id(id),
    _displayName(displayName),
    _type(type),
    _section(section),
    _position(position),
    _getter(std::move(getter)),
    _setter(std::move(setter)),
    _enumOptions(nullptr),
    _enumCount(0),
    _minValue(minValue),
    _maxValue(maxValue),
    _hasHelpText(hasHelpText),
    _helpText(helpText),
    _showCondition(std::move(showCondition)),
    _stringGetter(std::move(stringGetter)),
    _stringSetter(std::move(stringSetter)),
    _globalVariablePointer(globalVariablePointer) {
}

// Constructor for string parameters
Parameter::Parameter(const char* id,
                     const char* displayName,
                     EditableKind type,
                     int section,
                     int position,
                     const std::function<String()>& stringGetter,
                     const std::function<void(const String&)>& stringSetter,
                     double maxLength,
                     bool hasHelpText,
                     const char* helpText,
                     const std::function<bool()>& showCondition,
                     void* globalVariablePointer) :
    _id(id),
    _displayName(displayName),
    _type(type),
    _section(section),
    _position(position),
    _getter(nullptr),
    _setter(nullptr),
    _enumOptions(nullptr),
    _enumCount(0),
    _minValue(0),
    _maxValue(maxLength),
    _hasHelpText(hasHelpText),
    _helpText(helpText),
    _showCondition(showCondition),
    _stringGetter(stringGetter),
    _stringSetter(stringSetter),
    _globalVariablePointer(globalVariablePointer) {
}

// Constructor for numeric parameters (no string getter/setter)
Parameter::Parameter(const char* id,
                     const char* displayName,
                     EditableKind type,
                     int section,
                     int position,
                     std::function<double()> getter,
                     std::function<void(double)> setter,
                     double minValue,
                     double maxValue,
                     bool hasHelpText,
                     const char* helpText,
                     std::function<bool()> showCondition,
                     void* globalVariablePointer) :
    _id(id),
    _displayName(displayName),
    _type(type),
    _section(section),
    _position(position),
    _getter(std::move(getter)),
    _setter(std::move(setter)),
    _enumOptions(nullptr),
    _enumCount(0),
    _minValue(minValue),
    _maxValue(maxValue),
    _hasHelpText(hasHelpText),
    _helpText(helpText),
    _showCondition(std::move(showCondition)),
    _stringGetter(nullptr),
    _stringSetter(nullptr),
    _globalVariablePointer(globalVariablePointer) {
}

// Constructor for boolean parameters (using kUInt8 type with 0/1 values)
Parameter::Parameter(const char* id,
                     const char* displayName,
                     EditableKind type,
                     int section,
                     int position,
                     const std::function<bool()>& boolGetter,
                     const std::function<void(bool)>& boolSetter,
                     bool hasHelpText,
                     const char* helpText,
                     const std::function<bool()>& showCondition,
                     void* globalVariablePointer) :
    _id(id),
    _displayName(displayName),
    _type(type),
    _section(section),
    _position(position),
    _getter([boolGetter] { return boolGetter() ? 1.0 : 0.0; }),
    _setter([boolSetter](const double val) { boolSetter(val > 0.5); }),
    _enumOptions(nullptr),
    _enumCount(0),
    _minValue(0),
    _maxValue(1),
    _hasHelpText(hasHelpText),
    _helpText(helpText),
    _showCondition(showCondition),
    _stringGetter(nullptr),
    _stringSetter(nullptr),
    _globalVariablePointer(globalVariablePointer) {
}

// Constructor for enum parameters
Parameter::Parameter(const char* id,
                     const char* displayName,
                     EditableKind type,
                     int section,
                     int position,
                     const std::function<double()>& getter,
                     const std::function<void(double)>& setter,
                     const char* const* enumOptions,
                     size_t enumCount,
                     bool hasHelpText,
                     const char* helpText,
                     const std::function<bool()>& showCondition,
                     void* globalVariablePointer) :
    _id(id),
    _displayName(displayName),
    _type(type),
    _section(section),
    _position(position),
    _getter(getter),
    _setter(setter),
    _enumOptions(enumOptions),
    _enumCount(enumCount),
    _minValue(0),
    _maxValue(static_cast<double>(enumCount - 1)),
    _hasHelpText(hasHelpText),
    _helpText(helpText),
    _showCondition(showCondition),
    _stringGetter(nullptr),
    _stringSetter(nullptr),
    _globalVariablePointer(globalVariablePointer) {
}

const char* Parameter::getId() const {
    return _id;
}

const char* Parameter::getDisplayName() const {
    return _displayName;
}

EditableKind Parameter::getType() const {
    return _type;
}

int Parameter::getSection() const {
    return _section;
}

int Parameter::getPosition() const {
    return _position;
}

double Parameter::getValue() const {
    return _getter();
}

void Parameter::setValue(const double value) const {
    double currentValue = _getter();
    double difference = std::abs(currentValue - value);

    // Add debugging
    LOGF(DEBUG, "Parameter::setValue(%s): current=%.6f, new=%.6f, diff=%.9f", _id, currentValue, value, difference);

    // Check if value has actually changed
    if (difference < 1e-9) {
        LOGF(DEBUG, "Parameter::setValue(%s): No change detected, skipping save", _id);
        return; // No change, don't write to flash
    }

    LOGF(INFO, "Parameter::setValue(%s): Setting value and saving", _id);
    _setter(value);
    syncToGlobalVariable(value);
    saveToPreferences();
}

String Parameter::getStringValue() const {
    if (_stringGetter) {
        return _stringGetter();
    }
    return {};
}

void Parameter::setStringValue(const String& value) const {
    if (_stringSetter) {
        String currentValue = _stringGetter ? _stringGetter() : "";

        // Add debugging
        LOGF(DEBUG, "Parameter::setStringValue(%s): current='%s', new='%s'", _id, currentValue.c_str(), value.c_str());

        // Check if string value has actually changed
        if (_stringGetter && currentValue == value) {
            LOGF(DEBUG, "Parameter::setStringValue(%s): No change detected, skipping save", _id);
            return; // No change, don't write to flash
        }

        LOGF(INFO, "Parameter::setStringValue(%s): Setting value and saving", _id);
        _stringSetter(value);
        syncToGlobalVariable(value);
        saveToPreferences();
    }
}

// Internal method for loading values without triggering save logic
void Parameter::setValueForLoading(const double value) const {
    LOGF(TRACE, "Parameter::setValueForLoading(%s): Loading value %.6f (no save)", _id, value);
    _setter(value);
    syncToGlobalVariable(value);
}

// Internal method for loading string values without triggering save logic
void Parameter::setStringValueForLoading(const String& value) const {
    if (_stringSetter) {
        LOGF(TRACE, "Parameter::setStringValueForLoading(%s): Loading string '%s' (no save)", _id, value.c_str());
        _stringSetter(value);
        syncToGlobalVariable(value);
    }
}

double Parameter::getMinValue() const {
    return _minValue;
}

double Parameter::getMaxValue() const {
    return _maxValue;
}

bool Parameter::hasHelpText() const {
    return _hasHelpText;
}

const char* Parameter::getHelpText() const {
    return _helpText;
}

bool Parameter::shouldShow() const {
    return _showCondition();
}

String Parameter::getFormattedValue() const {
    switch (_type) {
        case kFloat:
            return String(getValueAs<float>());

        case kDouble:
            return String(getValueAs<double>());

        case kInteger:
            return String(getValueAs<int>());

        case kUInt8:
            return String(getValueAs<uint8_t>());

        case kCString:
            return getValueAs<String>();

        case kEnum:
            return getEnumDisplayValue();

        default:
            return "Unknown type";
    }
}

const char* const* Parameter::getEnumOptions() const {
    return _enumOptions;
}

size_t Parameter::getEnumCount() const {
    return _enumCount;
}

bool Parameter::isEnum() const {
    return _type == kEnum;
}

String Parameter::getEnumDisplayValue() const {
    if (!isEnum() || _enumOptions == nullptr) {
        return "";
    }

    const int index = static_cast<int>(getValue());

    return index >= 0 && index < static_cast<int>(_enumCount) ? String(_enumOptions[index]) : "";
}

void* Parameter::getGlobalVariablePointer() const {
    return _globalVariablePointer;
}

void Parameter::setGlobalVariablePointer(void* ptr) {
    _globalVariablePointer = ptr;
}

void Parameter::syncToGlobalVariable(const double value) const {
    if (_globalVariablePointer == nullptr) return;

    switch (_type) {
        case kInteger:
            *static_cast<int*>(_globalVariablePointer) = static_cast<int>(value);
            break;

        case kUInt8:
            *static_cast<uint8_t*>(_globalVariablePointer) = static_cast<uint8_t>(value);
            break;

        case kDouble:
            *static_cast<double*>(_globalVariablePointer) = value;
            break;

        case kFloat:
            *static_cast<float*>(_globalVariablePointer) = static_cast<float>(value);
            break;

        case kEnum:
            *static_cast<int*>(_globalVariablePointer) = static_cast<int>(value);
            break;

        case kCString:
            break;
    }
}

void Parameter::syncToGlobalVariable(const String& value) const {
    if (_globalVariablePointer == nullptr) {
        return;
    }

    if (_type == kCString) {
        *static_cast<String*>(_globalVariablePointer) = value;
    }
}

// Generate a short key for NVS storage (max 15 chars)
String Parameter::generateNvsKey() const {
    // Simple hash-based approach to create short unique keys
    uint32_t hash = 0;
    const char* str = _id;
    while (*str) {
        hash = hash * 31 + *str++;
    }

    // Create a key with prefix + hash (max 15 chars)
    String key = "p" + String(hash, HEX);
    if (key.length() > 15) {
        key = key.substring(0, 15);
    }

    LOGF(DEBUG, "Parameter::generateNvsKey(%s): Using NVS key '%s'", _id, key.c_str());
    return key;
}

// Save this parameter to NVS
void Parameter::saveToPreferences() const {
    LOGF(INFO, "Parameter::saveToPreferences(%s): Starting NVS save", _id);
    String nvsKey = generateNvsKey();
    paramPrefs.begin("params", false);
    switch (_type) {
        case kInteger:
        case kEnum:
            {
                int intVal = getValueAs<int>();
                LOGF(INFO, "Parameter::saveToPreferences(%s): Saving int value %d", _id, intVal);
                size_t bytesWritten = paramPrefs.putInt(nvsKey.c_str(), intVal);
                if (bytesWritten == 0) {
                    LOGF(ERROR, "Parameter::saveToPreferences(%s): Failed to write int value to NVS (key: %s)", _id, nvsKey.c_str());
                }
                else {
                    LOGF(DEBUG, "Parameter::saveToPreferences(%s): Successfully wrote %d bytes (key: %s)", _id, bytesWritten, nvsKey.c_str());
                }
            }
            break;
        case kUInt8:
            {
                uint8_t uintVal = getValueAs<uint8_t>();
                LOGF(INFO, "Parameter::saveToPreferences(%s): Saving uint8 value %d", _id, uintVal);
                size_t bytesWritten = paramPrefs.putUChar(nvsKey.c_str(), uintVal);
                if (bytesWritten == 0) {
                    LOGF(ERROR, "Parameter::saveToPreferences(%s): Failed to write uint8 value to NVS (key: %s)", _id, nvsKey.c_str());
                }
                else {
                    LOGF(DEBUG, "Parameter::saveToPreferences(%s): Successfully wrote %d bytes (key: %s)", _id, bytesWritten, nvsKey.c_str());
                }
            }
            break;
        case kDouble:
            {
                double doubleVal = getValueAs<double>();
                LOGF(INFO, "Parameter::saveToPreferences(%s): Saving double value %.6f", _id, doubleVal);
                size_t bytesWritten = paramPrefs.putDouble(nvsKey.c_str(), doubleVal);
                if (bytesWritten == 0) {
                    LOGF(ERROR, "Parameter::saveToPreferences(%s): Failed to write double value to NVS (key: %s)", _id, nvsKey.c_str());
                }
                else {
                    LOGF(DEBUG, "Parameter::saveToPreferences(%s): Successfully wrote %d bytes (key: %s)", _id, bytesWritten, nvsKey.c_str());
                }
            }
            break;
        case kFloat:
            {
                float floatVal = getValueAs<float>();
                LOGF(INFO, "Parameter::saveToPreferences(%s): Saving float value %.6f", _id, floatVal);
                size_t bytesWritten = paramPrefs.putFloat(nvsKey.c_str(), floatVal);
                if (bytesWritten == 0) {
                    LOGF(ERROR, "Parameter::saveToPreferences(%s): Failed to write float value to NVS (key: %s)", _id, nvsKey.c_str());
                }
                else {
                    LOGF(DEBUG, "Parameter::saveToPreferences(%s): Successfully wrote %d bytes (key: %s)", _id, bytesWritten, nvsKey.c_str());
                }
            }
            break;
        case kCString:
            {
                String stringVal = getStringValue();
                LOGF(INFO, "Parameter::saveToPreferences(%s): Saving string value '%s'", _id, stringVal.c_str());
                size_t bytesWritten = paramPrefs.putString(nvsKey.c_str(), stringVal);
                if (bytesWritten == 0) {
                    LOGF(ERROR, "Parameter::saveToPreferences(%s): Failed to write string value to NVS (key: %s)", _id, nvsKey.c_str());
                }
                else {
                    LOGF(DEBUG, "Parameter::saveToPreferences(%s): Successfully wrote %d bytes (key: %s)", _id, bytesWritten, nvsKey.c_str());
                }
            }
            break;
    }
    paramPrefs.end();
    LOGF(INFO, "Parameter::saveToPreferences(%s): NVS save completed", _id);
}

// Load this parameter from NVS
void Parameter::loadFromPreferences() const {
    paramPrefs.begin("params", true);
    String nvsKey = generateNvsKey();

    switch (_type) {
        case kInteger:
        case kEnum:
            {
                int currentVal = getValueAs<int>();
                int nvsVal = paramPrefs.getInt(nvsKey.c_str(), currentVal);
                if (nvsVal != currentVal) {
                    LOGF(INFO, "Parameter::loadFromPreferences(%s): Loading int value %d from NVS (current: %d)", _id, nvsVal, currentVal);
                    // Use loading method to avoid save during boot
                    setValueForLoading(static_cast<double>(nvsVal));
                }
                else {
                    LOGF(DEBUG, "Parameter::loadFromPreferences(%s): NVS value matches current value %d", _id, currentVal);
                }
                break;
            }
        case kUInt8:
            {
                uint8_t currentVal = getValueAs<uint8_t>();
                uint8_t nvsVal = paramPrefs.getUChar(nvsKey.c_str(), currentVal);
                if (nvsVal != currentVal) {
                    LOGF(INFO, "Parameter::loadFromPreferences(%s): Loading uint8 value %d from NVS (current: %d)", _id, nvsVal, currentVal);
                    // Use loading method to avoid save during boot
                    setValueForLoading(static_cast<double>(nvsVal));
                }
                else {
                    LOGF(DEBUG, "Parameter::loadFromPreferences(%s): NVS value matches current value %d", _id, currentVal);
                }
                break;
            }
        case kDouble:
            {
                double currentVal = getValueAs<double>();
                double nvsVal = paramPrefs.getDouble(nvsKey.c_str(), currentVal);
                if (std::abs(nvsVal - currentVal) > 1e-9) {
                    LOGF(INFO, "Parameter::loadFromPreferences(%s): Loading double value %.6f from NVS (current: %.6f)", _id, nvsVal, currentVal);
                    // Use loading method to avoid save during boot
                    setValueForLoading(nvsVal);
                }
                else {
                    LOGF(DEBUG, "Parameter::loadFromPreferences(%s): NVS value matches current value %.6f", _id, currentVal);
                }
                break;
            }
        case kFloat:
            {
                float currentVal = getValueAs<float>();
                float nvsVal = paramPrefs.getFloat(nvsKey.c_str(), currentVal);
                if (std::abs(nvsVal - currentVal) > 1e-6f) {
                    LOGF(INFO, "Parameter::loadFromPreferences(%s): Loading float value %.6f from NVS (current: %.6f)", _id, nvsVal, currentVal);
                    // Use loading method to avoid save during boot
                    setValueForLoading(static_cast<double>(nvsVal));
                }
                else {
                    LOGF(DEBUG, "Parameter::loadFromPreferences(%s): NVS value matches current value %.6f", _id, currentVal);
                }
                break;
            }
        case kCString:
            {
                String currentVal = getStringValue();
                String nvsVal = paramPrefs.getString(nvsKey.c_str(), currentVal);
                if (nvsVal != currentVal) {
                    LOGF(INFO, "Parameter::loadFromPreferences(%s): Loading string value '%s' from NVS (current: '%s')", _id, nvsVal.c_str(), currentVal.c_str());
                    // Use loading method to avoid save during boot
                    setStringValueForLoading(nvsVal);
                }
                else {
                    LOGF(DEBUG, "Parameter::loadFromPreferences(%s): NVS value matches current value '%s'", _id, currentVal.c_str());
                }
                break;
            }
    }

    paramPrefs.end();
}

// Helper: Save all parameters in a vector to NVS
void Parameter::saveAllToPreferences(const std::vector<Parameter*>& params) {
    for (const auto* param : params) {
        param->saveToPreferences();
    }
}

// Helper: Load all parameters in a vector from NVS
void Parameter::loadAllFromPreferences(const std::vector<Parameter*>& params) {
    for (const auto* param : params) {
        param->loadFromPreferences();
    }
}

// Static: Generate JSON for all parameters
String Parameter::generateJsonConfig(const std::vector<Parameter*>& params) {
    JsonDocument doc;
    for (const auto* param : params) {
        switch (param->getType()) {
            case kInteger:
            case kEnum:
                doc[param->getId()] = param->getValueAs<int>();
                break;
            case kUInt8:
                doc[param->getId()] = param->getValueAs<uint8_t>();
                break;
            case kDouble:
                doc[param->getId()] = param->getValueAs<double>();
                break;
            case kFloat:
                doc[param->getId()] = param->getValueAs<float>();
                break;
            case kCString:
                doc[param->getId()] = param->getStringValue();
                break;
        }
    }
    String output;
    serializeJson(doc, output);
    return output;
}
