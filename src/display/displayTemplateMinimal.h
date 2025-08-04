/**
 * @file displayTemplateMinimal.h
 *
 * @brief Minimal display template
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

    // If no specific machine state was printed, print default:
    u8g2->clearBuffer();

    displayStatusbar();

    int numDecimalsInput = 1;

    if (g_state.process.temperature > 99.999) {
        numDecimalsInput = 0;
    }

    int numDecimalsSetpoint = 1;

    if (g_state.process.setpoint > 99.999) {
        numDecimalsSetpoint = 0;
    }

    // Draw temp, blink if feature STATUS_LED is not enabled
    if (fabs(g_state.process.temperature - g_state.process.setpoint) < 0.3 && !Config::getInstance().get<bool>("hardware.leds.status.enabled")) {
        if (isrCounter < 500) {
            // limit to 4 characters
            u8g2->setCursor(2, 20);
            u8g2->setFont(u8g2_font_profont22_tf);
            u8g2->print(g_state.process.temperature, numDecimalsInput);
            u8g2->setFont(u8g2_font_open_iconic_arrow_2x_t);
            u8g2->print(static_cast<char>(78));
            u8g2->setCursor(78, 20);
            u8g2->setFont(u8g2_font_profont22_tf);
            u8g2->print(g_state.process.setpoint, numDecimalsSetpoint);
        }
    }
    else {
        u8g2->setCursor(2, 20);
        u8g2->setFont(u8g2_font_profont22_tf);
        u8g2->print(g_state.process.temperature, numDecimalsInput);
        u8g2->setFont(u8g2_font_open_iconic_arrow_2x_t);
        u8g2->setCursor(56, 24);

        if (g_state.pid->GetMode() == 1) {
            u8g2->print(static_cast<char>(74));
        }
        else {
            u8g2->print(static_cast<char>(70));
        }

        u8g2->setCursor(79, 20);
        u8g2->setFont(u8g2_font_profont22_tf);
        u8g2->print(g_state.process.setpoint, numDecimalsSetpoint);
    }

    u8g2->setFont(u8g2_font_profont11_tf);

    // Brew time
    if (Config::getInstance().get<bool>("hardware.switches.brew.enabled")) {
        // Show flush time
        if (machineState == kManualFlush) {
            u8g2->setCursor(34, 44);
            u8g2->print(langstring_manual_flush);
            u8g2->print(g_state.process.currBrewTime / 1000, 0);
        }
        // Show hot water time
        else if (machineState == kHotWater) {
            u8g2->setCursor(34, 44);
            u8g2->print(langstring_hot_water);
            u8g2->print(currPumpOnTime / 1000, 0);
        }
        else {
            if (shouldDisplayBrewTimer()) {
                u8g2->setCursor(34, 44);
                u8g2->print(langstring_brew);
                u8g2->print(g_state.process.currBrewTime / 1000, 0);

                if (Config::getInstance().get<bool>("brew.by_time.enabled") && Config::getInstance().get<int>("brew.mode") == 1) {
                    u8g2->print("/");
                    u8g2->print(g_state.process.totalTargetBrewTime / 1000, 0);
                }
            }
        }
    }

    // Show heater output in %
    displayProgressbar(g_state.process.pidOutput / 10, 15, 60, 100);

    g_state.coordination.displayBufferReady = true;
}
