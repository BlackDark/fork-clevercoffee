/**
 * @file Errors.h
 * @brief Error types and error handling utilities
 *
 * This file defines error codes and the Error class used throughout
 * the system for type-safe error handling.
 */

#pragma once

#include <string>

namespace CleverCoffee {

/**
 * @brief Error code enumeration
 *
 * Defines all possible error conditions in the system. Using an enum
 * instead of integers provides type safety and better error messages.
 *
 * Usage with Expected<T, Error>:
 * @code
 * Expected<int, Error> configureSensor() {
 *     if (!sensorDetected) {
 *         return Error(ErrorCode::SENSOR_READ_FAILED,
 *                      "No sensor detected on I2C bus");
 *     }
 *     return 0; // Success code
 * }
 * @endcode
 *
 * Error handling pattern:
 * @code
 * auto result = operation();
 * if (!result) {
 *     switch (result.error().code()) {
 *         case ErrorCode::SENSOR_READ_FAILED:
 *             // Handle sensor error
 *             break;
 *         case ErrorCode::NETWORK_ERROR:
 *             // Handle network error
 *             break;
 *         default:
 *             // Handle unknown error
 *     }
 * }
 * @endcode
 */
enum class ErrorCode {
    SUCCESS = 0,              ///< Operation completed successfully
    SENSOR_READ_FAILED,       ///< Failed to read from sensor
    SENSOR_TIMEOUT,           ///< Sensor communication timeout
    INVALID_CONFIG,           ///< Invalid configuration value
    INITIALIZATION_FAILED,    ///< Component initialization failed
    HARDWARE_ERROR,           ///< Generic hardware failure
    NETWORK_ERROR,            ///< Network operation failed
};

/**
 * @brief Error representation with code and message
 *
 * This class encapsulates an error condition with both a type-safe
 * error code and a human-readable message. It's designed to work
 * with the Expected<T, Error> type for error handling.
 *
 * Design Benefits:
 * - Type-safe error codes (not just integers)
 * - Human-readable messages for debugging/logging
 * - Lightweight (copyable, no dynamic allocation for message if using string_view)
 * - Works with Expected<T> for explicit error handling
 *
 * Example usage:
 * @code
 * Error err(ErrorCode::SENSOR_READ_FAILED,
 *           "Temperature sensor not responding");
 *
 * // Check error type
 * if (err.code() == ErrorCode::SENSOR_READ_FAILED) {
 *     // Handle sensor error
 * }
 *
 * // Log error message
 * Serial.println(err.message().c_str());
 *
 * // Return from function
 * Expected<double, Error> readSensor() {
 *     return Error(ErrorCode::SENSOR_TIMEOUT,
 *                  "Sensor read timeout after 1000ms");
 * }
 * @endcode
 *
 * @note For memory-constrained environments, consider using const char*
 *       or FlashStringHelper (Arduino F() macro) instead of std::string
 *       for error messages to keep strings in PROGMEM.
 */
class Error {
public:
    /**
     * @brief Construct an Error with code and message
     *
     * @param code The error code from ErrorCode enum
     * @param message Human-readable error description
     */
    Error(ErrorCode code, std::string message)
        : code_(code), message_(std::move(message)) {}

    /**
     * @brief Get the error code
     *
     * @return The ErrorCode enum value
     */
    ErrorCode code() const noexcept { return code_; }

    /**
     * @brief Get the error message
     *
     * @return Const reference to the human-readable error message
     */
    const std::string& message() const noexcept { return message_; }

private:
    ErrorCode code_;          ///< Type-safe error code
    std::string message_;     ///< Human-readable error description
};

} // namespace CleverCoffee
