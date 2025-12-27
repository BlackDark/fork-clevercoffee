#pragma once

#include <string>

namespace CleverCoffee {

enum class ErrorCode {
    SUCCESS = 0,
    SENSOR_READ_FAILED,
    SENSOR_TIMEOUT,
    INVALID_CONFIG,
    INITIALIZATION_FAILED,
    HARDWARE_ERROR,
    NETWORK_ERROR,
};

class Error {
public:
    Error(ErrorCode code, std::string message)
        : code_(code), message_(std::move(message)) {}

    ErrorCode code() const noexcept { return code_; }
    const std::string& message() const noexcept { return message_; }

private:
    ErrorCode code_;
    std::string message_;
};

} // namespace CleverCoffee
