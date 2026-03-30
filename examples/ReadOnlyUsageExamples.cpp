/**
 * @file ReadOnlyUsageExamples.cpp
 * @brief Comprehensive examples of read-only parameter usage in ConfigV2
 */

#include "../src/ConfigV2.h"
#include "Logger.h"

void demonstrateReadOnlyTypes() {
    ConfigV2& config = ConfigV2::getInstance();

    // === 1. LIVE SENSOR READINGS (Updated every loop) ===
    LOG(INFO, "=== Live Sensor Readings ===");

    // Real-time temperature - no caching, always fresh
    double currentTemp = config.stateTemperature.get();
    double heaterPower = config.stateHeaterPower.get();
    double pressure = config.statePressure.get();
    double weight = config.stateWeight.get();

    LOGF(INFO, "Temperature: %.2f°C, Heater: %.1f%%, Pressure: %.2f bar, Weight: %.1f g",
         currentTemp, heaterPower, pressure, weight);

    // === 2. MACHINE STATUS (Updated frequently, can be cached) ===
    LOG(INFO, "=== Machine Status ===");

    // These values change less frequently, so caching is beneficial
    int machineState = config.stateMachineState.getCached();
    bool wifiConnected = config.stateWifiConnected.getCached();
    bool mqttConnected = config.stateMqttConnected.getCached();
    bool waterTankFull = config.stateWaterTankFull.getCached();

    LOGF(INFO, "State: %d, WiFi: %s, MQTT: %s, Water: %s",
         machineState,
         wifiConnected ? "connected" : "disconnected",
         mqttConnected ? "connected" : "disconnected",
         waterTankFull ? "full" : "empty");

    // === 3. BREWING STATUS (Real-time during brewing) ===
    LOG(INFO, "=== Brewing Status ===");

    if (config.stateBrewActive.get()) {
        double brewTime = config.stateBrewTime.get();
        double brewWeight = config.stateBrewWeight.get();
        double brewRatio = config.computedBrewRatio.get();

        LOGF(INFO, "Brewing: %.1fs, %.1fg extracted, ratio: %.2f",
             brewTime, brewWeight, brewRatio);
    } else {
        LOG(INFO, "Not currently brewing");
    }

    // === 4. SYSTEM INFORMATION (Static or occasional updates) ===
    LOG(INFO, "=== System Information ===");

    // Static info - never changes after boot
    String version = config.stateSystemVersion.get();

    // Occasional info - cached and updated when needed
    String ipAddress = config.stateWifiIP.getCached();
    String ssid = config.stateWifiSSID.getCached();

    // Frequent info - for monitoring
    int freeHeap = config.stateFreeHeap.get();
    int uptime = config.stateUptime.get();

    LOGF(INFO, "Version: %s, IP: %s, SSID: %s", version.c_str(), ipAddress.c_str(), ssid.c_str());
    LOGF(INFO, "Free heap: %d bytes, Uptime: %ds", freeHeap, uptime);

    // === 5. COMPUTED VALUES (Derived from editable parameters) ===
    LOG(INFO, "=== Computed Values ===");

    // These are calculated from editable parameters
    double pidKi = config.computedPidKi.get();
    double pidKd = config.computedPidKd.get();

    LOGF(INFO, "Computed PID gains - Ki: %.4f, Kd: %.2f", pidKi, pidKd);

    // Show the source parameters too
    double kp = config.pidRegularKp.get();
    double tn = config.pidRegularTn.get();
    double tv = config.pidRegularTv.get();

    LOGF(INFO, "Source parameters - Kp: %.2f, Tn: %.1f, Tv: %.1f", kp, tn, tv);
}

void demonstrateCachingBehavior() {
    ConfigV2& config = ConfigV2::getInstance();

    LOG(INFO, "=== Caching Behavior Demonstration ===");

    // Real-time parameters - no caching
    unsigned long start = millis();
    for (int i = 0; i < 100; i++) {
        double temp = config.stateTemperature.get(); // Always reads fresh value
        (void)temp; // Suppress unused warning
    }
    unsigned long realtimeTime = millis() - start;

    // Cached parameters - much faster for repeated access
    start = millis();
    for (int i = 0; i < 100; i++) {
        bool wifi = config.stateWifiConnected.getCached(); // Uses cached value
        (void)wifi; // Suppress unused warning
    }
    unsigned long cachedTime = millis() - start;

    LOGF(INFO, "100 real-time reads: %lums, 100 cached reads: %lums", realtimeTime, cachedTime);

    // Force cache update
    config.stateWifiConnected.updateCache();

    // Check if cache needs update
    if (config.stateWifiConnected.needsUpdate(5000)) { // 5 second max age
        LOG(INFO, "WiFi status cache needs update");
        config.stateWifiConnected.updateCache();
    }
}

void webInterfaceReadOnlyExample() {
    ConfigV2& config = ConfigV2::getInstance();

    LOG(INFO, "=== Web Interface Read-Only Data ===");

    // Create JSON for web interface with different types of read-only data
    JsonDocument doc;

    // Live sensor data (for real-time charts)
    JsonObject sensorData = doc["sensors"].to<JsonObject>();
    sensorData["temperature"] = config.stateTemperature.get();
    sensorData["heater_power"] = config.stateHeaterPower.get();
    sensorData["pressure"] = config.statePressure.get();
    sensorData["weight"] = config.stateWeight.get();

    // Machine status (for status indicators)
    JsonObject statusData = doc["status"].to<JsonObject>();
    statusData["machine_state"] = config.stateMachineState.getCached();
    statusData["wifi_connected"] = config.stateWifiConnected.getCached();
    statusData["mqtt_connected"] = config.stateMqttConnected.getCached();
    statusData["water_tank_full"] = config.stateWaterTankFull.getCached();

    // System info (for about page)
    JsonObject systemData = doc["system"].to<JsonObject>();
    systemData["version"] = config.stateSystemVersion.get();
    systemData["ip_address"] = config.stateWifiIP.getCached();
    systemData["ssid"] = config.stateWifiSSID.getCached();
    systemData["free_heap"] = config.stateFreeHeap.get();
    systemData["uptime"] = config.stateUptime.get();

    // Computed values (for display)
    JsonObject computedData = doc["computed"].to<JsonObject>();
    computedData["pid_ki"] = config.computedPidKi.get();
    computedData["pid_kd"] = config.computedPidKd.get();
    if (config.stateBrewActive.get()) {
        computedData["brew_ratio"] = config.computedBrewRatio.get();
    }

    String jsonResponse;
    serializeJson(doc, jsonResponse);
    LOG(INFO, "JSON response for web interface:");
    LOG(INFO, jsonResponse);
}

void mqttReadOnlyExample() {
    ConfigV2& config = ConfigV2::getInstance();

    LOG(INFO, "=== MQTT Publishing Read-Only Data ===");

    // Publish sensor readings (high frequency)
    publishMqttValue("sensors/temperature", config.stateTemperature.get());
    publishMqttValue("sensors/heater_power", config.stateHeaterPower.get());
    publishMqttValue("sensors/pressure", config.statePressure.get());
    publishMqttValue("sensors/weight", config.stateWeight.get());

    // Publish status updates (medium frequency)
    static unsigned long lastStatusUpdate = 0;
    if (millis() - lastStatusUpdate > 5000) { // Every 5 seconds
        publishMqttValue("status/machine_state", config.stateMachineState.getCached());
        publishMqttValue("status/wifi_connected", config.stateWifiConnected.getCached());
        publishMqttValue("status/mqtt_connected", config.stateMqttConnected.getCached());
        publishMqttValue("status/water_tank_full", config.stateWaterTankFull.getCached());
        lastStatusUpdate = millis();
    }

    // Publish system info (low frequency)
    static unsigned long lastSystemUpdate = 0;
    if (millis() - lastSystemUpdate > 60000) { // Every minute
        publishMqttValue("system/free_heap", config.stateFreeHeap.get());
        publishMqttValue("system/uptime", config.stateUptime.get());
        lastSystemUpdate = millis();
    }

    // Publish computed values when brewing
    if (config.stateBrewActive.get()) {
        publishMqttValue("brew/time", config.stateBrewTime.get());
        publishMqttValue("brew/weight", config.stateBrewWeight.get());
        publishMqttValue("brew/ratio", config.computedBrewRatio.get());
    }
}

void performanceOptimizationExample() {
    ConfigV2& config = ConfigV2::getInstance();

    LOG(INFO, "=== Performance Optimization ===");

    // Smart caching strategy based on update frequency
    unsigned long loopStart = millis();

    // Always get fresh real-time data
    double temperature = config.stateTemperature.get();
    double heaterPower = config.stateHeaterPower.get();

    // Use cached values for less critical data
    static unsigned long lastStatusCheck = 0;
    static bool cachedWifiStatus = false;
    static int cachedMachineState = 0;

    if (millis() - lastStatusCheck > 1000) { // Update cache every 1 second
        cachedWifiStatus = config.stateWifiConnected.get();
        cachedMachineState = config.stateMachineState.get();
        lastStatusCheck = millis();
    }

    // Use the cached values in your control logic
    if (cachedWifiStatus && cachedMachineState == kPidNormal) {
        // Do something with real-time data
        LOGF(DEBUG, "Control loop: temp=%.2f, power=%.1f", temperature, heaterPower);
    }

    unsigned long loopTime = millis() - loopStart;
    LOGF(DEBUG, "Optimized loop time: %lums", loopTime);
}

// Helper functions
void publishMqttValue(const char* topic, double value) {
    LOGF(DEBUG, "MQTT: %s = %.2f", topic, value);
}

void publishMqttValue(const char* topic, bool value) {
    LOGF(DEBUG, "MQTT: %s = %s", topic, value ? "true" : "false");
}

void publishMqttValue(const char* topic, int value) {
    LOGF(DEBUG, "MQTT: %s = %d", topic, value);
}
