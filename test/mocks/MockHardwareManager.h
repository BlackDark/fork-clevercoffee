/**
 * @file MockHardwareManager.h
 * @brief Mock implementation of HardwareManager for testing
 * 
 * Provides a testable implementation that doesn't require actual hardware.
 */

#pragma once

#include "clevercoffee/hardware/HardwareManager.h"
#include "clevercoffee/state/IHardwareContext.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "../mocks/MockRelay.h"
#include "../mocks/MockLED.h"
#include "../mocks/MockSwitch.h"
#include <memory>

namespace CleverCoffee {

/**
 * @brief Mock HardwareManager for testing
 * 
 * This class provides a mock implementation of HardwareManager that can be used
 * in tests without requiring actual hardware initialization.
 * 
 * Usage:
 * @code
 * MockConfig mockConfig;
 * MockHardwareManager mockHwMgr(mockConfig);
 * 
 * // Set up mock behavior
 * mockHwMgr.setTemperature(95.5);
 * mockHwMgr.setWaterTankFull(true);
 * 
 * // Use in test
 * ProcessController controller(mockConfig, systemContext, mockHwMgr, ...);
 * @endcode
 */
class MockHardwareManager : public HardwareManager {
public:
    // Note: HardwareManager requires Config& in constructor
    // This mock would need to be created differently or we need a test-friendly constructor
    // For now, this is a placeholder showing the interface we'd want
    
    // Mock state
    double mockTemperature_ = 20.0;
    bool mockTemperatureError_ = false;
    bool mockWaterTankFull_ = true;
    double mockWeight_ = 0.0;
    bool mockHeaterEnabled_ = false;
    bool mockPumpEnabled_ = false;
    bool mockValveOpen_ = false;
    uint8_t mockHeaterPower_ = 0;
    float mockPumpPressure_ = 0.0f;
    
    // Setters for test control
    void setTemperature(double temp) { mockTemperature_ = temp; }
    void setTemperatureError(bool error) { mockTemperatureError_ = error; }
    void setWaterTankFull(bool full) { mockWaterTankFull_ = full; }
    void setWeight(double weight) { mockWeight_ = weight; }
    void setHeaterEnabled(bool enabled) { mockHeaterEnabled_ = enabled; }
    void setPumpEnabled(bool enabled) { mockPumpEnabled_ = enabled; }
    void setValveOpen(bool open) { mockValveOpen_ = open; }
    void setHeaterPower(uint8_t power) { mockHeaterPower_ = power; }
    void setPumpPressure(float pressure) { mockPumpPressure_ = pressure; }
    
    // Getters for verification
    bool isHeaterEnabled() const { return mockHeaterEnabled_; }
    bool isPumpEnabled() const { return mockPumpEnabled_; }
    bool isValveOpen() const { return mockValveOpen_; }
    uint8_t getHeaterPower() const { return mockHeaterPower_; }
    float getPumpPressure() const { return mockPumpPressure_; }
};

} // namespace CleverCoffee
