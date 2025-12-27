#pragma once

namespace CleverCoffee::Display {

// Dimensions - using unique names to avoid macro collisions with defaults.h
constexpr int OLED_WIDTH = 128;
constexpr int OLED_HEIGHT = 64;
constexpr int STATUS_BAR_PIXEL_HEIGHT = 12;
constexpr int STATUS_BAR_Y_POSITION = 12;

// Timing
constexpr unsigned long REFRESH_INTERVAL_MS = 100;
constexpr unsigned long AUTO_SLEEP_MINUTES = 35;

} // namespace CleverCoffee::Display
