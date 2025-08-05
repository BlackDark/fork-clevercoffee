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
    g_state.hardware.display->clearBuffer();

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
    if (fabs(g_state.process.temperature - g_state.process.setpoint) < 0.3 && !Config::getInstance().hardwareLedsStatusEnabled.get()) {
        if (g_state.timing.isrCounter < 500) {
            // limit to 4 characters
            g_state.hardware.display->setCursor(2, 20);
            g_state.hardware.display->setFont(u8g2_font_profont22_tf);
            g_state.hardware.display->print(g_state.process.temperature, numDecimalsInput);
            g_state.hardware.display->setFont(u8g2_font_open_iconic_arrow_2x_t);
            g_state.hardware.display->print(static_cast<char>(78));
            g_state.hardware.display->setCursor(78, 20);
            g_state.hardware.display->setFont(u8g2_font_profont22_tf);
            g_state.hardware.display->print(g_state.process.setpoint, numDecimalsSetpoint);
        }
    }
    else {
        g_state.hardware.display->setCursor(2, 20);
        g_state.hardware.display->setFont(u8g2_font_profont22_tf);
        g_state.hardware.display->print(g_state.process.temperature, numDecimalsInput);
        g_state.hardware.display->setFont(u8g2_font_open_iconic_arrow_2x_t);
        g_state.hardware.display->setCursor(56, 24);

        if (g_state.pid->GetMode() == 1) {
            g_state.hardware.display->print(static_cast<char>(74));
        }
        else {
            g_state.hardware.display->print(static_cast<char>(70));
        }

        g_state.hardware.display->setCursor(79, 20);
        g_state.hardware.display->setFont(u8g2_font_profont22_tf);
        g_state.hardware.display->print(g_state.process.setpoint, numDecimalsSetpoint);
    }

    g_state.hardware.display->setFont(u8g2_font_profont11_tf);

    // Brew time
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        // Show flush time
        if (g_state.machine.machineState == kManualFlush) {
            g_state.hardware.display->setCursor(34, 44);
            g_state.hardware.display->print(langstring_manual_flush);
            g_state.hardware.display->print(g_state.process.currBrewTime / 1000, 0);
        }
        // Show hot water time
        else if (g_state.machine.machineState == kHotWater) {
            g_state.hardware.display->setCursor(34, 44);
            g_state.hardware.display->print(langstring_hot_water);
            g_state.hardware.display->print(currPumpOnTime / 1000, 0);
        }
        else {
            if (shouldDisplayBrewTimer()) {
                g_state.hardware.display->setCursor(34, 44);
                g_state.hardware.display->print(langstring_brew);
                g_state.hardware.display->print(g_state.process.currBrewTime / 1000, 0);

                if (Config::getInstance().brewByTimeEnabled.get() && Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW) {
                    g_state.hardware.display->print("/");
                    g_state.hardware.display->print(g_state.process.totalTargetBrewTime / 1000, 0);
                }
            }
        }
    }

    // Show heater output in %
    displayProgressbar(g_state.process.pidOutput / 10, 15, 60, 100);

    g_state.coordination.displayBufferReady = true;
}
