/**
 * @file ModernDisplayTemplate.h
 * @brief Modern C++23 display template system using "deducing this"
 */

#pragma once

#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/display/displayCommon.h"
#include "clevercoffee/state/MachineStateIds.h"

/**
 * @brief Modern C++20 display template base using CRTP
 *
 * This eliminates code duplication across display templates by providing
 * common functionality while allowing specialized template behavior.
 * Uses traditional CRTP for maximum compatibility with zero-cost abstractions.
 */
// Forward declaration
namespace CleverCoffee {
class SystemContext;
}

template <typename Derived>
class ModernDisplayTemplate {
  public:
    /**
     * @brief Set system context for display operations
     */
    void setSystemContext(CleverCoffee::SystemContext* context) noexcept {
        systemContext_ = context;
    }
    
    /**
     * @brief Get system context
     */
    CleverCoffee::SystemContext* getSystemContext() const noexcept {
        return systemContext_;
    }
    
    /**
     * @brief Main screen rendering method using CRTP
     */
    void printScreen() {
        if (!systemContext_) {
            return; // No system context available
        }
        
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
        systemContext_->setDisplayBufferReady(true);
    }

    /**
     * @brief Handle fullscreen display modes (common across all templates)
     */
    bool handleFullscreenModes() {
        if (!systemContext_) return false;
        
        if (displayFullscreenBrewTimer(*systemContext_)) return true;
        if (displayFullscreenManualFlushTimer(*systemContext_)) return true;
        if (displayFullscreenHotWaterTimer(*systemContext_)) return true;

        // Default offline mode handling
        if (displayOfflineMode(*systemContext_)) return true;

        return false;
    }

    /**
     * @brief Handle machine state displays (common logic, template-specific rendering)
     */
    bool handleMachineStates() {
        // Default machine state handling
        if (!systemContext_) return false;
        return displayMachineState(*systemContext_);
    }

  protected:
    CleverCoffee::SystemContext* systemContext_{nullptr};

    /**
     * @brief Common temperature display with template-specific positioning
     */
    void displayTemperatureInfo(int baseX, int baseY) {
        if (!systemContext_) return;
        
        systemContext_->hardwareContext().display()->setFont(u8g2_font_profont11_tf);

        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getTemperatureCoords(baseX, baseY);

        // Current temperature
        systemContext_->hardwareContext().display()->setCursor(coords.currentTempX, coords.currentTempY);
        systemContext_->hardwareContext().display()->print(derived->getCurrentTempLabel());
        systemContext_->hardwareContext().display()->setCursor(coords.currentValueX, coords.currentTempY);
        systemContext_->hardwareContext().display()->print(systemContext_->processTemperature(), 1);
        systemContext_->hardwareContext().display()->print(" ");
        systemContext_->hardwareContext().display()->print(static_cast<char>(176));
        systemContext_->hardwareContext().display()->print("C");

        // Set temperature
        systemContext_->hardwareContext().display()->setCursor(coords.setTempX, coords.setTempY);
        systemContext_->hardwareContext().display()->print(derived->getSetTempLabel());
        systemContext_->hardwareContext().display()->setCursor(coords.setValueX, coords.setTempY);
        systemContext_->hardwareContext().display()->print(systemContext_->processSetpoint(), 1);
        systemContext_->hardwareContext().display()->print(" ");
        systemContext_->hardwareContext().display()->print(static_cast<char>(176));
        systemContext_->hardwareContext().display()->print("C");
    }

    /**
     * @brief Common PID display with template-specific formatting
     */
    void displayPIDInfo(int baseX, int baseY) {
        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getPIDCoords(baseX, baseY);

        systemContext_->hardwareContext().display()->setCursor(coords.pidX, coords.pidY);
        systemContext_->hardwareContext().display()->print(systemContext_->pidKp(), 0);
        systemContext_->hardwareContext().display()->print(derived->getPIDSeparator());

        if (systemContext_->pidKi() != 0) {
            systemContext_->hardwareContext().display()->print(systemContext_->pidKp() / systemContext_->pidKi(), 0);
        } else {
            systemContext_->hardwareContext().display()->print("0");
        }

        systemContext_->hardwareContext().display()->print(derived->getPIDSeparator());
        systemContext_->hardwareContext().display()->print(systemContext_->pidKd() / systemContext_->pidKp(), 0);

        // Output percentage
        systemContext_->hardwareContext().display()->setCursor(coords.outputX, coords.outputY);
        if (systemContext_->processPidOutput() < 99) {
            systemContext_->hardwareContext().display()->print(systemContext_->processPidOutput() / 10, 1);
        } else {
            systemContext_->hardwareContext().display()->print(systemContext_->processPidOutput() / 10, 0);
        }
        systemContext_->hardwareContext().display()->print("%");
    }

    /**
     * @brief Common brew time display with template-specific positioning
     */
    void displayBrewInfo(int baseX, int baseY) {
        if (!Config::getInstance().hardwareSwitchesBrewEnabled.get()) return;

        auto*       derived = static_cast<Derived*>(this);
        const auto& coords  = derived->getBrewCoords(baseX, baseY);

        // Show flush time
        if (isManualFlushState(systemContext_->machineStateContext()->getCurrentStateId())) {
            displayBrewTime(*systemContext_, coords.brewX, coords.brewY, derived->getManualFlushLabel(), systemContext_->processCurrentBrewTime());
        }
        // Show hot water time (when pump is active in PID_NORMAL or STEAM_RUNNING)
        else if (shouldDisplayHotWaterTimer(*systemContext_)) {
            displayBrewTime(*systemContext_, coords.brewX, coords.brewY, derived->getHotWaterLabel(), systemContext_->currPumpOnTime());
        } else if (shouldDisplayBrewTimer(*systemContext_)) {
            const bool isAutomatic = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
            const bool brewByTime  = Config::getInstance().brewByTimeEnabled.get();

            if (isAutomatic && brewByTime) {
                displayBrewTime(*systemContext_,
                                coords.brewX,
                                coords.brewY,
                                derived->getBrewLabel(),
                                systemContext_->processCurrentBrewTime(),
                                systemContext_->processTotalTargetBrewTime());
            } else {
                displayBrewTime(*systemContext_, coords.brewX, coords.brewY, derived->getBrewLabel(), systemContext_->processCurrentBrewTime());
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
        systemContext_->hardwareContext().display()->clearBuffer();

        displayStatusbar(*systemContext_);
        displayTemperatureInfo(34, 16);

        // Thermometer
        displayThermometerOutline(*systemContext_, 4, 62);
        if (fabs(systemContext_->processTemperature() - systemContext_->processSetpoint()) < 0.3) {
            if (systemContext_->isrCounter() < 500) {
                drawTemperaturebar(*systemContext_, 8, 30);
            }
        } else {
            drawTemperaturebar(*systemContext_, 8, 30);
        }

        displayBrewInfo(34, 36);
        displayPIDInfo(38, 47);

        // Heat bar
        displayProgressbar(*systemContext_, systemContext_->processPidOutput() / 10, 30, 60, 98);
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
        systemContext_->hardwareContext().display()->clearBuffer();

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
        if (getCurrentDisplayState(*systemContext_) == MachineStateId::WATER_TANK_EMPTY) {
            systemContext_->hardwareContext().display()->drawXBMP(
                8, 50, Water_Tank_Empty_Logo_width, Water_Tank_Empty_Logo_height, Water_Tank_Empty_Logo);
            return true;
        }

        if (getCurrentDisplayState(*systemContext_) == MachineStateId::SENSOR_ERROR) {
            systemContext_->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            char tempBuffer[16];
            snprintf(tempBuffer, sizeof(tempBuffer), "%.1f", systemContext_->processTemperature());
            displayMessage(*systemContext_, langstring_error_tsensor_ur[0],
                           langstring_error_tsensor_ur[1],
                           tempBuffer,
                           langstring_error_tsensor_ur[2],
                           langstring_error_tsensor_ur[3],
                           langstring_error_tsensor_ur[4]);
            return true;
        }

        if (Config::getInstance().displayPidOffLogo.get() && getCurrentDisplayState(*systemContext_) == MachineStateId::STANDBY) {
            systemContext_->hardwareContext().display()->drawXBMP(6, 50, Off_Logo_width, Off_Logo_height, Off_Logo);
            systemContext_->hardwareContext().display()->setCursor(1, 110);
            systemContext_->hardwareContext().display()->setFont(u8g2_font_profont10_tf);
            systemContext_->hardwareContext().display()->print("Standby mode");
            return true;
        }

        return false;
    }

  private:
    void displayHeatBar() {
        systemContext_->hardwareContext().display()->drawFrame(0, 124, 64, 4);
        systemContext_->hardwareContext().display()->drawLine(1, 125, systemContext_->processPidOutput() / 16.13 + 1, 125);
        systemContext_->hardwareContext().display()->drawLine(1, 126, systemContext_->processPidOutput() / 16.13 + 1, 126);
    }

    void displayMainStatus() {
        const bool scaleEnabled    = Config::getInstance().hardwareSensorsScaleEnabled.get();
        const bool pressureEnabled = Config::getInstance().hardwareSensorsPressureEnabled.get();

        int yPos = scaleEnabled && pressureEnabled ? 65 : (scaleEnabled || pressureEnabled ? 60 : 55);
        systemContext_->hardwareContext().display()->setCursor(1, yPos);
        systemContext_->hardwareContext().display()->setFont(u8g2_font_profont22_tf);

        if (isManualFlushState(systemContext_->machineStateContext()->getCurrentStateId())) {
            systemContext_->hardwareContext().display()->print("FLUSH");
        } else if (shouldDisplayBrewTimer(*systemContext_)) {
            systemContext_->hardwareContext().display()->print("BREW");
        } else if (fabs(systemContext_->processTemperature() - systemContext_->processSetpoint()) < 0.3) {
            if (systemContext_->isrCounter() < 500) {
                systemContext_->hardwareContext().display()->print("OK");
            }
        } else {
            systemContext_->hardwareContext().display()->print("WAIT");
        }
    }

    void displaySensorInfo() {
        const bool scaleEnabled    = Config::getInstance().hardwareSensorsScaleEnabled.get();
        const bool pressureEnabled = Config::getInstance().hardwareSensorsPressureEnabled.get();

        if (scaleEnabled && shouldDisplayBrewTimer(*systemContext_)) {
            const bool isAutomatic  = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
            const bool brewByWeight = Config::getInstance().brewByWeightEnabled.get();

            if (isAutomatic && brewByWeight) {
                const auto targetWeight = Config::getInstance().brewByWeightTargetWeight.get();
                displayBrewWeight(*systemContext_, 1, 44, systemContext_->currBrewWeight(), targetWeight, systemContext_->scaleFailure());
            } else {
                displayBrewWeight(*systemContext_, 1, 44, systemContext_->currBrewWeight(), -1, systemContext_->scaleFailure());
            }
        } else if (scaleEnabled) {
            displayBrewWeight(*systemContext_, 1, 44, systemContext_->currReadingWeight(), -1, systemContext_->scaleFailure());
        }

        if (pressureEnabled) {
            systemContext_->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            int yPos = scaleEnabled ? 54 : 44;
            systemContext_->hardwareContext().display()->setCursor(1, yPos);
            systemContext_->hardwareContext().display()->print(langstring_pressure_ur);
            systemContext_->hardwareContext().display()->print(systemContext_->inputPressure(), 1);
            systemContext_->hardwareContext().display()->print(" bar");
        }
    }

    void displayStatusBar() {
        systemContext_->hardwareContext().display()->drawLine(0, CleverCoffee::Display::STATUS_BAR_Y_POSITION, CleverCoffee::Display::OLED_WIDTH / 2, CleverCoffee::Display::STATUS_BAR_Y_POSITION);
        if (!systemContext_->networkCoordinator().isOfflineMode()) {
            displayWiFiStatus(*systemContext_, 4, 2);
            displayMQTTStatus(*systemContext_, 21, 0);
        } else {
            systemContext_->hardwareContext().display()->setCursor(4, 1);
            systemContext_->hardwareContext().display()->setFont(u8g2_font_profont11_tf);
            systemContext_->hardwareContext().display()->print(langstring_offlinemode);
        }

        if (Config::getInstance().hardwareSensorsScaleEnabled.get() &&
            Config::getInstance().hardwareSensorsScaleType.get() == Hardware::ScaleType::BLUETOOTH) {
            displayBluetoothStatus(*systemContext_, 54, 1);
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

    /**
     * @brief Set system context for all templates
     */
    static void setSystemContext(CleverCoffee::SystemContext* context) noexcept {
        standardTemplate_.setSystemContext(context);
        uprightTemplate_.setSystemContext(context);
    }

  private:
    static inline System::DisplayTemplate currentTemplate_ = System::DisplayTemplate::STANDARD;
    static inline StandardTemplate        standardTemplate_;
    static inline UprightTemplate         uprightTemplate_;
};
