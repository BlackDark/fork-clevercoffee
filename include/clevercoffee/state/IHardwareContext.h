/**
 * @file IHardwareContext.h
 * @brief Interface for hardware access in states
 *
 * Breaks circular dependency between State and Context.
 * States depend only on this interface, not concrete context.
 */

#pragma once

// Forward declarations
class TempSensor;
class Switch;
class Relay;
class LED;

namespace CleverCoffee {

/**
 * @brief Interface for hardware access in states
 *
 * Provides abstraction layer for hardware components, breaking
 * circular dependencies and improving testability.
 */
class IHardwareContext {
public:
    virtual ~IHardwareContext() = default;

    // Temperature sensor
    virtual TempSensor* getTempSensor() noexcept = 0;
    virtual const TempSensor* getTempSensor() const noexcept = 0;
    virtual double getCurrentTemperature() const noexcept = 0;
    virtual bool hasTemperatureError() const noexcept = 0;

    // Water tank sensor
    virtual Switch* getWaterTankSensor() noexcept = 0;
    virtual const Switch* getWaterTankSensor() const noexcept = 0;
    virtual bool isWaterTankEmpty() const noexcept = 0;

    // Relays
    virtual Relay* getHeaterRelay() noexcept = 0;
    virtual Relay* getPumpRelay() noexcept = 0;
    virtual Relay* getValveRelay() noexcept = 0;

    // LEDs
    virtual LED* getStatusLed() noexcept = 0;
    virtual const LED* getStatusLed() const noexcept = 0;

    // Scale
    virtual double getWeight() const noexcept = 0;
    virtual void tareScale() noexcept = 0;

    // Hardware operations
    virtual void updateHardware() noexcept = 0;
};

} // namespace CleverCoffee
