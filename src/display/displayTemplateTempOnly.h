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
    u8g2->clearBuffer();

    // draw (blinking) temp
    if (((fabs(g_state.process.temperature - g_state.process.setpoint) < blinkingTempDelta && blinkingTemp == 0) || fabs(g_state.process.temperature - g_state.process.setpoint) >= blinkingTempDelta) && !Config::getInstance().get<bool>("hardware.leds.status.enabled")) {
        if (isrCounter < 500) {
            if (g_state.process.temperature < 99.999) {
                u8g2->setCursor(8, 22);
                u8g2->setFont(u8g2_font_fub35_tf);
                u8g2->print(g_state.process.temperature, 1);
                u8g2->drawCircle(116, 27, 4);
            }
            else {
                u8g2->setCursor(24, 22);
                u8g2->setFont(u8g2_font_fub35_tf);
                u8g2->print(g_state.process.temperature, 0);
                u8g2->drawCircle(116, 27, 4);
            }
        }
    }
    else {
        if (g_state.process.temperature < 99.999) {
            u8g2->setCursor(8, 22);
            u8g2->setFont(u8g2_font_fub35_tf);
            u8g2->print(g_state.process.temperature, 1);
            u8g2->drawCircle(116, 27, 4);
        }
        else {
            u8g2->setCursor(24, 22);
            u8g2->setFont(u8g2_font_fub35_tf);
            u8g2->print(g_state.process.temperature, 0);
            u8g2->drawCircle(116, 27, 4);
        }
    }

    displayStatusbar();

    g_state.coordination.displayBufferReady = true;
}
