/**
 * @file MQTTManager.h
 * @brief RAII wrapper for MQTT functionality
 */

#pragma once

#include "clevercoffee/network/IMQTTManager.h"
#include "clevercoffee/types/GlobalTypes.h"

#include <ArduinoJson.h>
#include <PubSubClient.h>
#include <WString.h>
#include <WiFiClient.h>
#include <cstring>
#include <functional>
#include <memory>
#include <unordered_map>

// Forward declarations
namespace CleverCoffee {
class UICoordinator;
class SensorCoordinator;
class NetworkCoordinator;
class SystemContext;
} // namespace CleverCoffee

namespace CleverCoffee::Utils {
class RetryPolicy;
class CircuitBreaker;
} // namespace CleverCoffee::Utils

/**
 * @class MQTTManager
 * @brief RAII wrapper for MQTT connection and message handling
 *
 * This class provides safe management of MQTT connections using RAII principles.
 * It encapsulates all MQTT setup, connection, publishing, and Home Assistant discovery.
 */
class MQTTManager : public IMQTTManager {
  public:
    /**
     * @brief Constructor - initializes MQTT manager
     */
    MQTTManager();

    /**
     * @brief Destructor - automatically cleans up MQTT resources
     *
     * Defined in .cpp file to allow incomplete types in header
     */
    ~MQTTManager();

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
    [[nodiscard]] bool setup(const String& hostname);

    /**
     * @brief Set UI coordinator for state management
     * @param coordinator Pointer to UICoordinator
     */
    void setUICoordinator(CleverCoffee::UICoordinator* coordinator) noexcept override {
        uiCoordinator_ = coordinator;
    }

    /**
     * @brief Set Sensor coordinator for scale mode management
     * @param coordinator Pointer to SensorCoordinator
     */
    void setSensorCoordinator(CleverCoffee::SensorCoordinator* coordinator) noexcept override {
        sensorCoordinator_ = coordinator;
    }

    /**
     * @brief Set Network coordinator for connection state management
     * @param coordinator Pointer to NetworkCoordinator
     */
    void setNetworkCoordinator(CleverCoffee::NetworkCoordinator* coordinator) noexcept override {
        networkCoordinator_ = coordinator;
    }

    /**
     * @brief Set system context for state management
     * @param context Pointer to SystemContext
     */
    void setSystemContext(CleverCoffee::SystemContext* context) noexcept override {
        systemContext_ = context;
    }

    /**
     * @brief Check MQTT connection and reconnect if needed
     */
    void checkConnection() override;

    /**
     * @brief Process MQTT loop and handle messages
     */
    void loop() override;

    /**
     * @brief Publish system parameters to MQTT
     * @param continueOnError Whether to continue on errors
     * @return 0 on success, error code on failure
     */
    int writeSysParamsToMQTT(bool continueOnError = true) override;

    /**
     * @brief Send Home Assistant discovery messages
     * @return 0 on success, error code on failure
     */
    int sendHASSIODiscoveryMsg() override;

    /**
     * @brief Check if MQTT is enabled
     * @return true if MQTT is enabled
     */
    bool isEnabled() const noexcept override {
        return mqttEnabled_;
    }

    /**
     * @brief Check if MQTT is connected
     * @return true if connected
     */
    bool isConnected() const noexcept override {
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
     * @brief Register MQTT binary sensor callback
     * @param topic MQTT topic name
     * @param callback Function to get sensor state (true = ON, false = OFF)
     */
    void registerBinarySensor(const char* topic, std::function<bool()> callback);

    /**
     * @brief Set update running flag
     * @param running Whether update is running
     */
    void setUpdateRunning(bool running) noexcept override {
        mqttUpdateRunning_ = running;
    }

    /**
     * @brief Check if update is running
     * @return true if update is running
     */
    bool isUpdateRunning() const noexcept override {
        return mqttUpdateRunning_;
    }

    /**
     * @brief Check if was connected previously
     * @return true if was connected
     */
    bool wasConnected() const noexcept override {
        return mqttWasConnected_;
    }

    /**
     * @brief Set was connected flag
     * @param connected Connection state
     */
    void setWasConnected(bool connected) noexcept override {
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

    // Connection management (legacy - kept for compatibility)
    unsigned long                  lastConnectionAttempt_;
    unsigned int                   reconnectCount_;
    unsigned long                  previousConnection_;
    static constexpr unsigned long reconnectInterval_ = 300000; // 5 minutes

    // Error recovery with exponential backoff and circuit breaker
    std::unique_ptr<CleverCoffee::Utils::RetryPolicy>    retryPolicy_;
    std::unique_ptr<CleverCoffee::Utils::CircuitBreaker> circuitBreaker_;

    // Topics
    char topicWill_[256];
    char topicSet_[256];

    // Parameter and sensor mappings
    struct cmp_str {
        bool operator()(char const* a, char const* b) const {
            return std::strcmp(a, b) < 0;
        }
    };

    std::unordered_map<const char*, const char*, hash_cstr, equal_cstr> mqttVars_; ///< MQTT parameter mappings
    std::unordered_map<const char*, std::function<double()>, hash_cstr, equal_cstr>
        mqttSensors_;                                                              ///< MQTT numeric sensor callbacks
    std::unordered_map<const char*, std::function<bool()>, hash_cstr, equal_cstr>
        mqttBinarySensors_; ///< MQTT binary sensor callbacks (ON/OFF payloads)

    // Track last sent values to avoid duplicate MQTT messages (unordered_map for O(1) lookup)
    std::unordered_map<const char*, std::string, hash_cstr, equal_cstr> mqttLastSent_;

    // Incremental publish state for writeSysParamsToMQTT() — persists across
    // time-budget slices so publishing resumes where it left off. Cursors are
    // initialized lazily on the first call, after all registrations are done.
    int                                    publishPhase_        = 0; ///< 0 = vars, 1 = sensors, 2 = binary sensors
    bool                                   publishCursorsValid_ = false;
    decltype(mqttVars_)::iterator          mqttVarsIt_;
    decltype(mqttSensors_)::iterator       mqttSensorsIt_;
    decltype(mqttBinarySensors_)::iterator mqttBinarySensorsIt_;

    // Update management
    bool                           mqttUpdateRunning_;
    bool                           mqttWasConnected_;
    static constexpr unsigned long timeBudget_          = 10; // milliseconds per loop
    static constexpr unsigned long intervalMQTT_        = 5000;
    static constexpr unsigned long intervalMQTTBrew_    = 500;
    static constexpr unsigned long intervalMQTTStandby_ = 10000;
    unsigned long                  previousMillisMQTT_;

    // Coordinators
    CleverCoffee::UICoordinator*      uiCoordinator_{nullptr};      ///< UI coordinator for state management
    CleverCoffee::SensorCoordinator*  sensorCoordinator_{nullptr};  ///< Sensor coordinator for scale modes
    CleverCoffee::NetworkCoordinator* networkCoordinator_{nullptr}; ///< Network coordinator for connection state

    // System context for state management
    CleverCoffee::SystemContext* systemContext_{nullptr};

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
    [[nodiscard]] bool publish(const char* reading, const char* payload, bool retain = false);

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
    DiscoveryObject generateBinarySensorDevice(const String& name,
                                               const String& displayName,
                                               const String& device_class,
                                               const String& payload_on  = "ON",
                                               const String& payload_off = "OFF");
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
