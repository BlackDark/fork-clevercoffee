/**
 * @file IConfigContext.h
 * @brief Interface for config access in states
 *
 * This interface provides a clean abstraction to configuration parameters,
 * reducing coupling between states and the concrete configuration implementation.
 */

#pragma once

#include "clevercoffee/Config.h"

namespace CleverCoffee {

/**
 * @brief Interface for configuration access in states
 *
 * This interface abstracts configuration access, making states independent
 * of the concrete Config class implementation. This provides several benefits:
 *
 * 1. Reduced coupling: States don't need to know Config internals
 * 2. Improved testability: Easy to provide test configurations
 * 3. Flexibility: Config implementation can change without affecting states
 *
 * Design Pattern: Interface Segregation
 * - States depend only on the configuration methods they actually use
 * - Additional config access can be provided through getConfig() if needed
 *
 * Example usage in a state:
 * @code
 * class HeatState : public MachineState {
 * public:
 *     void execute(IConfigContext& config, IHardwareContext& hw) override {
 *         double setpoint = config.getBrewSetpoint();
 *         double current = hw.getCurrentTemperature();
 *
 *         if (current < setpoint) {
 *             hw.enableHeater();
 *         }
 *     }
 * };
 * @endcode
 *
 * Example for testing:
 * @code
 * class TestConfigContext : public IConfigContext {
 * public:
 *     double getBrewSetpoint() const noexcept override {
 *         return 93.0; // Test value
 *     }
 *     // ... implement other methods ...
 * };
 * @endcode
 */
class IConfigContext {
  public:
    virtual ~IConfigContext() = default;

    /**
     * @name Temperature Settings
     * @{
     */

    /**
     * @brief Get brew temperature setpoint
     *
     * Returns the target temperature for brewing coffee.
     *
     * @return Brew setpoint temperature in degrees Celsius
     */
    virtual double getBrewSetpoint() const noexcept = 0;

    /**
     * @brief Get steam temperature setpoint
     *
     * Returns the target temperature for steam mode.
     *
     * @return Steam setpoint temperature in degrees Celsius
     */
    virtual double getSteamSetpoint() const noexcept = 0;

    /** @} */

    /**
     * @name Time Settings
     * @{
     */

    /**
     * @brief Get target brew time
     *
     * Returns the desired duration for the brewing phase.
     *
     * @return Target brew time in seconds
     */
    virtual double getTargetBrewTime() const noexcept = 0;

    /**
     * @brief Get pre-infusion time
     *
     * Returns the duration for the pre-infusion phase (low pressure
     * water injection before full brewing).
     *
     * @return Pre-infusion time in seconds
     */
    virtual double getPreInfusionTime() const noexcept = 0;

    /** @} */

    /**
     * @name PID Controller Settings
     * @{
     */

    /**
     * @brief Get PID proportional gain (Kp)
     *
     * @return Proportional gain constant
     */
    virtual double getPidKp() const noexcept = 0;

    /**
     * @brief Get PID integral time (Tn)
     *
     * @return Integral time constant
     */
    virtual double getPidTn() const noexcept = 0;

    /**
     * @brief Get PID derivative time (Tv)
     *
     * @return Derivative time constant
     */
    virtual double getPidTv() const noexcept = 0;

    /** @} */

    /**
     * @name Full Configuration Access
     * @{
     */

    /**
     * @brief Get the full configuration object
     *
     * Provides access to the complete Config object for advanced
     * configuration parameters not exposed through specific methods.
     *
     * @return Reference to the configuration object
     */
    virtual Config& getConfig() noexcept = 0;

    /**
     * @brief Get the full configuration object (const overload)
     *
     * @return Const reference to the configuration object
     */
    virtual const Config& getConfig() const noexcept = 0;

    /** @} */
};

} // namespace CleverCoffee
