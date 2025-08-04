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
    const bool scaleEnabled = Config::getInstance().get<bool>("hardware.sensors.scale.enabled");
    const bool pressureEnabled = Config::getInstance().get<bool>("hardware.sensors.pressure.enabled");
    const bool brewEnabled = Config::getInstance().get<bool>("hardware.switches.brew.enabled");

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

    u8g2->clearBuffer();
    if (machineState == kWaterTankEmpty) {
        u8g2->drawXBMP(8, 50, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
    }
    else if (machineState == kSensorError) {
        u8g2->setFont(u8g2_font_profont11_tf);
        displayMessage(langstring_error_tsensor_ur[0], langstring_error_tsensor_ur[1], String(g_state.process.temperature), langstring_error_tsensor_ur[2], langstring_error_tsensor_ur[3], langstring_error_tsensor_ur[4]);
    }
    else if (Config::getInstance().get<bool>("display.pid_off_logo") && machineState == kStandby) {
        u8g2->drawXBMP(6, 50, Off_Logo_width, Off_Logo_height, Off_Logo);
        u8g2->setCursor(1, 110);
        u8g2->setFont(u8g2_font_profont10_tf);
        u8g2->print("Standby mode");
    }
    else {
        // no fullscreen states
        u8g2->setFont(u8g2_font_profont11_tf);
        u8g2->setCursor(1, 14);
        u8g2->print(langstring_current_temp_ur);
        u8g2->print(g_state.process.temperature, 1);
        u8g2->print(" ");
        u8g2->print(static_cast<char>(176));
        u8g2->print("C");
        u8g2->setCursor(1, 24);
        u8g2->print(langstring_set_temp_ur);
        u8g2->print(g_state.process.setpoint, 1);
        u8g2->print(" ");
        u8g2->print(static_cast<char>(176));
        u8g2->print("C");

        // Draw heat bar
        u8g2->drawFrame(0, 124, 64, 4);
        u8g2->drawLine(1, 125, g_state.process.pidOutput / 16.13 + 1, 125);
        u8g2->drawLine(1, 126, g_state.process.pidOutput / 16.13 + 1, 126);

        // logos that only fill the lower half leaving temperatures, top and bottom boxes
        if (Config::getInstance().get<bool>("display.pid_off_logo") && machineState == kPidDisabled) {
            u8g2->drawXBMP(6, 50, Off_Logo_width, Off_Logo_height, Off_Logo);
            u8g2->setCursor(1, 110);
            u8g2->setFont(u8g2_font_profont10_tf);
            u8g2->print("PID disabled");
        }

        // Steam
        else if (machineState == kSteam) {
            u8g2->drawXBMP(12, 50, Steam_Logo_width, Steam_Logo_height, Steam_Logo);
        }

        // Show the heating logo when we are in regular PID mode and more than 5degC below the set point
        else if (Config::getInstance().get<bool>("display.heating_logo") && machineState == kPidNormal && g_state.process.setpoint - g_state.process.temperature > 5.0) {
            // For status info
            u8g2->drawXBMP(12, 50, Heating_Logo_width, Heating_Logo_height, Heating_Logo);
            u8g2->setFont(u8g2_font_fub17_tf);
            u8g2->setCursor(8, 90);
            u8g2->print(g_state.process.temperature, 1);
        }
        else {
            // print status
            if (scaleEnabled && pressureEnabled) {
                u8g2->setCursor(1, 65);
            }
            else if (scaleEnabled || pressureEnabled) {
                u8g2->setCursor(1, 60);
            }
            else {
                u8g2->setCursor(1, 55);
            }

            u8g2->setFont(u8g2_font_profont22_tf);

            if (machineState == kManualFlush) {
                u8g2->print("FLUSH");
            }
            else if (shouldDisplayBrewTimer()) {
                u8g2->print("BREW");
            }
            else if (fabs(g_state.process.temperature - g_state.process.setpoint) < 0.3) {

                if (isrCounter < 500) {
                    u8g2->print("OK");
                }
                else {
                    u8g2->print("");
                }
            }
            else {
                u8g2->print("WAIT");
            }

            u8g2->setFont(u8g2_font_profont11_tf);

            // PID values above heater output bar
            u8g2->setCursor(1, 84);
            u8g2->print("P: ");
            u8g2->print(g_state.pid->GetKp(), 0);

            u8g2->setCursor(1, 93);
            u8g2->print("I: ");

            if (g_state.pid->GetKi() != 0) {
                u8g2->print(g_state.pid->GetKp() / g_state.pid->GetKi(), 0);
            }
            else {
                u8g2->print("0");
            }
            u8g2->setCursor(1, 102);
            u8g2->print("D: ");
            u8g2->print(g_state.pid->GetKd() / g_state.pid->GetKp(), 0);
            u8g2->setCursor(1, 111);

            if (g_state.process.pidOutput < 99) {
                u8g2->print(g_state.process.pidOutput / 10, 1);
            }
            else {
                u8g2->print(g_state.process.pidOutput / 10, 0);
            }

            u8g2->print("%");

            // Brew
            if (scaleEnabled) {
                displayBrewWeight(1, 44, currReadingWeight, -1, scaleFailure);
            }

            if (pressureEnabled) {
                u8g2->setFont(u8g2_font_profont11_tf);

                if (scaleEnabled) {
                    u8g2->setCursor(1, 54);
                }
                else {
                    u8g2->setCursor(1, 44);
                }

                u8g2->print(langstring_pressure_ur);
                u8g2->print(g_state.sensors.inputPressure, 1);
                u8g2->print(" bar");
            }

            // Brew time
            if (brewEnabled) {
                // Show flush time
                if (machineState == kManualFlush) {
                    displayBrewTime(1, 34, langstring_manual_flush_ur, g_state.process.currBrewTime);
                }
                // Show hot water time
                else if (machineState == kHotWater) {
                    displayBrewTime(1, 34, langstring_hot_water_ur, currPumpOnTime);
                }
                else {
                    const bool automaticBrewingEnabled = Config::getInstance().get<bool>("brew.mode") == 1;

                    // Show brew time
                    if (shouldDisplayBrewTimer()) {
                        if (automaticBrewingEnabled && Config::getInstance().get<bool>("brew.by_time.enabled")) {
                            displayBrewTime(1, 34, langstring_brew_ur, g_state.process.currBrewTime, g_state.process.totalTargetBrewTime);
                        }
                        else {
                            displayBrewTime(1, 34, langstring_brew_ur, g_state.process.currBrewTime);
                        }

                        if (scaleEnabled) {
                            if (automaticBrewingEnabled && Config::getInstance().get<bool>("brew.by_weight.enabled")) {
                                const auto targetBrewWeight = Config::getInstance().get<double>("brew.by_weight.target_weight");
                                displayBrewWeight(1, 44, currBrewWeight, targetBrewWeight, scaleFailure);
                            }
                            else {
                                displayBrewWeight(1, 44, currBrewWeight, -1, scaleFailure);
                            }
                        }
                    }
                }
            }
        }

        // Status info in top bar
        u8g2->drawLine(0, 12, 64, 12);
        if (!g_state.network.offlineMode) {
            displayWiFiStatus(4, 2);
            displayMQTTStatus(21, 0);
        }
        else {
            u8g2->setCursor(4, 1);
            u8g2->setFont(u8g2_font_profont11_tf);
            u8g2->print(langstring_offlinemode);
        }

        if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled") && Config::getInstance().get<int>("hardware.sensors.scale.type") == 2) {
            displayBluetoothStatus(54, 1);
        }
    }

    g_state.coordination.displayBufferReady = true;
}
