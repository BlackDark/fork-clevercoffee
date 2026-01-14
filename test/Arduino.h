/**
 * @file Arduino.h
 * @brief Arduino.h stub for native test environment
 * 
 * This file provides Arduino framework compatibility for PlatformIO native tests.
 * It should be included in the build path before the real Arduino.h
 */

#pragma once

#ifndef ARDUINO
// Only provide stubs if not compiling for real Arduino
#include <cstdint>
#include <string>
#include <cstring>
#include <cstdlib>
#include <algorithm>
#include <cctype>
#include <cstdio>

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
    int length() const { return static_cast<int>(str_.length()); }
    
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

// ESP32 hardware timer stub
typedef struct {
    void* timer;
} hw_timer_t;

// ESP32 Preferences (NVS) stub
class Preferences {
public:
    bool begin(const char*, bool = false) { return true; }
    void end() {}
    bool clear() { return true; }
    size_t putString(const char*, const String&) { return 0; }
    String getString(const char*, const String& = "") { return ""; }
    size_t putInt(const char*, int32_t) { return 0; }
    int32_t getInt(const char*, int32_t = 0) { return 0; }
    size_t putFloat(const char*, float) { return 0; }
    float getFloat(const char*, float = 0.0) { return 0.0; }
    size_t putDouble(const char*, double) { return sizeof(double); }  // Return success (non-zero)
    double getDouble(const char*, double = 0.0) { return 0.0; }
    size_t putBool(const char*, bool) { return sizeof(bool); }  // Return success (non-zero)
    bool getBool(const char*, bool = false) { return false; }
    size_t putBytes(const char*, const void*, size_t) { return 0; }
    size_t getBytes(const char*, void*, size_t) { return 0; }
    bool remove(const char*) { return true; }
    bool isKey(const char*) { return false; }
    void removeAll() {}
};
inline Preferences preferences;

#endif // ARDUINO
