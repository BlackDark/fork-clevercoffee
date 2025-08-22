/**
 * @file scaleHandler.cpp
 * @brief Implementation of scale handling functions
 */

#include "clevercoffee/scaleHandler.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/hardware/pinmapping.h"
#include "clevercoffee/hardware/scales/BluetoothScale.h"
#include "clevercoffee/hardware/scales/HX711Scale.h"

// Simple stub to avoid circular dependency with displayCommon.h
void displayScaleFailed() {
    // This function is defined in displayCommon.h but to avoid circular includes,
    // we provide a stub here. The actual display error handling happens elsewhere.
    LOG(ERROR, "Scale display failed - stub function called");
}

void initScale() {
    const Hardware::ScaleType scaleType    = Config::getInstance().hardwareSensorsScaleType.get();
    const int                 scaleSamples = Config::getInstance().hardwareSensorsScaleSamples.get();

    // Clean up existing scale (unique_ptr automatically deletes)
    g_state.hardware.scale.reset();

    if (scaleType == Hardware::ScaleType::BLUETOOTH) { // Bluetooth scale
        g_state.hardware.scale            = std::make_unique<BluetoothScale>();
        g_state.hardware.isBluetoothScale = true;

        LOG(INFO, "Initializing Bluetooth scale");

        g_state.hardware.scale->init();
    } else {
        // HX711 scale types
        const float cal1 = Config::getInstance().hardwareSensorsScaleCalibration.get();
        const float cal2 = Config::getInstance().hardwareSensorsScaleCalibration2.get();

        if (scaleType == Hardware::ScaleType::HX711_DUAL) { // Dual load cell
            g_state.hardware.scale = std::make_unique<HX711Scale>(PIN_HXDAT, PIN_HXDAT2, PIN_HXCLK, cal1, cal2);
        } else {                                            // Single load cell
            g_state.hardware.scale = std::make_unique<HX711Scale>(PIN_HXDAT, PIN_HXCLK, cal1);
        }

        g_state.hardware.isBluetoothScale = false;
        LOG(INFO, "Initializing HX711 scale");

        if (!g_state.hardware.scale->init()) {
            LOG(ERROR, "Scale initialization failed");
            displayScaleFailed();
            // Non-blocking: mark failure and continue - don't freeze the main loop
            g_state.sensors.scaleFailure = true;
            g_state.hardware.scale.reset();
            return;
        }

        // Set samples for HX711 scales
        g_state.hardware.scale->setSamples(scaleSamples);
    }

    // Reset connection state
    g_state.sensors.scaleConnectionLost        = false;
    g_state.sensors.scaleFailure               = false;
    g_state.sensors.brewByWeightFallbackActive = false;
    g_state.sensors.lastScaleConnectionCheck   = 0;
    g_state.sensors.scaleConnectionFailureTime = 0;
    g_state.sensors.lastValidWeight            = 0;

    // Reset error handling state
    g_state.sensors.scaleReadErrorCount  = 0;
    g_state.sensors.lastScaleErrorTime   = 0;
    g_state.sensors.scaleInErrorRecovery = false;

    g_state.sensors.scaleCalibrationOn = false;

    LOG(INFO, "Scale initialized successfully");
}
