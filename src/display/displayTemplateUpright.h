/**
 * @file displayTemplateUpright.h
 *
 * @brief Vertical display template
 *
 */

#pragma once

#include "../Config.h"
#include "../state/GlobalState.h"

/**
 * @brief Send data to display
 */
inline void printScreen() {
    const bool scaleEnabled = Config::getInstance().hardwareSensorsScaleEnabled.get();
    const bool pressureEnabled = Config::getInstance().hardwareSensorsPressureEnabled.get();
    const bool brewEnabled = Config::getInstance().hardwareSwitchesBrewEnabled.get();

    if (displayFullscreenBrewTimer()) {
        // Display was updated, end here
        return;
    }

    if (displayFullscreenManualFlushTimer()) {
        // Display was updated, end here
        return;
    }

    // Show fullscreen hot water timer:
    if (displayFullscreenHotWaterTimer()) {
        // Display was updated, end here
        return;
    }

    if (displayOfflineMode()) {
        // Display was updated, end here
        return;
    }

    g_state.hardware.display->clearBuffer();

    if (g_state.machine.machineState == kWaterTankEmpty) {
        g_state.hardware.display->drawXBMP(8, 50, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
    }
    else if (g_state.machine.machineState == kSensorError) {
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        displayMessage(langstring_error_tsensor_ur[0], langstring_error_tsensor_ur[1], String(g_state.process.temperature), langstring_error_tsensor_ur[2], langstring_error_tsensor_ur[3], langstring_error_tsensor_ur[4]);
    }
    else if (Config::getInstance().displayPidOffLogo.get() && g_state.machine.machineState == kStandby) {
        g_state.hardware.display->drawXBMP(6, 50, Off_Logo_width, Off_Logo_height, Off_Logo);
        g_state.hardware.display->setCursor(1, 110);
        g_state.hardware.display->setFont(u8g2_font_profont10_tf);
        g_state.hardware.display->print("Standby mode");
    }
    else {
        // no fullscreen states
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);
        g_state.hardware.display->setCursor(1, 14);
        g_state.hardware.display->print(langstring_current_temp_ur);
        g_state.hardware.display->print(g_state.process.temperature, 1);
        g_state.hardware.display->print(" ");
        g_state.hardware.display->print(static_cast<char>(176));
        g_state.hardware.display->print("C");
        g_state.hardware.display->setCursor(1, 24);
        g_state.hardware.display->print(langstring_set_temp_ur);
        g_state.hardware.display->print(g_state.process.setpoint, 1);
        g_state.hardware.display->print(" ");
        g_state.hardware.display->print(static_cast<char>(176));
        g_state.hardware.display->print("C");

        // Draw heat bar
        g_state.hardware.display->drawFrame(0, 124, 64, 4);
        g_state.hardware.display->drawLine(1, 125, g_state.process.pidOutput / 16.13 + 1, 125);
        g_state.hardware.display->drawLine(1, 126, g_state.process.pidOutput / 16.13 + 1, 126);

        // logos that only fill the lower half leaving temperatures, top and bottom boxes
        if (Config::getInstance().displayPidOffLogo.get() && g_state.machine.machineState == kPidDisabled) {
            g_state.hardware.display->drawXBMP(6, 50, Off_Logo_width, Off_Logo_height, Off_Logo);
            g_state.hardware.display->setCursor(1, 110);
            g_state.hardware.display->setFont(u8g2_font_profont10_tf);
            g_state.hardware.display->print("PID disabled");
        }

        // Steam
        else if (g_state.machine.machineState == kSteam) {
            g_state.hardware.display->drawXBMP(12, 50, Steam_Logo_width, Steam_Logo_height, Steam_Logo);
        }

        // Show the heating logo when we are in regular PID mode and more than 5degC below the set point
        else if (Config::getInstance().displayHeatingLogo.get() && g_state.machine.machineState == kPidNormal && g_state.process.setpoint - g_state.process.temperature > 5.0) {
            // For status info
            g_state.hardware.display->drawXBMP(12, 50, Heating_Logo_width, Heating_Logo_height, Heating_Logo);
            g_state.hardware.display->setFont(u8g2_font_fub17_tf);
            g_state.hardware.display->setCursor(8, 90);
            g_state.hardware.display->print(g_state.process.temperature, 1);
        }
        else {
            // print status
            if (scaleEnabled && pressureEnabled) {
                g_state.hardware.display->setCursor(1, 65);
            }
            else if (scaleEnabled || pressureEnabled) {
                g_state.hardware.display->setCursor(1, 60);
            }
            else {
                g_state.hardware.display->setCursor(1, 55);
            }

            g_state.hardware.display->setFont(u8g2_font_profont22_tf);

            if (g_state.machine.machineState == kManualFlush) {
                g_state.hardware.display->print("FLUSH");
            }
            else if (shouldDisplayBrewTimer()) {
                g_state.hardware.display->print("BREW");
            }
            else if (fabs(g_state.process.temperature - g_state.process.setpoint) < 0.3) {

                if (g_state.timing.isrCounter < 500) {
                    g_state.hardware.display->print("OK");
                }
                else {
                    g_state.hardware.display->print("");
                }
            }
            else {
                g_state.hardware.display->print("WAIT");
            }

            g_state.hardware.display->setFont(u8g2_font_profont11_tf);

            // PID values above heater output bar
            g_state.hardware.display->setCursor(1, 84);
            g_state.hardware.display->print("P: ");
            g_state.hardware.display->print(g_state.pid->GetKp(), 0);

            g_state.hardware.display->setCursor(1, 93);
            g_state.hardware.display->print("I: ");

            if (g_state.pid->GetKi() != 0) {
                g_state.hardware.display->print(g_state.pid->GetKp() / g_state.pid->GetKi(), 0);
            }
            else {
                g_state.hardware.display->print("0");
            }
            g_state.hardware.display->setCursor(1, 102);
            g_state.hardware.display->print("D: ");
            g_state.hardware.display->print(g_state.pid->GetKd() / g_state.pid->GetKp(), 0);
            g_state.hardware.display->setCursor(1, 111);

            if (g_state.process.pidOutput < 99) {
                g_state.hardware.display->print(g_state.process.pidOutput / 10, 1);
            }
            else {
                g_state.hardware.display->print(g_state.process.pidOutput / 10, 0);
            }

            g_state.hardware.display->print("%");

            // Brew
            if (scaleEnabled) {
                displayBrewWeight(1, 44, g_state.sensors.currReadingWeight, -1, g_state.sensors.scaleFailure);
            }

            if (pressureEnabled) {
                g_state.hardware.display->setFont(u8g2_font_profont11_tf);

                if (scaleEnabled) {
                    g_state.hardware.display->setCursor(1, 54);
                }
                else {
                    g_state.hardware.display->setCursor(1, 44);
                }

                g_state.hardware.display->print(langstring_pressure_ur);
                g_state.hardware.display->print(g_state.sensors.inputPressure, 1);
                g_state.hardware.display->print(" bar");
            }

            // Brew time
            if (brewEnabled) {
                // Show flush time
                if (g_state.machine.machineState == kManualFlush) {
                    displayBrewTime(1, 34, langstring_manual_flush_ur, g_state.process.currBrewTime);
                }
                // Show hot water time
                else if (g_state.machine.machineState == kHotWater) {
                    displayBrewTime(1, 34, langstring_hot_water_ur, currPumpOnTime);
                }
                else {
                    const bool automaticBrewingEnabled = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;

                    // Show brew time
                    if (shouldDisplayBrewTimer()) {
                        if (automaticBrewingEnabled && Config::getInstance().brewByTimeEnabled.get()) {
                            displayBrewTime(1, 34, langstring_brew_ur, g_state.process.currBrewTime, g_state.process.totalTargetBrewTime);
                        }
                        else {
                            displayBrewTime(1, 34, langstring_brew_ur, g_state.process.currBrewTime);
                        }

                        if (scaleEnabled) {
                            if (automaticBrewingEnabled && Config::getInstance().brewByWeightEnabled.get()) {
                                const auto targetBrewWeight = Config::getInstance().brewByWeightTargetWeight.get();
                                displayBrewWeight(1, 44, g_state.sensors.currBrewWeight, targetBrewWeight, g_state.sensors.scaleFailure);
                            }
                            else {
                                displayBrewWeight(1, 44, g_state.sensors.currBrewWeight, -1, g_state.sensors.scaleFailure);
                            }
                        }
                    }
                }
            }
        }

        // Status info in top bar
        g_state.hardware.display->drawLine(0, 12, 64, 12);
        if (!g_state.network.offlineMode) {
            displayWiFiStatus(4, 2);
            displayMQTTStatus(21, 0);
        }
        else {
            g_state.hardware.display->setCursor(4, 1);
            g_state.hardware.display->setFont(u8g2_font_profont11_tf);
            g_state.hardware.display->print(langstring_offlinemode);
        }

        if (Config::getInstance().hardwareSensorsScaleEnabled.get() && Config::getInstance().hardwareSensorsScaleType.get() == Hardware::ScaleType::BLUETOOTH) {
            displayBluetoothStatus(54, 1);
        }
    }

    g_state.coordination.displayBufferReady = true;
}
