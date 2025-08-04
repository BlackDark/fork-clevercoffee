/**
 * @file displayTemplateTempOnly.h
 *
 * @brief Temp-only display template
 *
 */

#pragma once

#include "../Config.h"
#include "../state/GlobalState.h"

// Define some Displayoptions
inline constexpr int blinkingTemp = 1;          // 0: blinking near setpoint, 1: blinking far away from setpoint
inline constexpr float blinkingTempDelta = 0.3; // Delta from setpoint for blinking temperature display

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

    // draw (blinking) temp
    if (((fabs(g_state.process.temperature - g_state.process.setpoint) < blinkingTempDelta && blinkingTemp == 0) || fabs(g_state.process.temperature - g_state.process.setpoint) >= blinkingTempDelta) &&
        !Config::getInstance().get<bool>("hardware.leds.status.enabled")) {
        if (g_state.timing.isrCounter < 500) {
            if (g_state.process.temperature < 99.999) {
                g_state.hardware.display->setCursor(8, 22);
                g_state.hardware.display->setFont(u8g2_font_fub35_tf);
                g_state.hardware.display->print(g_state.process.temperature, 1);
                g_state.hardware.display->drawCircle(116, 27, 4);
            }
            else {
                g_state.hardware.display->setCursor(24, 22);
                g_state.hardware.display->setFont(u8g2_font_fub35_tf);
                g_state.hardware.display->print(g_state.process.temperature, 0);
                g_state.hardware.display->drawCircle(116, 27, 4);
            }
        }
    }
    else {
        if (g_state.process.temperature < 99.999) {
            g_state.hardware.display->setCursor(8, 22);
            g_state.hardware.display->setFont(u8g2_font_fub35_tf);
            g_state.hardware.display->print(g_state.process.temperature, 1);
            g_state.hardware.display->drawCircle(116, 27, 4);
        }
        else {
            g_state.hardware.display->setCursor(24, 22);
            g_state.hardware.display->setFont(u8g2_font_fub35_tf);
            g_state.hardware.display->print(g_state.process.temperature, 0);
            g_state.hardware.display->drawCircle(116, 27, 4);
        }
    }

    displayStatusbar();

    g_state.coordination.displayBufferReady = true;
}
