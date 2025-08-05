/**
 * @file displayTemplateScale.h
 *
 * @brief Display template with brew scale
 *
 */

#pragma once

#include "../Config.h"
#include "../state/GlobalState.h"

/**
 * @brief Send data to display
 */
inline void printScreen() {

    // Show fullscreen brew timer:
    if (displayFullscreenBrewTimer()) {
        // Display was updated, end here
        return;
    }

    // Show fullscreen manual flush timer:
    if (displayFullscreenManualFlushTimer()) {
        // Display was updated, end here
        return;
    }

    // Show fullscreen hot water timer:
    if (displayFullscreenHotWaterTimer()) {
        // Display was updated, end here
        return;
    }

    // Print the machine state
    if (displayMachineState()) {
        // Display was updated, end here
        return;
    }

    // If no specific machine state was printed, print default:
    g_state.hardware.display->clearBuffer();

    displayStatusbar();

    displayThermometerOutline(4, 62);

    // Draw current temp in thermometer
    if (fabs(g_state.process.temperature - g_state.process.setpoint) < 0.3) {
        if (g_state.timing.isrCounter < 500) {
            drawTemperaturebar(8, 30);
        }
    }
    else {
        drawTemperaturebar(8, 30);
    }

    // Draw current temp and temp setpoint
    g_state.hardware.display->setFont(u8g2_font_profont11_tf);

    g_state.hardware.display->setCursor(32, 16);
    g_state.hardware.display->print("T: ");
    g_state.hardware.display->print(g_state.process.temperature, 1);
    g_state.hardware.display->print("/");
    g_state.hardware.display->print(g_state.process.setpoint, 1);
    g_state.hardware.display->print(static_cast<char>(176));
    g_state.hardware.display->print("C");

    const bool scaleEnabled = Config::getInstance().hardwareSensorsScaleEnabled.get();

    if (scaleEnabled) {
        // Show current weight if scale has no error
        displayBrewWeight(32, 26, g_state.sensors.currReadingWeight, -1, g_state.sensors.scaleFailure);
    }

    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        // Show flush time
        if (g_state.machine.machineState == kManualFlush) {
            displayBrewTime(32, 36, langstring_manual_flush, g_state.process.currBrewTime);
        }
        // Show hot water time
        else if (g_state.machine.machineState == kHotWater) {
            displayBrewTime(32, 36, langstring_hot_water, currPumpOnTime);
        }
        else if (shouldDisplayBrewTimer()) {
            const bool automaticBrewingEnabled = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;

            // Time
            if (automaticBrewingEnabled && Config::getInstance().brewByTimeEnabled.get()) {
                displayBrewTime(32, 36, langstring_brew, g_state.process.currBrewTime, g_state.process.totalTargetBrewTime);
            }
            else {
                displayBrewTime(32, 36, langstring_brew, g_state.process.currBrewTime);
            }

            // Weight
            if (scaleEnabled) {
                if (automaticBrewingEnabled && Config::getInstance().brewByWeightEnabled.get()) {
                    const auto targetBrewWeight = Config::getInstance().brewByWeightTargetWeight.get();
                    displayBrewWeight(32, 26, g_state.sensors.currBrewWeight, targetBrewWeight, g_state.sensors.scaleFailure);
                }
                else {
                    displayBrewWeight(32, 26, g_state.sensors.currBrewWeight, -1, g_state.sensors.scaleFailure);
                }
            }
        }
    }

    if (Config::getInstance().hardwareSensorsPressureEnabled.get()) {
        g_state.hardware.display->setCursor(32, 46);
        g_state.hardware.display->drawUTF8(32, 46, langstring_pressure);
        int labelWidth = g_state.hardware.display->getUTF8Width(langstring_pressure);
        g_state.hardware.display->setCursor(32 + labelWidth, 46);
        g_state.hardware.display->print(g_state.sensors.inputPressure, 1);
    }

    // Show heater output in %
    displayProgressbar(g_state.process.pidOutput / 10, 30, 60, 98);

    g_state.coordination.displayBufferReady = true;
}
