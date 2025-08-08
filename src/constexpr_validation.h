/**
 * @file constexpr_validation.h
 * @brief C++23 compile-time validation for CleverCoffee configuration parameters
 *
 * This header provides constexpr validation functions that catch invalid
 * configuration values at compile time instead of runtime.
 */

#pragma once

#include "defaults.h"
#include <type_traits>

#if __has_include(<type_traits>) && defined(__cpp_consteval)  // C++20 consteval support

namespace CleverCoffee::Validation {

// ============================================================================
// TEMPERATURE VALIDATION
// ============================================================================

constexpr bool isValidBrewTemperature(double temp) noexcept {
    return temp >= BREW_SETPOINT_MIN && temp <= BREW_SETPOINT_MAX;
}

constexpr bool isValidSteamTemperature(double temp) noexcept {
    return temp >= STEAM_SETPOINT_MIN && temp <= STEAM_SETPOINT_MAX;
}

constexpr bool isValidTemperatureOffset(double offset) noexcept {
    return offset >= BREW_TEMP_OFFSET_MIN && offset <= BREW_TEMP_OFFSET_MAX;
}

// ============================================================================
// PID VALIDATION
// ============================================================================

constexpr bool isValidPidKp(double kp) noexcept {
    return kp >= PID_KP_REGULAR_MIN && kp <= PID_KP_REGULAR_MAX;
}

constexpr bool isValidPidTn(double tn) noexcept {
    return tn >= PID_TN_REGULAR_MIN && tn <= PID_TN_REGULAR_MAX;
}

constexpr bool isValidPidTv(double tv) noexcept {
    return tv >= PID_TV_REGULAR_MIN && tv <= PID_TV_REGULAR_MAX;
}

constexpr bool isValidPidIMax(double imax) noexcept {
    return imax >= PID_I_MAX_REGULAR_MIN && imax <= PID_I_MAX_REGULAR_MAX;
}

constexpr bool isValidPidEmaFactor(double ema) noexcept {
    return ema >= PID_EMA_FACTOR_MIN && ema <= PID_EMA_FACTOR_MAX;
}

// Brew detection PID validation
constexpr bool isValidPidBdKp(double kp) noexcept {
    return kp >= PID_KP_BD_MIN && kp <= PID_KP_BD_MAX;
}

constexpr bool isValidPidBdTn(double tn) noexcept {
    return tn >= PID_TN_BD_MIN && tn <= PID_TN_BD_MAX;
}

constexpr bool isValidPidBdTv(double tv) noexcept {
    return tv >= PID_TV_BD_MIN && tv <= PID_TV_BD_MAX;
}

// ============================================================================
// BREW CONTROL VALIDATION
// ============================================================================

constexpr bool isValidBrewTime(double time) noexcept {
    return time >= TARGET_BREW_TIME_MIN && time <= TARGET_BREW_TIME_MAX;
}

constexpr bool isValidBrewWeight(double weight) noexcept {
    return weight >= TARGET_BREW_WEIGHT_MIN && weight <= TARGET_BREW_WEIGHT_MAX;
}

constexpr bool isValidPreInfusionTime(double time) noexcept {
    return time >= PRE_INFUSION_TIME_MIN && time <= PRE_INFUSION_TIME_MAX;
}

constexpr bool isValidPreInfusionPause(double pause) noexcept {
    return pause >= PRE_INFUSION_PAUSE_MIN && pause <= PRE_INFUSION_PAUSE_MAX;
}

constexpr bool isValidBrewPidDelay(double delay) noexcept {
    return delay >= BREW_PID_DELAY_MIN && delay <= BREW_PID_DELAY_MAX;
}

// ============================================================================
// SCALE VALIDATION
// ============================================================================

constexpr bool isValidScaleCalibration(double calibration) noexcept {
    return calibration >= SCALE_CALIBRATION_MIN && calibration <= SCALE_CALIBRATION_MAX;
}

constexpr bool isValidScaleKnownWeight(double weight) noexcept {
    return weight >= SCALE_KNOWN_WEIGHT_MIN && weight <= SCALE_KNOWN_WEIGHT_MAX;
}

constexpr bool isValidScaleSamples(int samples) noexcept {
    return samples >= SCALE_SAMPLES_MIN && samples <= SCALE_SAMPLES_MAX;
}

// ============================================================================
// BACKFLUSH VALIDATION
// ============================================================================

constexpr bool isValidBackflushCycles(int cycles) noexcept {
    return cycles >= BACKFLUSH_CYCLES_MIN && cycles <= BACKFLUSH_CYCLES_MAX;
}

constexpr bool isValidBackflushFillTime(double time) noexcept {
    return time >= BACKFLUSH_FILL_TIME_MIN && time <= BACKFLUSH_FILL_TIME_MAX;
}

constexpr bool isValidBackflushFlushTime(double time) noexcept {
    return time >= BACKFLUSH_FLUSH_TIME_MIN && time <= BACKFLUSH_FLUSH_TIME_MAX;
}

// ============================================================================
// STANDBY VALIDATION
// ============================================================================

constexpr bool isValidStandbyTime(double time) noexcept {
    return time >= STANDBY_MODE_TIME_MIN && time <= STANDBY_MODE_TIME_MAX;
}

// ============================================================================
// NETWORK VALIDATION
// ============================================================================

constexpr bool isValidMqttPort(int port) noexcept {
    return port > 0 && port <= 65535;
}

constexpr bool isValidMqttReconnectAttempts(int attempts) noexcept {
    return attempts >= -1 && attempts <= 100;  // -1 means infinite
}

// ============================================================================
// DISPLAY VALIDATION
// ============================================================================

constexpr bool isValidPostBrewTimerDuration(double duration) noexcept {
    return duration >= POST_BREW_TIMER_DURATION_MIN && duration <= POST_BREW_TIMER_DURATION_MAX;
}

// ============================================================================
// COMPREHENSIVE VALIDATION
// ============================================================================

/**
 * @brief Validate all default configuration values at compile time
 * @return true if all defaults are within valid ranges
 */
constexpr bool validateAllDefaults() noexcept {
    // Temperature defaults
    static_assert(isValidBrewTemperature(SETPOINT), "SETPOINT is outside valid range");
    static_assert(isValidTemperatureOffset(TEMPOFFSET), "TEMPOFFSET is outside valid range");
    static_assert(isValidSteamTemperature(STEAMSETPOINT), "STEAMSETPOINT is outside valid range");

    // PID defaults
    static_assert(isValidPidKp(AGGKP), "AGGKP is outside valid range");
    static_assert(isValidPidTn(AGGTN), "AGGTN is outside valid range");
    static_assert(isValidPidTv(AGGTV), "AGGTV is outside valid range");
    static_assert(isValidPidIMax(AGGIMAX), "AGGIMAX is outside valid range");
    static_assert(isValidPidBdKp(AGGBKP), "AGGBKP is outside valid range");

    // Scale defaults
    static_assert(isValidScaleCalibration(SCALE_CALIBRATION_FACTOR), "SCALE_CALIBRATION_FACTOR is outside valid range");
    static_assert(isValidScaleKnownWeight(SCALE_KNOWN_WEIGHT), "SCALE_KNOWN_WEIGHT is outside valid range");
    static_assert(isValidScaleSamples(SCALE_SAMPLES), "SCALE_SAMPLES is outside valid range");

    return true;
}

// Force validation at compile time
static_assert(validateAllDefaults(), "One or more default configuration values are invalid");

// ============================================================================
// RUNTIME VALIDATION HELPERS (for dynamic values)
// ============================================================================

/**
 * @brief Validate a temperature value at runtime with compile-time checks when possible
 */
template<typename T>
constexpr bool validateTemperature(T temp, T min, T max) noexcept {
    if constexpr (std::is_constant_evaluated()) {
        // Compile-time validation
        return temp >= min && temp <= max;
    } else {
        // Runtime validation
        return temp >= min && temp <= max;
    }
}

/**
 * @brief Validate any numeric parameter at runtime with compile-time optimization
 */
template<typename T>
constexpr bool validateRange(T value, T min, T max) noexcept {
    return value >= min && value <= max;
}

/**
 * @brief Type-safe validation wrapper that provides detailed error information
 */
template<typename T, auto ValidatorFunc>
struct ValidatedValue {
    T value_;

    constexpr ValidatedValue(T val) noexcept : value_(val) {
        if constexpr (std::is_constant_evaluated()) {
            // Compile-time validation
            static_assert(ValidatorFunc(val), "Value fails validation at compile time");
        }
    }

    constexpr T get() const noexcept { return value_; }
    constexpr operator T() const noexcept { return value_; }

    constexpr bool isValid() const noexcept {
        return ValidatorFunc(value_);
    }
};

// Convenience type aliases
using ValidatedBrewTemp = ValidatedValue<double, isValidBrewTemperature>;
using ValidatedSteamTemp = ValidatedValue<double, isValidSteamTemperature>;
using ValidatedPidKp = ValidatedValue<double, isValidPidKp>;
using ValidatedPidTn = ValidatedValue<double, isValidPidTn>;
using ValidatedPidTv = ValidatedValue<double, isValidPidTv>;

} // namespace CleverCoffee::Validation

// ============================================================================
// USAGE EXAMPLES
// ============================================================================

/*
// Compile-time validation - errors caught at build time
constexpr auto valid_temp = CleverCoffee::Validation::ValidatedBrewTemp{95.0};    // OK
constexpr auto invalid_temp = CleverCoffee::Validation::ValidatedBrewTemp{300.0}; // Compile error!

// Runtime validation with compile-time optimization
bool isValidConfig(double temp) {
    return CleverCoffee::Validation::isValidBrewTemperature(temp);
}

// Template validation for parameter classes
template<typename T>
class ConfigParam {
    T value_;
public:
    template<auto Validator>
    constexpr ConfigParam(T val) requires (Validator(val)) : value_(val) {}
};
*/

#endif // consteval support
