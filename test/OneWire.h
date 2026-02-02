/**
 * @file OneWire.h
 * @brief OneWire stub for native test environment
 */

#pragma once

#include <cstdint>

// Device address type for Dallas sensors
using DeviceAddress = uint8_t[8];

// Minimal stub for OneWire library
class OneWire {
public:
    explicit OneWire(uint8_t pin) {}
    void reset() {}
    void select(const uint8_t* address) {}
    void write(uint8_t data, uint8_t power = 0) {}
    uint8_t read() { return 0; }
    bool search(uint8_t* address) { return false; }
    void reset_search() {}
};
