#include "clevercoffee/Logger.h"

#include <cstdarg>
#include <cstdio>
#include <cstring>

Logger::Logger(const Config& config) : config_(config), level_(config.initialLevel), server_(config.port) {
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

const char* Logger::getLevelString(Level level) noexcept {
    switch (level) {
        case Level::TRACE:
            return "TRACE";
        case Level::DEBUG:
            return "DEBUG";
        case Level::INFO:
            return "INFO";
        case Level::WARNING:
            return "WARNING";
        case Level::ERROR:
            return "ERROR";
        case Level::FATAL:
            return "FATAL";
        case Level::SILENT:
            return "SILENT";
        default:
            return "UNKNOWN";
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

bool Logger::formatLogMessage(Level       level,
                              const char* file,
                              const char* function,
                              uint32_t    line,
                              const char* message,
                              char*       buffer,
                              size_t      bufferSize) const {
    // Safety checks
    if (!buffer || bufferSize == 0) {
        return false;
    }

    // Ensure message is not null
    const char* safeMessage = message ? message : "[NULL_MESSAGE]";

    bool hasTimestamp = formatTimestamp(timestampBuffer_, TIMESTAMP_BUFFER_SIZE);

    if (hasTimestamp) {
        int result =
            snprintf(buffer, bufferSize, "[%s] [%s] %s\r\n", timestampBuffer_, getLevelString(level), safeMessage);
        // Check for truncation or error
        if (result < 0 || static_cast<size_t>(result) >= bufferSize) {
            // Buffer too small, add truncation marker
            if (bufferSize > 4) {
                buffer[bufferSize - 4] = '.';
                buffer[bufferSize - 3] = '.';
                buffer[bufferSize - 2] = '.';
                buffer[bufferSize - 1] = '\0';
            }
        }
    } else {
        int result = snprintf(buffer, bufferSize, "[%s] %s\r\n", getLevelString(level), safeMessage);
        // Check for truncation or error
        if (result < 0 || static_cast<size_t>(result) >= bufferSize) {
            // Buffer too small, add truncation marker
            if (bufferSize > 4) {
                buffer[bufferSize - 4] = '.';
                buffer[bufferSize - 3] = '.';
                buffer[bufferSize - 2] = '.';
                buffer[bufferSize - 1] = '\0';
            }
        }
    }

    return true;
}

void Logger::sendLogMessage(const char* message) {
    // Safety check for null message
    if (!message) {
        return;
    }

    auto start_time = micros();

    // Send to serial if enabled
    if (config_.enableSerial) {
        Serial.print(message);
    }

    // Send to WiFi client if connected
    if (config_.enableWiFi && client_ && client_.connected()) {
        try {
            client_.write(message);
        } catch (...) {
            // Network error occurred, increment error counter
            ++stats_.networkErrors;
        }
    }

    // Update statistics
    auto end_time     = micros();
    stats_.totalTime += (end_time - start_time);
    ++stats_.messagesLogged;
}

void Logger::log(const Level level, const char* file, const char* function, uint32_t line, const char* logmsg) {
    if (level < level_) {
        return;
    }

    // Safety check for null message
    if (!logmsg) {
        logmsg = "[NULL_LOG_MESSAGE]";
    }

    char fullMessage[LOG_BUFFER_SIZE + 64]; // Extra space for timestamp and level
    formatLogMessage(level, file, function, line, logmsg, fullMessage, sizeof(fullMessage));
    sendLogMessage(fullMessage);
}

void Logger::logf(const Level level, const char* file, const char* function, uint32_t line, const char* format, ...) {
    if (level < level_) {
        return;
    }

    // Safety check for null format
    if (!format) {
        log(level, file, function, line, "[NULL_FORMAT_STRING]");
        return;
    }

    va_list args;
    va_start(args, format);
    int result = vsnprintf(logBuffer_, LOG_BUFFER_SIZE - 1, format, args);
    va_end(args);

    // Check for formatting errors or truncation
    if (result < 0) {
        // Formatting error occurred
        strcpy(logBuffer_, "[FORMAT_ERROR]");
    } else if (result >= LOG_BUFFER_SIZE - 1) {
        // Output was truncated, add truncation marker
        logBuffer_[LOG_BUFFER_SIZE - 4] = '.';
        logBuffer_[LOG_BUFFER_SIZE - 3] = '.';
        logBuffer_[LOG_BUFFER_SIZE - 2] = '.';
        logBuffer_[LOG_BUFFER_SIZE - 1] = '\0';
    } else {
        // Ensure null termination
        logBuffer_[LOG_BUFFER_SIZE - 1] = '\0';
    }

    log(level, file, function, line, logBuffer_);
}
