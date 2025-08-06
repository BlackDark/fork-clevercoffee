/**
 * @file cpp23_improvements.cpp
 * @brief Practical C++23 improvements for CleverCoffee codebase
 * 
 * This file demonstrates real improvements using C++23 features
 * applied to existing CleverCoffee patterns.
 */

#include <expected>
#include <print>
#include <format>
#include <ranges>
#include <algorithm>

// ============================================================================
// 1. IMPROVED ERROR HANDLING WITH std::expected
// ============================================================================

// Current pattern from your codebase (Config.h, SystemInitializer.cpp)
namespace Current {
    bool loadFromNvs(const String& key) {
        // Complex logic with no error context
        return preferences.isKey(key.c_str()) && /* validation */;
    }
    
    bool initialize() {
        if (!hardwareManager->initialize()) return false;
        if (!sensorManager->initialize()) return false;
        return true;
    }
}

// C++23 improvement
namespace Improved {
    enum class ConfigError {
        KeyNotFound,
        InvalidValue,
        ValidationFailed,
        NvsFailure
    };
    
    enum class InitError {
        HardwareFailure,
        SensorFailure,
        NetworkFailure,
        ConfigurationError
    };
    
    std::expected<String, ConfigError> loadFromNvs(const String& key) {
        if (!preferences.isKey(key.c_str())) {
            return std::unexpected{ConfigError::KeyNotFound};
        }
        
        auto value = preferences.getString(key.c_str());
        if (value.isEmpty()) {
            return std::unexpected{ConfigError::InvalidValue};
        }
        
        return value; // Success case
    }
    
    std::expected<void, InitError> initialize() {
        if (auto result = hardwareManager->initialize(); !result) {
            return std::unexpected{InitError::HardwareFailure};
        }
        
        if (auto result = sensorManager->initialize(); !result) {
            return std::unexpected{InitError::SensorFailure};
        }
        
        return {}; // Success
    }
    
    // Usage example - much cleaner error handling
    void setupSystem() {
        auto init_result = initialize();
        if (!init_result) {
            switch (init_result.error()) {
                case InitError::HardwareFailure:
                    LOG(ERROR, "Hardware initialization failed");
                    // Specific recovery action
                    break;
                case InitError::SensorFailure:
                    LOG(ERROR, "Sensor initialization failed");
                    // Different recovery action
                    break;
            }
            return;
        }
        
        // Continue with successful initialization
        LOG(INFO, "System initialized successfully");
    }
}

// ============================================================================
// 2. MODERN LOGGER WITH std::print
// ============================================================================

// Enhanced Logger class using C++23 std::print
class ModernLogger {
private:
    static constexpr size_t BUFFER_SIZE = 512;
    char buffer_[BUFFER_SIZE];
    
public:
    template<typename... Args>
    void log(Logger::Level level, std::format_string<Args...> fmt, Args&&... args) {
        if (level < getCurrentLevel()) return;
        
        // C++23 std::print - faster and safer than snprintf
        auto result = std::format_to_n(buffer_, BUFFER_SIZE - 1, fmt, std::forward<Args>(args)...);
        *result.out = '\0';
        
        sendLogMessage(buffer_);
    }
    
    // Specialized formatters for CleverCoffee types
    void logTemperature(double temp, double setpoint) {
        log(Logger::Level::INFO, "Temp: {:.2f}°C (target: {:.2f}°C)", temp, setpoint);
    }
    
    void logMemoryState(size_t free, size_t total) {
        log(Logger::Level::DEBUG, "Memory: {}/{} ({:.1f}% used)", 
            total - free, total, (double)(total - free) * 100.0 / total);
    }
    
    // Type-safe sensor logging
    template<typename SensorReading>
    void logSensorData(const SensorReading& reading) {
        log(Logger::Level::INFO, "Sensor: {}", reading); // Uses custom formatter
    }
};

// Custom formatter for your sensor types
template<>
struct std::formatter<SensorReading> {
    constexpr auto parse(std::format_parse_context& ctx) {
        return ctx.begin(); // Simple formatter
    }
    
    auto format(const SensorReading& reading, std::format_context& ctx) const {
        return std::format_to(ctx.out(), "{:.2f}°C @ {}ms", 
                             reading.temperature, reading.timestamp);
    }
};

// ============================================================================
// 3. TEMPLATE DEDUPLICATION WITH "deducing this"
// ============================================================================

// Current pattern: Multiple similar display templates
namespace CurrentDisplays {
    // You have 6 similar files: displayTemplateStandard.h, displayTemplateMinimal.h, etc.
    // All with similar structure but slight differences
}

// C++23 solution: Single template with "deducing this"
template<typename DisplayType>
class UnifiedDisplayTemplate {
public:
    // "deducing this" eliminates CRTP boilerplate
    void render(this auto&& self) {
        self.drawHeader();
        self.drawBody();
        self.drawFooter();
    }
    
    void handleInput(this auto&& self, uint8_t input) {
        if (self.validateInput(input)) {
            self.processInput(input);
            self.updateDisplay();
        }
    }
    
    void updateDisplay(this auto&& self) {
        // Common update logic
        if (shouldRefresh()) {
            self.render();
        }
    }
    
private:
    bool shouldRefresh() const {
        return millis() - lastRefresh_ > REFRESH_INTERVAL;
    }
    
    unsigned long lastRefresh_ = 0;
    static constexpr unsigned long REFRESH_INTERVAL = 100;
};

// Specific display implementations - only the differences
class StandardDisplay : public UnifiedDisplayTemplate<StandardDisplay> {
public:
    void drawHeader() { /* Standard header */ }
    void drawBody() { /* Full body with all info */ }
    void drawFooter() { /* Status bar */ }
    bool validateInput(uint8_t input) { /* Standard validation */ }
    void processInput(uint8_t input) { /* Standard processing */ }
};

class MinimalDisplay : public UnifiedDisplayTemplate<MinimalDisplay> {
public:
    void drawHeader() { /* Minimal header */ }
    void drawBody() { /* Essential info only */ }
    void drawFooter() { /* No footer */ }
    bool validateInput(uint8_t input) { /* Simple validation */ }
    void processInput(uint8_t input) { /* Minimal processing */ }
};

// ============================================================================
// 4. CONSTEXPR CONFIGURATION VALIDATION
// ============================================================================

// Current Config.h has runtime validation - move to compile-time
namespace ModernConfig {
    // Compile-time validation functions
    constexpr bool isValidTemperature(double temp) {
        return temp >= 0.0 && temp <= 200.0;
    }
    
    constexpr bool isValidPressure(double pressure) {
        return pressure >= 0.0 && pressure <= 15.0; // 15 bar max
    }
    
    constexpr bool isValidPort(int port) {
        return port > 0 && port <= 65535;
    }
    
    // Enhanced ParamDef with compile-time validation
    template<typename T, auto Validator = nullptr>
    class ValidatedParam {
        T value_;
        
    public:
        constexpr ValidatedParam(T default_val) requires (Validator == nullptr || Validator(default_val))
            : value_(default_val) {}
        
        constexpr T get() const { return value_; }
        
        bool set(T new_value) {
            if constexpr (Validator != nullptr) {
                if (!Validator(new_value)) {
                    return false;
                }
            }
            value_ = new_value;
            return true;
        }
    };
    
    // Usage with compile-time validation
    constexpr ValidatedParam<double, isValidTemperature> brewSetpoint{95.0};
    constexpr ValidatedParam<int, isValidPort> mqttPort{1883};
    
    // This would fail to compile:
    // constexpr ValidatedParam<double, isValidTemperature> invalidTemp{300.0}; // Error!
}

// ============================================================================
// 5. RANGES AND ALGORITHMS FOR SENSOR PROCESSING
// ============================================================================

// Current pattern: Manual loops for sensor data processing
namespace CurrentSensorProcessing {
    void processSensorData(const std::vector<double>& readings) {
        double sum = 0;
        for (auto reading : readings) {
            if (reading > 0) {  // Filter valid readings
                sum += reading;
            }
        }
        double average = sum / readings.size();
    }
}

// C++23 improvement using ranges
namespace ModernSensorProcessing {
    #include <ranges>
    
    double processSensorData(const std::vector<double>& readings) {
        using namespace std::ranges;
        
        auto valid_readings = readings 
            | views::filter([](double r) { return r > 0; })
            | views::transform([](double r) { return r * 0.1; }); // Convert units
        
        if (empty(valid_readings)) {
            return 0.0;
        }
        
        return std::reduce(begin(valid_readings), end(valid_readings)) / size(valid_readings);
    }
    
    // More complex sensor fusion
    std::vector<double> fuseSensorReadings(const auto& temp_readings, const auto& pressure_readings) {
        using namespace std::ranges;
        
        return views::zip(temp_readings, pressure_readings)
            | views::transform([](auto&& pair) {
                auto [temp, pressure] = pair;
                return temp * (1.0 + pressure * 0.01); // Pressure compensation
            })
            | to<std::vector>();
    }
}

// ============================================================================
// 6. PRACTICAL USAGE EXAMPLES IN YOUR CODEBASE
// ============================================================================

// Example integration in your existing LoopManager.cpp
class ModernLoopManager {
    std::expected<void, UpdateError> updateSensors() {
        if (auto temp_result = sensorManager_->readTemperature(); !temp_result) {
            return std::unexpected{UpdateError::TemperatureSensorFailed};
        }
        
        if (auto pressure_result = sensorManager_->readPressure(); !pressure_result) {
            // Can still continue - pressure is optional
            LOG(WARNING, "Pressure sensor failed: {}", magic_enum::enum_name(pressure_result.error()));
        }
        
        return {};
    }
    
    void update() {
        auto sensor_result = updateSensors();
        if (!sensor_result) {
            // Handle specific error
            handleSensorError(sensor_result.error());
            return;
        }
        
        // Continue with other updates...
    }
};

// Example integration in your Config system
namespace ConfigExample {
    // Compile-time parameter validation
    template<typename T>
    constexpr bool isInRange(T value, T min, T max) {
        return value >= min && value <= max;
    }
    
    // Your existing parameters with compile-time validation
    constexpr auto BREW_TEMP_MIN = 80.0;
    constexpr auto BREW_TEMP_MAX = 100.0;
    
    class BrewConfig {
    public:
        ValidatedParam<double, [](double t) { return isInRange(t, BREW_TEMP_MIN, BREW_TEMP_MAX); }> 
            brewSetpoint{95.0};
    };
    
    // This ensures invalid configurations are caught at compile-time
    // BrewConfig config; // OK
    // config.brewSetpoint.set(150.0); // Runtime error, but detected early
}