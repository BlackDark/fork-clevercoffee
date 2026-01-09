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
        CleverCoffee::getGlobalSystemContext()->markDisplayBufferReady(true);
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
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);

        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getTemperatureCoords(baseX, baseY);

        // Current temperature
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(coords.currentTempX, coords.currentTempY);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(derived->getCurrentTempLabel());
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(coords.currentValueX, coords.currentTempY);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(CleverCoffee::getGlobalSystemContext()->processTemperature(), 1);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(" ");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(static_cast<char>(176));
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("C");

        // Set temperature
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(coords.setTempX, coords.setTempY);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(derived->getSetTempLabel());
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(coords.setValueX, coords.setTempY);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(CleverCoffee::getGlobalSystemContext()->processSetpoint(), 1);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(" ");
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(static_cast<char>(176));
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("C");
    }

    /**
     * @brief Common PID display with template-specific formatting
     */
    void displayPIDInfo(int baseX, int baseY) {
        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getPIDCoords(baseX, baseY);

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(coords.pidX, coords.pidY);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(CleverCoffee::getGlobalSystemContext()->pidKp(), 0);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(derived->getPIDSeparator());

        if (CleverCoffee::getGlobalSystemContext()->pidKi() != 0) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(CleverCoffee::getGlobalSystemContext()->pidKp() / CleverCoffee::getGlobalSystemContext()->pidKi(), 0);
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("0");
        }

        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(derived->getPIDSeparator());
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(CleverCoffee::getGlobalSystemContext()->pidKd() / CleverCoffee::getGlobalSystemContext()->pidKp(), 0);

        // Output percentage
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(coords.outputX, coords.outputY);
        if (CleverCoffee::getGlobalSystemContext()->processPidOutput() < 99) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(CleverCoffee::getGlobalSystemContext()->processPidOutput() / 10, 1);
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(CleverCoffee::getGlobalSystemContext()->processPidOutput() / 10, 0);
        }
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("%");
    }

    /**
     * @brief Common brew time display with template-specific positioning
     */
    void displayBrewInfo(int baseX, int baseY) {
        if (!Config::getInstance().hardwareSwitchesBrewEnabled.get()) return;

        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getBrewCoords(baseX, baseY);

        // Show flush time
        if (isManualFlushState(CleverCoffee::getGlobalSystemContext()->machineStateContext()->getCurrentStateId())) {
            displayBrewTime(coords.brewX, coords.brewY, derived->getManualFlushLabel(), CleverCoffee::getGlobalSystemContext()->processCurrentBrewTime());
        }
        // Show hot water time
        else if (isHotWaterState(CleverCoffee::getGlobalSystemContext()->machineStateContext()->getCurrentStateId())) {
            displayBrewTime(coords.brewX, coords.brewY, derived->getHotWaterLabel(), CleverCoffee::getGlobalSystemContext()->currPumpOnTime());
        } else if (shouldDisplayBrewTimer()) {
            const bool isAutomatic = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
            const bool brewByTime  = Config::getInstance().brewByTimeEnabled.get();

            if (isAutomatic && brewByTime) {
                displayBrewTime(coords.brewX,
                                coords.brewY,
                                derived->getBrewLabel(),
                                CleverCoffee::getGlobalSystemContext()->processCurrentBrewTime(),
                                CleverCoffee::getGlobalSystemContext()->processTotalTargetBrewTime());
            } else {
                displayBrewTime(coords.brewX, coords.brewY, derived->getBrewLabel(), CleverCoffee::getGlobalSystemContext()->processCurrentBrewTime());
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
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();

        displayStatusbar();
        displayTemperatureInfo(34, 16);

        // Thermometer
        displayThermometerOutline(4, 62);
        if (fabs(CleverCoffee::getGlobalSystemContext()->processTemperature() - CleverCoffee::getGlobalSystemContext()->processSetpoint()) < 0.3) {
            if (CleverCoffee::getGlobalSystemContext()->isrCounter() < 500) {
                drawTemperaturebar(8, 30);
            }
        } else {
            drawTemperaturebar(8, 30);
        }

        displayBrewInfo(34, 36);
        displayPIDInfo(38, 47);

        // Heat bar
        displayProgressbar(CleverCoffee::getGlobalSystemContext()->processPidOutput() / 10, 30, 60, 98);
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
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->clearBuffer();

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
        if (getCurrentDisplayState() == MachineStateId::WATER_TANK_EMPTY) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(
                8, 50, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
            return true;
        }

        if (getCurrentDisplayState() == MachineStateId::SENSOR_ERROR) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            char tempBuffer[16];
            snprintf(tempBuffer, sizeof(tempBuffer), "%.1f", CleverCoffee::getGlobalSystemContext()->processTemperature());
            displayMessage(langstring_error_tsensor_ur[0],
                           langstring_error_tsensor_ur[1],
                           tempBuffer,
                           langstring_error_tsensor_ur[2],
                           langstring_error_tsensor_ur[3],
                           langstring_error_tsensor_ur[4]);
            return true;
        }

        if (Config::getInstance().displayPidOffLogo.get() && getCurrentDisplayState() == MachineStateId::STANDBY) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawXBMP(6, 50, Off_Logo_width, Off_Logo_height, Off_Logo);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(1, 110);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont10_tf);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("Standby mode");
            return true;
        }

        return false;
    }

  private:
    void displayHeatBar() {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawFrame(0, 124, 64, 4);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(1, 125, CleverCoffee::getGlobalSystemContext()->processPidOutput() / 16.13 + 1, 125);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(1, 126, CleverCoffee::getGlobalSystemContext()->processPidOutput() / 16.13 + 1, 126);
    }

    void displayMainStatus() {
        const bool scaleEnabled    = Config::getInstance().hardwareSensorsScaleEnabled.get();
        const bool pressureEnabled = Config::getInstance().hardwareSensorsPressureEnabled.get();

        int yPos = scaleEnabled && pressureEnabled ? 65 : (scaleEnabled || pressureEnabled ? 60 : 55);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(1, yPos);
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont22_tf);

        if (isManualFlushState(CleverCoffee::getGlobalSystemContext()->machineStateContext()->getCurrentStateId())) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("FLUSH");
        } else if (shouldDisplayBrewTimer()) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("BREW");
        } else if (fabs(CleverCoffee::getGlobalSystemContext()->processTemperature() - CleverCoffee::getGlobalSystemContext()->processSetpoint()) < 0.3) {
            if (CleverCoffee::getGlobalSystemContext()->isrCounter() < 500) {
                CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("OK");
            }
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print("WAIT");
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
                displayBrewWeight(1, 44, CleverCoffee::getGlobalSystemContext()->currBrewWeight(), targetWeight, CleverCoffee::getGlobalSystemContext()->scaleFailure());
            } else {
                displayBrewWeight(1, 44, CleverCoffee::getGlobalSystemContext()->currBrewWeight(), -1, CleverCoffee::getGlobalSystemContext()->scaleFailure());
            }
        } else if (scaleEnabled) {
            displayBrewWeight(1, 44, CleverCoffee::getGlobalSystemContext()->currReadingWeight(), -1, CleverCoffee::getGlobalSystemContext()->scaleFailure());
        }

        if (pressureEnabled) {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            int yPos = scaleEnabled ? 54 : 44;
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(1, yPos);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_pressure_ur);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(CleverCoffee::getGlobalSystemContext()->inputPressure(), 1);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(" bar");
        }
    }

    void displayStatusBar() {
        CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->drawLine(0, CleverCoffee::Display::STATUS_BAR_Y_POSITION, CleverCoffee::Display::OLED_WIDTH / 2, CleverCoffee::Display::STATUS_BAR_Y_POSITION);
        if (!CleverCoffee::getGlobalSystemContext()->networkCoordinator().isOfflineMode()) {
            displayWiFiStatus(4, 2);
            displayMQTTStatus(21, 0);
        } else {
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setCursor(4, 1);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            CleverCoffee::getGlobalSystemContext()->hardwareContext().display()->print(langstring_offlinemode);
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
