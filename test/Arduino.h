/**
 * @file Arduino.h
 * @brief Arduino.h stub for native test environment
 *
 * This file provides Arduino framework compatibility for PlatformIO native tests.
 * It should be force-included in the build path before any other headers.
 *
 * Note: This file replaces the real Arduino.h for native tests, so we don't
 * check for ARDUINO being defined - we always provide the stubs when this
 * file is included.
 */

#pragma once

#include <cstdint>
#include <string>
#include <cstring>
#include <cstdlib>
#include <algorithm>
#include <cctype>
#include <cstdio>
#include <map>

// Arduino constants (must be defined before String class uses them)
#ifndef HIGH
#define HIGH 1
#endif
#ifndef LOW
#define LOW 0
#endif
#ifndef HEX
#define HEX 16
#endif
#ifndef DEC
#define DEC 10
#endif
#ifndef BIN
#define BIN 2
#endif
#ifndef OCT
#define OCT 8
#endif

// Binary literal macros (B prefix)
#define B00000000 0x00
#define B00000001 0x01
#define B00000010 0x02
#define B00000011 0x03
#define B00000100 0x04
#define B00000101 0x05
#define B00000110 0x06
#define B00000111 0x07
#define B00001000 0x08
#define B00001001 0x09
#define B00001010 0x0A
#define B00001011 0x0B
#define B00001100 0x0C
#define B00001101 0x0D
#define B00001110 0x0E
#define B00001111 0x0F
#define B00010000 0x10
#define B00010001 0x11
#define B00010010 0x12
#define B00010011 0x13
#define B00010100 0x14
#define B00010101 0x15
#define B00010110 0x16
#define B00010111 0x17
#define B00011000 0x18
#define B00011001 0x19
#define B00011010 0x1A
#define B00011011 0x1B
#define B00011100 0x1C
#define B00011101 0x1D
#define B00011110 0x1E
#define B00011111 0x1F
#define B00100000 0x20
#define B00100001 0x21
#define B00100010 0x22
#define B00100011 0x23
#define B00100100 0x24
#define B00100101 0x25
#define B00100110 0x26
#define B00100111 0x27
#define B00101000 0x28
#define B00101001 0x29
#define B00101010 0x2A
#define B00101011 0x2B
#define B00101100 0x2C
#define B00101101 0x2D
#define B00101110 0x2E
#define B00101111 0x2F
#define B00110000 0x30
#define B00110001 0x31
#define B00110010 0x32
#define B00110011 0x33
#define B00110100 0x34
#define B00110101 0x35
#define B00110110 0x36
#define B00110111 0x37
#define B00111000 0x38
#define B00111001 0x39
#define B00111010 0x3A
#define B00111011 0x3B
#define B00111100 0x3C
#define B00111101 0x3D
#define B00111110 0x3E
#define B00111111 0x3F
#define B01000000 0x40
#define B01001000 0x48
#define B01001010 0x4A
#define B01111111 0x7F
#define B10000000 0x80
#define B10001000 0x88
#define B11111111 0xFF

// PROGMEM stubs for native tests
// These need to be valid C++ syntax when used in array declarations
// ArduinoJson uses: static type const name[] PROGMEM = {...};
// PROGMEM must be empty or the syntax breaks on native
#ifndef PROGMEM
#define PROGMEM
#endif
#ifndef U8X8_PROGMEM
#define U8X8_PROGMEM
#endif
#ifndef PGM_P
#define PGM_P const char*
#endif
#ifndef PSTR
#define PSTR(s) (s)
#endif

// pgmspace stubs for ArduinoJson compatibility
inline uint8_t pgm_read_byte(const void* p) { return *static_cast<const uint8_t*>(p); }
inline uint16_t pgm_read_word(const void* p) { return *static_cast<const uint16_t*>(p); }
inline uint32_t pgm_read_dword(const void* p) { return *static_cast<const uint32_t*>(p); }
inline float pgm_read_float(const void* p) { return *static_cast<const float*>(p); }
inline const void* pgm_read_ptr(const void* p) { return *static_cast<const void* const*>(p); }

// memcpy_P and strlen_P stubs
inline void* memcpy_P(void* dest, const void* src, size_t n) { return memcpy(dest, src, n); }
inline size_t strlen_P(const char* s) { return strlen(s); }

// ============================================================================
// LOG Macro Stubs
// ============================================================================

// Stub LOG macros for native tests (no-op)
// These are defined so files that use LOG macros work without Logger.h
// Note: Logger.h will redefine these when included, which is expected
#ifndef LOG
#define LOG(level, message) ((void)0)
#endif
#ifndef LOGF
#define LOGF(level, format, ...) ((void)0)
#endif
#ifndef IFLOG
#define IFLOG(level) if (false)
#endif

// Arduino String class stub with all methods
class String {
public:
    String() : str_() {}
    String(const char* s) : str_(s ? s : "") {}
    String(const std::string& s) : str_(s) {}
    String(char c) : str_(1, c) {}
    String(int value) : str_(std::to_string(value)) {}
    String(unsigned int value) : str_(std::to_string(value)) {}
    String(long value) : str_(std::to_string(value)) {}
    String(unsigned long value) : str_(std::to_string(value)) {}
    String(unsigned long value, int base) {
        if (base == HEX) {
            char buf[32];
            snprintf(buf, sizeof(buf), "%lx", value);
            str_ = buf;
        } else if (base == DEC) {
            str_ = std::to_string(value);
        } else if (base == OCT) {
            char buf[32];
            snprintf(buf, sizeof(buf), "%lo", value);
            str_ = buf;
        } else if (base == BIN) {
            str_ = "";
            for (int i = 31; i >= 0; i--) {
                str_ += ((value >> i) & 1) ? '1' : '0';
            }
        } else {
            str_ = std::to_string(value);
        }
    }
    String(double value) : str_(std::to_string(value)) {}
    String(float value) : str_(std::to_string(value)) {}
    
    // Conversion operators
    operator const char*() const { return str_.c_str(); }
    const char* c_str() const { return str_.c_str(); }
    
    // Comparison
    bool equals(const String& other) const { return str_ == other.str_; }
    bool equalsIgnoreCase(const String& other) const {
        std::string s1 = str_, s2 = other.str_;
        std::transform(s1.begin(), s1.end(), s1.begin(), ::tolower);
        std::transform(s2.begin(), s2.end(), s2.begin(), ::tolower);
        return s1 == s2;
    }
    bool equalsIgnoreCase(const char* other) const {
        return equalsIgnoreCase(String(other));
    }
    
    // Conversion methods
    int toInt() const {
        try { return std::stoi(str_); } catch (...) { return 0; }
    }
    double toDouble() const {
        try { return std::stod(str_); } catch (...) { return 0.0; }
    }
    float toFloat() const {
        try { return std::stof(str_); } catch (...) { return 0.0f; }
    }
    
    // Utility
    bool isEmpty() const { return str_.empty(); }
    unsigned int length() const { return static_cast<unsigned int>(str_.length()); }
    size_t size() const { return str_.size(); }  // Also add size() for std::string compatibility
    
    // Operators
    String& operator=(const String& other) { str_ = other.str_; return *this; }
    String& operator=(const char* s) { str_ = s ? s : ""; return *this; }
    String& operator+=(const String& other) { str_ += other.str_; return *this; }
    String& operator+=(const char* s) { str_ += (s ? s : ""); return *this; }
    String& operator+=(char c) { str_ += c; return *this; }
    String operator+(const String& other) const { return String(str_ + other.str_); }
    String operator+(const char* s) const { return String(str_ + (s ? s : "")); }
    friend String operator+(const char* s, const String& str) { return String((s ? s : "") + str.str_); }
    bool operator==(const String& other) const { return str_ == other.str_; }
    bool operator==(const char* s) const { return str_ == (s ? s : ""); }
    bool operator!=(const String& other) const { return str_ != other.str_; }
    bool operator!=(const char* s) const { return str_ != (s ? s : ""); }
    char operator[](int index) const { return str_[index]; }
    char& operator[](int index) { return str_[index]; }
    
    // String manipulation
    void toLowerCase() {
        std::transform(str_.begin(), str_.end(), str_.begin(), ::tolower);
    }
    void toUpperCase() {
        std::transform(str_.begin(), str_.end(), str_.begin(), ::toupper);
    }
    
    // Access to underlying string
    const std::string& getString() const { return str_; }
    
private:
    std::string str_;
};

typedef uint8_t byte;

// Mock time for millis() - can be controlled in tests
// Use inline variable (C++17) to avoid ODR violations
inline unsigned long g_test_millis = 0;

// Arduino functions
inline unsigned long millis() {
    return g_test_millis;
}

inline unsigned long micros() {
    return g_test_millis * 1000;  // Approximate - micros is milliseconds * 1000
}

inline void delay(unsigned long ms) {
    // No-op in tests - time advancement must be done explicitly
    g_test_millis += ms;
}

inline void delayMicroseconds(unsigned int us) {
    // No-op in tests
}

// Arduino map function
inline long map(long x, long in_min, long in_max, long out_min, long out_max) {
    return (x - in_min) * (out_max - out_min) / (in_max - in_min) + out_min;
}

// Arduino stubs
inline void setup() {}
inline void loop() {}

// Serial stubs
class SerialClass {
public:
    void begin(unsigned long) {}
    void print(const char*) {}
    void print(int) {}
    void print(unsigned int) {}
    void print(double) {}
    void println(const char*) {}
    void println(int) {}
    void println(unsigned int) {}
    void println(double) {}
    void flush() {}
    size_t write(const uint8_t*, size_t len) { return len; }
    int available() { return 0; }
    int read() { return -1; }
    operator bool() const { return true; }  // Contextually convertible to bool (required by Logger.cpp)
};
inline SerialClass Serial;

// IPAddress stub (must be before WiFiClass)
class IPAddress {
public:
    IPAddress() : ip(0) {}
    IPAddress(uint8_t a, uint8_t b, uint8_t c, uint8_t d) : ip((a<<24)|(b<<16)|(c<<8)|d) {}
    uint32_t ip;
};

// WiFi status enum (must be defined before WiFiClass for Logger.cpp)
enum {
    WL_CONNECTED = 3,
    WL_DISCONNECTED = 6,
    WL_CONNECT_FAILED = 4
};

// WiFiClient stub (forward declare for WiFiClass)
class WiFiClient {
public:
    WiFiClient() {}
    int connect(const char*, uint16_t) { return 0; }
    int connected() { return 0; }
    void stop() {}
    void flush() {}
    void setNoDelay(bool) {}
    int availableForWrite() { return 512; }
    size_t write(const uint8_t*, size_t) { return 0; }
    size_t write(const char* str) { return str ? strlen(str) : 0; }  // Overload for const char*
    int available() { return 0; }
    int read() { return -1; }
    operator bool() const { return false; }  // Contextually convertible to bool
};

// WiFiServer stub
class WiFiServer {
public:
    WiFiServer(uint16_t) {}
    void begin() {}
    WiFiClient available() { return WiFiClient(); }
    bool hasClient() { return false; }  // Required by Logger.cpp
};

// WiFi.h stub
class WiFiClass {
public:
    typedef enum {
        WL_CONNECTED = 3,
        WL_DISCONNECTED = 6,
        WL_CONNECT_FAILED = 4
    } wl_status_t;
    
    wl_status_t status() { return WL_DISCONNECTED; }
    String SSID() { return ""; }
    IPAddress localIP() { return IPAddress(0,0,0,0); }
};
inline WiFiClass WiFi;

// ESP stub for ESP32 chip functions
class ESPClass {
public:
    void restart() {}
};
inline ESPClass ESP;

// ESP32 hardware timer stub
typedef struct {
    void* timer;
} hw_timer_t;

// ESP32 Preferences (NVS) stub
class Preferences {
public:
    static void resetTestStore() {
        intStore_.clear();
        activeNamespace_.clear();
    }

    bool begin(const char* name, bool = false) {
        activeNamespace_ = name;
        return true;
    }
    void end() {
        activeNamespace_.clear();
    }
    bool clear() { return true; }
    size_t putString(const char*, const String&) { return 0; }
    String getString(const char*, const String& = "") { return ""; }
    size_t putInt(const char* key, int32_t value) {
        if (activeNamespace_.empty()) {
            return 0;
        }
        intStore_[activeNamespace_ + ":" + key] = value;
        return sizeof(int32_t);
    }
    int32_t getInt(const char* key, int32_t defaultValue = 0) {
        if (activeNamespace_.empty()) {
            return defaultValue;
        }
        const std::string fullKey = activeNamespace_ + ":" + key;
        const auto        it      = intStore_.find(fullKey);
        return it != intStore_.end() ? it->second : defaultValue;
    }
    size_t putFloat(const char*, float) { return 0; }
    float getFloat(const char*, float = 0.0) { return 0.0; }
    size_t putDouble(const char*, double) { return sizeof(double); }
    double getDouble(const char*, double = 0.0) { return 0.0; }
    size_t putBool(const char*, bool) { return sizeof(bool); }
    bool getBool(const char*, bool = false) { return false; }
    size_t putBytes(const char*, const void*, size_t) { return 0; }
    size_t getBytes(const char*, void*, size_t) { return 0; }
    bool remove(const char*) { return true; }
    bool isKey(const char*) { return false; }
    void removeAll() {}

private:
    static inline std::map<std::string, int32_t> intStore_;
    static inline std::string                    activeNamespace_;
};
inline Preferences preferences;
