/**
 * @file ZACwire.h
 * @brief ZACwire stub for native test environment
 */

#pragma once

// Minimal ZACwire stub
class ZACwire {
public:
    ZACwire(int pin) {}
    bool begin() { return true; }
    float getTemp() { return 0.0f; }
    bool available() { return true; }
};
