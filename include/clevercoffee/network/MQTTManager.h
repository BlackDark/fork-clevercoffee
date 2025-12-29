/**
 * @file MQTTManager.h
 * @brief RAII wrapper for MQTT functionality
 */

#pragma once

#include <ArduinoJson.h>
#include <PubSubClient.h>
#include <String>
#include <WiFiClient.h>
#include <functional>
#include <cstring>
#include <map>
#include <memory>

// Forward declarations
namespace CleverCoffee {
class UICoordinator;
class SensorCoordinator;
}

/**
 * @class MQTTManager
 * @brief RAII wrapper for MQTT connection and message handling
 *
 * This class provides safe management of MQTT connections using RAII principles.
 * It encapsulates all MQTT setup, connection, publishing, and Home Assistant discovery.
 */
class MQTTManager {
  public:
    /**
     * @brief Constructor - initializes MQTT manager
     */
    MQTTManager();

    /**
     * @brief Destructor - automatically cleans up MQTT resources
     */
    ~MQTTManager() = default;

    // Disable copy constructor and assignment operator
    MQTTManager(const MQTTManager&)            = delete;
    MQTTManager& operator=(const MQTTManager&) = delete;

    // Enable move constructor and assignment operator
    MQTTManager(MQTTManager&&)            = default;
    MQTTManager& operator=(MQTTManager&&) = default;

    /**
     * @brief Setup MQTT with configuration
     * @param hostname Device hostname
     * @return true if MQTT is enabled and configured
     */
    bool setup(const String& hostname);

     /**
      * @brief Set UI coordinator for state management
      * @param coordinator Pointer to UICoordinator
      */
     void setUICoordinator(CleverCoffee::UICoordinator* coordinator) noexcept {
         uiCoordinator_ = coordinator;
     }

     /**
      * @brief Set Sensor coordinator for scale mode management
      * @param coordinator Pointer to SensorCoordinator
      */
     void setSensorCoordinator(CleverCoffee::SensorCoordinator* coordinator) noexcept {
         sensorCoordinator_ = coordinator;
     }

     /**
      * @brief Check MQTT connection and reconnect if needed
      */
    void checkConnection();

    /**
     * @brief Process MQTT loop and handle messages
     */
    void loop();

    /**
     * @brief Publish system parameters to MQTT
     * @param continueOnError Whether to continue on errors
     * @return 0 on success, error code on failure
     */
    int writeSysParamsToMQTT(bool continueOnError = true);

    /**
     * @brief Send Home Assistant discovery messages
     * @return 0 on success, error code on failure
     */
    int sendHASSIODiscoveryMsg();

    /**
     * @brief Check if MQTT is enabled
     * @return true if MQTT is enabled
     */
    bool isEnabled() const noexcept {
        return mqttEnabled_;
    }

    /**
     * @brief Check if MQTT is connected
     * @return true if connected
     */
    bool isConnected() const noexcept {
        return const_cast<PubSubClient&>(mqttClient_).connected();
    }

    /**
     * @brief Register MQTT parameter mapping
     * @param mqttTopic MQTT topic name
     * @param configParam Configuration parameter ID
     */
    void registerParameter(const char* mqttTopic, const char* configParam);

    /**
     * @brief Register MQTT sensor callback
     * @param topic MQTT topic name
     * @param callback Function to get sensor value
     */
    void registerSensor(const char* topic, std::function<double()> callback);

    /**
     * @brief Set update running flag
     * @param running Whether update is running
     */
    void setUpdateRunning(bool running) noexcept {
        mqttUpdateRunning_ = running;
    }

    /**
     * @brief Check if update is running
     * @return true if update is running
     */
    bool isUpdateRunning() const noexcept {
        return mqttUpdateRunning_;
    }

    /**
     * @brief Check if was connected previously
     * @return true if was connected
     */
    bool wasConnected() const noexcept {
        return mqttWasConnected_;
    }

    /**
     * @brief Set was connected flag
     * @param connected Connection state
     */
    void setWasConnected(bool connected) noexcept {
        mqttWasConnected_ = connected;
    }

    /**
     * @brief Get reference to MQTT client for compatibility
     * @return Reference to PubSubClient
     */
    PubSubClient& getClient() noexcept {
        return mqttClient_;
    }

  private:
    // MQTT client and networking
    std::unique_ptr<WiFiClient> wifiClient_;
    PubSubClient                mqttClient_;

    // Configuration
    bool   mqttEnabled_;
    String serverIP_;
    int    serverPort_;
    String username_;
    String password_;
    String topicPrefix_;
    bool   hassioEnabled_;
    String hassioDiscoveryPrefix_;
    String hostname_;

    // Connection management
    unsigned long                  lastConnectionAttempt_;
    unsigned int                   reconnectCount_;
    unsigned long                  previousConnection_;
    static constexpr unsigned long reconnectInterval_ = 300000; // 5 minutes
    static constexpr unsigned long connectionDelay_   = 10000;  // 10 seconds
    static constexpr unsigned int  maxReconnects_     = 5;

    // Topics
    char topicWill_[256];
    char topicSet_[256];

    // Parameter and sensor mappings
    struct cmp_str {
        bool operator()(char const* a, char const* b) const {
            return std::strcmp(a, b) < 0;
        }
    };

    std::map<const char*, const char*, cmp_str>             mqttVars_;        ///< MQTT parameter mappings
    std::map<const char*, std::function<double()>, cmp_str> mqttSensors_;     ///< MQTT sensor callbacks


    std::map<const char*, std::string> mqttLastSent_;

    // Update management
    bool                           mqttUpdateRunning_;
    bool                           mqttWasConnected_;
    static constexpr unsigned long timeBudget_          = 10; // milliseconds per loop
    static constexpr unsigned long intervalMQTT_        = 5000;
    static constexpr unsigned long intervalMQTTBrew_    = 500;
    static constexpr unsigned long intervalMQTTStandby_ = 10000;
    unsigned long                  previousMillisMQTT_;

    // Coordinators
    CleverCoffee::UICoordinator* uiCoordinator_{nullptr}; ///< UI coordinator for state management
    CleverCoffee::SensorCoordinator* sensorCoordinator_{nullptr}; ///< Sensor coordinator for scale modes

    // Home Assistant discovery
    struct DiscoveryObject {
        String discovery_topic;
        String payload_json;
    };

    /**
     * @brief Initialize MQTT client settings
     */
    void initializeClient();

    /**
     * @brief Publish message to MQTT topic
     * @param reading Topic suffix
     * @param payload Message payload
     * @param retain Retain flag
     * @return true on success
     */
    bool publish(const char* reading, const char* payload, bool retain = false);

    /**
     * @brief Publish large message with chunking
     * @param topic Full topic path
     * @param largeMessage Message content
     * @return 0 on success, error code on failure
     */
    int publishLargeMessage(const String& topic, const String& largeMessage);

    /**
     * @brief MQTT callback for incoming messages
     * @param topic Message topic
     * @param data Message data
     * @param length Message length
     */
    void messageCallback(const char* topic, const byte* data, unsigned int length);

    /**
     * @brief Assign MQTT parameter value to configuration
     * @param param Parameter name
     * @param value Parameter value
     */
    void assignParameter(char* param, double value);

    // Home Assistant discovery helpers
    DiscoveryObject generateSwitchDevice(const String& name,
                                         const String& displayName,
                                         const String& payload_on  = "1",
                                         const String& payload_off = "0");
    DiscoveryObject generateButtonDevice(const String& name,
                                         const String& displayName,
                                         const String& payload_press = "1");
    DiscoveryObject generateSensorDevice(const String& name,
                                         const String& displayName,
                                         const String& unit_of_measurement,
                                         const String& device_class);
    DiscoveryObject generateNumberDevice(const String& name,
                                         const String& displayName,
                                         int           min_value,
                                         int           max_value,
                                         float         steps_value,
                                         const String& unit_of_measurement,
                                         const String& ui_mode = "box");
    int             publishDiscovery(const DiscoveryObject& obj);

    // Static callback wrapper
    static void         staticMessageCallback(char* topic, byte* data, unsigned int length);
    static MQTTManager* instance_;
};
