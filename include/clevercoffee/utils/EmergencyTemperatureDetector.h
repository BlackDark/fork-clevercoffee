#pragma once

#include "clevercoffee/constants/Temperature.h"
#include "clevercoffee/constants/Water.h"

/**
 * Emergency Temperature Detector
 *
 * Implements debouncing, hysteresis, and sensor validation for emergency
 * temperature detection. This prevents false alarms from sensor noise while
 * ensuring rapid detection of genuine overtemperature conditions.
 *
 * Key features:
 * - Debouncing: Requires 3 consecutive readings above threshold
 * - Hysteresis: Prevents oscillation between emergency and normal states
 * - Sensor validation: Detects out-of-range readings as emergency condition
 */
class EmergencyTemperatureDetector {
 public:
  /**
   * Constructor
   * 
   * @param threshold The temperature threshold above which to trigger emergency
   * @param hysteresis Hysteresis range to prevent state oscillation
   */
  EmergencyTemperatureDetector(float threshold, float hysteresis)
      : threshold_(threshold),
        hysteresis_(hysteresis),
        debounce_count_(0),
        is_emergency_(false) {}

  /**
   * Check current temperature and update emergency state
   * 
   * Detection logic:
   * 1. Out-of-range readings (< -40°C or > 180°C) immediately trigger emergency
   * 2. Readings above threshold accumulate debounce count
   * 3. After 3 consecutive above-threshold readings, sets emergency state to true
   * 4. Emergency state resets when temperature drops below (threshold - hysteresis)
   * 5. Readings in hysteresis zone maintain current emergency state
   * 
   * @param current_temp The current temperature reading
   * @return true if emergency condition is active, false otherwise
   */
  bool checkEmergency(float current_temp) {
    const int DEBOUNCE_REQUIRED = CleverCoffee::Water::TANK_EMPTY_READINGS_NEEDED;
    const float MIN_VALID_TEMP = CleverCoffee::Temperature::MIN_VALID_TEMP_C;
    const float MAX_VALID_TEMP = CleverCoffee::Temperature::MAX_VALID_TEMP_C;

    // Detect sensor errors (out of valid range)
    if (current_temp < MIN_VALID_TEMP || current_temp > MAX_VALID_TEMP) {
      return true;  // Treat out-of-range as emergency
    }

    // Main logic: detect high temperature
    if (current_temp > threshold_) {
      // Temperature above threshold - accumulate debounce count
      debounce_count_++;
      if (debounce_count_ >= DEBOUNCE_REQUIRED) {
        is_emergency_ = true;
      }
    } else if (current_temp < threshold_ - hysteresis_) {
      // Below threshold minus hysteresis - reset emergency
      debounce_count_ = 0;
      is_emergency_ = false;
    } else {
      // In hysteresis zone (threshold-hysteresis to threshold)
      // Don't change emergency state, but reset debounce for next trigger
      if (!is_emergency_) {
        debounce_count_ = 0;  // Reset debounce if not yet in emergency
      }
    }

    return is_emergency_;
  }

  /**
   * Reset the detector to initial state
   */
  void reset() {
    debounce_count_ = 0;
    is_emergency_ = false;
  }

  // Accessors for testing and debugging
  float getThreshold() const { return threshold_; }
  float getHysteresis() const { return hysteresis_; }
  bool isEmergency() const { return is_emergency_; }

 private:
  float threshold_;
  float hysteresis_;
  int debounce_count_;
  bool is_emergency_;
};
