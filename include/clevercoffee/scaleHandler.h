/**
 * @file scaleHandler.h
 *
 * @brief Implementation of scale initialization and weight measurement with Bluetooth support
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/display/languages.h"
#include "clevercoffee/hardware/pinmapping.h"
#include "clevercoffee/hardware/scales/BluetoothScale.h"
#include "clevercoffee/hardware/scales/HX711Scale.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/utils/helperUtils.h"

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
    if (const unsigned long currentTime = millis();
        currentTime - g_state.sensors.lastScaleConnectionCheck > SCALE_CONNECTION_CHECK_INTERVAL) {
        static_cast<BluetoothScale*>(g_state.hardware.scale.get())->updateConnection();

        g_state.sensors.lastScaleConnectionCheck = currentTime;

        if (const bool connected = g_state.hardware.scale->isConnected(); !connected) {
            if (!g_state.sensors.scaleConnectionLost) {
                // Connection just lost
                g_state.sensors.scaleConnectionLost        = true;
                g_state.sensors.scaleConnectionFailureTime = currentTime;

                LOG(WARNING, "Bluetooth scale connection lost");

                // During active brew, activate fallback mechanism
                if (isBrewState(g_state.machine.machineState) &&
                    g_state.machine.machineState != MachineStateId::BREW_IDLE &&
                    g_state.machine.machineState != MachineStateId::BREW_FINISHED) {
                    const bool brewByWeightEnabled = Config::getInstance().brewByWeightEnabled.get();
                    const bool brewByTimeEnabled   = Config::getInstance().brewByTimeEnabled.get();

                    if (brewByWeightEnabled && brewByTimeEnabled) {
                        LOG(INFO, "Activating brew-by-time fallback due to scale connection loss");
                        g_state.sensors.brewByWeightFallbackActive = true;
                    } else if (brewByWeightEnabled) {
                        LOG(WARNING, "BLE Scale connection lost during brew-by-weight only mode, stopping brew");
                        g_state.machine.flags.requestBrewStop =
                            true; // Use condition flag instead of direct state assignment
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
        } else {
            // Connection restored
            if (g_state.sensors.scaleConnectionLost) {
                g_state.sensors.scaleConnectionLost        = false;
                g_state.sensors.scaleFailure               = false;
                g_state.sensors.brewByWeightFallbackActive = false;
                LOG(INFO, "Bluetooth scale connection restored");
            }
        }
    }
}

/**
 * @brief Get weight with connection and error handling with retry logic
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

    // Check if we're in error recovery cooldown
    const unsigned long currentTime = millis();
    if (g_state.sensors.scaleInErrorRecovery) {
        if (currentTime - g_state.sensors.lastScaleErrorTime < g_state.sensors.scaleErrorCooldownMs) {
            // Still in cooldown, return last valid weight
            return g_state.sensors.lastValidWeight;
        }
        // Cooldown expired, try to recover
        g_state.sensors.scaleInErrorRecovery = false;
        LOGF(INFO, "Scale error recovery: Attempting to resume after cooldown (errors: %d/%d)", 
             g_state.sensors.scaleReadErrorCount, g_state.sensors.scaleMaxRetries);
    }

    // Attempt to update scale
    if (g_state.hardware.scale->update()) {
        const float weight = g_state.hardware.scale->getWeight();
        
        // Validate weight reading
        if (Scale::isValidWeight(weight)) {
            g_state.sensors.lastValidWeight = weight;
            
            // Reset error count on successful read
            if (g_state.sensors.scaleReadErrorCount > 0) {
                LOGF(INFO, "Scale error recovery successful after %d errors", g_state.sensors.scaleReadErrorCount);
                g_state.sensors.scaleReadErrorCount = 0;
                g_state.sensors.scaleInErrorRecovery = false;
            }
            
            return weight;
        } else {
            LOGF(WARNING, "Scale returned invalid weight: %.2f", weight);
            // Treat invalid weight as a read error
        }
    }

    // Scale update failed or returned invalid weight
    g_state.sensors.scaleReadErrorCount++;
    g_state.sensors.lastScaleErrorTime = currentTime;
    g_state.sensors.scaleInErrorRecovery = true;

    LOGF(WARNING, "Scale read error %d/%d (will retry after %lums)", 
         g_state.sensors.scaleReadErrorCount, g_state.sensors.scaleMaxRetries, 
         g_state.sensors.scaleErrorCooldownMs);

    // Check if we've exceeded max retries
    if (g_state.sensors.scaleReadErrorCount >= g_state.sensors.scaleMaxRetries) {
        if (!g_state.sensors.scaleFailure) {
            LOG(ERROR, "Scale failed after maximum retries - marking as failed and requesting sensor error state");
            g_state.sensors.scaleFailure = true;
            
            // Request transition to sensor error state
            g_state.machine.flags.requestBrewStop = true; // Stop any active operations first
            g_state.machine.flags.requestSensorError = true; // Request sensor error state
        }
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

    auto*      hx711Scale = static_cast<HX711Scale*>(g_state.hardware.scale.get());
    HX711_ADC* loadCell   = hx711Scale->getLoadCell(cellNumber);

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

    snprintf(
        msgBuffer, sizeof(msgBuffer), "%s%sg\n", langstring_calibrate_in_progress, number2string(scaleKnownWeight));
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
    } else {
        Config::getInstance().hardwareSensorsScaleCalibration.set(calibration);
    }

    snprintf(msgBuffer, sizeof(msgBuffer), "%s%s\n", langstring_calibrate_complete, number2string(calibration));
    displayWrappedMessage(msgBuffer);
    delay(2000);
}

inline float w1 = 0.0;
inline float w2 = 0.0;

/**
 * @brief Non-blocking calibration state management
 */
enum class CalibrationState {
    IDLE,
    START_CELL1,
    WAIT_TARE_CELL1,
    WAIT_LOAD_CELL1,
    EXECUTE_CELL1,
    START_CELL2,
    WAIT_TARE_CELL2,
    WAIT_LOAD_CELL2,
    EXECUTE_CELL2,
    COMPLETE
};

inline void processNonBlockingCalibration() {
    static CalibrationState calibState = CalibrationState::IDLE;
    static unsigned long calibStartTime = 0;
    static int currentCell = 1;
    static char msgBuffer[128];
    
    if (!g_state.sensors.scaleCalibrationOn) {
        calibState = CalibrationState::IDLE;
        return;
    }
    
    if (g_state.hardware.isBluetoothScale) {
        displayWrappedMessage("Bluetooth scales\nhandle calibration\ninternally");
        g_state.sensors.scaleCalibrationOn = false;
        return;
    }
    
    const unsigned long currentTime = millis();
    const int scaleSamples = Config::getInstance().hardwareSensorsScaleSamples.get();
    auto* hx711Scale = static_cast<HX711Scale*>(g_state.hardware.scale.get());
    
    switch (calibState) {
        case CalibrationState::IDLE:
            if (g_state.sensors.scaleCalibrationOn) {
                calibState = CalibrationState::START_CELL1;
                currentCell = 1;
                calibStartTime = currentTime;
                LOG(INFO, "Starting non-blocking scale calibration");
            }
            break;
            
        case CalibrationState::START_CELL1:
            snprintf(msgBuffer, sizeof(msgBuffer), "%s%d\n", langstring_calibrate_start, 1);
            displayWrappedMessage(msgBuffer);
            calibState = CalibrationState::WAIT_TARE_CELL1;
            calibStartTime = currentTime;
            break;
            
        case CalibrationState::WAIT_TARE_CELL1:
            if (currentTime - calibStartTime >= 2000) {
                HX711_ADC* loadCell = hx711Scale->getLoadCell(1);
                if (loadCell) {
                    loadCell->setCalFactor(1.0);
                    loadCell->update();
                    loadCell->tare();
                }
                calibState = CalibrationState::WAIT_LOAD_CELL1;
                calibStartTime = currentTime;
                
                const auto scaleKnownWeight = Config::getInstance().hardwareSensorsScaleKnownWeight.get();
                snprintf(msgBuffer, sizeof(msgBuffer), "%s%sg\n", langstring_calibrate_in_progress, number2string(scaleKnownWeight));
                displayWrappedMessage(msgBuffer);
            }
            break;
            
        case CalibrationState::WAIT_LOAD_CELL1:
            if (currentTime - calibStartTime >= 10000) {
                calibState = CalibrationState::EXECUTE_CELL1;
            }
            break;
            
        case CalibrationState::EXECUTE_CELL1: {
            HX711_ADC* loadCell = hx711Scale->getLoadCell(1);
            if (loadCell) {
                loadCell->setSamplesInUse(128);
                loadCell->refreshDataSet();
                const float calibration = loadCell->getNewCalibration(Config::getInstance().hardwareSensorsScaleKnownWeight.get());
                loadCell->setSamplesInUse(scaleSamples);
                
                hx711Scale->setCalibrationFactor(calibration, 1);
                Config::getInstance().hardwareSensorsScaleCalibration.set(calibration);
                
                snprintf(msgBuffer, sizeof(msgBuffer), "%s%s\n", langstring_calibrate_complete, number2string(calibration));
                displayWrappedMessage(msgBuffer);
                LOGF(INFO, "Cell 1 calibration complete: %f", calibration);
            }
            
            // Check if dual cell setup
            if (Config::getInstance().hardwareSensorsScaleType.get() == Hardware::ScaleType::HX711_DUAL) {
                calibState = CalibrationState::START_CELL2;
                calibStartTime = currentTime + 2000; // Wait 2s before starting cell 2
            } else {
                calibState = CalibrationState::COMPLETE;
                calibStartTime = currentTime + 2000;
            }
            break;
        }
        
        case CalibrationState::START_CELL2:
            if (currentTime >= calibStartTime) {
                snprintf(msgBuffer, sizeof(msgBuffer), "%s%d\n", langstring_calibrate_start, 2);
                displayWrappedMessage(msgBuffer);
                calibState = CalibrationState::WAIT_TARE_CELL2;
                calibStartTime = currentTime;
            }
            break;
            
        case CalibrationState::WAIT_TARE_CELL2:
            if (currentTime - calibStartTime >= 2000) {
                HX711_ADC* loadCell = hx711Scale->getLoadCell(2);
                if (loadCell) {
                    loadCell->setCalFactor(1.0);
                    loadCell->update();
                    loadCell->tare();
                }
                calibState = CalibrationState::WAIT_LOAD_CELL2;
                calibStartTime = currentTime;
                
                const auto scaleKnownWeight = Config::getInstance().hardwareSensorsScaleKnownWeight.get();
                snprintf(msgBuffer, sizeof(msgBuffer), "%s%sg\n", langstring_calibrate_in_progress, number2string(scaleKnownWeight));
                displayWrappedMessage(msgBuffer);
            }
            break;
            
        case CalibrationState::WAIT_LOAD_CELL2:
            if (currentTime - calibStartTime >= 10000) {
                calibState = CalibrationState::EXECUTE_CELL2;
            }
            break;
            
        case CalibrationState::EXECUTE_CELL2: {
            HX711_ADC* loadCell = hx711Scale->getLoadCell(2);
            if (loadCell) {
                loadCell->setSamplesInUse(128);
                loadCell->refreshDataSet();
                const float calibration = loadCell->getNewCalibration(Config::getInstance().hardwareSensorsScaleKnownWeight.get());
                loadCell->setSamplesInUse(scaleSamples);
                
                hx711Scale->setCalibrationFactor(calibration, 2);
                Config::getInstance().hardwareSensorsScaleCalibration2.set(calibration);
                
                snprintf(msgBuffer, sizeof(msgBuffer), "%s%s\n", langstring_calibrate_complete, number2string(calibration));
                displayWrappedMessage(msgBuffer);
                LOGF(INFO, "Cell 2 calibration complete: %f", calibration);
            }
            
            calibState = CalibrationState::COMPLETE;
            calibStartTime = currentTime + 2000;
            break;
        }
        
        case CalibrationState::COMPLETE:
            if (currentTime >= calibStartTime) {
                g_state.sensors.scaleCalibrationOn = false;
                calibState = CalibrationState::IDLE;
                LOG(INFO, "Scale calibration completed");
            }
            break;
    }
}

inline void checkWeight() {
    if (!g_state.hardware.scale) {
        return;
    }

    g_state.sensors.currReadingWeight = getScaleWeight();

    if (g_state.sensors.scaleFailure) {
        return;
    }

    // Handle non-blocking calibration
    if (g_state.sensors.scaleCalibrationOn) {
        processNonBlockingCalibration();
        return; // Don't do normal weight reading during calibration
    }

    if (g_state.sensors.scaleTareOn) {
        static enum { TARE_START, TARE_WAIT, TARE_EXECUTE, TARE_DONE } tareState = TARE_START;
        static unsigned long tareStartTime = 0;
        
        const unsigned long currentTime = millis();
        
        switch (tareState) {
            case TARE_START:
                g_state.hardware.display->clearBuffer();
                g_state.hardware.display->drawStr(0, 2, "Taring scale,");
                g_state.hardware.display->drawStr(0, 12, "remove any load!");
                g_state.hardware.display->drawStr(0, 22, "....");
                g_state.hardware.display->sendBuffer();
                tareStartTime = currentTime;
                tareState = TARE_WAIT;
                break;
                
            case TARE_WAIT:
                if (currentTime - tareStartTime >= 2000) {
                    g_state.hardware.scale->tare();
                    tareState = TARE_EXECUTE;
                    tareStartTime = currentTime;
                }
                break;
                
            case TARE_EXECUTE:
                g_state.hardware.display->drawStr(0, 32, "done");
                g_state.hardware.display->sendBuffer();
                tareState = TARE_DONE;
                tareStartTime = currentTime;
                break;
                
            case TARE_DONE:
                if (currentTime - tareStartTime >= 2000) {
                    g_state.sensors.scaleTareOn = false;
                    tareState = TARE_START;
                }
                break;
        }
        return; // Don't do normal weight reading during tare
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
            if (g_state.machine.machineState != MachineStateId::BREW_IDLE) {
                // For Bluetooth scales with auto-tare, wait a bit before capturing pre-brew weight
                if (g_state.hardware.isBluetoothScale && g_state.sensors.autoTareInProgress) {
                    // Wait at least 2 seconds for Bluetooth tare to complete
                    if (millis() - g_state.sensors.autoTareStartTime < 2000) {
                        break;
                    }

                    g_state.sensors.autoTareInProgress = false;
                }

                g_state.sensors.preBrewWeight    = g_state.sensors.currReadingWeight;
                g_state.sensors.shottimerCounter = 20;

                // Reset fallback state at start of new brew
                g_state.sensors.brewByWeightFallbackActive = false;
            }
            break;

        case 20:
            g_state.sensors.currBrewWeight = g_state.sensors.currReadingWeight - g_state.sensors.preBrewWeight;

            if (g_state.machine.machineState == MachineStateId::BREW_IDLE) {
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
