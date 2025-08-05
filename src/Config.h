/**
 * @file Config.h
 * @brief Next-generation type-safe configuration system with direct parameter access
 *
 * Design principles:
 * - Type-safe parameter access without global variable linking
 * - Separation of editable parameters and read-only state values
 * - Direct parameter access: Config::getInstance().pidEnabled.get()
 * - Automatic NVS persistence and validation
 * - No need for string-based parameter lookups in most cases
 */

#pragma once

#include "defaults.h"
#include "state/GlobalState.h"
#include "Logger.h"
#include <Arduino.h>
#include <ArduinoJson.h>
#include <Preferences.h>
#include <functional>
#include <map>
#include <memory>
#include <type_traits>
#include <vector>

// ParamType enum for compatibility with old Config system
enum class ParamType {
    INT = 0,
    UINT8 = 1,
    DOUBLE = 2,
    FLOAT = 3,
    STRING = 4,
    ENUM = 5,
    BOOL = 6
};

// Extern declarations for option vectors (defined in Config.cpp)
extern std::vector<std::pair<Hardware::SwitchType, String>> switchTypeOptions;
extern std::vector<std::pair<Hardware::SwitchMode, String>> switchModeOptions;
extern std::vector<std::pair<Hardware::RelayTriggerType, String>> relayTriggerOptions;
extern std::vector<std::pair<System::DisplayTemplate, String>> displayTemplateOptions;
extern std::vector<std::pair<System::Language, String>> languageOptions;
extern std::vector<std::pair<Hardware::OLEDType, String>> oledTypeOptions;
extern std::vector<std::pair<Hardware::OLEDAddress, String>> oledAddressOptions;
extern std::vector<std::pair<Hardware::TemperatureSensorType, String>> temperatureSensorTypeOptions;
extern std::vector<std::pair<Hardware::ScaleType, String>> scaleTypeOptions;
extern std::vector<std::pair<System::LogLevel, String>> logLevelOptions;
extern std::vector<std::pair<Process::BrewMode, String>> brewModeOptions;

// Forward declarations
template <typename T>
class ParamDef;

template <typename T>
class StateParamDef;

/**
 * @brief Base class for all parameter definitions
 */
class BaseParamDef {
    public:
        BaseParamDef(const String& key, const String& displayName, int section, int order, const String& helpText) :
            key_(key), displayName_(displayName), section_(section), order_(order), helpText_(helpText) {
        }

        virtual ~BaseParamDef() = default;

        const String& getKey() const {
            return key_;
        }
        const String& getDisplayName() const {
            return displayName_;
        }
        int getSection() const {
            return section_;
        }
        int getOrder() const {
            return order_;
        }
        const String& getHelpText() const {
            return helpText_;
        }

        virtual void toJson(JsonObject& obj) const = 0;
        virtual ParamType getParamType() const = 0;

    protected:
        String key_;
        String displayName_;
        int section_;
        int order_;
        String helpText_;

        const void toJsonBase(JsonObject& obj) const {
            obj["name"] = key_;
            obj["label"] = displayName_;
            obj["section"] = section_;
            obj["order"] = order_;
            obj["helpText"] = helpText_;
            obj["type"] = static_cast<int>(getParamType());
            //obj["id"] = key_;
            //obj["key"] = key_;
            // obj["type"] = getTypeName();
        }
};

/**
 * @brief Base class for all parameter definitions
 */
class ConfigParamDef : public BaseParamDef {
    public:
        ConfigParamDef(const String& key, const String& displayName, int section, int order, const String& helpText) :
            BaseParamDef(key, displayName, section, order, helpText){
        }

        virtual ~ConfigParamDef() = default;

        virtual bool fromString(const String& value) = 0;
        virtual bool loadFromNvs(Preferences& prefs) = 0;
        virtual bool saveToNvs(Preferences& prefs) const = 0;
        virtual void resetToDefault() = 0;
};

/**
 * @brief Type-safe parameter definition for editable configuration values
 */
template <typename T>
class ParamDef : public ConfigParamDef {
    public:
        ParamDef(const String& key, T defaultValue, const String& displayName, int section, int order, const String& helpText, T minValue = T{}, T maxValue = T{}, std::function<bool()> showCondition = nullptr) :
            ConfigParamDef(key, displayName, section, order, helpText), defaultValue_(defaultValue), currentValue_(defaultValue), minValue_(minValue), maxValue_(maxValue), showCondition_(showCondition) {
        }

        // Type-safe getter
        T get() const {
            return currentValue_;
        }

        // Type-safe setter with validation
        bool set(const T& value) {
            if (!isValid(value)) {
                LOGF(WARNING, "Invalid value for parameter '%s'", key_.c_str());
                return false;
            }
            currentValue_ = value;

            // Automatically save to NVS
            Preferences prefs;
            if (prefs.begin(STORAGE_NAMESPACE, false)) { // Read-write mode
                bool saveSuccess = saveToNvs(prefs);
                prefs.end();
                if (!saveSuccess) {
                    LOGF(ERROR, "Failed to save parameter '%s' to NVS", key_.c_str());
                    return false;
                } else {
                    LOGF(DEBUG, "Parameter '%s' successfully saved to NVS", key_.c_str());
                }
            } else {
                LOGF(ERROR, "Failed to open NVS namespace '%s' for parameter '%s'", STORAGE_NAMESPACE, key_.c_str());
                return false;
            }

            return true;
        }        // Validation
        bool isValid(const T& value) const {
            if constexpr (std::is_same_v<T, bool>) {
                return true; // Boolean values are always valid
            }
            else if constexpr (std::is_arithmetic_v<T>) {
                return value >= minValue_ && value <= maxValue_;
            }
            return true; // String and enum types are always valid if they parse correctly
        }

        // Reset to default
        void resetToDefault() override {
            currentValue_ = defaultValue_;
        }

        // Condition for showing in UI
        bool shouldShow() const {
            return !showCondition_ || showCondition_();
        }

        // Get ParamType for compatibility
        ParamType getParamType() const override {
            if constexpr (std::is_same_v<T, bool>)
                return ParamType::BOOL;
            else if constexpr (std::is_same_v<T, int>)
                return ParamType::INT;
            else if constexpr (std::is_same_v<T, double>)
                return ParamType::DOUBLE;
            else if constexpr (std::is_same_v<T, String>)
                return ParamType::STRING;
            else
                return ParamType::INT; // fallback
        }

        // JSON serialization
        void toJson(JsonObject& obj) const override {
            toJsonBase(obj);
            //obj["id"] = key_;
            //obj["key"] = key_;
            // obj["name"] = key_;
            // obj["label"] = displayName_;
            // obj["section"] = section_;
            // obj["order"] = order_;
            // obj["helpText"] = helpText_;
            // obj["type"] = getTypeName();
            //obj["type"] = static_cast<int>(getParamType());

            if constexpr (std::is_same_v<T, bool>) {
                obj["value"] = currentValue_;
                obj["default"] = defaultValue_;
            }
            else if constexpr (std::is_arithmetic_v<T>) {
                obj["value"] = currentValue_;
                obj["default"] = defaultValue_;
                obj["min"] = minValue_;
                obj["max"] = maxValue_;
            }
            else if constexpr (std::is_same_v<T, String>) {
                obj["value"] = currentValue_;
                obj["default"] = defaultValue_;
            }
        }

        // String deserialization - convert string to appropriate type
        bool fromString(const String& value) override {
            T newValue;
            if constexpr (std::is_same_v<T, bool>) {
                // Convert string "true"/"false"/"1"/"0" to boolean
                newValue = value.equalsIgnoreCase("true") || value == "1";
            }
            else if constexpr (std::is_same_v<T, int>) {
                // Convert string to integer
                newValue = value.toInt();
            }
            else if constexpr (std::is_same_v<T, double>) {
                // Convert string to double
                newValue = value.toDouble();
            }
            else if constexpr (std::is_same_v<T, String>) {
                // Keep as string
                newValue = value;
            }
            else {
                return false;
            }

            // Validate and set (which will auto-save)
            return set(newValue);
        }

        // NVS persistence
        bool loadFromNvs(Preferences& prefs) override {
            String nvsKey = generateNvsKey();
            if (!prefs.isKey(nvsKey.c_str())) {
                return false;
            }

            T value;
            if constexpr (std::is_same_v<T, bool>) {
                value = prefs.getBool(nvsKey.c_str());
            }
            else if constexpr (std::is_same_v<T, int>) {
                value = prefs.getInt(nvsKey.c_str());
            }
            else if constexpr (std::is_same_v<T, double>) {
                value = prefs.getDouble(nvsKey.c_str());
            }
            else if constexpr (std::is_same_v<T, String>) {
                value = prefs.getString(nvsKey.c_str());
            }
            else {
                return false;
            }

            currentValue_ = value;
            return true;
        }

        bool saveToNvs(Preferences& prefs) const override {
            String nvsKey = generateNvsKey();

            bool success = false;
            if constexpr (std::is_same_v<T, bool>) {
                success = prefs.putBool(nvsKey.c_str(), currentValue_);
            }
            else if constexpr (std::is_same_v<T, int>) {
                success = prefs.putInt(nvsKey.c_str(), currentValue_);
            }
            else if constexpr (std::is_same_v<T, double>) {
                success = prefs.putDouble(nvsKey.c_str(), currentValue_);
            }
            else if constexpr (std::is_same_v<T, String>) {
                success = prefs.putString(nvsKey.c_str(), currentValue_);
            }

            if (!success) {
                LOGF(ERROR, "NVS write failed for parameter '%s' with key '%s'", key_.c_str(), nvsKey.c_str());
            } else {
                LOGF(DEBUG, "NVS write successful for parameter '%s' with key '%s'", key_.c_str(), nvsKey.c_str());
            }

            return success;
        }

    private:
        T defaultValue_;
        T currentValue_;
        T minValue_;
        T maxValue_;
        std::function<bool()> showCondition_;

        String generateNvsKey() const {
            // Generate short hash-based key for NVS (max 15 chars)
            // Format: "p" + 8-char hex = 9 chars total (well under 15-char limit)
            uint32_t hash = fnv1a_hash(key_.c_str());
            return "p" + String(hash, HEX);
        }

        uint32_t fnv1a_hash(const char* str) const {
            uint32_t hash = 2166136261u;
            while (*str) {
                hash ^= *str++;
                hash *= 16777619u;
            }
            return hash;
        }

        const char* getTypeName() const {
            if constexpr (std::is_same_v<T, bool>)
                return "bool";
            else if constexpr (std::is_same_v<T, int>)
                return "int";
            else if constexpr (std::is_same_v<T, double>)
                return "double";
            else if constexpr (std::is_same_v<T, String>)
                return "string";
            else
                return "unknown";
        }
};

/**
 * @brief Specialized parameter definition for enum types
 */
template <typename E>
class EnumParamDef : public ConfigParamDef {
        static_assert(std::is_enum_v<E>, "EnumParamDef requires an enum type");

    public:
        EnumParamDef(
            const String& key, E defaultValue, const String& displayName, int section, int order, const String& helpText, std::vector<std::pair<E, String>> options = {}, std::function<bool()> showCondition = nullptr) :
            ConfigParamDef(key, displayName, section, order, helpText), defaultValue_(defaultValue), currentValue_(defaultValue), options_(options), showCondition_(showCondition) {
        }

        E get() const {
            return currentValue_;
        }

        bool set(const E& value) {
            if (!isValid(value)) {
                LOGF(WARNING, "Invalid value for enum parameter '%s'", key_.c_str());
                return false;
            }
            currentValue_ = value;

            // Automatically save to NVS
            Preferences prefs;
            if (prefs.begin(STORAGE_NAMESPACE, false)) { // Read-write mode
                bool saveSuccess = saveToNvs(prefs);
                prefs.end();
                if (!saveSuccess) {
                    LOGF(ERROR, "Failed to save enum parameter '%s' to NVS", key_.c_str());
                    return false;
                } else {
                    LOGF(DEBUG, "Enum parameter '%s' successfully saved to NVS", key_.c_str());
                }
            } else {
                LOGF(ERROR, "Failed to open NVS namespace '%s' for enum parameter '%s'", STORAGE_NAMESPACE, key_.c_str());
                return false;
            }

            return true;
        }        bool isValid(const E& value) const {
            if (options_.empty()) {
                return true; // No restrictions
            }
            for (const auto& option : options_) {
                if (option.first == value) {
                    return true;
                }
            }
            return false;
        }

        void resetToDefault() override {
            currentValue_ = defaultValue_;
        }

        bool shouldShow() const {
            return !showCondition_ || showCondition_();
        }

        // Get ParamType for compatibility
        ParamType getParamType() const override {
            return ParamType::ENUM;
        }

        void toJson(JsonObject& obj) const override {
            toJsonBase(obj);

            obj["value"] = static_cast<int>(currentValue_);
            obj["default"] = static_cast<int>(defaultValue_);

            if (!options_.empty()) {
                JsonArray optionsArray = obj["options"].to<JsonArray>();
                for (const auto& option : options_) {
                    JsonObject opt = optionsArray.add<JsonObject>();
                    opt["value"] = static_cast<int>(option.first);
                    opt["label"] = option.second;
                }
            }
        }

        bool fromString(const String& value) override {
            // For enum types, try to parse as integer first
            int intValue = value.toInt();

            // Check if the integer value is valid for this enum
            E enumValue = static_cast<E>(intValue);
            if (isValid(enumValue)) {
                return set(enumValue);
            }

            // If integer parsing failed or is invalid, try to match by label
            for (const auto& option : options_) {
                if (option.second.equalsIgnoreCase(value)) {
                    return set(option.first);
                }
            }

            return false; // No valid conversion found
        }

        bool loadFromNvs(Preferences& prefs) override {
            String nvsKey = generateNvsKey();
            if (!prefs.isKey(nvsKey.c_str())) {
                return false;
            }

            int intValue = prefs.getInt(nvsKey.c_str());
            currentValue_ = static_cast<E>(intValue);
            return true;
        }

        bool saveToNvs(Preferences& prefs) const override {
            String nvsKey = generateNvsKey();
            bool success = prefs.putInt(nvsKey.c_str(), static_cast<int>(currentValue_));

            if (!success) {
                LOGF(ERROR, "NVS write failed for enum parameter '%s' with key '%s'", key_.c_str(), nvsKey.c_str());
            } else {
                LOGF(DEBUG, "NVS write successful for enum parameter '%s' with key '%s'", key_.c_str(), nvsKey.c_str());
            }

            return success;
        }

    private:
        E defaultValue_;
        E currentValue_;
        std::vector<std::pair<E, String>> options_;
        std::function<bool()> showCondition_;

        String generateNvsKey() const {
            // Generate short hash-based key for NVS (max 15 chars)
            // Format: "e" + 8-char hex = 9 chars total (well under 15-char limit)
            uint32_t hash = fnv1a_hash(key_.c_str());
            return "e" + String(hash, HEX);
        }

        uint32_t fnv1a_hash(const char* str) const {
            uint32_t hash = 2166136261u;
            while (*str) {
                hash ^= *str++;
                hash *= 16777619u;
            }
            return hash;
        }
};

/**
 * @brief Read-only state parameter definition for runtime values
 *
 * There are different types of read-only data:
 * 1. Live sensor readings (temperature, pressure, weight)
 * 2. Calculated values (PID output, derived settings)
 * 3. System status (machine state, connection status, errors)
 * 4. Runtime statistics (loop timing, memory usage)
 * 5. Hardware info (firmware version, MAC address)
 */
template <typename T>
class StateParamDef : public BaseParamDef {
    public:
        enum class UpdateFrequency {
            REALTIME,   // Updated every loop (temperature, PID output)
            FREQUENT,   // Updated every few seconds (connection status)
            OCCASIONAL, // Updated when requested (system info)
            STATIC      // Never changes after init (hardware info)
        };

        StateParamDef(
            const String& key, const String& displayName, int section, int order, const String& helpText, std::function<T()> valueProvider, UpdateFrequency frequency = UpdateFrequency::REALTIME, const String& unit = "") :
            key_(key), displayName_(displayName), section_(section), order_(order), helpText_(helpText), valueProvider_(valueProvider), frequency_(frequency), unit_(unit) {
        }

        // Read-only access
        T get() const {
            return valueProvider_();
        }

        // Get cached value (for frequently accessed values)
        T getCached() const {
            if (frequency_ == UpdateFrequency::REALTIME || !hasCachedValue_) {
                cachedValue_ = valueProvider_();
                lastUpdate_ = millis();
                hasCachedValue_ = true;
            }
            return cachedValue_;
        }

        // Force update cached value
        void updateCache() const {
            cachedValue_ = valueProvider_();
            lastUpdate_ = millis();
            hasCachedValue_ = true;
        }

        // Check if cache needs update
        bool needsUpdate(unsigned long maxAge = 1000) const {
            return !hasCachedValue_ || (millis() - lastUpdate_) > maxAge;
        }

        // JSON serialization for web interface
        void toJson(JsonObject& obj) const override {
            toJsonBase(obj);

            obj["value"] = get();
            obj["readonly"] = true;
            obj["frequency"] = static_cast<int>(frequency_);
            if (!unit_.isEmpty()) {
                obj["unit"] = unit_;
            }
        }

        ParamType getParamType() const override {
            if constexpr (std::is_same_v<T, bool>)
                return ParamType::BOOL;
            else if constexpr (std::is_same_v<T, int>)
                return ParamType::INT;
            else if constexpr (std::is_same_v<T, double>)
                return ParamType::DOUBLE;
            else if constexpr (std::is_same_v<T, String>)
                return ParamType::STRING;
            else
                return ParamType::INT; // fallback
        }

        const String& getKey() const {
            return key_;
        }
        const String& getDisplayName() const {
            return displayName_;
        }
        int getSection() const {
            return section_;
        }
        int getOrder() const {
            return order_;
        }
        const String& getHelpText() const {
            return helpText_;
        }
        const String& getUnit() const {
            return unit_;
        }
        UpdateFrequency getFrequency() const {
            return frequency_;
        }

    private:
        String key_;
        String displayName_;
        int section_;
        int order_;
        String helpText_;
        String unit_;
        std::function<T()> valueProvider_;
        UpdateFrequency frequency_;

        // Caching for performance
        mutable T cachedValue_{};
        mutable unsigned long lastUpdate_ = 0;
        mutable bool hasCachedValue_ = false;

        const char* getTypeName() const {
            if constexpr (std::is_same_v<T, bool>)
                return "bool";
            else if constexpr (std::is_same_v<T, int>)
                return "int";
            else if constexpr (std::is_same_v<T, double>)
                return "double";
            else if constexpr (std::is_same_v<T, String>)
                return "string";
            else
                return "unknown";
        }
};

/**
 * @brief Read-only computed parameter - derives value from other parameters
 *
 * Example: PID Ki value computed from Kp and Tn
 */
template <typename T>
class ComputedParamDef : public BaseParamDef {
    public:
        ComputedParamDef(const String& key, const String& displayName, int section, int order, const String& helpText, std::function<T()> computation, const String& unit = "") :
            key_(key), displayName_(displayName), section_(section), order_(order), helpText_(helpText), computation_(computation), unit_(unit) {
        }

        T get() const {
            return computation_();
        }

        void toJson(JsonObject& obj) const override {
            toJsonBase(obj);
            obj["value"] = get();
            obj["readonly"] = true;
            obj["computed"] = true;
            if (!unit_.isEmpty()) {
                obj["unit"] = unit_;
            }
        }

        ParamType getParamType() const override {
            if constexpr (std::is_same_v<T, bool>)
                return ParamType::BOOL;
            else if constexpr (std::is_same_v<T, int>)
                return ParamType::INT;
            else if constexpr (std::is_same_v<T, double>)
                return ParamType::DOUBLE;
            else if constexpr (std::is_same_v<T, String>)
                return ParamType::STRING;
            else
                return ParamType::INT; // fallback
        }

        const String& getKey() const {
            return key_;
        }

    private:
        String key_;
        String displayName_;
        int section_;
        int order_;
        String helpText_;
        String unit_;
        std::function<T()> computation_;

        const char* getTypeName() const {
            if constexpr (std::is_same_v<T, bool>)
                return "bool";
            else if constexpr (std::is_same_v<T, int>)
                return "int";
            else if constexpr (std::is_same_v<T, double>)
                return "double";
            else if constexpr (std::is_same_v<T, String>)
                return "string";
            else
                return "unknown";
        }
};

/**
 * @brief Next-generation configuration system with direct parameter access
 */
class Config {
    public:
        static Config& getInstance() {
            static Config instance;
            return instance;
        }

        // ===============================
        // EDITABLE CONFIGURATION PARAMETERS
        // ===============================

        // === PID PARAMETERS (Section 0) ===
        ParamDef<bool> pidEnabled{"pid.enabled", false, "Enable PID Controller", 0, 101, "Enables or disables the PID temperature controller"};

        ParamDef<bool> pidUsePonm{"pid.use_ponm", false, "Enable PonM", 0, 102, "Use PonM mode (Proportional on Measurement)"};

        ParamDef<double> pidEmaFactor{"pid.ema_factor", EMA_FACTOR, "PID EMA Factor", 0, 111, "Smoothing of input for derivative component. Smaller = less smoothing but less delay", PID_EMA_FACTOR_MIN, PID_EMA_FACTOR_MAX};

        ParamDef<double> pidRegularKp{"pid.regular.kp", AGGKP, "PID Kp", 0, 112, "Proportional gain (in Watts/°C) for the main PID controller", PID_KP_REGULAR_MIN, PID_KP_REGULAR_MAX};

        ParamDef<double> pidRegularTn{"pid.regular.tn", AGGTN, "PID Tn", 0, 113, "Integral time constant (in seconds) for the main PID controller", PID_TN_REGULAR_MIN, PID_TN_REGULAR_MAX};

        ParamDef<double> pidRegularTv{"pid.regular.tv", AGGTV, "PID Tv", 0, 114, "Differential time constant (in seconds) for the main PID controller", PID_TV_REGULAR_MIN, PID_TV_REGULAR_MAX};

        ParamDef<double> pidRegularIMax{"pid.regular.i_max", AGGIMAX, "PID Integrator Max", 0, 115, "Internal integrator limit to prevent windup (in Watts)", PID_I_MAX_REGULAR_MIN, PID_I_MAX_REGULAR_MAX};

        ParamDef<double> pidSteamKp{"pid.steam.kp", STEAMKP, "Steam Kp", 0, 116, "Proportional gain for the steaming mode", PID_KP_STEAM_MIN, PID_KP_STEAM_MAX};

        // === BREW PARAMETERS (Section 1) ===
        ParamDef<double> brewSetpoint{"brew.setpoint", SETPOINT, "Setpoint (°C)", 1, 201, "The temperature that the PID will attempt to reach and hold", BREW_SETPOINT_MIN, BREW_SETPOINT_MAX};

        ParamDef<double> brewTempOffset{"brew.temp_offset", TEMPOFFSET, "Offset (°C)", 1, 202, "Optional offset added to the user-visible setpoint to compensate sensor offsets", BREW_TEMP_OFFSET_MIN, BREW_TEMP_OFFSET_MAX};

        ParamDef<double> steamSetpoint{"steam.setpoint", STEAMSETPOINT, "Steam Setpoint (°C)", 1, 203, "The temperature that the PID will use for steam mode", STEAM_SETPOINT_MIN, STEAM_SETPOINT_MAX};

        // === BREW DETECTION PID PARAMETERS (Section 2) ===
        ParamDef<bool> pidBdEnabled{"pid.bd.enabled", false, "Enable Brew PID", 2, 701, "Use separate PID parameters while brew is running"};

        ParamDef<double> brewPidDelay{"brew.pid_delay", BREW_PID_DELAY, "Brew PID Delay (s)", 2, 711, "Delay time during which PID will be disabled once brew is detected", BREW_PID_DELAY_MIN, BREW_PID_DELAY_MAX};

        ParamDef<double> pidBdKp{"pid.bd.kp", AGGBKP, "BD Kp", 2, 712, "Proportional gain for PID when brewing has been detected", PID_KP_BD_MIN, PID_KP_BD_MAX};

        ParamDef<double> pidBdTn{"pid.bd.tn", AGGBTN, "BD Tn", 2, 713, "Integral time constant for PID when brewing has been detected", PID_TN_BD_MIN, PID_TN_BD_MAX};

        ParamDef<double> pidBdTv{"pid.bd.tv", AGGBTV, "BD Tv", 2, 714, "Differential time constant for PID when brewing has been detected", PID_TV_BD_MIN, PID_TV_BD_MAX};

        // === BREWING CONTROL PARAMETERS (Section 3) ===
        ParamDef<bool> brewByTimeEnabled{"brew.by_time.enabled", false, "Brew by Time", 3, 311, "Enable brewing by time control"};

        ParamDef<double> brewByTimeTargetTime{"brew.by_time.target_time", TARGET_BREW_TIME, "Target Brew Time (s)", 3, 301, "Target brew time in seconds", TARGET_BREW_TIME_MIN, TARGET_BREW_TIME_MAX};

        ParamDef<bool> brewByWeightEnabled{"brew.by_weight.enabled", false, "Brew by Weight", 3, 321, "Enable brewing by weight control"};

        ParamDef<double> brewByWeightTargetWeight{"brew.by_weight.target_weight", TARGET_BREW_WEIGHT,    "Target Brew Weight (g)", 3, 322, "Brew is running until this weight has been measured",
                                                  TARGET_BREW_WEIGHT_MIN,         TARGET_BREW_WEIGHT_MAX};

        ParamDef<bool> brewByWeightAutoTare{"brew.by_weight.auto_tare", false, "Auto-tare", 3, 323, "Automatically tare scale before brewing"};

        ParamDef<bool> brewPreInfusionEnabled{"brew.pre_infusion.enabled", false, "Pre-Infusion", 3, 304, "Enable pre-infusion phase"};

        ParamDef<double> brewPreInfusionTime{"brew.pre_infusion.time", PRE_INFUSION_TIME, "Preinfusion Time (s)", 3, 302, "Pre-infusion time in seconds", PRE_INFUSION_TIME_MIN, PRE_INFUSION_TIME_MAX};

        ParamDef<double> brewPreInfusionPause{"brew.pre_infusion.pause", PRE_INFUSION_PAUSE_TIME, "Preinfusion Pause (s)", 3, 303, "Pre-infusion pause time in seconds", PRE_INFUSION_PAUSE_MIN, PRE_INFUSION_PAUSE_MAX};

        EnumParamDef<Process::BrewMode> brewMode{
            "brew.mode", Process::BrewMode::MANUAL_BREW, "Brew Mode", 3, 310, "Brewing mode selection", {{Process::BrewMode::MANUAL_BREW, "Manual"}, {Process::BrewMode::AUTOMATIC_BREW, "Automatic"}}};

        // === HARDWARE OLED PARAMETERS (Section 11) ===
        ParamDef<bool> hardwareOledEnabled{"hardware.oled.enabled", true, "Enable OLED Display", 11, 2001, "Enable or disable the OLED display"};

        EnumParamDef<Hardware::OLEDType> hardwareOledType{"hardware.oled.type", Hardware::OLEDType::SSD1306, "OLED Type", 11, 2002, "Select your OLED display type", oledTypeOptions};

        EnumParamDef<Hardware::OLEDAddress> hardwareOledAddress{"hardware.oled.address", Hardware::OLEDAddress::ADDR_3C, "I2C Address", 11, 2003, "I2C address of the OLED display", oledAddressOptions};

        // === HARDWARE RELAYS PARAMETERS (Section 12) ===
        EnumParamDef<Hardware::RelayTriggerType> hardwareRelaysHeaterTriggerType{
            "hardware.relays.heater.trigger_type", Hardware::RelayTriggerType::HIGH_TRIGGER, "Heater Relay Trigger Type", 12, 2101, "Relay trigger type for heater control", relayTriggerOptions};

        EnumParamDef<Hardware::RelayTriggerType> hardwareRelaysValveTriggerType{
            "hardware.relays.valve.trigger_type", Hardware::RelayTriggerType::HIGH_TRIGGER, "Valve Relay Trigger Type", 12, 2102, "Relay trigger type for valve control", relayTriggerOptions};

        EnumParamDef<Hardware::RelayTriggerType> hardwareRelaysPumpTriggerType{
            "hardware.relays.pump.trigger_type", Hardware::RelayTriggerType::HIGH_TRIGGER, "Pump Relay Trigger Type", 12, 2103, "Relay trigger type for pump control", relayTriggerOptions};

        // === HARDWARE SWITCHES PARAMETERS (Section 13) ===
        ParamDef<bool> hardwareSwitchesBrewEnabled{"hardware.switches.brew.enabled", false, "Enable Brew Switch", 13, 2201, "Enable physical brew switch"};

        EnumParamDef<Hardware::SwitchType> hardwareSwitchesBrewType{"hardware.switches.brew.type", Hardware::SwitchType::TOGGLE, "Brew Switch Type", 13, 2202, "Type of brew switch connected", switchTypeOptions};

        EnumParamDef<Hardware::SwitchMode> hardwareSwitchesBrewMode{
            "hardware.switches.brew.mode", Hardware::SwitchMode::NORMALLY_OPEN, "Brew Switch Mode", 13, 2203, "Electrical configuration of brew switch", switchModeOptions};

        ParamDef<bool> hardwareSwitchesSteamEnabled{"hardware.switches.steam.enabled", false, "Enable Steam Switch", 13, 2211, "Enable physical steam switch"};

        EnumParamDef<Hardware::SwitchType> hardwareSwitchesSteamType{"hardware.switches.steam.type", Hardware::SwitchType::TOGGLE, "Steam Switch Type", 13, 2212, "Type of steam switch connected", switchTypeOptions};

        EnumParamDef<Hardware::SwitchMode> hardwareSwitchesSteamMode{
            "hardware.switches.steam.mode", Hardware::SwitchMode::NORMALLY_OPEN, "Steam Switch Mode", 13, 2213, "Electrical configuration of steam switch", switchModeOptions};

        ParamDef<bool> hardwareSwitchesPowerEnabled{"hardware.switches.power.enabled", false, "Enable Power Switch", 13, 2221, "Enable physical power switch"};

        EnumParamDef<Hardware::SwitchType> hardwareSwitchesPowerType{"hardware.switches.power.type", Hardware::SwitchType::TOGGLE, "Power Switch Type", 13, 2222, "Type of power switch connected", switchTypeOptions};

        EnumParamDef<Hardware::SwitchMode> hardwareSwitchesPowerMode{
            "hardware.switches.power.mode", Hardware::SwitchMode::NORMALLY_OPEN, "Power Switch Mode", 13, 2223, "Electrical configuration of power switch", switchModeOptions};

        ParamDef<bool> hardwareSwitchesHotWaterEnabled{"hardware.switches.hot_water.enabled", false, "Enable Water Switch", 13, 2231, "Enable physical water switch"};

        EnumParamDef<Hardware::SwitchType> hardwareSwitchesHotWaterType{"hardware.switches.hot_water.type", Hardware::SwitchType::TOGGLE, "Water Switch Type", 13, 2232, "Type of water switch connected", switchTypeOptions};

        EnumParamDef<Hardware::SwitchMode> hardwareSwitchesHotWaterMode{
            "hardware.switches.hot_water.mode", Hardware::SwitchMode::NORMALLY_OPEN, "Water Switch Mode", 13, 2233, "Electrical configuration of water switch", switchModeOptions};

        // === HARDWARE LEDS PARAMETERS (Section 4) ===
        ParamDef<bool> hardwareLedsStatusEnabled{"hardware.leds.status.enabled", false, "Enable Status LED", 4, 2301, "Enable status indicator LED"};

        ParamDef<bool> hardwareLedsStatusInverted{"hardware.leds.status.inverted", false, "Invert Status LED", 4, 2302, "Invert the status LED logic (for common anode LEDs)"};

        ParamDef<bool> hardwareLedsBrewEnabled{"hardware.leds.brew.enabled", false, "Enable Brew LED", 4, 2311, "Enable brew indicator LED"};

        ParamDef<bool> hardwareLedsBrewInverted{"hardware.leds.brew.inverted", false, "Invert Brew LED", 4, 2312, "Invert the brew LED logic"};

        ParamDef<bool> hardwareLedsSteamEnabled{"hardware.leds.steam.enabled", false, "Enable Steam LED", 4, 2321, "Enable steam indicator LED"};

        ParamDef<bool> hardwareLedsSteamInverted{"hardware.leds.steam.inverted", false, "Invert Steam LED", 4, 2322, "Invert the steam LED logic"};

        // === HARDWARE SENSORS PARAMETERS (Section 15 & 4) ===
        EnumParamDef<Hardware::TemperatureSensorType> hardwareSensorsTemperatureType{
            "hardware.sensors.temperature.type", Hardware::TemperatureSensorType::TSIC_306, "Temperature Sensor Type", 15, 2401, "Type of temperature sensor connected", temperatureSensorTypeOptions};

        ParamDef<bool> hardwareSensorsPressureEnabled{"hardware.sensors.pressure.enabled", false, "Enable Pressure Sensor", 4, 2411, "Enable pressure sensor functionality"};

        ParamDef<bool> hardwareSensorsWatertankEnabled{"hardware.sensors.watertank.enabled", false, "Enable Water Tank Sensor", 4, 2421, "Enable water tank level sensor"};

        EnumParamDef<Hardware::SwitchMode> hardwareSensorsWatertankMode{
            "hardware.sensors.watertank.mode", Hardware::SwitchMode::NORMALLY_CLOSED, "Water Tank Sensor Mode", 15, 2422, "Electrical configuration of water tank sensor", switchModeOptions};

        ParamDef<bool> hardwareSensorsScaleEnabled{"hardware.sensors.scale.enabled", false, "Enable Scale", 4, 2501, "Enable scale functionality"};

        ParamDef<int> hardwareSensorsScaleSamples{"hardware.sensors.scale.samples", SCALE_SAMPLES, "Scale Samples", 4, 2502, "Number of samples used for calibration", SCALE_SAMPLES_MIN, SCALE_SAMPLES_MAX};

        EnumParamDef<Hardware::ScaleType> hardwareSensorsScaleType{
            "hardware.sensors.scale.type", Hardware::ScaleType::HX711_DUAL, "Scale Type", 15, 2503, "Integrated HX711-based scale with different load cell configurations or Bluetooth Low Energy scales", scaleTypeOptions};

        ParamDef<double> hardwareSensorsScaleCalibration{
            "hardware.sensors.scale.calibration", SCALE_CALIBRATION_FACTOR, "Scale Calibration", 4, 2504, "Raw data is divided by this value to convert to readable data", SCALE_CALIBRATION_MIN, SCALE_CALIBRATION_MAX};

        ParamDef<double> hardwareSensorsScaleCalibration2{
            "hardware.sensors.scale.calibration2", SCALE_CALIBRATION_FACTOR, "Scale Calibration 2", 4, 2505, "Second calibration factor for dual load cell scales", SCALE_CALIBRATION_MIN, SCALE_CALIBRATION_MAX};

        ParamDef<double> hardwareSensorsScaleKnownWeight{
            "hardware.sensors.scale.known_weight", SCALE_KNOWN_WEIGHT, "Scale Known Weight", 4, 2506, "Calibration weight for scale (weight of the tray)", SCALE_KNOWN_WEIGHT_MIN, SCALE_KNOWN_WEIGHT_MAX};

        // === DISPLAY PARAMETERS (Section 5) ===
        EnumParamDef<System::DisplayTemplate> displayTemplate{"display.template", System::DisplayTemplate::STANDARD, "Display Template", 5, 901, "Set the display template, changes require a reboot", displayTemplateOptions};

        ParamDef<bool> displayInverted{"display.inverted", false, "Invert Display", 5, 902, "Set the display rotation, changes require a reboot"};

        EnumParamDef<System::Language> displayLanguage{"display.language", System::Language::ENGLISH, "Display Language", 5, 903, "Set the language for the OLED display", languageOptions};

        ParamDef<bool> displayFullscreenBrewTimer{"display.fullscreen_brew_timer", false, "Enable Fullscreen Brew Timer", 3, 904, "Enable fullscreen overlay during brew"};

        ParamDef<bool> displayFullscreenManualFlushTimer{"display.fullscreen_manual_flush_timer", false, "Enable Fullscreen Manual Flush Timer", 3, 905, "Enable fullscreen overlay during manual flush"};

        ParamDef<bool> displayFullscreenHotWaterTimer{"display.fullscreen_hot_water_timer", false, "Enable Fullscreen Hot Water Timer", 3, 906, "Enable fullscreen overlay during hot water mode"};

        ParamDef<double> displayPostBrewTimerDuration{
            "display.post_brew_timer_duration", POST_BREW_TIMER_DURATION,    "Post Brew Timer Duration (s)", 3, 907, "Post brew timer will be shown for this many seconds after brew finished",
            POST_BREW_TIMER_DURATION_MIN,       POST_BREW_TIMER_DURATION_MAX};

        ParamDef<bool> displayHeatingLogo{"display.heating_logo", true, "Enable Heating Logo", 3, 908, "Full screen logo will be shown if temperature is 5°C below setpoint"};

        ParamDef<bool> displayPidOffLogo{"display.pid_off_logo", true, "Enable 'PID Disabled' Logo", 3, 909, "Full screen logo will be shown if PID is disabled"};

        // === BACKFLUSH PARAMETERS (Section 6) ===
        ParamDef<int> backflushCycles{"backflush.cycles", BACKFLUSH_CYCLES, "Backflush Cycles", 6, 401, "Number of backflush cycles to perform", BACKFLUSH_CYCLES_MIN, BACKFLUSH_CYCLES_MAX};

        ParamDef<double> backflushFillTime{"backflush.fill_time", BACKFLUSH_FILL_TIME, "Backflush Fill Time (s)", 6, 402, "Time to fill during backflush cycle", BACKFLUSH_FILL_TIME_MIN, BACKFLUSH_FILL_TIME_MAX};

        ParamDef<double> backflushFlushTime{"backflush.flush_time", BACKFLUSH_FLUSH_TIME, "Backflush Flush Time (s)", 6, 403, "Time to flush during backflush cycle", BACKFLUSH_FLUSH_TIME_MIN, BACKFLUSH_FLUSH_TIME_MAX};

        // === STANDBY PARAMETERS (Section 7) ===
        ParamDef<bool> standbyEnabled{"standby.enabled", false, "Enable Standby Timer", 7, 801, "Turn heater off after standby time has elapsed"};

        ParamDef<double> standbyTime{"standby.time", STANDBY_MODE_TIME, "Standby Time", 7, 802, "Time in minutes until the heater is turned off", STANDBY_MODE_TIME_MIN, STANDBY_MODE_TIME_MAX};

        // === MQTT PARAMETERS (Section 8) ===
        ParamDef<bool> mqttEnabled{"mqtt.enabled", false, "MQTT Enabled", 8, 1001, "Enables MQTT, change requires a restart"};

        ParamDef<String> mqttBroker{"mqtt.broker", "", "MQTT Broker", 8, 1011, "IP address or hostname of your MQTT broker"};

        ParamDef<int> mqttPort{"mqtt.port", 1883, "MQTT Port", 8, 1012, "Port number of your MQTT broker", 1, 65535};

        ParamDef<String> mqttUsername{"mqtt.username", MQTT_USERNAME, "Username", 8, 1013, "Username for your MQTT broker"};

        ParamDef<String> mqttPassword{"mqtt.password", MQTT_PASSWORD, "Password", 8, 1014, "Password for your MQTT broker"};

        ParamDef<String> mqttTopic{"mqtt.topic", MQTT_TOPIC, "Topic Prefix", 8, 1015, "Custom MQTT topic prefix"};

        ParamDef<bool> mqttHassioEnabled{"mqtt.hassio.enabled", false, "Hass.io enabled", 8, 1021, "Enables Home Assistant integration"};

        ParamDef<String> mqttHassioPrefix{"mqtt.hassio.prefix", MQTT_HASSIO_PREFIX, "Hass.io Prefix", 8, 1022, "Custom MQTT topic prefix for Home Assistant"};

        // === SYSTEM PARAMETERS (Section 9) ===
        ParamDef<String> systemHostname{"system.hostname", HOSTNAME, "Hostname", 9, 1101, "Hostname of your machine, changes require a restart"};

        ParamDef<String> systemOtaPassword{"system.ota_password", OTAPASS, "OTA Password", 9, 1102, "Password for over-the-air updates, changes require a restart"};

        ParamDef<bool> systemOfflineMode{"system.offline_mode", false, "Offline Mode", 9, 1103, "Run in offline mode without WiFi connection"};

        EnumParamDef<System::LogLevel> systemLogLevel{"system.log_level", System::LogLevel::INFO, "Log Level", 9, 1103, "Set the logging level for debug output", logLevelOptions};

        ParamDef<bool> systemAuthEnabled{"system.auth.enabled", false, "Enable Authentication", 9, 1201, "Enables authentication for accessing certain parts of the website"};

        ParamDef<String> systemAuthUsername{"system.auth.username", AUTH_USERNAME, "Website Username", 9, 1202, "Username for accessing the website and authenticating web requests"};

        ParamDef<String> systemAuthPassword{"system.auth.password", AUTH_PASSWORD, "Website Password", 9, 1203, "Password for accessing the website and authenticating web requests"};

        ParamDef<bool> systemTimingDebugEnabled{"system.timing_debug.enabled", false, "Loop timing in console", 9, 1301, "Enable or disable the process loop time debugging in console"};

        ParamDef<bool> systemShowdisplayEnabled{"system.showdisplay.enabled", true, "Activate display recording", 9, 1303, "Enable or disable showing sendBuffer loops in debug logs"};

        // ===============================
        // READ-ONLY STATE PARAMETERS (commented out for now - add as needed)
        // ===============================
        /*
        // === LIVE SENSOR READINGS ===
        StateParamDef<double> stateTemperature{"state.temperature", "Current Temperature", 6, 601,
                                               "Current temperature reading from sensor",
                                               []() { return g_state.process.temperature; },
                                               StateParamDef<double>::UpdateFrequency::REALTIME, "°C"};

        StateParamDef<double> stateHeaterPower{"state.heater_power", "Heater Power", 6, 602,
                                               "Current heater power output percentage",
                                               []() { return g_state.process.pidOutput / 10.0; },
                                               StateParamDef<double>::UpdateFrequency::REALTIME, "%"};

        StateParamDef<double> statePressure{"state.pressure", "Current Pressure", 6, 603,
                                            "Current pressure reading from sensor",
                                            []() { return g_state.sensors.inputPressureFilter; },
                                            StateParamDef<double>::UpdateFrequency::REALTIME, "bar"};

        StateParamDef<double> stateWeight{"state.weight", "Current Weight", 6, 604,
                                          "Current weight reading from scale",
                                          []() { return g_state.sensors.currReadingWeight; },
                                          StateParamDef<double>::UpdateFrequency::REALTIME, "g"};

        // === MACHINE STATUS ===
        StateParamDef<int> stateMachineState{"state.machine_state", "Machine State", 6, 605,
                                             "Current machine state",
                                             []() { return static_cast<int>(g_state.machine.machineState); },
                                             StateParamDef<int>::UpdateFrequency::FREQUENT};

        StateParamDef<bool> stateWifiConnected{"state.wifi_connected", "WiFi Connected", 6, 606,
                                               "WiFi connection status",
                                               []() { return WiFi.status() == WL_CONNECTED; },
                                               StateParamDef<bool>::UpdateFrequency::FREQUENT};

        StateParamDef<bool> stateMqttConnected{"state.mqtt_connected", "MQTT Connected", 6, 607,
                                               "MQTT broker connection status",
                                               []() { return g_state.network.mqttManager && g_state.network.mqttManager->isConnected(); },
                                               StateParamDef<bool>::UpdateFrequency::FREQUENT};

        StateParamDef<bool> stateWaterTank{"state.water_tank", "Water Tank Full", 6, 608,
                                               "Water tank sensor status",
                                               []() { return g_state.machine.waterTankFull; },
                                               StateParamDef<bool>::UpdateFrequency::FREQUENT};

        // === BREWING STATUS ===
        StateParamDef<double> stateBrewTime{"state.brew_time", "Current Brew Time", 7, 701,
                                            "Current brewing time in seconds",
                                            []() { return g_state.process.currBrewTime / 1000.0; },
                                            StateParamDef<double>::UpdateFrequency::REALTIME, "s"};

        StateParamDef<double> stateBrewWeight{"state.brew_weight", "Brew Weight", 7, 702,
                                              "Weight of extracted coffee",
                                              []() { return g_state.sensors.currBrewWeight; },
                                              StateParamDef<double>::UpdateFrequency::REALTIME, "g"};

        StateParamDef<bool> stateBrewActive{"state.brew_active", "Brew Active", 7, 703,
                                            "Whether brewing is currently active",
                                            []() { return g_state.machine.machineState == kBrew; },
                                            StateParamDef<bool>::UpdateFrequency::FREQUENT};

        // === SYSTEM INFORMATION ===
        StateParamDef<String> stateSystemVersion{"state.system_version", "Firmware Version", 8, 801,
                                                 "Current firmware version",
                                                 []() { return String(VERSION); },
                                                 StateParamDef<String>::UpdateFrequency::STATIC};

        StateParamDef<String> stateWifiIp{"state.wifi_ip", "WiFi IP Address", 8, 802,
                                          "Current WiFi IP address",
                                          []() { return WiFi.localIP().toString(); },
                                          StateParamDef<String>::UpdateFrequency::OCCASIONAL};

        StateParamDef<String> stateWifiSsid{"state.wifi_ssid", "WiFi Network", 8, 803,
                                            "Connected WiFi network name",
                                            []() { return WiFi.SSID(); },
                                            StateParamDef<String>::UpdateFrequency::OCCASIONAL};

        StateParamDef<int> stateFreeHeap{"state.free_heap", "Free Memory", 8, 804,
                                         "Available heap memory",
                                         []() { return static_cast<int>(ESP.getFreeHeap()); },
                                         StateParamDef<int>::UpdateFrequency::FREQUENT, "bytes"};

        StateParamDef<int> stateUptime{"state.uptime", "System Uptime", 8, 805,
                                       "System uptime in seconds",
                                       []() { return static_cast<int>(millis() / 1000); },
                                       StateParamDef<int>::UpdateFrequency::FREQUENT, "s"};
        */

        // ===============================
        // COMPUTED PARAMETERS (commented out for now - add as needed)
        // ===============================
        /*
        // === COMPUTED VALUES (derived from editable parameters) ===
        ComputedParamDef<double> computedPidKi{"computed.pid_ki", "PID Ki (computed)", 9, 901,
                                               "Computed integral gain from Kp and Tn",
                                               [this]() {
                                                   double tn = pidRegularTn.get();
                                                   return tn > 0 ? pidRegularKp.get() / tn : 0.0;
                                               }};

        ComputedParamDef<double> computedPidKd{"computed.pid_kd", "PID Kd (computed)", 9, 902,
                                               "Computed derivative gain from Kp and Tv",
                                               [this]() {
                                                   return pidRegularTv.get() * pidRegularKp.get();
                                               }};

        ComputedParamDef<double> computedBrewRatio{"computed.brew_ratio", "Brew Ratio", 9, 903,
                                                   "Current brew ratio (weight out / weight in)",
                                                   []() {
                                                       double preWeight = g_state.sensors.preBrewWeight;
                                                       double currentWeight = g_state.sensors.currBrewWeight;
                                                       return preWeight > 0 ? currentWeight / preWeight : 0.0;
                                                   }};
        */

        // System management
        bool begin();
        bool loadAll();
        bool saveAll();
        void resetAllToDefaults();

        // JSON export/import
        String exportToJson();
        bool importFromJson(const String& json);

        // Web interface support
        void getAllParameters(JsonArray& array, const String& filter = "");
        void getAllStateParams(JsonArray& array);

        // Parameter lookup by key (for web interface)
        ConfigParamDef* findConfigParameter(const String& key);

    private:
        Config() = default;
        ~Config() = default;
        Config(const Config&) = delete;
        Config& operator=(const Config&) = delete;

        std::vector<ConfigParamDef*> getAllConfigParams();
};
