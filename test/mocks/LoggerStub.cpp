/**
 * @file LoggerStub.cpp
 * @brief Stub implementation of Logger for tests
 * 
 * Provides minimal implementations of Logger functions needed for tests.
 */

#include "MockLogger.h"
#include "clevercoffee/Logger.h"

// Stub implementations for Logger singleton
Logger& Logger::getInstanceImpl() {
    static Logger instance(Logger::getDefaultConfig());
    return instance;
}

Logger& Logger::getInstanceImpl(const Config& config) {
    static Logger instance(config);
    return instance;
}

Logger::Config Logger::getDefaultConfig() {
    return Logger::Config{};
}

void Logger::init() {
    // No-op for tests
}

void Logger::init(const Config& config) {
    // No-op for tests
}

bool Logger::begin() {
    return true;
}

bool Logger::update() {
    return true;
}

uint16_t Logger::getPort() {
    return 23;
}

void Logger::log(const Level level, const char* file, const char* function, uint32_t line, const char* logmsg) {
    // No-op for tests - can be extended to capture logs if needed
    (void)level;
    (void)file;
    (void)function;
    (void)line;
    (void)logmsg;
}

void Logger::logf(const Level level, const char* file, const char* function, uint32_t line, const char* format, ...) {
    // No-op for tests - can be extended to capture logs if needed
    (void)level;
    (void)file;
    (void)function;
    (void)line;
    (void)format;
}

const char* Logger::getLevelString(Level level) noexcept {
    switch (level) {
        case Level::TRACE:   return "TRACE";
        case Level::DEBUG:   return "DEBUG";
        case Level::INFO:    return "INFO";
        case Level::WARNING: return "WARNING";
        case Level::ERROR:   return "ERROR";
        case Level::FATAL:   return "FATAL";
        case Level::SILENT:  return "SILENT";
        default:             return "UNKNOWN";
    }
}

// Constructor implementation
Logger::Logger(const Config& config) : config_(config), level_(config.initialLevel) {
}

bool Logger::formatTimestamp(char* buffer, size_t bufferSize) const {
    (void)buffer;
    (void)bufferSize;
    return false;
}

bool Logger::formatLogMessage(Level level, const char* file, const char* function, uint32_t line, const char* message, char* buffer, size_t bufferSize) const {
    (void)level;
    (void)file;
    (void)function;
    (void)line;
    (void)message;
    (void)buffer;
    (void)bufferSize;
    return false;
}
