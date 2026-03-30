/**
 * @file ModernLogger.h
 * @brief C++23 modernized version of Logger with std::print and improved performance
 * 
 * This demonstrates how your existing Logger.h could be enhanced with C++23 features:
 * - std::print for faster, type-safe formatting
 * - std::expected for better error handling
 * - constexpr improvements
 * - Custom formatters for CleverCoffee types
 */

#pragma once

#include <WiFi.h>
#include <WiFiClient.h>
#include <WiFiServer.h>
#include <expected>
#include <print>
#include <format>
#include <array>
#include <string_view>
#include <ctime>
#include <stdint.h>

// Forward declarations for CleverCoffee types
struct SensorReading;
struct MemoryInfo;
struct ProcessState;

class ModernLogger {
    public:
        enum class Level : int {
            TRACE = 0,
            DEBUG = 1,
            INFO = 2,
            WARNING = 3,
            ERROR = 4,
            FATAL = 5,
            SILENT = 6,
        };

        enum class LogError {
            NetworkFailure,
            BufferOverflow,
            SerialFailure,
            InvalidLevel
        };

        struct Config {
            uint16_t port = 23;
            size_t maxBufferSize = 512;
            bool enableSerial = true;
            bool enableWiFi = true;
            uint32_t serialBaud = 115200;
            Level initialLevel = Level::INFO;
        };

        // Singleton with modern initialization
        static ModernLogger& getInstance() {
            static ModernLogger instance{getDefaultConfig()};
            return instance;
        }

        // C++23 std::expected for initialization
        static std::expected<void, LogError> init() {
            return init(getDefaultConfig());
        }

        static std::expected<void, LogError> init(const Config& config) {
            auto& logger = getInstanceImpl(config);
            return logger.begin();
        }

        ModernLogger(ModernLogger const&) = delete;
        void operator=(ModernLogger const&) = delete;

        // C++23 std::print-based logging - much faster than printf family
        template<typename... Args>
        std::expected<void, LogError> log(Level level, std::format_string<Args...> fmt, Args&&... args) {
            if (level < level_) {
                return {}; // Early return if level too low
            }

            try {
                // Use std::format_to_n for safe, bounded formatting
                auto result = std::format_to_n(logBuffer_, LOG_BUFFER_SIZE - 1, fmt, std::forward<Args>(args)...);
                *result.out = '\0';
                
                return sendFormattedMessage(level, logBuffer_);
            } catch (const std::format_error&) {
                return std::unexpected{LogError::BufferOverflow};
            }
        }

        // Specialized logging methods for CleverCoffee types
        void logTemperature(Level level, double current, double target) {
            log(level, "Temperature: {:.2f}°C (target: {:.2f}°C) Δ{:+.2f}°C", 
                current, target, current - target);
        }

        void logMemory(Level level, const MemoryInfo& mem) {
            log(level, "Memory: {}/{} bytes ({:.1f}% used), largest block: {} bytes", 
                mem.used, mem.total, mem.usagePercent, mem.largestBlock);
        }

        void logSensorReading(Level level, const SensorReading& reading) {
            log(level, "Sensor: {}", reading); // Uses custom formatter
        }

        void logProcessState(Level level, const ProcessState& state) {
            log(level, "Process: {}", state); // Uses custom formatter
        }

        // Update method with better error handling
        std::expected<void, LogError> update() {
            try {
                if (config_.enableWiFi && WiFi.status() == WL_CONNECTED) {
                    if (auto result = handleNetworkConnections(); !result) {
                        return result;
                    }
                }
                return {};
            } catch (...) {
                return std::unexpected{LogError::NetworkFailure};
            }
        }

        // Getters with modern constexpr
        static constexpr Level getCurrentLevel() noexcept {
            return getInstance().level_;
        }

        static constexpr uint16_t getPort() noexcept {
            return getInstance().config_.port;
        }

        static constexpr std::string_view getLevelString(Level level) noexcept {
            using namespace std::string_view_literals;
            
            constexpr std::array<std::string_view, 7> level_names = {
                "TRACE"sv, "DEBUG"sv, "INFO"sv, "WARNING"sv, "ERROR"sv, "FATAL"sv, "SILENT"sv
            };
            
            auto index = static_cast<size_t>(level);
            return (index < level_names.size()) ? level_names[index] : "UNKNOWN"sv;
        }

        static void setLevel(Level level) noexcept {
            getInstance().level_ = level;
        }

        // Performance monitoring
        struct Stats {
            size_t messagesLogged = 0;
            size_t networkErrors = 0;
            size_t bufferOverflows = 0;
            unsigned long totalTime = 0; // microseconds
        };

        const Stats& getStats() const noexcept { return stats_; }
        void resetStats() noexcept { stats_ = {}; }

    private:
        static ModernLogger& getInstanceImpl(const Config& config = getDefaultConfig()) {
            static ModernLogger instance{config};
            return instance;
        }

        static constexpr Config getDefaultConfig() noexcept {
            return Config{};
        }

        explicit ModernLogger(const Config& config) noexcept 
            : config_(config), level_(config.initialLevel), server_(config.port) {
            logBuffer_[0] = '\0';
        }

        std::expected<void, LogError> begin() {
            if (config_.enableSerial) {
                Serial.begin(config_.serialBaud);
            }

            if (config_.enableWiFi && WiFi.status() == WL_CONNECTED) {
                server_.begin();
                return {};
            }

            return {};
        }

        std::expected<void, LogError> sendFormattedMessage(Level level, const char* message) {
            auto start_time = micros();
            
            // Format with timestamp and level
            char fullMessage[LOG_BUFFER_SIZE + 64]; // Extra space for metadata
            auto timestamp_result = formatTimestamp();
            
            if (timestamp_result) {
                std::format_to_n(fullMessage, sizeof(fullMessage) - 1, 
                                "[{}] [{}] {}\r\n", 
                                *timestamp_result, getLevelString(level), message);
            } else {
                std::format_to_n(fullMessage, sizeof(fullMessage) - 1, 
                                "[{}] {}\r\n", 
                                getLevelString(level), message);
            }
            
            // Send to all outputs
            auto result = sendToOutputs(fullMessage);
            
            // Update statistics
            auto end_time = micros();
            stats_.totalTime += (end_time - start_time);
            
            if (result) {
                ++stats_.messagesLogged;
            } else {
                ++stats_.networkErrors;
            }
            
            return result;
        }

        std::expected<void, LogError> sendToOutputs(const char* message) {
            bool success = true;
            
            // Serial output
            if (config_.enableSerial) {
                Serial.print(message);
            }

            // Network output
            if (config_.enableWiFi && client_ && client_.connected()) {
                size_t written = client_.write(message);
                if (written == 0) {
                    success = false;
                }
            }

            return success ? std::expected<void, LogError>{} 
                          : std::unexpected{LogError::NetworkFailure};
        }

        std::expected<std::string_view, LogError> formatTimestamp() const noexcept {
            auto now = time(nullptr);
            if (now == -1) {
                return std::unexpected{LogError::InvalidLevel};
            }
            
            auto tm = *localtime(&now);
            auto result = std::format_to_n(timestampBuffer_, TIMESTAMP_BUFFER_SIZE - 1,
                                         "{:02d}:{:02d}:{:02d}", 
                                         tm.tm_hour, tm.tm_min, tm.tm_sec);
            *result.out = '\0';
            
            return std::string_view{timestampBuffer_};
        }

        std::expected<void, LogError> handleNetworkConnections() {
            if (server_.hasClient()) {
                if (client_ && client_.connected()) {
                    client_.stop(); // Disconnect existing client
                }
                client_ = server_.available();
            }
            return {};
        }

        // Configuration and state
        Config config_;
        Level level_;
        Stats stats_;

        // Networking
        WiFiClient client_;
        WiFiServer server_;

        // Buffers - using std::array for better safety
        static constexpr size_t LOG_BUFFER_SIZE = 512;
        static constexpr size_t TIMESTAMP_BUFFER_SIZE = 16;
        
        alignas(4) char logBuffer_[LOG_BUFFER_SIZE]; // Aligned for better performance
        mutable char timestampBuffer_[TIMESTAMP_BUFFER_SIZE];
};

// ============================================================================
// CUSTOM FORMATTERS FOR CLEVERCOFFEE TYPES
// ============================================================================

// Example CleverCoffee types (you'd define these in your actual headers)
struct SensorReading {
    double temperature;
    double pressure;
    unsigned long timestamp;
    bool valid;
};

struct MemoryInfo {
    size_t total;
    size_t used;
    size_t largestBlock;
    float usagePercent;
};

struct ProcessState {
    double pidOutput;
    double setpoint;
    bool heaterOn;
    bool pumpOn;
};

// Custom formatters - these make logging type-safe and efficient
template<>
struct std::formatter<SensorReading> {
    constexpr auto parse(std::format_parse_context& ctx) {
        return ctx.begin();
    }
    
    auto format(const SensorReading& reading, std::format_context& ctx) const {
        if (reading.valid) {
            return std::format_to(ctx.out(), 
                "temp={:.2f}°C, pressure={:.2f}bar @ {}ms", 
                reading.temperature, reading.pressure, reading.timestamp);
        } else {
            return std::format_to(ctx.out(), "INVALID @ {}ms", reading.timestamp);
        }
    }
};

template<>
struct std::formatter<ProcessState> {
    constexpr auto parse(std::format_parse_context& ctx) {
        return ctx.begin();
    }
    
    auto format(const ProcessState& state, std::format_context& ctx) const {
        return std::format_to(ctx.out(), 
            "PID={:.1f}% target={:.1f}°C heater={} pump={}", 
            state.pidOutput, state.setpoint, 
            state.heaterOn ? "ON" : "OFF", 
            state.pumpOn ? "ON" : "OFF");
    }
};

// ============================================================================
// MODERN LOGGING MACROS
// ============================================================================

#ifndef __FILE_NAME__
#define __FILE_NAME__ (strrchr(__FILE__, '/') ? strrchr(__FILE__, '/') + 1 : __FILE__)
#endif

// Modern macros using std::format
#define MODERN_LOG(level, ...) \
    do { \
        if (ModernLogger::Level::level >= ModernLogger::getCurrentLevel()) { \
            ModernLogger::getInstance().log(ModernLogger::Level::level, __VA_ARGS__); \
        } \
    } while (0)

// Specialized macros for common use cases
#define LOG_TEMP(level, current, target) \
    ModernLogger::getInstance().logTemperature(ModernLogger::Level::level, current, target)

#define LOG_MEMORY(level, mem_info) \
    ModernLogger::getInstance().logMemory(ModernLogger::Level::level, mem_info)

#define LOG_SENSOR(level, reading) \
    ModernLogger::getInstance().logSensorReading(ModernLogger::Level::level, reading)

#define LOG_PROCESS(level, state) \
    ModernLogger::getInstance().logProcessState(ModernLogger::Level::level, state)

// Conditional logging for performance-critical sections
#define IFLOG(level) if (ModernLogger::Level::level >= ModernLogger::getCurrentLevel())

// Example usage:
// MODERN_LOG(INFO, "System initialized with {} sensors", sensor_count);
// LOG_TEMP(DEBUG, current_temp, target_temp);
// LOG_SENSOR(INFO, temperature_reading);