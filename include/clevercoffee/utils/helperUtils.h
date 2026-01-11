/**
 * @file helperUtils.h
 * @brief Common utility functions used across the application
 */

#pragma once

#include "clevercoffee/types/GlobalTypes.h"

#include <Arduino.h>
#include <cmath>

// ==================== UTILITY FUNCTIONS ====================

/**
 * @brief Modulo operation that handles negative numbers correctly
 * @param a The dividend
 * @param b The divisor
 * @return The modulo result
 */
constexpr int mod(const int a, const int b) noexcept {
    const int r = a % b;
    return r < 0 ? r + b : r;
}

/**
 * @brief Round a double value to 2 decimal places
 * @param value The value to round
 * @return The rounded value
 * @note Cannot be constexpr due to std::round() not being constexpr in C++11/14
 */
inline double round2(const double value) noexcept {
    return std::round(value * 100.0) / 100.0;
}

// Thread-safe number to string conversion functions using thread_local storage
namespace {
thread_local char number2string_double[22];
thread_local char number2string_float[22];
thread_local char number2string_int[22];
thread_local char number2string_uint[22];
} // namespace

inline char* number2string(const double in) {
    snprintf(number2string_double, sizeof(number2string_double), "%0.2f", in);
    return number2string_double;
}

inline char* number2string(const float in) {
    snprintf(number2string_float, sizeof(number2string_float), "%0.2f", in);
    return number2string_float;
}

inline char* number2string(const int in) {
    snprintf(number2string_int, sizeof(number2string_int), "%d", in);
    return number2string_int;
}

inline char* number2string(const unsigned int in) {
    snprintf(number2string_uint, sizeof(number2string_uint), "%u", in);
    return number2string_uint;
}
