#include "clevercoffee/Logger.h"

#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <utility>

#if defined(ARDUINO_ARCH_ESP32)
#include <ESP.h>
#endif

namespace {

#if defined(ARDUINO_ARCH_ESP32)
constexpr uint32_t MIN_HEAP_FOR_WIFI_LOG = 30000;

uint32_t currentFreeHeap() noexcept {
    return ESP.getFreeHeap();
}
#else
constexpr uint32_t MIN_HEAP_FOR_WIFI_LOG = 0;

uint32_t currentFreeHeap() noexcept {
    return 100000;
}
#endif

bool heapAllowsWifi() noexcept {
    return currentFreeHeap() >= MIN_HEAP_FOR_WIFI_LOG;
}

} // namespace

Logger::Logger(const Config& config) : config_(config), level_(config.initialLevel), server_(config.port) {
    resetStats();
}

Logger::Config Logger::getDefaultConfig() {
    return Config{};
}

Logger& Logger::getInstanceImpl(const Config* config) {
    static Logger instance = config ? Logger(*config) : Logger(getDefaultConfig());
    return instance;
}

void Logger::init() {
    getInstanceImpl(nullptr);
}

void Logger::init(const Config& config) {
    getInstanceImpl(&config);
}

void Logger::writeToOutputs(const uint8_t* data, size_t len, const bool flushWifi) noexcept {
    if (!data || len == 0) {
        return;
    }

    if (config_.enableSerial && Serial) {
        Serial.write(data, len);
    }

    if (!config_.enableWiFi || !client_.connected() || !heapAllowsWifi()) {
        return;
    }

    const size_t written = client_.write(data, len);
    if (written != len) {
        stats_.networkErrors.fetch_add(1, std::memory_order_relaxed);
    } else {
        lastWifiWriteMs_ = millis();
        if (flushWifi) {
            client_.flush();
        }
    }
}

void Logger::flushRingBuffer() noexcept {
    uint16_t head      = ringHead_.load(std::memory_order_acquire);
    uint16_t flushed   = 0;
    bool     wifiWrote = false;

    while (flushed < MAX_WIFI_FLUSH_PER_UPDATE) {
        LogEntry&  entry    = ring_[head];
        const bool occupied = entry.occupied.load(std::memory_order_acquire);
        if (!occupied) {
            break;
        }

        writeToOutputs(reinterpret_cast<const uint8_t*>(entry.data), entry.length, false);
        if (config_.enableWiFi && client_.connected()) {
            wifiWrote = true;
        }

        entry.occupied.store(false, std::memory_order_release);
        head = (head + 1) % LOG_RING_SIZE;
        ringHead_.store(head, std::memory_order_release);
        ++flushed;
    }

    if (wifiWrote && client_.connected()) {
        client_.flush();
    }
}

bool Logger::begin() {
    auto& instance = getInstance();

    if (instance.config_.enableSerial && !Serial) {
        Serial.begin(instance.config_.serialBaud);
        delay(100);
    }

    if (instance.config_.enableWiFi && WiFi.status() == WL_CONNECTED) {
        instance.server_.begin();
        instance.serverStarted_ = true;
    }

    return true;
}

bool Logger::update() {
    auto& instance = getInstance();

    if (instance.config_.enableWiFi && WiFi.status() == WL_CONNECTED) {
        if (!instance.serverStarted_) {
            instance.server_.begin();
            instance.serverStarted_ = true;
        }

        if (instance.server_.hasClient()) {
            if (instance.client_ && instance.client_.connected()) {
                instance.client_.stop();
            }
            instance.client_ = instance.server_.available();
            if (instance.client_) {
                instance.client_.setNoDelay(true);
                instance.client_.write("CleverCoffee log stream connected\r\n");
                instance.client_.flush();
                instance.lastWifiWriteMs_ = millis();
            }
        }
    }

    instance.flushRingBuffer();

    if (instance.config_.enableWiFi && instance.client_.connected()) {
        const uint32_t now = millis();
        if (now - instance.lastWifiWriteMs_ >= HEARTBEAT_INTERVAL_MS) {
            instance.client_.write("# heartbeat\r\n");
            instance.client_.flush();
            instance.lastWifiWriteMs_ = now;
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
    const auto now = time(nullptr);
    if (now == -1) {
        return false;
    }

    struct tm   tm_buf;
    const auto* tm = localtime_r(&now, &tm_buf);
    if (!tm) {
        return false;
    }
    snprintf(buffer, bufferSize, "%02d:%02d:%02d", tm->tm_hour, tm->tm_min, tm->tm_sec);
    return true;
}

bool Logger::formatLogMessage(Level       level,
                              const char* file,
                              const char* function,
                              uint32_t    line,
                              const char* message,
                              char*       buffer,
                              size_t      bufferSize) const {
    if (!buffer || bufferSize == 0) {
        return false;
    }

    const char* safeMessage = message ? message : "[NULL_MESSAGE]";
    char        tsBuf[TIMESTAMP_BUFFER_SIZE];
    const bool  hasTimestamp = formatTimestamp(tsBuf, sizeof(tsBuf));

    int result = 0;
    if (hasTimestamp) {
        result = snprintf(buffer, bufferSize, "[%s] [%s] %s\r\n", tsBuf, getLevelString(level), safeMessage);
    } else {
        result = snprintf(buffer, bufferSize, "[%s] %s\r\n", getLevelString(level), safeMessage);
    }

    if (result < 0 || static_cast<size_t>(result) >= bufferSize) {
        if (bufferSize > 4) {
            buffer[bufferSize - 4] = '.';
            buffer[bufferSize - 3] = '.';
            buffer[bufferSize - 2] = '.';
            buffer[bufferSize - 1] = '\0';
        }
    }

    return true;
}

void Logger::log(const Level level, const char* file, const char* function, uint32_t line, const char* logmsg) {
    if (level < level_) {
        return;
    }

    if (!logmsg) {
        logmsg = "[NULL_LOG_MESSAGE]";
    }

    auto& instance = getInstance();

    char tmp[LOG_ENTRY_SIZE];
    formatLogMessage(level, file, function, line, logmsg, tmp, sizeof(tmp));

    uint16_t tail = instance.ringTail_.load(std::memory_order_relaxed);
    uint16_t nextTail;
    do {
        nextTail = (tail + 1) % LOG_RING_SIZE;
        if (instance.ring_[nextTail].occupied.load(std::memory_order_acquire)) {
            instance.stats_.messagesDropped.fetch_add(1, std::memory_order_relaxed);
            return;
        }
    } while (!instance.ringTail_.compare_exchange_weak(
        tail, nextTail, std::memory_order_acq_rel, std::memory_order_relaxed));

    LogEntry& slot = instance.ring_[tail];
    size_t    len  = strnlen(tmp, sizeof(tmp));
    if (len >= LOG_ENTRY_SIZE) {
        len = LOG_ENTRY_SIZE - 1;
    }
    memcpy(slot.data, tmp, len);
    slot.data[len] = '\0';
    slot.length    = static_cast<uint16_t>(len);

    slot.occupied.store(true, std::memory_order_release);
}

void Logger::logf(const Level level, const char* file, const char* function, uint32_t line, const char* format, ...) {
    if (level < level_) {
        return;
    }

    if (!format) {
        log(level, file, function, line, "[NULL_FORMAT_STRING]");
        return;
    }

    char    buf[LOG_BUFFER_SIZE];
    va_list args;
    va_start(args, format);
    const int result = vsnprintf(buf, sizeof(buf) - 1, format, args);
    va_end(args);

    if (result < 0) {
        strcpy(buf, "[FORMAT_ERROR]");
    } else if (static_cast<size_t>(result) >= sizeof(buf) - 1) {
        buf[sizeof(buf) - 4] = '.';
        buf[sizeof(buf) - 3] = '.';
        buf[sizeof(buf) - 2] = '.';
        buf[sizeof(buf) - 1] = '\0';
    } else {
        buf[sizeof(buf) - 1] = '\0';
    }

    log(level, file, function, line, buf);
}
