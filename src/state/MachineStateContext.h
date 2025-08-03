/**
 * @file MachineStateContext.h
 * @brief Context class providing access to all machine resources for state implementations
 */

#pragma once

#include <memory>

// Forward declarations
class Config;
class DisplayManager;
class HardwareManager;
class SensorManager;
class MQTTManager;
class CleverCoffeeWiFiManager;
class U8G2;
class TempSensor;
class Switch;
class Relay;
class LED;
class Scale;

/**
 * @class MachineStateContext
 * @brief Provides unified access to all machine resources and state data
 *
 * The MachineStateContext serves as the interface between state implementations
 * and the coffee machine's hardware, sensors, configuration, and control systems.
 * It encapsulates all necessary data and provides clean APIs for state logic.
 */
class MachineStateContext {
    public:
        /**
         * @brief Construct context with references to all managers
         */
        MachineStateContext(DisplayManager* displayManager, HardwareManager* hardwareManager, SensorManager* sensorManager, CleverCoffeeWiFiManager* wifiManager, MQTTManager* mqttManager);

        /**
         * @brief Virtual destructor for proper cleanup
         */
        virtual ~MachineStateContext() = default;

        // === Hardware Access ===

        /**
         * @brief Get display manager
         */
        DisplayManager* getDisplayManager() const {
            return displayManager_;
        }

        /**
         * @brief Get hardware manager
         */
        HardwareManager* getHardwareManager() const {
            return hardwareManager_;
        }

        /**
         * @brief Get sensor manager
         */
        SensorManager* getSensorManager() const {
            return sensorManager_;
        }

        /**
         * @brief Get WiFi manager
         */
        CleverCoffeeWiFiManager* getWiFiManager() const {
            return wifiManager_;
        }

        /**
         * @brief Get MQTT manager
         */
        MQTTManager* getMQTTManager() const {
            return mqttManager_;
        }

        // === Hardware Component Access ===

        /**
         * @brief Get temperature sensor
         */
        TempSensor* getTemperatureSensor() const;

        /**
         * @brief Get water tank sensor
         */
        Switch* getWaterTankSensor() const;

        /**
         * @brief Get brew switch
         */
        Switch* getBrewSwitch() const;

        /**
         * @brief Get steam switch
         */
        Switch* getSteamSwitch() const;

        /**
         * @brief Get hot water switch
         */
        Switch* getHotWaterSwitch() const;

        /**
         * @brief Get power switch
         */
        Switch* getPowerSwitch() const;

        /**
         * @brief Get heater relay
         */
        Relay* getHeaterRelay() const;

        /**
         * @brief Get pump relay
         */
        Relay* getPumpRelay() const;

        /**
         * @brief Get valve relay
         */
        Relay* getValveRelay() const;

        /**
         * @brief Get status LED
         */
        LED* getStatusLED() const;

        /**
         * @brief Get brew LED
         */
        LED* getBrewLED() const;

        /**
         * @brief Get steam LED
         */
        LED* getSteamLED() const;

        /**
         * @brief Get scale
         */
        Scale* getScale() const;

        // === Sensor Data Access ===

        /**
         * @brief Get current temperature reading
         */
        double getCurrentTemperature() const;

        /**
         * @brief Check if temperature sensor has error
         */
        bool hasTemperatureError() const;

        /**
         * @brief Check if water tank is full
         */
        bool isWaterTankFull() const;

        /**
         * @brief Get current pressure reading
         */
        float getCurrentPressure() const;

        /**
         * @brief Get filtered pressure reading
         */
        float getFilteredPressure() const;

        /**
         * @brief Get current weight reading
         */
        float getCurrentWeight() const;

        /**
         * @brief Get current brew weight
         */
        float getCurrentBrewWeight() const;

        /**
         * @brief Check if scale has error
         */
        bool hasScaleError() const;

        /**
         * @brief Check if any sensors have errors
         */
        bool hasSensorError() const;

        // === Process Control Functions ===

        /**
         * @brief Check if brew process is active
         */
        bool isBrewActive() const;

        /**
         * @brief Check if manual flush is active
         */
        bool isManualFlushActive() const;

        /**
         * @brief Check if steam is active
         */
        bool isSteamActive() const;

        /**
         * @brief Check if hot water process is active
         */
        bool isHotWaterActive() const;

        /**
         * @brief Check if backflush is active
         */
        bool isBackflushActive() const;

        // === System State Access ===

        /**
         * @brief Check if PID is enabled
         */
        bool isPidEnabled() const;

        /**
         * @brief Check if emergency stop is active
         */
        bool isEmergencyStop() const;

        /**
         * @brief Check if standby mode should activate
         */
        bool shouldEnterStandby() const;

        /**
         * @brief Get standby remaining time
         */
        unsigned long getStandbyRemainingTime() const;

        // === Configuration Access ===

        /**
         * @brief Get configuration instance
         */
        Config& getConfig() const;

        /**
         * @brief Check if feature is enabled
         */
        template <typename T>
        T getConfigValue(const char* key) const;

        // === Timing Functions ===

        /**
         * @brief Get current time in milliseconds
         */
        unsigned long getCurrentTime() const;

        /**
         * @brief Reset standby timer for given state
         */
        void resetStandbyTimer(int stateId) const;

        // === Control Functions ===

        /**
         * @brief Set steam mode
         */
        void setSteamMode(bool enabled) const;

        /**
         * @brief Set PID runtime state
         */
        void setPidRuntimeState(bool enabled) const;

        /**
         * @brief Perform safe shutdown
         */
        void performSafeShutdown() const;

        // === Display Functions ===

        /**
         * @brief Get U8G2 display instance
         */
        U8G2* getDisplay() const;

        /**
         * @brief Set display power save mode
         */
        void setDisplayPowerSave(int mode) const;

        // === Logging Functions ===

        /**
         * @brief Log state transition
         */
        void logStateTransition(int fromState, int toState, const char* reason = nullptr) const;

        /**
         * @brief Log state entry
         */
        void logStateEntry(int stateId, const char* stateName) const;

        /**
         * @brief Log state exit
         */
        void logStateExit(int stateId, const char* stateName) const;

        // === MQTT Integration ===

        /**
         * @brief Check if MQTT reconnection count should be reset
         */
        void resetMqttReconnectCount() const;

    private:
        // Manager references
        DisplayManager* displayManager_;
        HardwareManager* hardwareManager_;
        SensorManager* sensorManager_;
        CleverCoffeeWiFiManager* wifiManager_;
        MQTTManager* mqttManager_;
};
