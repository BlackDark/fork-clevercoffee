/**
 * @file ModernDisplayTemplate.h
 * @brief Modern C++23 display template system using "deducing this"
 */

#pragma once

#include "clevercoffee/constants/Display.h"
#include "clevercoffee/display/displayCommon.h"
#include "clevercoffee/state/MachineStateIds.h"

/**
 * @brief Modern C++20 display template base using CRTP
 *
 * This eliminates code duplication across display templates by providing
 * common functionality while allowing specialized template behavior.
 * Uses traditional CRTP for maximum compatibility with zero-cost abstractions.
 */
template <typename Derived>
class ModernDisplayTemplate {
  public:
    /**
     * @brief Main screen rendering method using CRTP
     */
    void printScreen() {
        // Common early returns for fullscreen modes
        if (handleFullscreenModes()) {
            return;
        }

        // Handle machine-specific states
        if (handleMachineStates()) {
            return;
        }

        // Render normal template-specific display
        static_cast<Derived*>(this)->renderNormalDisplay();

        // Common finalization
        g_state.coordination.displayBufferReady = true;
    }

    /**
     * @brief Handle fullscreen display modes (common across all templates)
     */
    bool handleFullscreenModes() {
        if (displayFullscreenBrewTimer()) return true;
        if (displayFullscreenManualFlushTimer()) return true;
        if (displayFullscreenHotWaterTimer()) return true;

        // Default offline mode handling
        if (displayOfflineMode()) return true;

        return false;
    }

    /**
     * @brief Handle machine state displays (common logic, template-specific rendering)
     */
    bool handleMachineStates() {
        // Default machine state handling
        return displayMachineState();
    }

    /**
     * @brief Common temperature display with template-specific positioning
     */
    void displayTemperatureInfo(int baseX, int baseY) {
        g_state.hardware.display->setFont(u8g2_font_profont11_tf);

        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getTemperatureCoords(baseX, baseY);

        // Current temperature
        g_state.hardware.display->setCursor(coords.currentTempX, coords.currentTempY);
        g_state.hardware.display->print(derived->getCurrentTempLabel());
        g_state.hardware.display->setCursor(coords.currentValueX, coords.currentTempY);
        g_state.hardware.display->print(g_state.process.temperature, 1);
        g_state.hardware.display->print(" ");
        g_state.hardware.display->print(static_cast<char>(176));
        g_state.hardware.display->print("C");

        // Set temperature
        g_state.hardware.display->setCursor(coords.setTempX, coords.setTempY);
        g_state.hardware.display->print(derived->getSetTempLabel());
        g_state.hardware.display->setCursor(coords.setValueX, coords.setTempY);
        g_state.hardware.display->print(g_state.process.setpoint, 1);
        g_state.hardware.display->print(" ");
        g_state.hardware.display->print(static_cast<char>(176));
        g_state.hardware.display->print("C");
    }

    /**
     * @brief Common PID display with template-specific formatting
     */
    void displayPIDInfo(int baseX, int baseY) {
        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getPIDCoords(baseX, baseY);

        g_state.hardware.display->setCursor(coords.pidX, coords.pidY);
        g_state.hardware.display->print(g_state.pid->GetKp(), 0);
        g_state.hardware.display->print(derived->getPIDSeparator());

        if (g_state.pid->GetKi() != 0) {
            g_state.hardware.display->print(g_state.pid->GetKp() / g_state.pid->GetKi(), 0);
        } else {
            g_state.hardware.display->print("0");
        }

        g_state.hardware.display->print(derived->getPIDSeparator());
        g_state.hardware.display->print(g_state.pid->GetKd() / g_state.pid->GetKp(), 0);

        // Output percentage
        g_state.hardware.display->setCursor(coords.outputX, coords.outputY);
        if (g_state.process.pidOutput < 99) {
            g_state.hardware.display->print(g_state.process.pidOutput / 10, 1);
        } else {
            g_state.hardware.display->print(g_state.process.pidOutput / 10, 0);
        }
        g_state.hardware.display->print("%");
    }

    /**
     * @brief Common brew time display with template-specific positioning
     */
    void displayBrewInfo(int baseX, int baseY) {
        if (!Config::getInstance().hardwareSwitchesBrewEnabled.get()) return;

        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getBrewCoords(baseX, baseY);

        // Show flush time
        if (isManualFlushState(g_state.machine.machineState)) {
            displayBrewTime(coords.brewX, coords.brewY, derived->getManualFlushLabel(), g_state.process.currBrewTime);
        }
        // Show hot water time
        else if (isHotWaterState(g_state.machine.machineState)) {
            displayBrewTime(coords.brewX, coords.brewY, derived->getHotWaterLabel(), g_state.sensors.currPumpOnTime);
        } else if (shouldDisplayBrewTimer()) {
            const bool isAutomatic = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
            const bool brewByTime  = Config::getInstance().brewByTimeEnabled.get();

            if (isAutomatic && brewByTime) {
                displayBrewTime(coords.brewX,
                                coords.brewY,
                                derived->getBrewLabel(),
                                g_state.process.currBrewTime,
                                g_state.process.totalTargetBrewTime);
            } else {
                displayBrewTime(coords.brewX, coords.brewY, derived->getBrewLabel(), g_state.process.currBrewTime);
            }
        }
    }

  protected:
    /**
     * @brief Coordinate structures for different template layouts
     */
    struct TemperatureCoords {
        int currentTempX, currentTempY;
        int currentValueX;
        int setTempX, setTempY;
        int setValueX;
    };

    struct PIDCoords {
        int pidX, pidY;
        int outputX, outputY;
    };

    struct BrewCoords {
        int brewX, brewY;
    };
};

/**
 * @brief Standard horizontal layout template
 */
class StandardTemplate : public ModernDisplayTemplate<StandardTemplate> {
  public:
    void renderNormalDisplay() {
        g_state.hardware.display->clearBuffer();

        displayStatusbar();
        displayTemperatureInfo(34, 16);

        // Thermometer
        displayThermometerOutline(4, 62);
        if (fabs(g_state.process.temperature - g_state.process.setpoint) < 0.3) {
            if (g_state.timing.isrCounter < 500) {
                drawTemperaturebar(8, 30);
            }
        } else {
            drawTemperaturebar(8, 30);
        }

        displayBrewInfo(34, 36);
        displayPIDInfo(38, 47);

        // Heat bar
        displayProgressbar(g_state.process.pidOutput / 10, 30, 60, 98);
    }

    // Template-specific configuration methods
    TemperatureCoords getTemperatureCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, 84, baseX, baseY + 10, 84};
    }

    PIDCoords getPIDCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, 96, baseY};
    }

    BrewCoords getBrewCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY};
    }

    const char* getCurrentTempLabel() const noexcept {
        return langstring_current_temp;
    }
    const char* getSetTempLabel() const noexcept {
        return langstring_set_temp;
    }
    const char* getBrewLabel() const noexcept {
        return langstring_brew;
    }
    const char* getManualFlushLabel() const noexcept {
        return langstring_manual_flush;
    }
    const char* getHotWaterLabel() const noexcept {
        return langstring_hot_water;
    }
    const char* getPIDSeparator() const noexcept {
        return "|";
    }
};

/**
 * @brief Upright vertical layout template
 */
class UprightTemplate : public ModernDisplayTemplate<UprightTemplate> {
  public:
    void renderNormalDisplay() {
        g_state.hardware.display->clearBuffer();

        if (handleSpecialStates()) return;

        // Normal display
        displayTemperatureInfo(1, 14);
        displayHeatBar();
        displayMainStatus();
        displayBrewInfo(1, 34);
        displaySensorInfo();
        displayStatusBar();
    }

    bool handleSpecialStates() {
        if (g_state.machine.machineState == MachineStateId::WATER_TANK_EMPTY) {
            g_state.hardware.display->drawXBMP(
                8, 50, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
            return true;
        }

        if (g_state.machine.machineState == MachineStateId::SENSOR_ERROR) {
            g_state.hardware.display->setFont(u8g2_font_profont11_tf);
            char tempBuffer[16];
            snprintf(tempBuffer, sizeof(tempBuffer), "%.1f", g_state.process.temperature);
            displayMessage(langstring_error_tsensor_ur[0],
                           langstring_error_tsensor_ur[1],
                           tempBuffer,
                           langstring_error_tsensor_ur[2],
                           langstring_error_tsensor_ur[3],
                           langstring_error_tsensor_ur[4]);
            return true;
        }

        if (Config::getInstance().displayPidOffLogo.get() && g_state.machine.machineState == MachineStateId::STANDBY) {
            g_state.hardware.display->drawXBMP(6, 50, Off_Logo_width, Off_Logo_height, Off_Logo);
            g_state.hardware.display->setCursor(1, 110);
            g_state.hardware.display->setFont(u8g2_font_profont10_tf);
            g_state.hardware.display->print("Standby mode");
            return true;
        }

        return false;
    }

  private:
    void displayHeatBar() {
        g_state.hardware.display->drawFrame(0, 124, 64, 4);
        g_state.hardware.display->drawLine(1, 125, g_state.process.pidOutput / 16.13 + 1, 125);
        g_state.hardware.display->drawLine(1, 126, g_state.process.pidOutput / 16.13 + 1, 126);
    }

    void displayMainStatus() {
        const bool scaleEnabled    = Config::getInstance().hardwareSensorsScaleEnabled.get();
        const bool pressureEnabled = Config::getInstance().hardwareSensorsPressureEnabled.get();

        int yPos = scaleEnabled && pressureEnabled ? 65 : (scaleEnabled || pressureEnabled ? 60 : 55);
        g_state.hardware.display->setCursor(1, yPos);
        g_state.hardware.display->setFont(u8g2_font_profont22_tf);

        if (isManualFlushState(g_state.machine.machineState)) {
            g_state.hardware.display->print("FLUSH");
        } else if (shouldDisplayBrewTimer()) {
            g_state.hardware.display->print("BREW");
        } else if (fabs(g_state.process.temperature - g_state.process.setpoint) < 0.3) {
            if (g_state.timing.isrCounter < 500) {
                g_state.hardware.display->print("OK");
            }
        } else {
            g_state.hardware.display->print("WAIT");
        }
    }

    void displaySensorInfo() {
        const bool scaleEnabled    = Config::getInstance().hardwareSensorsScaleEnabled.get();
        const bool pressureEnabled = Config::getInstance().hardwareSensorsPressureEnabled.get();

        if (scaleEnabled && shouldDisplayBrewTimer()) {
            const bool isAutomatic  = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
            const bool brewByWeight = Config::getInstance().brewByWeightEnabled.get();

            if (isAutomatic && brewByWeight) {
                const auto targetWeight = Config::getInstance().brewByWeightTargetWeight.get();
                displayBrewWeight(1, 44, g_state.sensors.currBrewWeight, targetWeight, g_state.sensors.scaleFailure);
            } else {
                displayBrewWeight(1, 44, g_state.sensors.currBrewWeight, -1, g_state.sensors.scaleFailure);
            }
        } else if (scaleEnabled) {
            displayBrewWeight(1, 44, g_state.sensors.currReadingWeight, -1, g_state.sensors.scaleFailure);
        }

        if (pressureEnabled) {
            g_state.hardware.display->setFont(u8g2_font_profont11_tf);
            int yPos = scaleEnabled ? 54 : 44;
            g_state.hardware.display->setCursor(1, yPos);
            g_state.hardware.display->print(langstring_pressure_ur);
            g_state.hardware.display->print(g_state.sensors.inputPressure, 1);
            g_state.hardware.display->print(" bar");
        }
    }

    void displayStatusBar() {
        g_state.hardware.display->drawLine(0, CleverCoffee::Display::STATUS_BAR_Y_POSITION, CleverCoffee::Display::OLED_WIDTH / 2, CleverCoffee::Display::STATUS_BAR_Y_POSITION);
        if (!g_state.network.offlineMode) {
            displayWiFiStatus(4, 2);
            displayMQTTStatus(21, 0);
        } else {
            g_state.hardware.display->setCursor(4, 1);
            g_state.hardware.display->setFont(u8g2_font_profont11_tf);
            g_state.hardware.display->print(langstring_offlinemode);
        }

        if (Config::getInstance().hardwareSensorsScaleEnabled.get() &&
            Config::getInstance().hardwareSensorsScaleType.get() == Hardware::ScaleType::BLUETOOTH) {
            displayBluetoothStatus(54, 1);
        }
    }

  public:
    // Template-specific configuration methods
    TemperatureCoords getTemperatureCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, baseX + 50, baseX, baseY + 10, baseX + 50};
    }

    PIDCoords getPIDCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, baseX, baseY + 27};
    }

    BrewCoords getBrewCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY};
    }

    const char* getCurrentTempLabel() const noexcept {
        return langstring_current_temp_ur;
    }
    const char* getSetTempLabel() const noexcept {
        return langstring_set_temp_ur;
    }
    const char* getBrewLabel() const noexcept {
        return langstring_brew_ur;
    }
    const char* getManualFlushLabel() const noexcept {
        return langstring_manual_flush_ur;
    }
    const char* getHotWaterLabel() const noexcept {
        return langstring_hot_water_ur;
    }
    const char* getPIDSeparator() const noexcept {
        return " ";
    }
};

/**
 * @brief Modern template manager using C++23 features
 */
class ModernDisplayTemplateManager {
  public:
    static void printScreen() {
        switch (currentTemplate_) {
            case System::DisplayTemplate::STANDARD:
                standardTemplate_.printScreen();
                break;
            case System::DisplayTemplate::UPRIGHT:
                uprightTemplate_.printScreen();
                break;
            case System::DisplayTemplate::MINIMAL:
            case System::DisplayTemplate::TEMPERATURE_ONLY:
            case System::DisplayTemplate::SCALE:
            default:
                // Fallback to standard for now
                standardTemplate_.printScreen();
                break;
        }
    }

    static void setTemplate(System::DisplayTemplate templateId) {
        currentTemplate_ = templateId;
    }

  private:
    static inline System::DisplayTemplate currentTemplate_ = System::DisplayTemplate::STANDARD;
    static inline StandardTemplate        standardTemplate_;
    static inline UprightTemplate         uprightTemplate_;
};
