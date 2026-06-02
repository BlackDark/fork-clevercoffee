/**
 * @file MockHardwareManager.h
 * @brief GMock implementation for IHardwareContext interface
 *
 * Allows testing code that depends on hardware access without actual hardware.
 * Implements IHardwareContext interface using GoogleMock.
 */

#pragma once

#include <gmock/gmock.h>
#include "clevercoffee/state/IHardwareContext.h"
#include "clevercoffee/hardware/Relay.h"
#include "clevercoffee/hardware/Switch.h"
#include "clevercoffee/hardware/LED.h"
#include "clevercoffee/hardware/tempsensors/TempSensor.h"
#include <memory>

namespace CleverCoffee {

/**
 * @class MockHardwareManager
 * @brief Mock implementation of IHardwareContext for testing
 *
 * Provides EXPECT_CALL() support for all hardware operations.
 * 
 * Example usage:
 * @code
 * MockHardwareManager mockHw;
 * MockRelay mockHeaterRelay(gpioPin);
 * 
 * EXPECT_CALL(mockHw, getHeaterRelay()).WillRepeatedly(Return(&mockHeaterRelay));
 * EXPECT_CALL(mockHw, getCurrentTemperature()).WillRepeatedly(Return(95.5));
 * EXPECT_CALL(mockHw, enableHeater()).Times(1);
 * @endcode
 */
class MockHardwareManager : public IHardwareContext {
public:
    MockHardwareManager() = default;
    virtual ~MockHardwareManager() = default;

    // Temperature Sensor
    MOCK_METHOD(TempSensor*, getTempSensor, (), (noexcept, override));
    MOCK_METHOD(const TempSensor*, getTempSensor, (), (const, noexcept, override));
    MOCK_METHOD(double, getCurrentTemperature, (), (const, noexcept, override));
    MOCK_METHOD(bool, hasTemperatureError, (), (const, noexcept, override));

    // Water Tank Sensor
    MOCK_METHOD(Switch*, getWaterTankSensor, (), (noexcept, override));
    MOCK_METHOD(const Switch*, getWaterTankSensor, (), (const, noexcept, override));
    MOCK_METHOD(bool, isWaterTankEmpty, (), (const, noexcept, override));

    // Relay Control
    MOCK_METHOD(Relay*, getHeaterRelay, (), (noexcept, override));
    MOCK_METHOD(Relay*, getPumpRelay, (), (noexcept, override));
    MOCK_METHOD(Relay*, getValveRelay, (), (noexcept, override));

    // LED Indicators
    MOCK_METHOD(LED*, getStatusLed, (), (noexcept, override));
    MOCK_METHOD(const LED*, getStatusLed, (), (const, noexcept, override));

    // Scale Operations
    MOCK_METHOD(double, getWeight, (), (const, noexcept, override));
    MOCK_METHOD(void, tareScale, (), (noexcept, override));

    // Hardware Operations
    MOCK_METHOD(void, updateHardware, (), (noexcept, override));

    // High-Level Hardware Control
    MOCK_METHOD(void, enableHeater, (), (noexcept, override));
    MOCK_METHOD(void, disableHeater, (), (noexcept, override));
    MOCK_METHOD(void, setHeaterPower, (uint8_t), (noexcept, override));
    
    MOCK_METHOD(void, enablePump, (), (noexcept, override));
    MOCK_METHOD(void, disablePump, (), (noexcept, override));
    MOCK_METHOD(void, setPumpPressure, (float), (noexcept, override));
    
    MOCK_METHOD(void, openSteamValve, (), (noexcept, override));
    MOCK_METHOD(void, closeSteamValve, (), (noexcept, override));
    MOCK_METHOD(void, openWaterValve, (), (noexcept, override));
    MOCK_METHOD(void, closeWaterValve, (), (noexcept, override));
    
    MOCK_METHOD(void, openSolenoid, (), (noexcept, override));
    MOCK_METHOD(void, closeSolenoid, (), (noexcept, override));
    
    MOCK_METHOD(void, emergencyShutdown, (), (noexcept, override));
    MOCK_METHOD(void, safeHardwareShutdown, (), (noexcept, override));
    MOCK_METHOD(void, clearEmergencyMode, (), (noexcept, override));
};

} // namespace CleverCoffee
