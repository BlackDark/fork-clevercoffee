
/**
 * @file Config.h
 *
 * @brief Centralized configuration management with NVS storage
 */

#pragma once

#include "ConfigDef.h"
#include "GlobalVariables.h"
#include "Logger.h"
#include "defaults.h"
#include "hardware/Relay.h"
#include "hardware/Switch.h"
#include <ArduinoJson.h>
#include <map>
#include <optional>

// Forward declaration to avoid circular dependency
class ParameterRegistry;

class Config {
    public:
        /**
         * @brief Initialize the configuration system
         *
         * @return true if successful, false otherwise
         */
        bool begin() {
            initializeConfigDefs();
            LOG(INFO, "Configuration system initialized");
            return true;
        }

        bool validateAndApplyFromJson(const String& jsonString) {
            JsonDocument doc;
            const DeserializationError error = deserializeJson(doc, jsonString);

            if (error) {
                LOGF(ERROR, "JSON parsing failed: %s", error.c_str());
                return false;
            }

            if (!validateAndApplyConfig(doc)) {
                return false;
            }

            return true;
        }

        /**
         * @brief Get configuration value from global variables (NVS-backed)
         */
        template <typename T>
        T get(const String& path) const {
            if (auto globalValue = getFromGlobalVariable<T>(path); globalValue.has_value()) {
                return globalValue.value();
            }

            // If no global variable found, return default value
            LOGF(WARNING, "No global variable found for path: %s", path.c_str());
            return T{};
        }

        template <typename T>
        std::optional<T> getFromGlobalVariable(const String& path) const {
            // System settings - String type
            if constexpr (std::is_same_v<T, String>) {
                if (path == "system.hostname") return hostname;
                if (path == "system.ota_password") return otaPassword;
                if (path == "system.auth.username") return authUsername;
                if (path == "system.auth.password") return authPassword;
                if (path == "mqtt.broker") return mqttBroker;
                if (path == "mqtt.username") return mqttUsername;
                if (path == "mqtt.password") return mqttPassword;
                if (path == "mqtt.topic") return mqttTopic;
                if (path == "mqtt.hassio.prefix") return mqttHassioPrefix;
            }

            // Boolean type
            if constexpr (std::is_same_v<T, bool>) {
                if (path == "system.offline_mode") return offlineMode;
                if (path == "system.auth.enabled") return authEnabled;
                if (path == "display.inverted") return displayInverted;
                if (path == "hardware.oled.enabled") return oledEnabled;
                if (path == "hardware.switches.brew.enabled") return brewSwitchEnabled;
                if (path == "hardware.switches.steam.enabled") return steamSwitchEnabled;
                if (path == "hardware.switches.power.enabled") return powerSwitchEnabled;
                if (path == "hardware.switches.hot_water.enabled") return hotWaterSwitchEnabled;
                if (path == "hardware.leds.status.enabled") return statusLedEnabled;
                if (path == "hardware.leds.status.inverted") return statusLedInverted;
                if (path == "hardware.leds.brew.enabled") return brewLedEnabled;
                if (path == "hardware.leds.brew.inverted") return brewLedInverted;
                if (path == "hardware.leds.steam.enabled") return steamLedEnabled;
                if (path == "hardware.leds.steam.inverted") return steamLedInverted;
                if (path == "hardware.sensors.pressure.enabled") return pressureSensorEnabled;
                if (path == "hardware.sensors.watertank.enabled") return waterTankSensorEnabled;
                if (path == "hardware.sensors.scale.enabled") return scaleEnabled;
                if (path == "mqtt.enabled") return mqttEnabled;
                if (path == "mqtt.hassio.enabled") return mqttHassioEnabled;
            }

            // Integer type
            if constexpr (std::is_same_v<T, int>) {
                if (path == "display.template") return displayTemplate;
                if (path == "display.language") return displayLanguage;
                if (path == "hardware.oled.type") return oledType;
                if (path == "hardware.oled.address") return oledAddress;
                if (path == "hardware.relays.heater.trigger_type") return heaterTriggerType;
                if (path == "hardware.relays.valve.trigger_type") return valveTriggerType;
                if (path == "hardware.relays.pump.trigger_type") return pumpTriggerType;
                if (path == "hardware.switches.brew.type") return brewSwitchType;
                if (path == "hardware.switches.brew.mode") return brewSwitchMode;
                if (path == "hardware.switches.steam.type") return steamSwitchType;
                if (path == "hardware.switches.steam.mode") return steamSwitchMode;
                if (path == "hardware.switches.power.type") return powerSwitchType;
                if (path == "hardware.switches.power.mode") return powerSwitchMode;
                if (path == "hardware.switches.hot_water.type") return hotWaterSwitchType;
                if (path == "hardware.switches.hot_water.mode") return hotWaterSwitchMode;
                if (path == "hardware.sensors.temperature.type") return temperatureSensorType;
                if (path == "hardware.sensors.watertank.mode") return waterTankSensorMode;
                if (path == "hardware.sensors.scale.samples") return scaleSamples;
                if (path == "hardware.sensors.scale.type") return scaleType;
                if (path == "mqtt.port") return mqttPort;
                if (path == "system.log_level") return logLevel;
            }

            // Double type
            if constexpr (std::is_same_v<T, double>) {
                if (path == "hardware.sensors.scale.calibration") return scaleCalibrationFactor;
                if (path == "hardware.sensors.scale.calibration2") return scaleCalibrationFactor2;
                if (path == "hardware.sensors.scale.known_weight") return scaleKnownWeight;
            }

            // No matching global variable found
            return std::nullopt;
        }

    private:
        std::map<std::string, ConfigDef> _configDefs;

        void initializeConfigDefs() {
            _configDefs.clear();

            // PID general
            _configDefs.emplace("pid.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("pid.use_ponm", ConfigDef::forBool(false));
            _configDefs.emplace("pid.ema_factor", ConfigDef::forDouble(EMA_FACTOR, PID_EMA_FACTOR_MIN, PID_EMA_FACTOR_MAX));

            // PID regular
            _configDefs.emplace("pid.regular.kp", ConfigDef::forDouble(AGGKP, PID_KP_REGULAR_MIN, PID_KP_REGULAR_MAX));
            _configDefs.emplace("pid.regular.tn", ConfigDef::forDouble(AGGTN, PID_TN_REGULAR_MIN, PID_TN_REGULAR_MAX));
            _configDefs.emplace("pid.regular.tv", ConfigDef::forDouble(AGGTV, PID_TV_REGULAR_MIN, PID_TV_REGULAR_MAX));
            _configDefs.emplace("pid.regular.i_max", ConfigDef::forDouble(AGGIMAX, PID_I_MAX_REGULAR_MIN, PID_I_MAX_REGULAR_MAX));

            // PID brew detection
            _configDefs.emplace("pid.bd.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("pid.bd.kp", ConfigDef::forDouble(AGGBKP, PID_KP_BD_MIN, PID_KP_BD_MAX));
            _configDefs.emplace("pid.bd.tn", ConfigDef::forDouble(AGGBTN, PID_TN_BD_MIN, PID_TN_BD_MAX));
            _configDefs.emplace("pid.bd.tv", ConfigDef::forDouble(AGGBTV, PID_TV_BD_MIN, PID_TV_BD_MAX));

            // PID steam
            _configDefs.emplace("pid.steam.kp", ConfigDef::forDouble(STEAMKP, PID_KP_STEAM_MIN, PID_KP_STEAM_MAX));

            // Brew settings
            _configDefs.emplace("brew.setpoint", ConfigDef::forDouble(SETPOINT, BREW_SETPOINT_MIN, BREW_SETPOINT_MAX));
            _configDefs.emplace("brew.temp_offset", ConfigDef::forDouble(TEMPOFFSET, BREW_TEMP_OFFSET_MIN, BREW_TEMP_OFFSET_MAX));
            _configDefs.emplace("brew.pid_delay", ConfigDef::forDouble(BREW_PID_DELAY, BREW_PID_DELAY_MIN, BREW_PID_DELAY_MAX));
            _configDefs.emplace("brew.mode", ConfigDef::forInt(0, 0, 2));
            _configDefs.emplace("brew.by_time.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("brew.by_time.target_time", ConfigDef::forDouble(TARGET_BREW_TIME, TARGET_BREW_TIME_MIN, TARGET_BREW_TIME_MAX));
            _configDefs.emplace("brew.by_weight.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("brew.by_weight.target_weight", ConfigDef::forDouble(TARGET_BREW_WEIGHT, TARGET_BREW_WEIGHT_MIN, TARGET_BREW_WEIGHT_MAX));
            _configDefs.emplace("brew.by_weight.auto_tare", ConfigDef::forBool(false));

            // Pre-infusion
            _configDefs.emplace("brew.pre_infusion.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("brew.pre_infusion.time", ConfigDef::forDouble(PRE_INFUSION_TIME, PRE_INFUSION_TIME_MIN, PRE_INFUSION_TIME_MAX));
            _configDefs.emplace("brew.pre_infusion.pause", ConfigDef::forDouble(PRE_INFUSION_PAUSE_TIME, PRE_INFUSION_PAUSE_MIN, PRE_INFUSION_PAUSE_MAX));

            // Steam
            _configDefs.emplace("steam.setpoint", ConfigDef::forDouble(STEAMSETPOINT, STEAM_SETPOINT_MIN, STEAM_SETPOINT_MAX));

            // Backflushing
            _configDefs.emplace("backflush.cycles", ConfigDef::forInt(BACKFLUSH_CYCLES, BACKFLUSH_CYCLES_MIN, BACKFLUSH_CYCLES_MAX));
            _configDefs.emplace("backflush.fill_time", ConfigDef::forDouble(BACKFLUSH_FILL_TIME, BACKFLUSH_FILL_TIME_MIN, BACKFLUSH_FILL_TIME_MAX));
            _configDefs.emplace("backflush.flush_time", ConfigDef::forDouble(BACKFLUSH_FLUSH_TIME, BACKFLUSH_FLUSH_TIME_MIN, BACKFLUSH_FLUSH_TIME_MAX));

            // Standby
            _configDefs.emplace("standby.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("standby.time", ConfigDef::forDouble(STANDBY_MODE_TIME, STANDBY_MODE_TIME_MIN, STANDBY_MODE_TIME_MAX));

            // MQTT
            _configDefs.emplace("mqtt.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("mqtt.broker", ConfigDef::forString("", MQTT_BROKER_MAX_LENGTH));
            _configDefs.emplace("mqtt.port", ConfigDef::forInt(1883, 1, 65535));
            _configDefs.emplace("mqtt.username", ConfigDef::forString(MQTT_USERNAME, USERNAME_MAX_LENGTH));
            _configDefs.emplace("mqtt.password", ConfigDef::forString(MQTT_PASSWORD, PASSWORD_MAX_LENGTH));
            _configDefs.emplace("mqtt.topic", ConfigDef::forString(MQTT_TOPIC, MQTT_TOPIC_MAX_LENGTH));
            _configDefs.emplace("mqtt.hassio.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("mqtt.hassio.prefix", ConfigDef::forString(MQTT_HASSIO_PREFIX, MQTT_HASSIO_PREFIX_MAX_LENGTH));

            // System
            _configDefs.emplace("system.hostname", ConfigDef::forString(HOSTNAME, HOSTNAME_MAX_LENGTH));
            _configDefs.emplace("system.ota_password", ConfigDef::forString(OTAPASS, PASSWORD_MAX_LENGTH));
            _configDefs.emplace("system.offline_mode", ConfigDef::forBool(false));
            _configDefs.emplace("system.log_level", ConfigDef::forInt(static_cast<int>(Logger::Level::INFO), 0, 5));
            _configDefs.emplace("system.auth.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("system.auth.username", ConfigDef::forString(AUTH_USERNAME, USERNAME_MAX_LENGTH));
            _configDefs.emplace("system.auth.password", ConfigDef::forString(AUTH_PASSWORD, PASSWORD_MAX_LENGTH));

            // Debugging
            _configDefs.emplace("system.timing_debug.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("system.showdisplay.enabled", ConfigDef::forBool(true));

            // Display
            _configDefs.emplace("display.template", ConfigDef::forInt(0, 0, 4));
            _configDefs.emplace("display.inverted", ConfigDef::forBool(false));
            _configDefs.emplace("display.language", ConfigDef::forInt(0, 0, 2));
            _configDefs.emplace("display.fullscreen_brew_timer", ConfigDef::forBool(false));
            _configDefs.emplace("display.fullscreen_manual_flush_timer", ConfigDef::forBool(false));
            _configDefs.emplace("display.fullscreen_hot_water_timer", ConfigDef::forBool(false));
            _configDefs.emplace("display.post_brew_timer_duration", ConfigDef::forDouble(POST_BREW_TIMER_DURATION, POST_BREW_TIMER_DURATION_MIN, POST_BREW_TIMER_DURATION_MAX));
            _configDefs.emplace("display.heating_logo", ConfigDef::forBool(true));
            _configDefs.emplace("display.pid_off_logo", ConfigDef::forBool(true));

            // Hardware - OLED
            _configDefs.emplace("hardware.oled.enabled", ConfigDef::forBool(true));
            _configDefs.emplace("hardware.oled.type", ConfigDef::forInt(0, 0, 1));
            _configDefs.emplace("hardware.oled.address", ConfigDef::forInt(0, 0, 1));

            // Hardware - Relays
            _configDefs.emplace("hardware.relays.heater.trigger_type", ConfigDef::forInt(Relay::HIGH_TRIGGER, 0, 1));
            _configDefs.emplace("hardware.relays.valve.trigger_type", ConfigDef::forInt(Relay::HIGH_TRIGGER, 0, 1));
            _configDefs.emplace("hardware.relays.pump.trigger_type", ConfigDef::forInt(Relay::HIGH_TRIGGER, 0, 1));

            // Hardware - Switches
            _configDefs.emplace("hardware.switches.brew.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.switches.brew.type", ConfigDef::forInt(Switch::TOGGLE, 0, 2));
            _configDefs.emplace("hardware.switches.brew.mode", ConfigDef::forInt(Switch::NORMALLY_OPEN, 0, 1));
            _configDefs.emplace("hardware.switches.steam.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.switches.steam.type", ConfigDef::forInt(Switch::TOGGLE, 0, 2));
            _configDefs.emplace("hardware.switches.steam.mode", ConfigDef::forInt(Switch::NORMALLY_OPEN, 0, 1));
            _configDefs.emplace("hardware.switches.power.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.switches.power.type", ConfigDef::forInt(Switch::TOGGLE, 0, 2));
            _configDefs.emplace("hardware.switches.power.mode", ConfigDef::forInt(Switch::NORMALLY_OPEN, 0, 1));
            _configDefs.emplace("hardware.switches.hot_water.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.switches.hot_water.type", ConfigDef::forInt(Switch::TOGGLE, 0, 2));
            _configDefs.emplace("hardware.switches.hot_water.mode", ConfigDef::forInt(Switch::NORMALLY_OPEN, 0, 1));

            // Hardware - LEDs
            _configDefs.emplace("hardware.leds.status.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.leds.status.inverted", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.leds.brew.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.leds.brew.inverted", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.leds.steam.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.leds.steam.inverted", ConfigDef::forBool(false));

            // Hardware - Sensors
            _configDefs.emplace("hardware.sensors.temperature.type", ConfigDef::forInt(0, 0, 1));
            _configDefs.emplace("hardware.sensors.pressure.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.sensors.watertank.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.sensors.watertank.mode", ConfigDef::forInt(Switch::NORMALLY_CLOSED, 0, 1));

            // Scale
            _configDefs.emplace("hardware.sensors.scale.enabled", ConfigDef::forBool(false));
            _configDefs.emplace("hardware.sensors.scale.samples", ConfigDef::forInt(SCALE_SAMPLES, 1, 20));
            _configDefs.emplace("hardware.sensors.scale.type", ConfigDef::forInt(0, 0, 5));
            _configDefs.emplace("hardware.sensors.scale.calibration", ConfigDef::forDouble(SCALE_CALIBRATION_FACTOR, SCALE_CALIBRATION_MIN, SCALE_CALIBRATION_MAX));
            _configDefs.emplace("hardware.sensors.scale.calibration2", ConfigDef::forDouble(SCALE_CALIBRATION_FACTOR, SCALE_CALIBRATION_MIN, SCALE_CALIBRATION_MAX));
            _configDefs.emplace("hardware.sensors.scale.known_weight", ConfigDef::forDouble(SCALE_KNOWN_WEIGHT, SCALE_KNOWN_WEIGHT_MIN, SCALE_KNOWN_WEIGHT_MAX));
        }

        bool validateAndApplyConfig(const JsonDocument& doc);
};
