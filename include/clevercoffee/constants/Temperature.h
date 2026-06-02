#pragma once

namespace CleverCoffee::Temperature {

// Emergency thresholds
constexpr float EMERGENCY_THRESHOLD_C       = 145.0f;
constexpr float EMERGENCY_RESET_THRESHOLD_C = 120.0f;
constexpr float HYSTERESIS_C                = 10.0f;

// Emergency safe temperature threshold (below which emergency can be cleared)
constexpr float EMERGENCY_SAFE_TEMP_C = 100.0f;

// Sensor validation
constexpr float MIN_VALID_TEMP_C = 0.0f;
constexpr float MAX_VALID_TEMP_C = 200.0f;

// Default setpoints
constexpr float DEFAULT_BREW_SETPOINT_C  = 95.0f;
constexpr float DEFAULT_STEAM_SETPOINT_C = 120.0f;

// Temperature tolerance for LED status indication
constexpr float TEMP_TOLERANCE_NORMAL_C = 0.3f; ///< Temperature tolerance for normal mode LED
constexpr float TEMP_TOLERANCE_STEAM_C  = 5.0f; ///< Temperature tolerance for steam mode LED

// Fullscreen heating logo when further than this below brew setpoint (PID normal mode)
constexpr float HEATING_LOGO_THRESHOLD_C = 5.0f;

// Steam mode temperature threshold for LED indication
constexpr float STEAM_LED_THRESHOLD_C = 115.0f; ///< Temperature above which steam LED logic applies

} // namespace CleverCoffee::Temperature
