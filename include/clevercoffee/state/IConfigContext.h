/**
 * @file IConfigContext.h
 * @brief Interface for config access in states
 *
 * Provides clean interface to configuration parameters,
 * reducing coupling between states and concrete configuration.
 */

#pragma once

#include "clevercoffee/Config.h"

namespace CleverCoffee {

/**
 * @brief Interface for config access in states
 *
 * Abstracts configuration access, making states independent
 * of concrete configuration implementation.
 */
class IConfigContext {
public:
    virtual ~IConfigContext() = default;

    // Temperature settings
    virtual double getBrewSetpoint() const noexcept = 0;
    virtual double getSteamSetpoint() const noexcept = 0;

    // Time settings
    virtual double getTargetBrewTime() const noexcept = 0;
    virtual double getPreInfusionTime() const noexcept = 0;

    // PID settings
    virtual double getPidKp() const noexcept = 0;
    virtual double getPidTn() const noexcept = 0;
    virtual double getPidTv() const noexcept = 0;

    // Access to full config
    virtual Config& getConfig() noexcept = 0;
    virtual const Config& getConfig() const noexcept = 0;
};

} // namespace CleverCoffee
