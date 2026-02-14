/**
 * @file Logger.h
 * @brief Logger
 *
 */

#pragma once

#include <WiFi.h>
#include <WiFiClient.h>
#include <WiFiServer.h>
#include <array>
#include <atomic>
#include <ctime>
#include <memory>
#include <stdint.h>
#include <vector>

#if __has_include(<esp_log.h>)
#include <esp_log.h>
#define HAVE_ESP_LOG 1
#endif

class Logger {
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

    struct Config {
        uint16_t port          = 23;
        size_t   maxBufferSize = 512;
        bool     enableSerial  = true;
        bool     enableWiFi    = true;
        uint32_t serialBaud    = 115200;
        Level    initialLevel  = Level::INFO;
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

    Logger(Logger const&)         = delete;
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

    static const char* getLevelString(Level level) noexcept;

    // Performance statistics
    struct Stats {
        size_t        messagesLogged  = 0;
        size_t        messagesDropped = 0;
        size_t        networkErrors   = 0;
        unsigned long totalTime       = 0; // microseconds
    };

    const Stats& getStats() const {
        return stats_;
    }
    void resetStats() {
        stats_ = {};
    }

  private:
    static Logger& getInstanceImpl(const Config* config = nullptr);
    static Config  getDefaultConfig();

    /**
     * @brief Constructor for a logger
     * @param config Configuration for the logger
     */
    explicit Logger(const Config& config);

    bool formatTimestamp(char* buffer, size_t bufferSize) const;
    bool formatLogMessage(Level       level,
                          const char* file,
                          const char* function,
                          uint32_t    line,
                          const char* message,
                          char*       buffer,
                          size_t      bufferSize) const;
    void sendLogMessage(const char* message);

    // Configuration and state
    Config config_;
    Level  level_;
    Stats  stats_;

    // Networking
    WiFiClient client_;
    WiFiServer server_;

    // Static buffer to avoid heap fragmentation
    static constexpr size_t LOG_BUFFER_SIZE = 512;
    char                    logBuffer_[LOG_BUFFER_SIZE];

#ifndef HAVE_ESP_LOG
    // Lock-free ring buffer for non-blocking logging
    static constexpr size_t LOG_ENTRY_SIZE = LOG_BUFFER_SIZE + 64;
    static constexpr size_t LOG_RING_SIZE  = 64; // configurable number of entries

    struct LogEntry {
        std::atomic<bool> occupied{false};
        uint16_t          length{0};
        char              data[LOG_ENTRY_SIZE];
    };

    // Circular buffer indexes
    std::atomic<uint16_t> ringHead_{0};
    std::atomic<uint16_t> ringTail_{0};
    LogEntry              ring_[LOG_RING_SIZE];

    // Pluggable backend abstraction
    struct ILoggerBackend {
        virtual ~ILoggerBackend()             = default;
        virtual void begin(const Config& cfg) = 0;
        virtual void update()                 = 0;
        virtual void sink(const char* msg)    = 0;
    };

    // Registered backends
    std::vector<std::unique_ptr<ILoggerBackend>> backends_;

    // Register a backend (call before begin() for custom ordering)
    void registerBackend(std::unique_ptr<ILoggerBackend> backend) {
        backends_.push_back(std::move(backend));
    }
#endif

    // Timestamp buffer
    static constexpr size_t TIMESTAMP_BUFFER_SIZE = 16;
    mutable char            timestampBuffer_[TIMESTAMP_BUFFER_SIZE];
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

// Original macros - maintained for backwards compatibility
#define LOG(level, message)                                                                                            \
    do {                                                                                                               \
        if (Logger::Level::level >= Logger::getCurrentLevel()) {                                                       \
            Logger::getInstance().log(Logger::Level::level, __FILE_NAME__, __func__, __LINE__, message);               \
        }                                                                                                              \
    } while (0)

#define LOGF(level, format, ...)                                                                                       \
    do {                                                                                                               \
        if (Logger::Level::level >= Logger::getCurrentLevel()) {                                                       \
            Logger::getInstance().logf(                                                                                \
                Logger::Level::level, __FILE_NAME__, __func__, __LINE__, format, ##__VA_ARGS__);                       \
        }                                                                                                              \
    } while (0)
