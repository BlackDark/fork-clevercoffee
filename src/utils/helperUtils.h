/**
 * @file helperUtils.h
 * @brief Common utility functions used across the application
 */

#pragma once

#include <Arduino.h>
#include <cmath>
#include "../state/GlobalState.h"

// ==================== UTILITY FUNCTIONS ====================

/**
 * @brief Flip uint8_t value between 0 and 1
 * @param value The value to flip
 * @return 1 if value is 0, otherwise 0
 */
inline uint8_t flipUintValue(const uint8_t value) {
    return value == 0 ? 1 : 0;
}

/**
 * @brief Modulo operation that handles negative numbers correctly
 * @param a The dividend
 * @param b The divisor
 * @return The modulo result
 */
inline int mod(const int a, const int b) {
    const int r = a % b;
    return r < 0 ? r + b : r;
}

/**
 * @brief Round a double value to 2 decimal places
 * @param value The value to round
 * @return The rounded value
 */
inline double round2(const double value) {
    return std::round(value * 100.0) / 100.0;
}

/**
 * @brief Validate if a string represents a valid number
 * @param str The string to validate
 * @return true if the string is a valid number, false otherwise
 */
inline bool isValidNumber(const String& str) {
    if (str.length() == 0) return false;

    for (const auto& ch : str) {
        if (!isdigit(ch) && ch != '.' && ch != '-') {
            return false;
        }
    }
    return true;
}

inline char number2string_double[22];

inline char* number2string(const double in) {
    snprintf(number2string_double, sizeof(number2string_double), "%0.2f", in);

    return number2string_double;
}

inline char number2string_float[22];

inline char* number2string(const float in) {
    snprintf(number2string_float, sizeof(number2string_float), "%0.2f", in);

    return number2string_float;
}

inline char number2string_int[22];

inline char* number2string(const int in) {
    snprintf(number2string_int, sizeof(number2string_int), "%d", in);

    return number2string_int;
}

inline char number2string_uint[22];

inline char* number2string(const unsigned int in) {
    snprintf(number2string_uint, sizeof(number2string_uint), "%u", in);

    return number2string_uint;
}

/**
 * @brief Filter input value using exponential moving average filter (using fixed coefficients)
 *      After ~28 cycles the input is set to 99,66% if the real input value sum of inX and inY
 *      multiplier must be 1 increase inX multiplier to make the filter faster
 */
inline float filterPressureValue(const float input) {
    g_state.sensors.inX = static_cast<float>(input * 0.3f);
    g_state.sensors.inY = static_cast<float>(g_state.sensors.inOld * 0.7f);
    g_state.sensors.inSum = g_state.sensors.inX + g_state.sensors.inY;
    g_state.sensors.inOld = g_state.sensors.inSum;

    return g_state.sensors.inSum;
}
