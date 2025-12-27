#pragma once

namespace CleverCoffee::BrewTiming {

// Millisecond constants
constexpr unsigned long FINISHED_DISPLAY_TIMEOUT_MS = 3000;
constexpr unsigned long PRE_INFUSION_TIME_MS = 2000;
constexpr unsigned long PRE_INFUSION_PAUSE_MS = 5000;
constexpr unsigned long BREW_PID_DELAY_MS = 10000;

// Second constants
constexpr double TARGET_BREW_TIME_SEC = 25.0;
constexpr double PRE_INFUSION_TIME_SEC = 2.0;
constexpr double PRE_INFUSION_PAUSE_TIME_SEC = 5.0;
constexpr double BREW_PID_DELAY_SEC = 10.0;

} // namespace CleverCoffee::BrewTiming
