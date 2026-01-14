/**
 * @file MockLogger.h
 * @brief Mock Logger implementation for testing
 * 
 * Provides a no-op Logger implementation that can be used in tests
 * to avoid dependencies on WiFi, Serial, and other hardware.
 */

#pragma once

#include "clevercoffee/Logger.h"
#include <cstdarg>
#include <cstdio>

// Mock Logger implementation for tests
class MockLogger {
public:
    enum class Level : int {
        TRACE   = 0,
        DEBUG   = 1,
        INFO    = 2,
        WARNING = 3,
        ERROR   = 4,
        FATAL   = 5,
        SILENT  = 6,
    };

    static MockLogger& getInstance() {
        static MockLogger instance;
        return instance;
    }

    static void setLevel(Level level) {
        getInstance().level_ = level;
    }

    static Level getCurrentLevel() {
        return getInstance().level_;
    }

    void log(Level level, const char* file, const char* function, uint32_t line, const char* logmsg) {
        // No-op for tests - can be extended to capture logs if needed
        (void)level;
        (void)file;
        (void)function;
        (void)line;
        (void)logmsg;
    }

    void logf(Level level, const char* file, const char* function, uint32_t line, const char* format, ...) {
        // No-op for tests - can be extended to capture logs if needed
        (void)level;
        (void)file;
        (void)function;
        (void)line;
        (void)format;
    }

private:
    Level level_ = Level::INFO;
};

// Override Logger::getInstance() to return MockLogger in tests
namespace CleverCoffee {
    // Forward declare that we'll provide a test implementation
}

// Provide __FILE_NAME__ if not defined
#ifndef __FILE_NAME__
#define __FILE_NAME__ (strrchr(__FILE__, '/') ? strrchr(__FILE__, '/') + 1 : __FILE__)
#endif

// Note: LOG and LOGF macros from Logger.h will be used, but they call Logger::getInstance()
// which will work with the real Logger implementation. The MockLogger is just for reference.
// We don't override the macros here to avoid conflicts - the real Logger will work with stubbed WiFi/Serial.
