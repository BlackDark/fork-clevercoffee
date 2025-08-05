/**
 * @file standby.h
 *
 * @brief Standby mode
 */

#pragma once

#include "Config.h"
#include "utils/legacyUtils.h"

inline unsigned long getStandbyTimeoutMillis() {
    return static_cast<unsigned long>(Config::getInstance().standbyTime.get() * 60 * 1000);
}

/**
 * @brief Decrements the remaining standby time every second, counting down from the configured duration
 */
inline void updateStandbyTimer() {
    if (!Config::getInstance().standbyEnabled.get()) {
        return;
    }

    const unsigned long currentTime = millis();

    // Update every second since last update
    if (currentTime - g_state.standby.lastStandbyTimeMillis >= 1000) {
        g_state.standby.lastStandbyTimeMillis = currentTime;

        if (g_state.standby.standbyModeRemainingTimeMillis != 0) {
            const unsigned long standbyModeTimeMillis = getStandbyTimeoutMillis();
            const unsigned long elapsedTime = currentTime - g_state.standby.standbyModeStartTimeMillis;

            if (standbyModeTimeMillis > elapsedTime) {
                g_state.standby.standbyModeRemainingTimeMillis = standbyModeTimeMillis - elapsedTime;

                if (elapsedTime % 60000 < 1000) {
                    LOGF(INFO, "Standby time remaining: %i minutes", (g_state.standby.standbyModeRemainingTimeMillis / 60000) + 1);
                }
            }
            else {
                g_state.standby.standbyModeRemainingTimeMillis = 0;
                LOG(INFO, "Entering standby mode...");
            }
        }
        else if (g_state.standby.standbyModeRemainingTimeDisplayOffMillis != 0) {
            const unsigned long standbyModeTimeMillis = getStandbyTimeoutMillis() + TIME_TO_DISPLAY_OFF_MILLIS;
            const unsigned long elapsedTime = currentTime - g_state.standby.standbyModeStartTimeMillis;

            if (standbyModeTimeMillis > elapsedTime) {
                g_state.standby.standbyModeRemainingTimeDisplayOffMillis = standbyModeTimeMillis - elapsedTime;

                if (elapsedTime % 60000 < 1000) {
                    LOGF(INFO, "Standby time until display is turned off: %i minutes", (g_state.standby.standbyModeRemainingTimeDisplayOffMillis / 60000) + 1);
                }
            }
            else {
                g_state.standby.standbyModeRemainingTimeDisplayOffMillis = 0;
                LOG(INFO, "Turning off display...");
            }
        }
    }
}

inline void resetStandbyTimer(const LegacyMachineState state) {
    g_state.standby.standbyModeRemainingTimeMillis = getStandbyTimeoutMillis();
    g_state.standby.standbyModeRemainingTimeDisplayOffMillis = TIME_TO_DISPLAY_OFF_MILLIS;
    g_state.standby.standbyModeStartTimeMillis = millis();

    LOGF(INFO, "Resetting standby timer to %i minutes", static_cast<int>(Config::getInstance().standbyTime.get()));
    LOGF(DEBUG, "New machine state: %s", machinestateEnumToString(state));
}
