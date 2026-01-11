/**
 * @file EmergencyStopManager.cpp
 * @brief Implementation of centralized emergency stop logic manager
 */

#include "clevercoffee/control/EmergencyStopManager.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/constants/Temperature.h"

namespace CleverCoffee {

EmergencyStopManager::EmergencyStopManager(const Config& config)
    : config_(config) {
}

bool EmergencyStopManager::checkEmergencyConditions(double temperature) {
    const double emergencyTemp = config_.emergencyStopTemp.get();
    const double hysteresis = config_.emergencyStopHysteresis.get();
    const double sensorMinValid = Temperature::MIN_VALID_TEMP_C;
    const double sensorMaxValid = Temperature::MAX_VALID_TEMP_C;

    // STEP 1: Check for sensor disconnection or invalid reading
    // Invalid readings immediately trigger emergency (no debouncing for safety)
    if (temperature < sensorMinValid || temperature > sensorMaxValid) {
        LOGF(ERROR, "Emergency: Invalid temperature reading (%.1f°C outside valid range [%.1f, %.1f])",
             temperature, sensorMinValid, sensorMaxValid);
        triggerEmergency();
        return true;
    }

    // STEP 2: Hysteresis-based emergency detection with debouncing
    if (temperature > emergencyTemp) {
        emergencyTempReadingCount_++;
        LOGF(WARNING, "High temperature detected: %.1f°C (reading %d/%d, threshold: %.1f°C)",
             temperature, emergencyTempReadingCount_, DEBOUNCE_COUNT, emergencyTemp);

        // Require multiple consecutive high readings to trigger emergency
        if (emergencyTempReadingCount_ >= DEBOUNCE_COUNT) {
            LOGF(ERROR, "Emergency: Temperature too high (%.1f°C > %.1f°C limit)!",
                 temperature, emergencyTemp);
            triggerEmergency();
            return true;
        }
    } else if (temperature < (emergencyTemp - hysteresis)) {
        // Reset counter only when temperature drops below threshold minus hysteresis
        if (emergencyTempReadingCount_ > 0) {
            LOGF(INFO, "Temperature normalized (%.1f°C < %.1f°C). Resetting emergency counter.",
                 temperature, emergencyTemp - hysteresis);
            emergencyTempReadingCount_ = 0;
        }
        // If emergency was active, check if we can clear it
        if (emergencyActive_ && isEmergencyCleared(temperature)) {
            clearEmergency();
        }
    }
    // If temperature is between (threshold - hysteresis) and threshold, keep counter as-is
    // This implements hysteresis to prevent oscillation

    return emergencyActive_;
}

bool EmergencyStopManager::isEmergencyCleared(double temperature) const {
    // Emergency can be cleared when:
    // 1. Temperature is below safe threshold
    // 2. Sensor reading is valid
    const double sensorMinValid = Temperature::MIN_VALID_TEMP_C;
    const double sensorMaxValid = Temperature::MAX_VALID_TEMP_C;

    if (temperature < sensorMinValid || temperature > sensorMaxValid) {
        // Invalid reading - cannot clear emergency
        return false;
    }

    if (temperature > SAFE_TEMPERATURE_THRESHOLD) {
        LOGF(WARNING, "Temperature still elevated: %.1f°C (safe threshold: %.1f°C)",
             temperature, SAFE_TEMPERATURE_THRESHOLD);
        return false;
    }

    return true;
}

void EmergencyStopManager::triggerEmergency() noexcept {
    if (!emergencyActive_) {
        emergencyActive_ = true;
        LOG(ERROR, "Emergency stop triggered");
    }
}

void EmergencyStopManager::clearEmergency() noexcept {
    if (emergencyActive_) {
        emergencyActive_ = false;
        emergencyTempReadingCount_ = 0;
        LOG(INFO, "Emergency stop cleared");
    }
}

void EmergencyStopManager::reset() noexcept {
    emergencyActive_ = false;
    emergencyTempReadingCount_ = 0;
}

}  // namespace CleverCoffee
