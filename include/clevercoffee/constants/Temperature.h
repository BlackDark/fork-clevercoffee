#pragma once

namespace CleverCoffee::Temperature {

// Emergency thresholds
constexpr float EMERGENCY_THRESHOLD_C = 145.0f;
constexpr float EMERGENCY_RESET_THRESHOLD_C = 120.0f;
constexpr float HYSTERESIS_C = 10.0f;

// Sensor validation
constexpr float MIN_VALID_TEMP_C = 0.0f;
constexpr float MAX_VALID_TEMP_C = 200.0f;

// Default setpoints
constexpr float DEFAULT_BREW_SETPOINT_C = 95.0f;
constexpr float DEFAULT_STEAM_SETPOINT_C = 120.0f;

} // namespace CleverCoffee::Temperature
