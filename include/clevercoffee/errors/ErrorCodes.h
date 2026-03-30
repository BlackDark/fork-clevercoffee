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
    ErrorCode   code_;
    const char* message_;

  public:
    /**
     * @brief Constructor
     * @param code Error code
     * @param message Human-readable error message (must be static string)
     */
    Error(ErrorCode code, const char* message) noexcept : code_(code), message_(message) {}

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
     * @brief Get user-friendly error message with recovery suggestion
     * @return Formatted error message with actionable advice
     */
    [[nodiscard]] const char* getUserMessage() const noexcept {
        switch (code_) {
            case ErrorCode::SENSOR_TIMEOUT:
                return "Sensor timeout - check wiring and power";
            case ErrorCode::SENSOR_DISCONNECTED:
                return "Sensor disconnected - check connections";
            case ErrorCode::SENSOR_FAULT:
                return "Sensor fault detected - check wiring or replace sensor";
            case ErrorCode::SENSOR_NOT_READY:
                return "Sensor not ready - wait for initialization";
            case ErrorCode::HARDWARE_FAILURE:
                return "Hardware failure - check power and connections";
            case ErrorCode::WATER_TANK_EMPTY:
                return "Water tank empty - please refill";
            case ErrorCode::INVALID_STATE:
                return "Invalid system state - restart may be required";
            case ErrorCode::INVALID_TRANSITION:
                return "Invalid state transition - system will recover automatically";
            case ErrorCode::EMERGENCY_STOP:
                return "EMERGENCY STOP - temperature too high, system disabled";
            case ErrorCode::EMERGENCY_TEMPERATURE:
                return "EMERGENCY - temperature critical, system disabled";
            case ErrorCode::UNKNOWN_ERROR:
            default:
                return message_ ? message_ : "Unknown error occurred";
        }
    }

    /**
     * @brief Get recovery suggestion for this error
     * @return Actionable recovery steps
     */
    [[nodiscard]] const char* getRecoverySuggestion() const noexcept {
        switch (code_) {
            case ErrorCode::SENSOR_TIMEOUT:
                return "Check sensor wiring, ensure power is connected, wait 10s and retry";
            case ErrorCode::SENSOR_DISCONNECTED:
                return "Verify sensor is properly connected, check for loose wires";
            case ErrorCode::SENSOR_FAULT:
                return "Inspect sensor wiring for damage, replace sensor if needed";
            case ErrorCode::SENSOR_NOT_READY:
                return "Wait for system initialization to complete (usually < 5s)";
            case ErrorCode::HARDWARE_FAILURE:
                return "Check all power connections, verify hardware is properly installed";
            case ErrorCode::WATER_TANK_EMPTY:
                return "Refill water tank - system will resume automatically when full";
            case ErrorCode::INVALID_STATE:
                return "Restart the system - press reset button or power cycle";
            case ErrorCode::INVALID_TRANSITION:
                return "No action needed - system will recover automatically";
            case ErrorCode::EMERGENCY_STOP:
                return "Wait for temperature to cool below 100°C, then restart system";
            case ErrorCode::EMERGENCY_TEMPERATURE:
                return "CRITICAL - Allow system to cool, check for hardware issues before restart";
            case ErrorCode::UNKNOWN_ERROR:
            default:
                return "Check system logs for details, restart if problem persists";
        }
    }

    /**
     * @brief Check if this is a critical error
     * @return true if error requires immediate action
     */
    [[nodiscard]] bool isCritical() const noexcept {
        return code_ == ErrorCode::SENSOR_DISCONNECTED || code_ == ErrorCode::SENSOR_FAULT ||
               code_ == ErrorCode::HARDWARE_FAILURE || code_ == ErrorCode::EMERGENCY_STOP ||
               code_ == ErrorCode::EMERGENCY_TEMPERATURE;
    }

    /**
     * @brief Get error code as string for logging
     * @return Error code name as string
     */
    [[nodiscard]] const char* codeString() const noexcept {
        switch (code_) {
            case ErrorCode::SUCCESS:
                return "SUCCESS";
            case ErrorCode::SENSOR_TIMEOUT:
                return "SENSOR_TIMEOUT";
            case ErrorCode::SENSOR_DISCONNECTED:
                return "SENSOR_DISCONNECTED";
            case ErrorCode::SENSOR_FAULT:
                return "SENSOR_FAULT";
            case ErrorCode::SENSOR_NOT_READY:
                return "SENSOR_NOT_READY";
            case ErrorCode::HARDWARE_FAILURE:
                return "HARDWARE_FAILURE";
            case ErrorCode::WATER_TANK_EMPTY:
                return "WATER_TANK_EMPTY";
            case ErrorCode::INVALID_STATE:
                return "INVALID_STATE";
            case ErrorCode::INVALID_TRANSITION:
                return "INVALID_TRANSITION";
            case ErrorCode::EMERGENCY_STOP:
                return "EMERGENCY_STOP";
            case ErrorCode::EMERGENCY_TEMPERATURE:
                return "EMERGENCY_TEMPERATURE";
            case ErrorCode::UNKNOWN_ERROR:
                return "UNKNOWN_ERROR";
            default:
                return "UNKNOWN";
        }
    }
};

} // namespace CleverCoffee
