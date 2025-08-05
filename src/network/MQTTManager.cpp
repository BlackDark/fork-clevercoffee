/**
 * @file MQTTManager.cpp
 * @brief Implementation of RAII wrapper for MQTT management
 */

#include "MQTTManager.h"
#include "../Config.h"
#include "../defaults.h"
#include "../state/GlobalState.h"
#include "../utils/brewUtils.h"
#include "../utils/legacyUtils.h"
#include "Logger.h"
#include <Arduino.h>
#include <cstdio>

// Static instance for callback
MQTTManager* MQTTManager::instance_ = nullptr;

MQTTManager::MQTTManager() :
    wifiClient_(std::make_unique<WiFiClient>()),
    mqttClient_(*wifiClient_),
    mqttEnabled_(false),
    serverPort_(1883),
    lastConnectionAttempt_(0),
    reconnectCount_(0),
    previousConnection_(millis()),
    mqttUpdateRunning_(false),
    mqttWasConnected_(false),
    previousMillisMQTT_(0) {

    instance_ = this;
    initializeClient();
}

bool MQTTManager::setup(const String& hostname) {
    hostname_ = hostname;

    // Load MQTT configuration
    const auto& config = Config::getInstance();
    mqttEnabled_ = Config::getInstance().mqttEnabled.get();

    if (!mqttEnabled_) {
        LOG(INFO, "MQTT is disabled");
        return false;
    }

    serverIP_ = Config::getInstance().mqttBroker.get();
    serverPort_ = Config::getInstance().mqttPort.get();
    username_ = Config::getInstance().mqttUsername.get();
    password_ = Config::getInstance().mqttPassword.get();
    topicPrefix_ = Config::getInstance().mqttTopic.get();
    hassioEnabled_ = Config::getInstance().mqttHassioEnabled.get();
    hassioDiscoveryPrefix_ = Config::getInstance().mqttHassioPrefix.get();

    // Setup topics
    snprintf(topicWill_, sizeof(topicWill_), "%s%s/%s", topicPrefix_.c_str(), hostname_.c_str(), "status");
    snprintf(topicSet_, sizeof(topicSet_), "%s%s/+/%s", topicPrefix_.c_str(), hostname_.c_str(), "set");

    // Configure MQTT client
    mqttClient_.setServer(serverIP_.c_str(), serverPort_);
    mqttClient_.setCallback(staticMessageCallback);

    LOG(INFO, "MQTT setup completed");
    return true;
}

void MQTTManager::initializeClient() {
    // Set larger buffer size for Home Assistant discovery messages
    mqttClient_.setBufferSize(1024);
}

void MQTTManager::checkConnection() {
    if (g_state.network.offlineMode || checkBrewActive()) {
        return;
    }

    if (millis() - lastConnectionAttempt_ >= connectionDelay_ && reconnectCount_ <= maxReconnects_) {
        if (!mqttClient_.connected()) {
            lastConnectionAttempt_ = millis();
            reconnectCount_++;
            LOGF(DEBUG, "Attempting MQTT reconnection: %i", reconnectCount_);

            if (mqttClient_.connect(hostname_.c_str(), username_.c_str(), password_.c_str(), topicWill_, 0, true, "offline")) {
                mqttClient_.subscribe(topicSet_);
                LOGF(DEBUG, "Subscribed to MQTT Topic: %s", topicSet_);
                reconnectCount_ = 0;
            }
            else {
                LOGF(DEBUG, "Failed to connect to MQTT due to reason: %i", mqttClient_.state());
            }
        }
    }
    // Reset reconnect count after interval
    else if (millis() - previousConnection_ >= reconnectInterval_) {
        reconnectCount_ = 0;
        previousConnection_ = millis();
    }
}

void MQTTManager::loop() {
    if (mqttEnabled_ && mqttClient_.connected()) {
        mqttClient_.loop();
    }
}

bool MQTTManager::publish(const char* reading, const char* payload, bool retain) {
    char topic[120];
    snprintf(topic, 120, "%s%s/%s", topicPrefix_.c_str(), hostname_.c_str(), reading);
    return mqttClient_.publish(topic, payload, retain);
}

int MQTTManager::publishLargeMessage(const String& topic, const String& largeMessage) {
    constexpr size_t splitSize = 128;

    if (const size_t messageLength = largeMessage.length(); messageLength > splitSize) {
        const size_t count = messageLength / splitSize;
        mqttClient_.beginPublish(topic.c_str(), messageLength, true);

        for (size_t i = 0; i < count; i++) {
            const size_t startIndex = i * splitSize;
            const size_t endIndex = startIndex + splitSize;
            mqttClient_.print(largeMessage.substring(startIndex, endIndex));
        }

        mqttClient_.print(largeMessage.substring(count * splitSize));

        if (const int publishResult = mqttClient_.endPublish(); publishResult == 0) {
            LOG(WARNING, "[MQTT] PublishLargeMessage sent failed");
            return 1;
        }
        else {
            return 0;
        }
    }
    else {
        boolean publishResult = mqttClient_.publish(topic.c_str(), largeMessage.c_str());
        return publishResult ? 0 : -1;
    }
}

void MQTTManager::staticMessageCallback(char* topic, byte* data, unsigned int length) {
    if (instance_) {
        instance_->messageCallback(topic, data, length);
    }
}

void MQTTManager::messageCallback(const char* topic, const byte* data, unsigned int length) {
    char topic_str[256];
    strncpy(topic_str, topic, sizeof(topic_str) - 1);
    topic_str[255] = '\0';

    char data_str[length + 1];
    memcpy(data_str, data, length);
    data_str[length] = '\0';

    char topic_pattern[255];
    char configVar[120];
    char cmd[64];
    double data_double;

    snprintf(topic_pattern, sizeof(topic_pattern), "%s%s/%%[^\\/]/%%[^\\/]", topicPrefix_.c_str(), hostname_.c_str());

    if (sscanf(topic_str, topic_pattern, configVar, cmd) != 2 || strcmp(cmd, "set") != 0) {
        LOGF(WARNING, "Invalid MQTT topic/command: %s", topic_str);
        return;
    }

    LOGF(DEBUG, "Received MQTT command %s %s", topic_str, data_str);

    sscanf(data_str, "%lf", &data_double);
    assignParameter(configVar, data_double);
}

void MQTTManager::assignParameter(char* param, double value) {
    try {
        const auto it = g_state.network.mqttVars.find(param);

        if (it == g_state.network.mqttVars.end()) {
            LOGF(WARNING, "MQTT topic %s not found in mapping", param);
            return;
        }

        const char* parameterId = it->second;
        auto& config = Config::getInstance();

        LOGF(DEBUG, "Getting MQTT parameter: %s", parameterId);

        bool success = false;

        // Try boolean first
        if (value == 0.0 || value == 1.0) {
            bool boolValue = (value == 1.0);
            success = Config::getInstance().set<bool>(parameterId, boolValue);
        }

        // Try integer if boolean failed
        if (!success && value == static_cast<int>(value)) {
            int intValue = static_cast<int>(value);
            success = Config::getInstance().set<int>(parameterId, intValue);
        }

        // Try uint8 if integer failed
        if (!success && value >= 0 && value <= 255 && value == static_cast<uint8_t>(value)) {
            uint8_t uint8Value = static_cast<uint8_t>(value);
            success = Config::getInstance().set<uint8_t>(parameterId, uint8Value);
            if (success && strcasecmp(param, "steamON") == 0) {
                g_state.machine.steamFirstON = uint8Value;
            }
        }

        // Try float if uint8 failed
        if (!success) {
            float floatValue = static_cast<float>(value);
            success = Config::getInstance().set<float>(parameterId, floatValue);
        }

        // Try double if float failed
        if (!success) {
            success = Config::getInstance().set<double>(parameterId, value);
        }

        if (success) {
            publish(param, number2string(value), true);
            LOGF(DEBUG, "MQTT parameter %s (ID: %s) updated to %f", param, parameterId, value);
        }
        else {
            LOGF(WARNING, "Failed to update MQTT parameter %s", param);
        }
    } catch (const std::exception& e) {
        LOGF(WARNING, "Error processing MQTT parameter %s: %s", param, e.what());
    }
}

void MQTTManager::registerParameter(const char* mqttTopic, const char* configParam) {
    g_state.network.mqttVars[mqttTopic] = configParam;
}

void MQTTManager::registerSensor(const char* topic, std::function<double()> callback) {
    g_state.network.mqttSensors[topic] = callback;
}

int MQTTManager::writeSysParamsToMQTT(bool continueOnError) {
    static auto mqttVarsIt = g_state.network.mqttVars.begin();
    static auto mqttSensorsIt = g_state.network.mqttSensors.begin();
    static bool inSensors = false;

    unsigned long currentMillisMQTT = millis();
    unsigned long interval = (g_state.machine.machineState == kBrew) ? intervalMQTTBrew_ : (g_state.machine.machineState == kStandby) ? intervalMQTTStandby_ : intervalMQTT_;

    if ((currentMillisMQTT - previousMillisMQTT_ < interval) || !mqttEnabled_ || !mqttClient_.connected()) {
        return 0;
    }

    if (!inSensors && mqttVarsIt == g_state.network.mqttVars.begin()) {
        previousMillisMQTT_ = currentMillisMQTT;
        publish("status", "online");
    }

    mqttUpdateRunning_ = true;
    unsigned long start = millis();

    char data[256];
    int errorState = 0;
    auto& config = Config::getInstance();

    // Process parameter mappings
    if (!inSensors) {
        while (mqttVarsIt != g_state.network.mqttVars.end()) {
            const char* mqttTopic = mqttVarsIt->first;
            const char* parameterId = mqttVarsIt->second;

            bool paramFound = false;

            try {
                if (Config::getInstance().hasParameter(parameterId)) {
                    // Try different types
                    bool boolVal;
                    if (Config::getInstance().tryGet<bool>(parameterId, boolVal)) {
                        snprintf(data, sizeof(data), "%d", boolVal ? 1 : 0);
                        paramFound = true;
                    }
                    else {
                        int intVal;
                        if (Config::getInstance().tryGet<int>(parameterId, intVal)) {
                            snprintf(data, sizeof(data), "%d", intVal);
                            paramFound = true;
                        }
                        else {
                            uint8_t uint8Val;
                            if (Config::getInstance().tryGet<uint8_t>(parameterId, uint8Val)) {
                                snprintf(data, sizeof(data), "%u", uint8Val);
                                paramFound = true;
                            }
                            else {
                                double doubleVal;
                                if (Config::getInstance().tryGet<double>(parameterId, doubleVal)) {
                                    snprintf(data, sizeof(data), "%.2f", doubleVal);
                                    paramFound = true;
                                }
                                else {
                                    float floatVal;
                                    if (Config::getInstance().tryGet<float>(parameterId, floatVal)) {
                                        snprintf(data, sizeof(data), "%.2f", floatVal);
                                        paramFound = true;
                                    }
                                    else {
                                        String stringVal;
                                        if (Config::getInstance().tryGet<String>(parameterId, stringVal)) {
                                            snprintf(data, sizeof(data), "%s", stringVal.c_str());
                                            paramFound = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } catch (const std::exception& e) {
                LOGF(WARNING, "Error getting parameter %s: %s", parameterId, e.what());
            }

            if (!paramFound) {
                if (!continueOnError) {
                    LOGF(ERROR, "Parameter %s not found for MQTT topic %s", parameterId, mqttTopic);
                    return 1;
                }
                LOGF(WARNING, "Parameter %s not found for MQTT topic %s, skipping", parameterId, mqttTopic);
                ++mqttVarsIt;
                continue;
            }

            std::string value = std::string(data);

            if (mqttLastSent_[mqttTopic] != value) {
                if (!publish(mqttTopic, data, true)) {
                    errorState = mqttClient_.state();
                    if (!continueOnError) {
                        LOGF(ERROR, "Failed to publish parameter %s to MQTT, error: %d", mqttTopic, errorState);
                        return errorState;
                    }
                    LOGF(WARNING, "Failed to publish parameter %s to MQTT, error: %d", mqttTopic, errorState);
                }
                else {
                    mqttLastSent_[mqttTopic] = value;
                    IFLOG(DEBUG) {
                        LOGF(DEBUG, "Published %s = %s to MQTT", mqttTopic, data);
                    }
                }
            }

            ++mqttVarsIt;

            if (millis() - start >= timeBudget_) {
                return 0;
            }
        }

        mqttVarsIt = g_state.network.mqttVars.begin();
        inSensors = true;
    }

    // Process sensor callbacks
    while (mqttSensorsIt != g_state.network.mqttSensors.end()) {
        const char* topic = mqttSensorsIt->first;
        const auto& sensorFunc = mqttSensorsIt->second;
        std::string value = number2string(sensorFunc());

        if (mqttLastSent_[topic] != value) {
            if (!publish(topic, value.c_str())) {
                errorState = mqttClient_.state();
                if (!continueOnError) {
                    return errorState;
                }
            }
            else {
                mqttLastSent_[topic] = value;
            }
        }

        ++mqttSensorsIt;

        if (millis() - start >= timeBudget_) {
            return 0;
        }
    }

    mqttSensorsIt = g_state.network.mqttSensors.begin();
    inSensors = false;

    return 0;
}

// Home Assistant Discovery Implementation

MQTTManager::DiscoveryObject MQTTManager::generateSwitchDevice(const String& name, const String& displayName, const String& payload_on, const String& payload_off) {
    String mqtt_topic = String(topicPrefix_) + hostname_;
    DiscoveryObject switch_device;
    String unique_id = "clevercoffee-" + hostname_;
    String SwitchDiscoveryTopic = hassioDiscoveryPrefix_ + "/switch/";

    String switch_command_topic = mqtt_topic + "/" + name + "/set";
    String switch_state_topic = mqtt_topic + "/" + name;

    switch_device.discovery_topic = SwitchDiscoveryTopic + unique_id + "/" + name + "/config";

    JsonDocument deviceMapDoc;
    deviceMapDoc["identifiers"] = hostname_;
    deviceMapDoc["manufacturer"] = "CleverCoffee";
    deviceMapDoc["name"] = hostname_;

    JsonDocument switchConfigDoc;
    switchConfigDoc["name"] = displayName;
    switchConfigDoc["command_topic"] = switch_command_topic;
    switchConfigDoc["state_topic"] = switch_state_topic;
    switchConfigDoc["unique_id"] = unique_id + "-" + name;
    switchConfigDoc["payload_on"] = payload_on;
    switchConfigDoc["payload_off"] = payload_off;
    switchConfigDoc["payload_available"] = "online";
    switchConfigDoc["payload_not_available"] = "offline";
    switchConfigDoc["availability_topic"] = mqtt_topic + "/status";

    auto switchDeviceField = switchConfigDoc["device"].to<JsonObject>();

    for (JsonPair keyValue : deviceMapDoc.as<JsonObject>()) {
        switchDeviceField[keyValue.key()] = keyValue.value();
    }

    String switchConfigDocBuffer;
    serializeJson(switchConfigDoc, switchConfigDocBuffer);

    switch_device.payload_json = switchConfigDocBuffer;

    return switch_device;
}

MQTTManager::DiscoveryObject MQTTManager::generateButtonDevice(const String& name, const String& displayName, const String& payload_press) {
    String mqtt_topic = String(topicPrefix_) + hostname_;
    DiscoveryObject button_device;
    String unique_id = "clevercoffee-" + hostname_;
    String buttonDiscoveryTopic = hassioDiscoveryPrefix_ + "/button/";

    String button_command_topic = mqtt_topic + "/" + name + "/set";
    String button_state_topic = mqtt_topic + "/" + name;

    button_device.discovery_topic = buttonDiscoveryTopic + unique_id + "/" + name + "/config";

    JsonDocument deviceMapDoc;
    deviceMapDoc["identifiers"] = hostname_;
    deviceMapDoc["manufacturer"] = "CleverCoffee";
    deviceMapDoc["name"] = hostname_;

    JsonDocument buttonConfigDoc;
    buttonConfigDoc["name"] = displayName;
    buttonConfigDoc["command_topic"] = button_command_topic;
    buttonConfigDoc["state_topic"] = button_state_topic;
    buttonConfigDoc["unique_id"] = unique_id + "-" + name;
    buttonConfigDoc["payload_press"] = payload_press;
    buttonConfigDoc["payload_available"] = "online";
    buttonConfigDoc["payload_not_available"] = "offline";
    buttonConfigDoc["availability_topic"] = mqtt_topic + "/status";

    auto buttonDeviceField = buttonConfigDoc["device"].to<JsonObject>();

    for (JsonPair keyValue : deviceMapDoc.as<JsonObject>()) {
        buttonDeviceField[keyValue.key()] = keyValue.value();
    }

    String buttonConfigDocBuffer;
    serializeJson(buttonConfigDoc, buttonConfigDocBuffer);
    LOG(DEBUG, "Generated button device");
    button_device.payload_json = buttonConfigDocBuffer;

    return button_device;
}

MQTTManager::DiscoveryObject MQTTManager::generateSensorDevice(const String& name, const String& displayName, const String& unit_of_measurement, const String& device_class) {
    String mqtt_topic = String(topicPrefix_) + hostname_;
    DiscoveryObject sensor_device;
    String unique_id = "clevercoffee-" + hostname_;
    String SensorDiscoveryTopic = hassioDiscoveryPrefix_ + "/sensor/";

    String sensor_state_topic = mqtt_topic + "/" + name;
    sensor_device.discovery_topic = SensorDiscoveryTopic + unique_id + "/" + name + "/config";

    JsonDocument deviceMapDoc;
    deviceMapDoc["identifiers"] = hostname_;
    deviceMapDoc["manufacturer"] = "CleverCoffee";
    deviceMapDoc["name"] = hostname_;

    JsonDocument sensorConfigDoc;
    sensorConfigDoc["name"] = displayName;
    sensorConfigDoc["state_topic"] = sensor_state_topic;
    sensorConfigDoc["unique_id"] = unique_id + "-" + name;
    sensorConfigDoc["unit_of_measurement"] = unit_of_measurement;
    sensorConfigDoc["device_class"] = device_class;
    sensorConfigDoc["payload_available"] = "online";
    sensorConfigDoc["payload_not_available"] = "offline";
    sensorConfigDoc["availability_topic"] = mqtt_topic + "/status";

    auto sensorDeviceField = sensorConfigDoc["device"].to<JsonObject>();

    for (JsonPair keyValue : deviceMapDoc.as<JsonObject>()) {
        sensorDeviceField[keyValue.key()] = keyValue.value();
    }

    String sensorConfigDocBuffer;
    serializeJson(sensorConfigDoc, sensorConfigDocBuffer);

    sensor_device.payload_json = sensorConfigDocBuffer;

    return sensor_device;
}

MQTTManager::DiscoveryObject MQTTManager::generateNumberDevice(const String& name, const String& displayName, int min_value, int max_value, float steps_value, const String& unit_of_measurement, const String& ui_mode) {
    String mqtt_topic = String(topicPrefix_) + hostname_;
    DiscoveryObject number_device;
    String unique_id = "clevercoffee-" + hostname_;

    String NumberDiscoveryTopic = String(hassioDiscoveryPrefix_) + "/number/";
    number_device.discovery_topic = NumberDiscoveryTopic + unique_id + "/" + name + "/config";

    JsonDocument deviceMapDoc;
    deviceMapDoc["identifiers"] = hostname_;
    deviceMapDoc["manufacturer"] = "CleverCoffee";
    deviceMapDoc["name"] = hostname_;

    JsonDocument numberConfigDoc;
    numberConfigDoc["name"] = displayName;
    numberConfigDoc["command_topic"] = mqtt_topic + "/" + name + "/set";
    numberConfigDoc["state_topic"] = mqtt_topic + "/" + name;
    numberConfigDoc["unique_id"] = unique_id + "-" + name;
    numberConfigDoc["min"] = min_value;
    numberConfigDoc["max"] = max_value;
    numberConfigDoc["step"] = String(steps_value, 2);
    numberConfigDoc["unit_of_measurement"] = unit_of_measurement;
    numberConfigDoc["mode"] = ui_mode;
    numberConfigDoc["payload_available"] = "online";
    numberConfigDoc["payload_not_available"] = "offline";
    numberConfigDoc["availability_topic"] = mqtt_topic + "/status";

    auto numberDeviceField = numberConfigDoc["device"].to<JsonObject>();

    for (JsonPair keyValue : deviceMapDoc.as<JsonObject>()) {
        numberDeviceField[keyValue.key()] = keyValue.value();
    }

    String numberConfigDocBuffer;
    serializeJson(numberConfigDoc, numberConfigDocBuffer);

    number_device.payload_json = numberConfigDocBuffer;

    return number_device;
}

int MQTTManager::publishDiscovery(const DiscoveryObject& obj) {
    if (obj.discovery_topic.isEmpty() || obj.payload_json.isEmpty()) {
        LOGF(WARNING, "[MQTT] Skipping invalid discovery message: topic or payload is empty");
        return 1;
    }

    IFLOG(DEBUG) {
        LOGF(DEBUG, "Publishing topic: %s, payload length: %d", obj.discovery_topic.c_str(), obj.payload_json.length());
    }

    int result = publishLargeMessage(obj.discovery_topic.c_str(), obj.payload_json.c_str());

    if (result != 0) {
        LOGF(ERROR, "[MQTT] Failed to publish discovery message. Error code: %d", result);
        return 1;
    }
    return 0;
}

int MQTTManager::sendHASSIODiscoveryMsg() {
    g_state.coordination.hassioUpdateRunning = true;

    if (!mqttClient_.connected()) {
        LOG(DEBUG, "[MQTT] Failed to send Hassio Discover, MQTT Client is not connected");
        g_state.network.hassioFailed = true;
        return -1;
    }

    int failures = 0;
    const auto& config = Config::getInstance();

    // Always published devices
    failures += publishDiscovery(generateSensorDevice("machineState", "Machine State", "", "enum"));
    failures += publishDiscovery(generateSensorDevice("temperature", "Boiler Temperature", "°C", "temperature"));
    failures += publishDiscovery(generateSensorDevice("heaterPower", "Heater Power", "%", "power_factor"));

    failures += publishDiscovery(generateNumberDevice("brewSetpoint", "Brew setpoint", BREW_SETPOINT_MIN, BREW_SETPOINT_MAX, 0.1, "°C"));
    failures += publishDiscovery(generateNumberDevice("steamSetpoint", "Steam setpoint", STEAM_SETPOINT_MIN, STEAM_SETPOINT_MAX, 0.1, "°C"));
    failures += publishDiscovery(generateNumberDevice("brewTempOffset", "Brew Temp. Offset", BREW_TEMP_OFFSET_MIN, BREW_TEMP_OFFSET_MAX, 0.1, "°C"));
    failures += publishDiscovery(generateNumberDevice("steamKp", "Steam Kp", PID_KP_STEAM_MIN, PID_KP_STEAM_MAX, 0.1, ""));
    failures += publishDiscovery(generateNumberDevice("aggKp", "aggKp", PID_KP_REGULAR_MIN, PID_KP_REGULAR_MAX, 0.1, ""));
    failures += publishDiscovery(generateNumberDevice("aggTn", "aggTn", PID_TN_REGULAR_MIN, PID_TN_REGULAR_MAX, 0.1, ""));
    failures += publishDiscovery(generateNumberDevice("aggTv", "aggTv", PID_TV_REGULAR_MIN, PID_TV_REGULAR_MAX, 0.1, ""));
    failures += publishDiscovery(generateNumberDevice("aggIMax", "aggIMax", PID_I_MAX_REGULAR_MIN, PID_I_MAX_REGULAR_MAX, 0.1, ""));

    failures += publishDiscovery(generateSwitchDevice("pidON", "Use PID"));
    failures += publishDiscovery(generateSwitchDevice("steamON", "Steam"));
    failures += publishDiscovery(generateSwitchDevice("usePonM", "Use PonM"));

    // Conditional devices
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        failures += publishDiscovery(generateSensorDevice("currBrewTime", "Current Brew Time ", "s", "duration"));
        failures += publishDiscovery(generateNumberDevice("brewPidDelay", "Brew Pid Delay", BREW_PID_DELAY_MIN, BREW_PID_DELAY_MAX, 0.1, "s"));
        failures += publishDiscovery(generateNumberDevice("targetBrewTime", "Target Brew time", TARGET_BREW_TIME_MIN, TARGET_BREW_TIME_MAX, 0.1, "s"));
        failures += publishDiscovery(generateNumberDevice("preinfusion", "Preinfusion filling time", PRE_INFUSION_TIME_MIN, PRE_INFUSION_TIME_MAX, 0.1, "s"));
        failures += publishDiscovery(generateNumberDevice("preinfusionPause", "Preinfusion pause time", PRE_INFUSION_PAUSE_MIN, PRE_INFUSION_PAUSE_MAX, 0.1, "s"));
        failures += publishDiscovery(generateNumberDevice("backflushCycles", "Backflush Cycles", BACKFLUSH_CYCLES_MIN, BACKFLUSH_CYCLES_MAX, 1, ""));
        failures += publishDiscovery(generateNumberDevice("backflushFillTime", "Backflush filling time", BACKFLUSH_FILL_TIME_MIN, BACKFLUSH_FILL_TIME_MAX, 0.1, "s"));
        failures += publishDiscovery(generateNumberDevice("backflushFlushTime", "Backflush flushing time", BACKFLUSH_FLUSH_TIME_MIN, BACKFLUSH_FLUSH_TIME_MAX, 0.1, "s"));
        failures += publishDiscovery(generateSwitchDevice("backflushOn", "Backflush"));
    }

    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        failures += publishDiscovery(generateSensorDevice("currReadingWeight", "Weight", "g", "weight"));
        failures += publishDiscovery(generateSensorDevice("currBrewWeight", "current Brew Weight", "g", "weight"));
        failures += publishDiscovery(generateButtonDevice("scaleCalibrationOn", "Calibrate Scale"));
        failures += publishDiscovery(generateButtonDevice("scaleTareOn", "Tare Scale"));
        failures += publishDiscovery(generateNumberDevice("targetBrewWeight", "Brew Weight Target", TARGET_BREW_WEIGHT_MIN, TARGET_BREW_WEIGHT_MAX, 0.1, "g"));
    }

    if (Config::getInstance().hardwareSensorsPressureEnabled.get()) {
        failures += publishDiscovery(generateSensorDevice("pressure", "Pressure", "bar", "pressure"));
    }

    if (failures > 0) {
        LOGF(DEBUG, "Hassio failed to send %d entries", failures);
        g_state.network.hassioFailed = true;
    }
    else {
        LOG(DEBUG, "Hassio send successful");
        g_state.network.hassioFailed = false;
        return 0;
    }

    return -1;
}
