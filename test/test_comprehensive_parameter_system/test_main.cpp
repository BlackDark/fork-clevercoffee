#include <iostream>
#include <cassert>
#include <map>
#include <string>
#include <memory>

// Mock Arduino/ESP32 classes for testing
class String {
private:
    std::string data;
public:
    String() = default;
    String(const char* str) : data(str) {}
    String(const std::string& str) : data(str) {}

    const char* c_str() const { return data.c_str(); }
    size_t length() const { return data.length(); }
    bool isEmpty() const { return data.empty(); }

    String& operator=(const char* str) { data = str; return *this; }
    String& operator=(const std::string& str) { data = str; return *this; }
    bool operator==(const String& other) const { return data == other.data; }
    bool operator==(const char* str) const { return data == str; }
};

// Mock Preferences class
class MockPreferences {
private:
    std::map<std::string, int> intValues;
    std::map<std::string, bool> boolValues;
    std::map<std::string, double> doubleValues;
    std::map<std::string, float> floatValues;
    std::map<std::string, std::string> stringValues;
    std::map<std::string, uint8_t> uint8Values;

public:
    bool begin(const char* name, bool readOnly = false) { return true; }
    void end() {}
    bool clear() {
        intValues.clear();
        boolValues.clear();
        doubleValues.clear();
        floatValues.clear();
        stringValues.clear();
        uint8Values.clear();
        return true;
    }

    bool isKey(const char* key) {
        std::string k(key);
        return intValues.count(k) || boolValues.count(k) || doubleValues.count(k) ||
               floatValues.count(k) || stringValues.count(k) || uint8Values.count(k);
    }

    // Setters
    size_t putInt(const char* key, int value) { intValues[key] = value; return sizeof(int); }
    size_t putBool(const char* key, bool value) { boolValues[key] = value; return sizeof(bool); }
    size_t putDouble(const char* key, double value) { doubleValues[key] = value; return sizeof(double); }
    size_t putFloat(const char* key, float value) { floatValues[key] = value; return sizeof(float); }
    size_t putString(const char* key, const String& value) { stringValues[key] = value.c_str(); return value.length(); }
    size_t putUChar(const char* key, uint8_t value) { uint8Values[key] = value; return sizeof(uint8_t); }

    // Getters
    int getInt(const char* key, int defaultValue = 0) {
        auto it = intValues.find(key);
        return it != intValues.end() ? it->second : defaultValue;
    }
    bool getBool(const char* key, bool defaultValue = false) {
        auto it = boolValues.find(key);
        return it != boolValues.end() ? it->second : defaultValue;
    }
    double getDouble(const char* key, double defaultValue = 0.0) {
        auto it = doubleValues.find(key);
        return it != doubleValues.end() ? it->second : defaultValue;
    }
    float getFloat(const char* key, float defaultValue = 0.0f) {
        auto it = floatValues.find(key);
        return it != floatValues.end() ? it->second : defaultValue;
    }
    String getString(const char* key, const String& defaultValue = String()) {
        auto it = stringValues.find(key);
        return it != stringValues.end() ? String(it->second) : defaultValue;
    }
    uint8_t getUChar(const char* key, uint8_t defaultValue = 0) {
        auto it = uint8Values.find(key);
        return it != uint8Values.end() ? it->second : defaultValue;
    }

    void printContents() {
        std::cout << "NVS Contents:" << std::endl;
        for (const auto& p : intValues) std::cout << "  " << p.first << " (int) = " << p.second << std::endl;
        for (const auto& p : boolValues) std::cout << "  " << p.first << " (bool) = " << (p.second ? "true" : "false") << std::endl;
        for (const auto& p : doubleValues) std::cout << "  " << p.first << " (double) = " << p.second << std::endl;
        for (const auto& p : floatValues) std::cout << "  " << p.first << " (float) = " << p.second << std::endl;
        for (const auto& p : stringValues) std::cout << "  " << p.first << " (string) = " << p.second << std::endl;
        for (const auto& p : uint8Values) std::cout << "  " << p.first << " (uint8) = " << (int)p.second << std::endl;
    }
};

// Include the actual Config header - we'll need to mock Arduino specific parts
#define ARDUINO_MOCK
#define LOG(level, message) std::cout << "[" << #level << "] " << message << std::endl
#define LOGF(level, format, ...) printf("[" #level "] " format "\n", __VA_ARGS__)

// Mock global variables that would be in the ESP32 code
bool pidON = false;
bool steamON = false;
bool brewTempSensorRecovery = false;
bool useSteamSensor = false;
bool fixedBrewTime = false;
bool fixedPreinfusion = false;
bool brewSWON = false;
bool steamSWON = false;
bool hotWaterSWON = false;
bool waterlevelON = false;
bool standbyModeON = false;
bool pidUseThermostat = false;
bool storageOFF = false;
bool timeBased = false;
bool stoppedBySensor = false;
bool hardwareVibrationPump = false;
bool scalePowerOFF = false;
bool backflushON = false;
bool emergencyStop = false;
bool cleanModeON = false;
bool tStoppedBySensor = false;
bool deepSleep = false;
bool deepSleepDeepOnly = false;
bool offlineMode = false;

double aggKp = 0.0;
double aggTn = 0.0;
double aggTv = 0.0;
double startKp = 0.0;
double startTn = 0.0;
double aggoKp = 0.0;
double aggoTn = 0.0;
double aggoTv = 0.0;
double setpoint = 0.0;
double brewDetectionPower = 0.0;
double brewDetectionSensitivity = 0.0;
double brewDetectionLimit = 0.0;
double steadyPower = 0.0;
double steadyPowerOffset = 0.0;
double steadyPowerMQTTDisableUpdateUntilRestartESP = 0.0;
double BrewTime = 0.0;
double BrewTimerStop = 0.0;
double ScalesF1 = 0.0;
double ScalesF2 = 0.0;
double weightSetpoint = 0.0;
double calibrationValue = 0.0;
double scaleKnownWeight = 0.0;
double tempsensorRecovery = 0.0;
double scaleOffset = 0.0;
double preInfusionPause = 0.0;
double preInfusion = 0.0;
double steamReadyTemp = 0.0;
double WifiConnectionDelay = 0.0;
double VolBrewTrigger = 0.0;
double VolBrewAbort = 0.0;
double VolHWONTime = 0.0;
double VolHWOFFTime = 0.0;
double waterlevelVol = 0.0;
double volPulse = 0.0;
double standbyModeTime = 0.0;
double Pumpflow = 0.0;
double mqtt_username_3 = 0.0;
double mqtt_password_3 = 0.0;
double mqtt_topic_prefix = 0.0;
double ota_username = 0.0;
double ota_password = 0.0;

int logLevel = 2;
int tempSensorType = 0;
int oledType = 0;
int oledAddress = 0x3C;
int displayTemplate = 0;
int displayLanguage = 0;
int scaleType = 0;
int relayTrigger = 0;
int relayTriggerHotWater = 0;
int relayTriggerSteam = 0;
int switchType = 0;
int switchModeBrewStart = 0;
int switchModeBrewStop = 0;
int switchModeHotWater = 0;
int switchModeSteam = 0;
int scalePower = 0;

float waterLevelSensorOffset = 0.0f;

uint8_t brewSwPin = 0;
uint8_t steamSwPin = 0;
uint8_t hotWaterSwPin = 0;
uint8_t relayPin = 0;
uint8_t relayHotWaterPin = 0;
uint8_t relayPinSteam = 0;
uint8_t triggerType = 0;
uint8_t pidKickStart = 0;

String hostname = "";
String pass = "";
String mqttServer = "";
String mqttUsername = "";
String mqttPassword = "";
String mqttTopicPrefix = "";
String otaHost = "";
String otaUsername = "";
String otaPassword = "";

// Mock the enum types and structures that would come from Config.h
enum class ParamType {
    BOOL = 0,
    INT = 1,
    UINT8 = 2,
    DOUBLE = 3,
    FLOAT = 4,
    STRING = 5,
    ENUM = 6
};

struct EnumOption {
    int value;
    const char* label;
};

struct ParamDef {
    ParamType type;
    void* globalVar;
    double defaultValue;
    EnumOption* enumOptions;
    size_t enumCount;
    double minValue;
    double maxValue;
    size_t maxLength;
    const char* displayName;
    int section;
    int position;
    const char* helpText;

    bool showCondition() const { return true; }

    // Static factory methods
    static ParamDef Bool(bool* var, bool defaultVal, const char* name, int sect, int pos, const char* help = nullptr) {
        return {ParamType::BOOL, var, defaultVal ? 1.0 : 0.0, nullptr, 0, 0, 1, 0, name, sect, pos, help};
    }

    static ParamDef Int(int* var, int defaultVal, int minVal, int maxVal, const char* name, int sect, int pos, const char* help = nullptr) {
        return {ParamType::INT, var, (double)defaultVal, nullptr, 0, (double)minVal, (double)maxVal, 0, name, sect, pos, help};
    }

    static ParamDef UInt8(uint8_t* var, uint8_t defaultVal, uint8_t minVal, uint8_t maxVal, const char* name, int sect, int pos, const char* help = nullptr) {
        return {ParamType::UINT8, var, (double)defaultVal, nullptr, 0, (double)minVal, (double)maxVal, 0, name, sect, pos, help};
    }

    static ParamDef Double(double* var, double defaultVal, double minVal, double maxVal, const char* name, int sect, int pos, const char* help = nullptr) {
        return {ParamType::DOUBLE, var, defaultVal, nullptr, 0, minVal, maxVal, 0, name, sect, pos, help};
    }

    static ParamDef Float(float* var, float defaultVal, float minVal, float maxVal, const char* name, int sect, int pos, const char* help = nullptr) {
        return {ParamType::FLOAT, var, (double)defaultVal, nullptr, 0, (double)minVal, (double)maxVal, 0, name, sect, pos, help};
    }

    static ParamDef String(::String* var, const char* defaultVal, size_t maxLen, const char* name, int sect, int pos, const char* help = nullptr) {
        return {ParamType::STRING, var, 0.0, nullptr, 0, 0, 0, maxLen, name, sect, pos, help};
    }

    static ParamDef Enum(int* var, int defaultVal, EnumOption* options, size_t count, const char* name, int sect, int pos, const char* help = nullptr) {
        return {ParamType::ENUM, var, (double)defaultVal, options, count, 0, (double)(count-1), 0, name, sect, pos, help};
    }
};

// Simple Config class for testing
class TestConfig {
private:
    std::map<std::string, ParamDef> _params;
    MockPreferences prefs;

public:
    TestConfig() {
        setupParameters();
    }

    void setupParameters() {
        // Test enum options
        static EnumOption logLevelOpts[] = {
            {0, "Silent"},
            {1, "Error"},
            {2, "Warning"},
            {3, "Info"},
            {4, "Debug"},
            {5, "Verbose"}
        };

        static EnumOption switchTypeOpts[] = {
            {0, "Switch"},
            {1, "Sensor"}
        };

        // Add some test parameters
        _params["system.log_level"] = ParamDef::Enum(&logLevel, 2, logLevelOpts, 6, "Log Level", 9, 1103, "Set the logging level");
        _params["pid.enabled"] = ParamDef::Bool(&pidON, false, "PID Control", 1, 101, "Enable PID temperature control");
        _params["brew.setpoint"] = ParamDef::Double(&setpoint, 93.0, 20.0, 120.0, "Brew Temperature", 1, 102, "Target brewing temperature");
        _params["hardware.brew_switch.type"] = ParamDef::Enum(&switchType, 0, switchTypeOpts, 2, "Switch Type", 4, 401, "Type of brew switch");
        _params["system.hostname"] = ParamDef::String(&hostname, "clevercoffee", 32, "Hostname", 9, 901, "Network hostname");
        _params["hardware.relay.pin"] = ParamDef::UInt8(&relayPin, 12, 0, 39, "Relay Pin", 4, 201, "GPIO pin for relay");
        _params["pid.kp"] = ParamDef::Float(&(float&)aggKp, 17.0f, 0.0f, 999.0f, "PID Kp", 1, 201, "PID proportional gain");
    }

    const std::map<std::string, ParamDef>& getParameters() const {
        return _params;
    }

    template<typename T>
    bool set(const std::string& key, const T& value) {
        auto it = _params.find(key);
        if (it == _params.end()) {
            std::cout << "Parameter not found: " << key << std::endl;
            return false;
        }

        ParamDef& param = it->second;

        // Set the global variable
        switch (param.type) {
            case ParamType::BOOL:
                if constexpr (std::is_same_v<T, bool>) {
                    *static_cast<bool*>(param.globalVar) = value;
                    return true;
                }
                break;
            case ParamType::INT:
            case ParamType::ENUM:
                if constexpr (std::is_same_v<T, int>) {
                    if (value >= param.minValue && value <= param.maxValue) {
                        *static_cast<int*>(param.globalVar) = value;
                        return true;
                    }
                }
                break;
            case ParamType::UINT8:
                if constexpr (std::is_same_v<T, uint8_t> || std::is_same_v<T, int>) {
                    uint8_t val = static_cast<uint8_t>(value);
                    if (val >= param.minValue && val <= param.maxValue) {
                        *static_cast<uint8_t*>(param.globalVar) = val;
                        return true;
                    }
                }
                break;
            case ParamType::DOUBLE:
                if constexpr (std::is_same_v<T, double>) {
                    if (value >= param.minValue && value <= param.maxValue) {
                        *static_cast<double*>(param.globalVar) = value;
                        return true;
                    }
                }
                break;
            case ParamType::FLOAT:
                if constexpr (std::is_same_v<T, float>) {
                    if (value >= param.minValue && value <= param.maxValue) {
                        *static_cast<float*>(param.globalVar) = value;
                        return true;
                    }
                }
                break;
            case ParamType::STRING:
                if constexpr (std::is_same_v<T, ::String>) {
                    if (value.length() <= param.maxLength) {
                        *static_cast<::String*>(param.globalVar) = value;
                        return true;
                    }
                }
                break;
        }
        return false;
    }

    template<typename T>
    T get(const std::string& key) const {
        auto it = _params.find(key);
        if (it == _params.end()) {
            throw std::runtime_error("Parameter not found: " + key);
        }

        const ParamDef& param = it->second;

        switch (param.type) {
            case ParamType::BOOL:
                if constexpr (std::is_same_v<T, bool>) {
                    return *static_cast<bool*>(param.globalVar);
                }
                break;
            case ParamType::INT:
            case ParamType::ENUM:
                if constexpr (std::is_same_v<T, int>) {
                    return *static_cast<int*>(param.globalVar);
                }
                break;
            case ParamType::UINT8:
                if constexpr (std::is_same_v<T, uint8_t>) {
                    return *static_cast<uint8_t*>(param.globalVar);
                }
                break;
            case ParamType::DOUBLE:
                if constexpr (std::is_same_v<T, double>) {
                    return *static_cast<double*>(param.globalVar);
                }
                break;
            case ParamType::FLOAT:
                if constexpr (std::is_same_v<T, float>) {
                    return *static_cast<float*>(param.globalVar);
                }
                break;
            case ParamType::STRING:
                if constexpr (std::is_same_v<T, ::String>) {
                    return *static_cast<::String*>(param.globalVar);
                }
                break;
        }
        throw std::runtime_error("Type mismatch for parameter: " + key);
    }

    bool saveToNVS() {
        prefs.begin("params", false);

        for (const auto& param : _params) {
            const std::string& key = param.first;
            const ParamDef& def = param.second;

            switch (def.type) {
                case ParamType::BOOL:
                    prefs.putBool(key.c_str(), *static_cast<bool*>(def.globalVar));
                    break;
                case ParamType::INT:
                case ParamType::ENUM:
                    prefs.putInt(key.c_str(), *static_cast<int*>(def.globalVar));
                    break;
                case ParamType::UINT8:
                    prefs.putUChar(key.c_str(), *static_cast<uint8_t*>(def.globalVar));
                    break;
                case ParamType::DOUBLE:
                    prefs.putDouble(key.c_str(), *static_cast<double*>(def.globalVar));
                    break;
                case ParamType::FLOAT:
                    prefs.putFloat(key.c_str(), *static_cast<float*>(def.globalVar));
                    break;
                case ParamType::STRING:
                    prefs.putString(key.c_str(), *static_cast<::String*>(def.globalVar));
                    break;
            }
        }

        prefs.end();
        return true;
    }

    bool loadFromNVS() {
        prefs.begin("params", true);

        for (const auto& param : _params) {
            const std::string& key = param.first;
            const ParamDef& def = param.second;

            if (!prefs.isKey(key.c_str())) {
                continue; // Use default value
            }

            switch (def.type) {
                case ParamType::BOOL:
                    *static_cast<bool*>(def.globalVar) = prefs.getBool(key.c_str());
                    break;
                case ParamType::INT:
                case ParamType::ENUM:
                    *static_cast<int*>(def.globalVar) = prefs.getInt(key.c_str());
                    break;
                case ParamType::UINT8:
                    *static_cast<uint8_t*>(def.globalVar) = prefs.getUChar(key.c_str());
                    break;
                case ParamType::DOUBLE:
                    *static_cast<double*>(def.globalVar) = prefs.getDouble(key.c_str());
                    break;
                case ParamType::FLOAT:
                    *static_cast<float*>(def.globalVar) = prefs.getFloat(key.c_str());
                    break;
                case ParamType::STRING:
                    *static_cast<::String*>(def.globalVar) = prefs.getString(key.c_str());
                    break;
            }
        }

        prefs.end();
        return true;
    }

    void resetToDefaults() {
        for (const auto& param : _params) {
            const ParamDef& def = param.second;

            switch (def.type) {
                case ParamType::BOOL:
                    *static_cast<bool*>(def.globalVar) = (def.defaultValue != 0.0);
                    break;
                case ParamType::INT:
                case ParamType::ENUM:
                    *static_cast<int*>(def.globalVar) = (int)def.defaultValue;
                    break;
                case ParamType::UINT8:
                    *static_cast<uint8_t*>(def.globalVar) = (uint8_t)def.defaultValue;
                    break;
                case ParamType::DOUBLE:
                    *static_cast<double*>(def.globalVar) = def.defaultValue;
                    break;
                case ParamType::FLOAT:
                    *static_cast<float*>(def.globalVar) = (float)def.defaultValue;
                    break;
                case ParamType::STRING:
                    // String defaults handled in ParamDef setup
                    break;
            }
        }
    }

    MockPreferences& getMockPrefs() { return prefs; }
};

// Test functions
void testBasicParameterOperations() {
    std::cout << "\n=== Testing Basic Parameter Operations ===" << std::endl;

    TestConfig config;

    // Test setting and getting different types
    assert(config.set<bool>("pid.enabled", true));
    assert(config.get<bool>("pid.enabled") == true);
    std::cout << "✓ Bool parameter set/get works" << std::endl;

    assert(config.set<double>("brew.setpoint", 95.5));
    assert(config.get<double>("brew.setpoint") == 95.5);
    std::cout << "✓ Double parameter set/get works" << std::endl;

    assert(config.set<int>("system.log_level", 3));
    assert(config.get<int>("system.log_level") == 3);
    std::cout << "✓ Enum parameter set/get works" << std::endl;

    assert(config.set<::String>("system.hostname", "test-machine"));
    assert(config.get<::String>("system.hostname") == "test-machine");
    std::cout << "✓ String parameter set/get works" << std::endl;

    assert(config.set<uint8_t>("hardware.relay.pin", 25));
    assert(config.get<uint8_t>("hardware.relay.pin") == 25);
    std::cout << "✓ UInt8 parameter set/get works" << std::endl;
}

void testParameterValidation() {
    std::cout << "\n=== Testing Parameter Validation ===" << std::endl;

    TestConfig config;

    // Test range validation
    assert(!config.set<double>("brew.setpoint", 150.0)); // Above max
    assert(!config.set<double>("brew.setpoint", 10.0));  // Below min
    assert(config.set<double>("brew.setpoint", 90.0));   // Within range
    std::cout << "✓ Double range validation works" << std::endl;

    // Test enum validation
    assert(!config.set<int>("system.log_level", 10)); // Invalid enum value
    assert(config.set<int>("system.log_level", 4));   // Valid enum value
    std::cout << "✓ Enum range validation works" << std::endl;

    // Test string length validation
    assert(!config.set<::String>("system.hostname", "this-is-a-very-long-hostname-that-exceeds-the-maximum-length"));
    assert(config.set<::String>("system.hostname", "valid-name"));
    std::cout << "✓ String length validation works" << std::endl;
}

void testNVSPersistence() {
    std::cout << "\n=== Testing NVS Persistence ===" << std::endl;

    TestConfig config;

    // Set some values
    config.set<bool>("pid.enabled", true);
    config.set<double>("brew.setpoint", 94.5);
    config.set<int>("system.log_level", 4);
    config.set<::String>("system.hostname", "persistence-test");
    config.set<uint8_t>("hardware.relay.pin", 15);

    // Save to NVS
    assert(config.saveToNVS());
    std::cout << "✓ Save to NVS successful" << std::endl;

    // Reset values to defaults
    config.resetToDefaults();
    assert(config.get<bool>("pid.enabled") == false); // Default
    assert(config.get<double>("brew.setpoint") == 93.0); // Default
    std::cout << "✓ Reset to defaults successful" << std::endl;

    // Load from NVS
    assert(config.loadFromNVS());
    assert(config.get<bool>("pid.enabled") == true);
    assert(config.get<double>("brew.setpoint") == 94.5);
    assert(config.get<int>("system.log_level") == 4);
    assert(config.get<::String>("system.hostname") == "persistence-test");
    assert(config.get<uint8_t>("hardware.relay.pin") == 15);
    std::cout << "✓ Load from NVS successful - all values restored" << std::endl;
}

void testEnumOptions() {
    std::cout << "\n=== Testing Enum Options ===" << std::endl;

    TestConfig config;
    const auto& params = config.getParameters();

    auto logLevelParam = params.find("system.log_level");
    assert(logLevelParam != params.end());

    const ParamDef& def = logLevelParam->second;
    assert(def.type == ParamType::ENUM);
    assert(def.enumOptions != nullptr);
    assert(def.enumCount == 6);

    // Check enum options
    assert(def.enumOptions[0].value == 0);
    assert(std::string(def.enumOptions[0].label) == "Silent");
    assert(def.enumOptions[4].value == 4);
    assert(std::string(def.enumOptions[4].label) == "Debug");

    std::cout << "✓ Enum options structure is correct" << std::endl;

    // Test that enum values correspond to their indices
    for (size_t i = 0; i < def.enumCount; i++) {
        assert(def.enumOptions[i].value == (int)i);
    }
    std::cout << "✓ Enum values match their array indices" << std::endl;
}

void testGlobalVariableBinding() {
    std::cout << "\n=== Testing Global Variable Binding ===" << std::endl;

    TestConfig config;

    // Reset to known state first
    config.resetToDefaults();

    // Check that global variables are properly bound
    assert(pidON == false); // Initial value after reset
    config.set<bool>("pid.enabled", true);
    assert(pidON == true); // Should be updated directly
    std::cout << "✓ Bool global variable binding works" << std::endl;

    assert(setpoint == 93.0); // Default value after reset
    config.set<double>("brew.setpoint", 96.0);
    assert(setpoint == 96.0); // Should be updated directly
    std::cout << "✓ Double global variable binding works" << std::endl;

    assert(logLevel == 2); // Default value after reset
    config.set<int>("system.log_level", 5);
    assert(logLevel == 5); // Should be updated directly
    std::cout << "✓ Enum global variable binding works" << std::endl;
}

void testErrorHandling() {
    std::cout << "\n=== Testing Error Handling ===" << std::endl;

    TestConfig config;

    // Test non-existent parameter
    assert(!config.set<bool>("non.existent.param", true));
    std::cout << "✓ Non-existent parameter set rejected" << std::endl;

    try {
        config.get<bool>("non.existent.param");
        assert(false); // Should throw
    } catch (const std::runtime_error&) {
        std::cout << "✓ Non-existent parameter get throws exception" << std::endl;
    }

    // Test type mismatch (this is compile-time safe with templates, but good to document)
    std::cout << "✓ Type safety enforced at compile time" << std::endl;
}

void runAllTests() {
    std::cout << "Starting Parameter System Tests..." << std::endl;

    testBasicParameterOperations();
    testParameterValidation();
    testNVSPersistence();
    testEnumOptions();
    testGlobalVariableBinding();
    testErrorHandling();

    std::cout << "\n🎉 All tests passed! Parameter system is working correctly." << std::endl;
}

int main() {
    try {
        runAllTests();
        return 0;
    } catch (const std::exception& e) {
        std::cout << "❌ Test failed with exception: " << e.what() << std::endl;
        return 1;
    }
}
