#pragma once

#include <WiFi.h>
#include <WiFiClient.h>
#include <WiFiServer.h>
#include <ctime>
#include <stdint.h>
#include <string_view>

class Logger {
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

        struct Config {
                uint16_t port = 23;
                size_t maxBufferSize = 512;
                bool enableSerial = true;
                bool enableWiFi = true;
                uint32_t serialBaud = 115200;
                Level initialLevel = Level::INFO;
        };

        /**
         * @brief Return singleton instance of the logger
         * @return Logger instance
         */
        static Logger& getInstance() {
            return getInstanceImpl();
        }

        /**
         * @brief Initialize the singleton logger instance with default config
         */
        static void init();

        /**
         * @brief Initialize the singleton logger instance with custom config
         * @param config Configuration struct for the logger
         */
        static void init(const Config& config);

        Logger(Logger const&) = delete;
        void operator=(Logger const&) = delete;

        /**
         * @brief Start the logger
         * @return Boolean indicating the success of the operation
         */
        static bool begin();

        /**
         * @brief Update the logger - handle connections and level changes
         * @return Boolean indicating the success of the operation
         */
        static bool update();

        /**
         * @brief Get the port this logger is communicating over
         * @return Used port
         */
        static uint16_t getPort();

        /**
         * @brief Send a log message either via serial or serial-over-wifi
         * @param level Log level
         * @param file The file name of the file containing the log message
         * @param function The function containing the log message
         * @param line The line number of the log message
         * @param logmsg Log message to be sent as payload
         */
        void log(const Level level, const char* file, const char* function, uint32_t line, const char* logmsg);

        /**
         * @brief Send a formatted log message either via serial or serial-over-wifi
         * @param level Log level
         * @param file The file name of the file containing the log message
         * @param function The function containing the log message
         * @param line The line number of the log message
         * @param format Format string akin to printf
         * @param ... Parameter list
         */
        void logf(const Level level, const char* file, const char* function, uint32_t line, const char* format, ...);

        static void setLevel(Level level) {
            getInstance().level_ = level;
        }

        static Level getCurrentLevel() {
            return getInstance().level_;
        }

        static std::string_view getLevelString(Level level) noexcept;

    private:
        static Logger& getInstanceImpl();
        static Logger& getInstanceImpl(const Config& config);
        static Config getDefaultConfig();

        /**
         * @brief Constructor for a logger
         * @param config Configuration for the logger
         */
        explicit Logger(const Config& config);

        bool formatTimestamp(char* buffer, size_t bufferSize) const;
        bool formatLogMessage(Level level, const char* file, const char* function, uint32_t line, const char* message, char* buffer, size_t bufferSize) const;
        void sendLogMessage(const char* message);

        // Configuration and state
        Config config_;
        Level level_;

        // Networking
        WiFiClient client_;
        WiFiServer server_;

        // Static buffer to avoid heap fragmentation
        static constexpr size_t LOG_BUFFER_SIZE = 512;
        char logBuffer_[LOG_BUFFER_SIZE];

        // Timestamp buffer
        static constexpr size_t TIMESTAMP_BUFFER_SIZE = 16;
        mutable char timestampBuffer_[TIMESTAMP_BUFFER_SIZE];
};

#ifndef __FILE_NAME__
/**
 * @brief Base name of the file without the directory
 */
#define __FILE_NAME__ (strrchr(__FILE__, '/') ? strrchr(__FILE__, '/') + 1 : __FILE__)
#endif

/**
 * @brief Execute a block only if the reporting level is high enough
 * @param level The minimum log level
 */
#define IFLOG(level) if (Logger::Level::level >= Logger::getCurrentLevel())

#define LOG(level, message)                                                                                                                                                                                                     \
    do {                                                                                                                                                                                                                        \
        if (Logger::Level::level >= Logger::getCurrentLevel()) {                                                                                                                                                                \
            Logger::getInstance().log(Logger::Level::level, __FILE_NAME__, __func__, __LINE__, message);                                                                                                                        \
        }                                                                                                                                                                                                                       \
    } while (0)

// Fixed LOGF macro to handle variadic arguments properly
#define LOGF(level, format, ...)                                                                                                                                                                                                \
    do {                                                                                                                                                                                                                        \
        if (Logger::Level::level >= Logger::getCurrentLevel()) {                                                                                                                                                                \
            Logger::getInstance().logf(Logger::Level::level, __FILE_NAME__, __func__, __LINE__, format, ##__VA_ARGS__);                                                                                                         \
        }                                                                                                                                                                                                                       \
    } while (0)
