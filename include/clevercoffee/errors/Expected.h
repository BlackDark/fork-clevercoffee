/**
 * @file Expected.h
 * @brief Expected type for error handling
 *
 * This file provides a simple Expected type for error handling,
 * similar to std::expected (C++23) but compatible with ESP32.
 */

#pragma once

#include <variant>
#include <string>

namespace CleverCoffee {

/**
 * @brief Simple Expected type for error handling
 *
 * This class provides a type-safe error handling mechanism, similar to
 * Rust's Result<T, E> or C++23's std::expected. Since ESP32/Arduino
 * doesn't yet support std::expected, this provides a similar interface
 * using std::variant.
 *
 * The Expected type represents either a successful value of type T or
 * an error of type E. This encourages explicit error handling and makes
 * error states visible in the type system.
 *
 * Design Pattern: Result Type / Monad
 * - Forces explicit handling of error cases
 * - Prevents use of invalid values
 * - Makes control flow clearer than exceptions or error codes
 *
 * Why use Expected instead of exceptions?
 * - ESP32 has limited exception support
 * - Zero overhead compared to exception handling
 * - Explicit error handling in type signature
 * - Better for embedded systems
 *
 * Example usage:
 * @code
 * Expected<double, Error> readTemperature() {
 *     if (sensorFailed) {
 *         return Error(ErrorCode::SENSOR_READ_FAILED, "Sensor not responding");
 *     }
 *     return 95.0; // Success
 * }
 *
 * // Usage at call site
 * auto result = readTemperature();
 * if (result) {
 *     double temp = result.value();
 *     Serial.println(temp);
 * } else {
 *     Error err = result.error();
 *     Serial.println(err.message().c_str());
 * }
 * @endcode
 *
 * @tparam T The success value type
 * @tparam E The error type (defaults to std::string)
 *
 * @note This is a simplified version compared to std::expected.
 *       It lacks monadic operations like and_then, or_else, transform, etc.
 *       These can be added if needed.
 */
template<typename T, typename E = std::string>
class Expected {
public:
    /**
     * @brief Construct with a success value
     *
     * Creates an Expected containing a successful result.
     *
     * @param value The success value to store
     *
     * @post hasValue() returns true
     */
    Expected(T value) : data_(std::move(value)) {}

    /**
     * @brief Construct with an error value
     *
     * Creates an Expected containing an error.
     *
     * @param error The error value to store
     *
     * @post hasValue() returns false
     */
    Expected(E error) : data_(std::move(error)) {}

    /**
     * @brief Check if this contains a value (success)
     *
     * @return true if containing a success value, false if containing an error
     */
    bool hasValue() const noexcept {
        return std::holds_alternative<T>(data_);
    }

    /**
     * @brief Get the success value (const ref)
     *
     * @pre hasValue() must be true (undefined behavior otherwise)
     * @return Const reference to the success value
     *
     * @note Consider checking hasValue() before calling this method
     */
    const T& value() const& {
        return std::get<T>(data_);
    }

    /**
     * @brief Get the success value (non-const ref)
     *
     * @pre hasValue() must be true (undefined behavior otherwise)
     * @return Reference to the success value
     *
     * @note Consider checking hasValue() before calling this method
     */
    T& value() & {
        return std::get<T>(data_);
    }

    /**
     * @brief Get the error value (const ref)
     *
     * @pre hasValue() must be false (undefined behavior otherwise)
     * @return Const reference to the error value
     *
     * @note Consider checking !hasValue() before calling this method
     */
    const E& error() const& {
        return std::get<E>(data_);
    }

    /**
     * @brief Boolean conversion operator
     *
     * Allows convenient checking in conditionals:
     * @code
     * if (result) {
     *     // Handle success
     * } else {
     *     // Handle error
     * }
     * @endcode
     *
     * @return true if containing a value, false if containing an error
     */
    explicit operator bool() const noexcept {
        return hasValue();
    }

private:
    std::variant<T, E> data_; ///< Holds either success value or error
};

} // namespace CleverCoffee
