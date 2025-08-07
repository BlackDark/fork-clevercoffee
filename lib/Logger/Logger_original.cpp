#include "Logger.h"
#include <cstdarg>
#include <cstdio>
#include <cstring>

Logger::Logger(const Config& config) :
    config_(config), level_(config.initialLevel), server_(config.port) {
    memset(logBuffer_, 0, LOG_BUFFER_SIZE);
    memset(timestampBuffer_, 0, TIMESTAMP_BUFFER_SIZE);
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

    // Handle WiFi client connections
    if (instance.config_.enableWiFi && WiFi.status() == WL_CONNECTED) {
        if (instance.server_.hasClient()) {
            if (instance.client_.connected()) {
                // Reject new connection if already connected
                LOG(WARNING, "Serial Server Connection rejected - already connected");
                WiFiClient newClient = instance.server_.available();
                newClient.stop();
            }
            else {
                // Accept new connection
                LOG(INFO, "Serial Server Connection accepted");
                instance.client_ = instance.server_.available();
            }
        }
    }

    return true;
}

uint16_t Logger::getPort() {
    return getInstance().config_.port;
}

void Logger::log(const Level level, const char* file, const char* function, uint32_t line, const char* logmsg) {
    if (!logmsg) return;

    if (formatLogMessage(level, file, function, line, logmsg, logBuffer_, LOG_BUFFER_SIZE)) {
        sendLogMessage(logBuffer_);
    }
}

void Logger::logf(const Level level, const char* file, const char* function, uint32_t line, const char* format, ...) {
    if (!format) return;

    va_list args;
    va_start(args, format);

    // Create a temporary buffer for the formatted message
    char tempBuffer[256];
    int result = vsnprintf(tempBuffer, sizeof(tempBuffer), format, args);
    va_end(args);

    if (result < 0) {
        // Formatting error
        log(level, file, function, line, "[FORMAT ERROR]");
        return;
    }

    if (result >= static_cast<int>(sizeof(tempBuffer))) {
        // Message was truncated
        tempBuffer[sizeof(tempBuffer) - 1] = '\0';
        strncat(tempBuffer, "...[TRUNCATED]", sizeof(tempBuffer) - strlen(tempBuffer) - 1);
    }

    log(level, file, function, line, tempBuffer);
}

bool Logger::formatTimestamp(char* buffer, size_t bufferSize) const {
    if (!buffer || bufferSize < 12) return false;

    time_t rawtime;
    struct tm* timeinfo;
    time(&rawtime);
    timeinfo = localtime(&rawtime);

    if (!timeinfo) {
        // Fallback if time is not available
        snprintf(buffer, bufferSize, "[--:--:--] ");
        return true;
    }

    int result = snprintf(buffer, bufferSize, "[%02d:%02d:%02d] ", timeinfo->tm_hour, timeinfo->tm_min, timeinfo->tm_sec);

    return result > 0 && result < static_cast<int>(bufferSize);
}

bool Logger::formatLogMessage(Level level, const char* file, const char* function, uint32_t line, const char* message, char* buffer, size_t bufferSize) const {
    if (!buffer || !message || bufferSize == 0) return false;

    // Format timestamp
    if (!formatTimestamp(timestampBuffer_, TIMESTAMP_BUFFER_SIZE)) {
        return false;
    }

    const char* levelStr = getLevelString(level);
    size_t pos = 0;

    // Add timestamp
    int result = snprintf(buffer + pos, bufferSize - pos, "%s", timestampBuffer_);
    if (result < 0 || static_cast<size_t>(result) >= bufferSize - pos) return false;
    pos += result;

    // Add log level
    result = snprintf(buffer + pos, bufferSize - pos, "%s ", levelStr);
    if (result < 0 || static_cast<size_t>(result) >= bufferSize - pos) return false;
    pos += result;

    // Add file, line, and function info for TRACE and DEBUG levels
    if (level <= Level::DEBUG && file && function) {
        result = snprintf(buffer + pos, bufferSize - pos, "%s:%u@%s() ", file, line, function);
        if (result < 0 || static_cast<size_t>(result) >= bufferSize - pos) return false;
        pos += result;
    }

    // Add the actual message
    result = snprintf(buffer + pos, bufferSize - pos, "%s\n", message);
    if (result < 0 || static_cast<size_t>(result) >= bufferSize - pos) return false;

    return true;
}

void Logger::sendLogMessage(const char* message) {
    if (!message) return;

    // Send to WiFi client if connected
    if (config_.enableWiFi && client_.connected()) {
        client_.print(message);
    }
    // Send to Serial (fallback or primary output)
    else if (config_.enableSerial && Serial) {
        Serial.print(message);
    }
}

const char* Logger::getLevelString(Level level) {
    switch (level) {
        case Level::TRACE:
            return "  TRACE";
        case Level::DEBUG:
            return "  DEBUG";
        case Level::INFO:
            return "   INFO";
        case Level::WARNING:
            return "WARNING";
        case Level::ERROR:
            return "  ERROR";
        case Level::FATAL:
            return "  FATAL";
        default:
            return " SILENT";
    }
}
