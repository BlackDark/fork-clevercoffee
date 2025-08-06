#include "Logger.h"
#include <cstdarg>
#include <cstdio>
#include <cstring>

Logger::Logger(const Config& config) :
    config_(config), level_(config.initialLevel), server_(config.port) {
    memset(logBuffer_, 0, LOG_BUFFER_SIZE);
    memset(timestampBuffer_, 0, TIMESTAMP_BUFFER_SIZE);
    stats_ = {}; // Initialize stats
}

Logger::Config Logger::getDefaultConfig() {
    return Config{}; // Returns a default-constructed Config
}

Logger& Logger::getInstanceImpl() {
    static Logger instance(getDefaultConfig());
    return instance;
}

Logger& Logger::getInstanceImpl(const Config& config) {
    static Logger instance(config);
    return instance;
}

void Logger::init() {
    getInstanceImpl(); // Use default config
}

void Logger::init(const Config& config) {
    getInstanceImpl(config);
}

bool Logger::begin() {
    auto& instance = getInstance();

    // Initialize WiFi server if WiFi is enabled and connected
    if (instance.config_.enableWiFi && WiFi.status() == WL_CONNECTED) {
        instance.server_.begin();
    }

    // Initialize Serial if enabled and not already started
    if (instance.config_.enableSerial && !Serial) {
        Serial.begin(instance.config_.serialBaud);
        // Give serial some time to initialize
        delay(100);
    }

    return true;
}

bool Logger::update() {
    auto& instance = getInstance();

    // Handle WiFi client connections if enabled
    if (instance.config_.enableWiFi && WiFi.status() == WL_CONNECTED) {
        // Accept new client if available
        if (instance.server_.hasClient()) {
            // If we already have a client, disconnect it
            if (instance.client_ && instance.client_.connected()) {
                instance.client_.stop();
            }
            instance.client_ = instance.server_.available();
        }
    }

    return true;
}

uint16_t Logger::getPort() {
    return getInstance().config_.port;
}

const char* Logger::getLevelString(Level level) {
    switch (level) {
        case Level::TRACE: return "TRACE";
        case Level::DEBUG: return "DEBUG";
        case Level::INFO: return "INFO";
        case Level::WARNING: return "WARNING";
        case Level::ERROR: return "ERROR";
        case Level::FATAL: return "FATAL";
        case Level::SILENT: return "SILENT";
        default: return "UNKNOWN";
    }
}

bool Logger::formatTimestamp(char* buffer, size_t bufferSize) const {
    auto now = time(nullptr);
    if (now == -1) {
        return false;
    }
    
    auto tm = *localtime(&now);
    snprintf(buffer, bufferSize, "%02d:%02d:%02d", tm.tm_hour, tm.tm_min, tm.tm_sec);
    return true;
}

bool Logger::formatLogMessage(Level level, const char* file, const char* function, uint32_t line, const char* message, char* buffer, size_t bufferSize) const {
    bool hasTimestamp = formatTimestamp(timestampBuffer_, TIMESTAMP_BUFFER_SIZE);
    
    if (hasTimestamp) {
        snprintf(buffer, bufferSize, "[%s] [%s] %s\r\n", 
                 timestampBuffer_, getLevelString(level), message);
    } else {
        snprintf(buffer, bufferSize, "[%s] %s\r\n", 
                 getLevelString(level), message);
    }
    
    return true;
}

void Logger::sendLogMessage(const char* message) {
    auto start_time = micros();
    
    // Send to serial if enabled
    if (config_.enableSerial) {
        Serial.print(message);
    }

    // Send to WiFi client if connected
    if (config_.enableWiFi && client_ && client_.connected()) {
        client_.write(message);
    }
    
    // Update statistics
    auto end_time = micros();
    stats_.totalTime += (end_time - start_time);
    ++stats_.messagesLogged;
}

void Logger::log(const Level level, const char* file, const char* function, uint32_t line, const char* logmsg) {
    if (level < level_) {
        return;
    }

    char fullMessage[LOG_BUFFER_SIZE + 64]; // Extra space for timestamp and level
    formatLogMessage(level, file, function, line, logmsg, fullMessage, sizeof(fullMessage));
    sendLogMessage(fullMessage);
}

void Logger::logf(const Level level, const char* file, const char* function, uint32_t line, const char* format, ...) {
    if (level < level_) {
        return;
    }

    va_list args;
    va_start(args, format);
    vsnprintf(logBuffer_, LOG_BUFFER_SIZE - 1, format, args);
    va_end(args);
    
    logBuffer_[LOG_BUFFER_SIZE - 1] = '\0'; // Ensure null termination
    
    log(level, file, function, line, logBuffer_);
}

#if __cplusplus >= 202300L
void Logger::sendFormattedMessage(Level level, const char* file, const char* function, uint32_t line, const char* message) {
    char fullMessage[LOG_BUFFER_SIZE + 64]; // Extra space for timestamp and level
    formatLogMessage(level, file, function, line, message, fullMessage, sizeof(fullMessage));
    sendLogMessage(fullMessage);
}
#endif