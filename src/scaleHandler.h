/**
 * @file scaleHandler.h
 *
 * @brief Implementation of scale initialization and weight measurement with Bluetooth support
 */

#pragma once

#include "Config.h"
#include "brewStates.h"
#include "display/languages.h"
#include "hardware/pinmapping.h"
#include "hardware/scales/BluetoothScale.h"
#include "hardware/scales/HX711Scale.h"
#include "utils/helperUtils.h"
#include <U8g2lib.h> // Required for U8G2 display methods

void displayScaleFailed();
void displayWrappedMessage(const char* msg);

/**
 * @brief Check Bluetooth scale connection status and handle failures
 */
inline void checkBluetoothScaleConnection() {
    if (!g_state.hardware.isBluetoothScale || !g_state.hardware.scale) {
        return;
    }

    // Check connection status periodically for logging/fallback logic
    if (const unsigned long currentTime = millis(); currentTime - g_state.sensors.lastScaleConnectionCheck > SCALE_CONNECTION_CHECK_INTERVAL) {
        static_cast<BluetoothScale*>(g_state.hardware.scale.get())->updateConnection();

        g_state.sensors.lastScaleConnectionCheck = currentTime;

        if (const bool connected = g_state.hardware.scale->isConnected(); !connected) {
            if (!g_state.sensors.scaleConnectionLost) {
                // Connection just lost
                g_state.sensors.scaleConnectionLost = true;
                g_state.sensors.scaleConnectionFailureTime = currentTime;

                LOG(WARNING, "Bluetooth scale connection lost");

                // During active brew, activate fallback mechanism
                if (g_state.sensors.currBrewState != kBrewIdle && g_state.sensors.currBrewState != kBrewFinished) {
                    const bool brewByWeightEnabled = Config::getInstance().brewByWeightEnabled.get();
                    const bool brewByTimeEnabled = Config::getInstance().brewByTimeEnabled.get();

                    if (brewByWeightEnabled && brewByTimeEnabled) {
                        LOG(INFO, "Activating brew-by-time fallback due to scale connection loss");
                        g_state.sensors.brewByWeightFallbackActive = true;
                    }
                    else if (brewByWeightEnabled) {
                        LOG(WARNING, "BLE Scale connection lost during brew-by-weight only mode, stopping brew");
                        g_state.sensors.currBrewState = kBrewFinished;
                    }
                }
            }

            // Check if we should give up reconnecting
            if (currentTime - g_state.sensors.scaleConnectionFailureTime > SCALE_RECONNECTION_TIMEOUT) {
                if (!g_state.sensors.scaleFailure) {
                    LOG(ERROR, "Bluetooth scale connection timeout - marking as failed");
                    g_state.sensors.scaleFailure = true;
                }
            }
        }
        else {
            // Connection restored
            if (g_state.sensors.scaleConnectionLost) {
                g_state.sensors.scaleConnectionLost = false;
                g_state.sensors.scaleFailure = false;
                g_state.sensors.brewByWeightFallbackActive = false;
                LOG(INFO, "Bluetooth scale connection restored");
            }
        }
    }
}

/**
 * @brief Get weight with connection error handling
 */
inline float getScaleWeight() {
    if (!g_state.hardware.scale) {
        return g_state.sensors.lastValidWeight;
    }

    if (g_state.hardware.isBluetoothScale) {
        // Always check connection - even if we've marked it as failed
        checkBluetoothScaleConnection();

        if (g_state.sensors.scaleConnectionLost) {
            // Return last valid weight during connection issues
            return g_state.sensors.lastValidWeight;
        }
    }

    if (g_state.hardware.scale->update()) {
        const float weight = g_state.hardware.scale->getWeight();
        g_state.sensors.lastValidWeight = weight;
        return weight;
    }

    return g_state.sensors.lastValidWeight;
}

/**
 * @brief Check if brew-by-weight should be used (considering fallback state)
 */
inline bool shouldUseBrewByWeight() {
    const bool brewByWeightEnabled = Config::getInstance().brewByWeightEnabled.get();
    return brewByWeightEnabled && !g_state.sensors.brewByWeightFallbackActive && !g_state.sensors.scaleConnectionLost;
}

inline void scaleCalibrate(const int cellNumber, const int pin) {
    if (g_state.hardware.isBluetoothScale) {
        // Bluetooth scales handle calibration internally
        displayWrappedMessage("Bluetooth scales\nhandle calibration\ninternally");
        delay(2000);
        return;
    }

    const int scaleSamples = Config::getInstance().hardwareSensorsScaleSamples.get();

    auto* hx711Scale = static_cast<HX711Scale*>(g_state.hardware.scale.get());
    HX711_ADC* loadCell = hx711Scale->getLoadCell(cellNumber);

    if (!loadCell) {
        return;
    }

    loadCell->setCalFactor(1.0);

    // Use buffer for message formatting to avoid String allocation
    char msgBuffer[128];
    snprintf(msgBuffer, sizeof(msgBuffer), "%s%d\n", langstring_calibrate_start, cellNumber);
    displayWrappedMessage(msgBuffer);
    delay(2000);

    LOGF(INFO, "Taking scale %d, pin %d to zero point", cellNumber, pin);

    loadCell->update();
    loadCell->tare();

    LOGF(INFO, "Put load on scale %d within the next 10 seconds", pin);

    const auto scaleKnownWeight = Config::getInstance().hardwareSensorsScaleKnownWeight.get();

    snprintf(msgBuffer, sizeof(msgBuffer), "%s%sg\n", langstring_calibrate_in_progress, number2string(scaleKnownWeight));
    displayWrappedMessage(msgBuffer);
    delay(10000);

    LOG(INFO, "Taking scale load point");

    // increase scale samples temporarily to ensure a stable reading
    loadCell->setSamplesInUse(128);
    loadCell->refreshDataSet();
    const float calibration = loadCell->getNewCalibration(scaleKnownWeight);
    loadCell->setSamplesInUse(scaleSamples);

    LOGF(INFO, "New calibration: %f", calibration);

    hx711Scale->setCalibrationFactor(calibration, cellNumber);

    // Save calibration to config system
    if (cellNumber == 2) {
        Config::getInstance().hardwareSensorsScaleCalibration2.set(calibration);
    }
    else {
        Config::getInstance().hardwareSensorsScaleCalibration.set(calibration);
    }

    snprintf(msgBuffer, sizeof(msgBuffer), "%s%s\n", langstring_calibrate_complete, number2string(calibration));
    displayWrappedMessage(msgBuffer);
    delay(2000);
}

inline float w1 = 0.0;
inline float w2 = 0.0;

inline void checkWeight() {
    if (!g_state.hardware.scale) {
        return;
    }

    g_state.sensors.currReadingWeight = getScaleWeight();

    if (g_state.sensors.scaleFailure) {
        return;
    }

    if (g_state.sensors.scaleCalibrationOn && !g_state.hardware.isBluetoothScale) {
        scaleCalibrate(1, PIN_HXDAT);

        // Calibrate second cell
        if (const Hardware::ScaleType scaleType = Config::getInstance().hardwareSensorsScaleType.get(); scaleType == Hardware::ScaleType::HX711_DUAL) {
            scaleCalibrate(2, PIN_HXDAT2);
        }

        g_state.sensors.scaleCalibrationOn = false;
    }

    if (g_state.sensors.scaleTareOn) {
        g_state.sensors.scaleTareOn = false;
        g_state.hardware.display->clearBuffer();
        g_state.hardware.display->drawStr(0, 2, "Taring scale,");
        g_state.hardware.display->drawStr(0, 12, "remove any load!");
        g_state.hardware.display->drawStr(0, 22, "....");
        g_state.hardware.display->sendBuffer();
        delay(2000);

        g_state.hardware.scale->tare();

        g_state.hardware.display->drawStr(0, 32, "done");
        g_state.hardware.display->sendBuffer();
        delay(2000);
    }
}

void initScale();

/**
 * @brief Scale with shot timer and connection handling
 */
/**
 * @brief Scale with shot timer and connection handling
 */
inline void shotTimerScale() {
    switch (g_state.sensors.shottimerCounter) {
        case 10: // waiting step for brew switch turning on
            if (g_state.sensors.currBrewState != kBrewIdle) {
                // For Bluetooth scales with auto-tare, wait a bit before capturing pre-brew weight
                if (g_state.hardware.isBluetoothScale && g_state.sensors.autoTareInProgress) {
                    // Wait at least 2 seconds for Bluetooth tare to complete
                    if (millis() - g_state.sensors.autoTareStartTime < 2000) {
                        break;
                    }

                    g_state.sensors.autoTareInProgress = false;
                }

                g_state.sensors.preBrewWeight = g_state.sensors.currReadingWeight;
                g_state.sensors.shottimerCounter = 20;

                // Reset fallback state at start of new brew
                g_state.sensors.brewByWeightFallbackActive = false;
            }
            break;

        case 20:
            g_state.sensors.currBrewWeight = g_state.sensors.currReadingWeight - g_state.sensors.preBrewWeight;

            if (g_state.sensors.currBrewState == kBrewIdle) {
                g_state.sensors.shottimerCounter = 10;

                // Reset fallback state when brew ends
                g_state.sensors.brewByWeightFallbackActive = false;
            }
            break;

        default:;
    }
}

/**
 * @brief Get scale connection status for display
 */
inline bool getScaleConnectionStatus() {
    if (!g_state.hardware.isBluetoothScale || !g_state.hardware.scale) {
        return true; // Not applicable for HX711 scales
    }

    return g_state.hardware.scale->isConnected();
}

/**
 * @brief Check if scale is in fallback mode
 */
inline bool isScaleInFallbackMode() {
    return g_state.sensors.brewByWeightFallbackActive;
}

/**
 * @brief Check if Bluetooth scale is currently trying to connect
 */
inline bool isBluetoothScaleConnecting() {
    if (!g_state.hardware.isBluetoothScale || !g_state.hardware.scale) {
        return false;
    }

    return static_cast<BluetoothScale*>(g_state.hardware.scale.get())->isConnecting();
}
