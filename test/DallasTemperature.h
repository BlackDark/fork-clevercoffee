/**
 * @file DallasTemperature.h
 * @brief DallasTemperature stub for native test environment
 */

#pragma once

#include "OneWire.h"

// Minimal stub for DallasTemperature library
class DallasTemperature {
public:
    DallasTemperature(void* oneWire) {}
    void begin() {}
    float getTempC(void* deviceAddress) { return 0.0f; }
    void requestTemperatures() {}
    bool isConversionComplete() { return true; }
};
