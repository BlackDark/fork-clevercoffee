#include "clevercoffee/Logger.h"

#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <utility>
#if __has_include(<esp_log.h>)
#include <esp_log.h>
#define HAVE_ESP_LOG 1
#endif

Logger::Logger(const Config& config) : config_(config), level_(config.initialLevel), server_(config.port) {
    memset(logBuffer_, 0, LOG_BUFFER_SIZE);
    memset(timestampBuffer_, 0, TIMESTAMP_BUFFER_SIZE);
    stats_ = {}; // Initialize stats
}

Logger::Config Logger::getDefaultConfig() {
    return Config{}; // Returns a default-constructed Config
}

Logger& Logger::getInstanceImpl(const Config* config) {
    // Single static instance - initialized on first access
    // If config is provided on first call, it's used; otherwise default config
    static Logger instance = config ? Logger(*config) : Logger(getDefaultConfig());
    return instance;
}

void Logger::init() {
    // Initialize default instance with default config
    getInstanceImpl(nullptr);
}

void Logger::init(const Config& config) {
    // Initialize default instance with provided config
    getInstanceImpl(&config);
}

bool Logger::begin() {
    auto& instance = getInstance();

    // Initialize Serial if enabled and not already started
    if (instance.config_.enableSerial && !Serial) {
        Serial.begin(instance.config_.serialBaud);
        // Give serial some time to initialize
        delay(100);
    }

#ifndef HAVE_ESP_LOG
    // If no backends were registered, register default backends
    if (instance.backends_.empty()) {
        // Serial backend (always available)
        struct SerialBackend : public Logger::ILoggerBackend {
            void begin(const Logger::Config& cfg) override {
                (void)cfg;
            }
            void update() override {}
            void sink(const char* msg) override {
                if (Serial) Serial.print(msg);
            }
        };
        instance.registerBackend(std::make_unique<SerialBackend>());

        // Network backend: simple WiFi client writer using existing server/client
        struct NetBackend : public Logger::ILoggerBackend {
            void begin(const Logger::Config& cfg) override {
                (void)cfg;
            }
            void update() override {}
            void sink(const char* msg) override {
                // Attempt best-effort write using global client in Logger instance
                auto& inst = Logger::getInstance();
                if (inst.config_.enableWiFi && inst.client_ && inst.client_.connected()) {
                    if (!inst.client_.write(msg)) {
                        ++inst.stats_.networkErrors;
                    }
                }
            }
        };
        instance.registerBackend(std::make_unique<NetBackend>());
    }

    // Call begin on registered backends
    for (auto& b : instance.backends_) {
        if (b) b->begin(instance.config_);
    }
#endif

    // Start server if WiFi is enabled AND connected
    // WiFi must be initialized before calling server_.begin() or LWIP will crash
    if (instance.config_.enableWiFi && WiFi.status() == WL_CONNECTED) {
        instance.server_.begin();
        instance.serverStarted_ = true;
    }

    return true;
}

bool Logger::update() {
    auto& instance = getInstance();

    // Handle WiFi client connections if enabled
    if (instance.config_.enableWiFi && WiFi.status() == WL_CONNECTED) {
        // Start server if not yet started (deferred from begin() when WiFi wasn't ready)
        if (!instance.serverStarted_) {
            instance.server_.begin();
            instance.serverStarted_ = true;
        }

        // Accept new client if available
        if (instance.server_.hasClient()) {
            // If we already have a client, disconnect it
            if (instance.client_ && instance.client_.connected()) {
                instance.client_.stop();
            }
            instance.client_ = instance.server_.available();
        }
    }

#ifndef HAVE_ESP_LOG
    // Give backends a chance to update (e.g., accept clients)
    for (auto& b : instance.backends_) {
        if (b) b->update();
    }

    // Flush queued log messages (non-blocking producers push into ring)
    uint16_t head = instance.ringHead_.load(std::memory_order_acquire);
    while (true) {
        LogEntry& entry    = instance.ring_[head];
        bool      occupied = entry.occupied.load(std::memory_order_acquire);
        if (!occupied) {
            break; // queue empty
        }

        // Send the message to backends (or fallback inside sendLogMessage)
        instance.sendLogMessage(entry.data);

        // Mark entry free
        entry.occupied.store(false, std::memory_order_release);

        // advance head
        head = (head + 1) % LOG_RING_SIZE;
        instance.ringHead_.store(head, std::memory_order_release);
    }
#endif

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

    // Route to registered backends; if none present, fall back to Serial
#ifndef HAVE_ESP_LOG
    if (!backends_.empty()) {
        for (auto& b : backends_) {
            if (b) {
                b->sink(message);
            }
        }
    } else {
        // Fallback behaviour
        if (config_.enableSerial) {
            Serial.print(message);
        }
        if (config_.enableWiFi && client_ && client_.connected()) {
            // best-effort network write
            if (!client_.write(message)) {
                ++stats_.networkErrors;
            }
        }
    }
#else
    // In esp builds we prefer esp_log; but sendLogMessage can still be used as a fallback to serial/network
    if (config_.enableSerial) {
        Serial.print(message);
    }
#endif

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

    // If esp-idf native logging is available, use it directly and bypass buffering/backends.
#if HAVE_ESP_LOG
    char tmp[LOG_BUFFER_SIZE + 64];
    formatLogMessage(level, file, function, line, logmsg, tmp, sizeof(tmp));
    // Map levels to esp_log equivalents
    const char* tag = "CleverCoffee";
    switch (level) {
        case Level::TRACE:
        case Level::DEBUG:
            ESP_LOGD(tag, "%s", tmp);
            break;
        case Level::INFO:
            ESP_LOGI(tag, "%s", tmp);
            break;
        case Level::WARNING:
            ESP_LOGW(tag, "%s", tmp);
            break;
        case Level::ERROR:
        case Level::FATAL:
            ESP_LOGE(tag, "%s", tmp);
            break;
        default:
            ESP_LOGI(tag, "%s", tmp);
            break;
    }
    ++getInstance().stats_.messagesLogged;
    return;
#else
    // Attempt to enqueue the formatted message into the ring buffer.
    // If the ring is full, drop the message to keep callers non-blocking.
    auto& instance = getInstance();

    // Format directly into a temporary buffer on the stack and then copy into ring entry
    char tmp[LOG_BUFFER_SIZE + 64];
    formatLogMessage(level, file, function, line, logmsg, tmp, sizeof(tmp));

    // Reserve tail
    uint16_t  tail     = instance.ringTail_.load(std::memory_order_acquire);
    uint16_t  nextTail = (tail + 1) % LOG_RING_SIZE;
    LogEntry& slot     = instance.ring_[tail];

    // If next slot is occupied then the ring is full
    if (instance.ring_[nextTail].occupied.load(std::memory_order_acquire)) {
        ++instance.stats_.messagesDropped;
        return; // drop
    }

    // Copy into slot
    size_t len = strnlen(tmp, sizeof(tmp));
    if (len >= LOG_ENTRY_SIZE) {
        len = LOG_ENTRY_SIZE - 1;
    }
    memcpy(slot.data, tmp, len);
    slot.data[len] = '\0';
    slot.length    = static_cast<uint16_t>(len);

    // Publish
    slot.occupied.store(true, std::memory_order_release);
    instance.ringTail_.store(nextTail, std::memory_order_release);
#endif
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
