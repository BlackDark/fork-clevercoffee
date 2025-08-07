/**
 * @file EnhancedLogging.h
 * @brief Enhanced logging utilities using custom formatters
 */

#pragma once

#include "CustomFormatters.h"
#include "../../lib/Logger/Logger.h"

namespace CleverCoffee::Logging {

/**
 * @brief Log system initialization with detailed component information
 */
inline void logSystemInit() {
    const auto& config = Config::getInstance();
    
    MODERN_LOG(INFO, "System Initialization:");
    MODERN_LOG(INFO, "├─ Hardware: temp_sensor={}, scale={}, relays={}", 
              config.hardwareSensorsTemperatureType.get(),
              config.hardwareSensorsScaleType.get(),
              config.hardwareRelaysHeaterTriggerType.get());
              
    MODERN_LOG(INFO, "├─ Display: template={}, language={}", 
              config.displayTemplate.get(),
              config.language.get());
              
    MODERN_LOG(INFO, "└─ Process: brew_mode={}, temp={}, pid={:.1f}", 
              config.brewMode.get(),
              Formatters::temp(config.brewSetpoint.get()),
              Formatters::pid(config.pidRegularKp.get(), 
                             config.pidRegularKi.get(), 
                             config.pidRegularKd.get()));
}

/**
 * @brief Log memory status with visual indicators
 */
inline void logMemoryStatus(const char* context = "Memory") {
    const auto totalHeap = ESP.getHeapSize();
    const auto freeHeap = ESP.getFreeHeap();
    const auto usedHeap = totalHeap - freeHeap;
    const auto largestBlock = ESP.getMaxAllocHeap();
    
    MODERN_LOG(DEBUG, "{}: {}", context, 
              Formatters::memory(usedHeap, totalHeap, freeHeap, largestBlock));
}

/**
 * @brief Log network status with signal strength
 */
inline void logNetworkStatus() {
    if (WiFi.status() == WL_CONNECTED) {
        int signalStrength = 4; // This would come from CleverCoffeeWiFiManager
        if (g_state.network.cleverCoffeeWiFiManager) {
            signalStrength = g_state.network.cleverCoffeeWiFiManager->getSignalStrength();
        }
        
        MODERN_LOG(INFO, "Network: connected, {}, IP: {}", 
                  Formatters::wifi(signalStrength), 
                  WiFi.localIP().toString());
    } else {
        MODERN_LOG(WARNING, "Network: {}", Formatters::wifi(0, false));
    }
}

/**
 * @brief Log brewing progress with comprehensive information  
 */
inline void logBrewingProgress() {
    if (g_state.machine.machineState == kBrew || g_state.machine.machineState == kBrewRunning) {
        const bool hasScale = Config::getInstance().hardwareSensorsScaleEnabled.get();
        const bool hasTime = Config::getInstance().brewByTimeEnabled.get();
        const bool hasWeight = Config::getInstance().brewByWeightEnabled.get();
        
        if (hasScale && hasTime && hasWeight) {
            MODERN_LOG(DEBUG, "Brew: time={:.1f}s/{:.0f}s weight={:.1f}g/{:.0f}g temp={} state={}", 
                      g_state.process.currBrewTime / 1000.0,
                      g_state.process.totalTargetBrewTime / 1000.0,
                      g_state.sensors.currBrewWeight,
                      Config::getInstance().brewByWeightTargetWeight.get(),
                      Formatters::temp(g_state.process.temperature),
                      std::format("{:m}", g_state.machine.machineState));
        } else if (hasTime) {
            MODERN_LOG(DEBUG, "Brew: time={:.1f}s temp={} state={}", 
                      g_state.process.currBrewTime / 1000.0,
                      Formatters::temp(g_state.process.temperature),
                      std::format("{:m}", g_state.machine.machineState));
        } else {
            MODERN_LOG(DEBUG, "Brew: temp={} state={}", 
                      Formatters::temp(g_state.process.temperature),
                      std::format("{:m}", g_state.machine.machineState));
        }
    }
}

/**
 * @brief Log PID tuning information
 */
inline void logPIDTuning(const char* context = "PID") {
    if (g_state.pid) {
        const double kp = g_state.pid->GetKp();
        const double ki = g_state.pid->GetKi();  
        const double kd = g_state.pid->GetKd();
        
        MODERN_LOG(INFO, "{} Tuning: {} temp={} setpoint={} output={:.1f}%", 
                  context,
                  Formatters::pid(kp, ki, kd),
                  Formatters::temp(g_state.process.temperature),
                  Formatters::temp(g_state.process.setpoint),
                  g_state.process.pidOutput / 10.0);
    }
}

/**
 * @brief Log hardware configuration changes
 */
inline void logHardwareConfig(const char* component, Hardware::SwitchType type, Hardware::SwitchMode mode) {
    MODERN_LOG(INFO, "Hardware Config: {} → type={}, mode={}", component, type, mode);
}

inline void logHardwareConfig(const char* component, Hardware::RelayTriggerType trigger) {
    MODERN_LOG(INFO, "Hardware Config: {} → trigger={}", component, trigger);
}

inline void logHardwareConfig(const char* component, Hardware::TemperatureSensorType sensor) {
    MODERN_LOG(INFO, "Hardware Config: {} → sensor={}", component, sensor);
}

/**
 * @brief Log state machine transitions
 */
inline void logStateTransition(int oldState, int newState, const char* reason = "unknown") {
    MODERN_LOG(DEBUG, "State: {} → {} ({})", 
              std::format("{:m}", oldState),
              std::format("{:m}", newState),
              reason);
}

/**
 * @brief Log errors with detailed context
 */
inline void logError(const char* component, const char* error, const char* suggestion = "") {
    if (strlen(suggestion) > 0) {
        MODERN_LOG(ERROR, "{} Error: {} | Suggestion: {}", component, error, suggestion);
    } else {
        MODERN_LOG(ERROR, "{} Error: {}", component, error);
    }
}

/**
 * @brief Log sensor readings with proper formatting
 */
inline void logSensorReading(const char* sensor, double value, const char* unit = "", 
                           double min = -999, double max = 999) {
    if (min != -999 && max != 999) {
        if (value < min || value > max) {
            MODERN_LOG(WARNING, "Sensor {}: {:.2f}{} (out of range: {:.1f}-{:.1f}{})", 
                      sensor, value, unit, min, max, unit);
        } else {
            MODERN_LOG(TRACE, "Sensor {}: {:.2f}{}", sensor, value, unit);
        }
    } else {
        MODERN_LOG(TRACE, "Sensor {}: {:.2f}{}", sensor, value, unit);
    }
}

/**
 * @brief Comprehensive system status log (replaces multiple individual logs)
 */
inline void logSystemStatus() {
    const auto totalHeap = ESP.getHeapSize();
    const auto freeHeap = ESP.getFreeHeap();
    const auto usedHeap = totalHeap - freeHeap;
    const auto largestBlock = ESP.getMaxAllocHeap();
    
    int wifiStrength = 0;
    bool wifiConnected = false;
    if (g_state.network.cleverCoffeeWiFiManager) {
        wifiStrength = g_state.network.cleverCoffeeWiFiManager->getSignalStrength();
        wifiConnected = WiFi.status() == WL_CONNECTED;
    }
    
    MODERN_LOG(INFO, 
        "System: state={}, temp={}, setpoint={}, output={:.1f}%, "
        "brew={:.1f}s, weight={:.1f}g, wifi={}, memory={}", 
        std::format("{:m}", g_state.machine.machineState),
        Formatters::temp(g_state.process.temperature), 
        Formatters::temp(g_state.process.setpoint),
        g_state.process.pidOutput / 10.0,
        g_state.process.currBrewTime / 1000.0,
        g_state.sensors.currBrewWeight,
        Formatters::wifi(wifiStrength, wifiConnected),
        Formatters::memory(usedHeap, totalHeap, freeHeap, largestBlock));
}

} // namespace CleverCoffee::Logging

// Convenience macros for common logging patterns
#define LOG_SYSTEM_INIT() CleverCoffee::Logging::logSystemInit()
#define LOG_MEMORY(context) CleverCoffee::Logging::logMemoryStatus(context)
#define LOG_NETWORK() CleverCoffee::Logging::logNetworkStatus()
#define LOG_BREW_PROGRESS() CleverCoffee::Logging::logBrewingProgress()
#define LOG_PID_TUNING(context) CleverCoffee::Logging::logPIDTuning(context)
#define LOG_STATE_TRANSITION(old, new, reason) CleverCoffee::Logging::logStateTransition(old, new, reason)
#define LOG_SYSTEM_STATUS() CleverCoffee::Logging::logSystemStatus()