/**
 * @file IHardwareContext.h
 * @brief Interface for hardware access in states
 *
 * This interface breaks the circular dependency between State and Context
 * by providing an abstraction layer. States depend only on this interface,
 * not on the concrete context implementation.
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
 * This interface provides an abstraction layer for hardware components, serving
 * two key purposes:
 *
 * 1. Breaking circular dependencies: States can depend on this interface without
 *    needing the full concrete context definition, which itself depends on states.
 *
 * 2. Improving testability: Mock implementations can be provided for testing
 *    without requiring actual hardware.
 *
 * Design Pattern: Dependency Inversion Principle
 * - High-level modules (states) depend on abstractions (this interface)
 * - Low-level modules (concrete hardware) implement these abstractions
 *
 * Example usage in a state:
 * @code
 * class BrewState : public MachineState {
 * public:
 *     void execute(IHardwareContext& hw) override {
 *         if (hw.isWaterTankEmpty()) {
 *             // Handle empty tank
 *             return;
 *         }
 *
 *         hw.getHeaterRelay()->turnOn();
 *         hw.getPumpRelay()->turnOn();
 *
 *         double temp = hw.getCurrentTemperature();
 *         // ... use temperature ...
 *     }
 * };
 * @endcode
 *
 * Example mock for testing:
 * @code
 * class MockHardwareContext : public IHardwareContext {
 * public:
 *     double getCurrentTemperature() const noexcept override {
 *         return 95.0; // Return test value
 *     }
 *     // ... implement other methods ...
 * };
 * @endcode
 */
class IHardwareContext {
public:
    virtual ~IHardwareContext() = default;

    /**
     * @name Temperature Sensor
     * @{
     */

    /**
     * @brief Get the temperature sensor instance
     *
     * @return Pointer to the temperature sensor, or nullptr if not available
     */
    virtual TempSensor* getTempSensor() noexcept = 0;

    /**
     * @brief Get the temperature sensor instance (const overload)
     *
     * @return Const pointer to the temperature sensor, or nullptr if not available
     */
    virtual const TempSensor* getTempSensor() const noexcept = 0;

    /**
     * @brief Get current temperature reading
     *
     * @return Current temperature in degrees Celsius
     */
    virtual double getCurrentTemperature() const noexcept = 0;

    /**
     * @brief Check if temperature sensor has an error
     *
     * @return true if temperature sensor is in error state, false otherwise
     */
    virtual bool hasTemperatureError() const noexcept = 0;

    /** @} */

    /**
     * @name Water Tank Sensor
     * @{
     */

    /**
     * @brief Get the water tank sensor switch
     *
     * @return Pointer to the water tank sensor, or nullptr if not available
     */
    virtual Switch* getWaterTankSensor() noexcept = 0;

    /**
     * @brief Get the water tank sensor switch (const overload)
     *
     * @return Const pointer to the water tank sensor, or nullptr if not available
     */
    virtual const Switch* getWaterTankSensor() const noexcept = 0;

    /**
     * @brief Check if water tank is empty
     *
     * @return true if water tank is empty, false if water is available
     */
    virtual bool isWaterTankEmpty() const noexcept = 0;

    /** @} */

    /**
     * @name Relay Control
     * @{
     */

    /**
     * @brief Get heater relay
     *
     * @return Pointer to the heater relay, or nullptr if not available
     */
    virtual Relay* getHeaterRelay() noexcept = 0;

    /**
     * @brief Get pump relay
     *
     * @return Pointer to the pump relay, or nullptr if not available
     */
    virtual Relay* getPumpRelay() noexcept = 0;

    /**
     * @brief Get valve relay
     *
     * @return Pointer to the valve relay, or nullptr if not available
     */
    virtual Relay* getValveRelay() noexcept = 0;

    /** @} */

    /**
     * @name LED Indicators
     * @{
     */

    /**
     * @brief Get status LED
     *
     * @return Pointer to the status LED, or nullptr if not available
     */
    virtual LED* getStatusLed() noexcept = 0;

    /**
     * @brief Get status LED (const overload)
     *
     * @return Const pointer to the status LED, or nullptr if not available
     */
    virtual const LED* getStatusLed() const noexcept = 0;

    /** @} */

    /**
     * @name Scale Operations
     * @{
     */

    /**
     * @brief Get current weight reading
     *
     * @return Current weight in grams
     */
    virtual double getWeight() const noexcept = 0;

    /**
     * @brief Tare the scale (reset to zero)
     *
     * Resets the scale to read zero with current load as baseline.
     */
    virtual void tareScale() noexcept = 0;

    /** @} */

    /**
     * @name Hardware Operations
     * @{
     */

    /**
     * @brief Update all hardware components
     *
     * Should be called periodically to update hardware state.
     * This may include reading sensors, updating outputs, etc.
     */
    virtual void updateHardware() noexcept = 0;

    /** @} */
};

} // namespace CleverCoffee
