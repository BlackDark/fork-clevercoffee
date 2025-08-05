/**
 * @file ConfigV2.h
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
#include <Arduino.h>
#include <ArduinoJson.h>
#include <Preferences.h>
#include <functional>
#include <map>
#include <memory>
#include <type_traits>

// Forward declarations
template<typename T>
class ParamDef;

template<typename T>
class StateParamDef;

/**
 * @brief Base class for all parameter definitions
 */
class BaseParamDef {
public:
    BaseParamDef(const String& key, const String& displayName, int section, int order, const String& helpText)
        : key_(key), displayName_(displayName), section_(section), order_(order), helpText_(helpText) {}
    
    virtual ~BaseParamDef() = default;
    
    const String& getKey() const { return key_; }
    const String& getDisplayName() const { return displayName_; }
    int getSection() const { return section_; }
    int getOrder() const { return order_; }
    const String& getHelpText() const { return helpText_; }
    
    virtual void toJson(JsonObject& obj) const = 0;
    virtual bool fromJson(const JsonVariant& value) = 0;
    virtual bool loadFromNvs(Preferences& prefs) = 0;
    virtual bool saveToNvs(Preferences& prefs) const = 0;
    virtual void resetToDefault() = 0;
    
protected:
    String key_;
    String displayName_;
    int section_;
    int order_;
    String helpText_;
};

/**
 * @brief Type-safe parameter definition for editable configuration values
 */
template<typename T>
class ParamDef : public BaseParamDef {
public:
    ParamDef(const String& key, T defaultValue, const String& displayName, int section, int order, 
             const String& helpText, T minValue = T{}, T maxValue = T{}, 
             std::function<bool()> showCondition = nullptr)
        : BaseParamDef(key, displayName, section, order, helpText),
          defaultValue_(defaultValue), currentValue_(defaultValue),
          minValue_(minValue), maxValue_(maxValue), showCondition_(showCondition) {}

    // Type-safe getter
    T get() const { return currentValue_; }
    
    // Type-safe setter with validation
    bool set(const T& value) {
        if (!isValid(value)) {
            return false;
        }
        currentValue_ = value;
        markDirty();
        return true;
    }
    
    // Validation
    bool isValid(const T& value) const {
        if constexpr (std::is_arithmetic_v<T>) {
            return value >= minValue_ && value <= maxValue_;
        }
        return true; // String and bool types are always valid if they parse correctly
    }
    
    // Reset to default
    void resetToDefault() override {
        currentValue_ = defaultValue_;
        markDirty();
    }
    
    // Condition for showing in UI
    bool shouldShow() const {
        return !showCondition_ || showCondition_();
    }
    
    // JSON serialization
    void toJson(JsonObject& obj) const override {
        obj["key"] = key_;
        obj["displayName"] = displayName_;
        obj["section"] = section_;
        obj["order"] = order_;
        obj["helpText"] = helpText_;
        obj["type"] = getTypeName();
        
        if constexpr (std::is_same_v<T, bool>) {
            obj["value"] = currentValue_;
            obj["default"] = defaultValue_;
        } else if constexpr (std::is_arithmetic_v<T>) {
            obj["value"] = currentValue_;
            obj["default"] = defaultValue_;
            obj["min"] = minValue_;
            obj["max"] = maxValue_;
        } else if constexpr (std::is_same_v<T, String>) {
            obj["value"] = currentValue_;
            obj["default"] = defaultValue_;
        }
    }
    
    // JSON deserialization
    bool fromJson(const JsonVariant& value) override {
        T newValue;
        if constexpr (std::is_same_v<T, bool>) {
            newValue = value.as<bool>();
        } else if constexpr (std::is_same_v<T, int>) {
            newValue = value.as<int>();
        } else if constexpr (std::is_same_v<T, double>) {
            newValue = value.as<double>();
        } else if constexpr (std::is_same_v<T, String>) {
            newValue = value.as<String>();
        } else {
            return false;
        }
        
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
        } else if constexpr (std::is_same_v<T, int>) {
            value = prefs.getInt(nvsKey.c_str());
        } else if constexpr (std::is_same_v<T, double>) {
            value = prefs.getDouble(nvsKey.c_str());
        } else if constexpr (std::is_same_v<T, String>) {
            value = prefs.getString(nvsKey.c_str());
        } else {
            return false;
        }
        
        currentValue_ = value;
        return true;
    }
    
    bool saveToNvs(Preferences& prefs) const override {
        if (!isDirty_) {
            return true; // No need to save
        }
        
        String nvsKey = generateNvsKey();
        
        if constexpr (std::is_same_v<T, bool>) {
            return prefs.putBool(nvsKey.c_str(), currentValue_);
        } else if constexpr (std::is_same_v<T, int>) {
            return prefs.putInt(nvsKey.c_str(), currentValue_);
        } else if constexpr (std::is_same_v<T, double>) {
            return prefs.putDouble(nvsKey.c_str(), currentValue_);
        } else if constexpr (std::is_same_v<T, String>) {
            return prefs.putString(nvsKey.c_str(), currentValue_);
        }
        
        return false;
    }

private:
    T defaultValue_;
    T currentValue_;
    T minValue_;
    T maxValue_;
    std::function<bool()> showCondition_;
    mutable bool isDirty_ = false;
    
    void markDirty() const { isDirty_ = true; }
    
    String generateNvsKey() const {
        // Generate short hash-based key for NVS (max 15 chars)
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
        if constexpr (std::is_same_v<T, bool>) return "bool";
        else if constexpr (std::is_same_v<T, int>) return "int";
        else if constexpr (std::is_same_v<T, double>) return "double";
        else if constexpr (std::is_same_v<T, String>) return "string";
        else return "unknown";
    }
};

/**
 * @brief Specialized parameter definition for enum types
 */
template<typename E>
class EnumParamDef : public BaseParamDef {
    static_assert(std::is_enum_v<E>, "EnumParamDef requires an enum type");
    
public:
    EnumParamDef(const String& key, E defaultValue, const String& displayName, int section, int order,
                 const String& helpText, std::vector<std::pair<E, String>> options = {},
                 std::function<bool()> showCondition = nullptr)
        : BaseParamDef(key, displayName, section, order, helpText),
          defaultValue_(defaultValue), currentValue_(defaultValue),
          options_(options), showCondition_(showCondition) {}
    
    E get() const { return currentValue_; }
    
    bool set(const E& value) {
        if (!isValid(value)) {
            return false;
        }
        currentValue_ = value;
        markDirty();
        return true;
    }
    
    bool isValid(const E& value) const {
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
        markDirty();
    }
    
    bool shouldShow() const {
        return !showCondition_ || showCondition_();
    }
    
    void toJson(JsonObject& obj) const override {
        obj["key"] = key_;
        obj["displayName"] = displayName_;
        obj["section"] = section_;
        obj["order"] = order_;
        obj["helpText"] = helpText_;
        obj["type"] = "enum";
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
    
    bool fromJson(const JsonVariant& value) override {
        int intValue = value.as<int>();
        return set(static_cast<E>(intValue));
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
        if (!isDirty_) {
            return true;
        }
        
        String nvsKey = generateNvsKey();
        return prefs.putInt(nvsKey.c_str(), static_cast<int>(currentValue_));
    }

private:
    E defaultValue_;
    E currentValue_;
    std::vector<std::pair<E, String>> options_;
    std::function<bool()> showCondition_;
    mutable bool isDirty_ = false;
    
    void markDirty() const { isDirty_ = true; }
    
    String generateNvsKey() const {
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
template<typename T>
class StateParamDef {
public:
    enum class UpdateFrequency {
        REALTIME,    // Updated every loop (temperature, PID output)
        FREQUENT,    // Updated every few seconds (connection status)
        OCCASIONAL,  // Updated when requested (system info)
        STATIC       // Never changes after init (hardware info)
    };
    
    StateParamDef(const String& key, const String& displayName, int section, int order,
                  const String& helpText, std::function<T()> valueProvider,
                  UpdateFrequency frequency = UpdateFrequency::REALTIME,
                  const String& unit = "")
        : key_(key), displayName_(displayName), section_(section), order_(order),
          helpText_(helpText), valueProvider_(valueProvider), frequency_(frequency), unit_(unit) {}
    
    // Read-only access
    T get() const { return valueProvider_(); }
    
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
    void toJson(JsonObject& obj) const {
        obj["key"] = key_;
        obj["displayName"] = displayName_;
        obj["section"] = section_;
        obj["order"] = order_;
        obj["helpText"] = helpText_;
        obj["type"] = getTypeName();
        obj["value"] = get();
        obj["readonly"] = true;
        obj["frequency"] = static_cast<int>(frequency_);
        if (!unit_.isEmpty()) {
            obj["unit"] = unit_;
        }
    }
    
    const String& getKey() const { return key_; }
    const String& getDisplayName() const { return displayName_; }
    int getSection() const { return section_; }
    int getOrder() const { return order_; }
    const String& getHelpText() const { return helpText_; }
    const String& getUnit() const { return unit_; }
    UpdateFrequency getFrequency() const { return frequency_; }

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
        if constexpr (std::is_same_v<T, bool>) return "bool";
        else if constexpr (std::is_same_v<T, int>) return "int";
        else if constexpr (std::is_same_v<T, double>) return "double";
        else if constexpr (std::is_same_v<T, String>) return "string";
        else return "unknown";
    }
};

/**
 * @brief Read-only computed parameter - derives value from other parameters
 * 
 * Example: PID Ki value computed from Kp and Tn
 */
template<typename T>
class ComputedParamDef {
public:
    ComputedParamDef(const String& key, const String& displayName, int section, int order,
                     const String& helpText, std::function<T()> computation,
                     const String& unit = "")
        : key_(key), displayName_(displayName), section_(section), order_(order),
          helpText_(helpText), computation_(computation), unit_(unit) {}
    
    T get() const { return computation_(); }
    
    void toJson(JsonObject& obj) const {
        obj["key"] = key_;
        obj["displayName"] = displayName_;
        obj["section"] = section_;
        obj["order"] = order_;
        obj["helpText"] = helpText_;
        obj["type"] = getTypeName();
        obj["value"] = get();
        obj["readonly"] = true;
        obj["computed"] = true;
        if (!unit_.isEmpty()) {
            obj["unit"] = unit_;
        }
    }
    
    const String& getKey() const { return key_; }

private:
    String key_;
    String displayName_;
    int section_;
    int order_;
    String helpText_;
    String unit_;
    std::function<T()> computation_;
    
    const char* getTypeName() const {
        if constexpr (std::is_same_v<T, bool>) return "bool";
        else if constexpr (std::is_same_v<T, int>) return "int";
        else if constexpr (std::is_same_v<T, double>) return "double";
        else if constexpr (std::is_same_v<T, String>) return "string";
        else return "unknown";
    }
};

/**
 * @brief Next-generation configuration system with direct parameter access
 */
class ConfigV2 {
public:
    static ConfigV2& getInstance() {
        static ConfigV2 instance;
        return instance;
    }
    
    // Core PID Parameters
    ParamDef<bool> pidEnabled{"pid.enabled", true, "Enable PID Controller", 0, 101, 
                              "Enables or disables the PID temperature controller"};
    
    ParamDef<bool> pidUsePonM{"pid.use_ponm", false, "Enable PonM", 0, 102, 
                              "Use PonM mode (Proportional on Measurement)"};
    
    ParamDef<double> pidRegularKp{"pid.regular.kp", 69.0, "PID Kp", 0, 103, 
                                  "Proportional gain for regular PID", 0.0, 200.0};
    
    ParamDef<double> pidRegularTn{"pid.regular.tn", 400.0, "PID Tn", 0, 104, 
                                  "Integral time constant", 0.0, 1000.0};
    
    ParamDef<double> pidRegularTv{"pid.regular.tv", 1.0, "PID Tv", 0, 105, 
                                  "Derivative time constant", 0.0, 100.0};
    
    // Brew Parameters
    ParamDef<double> brewSetpoint{"brew.setpoint", 95.0, "Brew Temperature", 1, 201, 
                                  "Target temperature for brewing", 80.0, 110.0};
    
    ParamDef<double> brewTempOffset{"brew.temp_offset", 0.0, "Temperature Offset", 1, 202, 
                                    "Offset applied to temperature reading", -10.0, 10.0};
    
    ParamDef<bool> brewPidEnabled{"brew.pid.enabled", true, "Enable Brew PID", 1, 203, 
                                  "Enable PID control during brewing"};
    
    // Steam Parameters  
    ParamDef<double> steamSetpoint{"steam.setpoint", 120.0, "Steam Temperature", 2, 301, 
                                   "Target temperature for steam mode", 100.0, 140.0};
    
    // Hardware Parameters
    EnumParamDef<Hardware::OLEDType> hardwareOledType{"hardware.oled.type", Hardware::OLEDType::SSD1306, 
                                                      "OLED Type", 4, 401, "Type of OLED display",
                                                      {{Hardware::OLEDType::SSD1306, "SSD1306"}, 
                                                       {Hardware::OLEDType::SH1106, "SH1106"}}};
    
    ParamDef<bool> hardwareOledEnabled{"hardware.oled.enabled", true, "Enable OLED", 4, 402, 
                                       "Enable OLED display"};
    
    ParamDef<int> hardwareOledAddress{"hardware.oled.address", 0x3C, "OLED I2C Address", 4, 403, 
                                      "I2C address for OLED display", 0x3C, 0x3D};
    
    // System Parameters
    ParamDef<String> systemHostname{"system.hostname", "clevercoffee", "Hostname", 5, 501, 
                                     "Network hostname for the device"};
    
    ParamDef<bool> systemOfflineMode{"system.offline_mode", false, "Offline Mode", 5, 502, 
                                     "Disable all network functionality"};
    
    ParamDef<int> systemLogLevel{"system.log_level", 3, "Log Level", 5, 503, 
                                 "System logging level (0=Error, 1=Warning, 2=Info, 3=Debug)", 0, 3};
    
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
    
    StateParamDef<bool> stateWaterTankFull{"state.water_tank", "Water Tank Full", 6, 608,
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
    
    StateParamDef<String> stateWifiIP{"state.wifi_ip", "WiFi IP Address", 8, 802,
                                      "Current WiFi IP address",
                                      []() { return WiFi.localIP().toString(); },
                                      StateParamDef<String>::UpdateFrequency::OCCASIONAL};
    
    StateParamDef<String> stateWifiSSID{"state.wifi_ssid", "WiFi Network", 8, 803,
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
    BaseParamDef* findParameter(const String& key);
    
private:
    ConfigV2() = default;
    ~ConfigV2() = default;
    ConfigV2(const ConfigV2&) = delete;
    ConfigV2& operator=(const ConfigV2&) = delete;
    
    std::vector<BaseParamDef*> getAllParamDefs();
    
    static constexpr const char* STORAGE_NAMESPACE = "clevercoffee";
};

// Global state reference for state parameters
extern struct GlobalState g_state;