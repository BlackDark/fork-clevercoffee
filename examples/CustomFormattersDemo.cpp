/**
 * @file CustomFormattersDemo.cpp
 * @brief Demonstration of custom formatters for CleverCoffee types
 */

#include "../src/utils/CustomFormatters.h"
#include "../lib/Logger/Logger.h"
#include "../src/Config.h"
#include "../src/state/GlobalState.h"

#if __cplusplus >= 202002L && __has_include(<format>)

using namespace CleverCoffee::Formatters;

void demonstrateCustomFormatters() {
    // Hardware type formatters
    MODERN_LOG(INFO, "Switch configuration: type={}, mode={}", 
              Hardware::SwitchType::MOMENTARY, 
              Hardware::SwitchMode::NORMALLY_OPEN);
    
    MODERN_LOG(INFO, "Relay setup: heater={}, pump={}", 
              Hardware::RelayTriggerType::HIGH_TRIGGER,
              Hardware::RelayTriggerType::LOW_TRIGGER);
    
    MODERN_LOG(INFO, "Sensors: temperature={}, scale={}", 
              Hardware::TemperatureSensorType::TSIC_306,
              Hardware::ScaleType::HX711_DUAL);
    
    // System configuration formatters
    MODERN_LOG(INFO, "Display: template={}, language={}", 
              System::DisplayTemplate::UPRIGHT,
              System::Language::ENGLISH);
    
    MODERN_LOG(INFO, "Brewing: mode={}", Process::BrewMode::AUTOMATIC_BREW);
    
    // Machine state formatting (custom format specifier)
    MODERN_LOG(INFO, "Machine state changed: {}", std::format("{:m}", kPidNormal));
    MODERN_LOG(INFO, "Brew state: {}", std::format("{:m}", kBrewRunning));
    
    // Temperature formatting with units
    MODERN_LOG(INFO, "Current temperature: {}", temp(87.5));
    MODERN_LOG(INFO, "Steam temperature: {:.0f}", temp(125.0, "°C"));
    MODERN_LOG(INFO, "Fahrenheit reading: {:.1f}", temp(185.5, "°F"));
    
    // PID parameters formatting
    MODERN_LOG(INFO, "PID tuned: {}", pid(62.0, 52.0, 11.5));
    MODERN_LOG(INFO, "Steam PID: {:.1f}", pid(150.0, 75.0, 5.2));
    
    // Memory information with visual feedback
    MODERN_LOG(DEBUG, "Memory status: {}", memory(245760, 327680, 81920, 32768));
    MODERN_LOG(DEBUG, "Memory (no %): {:n}", memory(245760, 327680, 81920, 32768));
    
    // WiFi signal strength with visual indicators
    MODERN_LOG(INFO, "WiFi: {}", wifi(4)); // Excellent signal
    MODERN_LOG(INFO, "WiFi: {}", wifi(2)); // Fair signal  
    MODERN_LOG(INFO, "WiFi: {:n}", wifi(1)); // Weak signal, no visual
    MODERN_LOG(WARNING, "WiFi: {}", wifi(0, false)); // Disconnected
}

void demonstrateAdvancedFormatting() {
    // Complex logging scenarios
    const auto& config = Config::getInstance();
    
    MODERN_LOG(INFO, "System startup: display={}, brew_mode={}, temp_sensor={}", 
              config.displayTemplate.get(),
              config.brewMode.get(), 
              config.hardwareSensorsTemperatureType.get());
    
    // Real-time monitoring with formatted output
    if (g_state.pid) {
        double kp = g_state.pid->GetKp();
        double ki = g_state.pid->GetKi(); 
        double kd = g_state.pid->GetKd();
        
        MODERN_LOG(DEBUG, "PID control: {} | temp={} | setpoint={} | output={:.1f}%", 
                  pid(kp, ki, kd),
                  temp(g_state.process.temperature),
                  temp(g_state.process.setpoint), 
                  g_state.process.pidOutput / 10.0);
    }
    
    // Error logging with context
    if (g_state.machine.machineState == kSensorError) {
        MODERN_LOG(ERROR, "Sensor failure detected: type={}, current_reading={}, state={}", 
                  config.hardwareSensorsTemperatureType.get(),
                  temp(g_state.process.temperature),
                  std::format("{:m}", g_state.machine.machineState));
    }
    
    // Performance monitoring
    MODERN_LOG(TRACE, "Brew cycle: time={:.1f}s, weight={:.1f}g, pressure={:.1f}bar", 
              g_state.process.currBrewTime / 1000.0,
              g_state.sensors.currBrewWeight,
              g_state.sensors.inputPressure);
}

void demonstrateConfigurationLogging() {
    const auto& config = Config::getInstance();
    
    MODERN_LOG(INFO, "Hardware Configuration:");
    MODERN_LOG(INFO, "  Switches: power={}, brew={}, steam={}", 
              config.hardwareSwitchesPowerEnabled.get(),
              config.hardwareSwitchesBrewEnabled.get(), 
              config.hardwareSwitchesSteamEnabled.get());
    
    MODERN_LOG(INFO, "  Switch Types: power={}, brew={}", 
              config.hardwareSwitchesPowerType.get(),
              config.hardwareSwitchesBrewType.get());
              
    MODERN_LOG(INFO, "  LEDs: status={}, brew={}, steam={}", 
              config.hardwareLedsStatusEnabled.get(),
              config.hardwareLedsBrewEnabled.get(),
              config.hardwareLedsSteamEnabled.get());
              
    MODERN_LOG(INFO, "  Sensors: temp={}, scale={}", 
              config.hardwareSensorsTemperatureType.get(),
              config.hardwareSensorsScaleType.get());
    
    MODERN_LOG(INFO, "Process Configuration:");
    MODERN_LOG(INFO, "  Temperatures: brew={} steam={} offset={}", 
              temp(config.brewSetpoint.get()),
              temp(config.steamSetpoint.get()),
              temp(config.brewTempOffset.get()));
              
    MODERN_LOG(INFO, "  PID: regular={} steam={}", 
              pid(config.pidRegularKp.get(), config.pidRegularKi.get(), config.pidRegularKd.get()),
              pid(config.pidSteamKp.get(), 0, 0));
              
    MODERN_LOG(INFO, "  Brew: mode={} time={:.0f}s weight={:.0f}g", 
              config.brewMode.get(),
              config.targetBrewTime.get(),
              config.brewByWeightTargetWeight.get());
    
    MODERN_LOG(INFO, "Display Configuration:");
    MODERN_LOG(INFO, "  Template: {} Language: {}", 
              config.displayTemplate.get(),
              config.language.get());
}

// Integration with existing CleverCoffee logging patterns
void logSystemStatus() {
    // Replaces multiple LOG calls with a single, comprehensive one
    if (Logger::getCurrentLevel() <= Logger::Level::INFO) {
        MODERN_LOG(INFO, 
            "System Status: state={}, temp={}, setpoint={}, pid_out={:.1f}%, "
            "brew_time={:.1f}s, scale={:.1f}g, wifi={}, memory={}", 
            std::format("{:m}", g_state.machine.machineState),
            temp(g_state.process.temperature), 
            temp(g_state.process.setpoint),
            g_state.process.pidOutput / 10.0,
            g_state.process.currBrewTime / 1000.0,
            g_state.sensors.currBrewWeight,
            wifi(g_state.network.cleverCoffeeWiFiManager ? 
                 g_state.network.cleverCoffeeWiFiManager->getSignalStrength() : 0,
                 WiFi.isConnected()),
            memory(ESP.getHeapSize() - ESP.getFreeHeap(), 
                   ESP.getHeapSize(), 
                   ESP.getFreeHeap(), 
                   ESP.getMaxAllocHeap()));
    }
}

// Example of how this improves error diagnostics
void logHardwareError(Hardware::TemperatureSensorType sensorType, double reading) {
    MODERN_LOG(ERROR, 
        "Hardware Error: sensor={} reading={} (expected range: {} to {}) "
        "Check connections and sensor calibration.", 
        sensorType,
        temp(reading),
        temp(0.0), temp(150.0));
}

void logBrewingProgress(double brewTime, double targetTime, double weight, double targetWeight) {
    MODERN_LOG(DEBUG, 
        "Brew Progress: time={:.1f}s/{:.0f}s ({:.1f}%) weight={:.1f}g/{:.0f}g ({:.1f}%)",
        brewTime / 1000.0, targetTime / 1000.0, (brewTime / targetTime) * 100.0,
        weight, targetWeight, (weight / targetWeight) * 100.0);
}

// Usage examples that could be integrated into main CleverCoffee code:

/* In SystemInitializer.cpp:
MODERN_LOG(INFO, "Initializing hardware: temp_sensor={}, scale={}, display={}", 
          config.hardwareSensorsTemperatureType.get(),
          config.hardwareSensorsScaleType.get(),
          config.displayTemplate.get());
*/

/* In StateMachine.cpp:  
MODERN_LOG(DEBUG, "State transition: {} → {} (trigger: {})",
          std::format("{:m}", oldState), 
          std::format("{:m}", newState),
          triggerReason);
*/

/* In brewing logic:
MODERN_LOG(INFO, "Brew started: mode={}, temp={}, pid={}", 
          config.brewMode.get(),
          temp(g_state.process.temperature),
          pid(kp, ki, kd));
*/

#endif // __cplusplus >= 202002L && __has_include(<format>)