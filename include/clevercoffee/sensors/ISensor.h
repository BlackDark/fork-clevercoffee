/**
 * @file ISensor.h
 * @brief Unified sensor interface for all sensor types
 */

#pragma once

#include "clevercoffee/errors/ErrorCodes.h"
#include "clevercoffee/errors/Expected.h"

namespace CleverCoffee {

/**
 * @class ISensor
 * @brief Abstract interface for all sensors
 *
 * All sensors (temperature, scale, pressure, etc.) implement this interface.
 * Sensors follow an async read pattern:
 * 1. startRead() - initiate read (non-blocking)
 * 2. tryGetValue() - poll for result (non-blocking)
 *
 * Returns Expected<double, Error> to handle both success and failure cases.
 * Timeouts are handled internally by each sensor.
 */
class ISensor {
  public:
    virtual ~ISensor() = default;

    /**
     * @brief Start an async sensor read
     *
     * This is non-blocking and just initiates the read.
     * Call tryGetValue() to get the result.
     */
    virtual void startRead() noexcept = 0;

    /**
     * @brief Try to get the sensor reading result
     *
     * Non-blocking. Returns:
     * - Success with value if read complete
     * - Error with NOT_READY if still reading
     * - Error with TIMEOUT if read took too long
     * - Error with other codes for hardware faults
     *
     * @return Expected<double, Error> containing value or error
     */
    virtual Expected<double, Error> tryGetValue() noexcept = 0;

    /**
     * @brief Get the sensor type name for logging
     * @return Human-readable sensor type (e.g., "TempSensorDallas")
     */
    virtual const char* getSensorType() const noexcept = 0;

    /**
     * @brief Check if sensor is connected/operational
     * @return true if sensor is connected and responding
     */
    virtual bool isConnected() const noexcept {
        return true;
    }

    /**
     * @brief Request a tare/zero operation when supported by the sensor
     */
    virtual void requestTare() noexcept {}
};

} // namespace CleverCoffee
