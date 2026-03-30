/**
 * @file ConfigV2Usage.cpp
 * @brief Example usage of the new ConfigV2 system
 *
 * This file demonstrates how the new type-safe configuration system would be used
 * throughout the codebase, replacing string-based parameter access.
 */

#include "../src/ConfigV2.h"
#include "Logger.h"

void exampleUsage() {
    // Initialize the configuration system
    ConfigV2& config = ConfigV2::getInstance();
    config.begin();

    // === TYPE-SAFE PARAMETER ACCESS ===

    // Reading parameters - fully type-safe, no strings needed
    if (config.pidEnabled.get()) {
        double kp = config.pidRegularKp.get();
        double brewTemp = config.brewSetpoint.get();

        LOGF(INFO, "PID enabled with Kp=%.2f, target temp=%.2f", kp, brewTemp);
    }

    // Setting parameters with automatic validation
    if (!config.brewSetpoint.set(105.0)) {
        LOG(ERROR, "Invalid brew temperature - outside valid range");
    }

    // Enum parameter access - type-safe
    if (config.hardwareOledType.get() == Hardware::OLEDType::SSD1306) {
        LOG(INFO, "Using SSD1306 OLED display");
    }

    // === READ-ONLY STATE ACCESS ===

    // Access current system state (read-only)
    double currentTemp = config.stateTemperature.get();
    double heaterPower = config.stateHeaterPower.get();
    int machineState = config.stateMachineState.get();

    LOGF(INFO, "Current: temp=%.2f°C, power=%.1f%%, state=%d",
         currentTemp, heaterPower, machineState);

    // === CONDITIONAL PARAMETERS ===

    // Parameters can have show conditions
    if (config.hardwareOledEnabled.get()) {
        // Only access OLED-related parameters if OLED is enabled
        int address = config.hardwareOledAddress.get();
        LOGF(INFO, "OLED enabled at address 0x%02X", address);
    }

    // === BULK OPERATIONS ===

    // Reset all parameters to defaults
    // config.resetAllToDefaults();

    // Export configuration to JSON
    String jsonConfig = config.exportToJson();
    LOG(INFO, "Exported config: " + jsonConfig);

    // Import configuration from JSON
    // config.importFromJson(jsonConfig);

    // Save all changes to NVS
    config.saveAll();
}

// === INTEGRATION WITH EXISTING CODE ===

void pidControllerExample() {
    ConfigV2& config = ConfigV2::getInstance();

    // Replace old string-based access:
    // OLD: Config::getInstance().get<bool>("pid.enabled")
    // NEW: config.pidEnabled.get()

    if (config.pidEnabled.get()) {
        // Replace old access:
        // OLD: Config::getInstance().get<double>("pid.regular.kp")
        // NEW: config.pidRegularKp.get()

        double kp = config.pidRegularKp.get();
        double tn = config.pidRegularTn.get();
        double tv = config.pidRegularTv.get();

        // Calculate derived values
        double ki = tn > 0 ? kp / tn : 0;
        double kd = tv * kp;

        LOGF(INFO, "PID parameters: Kp=%.2f, Ki=%.4f, Kd=%.2f", kp, ki, kd);
    }
}

void webInterfaceExample() {
    ConfigV2& config = ConfigV2::getInstance();

    // For web interface - get all parameters as JSON
    JsonDocument doc;
    JsonArray paramArray = doc["parameters"].to<JsonArray>();
    config.getAllParameters(paramArray, "behavior"); // Filter by category

    JsonArray stateArray = doc["state"].to<JsonArray>();
    config.getAllStateParams(stateArray);

    String response;
    serializeJson(doc, response);

    // Send to web client
    LOG(INFO, "Web response: " + response);
}

void mqttExample() {
    ConfigV2& config = ConfigV2::getInstance();

    // MQTT publishing - direct access to values
    // No need for string-based lookups or type conversions

    publishMqttValue("temperature", config.stateTemperature.get());
    publishMqttValue("heater_power", config.stateHeaterPower.get());
    publishMqttValue("brew_setpoint", config.brewSetpoint.get());
    publishMqttValue("pid_enabled", config.pidEnabled.get());
}

// Placeholder function
void publishMqttValue(const char* topic, double value) {
    LOGF(INFO, "MQTT: %s = %.2f", topic, value);
}

void publishMqttValue(const char* topic, bool value) {
    LOGF(INFO, "MQTT: %s = %s", topic, value ? "true" : "false");
}

// === MIGRATION STRATEGY ===

/*
Migration from old Config system to ConfigV2:

1. Phase 1: Parallel existence
   - ConfigV2 runs alongside existing Config
   - New code uses ConfigV2
   - Old code continues using Config
   - Both systems manage their own NVS keys

2. Phase 2: Gradual replacement
   - Replace Config::getInstance().get<T>("key") calls with config.param.get()
   - Replace Config::getInstance().set<T>("key", value) with config.param.set(value)
   - Update web interface to use ConfigV2 parameter definitions
   - Update MQTT integration to use ConfigV2

3. Phase 3: Complete replacement
   - Remove old Config class
   - Clean up old NVS keys
   - Verify all functionality works with ConfigV2

Benefits of new system:
- Type safety: No runtime type errors
- Direct access: No string lookups or hash map searches
- Better performance: Direct member access vs map lookups
- Cleaner code: config.pidEnabled.get() vs Config::getInstance().get<bool>("pid.enabled")
- Automatic validation: Range checking built into set() methods
- Separation of concerns: Editable config vs read-only state
- Better IDE support: Auto-completion and type checking
*/
