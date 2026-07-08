/**
 * @file Timing.h
 * @brief Timing constants for system operations
 *
 * This file consolidates all timing-related constants including:
 * - General system timing (ISR, sensors, loops)
 * - Display timing and dimensions
 * - Brew-specific timing constants
 */

#pragma once

namespace CleverCoffee::Timing {

// ISR Timer constants
constexpr unsigned long ISR_TIMER_INTERVAL_US = 10000; ///< ISR timer interval in microseconds (10ms)
constexpr unsigned int  ISR_COUNTER_INCREMENT = 10;    ///< ISR counter increment per tick (10ms per tick)

// State timeout constants
constexpr unsigned long STEAM_STOPPED_DISPLAY_TIMEOUT_MS     = 2000; ///< Display timeout for steam stopped state
constexpr unsigned long HOT_WATER_STOPPED_DISPLAY_TIMEOUT_MS = 2000; ///< Display timeout for hot water stopped state

// Debug logging throttling
constexpr unsigned long DEBUG_LOG_THROTTLE_MS = 5000; ///< Throttle debug logs to every 5 seconds

// Recovery and error handling timeouts
constexpr unsigned long ERROR_RECOVERY_DELAY_MS    = 5000;   ///< Delay before attempting error recovery
constexpr unsigned long EEPROM_RECOVERY_TIMEOUT_MS = 300000; ///< EEPROM recovery timeout (5 minutes)
constexpr unsigned long STANDBY_TIMER_RESET_INTERVAL_MS =
    30000; ///< Interval for resetting standby timer in PID state (30 seconds)

// Performance monitoring
constexpr unsigned long SLOW_LOOP_REPORT_INTERVAL_MS   = 30000; ///< Interval for slow loop performance reports
constexpr unsigned long SLOW_LOOP_THRESHOLD_MS         = 100;   ///< Threshold for detecting slow loops (>100ms)
constexpr unsigned int  PERFORMANCE_LOG_INTERVAL_LOOPS = 1000;  ///< Log performance stats every N loops

// Loop manager timing constants
constexpr unsigned long DISPLAY_REFRESH_INTERVAL_MS = 100; ///< Display refresh interval
// Temperature sensor interval: 400ms (2.5Hz) matches DS18B20 11-bit conversion time (~380ms)
// Optimization opportunity: Reduce to 200ms (5Hz) by using 10-bit resolution (~188ms conversion)
// See SENSOR_OPTIMIZATION.md for details
constexpr unsigned long TEMPERATURE_SENSOR_INTERVAL_MS = 400;    ///< Temperature sensor read interval (2.5Hz)
constexpr unsigned long PRESSURE_SENSOR_INTERVAL_MS    = 50;     ///< Pressure sensor read interval (20Hz) - optimal
constexpr unsigned long SCALE_SENSOR_INTERVAL_MS       = 100;    ///< Scale sensor read interval (10Hz) - optimal
constexpr unsigned long HASSIO_DISCOVERY_INTERVAL_MS   = 300000; ///< HASSIO discovery message interval (5 minutes)

} // namespace CleverCoffee::Timing

// Display constants (dimensions and timing)
namespace CleverCoffee::Display {

// Dimensions - using unique names to avoid macro collisions with defaults.h
constexpr int OLED_WIDTH              = 128;
constexpr int OLED_HEIGHT             = 64;
constexpr int STATUS_BAR_PIXEL_HEIGHT = 12;
constexpr int STATUS_BAR_Y_POSITION   = 12;

// Timing
// Note: REFRESH_INTERVAL_MS is the same as Timing::DISPLAY_REFRESH_INTERVAL_MS (100ms)
// Use Timing::DISPLAY_REFRESH_INTERVAL_MS for consistency
constexpr unsigned long AUTO_SLEEP_MINUTES = 35;

} // namespace CleverCoffee::Display

// Brew-specific timing constants
namespace CleverCoffee::BrewTiming {

// Millisecond constants
constexpr unsigned long FINISHED_DISPLAY_TIMEOUT_MS = 3000;
constexpr unsigned long PRE_INFUSION_TIME_MS        = 2000;
constexpr unsigned long PRE_INFUSION_PAUSE_MS       = 5000;
constexpr unsigned long BREW_PID_DELAY_MS           = 10000;

// Second constants
constexpr double TARGET_BREW_TIME_SEC        = 25.0;
constexpr double PRE_INFUSION_TIME_SEC       = 2.0;
constexpr double PRE_INFUSION_PAUSE_TIME_SEC = 5.0;
constexpr double BREW_PID_DELAY_SEC          = 10.0;

} // namespace CleverCoffee::BrewTiming

namespace CleverCoffee::BackflushTiming {

constexpr unsigned long FINISHED_DISPLAY_TIMEOUT_MS = 3000;

} // namespace CleverCoffee::BackflushTiming
