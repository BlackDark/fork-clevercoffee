/**
 * @file Wire.h
 * @brief Wire (I2C) stub for native test environment
 */

#pragma once

#include <cstdint>

// Minimal stub for Wire library (I2C)
class TwoWire {
public:
    void begin() {}
    void begin(int sda, int scl) {}
    void beginTransmission(uint8_t address) {}
    uint8_t endTransmission() { return 0; }
    uint8_t requestFrom(uint8_t address, uint8_t quantity) { return 0; }
    size_t write(uint8_t data) { return 1; }
    size_t write(const uint8_t* data, size_t length) { return length; }
    int available() { return 0; }
    int read() { return 0; }
};

inline TwoWire Wire;
