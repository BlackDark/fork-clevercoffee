/**
 * @file displayTemplateStandard.h
 *
 * @brief Standard display template
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
    g_state.hardware.display->setFont(u8g2_font_profont11_tf); // set font

    displayStatusbar();

    g_state.hardware.display->setCursor(34, 16);
    g_state.hardware.display->print(langstring_current_temp);
    g_state.hardware.display->setCursor(84, 16);
    g_state.hardware.display->print(g_state.process.temperature, 1);
    g_state.hardware.display->setCursor(115, 16);
    g_state.hardware.display->print(static_cast<char>(176));
    g_state.hardware.display->print("C");
    g_state.hardware.display->setCursor(34, 26);
    g_state.hardware.display->print(langstring_set_temp);
    g_state.hardware.display->setCursor(84, 26);
    g_state.hardware.display->print(g_state.process.setpoint, 1);
    g_state.hardware.display->setCursor(115, 26);
    g_state.hardware.display->print(static_cast<char>(176));
    g_state.hardware.display->print("C");

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

    // Brew and flush time
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        // Show flush time
        if (g_state.machine.machineState == kManualFlush) {
            displayBrewTime(34, 36, langstring_manual_flush, g_state.process.currBrewTime);
        }
        // Show hot water time
        else if (g_state.machine.machineState == kHotWater) {
            displayBrewTime(34, 36, langstring_hot_water, currPumpOnTime);
        }
        else {
            if (shouldDisplayBrewTimer()) {
                if (Config::getInstance().brewByTimeEnabled.get() && Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW) {
                    displayBrewTime(34, 36, langstring_brew, g_state.process.currBrewTime, g_state.process.totalTargetBrewTime);
                }
                else {
                    displayBrewTime(34, 36, langstring_brew, g_state.process.currBrewTime);
                }
            }
        }
    }

    // PID values over heat bar
    g_state.hardware.display->setCursor(38, 47);

    g_state.hardware.display->print(g_state.pid->GetKp(), 0);
    g_state.hardware.display->print("|");

    if (g_state.pid->GetKi() != 0) {
        g_state.hardware.display->print(g_state.pid->GetKp() / g_state.pid->GetKi(), 0);
    }
    else {
        g_state.hardware.display->print("0");
    }

    g_state.hardware.display->print("|");
    g_state.hardware.display->print(g_state.pid->GetKd() / g_state.pid->GetKp(), 0);
    g_state.hardware.display->setCursor(96, 47);

    if (g_state.process.pidOutput < 99) {
        g_state.hardware.display->print(g_state.process.pidOutput / 10, 1);
    }
    else {
        g_state.hardware.display->print(g_state.process.pidOutput / 10, 0);
    }

    g_state.hardware.display->print("%");

    // Show heater output in %
    displayProgressbar(g_state.process.pidOutput / 10, 30, 60, 98);

    g_state.coordination.displayBufferReady = true;
}
