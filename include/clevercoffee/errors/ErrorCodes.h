/**
 * @file ErrorCodes.h
 * @brief Standardized error codes and Error class for system-wide use
 */

#pragma once

#include <Arduino.h>

namespace CleverCoffee {

/**
 * @enum ErrorCode
 * @brief Standardized error codes for the system
 */
enum class ErrorCode {
    // Success
    SUCCESS = 0,
    
    // Sensor errors
    SENSOR_TIMEOUT,
    SENSOR_DISCONNECTED,
    SENSOR_FAULT,
    SENSOR_NOT_READY,
    
    // Hardware errors
    HARDWARE_FAILURE,
    WATER_TANK_EMPTY,
    
    // State errors
    INVALID_STATE,
    INVALID_TRANSITION,
    
    // System errors
    EMERGENCY_STOP,
    EMERGENCY_TEMPERATURE,
    
    // Generic
    UNKNOWN_ERROR
};

/**
 * @class Error
 * @brief Type-safe error representation with code and message
 */
class Error {
    ErrorCode code_;
    const char* message_;
    
public:
    /**
     * @brief Constructor
     * @param code Error code
     * @param message Human-readable error message (must be static string)
     */
    Error(ErrorCode code, const char* message) noexcept
        : code_(code), message_(message) {}
    
    /**
     * @brief Get error code
     * @return The error code
     */
    [[nodiscard]] ErrorCode code() const noexcept {
        return code_;
    }
    
    /**
     * @brief Get error message
     * @return Human-readable error message
     */
    [[nodiscard]] const char* message() const noexcept {
        return message_;
    }
    
    /**
     * @brief Check if this is a critical error
     * @return true if error requires immediate action
     */
    [[nodiscard]] bool isCritical() const noexcept {
        return code_ == ErrorCode::SENSOR_DISCONNECTED ||
               code_ == ErrorCode::SENSOR_FAULT ||
               code_ == ErrorCode::HARDWARE_FAILURE ||
               code_ == ErrorCode::EMERGENCY_STOP ||
               code_ == ErrorCode::EMERGENCY_TEMPERATURE;
    }
};

}  // namespace CleverCoffee
